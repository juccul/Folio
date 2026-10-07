//! Recovery never overwrites the source. Healthy snapshots are preferred;
//! salvage copies independently readable documents into a fresh database.
use super::*;
use std::collections::HashSet;

pub fn snapshots(root: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = std::fs::read_dir(root)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("notes.recovery-") && n.ends_with(".sqlite3"))
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries.reverse();
    Ok(entries)
}
fn read_only(path: &Path) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}
fn healthy(path: &Path) -> bool {
    read_only(path)
        .and_then(|c| Ok(c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))?))
        .is_ok_and(|s| s == "ok")
}
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoveryReport {
    pub destination: PathBuf,
    pub snapshot: Option<PathBuf>,
    pub recovered_notes: usize,
    pub issues: Vec<String>,
}

pub fn recover(root: &Path) -> Result<RecoveryReport> {
    let destination = root.parent().unwrap_or(root).join(format!(
        "folio-recovered-{}-{}",
        now_ms(),
        Id::new_v4()
    ));
    std::fs::create_dir(&destination)?;
    let mut report = RecoveryReport {
        destination: destination.clone(),
        snapshot: None,
        recovered_notes: 0,
        issues: vec![],
    };
    let recovered = (|| {
        let snapshot = snapshots(root)?.into_iter().find(|p| healthy(p));
        let database = destination.join("notes.sqlite3");
        if let Some(snapshot) = snapshot {
            std::fs::copy(&snapshot, &database)?;
            let store = Store::open(&database)?;
            report.recovered_notes = store.list_notes()?.len();
            report.snapshot = Some(snapshot);
        } else {
            let original = read_only(&root.join("notes.sqlite3"))?;
            let mut target = Store::open(&database)?;
            let mut notes = original.prepare("SELECT id,metadata FROM notes")?;
            let rows =
                notes.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (note, data) = match row {
                    Ok(value) => value,
                    Err(e) => {
                        report.issues.push(e.to_string());
                        continue;
                    }
                };
                let metadata: NoteMetadata = match serde_json::from_str(&data) {
                    Ok(value) => value,
                    Err(e) => {
                        report.issues.push(format!("Note {note}: {e}"));
                        continue;
                    }
                };
                let mut pages = vec![];
                let mut headers = original
                    .prepare("SELECT header FROM pages WHERE note_id=?1 ORDER BY position")?;
                for row in headers.query_map([&note], |r| r.get::<_, String>(0))? {
                    let header: PageHeader = match row.and_then(|data| {
                        serde_json::from_str(&data)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                    }) {
                        Ok(value) => value,
                        Err(e) => {
                            report.issues.push(format!("Page in {note}: {e}"));
                            continue;
                        }
                    };
                    let mut page = Page {
                        id: header.id,
                        properties: header.properties,
                        objects: Default::default(),
                        order: vec![],
                        groups: vec![],
                        revision: header.revision + 1,
                    };
                    let mut objects =
                        original.prepare("SELECT id,data FROM objects WHERE page_id=?1")?;
                    for row in objects.query_map([header.id.to_string()], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })? {
                        let (id, data) = match row {
                            Ok(value) => value,
                            Err(e) => {
                                report.issues.push(e.to_string());
                                continue;
                            }
                        };
                        match serde_json::from_str::<Object>(&data) {
                            Ok(object) => {
                                let mut isolated = page.clone();
                                isolated.order = vec![object.id()];
                                isolated.objects = std::collections::BTreeMap::from([(
                                    object.id(),
                                    Arc::new(object.clone()),
                                )]);
                                if let Err(error) = (Document {
                                    version: FORMAT_VERSION,
                                    metadata: metadata.clone(),
                                    pages: vec![isolated],
                                })
                                .validate()
                                {
                                    report.issues.push(format!("Object {id}: {error}"));
                                    continue;
                                }
                                page.order.push(object.id());
                                page.objects.insert(object.id(), Arc::new(object));
                            }
                            Err(e) => report.issues.push(format!("Object {id}: {e}")),
                        }
                    }
                    let mut order = header
                        .order
                        .into_iter()
                        .filter(|id| page.objects.contains_key(id))
                        .collect::<Vec<_>>();
                    for id in &page.order {
                        if !order.contains(id) {
                            order.push(*id);
                        }
                    }
                    page.order = order;
                    let candidate = Document {
                        version: FORMAT_VERSION,
                        metadata: metadata.clone(),
                        pages: vec![page.clone()],
                    };
                    if let Err(e) = candidate.validate() {
                        report.issues.push(format!("Page {} skipped: {e}", page.id));
                    } else {
                        pages.push(page);
                    }
                }
                if pages.is_empty() {
                    report.issues.push(format!("No readable pages in {note}"));
                    continue;
                }
                target.save(&Delta::full(&Document {
                    version: FORMAT_VERSION,
                    metadata,
                    pages,
                }))?;
                report.recovered_notes += 1;
            }
            if report.recovered_notes == 0 {
                return Err(Error::Invalid("No readable notes or healthy snapshot was found. Original files were preserved.".into()));
            }
        }
        let assets = destination.join("assets");
        std::fs::create_dir(&assets)?;
        if let Ok(entries) = std::fs::read_dir(root.join("assets")) {
            for entry in entries.flatten() {
                if entry.file_type()?.is_file() {
                    std::fs::copy(entry.path(), assets.join(entry.file_name()))?;
                }
            }
        }
        std::fs::write(
            destination.join("RECOVERY.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        Ok(report)
    })();
    // Failed recoveries remain available for examination; originals never move.
    recovered
}

fn asset_references(document: &Document, keep: &mut HashSet<String>) {
    for page in &document.pages {
        if let Some(background) = &page.properties.pdf {
            keep.insert(background.asset.clone());
            if let Some(preview) = &background.preview_asset {
                keep.insert(preview.clone());
            }
            if let Some(stem) = background.asset.strip_suffix("-unlocked.pdf") {
                keep.insert(format!("{stem}.pdf"));
            }
        }
        for object in page.objects.values() {
            if let Object::Image(image) = object.as_ref() {
                keep.insert(image.asset.clone());
            }
        }
    }
}
/// Quarantine unreferenced assets after considering notes, undo history and every
/// retained snapshot. Nothing is permanently deleted by this maintenance action.
pub fn quarantine_orphans(root: &Path) -> Result<usize> {
    let mut keep = HashSet::new();
    let mut databases = vec![root.join("notes.sqlite3")];
    databases.extend(snapshots(root)?);
    for path in databases {
        let connection = read_only(&path)?;
        // A damaged snapshot prevents cleanup rather than guessing its references.
        let notes = {
            let mut query = connection.prepare("SELECT metadata FROM notes")?;
            query
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let store = Store { connection, path };
        if let Some(preferences) = store.setting::<serde_json::Value>("preferences")?
            && let Some(templates) = preferences.get("templates").and_then(|v| v.as_array())
        {
            for template in templates {
                for key in ["asset", "preview"] {
                    if let Some(name) = template.get(key).and_then(|v| v.as_str()) {
                        keep.insert(name.into());
                    }
                }
            }
        }

        for metadata in notes {
            let note: NoteMetadata = serde_json::from_str(&metadata)?;
            if let Some(document) = store.load(note.id)? {
                asset_references(&document, &mut keep);
            }
        }
        let has_history: bool = store.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='history')",
            [],
            |r| r.get(0),
        )?;
        if !has_history {
            continue;
        }
        let mut history = store.connection.prepare("SELECT command FROM history")?;
        for row in history.query_map([], |r| r.get::<_, String>(0))? {
            let command: Command = serde_json::from_str(&row?)?;
            for change in command.changes {
                match change {
                    Change::Object { before, after, .. } => {
                        for object in before.into_iter().chain(after) {
                            if let Object::Image(image) = object.as_ref() {
                                keep.insert(image.asset.clone());
                            }
                        }
                    }
                    Change::Page { before, after, .. } => {
                        for page in before.into_iter().chain(after) {
                            asset_references(
                                &Document {
                                    version: FORMAT_VERSION,
                                    metadata: Document::new("history").metadata,
                                    pages: vec![page],
                                },
                                &mut keep,
                            );
                        }
                    }
                    Change::Properties { before, after, .. } => {
                        for properties in [before, after] {
                            if let Some(pdf) = properties.pdf {
                                keep.insert(pdf.asset);
                                if let Some(preview) = pdf.preview_asset {
                                    keep.insert(preview);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let assets = root.join("assets");
    let quarantine = root.join("orphaned-assets");
    std::fs::create_dir_all(&quarantine)?;
    let mut count = 0;
    for entry in std::fs::read_dir(assets)? {
        let entry = entry?;
        let name = entry.file_name();
        if entry.file_type()?.is_file() && !keep.contains(&name.to_string_lossy().into_owned()) {
            std::fs::rename(
                entry.path(),
                quarantine.join(format!("{}-{}", Id::new_v4(), name.to_string_lossy())),
            )?;
            count += 1;
        }
    }
    Ok(count)
}
