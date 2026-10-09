//! Transactional incremental SQLite storage. Every command persists only changed
//! objects, with WAL/FULL durability. The UI never performs disk writes.
pub mod recovery;
use folio_document::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Storage: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("Invalid stored data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("File system: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageHeader {
    #[serde(default)]
    pub ink_text: Vec<InkText>,
    pub id: Id,
    pub properties: PageProperties,
    pub position: usize,
    pub revision: u64,
    pub order: Vec<Id>,
    pub groups: Vec<InkGroup>,
    pub text: String,
}
#[derive(Clone, Debug)]
pub struct ObjectWrite {
    pub page: Id,
    pub id: Id,
    pub value: Option<Arc<Object>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum JournalEvent {
    Replace(History),
    Execute(Command),
    Undo,
    Redo,
}
#[derive(Clone, Debug)]
pub struct Delta {
    pub metadata: NoteMetadata,
    pub pages: Vec<PageHeader>,
    pub objects: Vec<ObjectWrite>,
    pub authoritative: bool,
    pub page_ids: Vec<Id>,
    pub metadata_changed: bool,
    pub journal: Vec<JournalEvent>,
}
impl Delta {
    pub fn full(doc: &Document) -> Self {
        let mut delta = Self::from_document(
            doc,
            doc.pages
                .iter()
                .flat_map(|p| {
                    p.objects.values().map(|o| ObjectWrite {
                        page: p.id,
                        id: o.id(),
                        value: Some(o.clone()),
                    })
                })
                .collect(),
        );
        delta.authoritative = true;
        delta
    }
    pub fn from_document(doc: &Document, objects: Vec<ObjectWrite>) -> Self {
        Self {
            authoritative: false,
            page_ids: doc.pages.iter().map(|p| p.id).collect(),
            metadata_changed: true,
            journal: vec![],
            metadata: doc.metadata.clone(),
            pages: doc
                .pages
                .iter()
                .enumerate()
                .map(|(position, p)| PageHeader {
                    ink_text: p.ink_text.clone(),
                    id: p.id,
                    properties: p.properties.clone(),
                    position,
                    revision: p.revision,
                    order: p.order.clone(),
                    groups: p.groups.clone(),
                    text: p.text(),
                })
                .collect(),
            objects,
        }
    }
    pub fn command(doc: &Document, cmd: &Command) -> Self {
        let mut objects = Vec::new();
        for c in &cmd.changes {
            match c {
                Change::Object { page, id, .. } => objects.push(ObjectWrite {
                    page: *page,
                    id: *id,
                    value: doc.page(*page).and_then(|p| p.objects.get(id).cloned()),
                }),
                Change::Page { before, after, .. } => {
                    for page in before.iter().chain(after.iter()) {
                        if let Some(p) = doc.page(page.id) {
                            for o in p.objects.values() {
                                objects.push(ObjectWrite {
                                    page: p.id,
                                    id: o.id(),
                                    value: Some(o.clone()),
                                })
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // A replacement includes before and after snapshots of the same page,
        // and commands can touch one object more than once. Persist its final
        // document value once rather than serializing and writing it repeatedly.
        if objects.len() > 1 {
            let mut written = std::collections::HashSet::with_capacity(objects.len());
            objects.retain(|write| written.insert((write.page, write.id)));
        }
        let touched = cmd
            .changes
            .iter()
            .filter_map(|c| match c {
                Change::Object { page, .. }
                | Change::Properties { page, .. }
                | Change::Groups { page, .. }
                | Change::InkText { page, .. } => Some(*page),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        let metadata_changed = cmd
            .changes
            .iter()
            .any(|c| matches!(c, Change::Metadata { .. }));
        let all_pages = cmd.changes.iter().any(|c| {
            matches!(c, Change::Page { .. })
                || matches!(c,Change::Metadata {before,after} if before.trashed && !after.trashed)
        });
        // Build headers and searchable text only for pages actually changed.
        let mut delta = Self {
            metadata: doc.metadata.clone(),
            pages: vec![],
            objects,
            authoritative: false,
            page_ids: doc.pages.iter().map(|p| p.id).collect(),
            metadata_changed,
            journal: vec![],
        };
        for (position, p) in doc
            .pages
            .iter()
            .enumerate()
            .filter(|(_, p)| all_pages || touched.contains(&p.id))
        {
            delta.pages.push(PageHeader {
                ink_text: p.ink_text.clone(),
                id: p.id,
                properties: p.properties.clone(),
                position,
                revision: p.revision,
                order: p.order.clone(),
                groups: p.groups.clone(),
                text: p.text(),
            });
        }
        delta
    }
}
fn execute_cached(
    connection: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> rusqlite::Result<usize> {
    connection.prepare_cached(sql)?.execute(params)
}

pub struct Store {
    pub connection: Connection,
    path: PathBuf,
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(&path)?;
        conn.busy_timeout(std::time::Duration::from_secs(3))?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 6 {
            return Err(Error::Invalid(format!(
                "Database version {version} is newer than this application"
            )));
        }
        conn.execute_batch("BEGIN IMMEDIATE;
   CREATE TABLE IF NOT EXISTS notes(id TEXT PRIMARY KEY,metadata TEXT NOT NULL);
   CREATE TABLE IF NOT EXISTS pages(id TEXT PRIMARY KEY,note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,position INTEGER NOT NULL,header TEXT NOT NULL);
   CREATE TABLE IF NOT EXISTS objects(id TEXT PRIMARY KEY,page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,data TEXT NOT NULL);
   CREATE INDEX IF NOT EXISTS objects_page ON objects(page_id);
   CREATE INDEX IF NOT EXISTS pages_note_position ON pages(note_id,position);
   CREATE TABLE IF NOT EXISTS drafts(id TEXT PRIMARY KEY,note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,page_id TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,data TEXT NOT NULL);
   CREATE INDEX IF NOT EXISTS drafts_note ON drafts(note_id);
   CREATE TABLE IF NOT EXISTS history(note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, command TEXT NOT NULL, applied INTEGER NOT NULL, PRIMARY KEY(note_id,ordinal));
   CREATE TABLE IF NOT EXISTS notebooks(id TEXT PRIMARY KEY,data TEXT NOT NULL);
   CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,data TEXT NOT NULL);
   CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(note_id UNINDEXED,page_id UNINDEXED,title,tags,body,tokenize='unicode61 remove_diacritics 2');
   COMMIT;")?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(Error::Invalid(format!(
                "Database integrity check failed: {check}. Recovery snapshots are next to the database."
            )));
        }
        if version < 3 {
            // Recognition is retired. Rebuild only the derived search index;
            // keep original objects, legacy group metadata and undo untouched.
            // The version and index change commit together, including on failure.
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM search", [])?;
            {
                let mut pages = tx.prepare(
                    "SELECT p.id,p.note_id,p.header,n.metadata FROM pages p JOIN notes n ON n.id=p.note_id",
                )?;
                let rows = pages.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })?;
                let mut objects = tx.prepare("SELECT data FROM objects WHERE page_id=?1")?;
                for row in rows {
                    let (page, note, header, metadata) = row?;
                    let metadata: NoteMetadata = serde_json::from_str(&metadata)?;
                    if metadata.trashed {
                        continue;
                    }
                    let header: PageHeader = serde_json::from_str(&header)?;
                    let mut text = std::collections::HashMap::new();
                    for data in objects.query_map([&page], |r| r.get::<_, String>(0))? {
                        let object: Object = serde_json::from_str(&data?)?;
                        if !object.searchable_text().is_empty() {
                            text.insert(object.id(), object.searchable_text().to_owned());
                        }
                    }
                    let body = header
                        .order
                        .iter()
                        .filter_map(|id| text.get(id).map(String::as_str))
                        .collect::<Vec<_>>()
                        .join("\n");
                    tx.execute(
                        "INSERT INTO search(note_id,page_id,title,tags,body) VALUES(?1,?2,?3,?4,?5)",
                        params![note, page, metadata.title, metadata.tags.join(" "), body],
                    )?;
                }
            }
            tx.pragma_update(None, "user_version", 3)?;
            tx.commit()?;
        }
        if version < 6 {
            // FTS UNINDEXED columns cannot locate a page without scanning the
            // whole library. Preserve existing FTS rowids in a durable integer
            // primary-key table (ordinary pages.rowid can change on VACUUM).
            let tx = conn.transaction()?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS search_entries(
                id INTEGER PRIMARY KEY,
                page_id TEXT NOT NULL UNIQUE REFERENCES pages(id) ON DELETE CASCADE
            );
            INSERT OR IGNORE INTO search_entries(id,page_id)
                SELECT MIN(s.rowid),p.id FROM search s JOIN pages p ON p.id=s.page_id GROUP BY p.id;
            DELETE FROM search WHERE rowid NOT IN (SELECT id FROM search_entries);",
            )?;
            tx.pragma_update(None, "user_version", 6)?;
            tx.commit()?;
        }
        Ok(Self {
            connection: conn,
            path,
        })
    }
    /// Background reads reuse the schema initialized by the owning controller.
    /// Opening a tab or searching must not request a write lock or scan the
    /// entire database for integrity on each keystroke.
    pub fn open_reader(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let connection = Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(std::time::Duration::from_secs(3))?;
        let version: i32 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 6 {
            return Err(Error::Invalid(format!(
                "Database version {version} requires initialization by the application"
            )));
        }
        Ok(Self { connection, path })
    }

    /// Document and undo history must come from the same committed WAL snapshot.
    pub fn load_with_history(&self, id: Id) -> Result<Option<(Document, History)>> {
        let transaction = self.connection.unchecked_transaction()?;
        let result = self
            .load_snapshot(id)?
            .map(|document| self.history(id).map(|history| (document, history)))
            .transpose()?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn list_notes(&self) -> Result<Vec<NoteMetadata>> {
        let mut q = self.connection.prepare("SELECT metadata FROM notes")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        let mut notes: Vec<NoteMetadata> = Vec::new();
        for row in rows {
            notes.push(serde_json::from_str(&row?)?)
        }
        notes.sort_by_key(|n| std::cmp::Reverse(n.updated_at));
        Ok(notes)
    }
    /// Read just the chosen cover page and page count in one WAL snapshot.
    pub fn library_preview(&self, note: Id, cover: Option<Id>) -> Result<Option<(Page, usize)>> {
        let tx = self.connection.unchecked_transaction()?;
        let header: Option<String> = tx.query_row(
            "SELECT header FROM pages WHERE note_id=?1 ORDER BY CASE WHEN id=?2 THEN 0 ELSE 1 END,position LIMIT 1",
            params![note.to_string(), cover.map(|id| id.to_string())], |r| r.get(0)).optional()?;
        let Some(header) = header else {
            return Ok(None);
        };
        let header: PageHeader = serde_json::from_str(&header)?;
        let count = tx.query_row(
            "SELECT count(*) FROM pages WHERE note_id=?1",
            [note.to_string()],
            |r| r.get::<_, i64>(0),
        )?;
        let objects = {
            let mut query = tx.prepare("SELECT data FROM objects WHERE page_id=?1")?;
            let mut objects = std::collections::BTreeMap::new();
            for row in query.query_map([header.id.to_string()], |r| r.get::<_, String>(0))? {
                let object: Object = serde_json::from_str(&row?)?;
                objects.insert(object.id(), Arc::new(object));
            }
            objects
        };
        let page = Page {
            ink_text: header.ink_text,
            id: header.id,
            properties: header.properties,
            objects,
            order: header.order,
            groups: header.groups,
            revision: header.revision,
        };
        tx.commit()?;
        let mut check = Document::new("preview");
        check.pages = vec![page];
        check.validate().map_err(Error::Invalid)?;
        Ok(Some((check.pages.pop().unwrap(), count as usize)))
    }
    pub fn load(&self, id: Id) -> Result<Option<Document>> {
        let transaction = self.connection.unchecked_transaction()?;
        let document = self.load_snapshot(id)?;
        transaction.commit()?;
        Ok(document)
    }
    fn load_snapshot(&self, id: Id) -> Result<Option<Document>> {
        let metadata: Option<String> = self
            .connection
            .query_row(
                "SELECT metadata FROM notes WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(metadata) = metadata else {
            return Ok(None);
        };
        let mut pages = Vec::new();
        let mut q = self
            .connection
            .prepare("SELECT header FROM pages WHERE note_id=?1 ORDER BY position")?;
        let mut object_query = self
            .connection
            .prepare_cached("SELECT data FROM objects WHERE page_id=?1")?;
        for row in q.query_map([id.to_string()], |r| r.get::<_, String>(0))? {
            let h: PageHeader = serde_json::from_str(&row?)?;
            let mut objects = std::collections::BTreeMap::new();
            for row in object_query.query_map([h.id.to_string()], |r| r.get::<_, String>(0))? {
                let o: Object = serde_json::from_str(&row?)?;
                objects.insert(o.id(), Arc::new(o));
            }
            pages.push(Page {
                ink_text: h.ink_text,
                id: h.id,
                properties: h.properties,
                objects,
                order: h.order,
                groups: h.groups,
                revision: h.revision,
            });
        }
        let page_indices: std::collections::HashMap<_, _> = pages
            .iter()
            .enumerate()
            .map(|(index, page)| (page.id, index))
            .collect();
        let mut drafts = self
            .connection
            .prepare("SELECT page_id,data FROM drafts WHERE note_id=?1")?;
        for row in drafts.query_map([id.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (page_id, data) = row?;
            let o: Object = serde_json::from_str(&data)?;
            let page_id = Id::parse_str(&page_id)
                .map_err(|_| Error::Invalid("Invalid draft page ID".into()))?;
            if let Some(p) = page_indices.get(&page_id).map(|index| &mut pages[*index])
                && !p.objects.contains_key(&o.id())
            {
                p.order.push(o.id());
                p.objects.insert(o.id(), Arc::new(o));
                p.groups.clear();
                p.revision += 1;
            }
        }
        let d = Document {
            version: FORMAT_VERSION,
            metadata: serde_json::from_str(&metadata)?,
            pages,
        };
        d.validate().map_err(Error::Invalid)?;
        Ok(Some(d))
    }
    pub fn save(&mut self, delta: &Delta) -> Result<()> {
        let tx = self.connection.transaction()?;
        // Prevent older apps from silently dropping durable math dependencies.
        if delta.objects.iter().any(|write| {
            write
                .value
                .as_ref()
                .is_some_and(|o| matches!(o.as_ref(), Object::Equation(e) if e.math_link.is_some()))
        }) {
            let version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if version < 4 {
                tx.pragma_update(None, "user_version", 4)?;
            }
        }
        if delta.pages.iter().any(|page| !page.ink_text.is_empty()) || delta.journal.iter().any(|event| matches!(event, JournalEvent::Execute(command) if command.changes.iter().any(|c| matches!(c, Change::InkText { .. })))) {
            let version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if version < 5 {
                tx.pragma_update(None, "user_version", 5)?;
            }
        }
        let note = delta.metadata.id.to_string();
        execute_cached(
            &tx,
            "INSERT INTO notes VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET metadata=excluded.metadata",
            params![note, serde_json::to_string(&delta.metadata)?],
        )?;
        let page_ids = delta
            .page_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        // Header order is authoritative, and page deletion cascades its object rows.
        let current = {
            let mut q = tx.prepare("SELECT id FROM pages WHERE note_id=?1")?;
            q.query_map([&note], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for old in current {
            if !Id::parse_str(&old).is_ok_and(|id| page_ids.contains(&id)) {
                execute_cached(
                    &tx,
                    "DELETE FROM search WHERE rowid=(SELECT id FROM search_entries WHERE page_id=?1)",
                    [&old],
                )?;
                execute_cached(&tx, "DELETE FROM pages WHERE id=?1", [old])?;
            }
        }
        for p in &delta.pages {
            execute_cached(
                &tx,
                "INSERT INTO pages VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET position=excluded.position,header=excluded.header",
                params![
                    p.id.to_string(),
                    note,
                    p.position as i64,
                    serde_json::to_string(p)?
                ],
            )?;
        }
        for p in &delta.pages {
            execute_cached(
                &tx,
                "INSERT OR IGNORE INTO search_entries(page_id) VALUES(?1)",
                [p.id.to_string()],
            )?;
        }
        for o in &delta.objects {
            if !page_ids.contains(&o.page) {
                continue;
            }
            execute_cached(&tx, "DELETE FROM drafts WHERE id=?1", [o.id.to_string()])?;
            if let Some(value) = &o.value {
                execute_cached(
                    &tx,
                    "INSERT INTO objects VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET page_id=excluded.page_id,data=excluded.data",
                    params![
                        o.id.to_string(),
                        o.page.to_string(),
                        serde_json::to_string(value)?
                    ],
                )?;
            } else {
                execute_cached(&tx, "DELETE FROM objects WHERE id=?1", [o.id.to_string()])?;
            }
        }
        if delta.authoritative {
            for page in &delta.pages {
                execute_cached(
                    &tx,
                    "DELETE FROM objects WHERE page_id=?1 AND id NOT IN (SELECT value FROM json_each(?2))",
                    params![page.id.to_string(), serde_json::to_string(&page.order)?],
                )?;
            }
        }
        let tags = delta.metadata.tags.join(" ");
        if delta.metadata.trashed {
            execute_cached(
                &tx,
                "DELETE FROM search WHERE rowid IN (SELECT e.id FROM search_entries e JOIN pages p ON p.id=e.page_id WHERE p.note_id=?1)",
                [&note],
            )?;
        } else {
            if delta.metadata_changed {
                execute_cached(
                    &tx,
                    "UPDATE search SET title=?2,tags=?3 WHERE rowid IN (SELECT e.id FROM search_entries e JOIN pages p ON p.id=e.page_id WHERE p.note_id=?1) AND (title<>?2 OR tags<>?3)",
                    params![note, delta.metadata.title, tags],
                )?;
            }
            for p in &delta.pages {
                // Ink geometry changes usually leave searchable text unchanged.
                // Avoid replacing an identical FTS row and rewriting its tokens.
                execute_cached(
                    &tx,
                    "INSERT OR REPLACE INTO search(rowid,note_id,page_id,title,tags,body) SELECT e.id,?1,?2,?3,?4,?5 FROM search_entries e WHERE e.page_id=?2 AND NOT EXISTS (SELECT 1 FROM search WHERE rowid=e.id AND title=?3 AND tags=?4 AND body=?5)",
                    params![note, p.id.to_string(), delta.metadata.title, tags, p.text],
                )?;
            }
        }
        for event in &delta.journal {
            match event {
                JournalEvent::Replace(history) => {
                    execute_cached(&tx, "DELETE FROM history WHERE note_id=?1", [&note])?;
                    for (index, (command, applied)) in history.entries().enumerate() {
                        execute_cached(
                            &tx,
                            "INSERT INTO history VALUES(?1,?2,?3,?4)",
                            params![
                                &note,
                                index as i64,
                                serde_json::to_string(command)?,
                                applied
                            ],
                        )?;
                    }
                }
                JournalEvent::Execute(command) => {
                    execute_cached(
                        &tx,
                        "DELETE FROM history WHERE note_id=?1 AND applied=0",
                        [&note],
                    )?;
                    let ordinal: i64 = tx.query_row(
                        "SELECT COALESCE(MAX(ordinal),0)+1 FROM history WHERE note_id=?1",
                        [&note],
                        |r| r.get(0),
                    )?;
                    execute_cached(
                        &tx,
                        "INSERT INTO history VALUES(?1,?2,?3,1)",
                        params![note, ordinal, serde_json::to_string(command)?],
                    )?;
                    execute_cached(
                        &tx,
                        "DELETE FROM history WHERE note_id=?1 AND ordinal<=?2",
                        params![note, ordinal - 512],
                    )?;
                }
                JournalEvent::Undo => {
                    execute_cached(
                        &tx,
                        "UPDATE history SET applied=0 WHERE note_id=?1 AND ordinal=(SELECT MAX(ordinal) FROM history WHERE note_id=?1 AND applied=1)",
                        [&note],
                    )?;
                }
                JournalEvent::Redo => {
                    execute_cached(
                        &tx,
                        "UPDATE history SET applied=1 WHERE note_id=?1 AND ordinal=(SELECT MIN(ordinal) FROM history WHERE note_id=?1 AND applied=0)",
                        [&note],
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn history(&self, note: Id) -> Result<History> {
        let mut undo = vec![];
        let mut redo = vec![];
        let mut query = self
            .connection
            .prepare("SELECT command,applied FROM history WHERE note_id=?1 ORDER BY ordinal")?;
        for row in query.query_map([note.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        })? {
            let (data, applied) = row?;
            let command = serde_json::from_str(&data)?;
            if applied {
                undo.push(command);
            } else {
                redo.push(command);
            }
        }
        redo.reverse();
        Ok(History::from_commands(undo, redo))
    }
    pub fn draft(&self, note: Id, page: Id, id: Id, value: Option<&Arc<Object>>) -> Result<()> {
        if let Some(value) = value {
            self.connection.execute("INSERT INTO drafts VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![id.to_string(),note.to_string(),page.to_string(),serde_json::to_string(value)?])?;
        } else {
            self.connection
                .execute("DELETE FROM drafts WHERE id=?1", [id.to_string()])?;
        }
        Ok(())
    }
    pub fn notebooks(&self) -> Result<Vec<Notebook>> {
        let mut q = self
            .connection
            .prepare("SELECT data FROM notebooks ORDER BY id")?;
        let mut out = vec![];
        for row in q.query_map([], |r| r.get::<_, String>(0))? {
            out.push(serde_json::from_str(&row?)?)
        }
        Ok(out)
    }
    pub fn save_notebook(&self, n: &Notebook) -> Result<()> {
        self.connection.execute(
            "INSERT INTO notebooks VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![n.id.to_string(), serde_json::to_string(n)?],
        )?;
        Ok(())
    }
    pub fn setting<T: for<'a> Deserialize<'a>>(&self, key: &str) -> Result<Option<T>> {
        let s: Option<String> = self
            .connection
            .query_row("SELECT data FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(s.map(|s| serde_json::from_str(&s)).transpose()?)
    }
    pub fn save_setting(&self, key: &str, data: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET data=excluded.data",
            params![key, data],
        )?;
        Ok(())
    }
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        if path.exists() {
            return Err(Error::Invalid(
                "Backup snapshot destination already exists".into(),
            ));
        }
        let mut target = Connection::open(path)?;
        rusqlite::backup::Backup::new(&self.connection, &mut target)?.run_to_completion(
            256,
            std::time::Duration::from_millis(5),
            None,
        )?;
        Ok(())
    }
    pub fn checkpoint(&self) -> Result<PathBuf> {
        let backup = self
            .path
            .with_extension(format!("recovery-{}.sqlite3", now_ms()));
        self.connection
            .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE)")?;
        Ok(backup)
    }
}
#[derive(Debug)]
pub struct Receipt {
    pub sequence: u64,
    pub result: std::result::Result<(), String>,
}
enum Work {
    Draft {
        note: Id,
        page: Id,
        id: Id,
        value: Option<Arc<Object>>,
    },
    Save(u64, Delta),
    Notebook(Notebook),
    DeleteNotebook(Id),
    Settings(String),
    Checkpoint,
    Flush(mpsc::Sender<std::result::Result<(), String>>),
    Stop,
}
pub struct Persistence {
    sender: mpsc::SyncSender<Work>,
    pub receipts: mpsc::Receiver<Receipt>,
    join: Option<JoinHandle<()>>,
    sequence: u64,
}
impl Persistence {
    pub fn new(mut store: Store) -> Self {
        let (sender, receiver) = mpsc::sync_channel(64);
        let (tx, receipts) = mpsc::channel();
        let join = thread::Builder::new()
            .name("folio-storage".into())
            .spawn(move || {
                let mut error: Option<String> = None;
                let mut failed = std::collections::HashSet::new();
                let mut pending_notebooks = std::collections::HashMap::new();
                let mut pending_deletes = std::collections::HashSet::new();
                let mut pending_settings = None;
                let mut last_checkpoint = std::time::Instant::now();
                while let Ok(work) = receiver.recv() {
                    match work {
                        Work::Draft {
                            note,
                            page,
                            id,
                            value,
                        } => {
                            if let Err(e) = store.draft(note, page, id, value.as_ref()) {
                                let _ = tx.send(Receipt {
                                    sequence: 0,
                                    result: Err(e.to_string()),
                                });
                            }
                        }
                        Work::Save(sequence, delta) => {
                            let note = delta.metadata.id;
                            let result = if failed.contains(&note) && !delta.authoritative {
                                Err("Earlier save failed; waiting for a complete recovery save"
                                    .to_string())
                            } else {
                                store.save(&delta).map_err(|e| e.to_string())
                            };
                            match &result {
                                Ok(()) => {
                                    failed.remove(&note);
                                    if failed.is_empty()
                                        && pending_notebooks.is_empty()
                                        && pending_deletes.is_empty()
                                        && pending_settings.is_none()
                                    {
                                        error = None;
                                    }
                                }
                                Err(message) => {
                                    failed.insert(note);
                                    error = Some(message.clone());
                                }
                            }
                            let _ = tx.send(Receipt { sequence, result });
                        }
                        Work::DeleteNotebook(id) => {
                            pending_deletes.insert(id);
                            pending_notebooks.remove(&id);
                            if let Err(e) = store
                                .connection
                                .execute("DELETE FROM notebooks WHERE id=?1", [id.to_string()])
                            {
                                error = Some(e.to_string());
                                let _ = tx.send(Receipt {
                                    sequence: 0,
                                    result: Err(e.to_string()),
                                });
                            }
                        }
                        Work::Notebook(n) => {
                            pending_notebooks.insert(n.id, n.clone());
                            pending_deletes.remove(&n.id);
                            if let Err(e) = store.save_notebook(&n) {
                                error = Some(e.to_string());
                                let _ = tx.send(Receipt {
                                    sequence: 0,
                                    result: Err(e.to_string()),
                                });
                            }
                        }
                        Work::Settings(data) => {
                            pending_settings = Some(data.clone());
                            if let Err(e) = store.save_setting("preferences", &data) {
                                error = Some(e.to_string());
                                let _ = tx.send(Receipt {
                                    sequence: 0,
                                    result: Err(e.to_string()),
                                });
                            }
                        }
                        Work::Checkpoint => {
                            if let Err(e) = store.checkpoint() {
                                let _ = tx.send(Receipt {
                                    sequence: 0,
                                    result: Err(e.to_string()),
                                });
                            }
                        }
                        Work::Flush(reply) => {
                            pending_notebooks
                                .retain(|_, notebook| store.save_notebook(notebook).is_err());
                            pending_deletes.retain(|id| {
                                store
                                    .connection
                                    .execute("DELETE FROM notebooks WHERE id=?1", [id.to_string()])
                                    .is_err()
                            });
                            if pending_settings
                                .as_ref()
                                .is_some_and(|data| store.save_setting("preferences", data).is_ok())
                            {
                                pending_settings = None;
                            }
                            if failed.is_empty()
                                && pending_notebooks.is_empty()
                                && pending_deletes.is_empty()
                                && pending_settings.is_none()
                            {
                                error = None;
                            } else if error.is_none() {
                                error = Some(
                                    "Settings or folders are still awaiting a successful save"
                                        .into(),
                                );
                            }

                            let result = if failed.is_empty() {
                                error.clone().map_or(Ok(()), Err)
                            } else {
                                Err(error.clone().unwrap_or_else(|| {
                                    "Some notes still need a successful recovery save".into()
                                }))
                            };
                            let _ = reply.send(result);
                        }
                        Work::Stop => break,
                    }
                    if last_checkpoint.elapsed().as_secs() > 300 && failed.is_empty() {
                        if let Ok(path) = store.checkpoint() {
                            prune_checkpoints(&path);
                        }
                        last_checkpoint = std::time::Instant::now();
                    }
                }
            })
            .expect("start storage worker");
        Self {
            sender,
            receipts,
            join: Some(join),
            sequence: 0,
        }
    }
    pub fn save(&mut self, delta: Delta) -> std::result::Result<u64, String> {
        self.sequence += 1;
        self.sender
            .try_send(Work::Save(self.sequence, delta))
            .map_err(|e| match e {
                mpsc::TrySendError::Full(_) => {
                    "Storage queue is full; changes are retained in memory for retry".to_string()
                }
                _ => "Storage worker stopped".to_string(),
            })?;
        Ok(self.sequence)
    }
    pub fn save_blocking(&mut self, delta: Delta) -> std::result::Result<u64, String> {
        self.sequence += 1;
        self.sender
            .send(Work::Save(self.sequence, delta))
            .map_err(|_| "Storage worker stopped".to_string())?;
        Ok(self.sequence)
    }
    pub fn draft(&self, note: Id, page: Id, id: Id, value: Option<Arc<Object>>) {
        let _ = self.sender.try_send(Work::Draft {
            note,
            page,
            id,
            value,
        });
    }
    pub fn delete_notebook(&self, id: Id) {
        let _ = self.sender.send(Work::DeleteNotebook(id));
    }
    pub fn notebook(&self, n: Notebook) {
        let _ = self.sender.send(Work::Notebook(n));
    }
    pub fn settings(&self, data: String) {
        let _ = self.sender.send(Work::Settings(data));
    }
    pub fn checkpoint(&self) {
        let _ = self.sender.send(Work::Checkpoint);
    }
    pub fn flush(&self) -> std::result::Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(Work::Flush(tx))
            .map_err(|_| "Storage worker stopped")?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| "Timed out waiting for storage")?
    }
}
impl Drop for Persistence {
    fn drop(&mut self) {
        let _ = self.sender.send(Work::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
fn prune_checkpoints(newest: &Path) {
    let Some(parent) = newest.parent() else {
        return;
    };
    let Some(stem) = newest
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split(".recovery-").next())
    else {
        return;
    };
    if let Ok(entries) = std::fs::read_dir(parent) {
        let mut snapshots: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
                    s.starts_with(&format!("{stem}.recovery-")) && s.ends_with(".sqlite3")
                })
            })
            .collect();
        snapshots.sort();
        let remove = snapshots.len().saturating_sub(3);
        for p in snapshots.into_iter().take(remove) {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> PathBuf {
        std::env::temp_dir().join(format!("folio-storage-test-{}.db", Id::new_v4()))
    }
    #[test]
    fn replacement_page_delta_serializes_each_final_object_once() {
        let mut document = Document::new("Replace");
        let object = Arc::new(Object::Text(TextBlock {
            id: Id::new_v4(),
            text: "Final content".into(),
            rect: Rect::new(0., 0., 200., 100.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 16.,
            color: Color::INK,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        }));
        document.pages[0].order.push(object.id());
        document.pages[0]
            .objects
            .insert(object.id(), object.clone());
        let command = Command {
            label: "Replace page".into(),
            changes: vec![Change::Page {
                index: 0,
                before: Some(document.pages[0].clone()),
                after: Some(document.pages[0].clone()),
            }],
        };
        let delta = Delta::command(&document, &command);
        assert_eq!(delta.objects.len(), 1);
        assert!(Arc::ptr_eq(
            delta.objects[0].value.as_ref().unwrap(),
            &object
        ));
    }

    #[test]
    fn legacy_search_rowids_migrate_without_reindexing_and_survive_vacuum() {
        for version in 3..=5 {
            let path = path();
            let mut store = Store::open(&path).unwrap();
            let document = Document::new("Legacy searchable note");
            store.save(&Delta::full(&document)).unwrap();
            store
                .connection
                .execute_batch("DROP TABLE search_entries; UPDATE search SET rowid=1000; VACUUM;")
                .unwrap();
            store
                .connection
                .pragma_update(None, "user_version", version)
                .unwrap();
            assert!(Store::open_reader(&path).is_err());
            drop(store);
            let mut store = Store::open(&path).unwrap();
            let locator: i64 = store
                .connection
                .query_row(
                    "SELECT id FROM search_entries WHERE page_id=?1",
                    [document.pages[0].id.to_string()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(locator, 1000);
            assert_eq!(store.load(document.metadata.id).unwrap().unwrap(), document);
            store.connection.execute_batch("VACUUM").unwrap();
            store.save(&Delta::full(&document)).unwrap();
            assert_eq!(
                store
                    .connection
                    .query_row("SELECT rowid FROM search", [], |row| row.get::<_, i64>(0))
                    .unwrap(),
                locator
            );
            assert_eq!(
                store
                    .connection
                    .query_row(
                        "SELECT count(*) FROM search WHERE search MATCH 'Legacy'",
                        [],
                        |row| row.get::<_, i64>(0)
                    )
                    .unwrap(),
                1
            );
            let reader = Store::open_reader(&path).unwrap();
            assert_eq!(
                reader.load(document.metadata.id).unwrap().unwrap(),
                document
            );
            drop(reader);
            drop(store);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn search_locators_preserve_other_notes_across_rename_trash_restore_and_delete() {
        let path = path();
        let mut store = Store::open(&path).unwrap();
        let mut document = Document::new("First");
        document.pages.push(Page::new());
        let other = Document::new("Unrelated");
        store.save(&Delta::full(&document)).unwrap();
        store.save(&Delta::full(&other)).unwrap();
        let count = |store: &Store, note: Id| {
            store
                .connection
                .query_row(
                    "SELECT count(*) FROM search WHERE note_id=?1",
                    [note.to_string()],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
        };
        let before = document.metadata.clone();
        document.metadata.title = "Renamed".into();
        document.metadata.tags = vec!["tagged".into()];
        let rename = Command {
            label: "Rename".into(),
            changes: vec![Change::Metadata {
                before,
                after: document.metadata.clone(),
            }],
        };
        store.save(&Delta::command(&document, &rename)).unwrap();
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT count(*) FROM search WHERE search MATCH 'Renamed AND tagged'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        document.metadata.trashed = true;
        store.save(&Delta::full(&document)).unwrap();
        assert_eq!(count(&store, document.metadata.id), 0);
        assert_eq!(count(&store, other.metadata.id), 1);
        let before = document.metadata.clone();
        document.metadata.trashed = false;
        let restore = Command {
            label: "Restore".into(),
            changes: vec![Change::Metadata {
                before,
                after: document.metadata.clone(),
            }],
        };
        store.save(&Delta::command(&document, &restore)).unwrap();
        assert_eq!(count(&store, document.metadata.id), 2);
        let deleted = document.pages.pop().unwrap();
        let delete = Command {
            label: "Delete page".into(),
            changes: vec![Change::Page {
                index: 1,
                before: Some(deleted.clone()),
                after: None,
            }],
        };
        store.save(&Delta::command(&document, &delete)).unwrap();
        assert_eq!(count(&store, document.metadata.id), 1);
        assert_eq!(count(&store, other.metadata.id), 1);
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT count(*) FROM search_entries WHERE page_id=?1",
                    [deleted.id.to_string()],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        let plan: String = store.connection.query_row("EXPLAIN QUERY PLAN DELETE FROM search WHERE rowid=(SELECT id FROM search_entries WHERE page_id=?1)", [document.pages[0].id.to_string()], |row| row.get(3)).unwrap();
        assert!(
            plan.contains('='),
            "FTS rowid lookup must be constrained: {plan}"
        );
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_search_migration_rolls_back_and_annotation_saves_keep_schema_version() {
        let path = path();
        let mut store = Store::open(&path).unwrap();
        let document = Document::new("Original");
        store.save(&Delta::full(&document)).unwrap();
        store.connection.execute_batch("DELETE FROM search_entries; CREATE TRIGGER reject_locator BEFORE INSERT ON search_entries BEGIN SELECT RAISE(ABORT, 'migration fault'); END; PRAGMA user_version=5;").unwrap();
        drop(store);
        assert!(Store::open(&path).is_err());
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            5
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM search", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        connection
            .execute_batch("DROP TRIGGER reject_locator")
            .unwrap();
        drop(connection);
        let mut store = Store::open(&path).unwrap();
        let mut delta = Delta::full(&document);
        delta.journal.push(JournalEvent::Execute(Command {
            label: "Annotation".into(),
            changes: vec![Change::InkText {
                page: document.pages[0].id,
                before: vec![],
                after: vec![],
            }],
        }));
        store.save(&delta).unwrap();
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            6
        );
        assert_eq!(store.load(document.metadata.id).unwrap().unwrap(), document);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    /// Run with `cargo test -p folio-storage incremental_save_scaling -- --ignored --nocapture`.
    #[test]
    #[ignore = "manual performance measurement"]
    fn incremental_save_scaling() {
        for unrelated in [0, 10_000] {
            let path = path();
            let mut store = Store::open(&path).unwrap();
            let document = Document::new("Active note");
            let delta = Delta::full(&document);
            store.save(&delta).unwrap();
            let tx = store.connection.transaction().unwrap();
            for _ in 0..unrelated {
                let note = Document::new("Unrelated note");
                let header = &Delta::full(&note).pages[0];
                tx.execute(
                    "INSERT INTO notes VALUES(?1,?2)",
                    params![
                        note.metadata.id.to_string(),
                        serde_json::to_string(&note.metadata).unwrap()
                    ],
                )
                .unwrap();
                tx.execute(
                    "INSERT INTO pages VALUES(?1,?2,0,?3)",
                    params![
                        header.id.to_string(),
                        note.metadata.id.to_string(),
                        serde_json::to_string(header).unwrap()
                    ],
                )
                .unwrap();
                tx.execute("INSERT INTO search(note_id,page_id,title,tags,body) VALUES(?1,?2,'Unrelated note','','background text')", params![note.metadata.id.to_string(), header.id.to_string()]).unwrap();
            }
            tx.commit().unwrap();
            // Populate the derived locator table when running the optimized schema.
            if store
                .connection
                .prepare("SELECT 1 FROM search_entries")
                .is_ok()
            {
                store.connection.execute("INSERT OR IGNORE INTO search_entries(id,page_id) SELECT rowid,page_id FROM search", []).unwrap();
            }
            for _ in 0..5 {
                store.save(&delta).unwrap();
            }
            let started = std::time::Instant::now();
            for _ in 0..100 {
                store.save(&delta).unwrap();
            }
            eprintln!(
                "unrelated_pages={unrelated}, mean_save_us={:.1}",
                started.elapsed().as_secs_f64() * 10_000.
            );
            drop(store);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn reader_works_during_a_write_and_never_creates_or_modifies_a_database() {
        let path = path();
        assert!(Store::open_reader(&path).is_err());
        assert!(!path.exists());
        let mut writer = Store::open(&path).unwrap();
        let document = Document::new("Before commit");
        writer.save(&Delta::full(&document)).unwrap();
        let transaction = writer.connection.unchecked_transaction().unwrap();
        transaction
            .execute("UPDATE notes SET metadata='not committed'", [])
            .unwrap();
        let reader = Store::open_reader(&path).unwrap();
        assert_eq!(
            reader.load(document.metadata.id).unwrap().unwrap(),
            document
        );
        assert!(reader.connection.execute("DELETE FROM notes", []).is_err());
        transaction.rollback().unwrap();
        drop(reader);
        drop(writer);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_transaction_keeps_document_and_history_on_one_snapshot() {
        let path = path();
        let mut writer = Store::open(&path).unwrap();
        let mut document = Document::new("Before");
        writer.save(&Delta::full(&document)).unwrap();
        let reader = Store::open_reader(&path).unwrap();
        // A concurrent commit between the first document query and history
        // query must remain invisible until the reader ends its transaction.
        let transaction = reader.connection.unchecked_transaction().unwrap();
        assert_eq!(
            reader.load_snapshot(document.metadata.id).unwrap().unwrap(),
            document
        );
        let before = document.metadata.clone();
        let mut after = before.clone();
        after.title = "After".into();
        let command = Command {
            label: "Rename".into(),
            changes: vec![Change::Metadata { before, after }],
        };
        let mut history = History::default();
        history.execute(command.clone(), &mut document);
        let mut delta = Delta::command(&document, &command);
        delta.journal.push(JournalEvent::Execute(command));
        writer.save(&delta).unwrap();
        assert!(!reader.history(document.metadata.id).unwrap().can_undo());
        transaction.commit().unwrap();
        let (loaded, history) = reader
            .load_with_history(document.metadata.id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, document);
        assert!(history.can_undo());
        assert!(reader.load_with_history(Id::new_v4()).unwrap().is_none());
        drop(reader);
        drop(writer);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn version_two_search_migration_preserves_documents_and_undo() {
        let path = path();
        let mut store = Store::open(&path).unwrap();
        let mut document = Document::new("Old notebook");
        document.metadata.tags = vec!["work".into()];
        let text = Object::Text(TextBlock {
            id: Id::new_v4(),
            text: "Typed content".into(),
            rect: Rect::new(20., 20., 200., 80.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 20.,
            color: Color::INK,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        });
        let group = InkGroup {
            id: Id::new_v4(),
            stroke_ids: vec![],
            bounding_box: Rect::new(20., 20., 50., 30.),
            level: GroupLevel::Line,
            recognized_text: Some("obsoleteocr".into()),
            alternatives: vec![],
            confidence: None,
            language: "eng".into(),
            engine: Some("trocr".into()),
            revision: 0,
        };
        let page = &mut document.pages[0];
        page.groups.push(group.clone());
        page.order.push(text.id());
        page.objects.insert(text.id(), Arc::new(text));
        let mut history = History::default();
        history.execute(
            Command {
                label: "Legacy recognition metadata".into(),
                changes: vec![Change::Groups {
                    page: document.pages[0].id,
                    before: vec![],
                    after: vec![group.clone()],
                }],
            },
            &mut document,
        );
        let mut delta = Delta::full(&document);
        delta.journal = vec![JournalEvent::Replace(history.clone())];
        store.save(&delta).unwrap();
        store
            .connection
            .execute("UPDATE search SET body='obsoleteocr Typed content'", [])
            .unwrap();
        store
            .connection
            .pragma_update(None, "user_version", 2)
            .unwrap();
        drop(store);

        let store = Store::open(path).unwrap();
        let loaded = store.load(document.metadata.id).unwrap().unwrap();
        assert_eq!(loaded, document);
        assert_eq!(
            serde_json::to_value(store.history(document.metadata.id).unwrap()).unwrap(),
            serde_json::to_value(&history).unwrap()
        );
        assert!(store.history(document.metadata.id).unwrap().can_undo());
        assert_eq!(loaded.pages[0].groups, vec![group]);
        assert_eq!(loaded.pages[0].text(), "Typed content");
        let count = |word: &str| {
            store
                .connection
                .query_row(
                    "SELECT count(*) FROM search WHERE search MATCH ?1",
                    [word],
                    |r| r.get::<_, u32>(0),
                )
                .unwrap()
        };
        assert_eq!(count("obsoleteocr"), 0);
        assert_eq!(count("Typed"), 1);
        assert_eq!(count("notebook"), 1);
        assert_eq!(count("work"), 1);
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
    }
    #[test]
    fn math_links_roundtrip_and_keep_current_schema() {
        let path = path();
        let mut store = Store::open(&path).unwrap();
        let mut document = Document::new("Live calculation");
        store.save(&Delta::full(&document)).unwrap();
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
        let id = Id::new_v4();
        let source = Id::new_v4();
        let link = MathLink {
            expression: "a*2=".into(),
            operation: "auto".into(),
            variable: "x".into(),
            domain: "real".into(),
            angle: "radians".into(),
            method: String::new(),
            x_min: -10.,
            x_max: 10.,
            live: true,
            sources: vec![source],
            ink_region: Some(Rect::new(20., 20., 360., 64.)),
            last_error: Some("The source expression no longer parses".into()),
        };
        let equation = Object::Equation(Equation {
            id,
            latex: "10".into(),
            rendered_svg: None,
            rect: Rect::new(20., 100., 180., 64.),
            source_strokes: vec![],
            transform: Transform::default(),
            math_link: Some(link),
        });
        document.pages[0].order.push(id);
        document.pages[0].objects.insert(id, Arc::new(equation));
        store.save(&Delta::full(&document)).unwrap();
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
        drop(store);
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.load(document.metadata.id).unwrap().unwrap(), document);
        document.pages[0].order.clear();
        document.pages[0].objects.clear();
        store.save(&Delta::full(&document)).unwrap();
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn durable_roundtrip_and_incremental_delete() {
        let p = path();
        let mut s = Store::open(&p).unwrap();
        let mut d = Document::new("Journal");
        s.save(&Delta::full(&d)).unwrap();
        assert_eq!(s.load(d.metadata.id).unwrap().unwrap(), d);
        let page = d.pages[0].id;
        let t = Object::Text(TextBlock {
            id: Id::new_v4(),
            text: "search me".into(),
            rect: Rect::new(1., 2., 100., 20.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 16.,
            color: Color::INK,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        });
        let id = t.id();
        let cmd = Command {
            label: "text".into(),
            changes: vec![Change::Object {
                page,
                id,
                before: None,
                after: Some(Arc::new(t)),
                index: 0,
            }],
        };
        cmd.apply(&mut d, true);
        s.save(&Delta::command(&d, &cmd)).unwrap();
        drop(s);
        let mut s = Store::open(&p).unwrap();
        assert_eq!(s.load(d.metadata.id).unwrap().unwrap().pages, d.pages);
        cmd.apply(&mut d, false);
        s.save(&Delta::command(&d, &cmd)).unwrap();
        assert!(
            s.load(d.metadata.id).unwrap().unwrap().pages[0]
                .objects
                .is_empty()
        );
    }
    #[test]
    fn recovery_is_a_valid_database() {
        let p = path();
        let mut s = Store::open(&p).unwrap();
        let d = Document::new("snapshot");
        s.save(&Delta::full(&d)).unwrap();
        let snapshot = s.checkpoint().unwrap();
        let restored = Store::open(snapshot).unwrap();
        assert_eq!(restored.load(d.metadata.id).unwrap().unwrap(), d);
    }
    #[test]
    fn worker_flush_and_reload() {
        let p = path();
        let store = Store::open(&p).unwrap();
        let mut worker = Persistence::new(store);
        let d = Document::new("worker");
        worker.save(Delta::full(&d)).unwrap();
        worker.flush().unwrap();
        drop(worker);
        assert_eq!(
            Store::open(p)
                .unwrap()
                .load(d.metadata.id)
                .unwrap()
                .unwrap(),
            d
        );
    }
}
#[cfg(test)]
mod resilience_tests {
    use super::*;
    #[test]
    fn recovery_preserves_corrupt_original_and_uses_a_valid_snapshot() {
        let root = std::env::temp_dir().join(format!("folio-recovery-{}", Id::new_v4()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let database = root.join("notes.sqlite3");
        let mut store = Store::open(&database).unwrap();
        let document = Document::new("Recovered journal");
        store.save(&Delta::full(&document)).unwrap();
        store.checkpoint().unwrap();
        drop(store);
        std::fs::write(&database, b"broken database").unwrap();
        let before = std::fs::read(&database).unwrap();
        let report = recovery::recover(&root).unwrap();
        assert!(report.snapshot.is_some());
        assert_eq!(report.recovered_notes, 1);
        assert_eq!(std::fs::read(&database).unwrap(), before);
        let recovered = Store::open(report.destination.join("notes.sqlite3")).unwrap();
        assert_eq!(
            recovered.load(document.metadata.id).unwrap().unwrap(),
            document
        );
    }
    #[test]
    fn disk_full_rolls_back_an_entire_command() {
        let root = std::env::temp_dir().join(format!("folio-full-{}.db", Id::new_v4()));
        let mut store = Store::open(&root).unwrap();
        let document = Document::new("Keep me");
        store.save(&Delta::full(&document)).unwrap();
        let count: i64 = store
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        store
            .connection
            .execute_batch(&format!("PRAGMA max_page_count={count}"))
            .unwrap();
        let mut oversized = document.clone();
        oversized.metadata.title = "large".repeat(40000);
        assert!(store.save(&Delta::full(&oversized)).is_err());
        assert_eq!(store.load(document.metadata.id).unwrap().unwrap(), document);
    }
    #[test]
    fn garbage_collection_keeps_assets_referenced_by_undo() {
        let root = std::env::temp_dir().join(format!("folio-gc-{}", Id::new_v4()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let mut store = Store::open(root.join("notes.sqlite3")).unwrap();
        let mut document = Document::new("undo");
        let page = document.pages[0].id;
        let object = Arc::new(Object::Image(ImageObject {
            id: Id::new_v4(),
            asset: "needed.png".into(),
            rect: Rect::new(0., 0., 20., 20.),
            transform: Transform::default(),
            crop: None,
        }));
        document.pages[0].order.push(object.id());
        document.pages[0]
            .objects
            .insert(object.id(), object.clone());
        store.save(&Delta::full(&document)).unwrap();
        let command = Command {
            label: "Delete".into(),
            changes: vec![Change::Object {
                page,
                id: object.id(),
                before: Some(object),
                after: None,
                index: 0,
            }],
        };
        command.apply(&mut document, true);
        let mut delta = Delta::command(&document, &command);
        delta.journal.push(JournalEvent::Execute(command));
        store.save(&delta).unwrap();
        for name in ["needed.png", "orphan.png"] {
            std::fs::write(root.join("assets").join(name), b"test").unwrap();
        }
        drop(store);
        assert_eq!(recovery::quarantine_orphans(&root).unwrap(), 1);
        assert!(root.join("assets/needed.png").is_file());
        assert!(!root.join("assets/orphan.png").exists());
    }
}
