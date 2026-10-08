//! Application controller. Commands, input routing and worker scheduling live here;
//! the GPUI layer only displays state and forwards user actions.
#[cfg(test)]
mod gesture_tests;
mod handwriting_search;
mod library_actions;
mod library_previews;
#[cfg(test)]
mod management_tests;
#[cfg(test)]
mod optimization_tests;
mod page_actions;
pub mod portable;
mod starter;
mod templates;
pub use library_actions::NoteAction;
pub mod appearance;
mod math_solver;
mod recognition;
mod settings;
#[cfg(test)]
mod shape_tests;
mod shape_tools;
mod workers;
use folio_canvas::{PageStack, SpatialIndex, Viewport};
use folio_document::*;
use folio_ink::StrokeBuilder;
use folio_input::{PenEvent, Phase, Tool as PenTool};
pub use folio_math::{VectorCommand, VectorFormula};
use folio_storage::{Delta, JournalEvent, Persistence, Store};
pub use math_solver::{MathReport, MathRequest, MathSession, MathStep};
pub use recognition::{RecognitionKind, RecognitionReview};
pub use settings::{PageTemplate, Settings};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
pub use workers::ExportKind;
use workers::*;
const INK_HOLD_DELAY: Duration = Duration::from_millis(550);
const INK_ENDPOINT_SLOP: f32 = 4.;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser,
    Lasso,
    Rectangle,
    Hand,
    Shape,
    Text,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteFilter {
    All,
    Favorites,
    Recent,
    Trash,
    Notebook(Id),
}
pub struct Session {
    pub document: Document,
    pub history: History,
    pub page: usize,
    pub viewport: Viewport,
    pub selection: HashSet<Id>,
    pub index: SpatialIndex,
    pub order_positions: HashMap<Id, usize>,
    pending_journal: Vec<JournalEvent>,
    linked_math: HashSet<Id>,
}
impl Session {
    fn new(document: Document) -> Self {
        let mut index = SpatialIndex::default();
        index.rebuild(&document.pages[0]);
        let order_positions = document.pages[0]
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        let linked_math = linked_math_objects(&document.pages[0]);
        Self {
            document,
            linked_math,
            history: History::default(),
            page: 0,
            viewport: Viewport::default(),
            selection: HashSet::new(),
            index,
            order_positions,
            pending_journal: vec![],
        }
    }
    pub fn page(&self) -> &Page {
        &self.document.pages[self.page]
    }
    pub fn page_mut(&mut self) -> &mut Page {
        &mut self.document.pages[self.page]
    }
    fn refresh_command(&mut self, command: &Command) {
        let old_page = self.page;
        self.page = self.page.min(self.document.pages.len() - 1);
        let page = &self.document.pages[self.page];
        let pages_changed = command
            .changes
            .iter()
            .any(|c| matches!(c, Change::Page { .. }));
        let object_changes = command
            .changes
            .iter()
            .filter_map(|c| match c {
                Change::Object {
                    page: id,
                    id: object,
                    ..
                } if *id == page.id => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>();
        let inserted = object_changes
            .iter()
            .copied()
            .filter(|id| !self.order_positions.contains_key(id) && page.objects.contains_key(id))
            .collect::<HashSet<_>>();
        let removed = object_changes
            .iter()
            .any(|id| self.order_positions.contains_key(id) && !page.objects.contains_key(id));
        let suffix = page.order.len().saturating_sub(inserted.len());
        let appended = page.order[suffix..].iter().all(|id| inserted.contains(id));
        if pages_changed || removed || !appended {
            self.order_positions = page
                .order
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, i))
                .collect();
        } else {
            for (offset, id) in page.order[suffix..].iter().enumerate() {
                self.order_positions.insert(*id, suffix + offset);
            }
        }
        if pages_changed || old_page != self.page {
            self.linked_math = linked_math_objects(page);
        } else {
            for id in &object_changes {
                if page.objects.get(id).is_some_and(|object|
                    matches!(object.as_ref(), Object::Equation(equation) if equation.math_link.is_some())) {
                    self.linked_math.insert(*id);
                } else {
                    self.linked_math.remove(id);
                }
            }
        }
        self.selection.retain(|id| page.objects.contains_key(id));
        if old_page != self.page {
            self.index.rebuild(page);
        } else {
            self.index.update(page, command);
        }
    }
    fn refresh(&mut self) {
        self.page = self.page.min(self.document.pages.len() - 1);
        self.selection
            .retain(|id| self.document.pages[self.page].objects.contains_key(id));
        self.index.rebuild(&self.document.pages[self.page]);
        self.linked_math = linked_math_objects(&self.document.pages[self.page]);
        self.order_positions = self.document.pages[self.page]
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
    }
}
fn linked_math_objects(page: &Page) -> HashSet<Id> {
    page.objects
        .values()
        .filter_map(|object| match object.as_ref() {
            Object::Equation(equation) if equation.math_link.is_some() => Some(equation.id),
            _ => None,
        })
        .collect()
}

/// Immutable target captured before a native export dialog can switch focus.
pub struct ExportSnapshot {
    document: Document,
    page: Id,
}
pub enum Interaction {
    Ink {
        builder: StrokeBuilder,
        last_move: Instant,
        anchor: Point,
        fit_attempted: bool,
        preview: Option<folio_shapes::Fit>,
        suppress_tap: bool,
    },
    Erase {
        ids: HashSet<Id>,
        last: Point,
        fragments: HashMap<Id, Vec<InkStroke>>,
    },
    Lasso {
        points: Vec<Point>,
    },
    Rectangle {
        start: Point,
        end: Point,
    },
    Move {
        start: Point,
        end: Point,
    },
    Resize {
        anchor: Point,
        start: Point,
        end: Point,
    },
    Rotate {
        center: Point,
        start: Point,
        end: Point,
    },
    Pan {
        start: Point,
        pan: Point,
    },
}
impl Interaction {
    fn update_ink_endpoint(&mut self, p: Point, zoom: f32) {
        if let Self::Ink {
            last_move,
            anchor,
            fit_attempted,
            preview,
            suppress_tap,
            ..
        } = self
            && p.distance(*anchor) > INK_ENDPOINT_SLOP / zoom
        {
            *last_move = Instant::now();
            *anchor = p;
            *preview = None;
            *fit_attempted = false;
            *suppress_tap = false;
        }
    }
}
pub struct RasterPreview {
    pub pixels_budget: u32,
    pub object: Arc<Object>,
    pub bounds: Rect,
    pub width: u32,
    pub height: u32,
    pub bgra: Arc<Vec<u8>>,
}
#[derive(Clone, Debug)]
pub struct LibraryImport {
    pub path: PathBuf,
    pub pending: bool,
    pub error: Option<String>,
}
pub struct Controller {
    pub recognition_for_index: bool,
    pub restored_library: Option<PathBuf>,
    library_previews: HashMap<Id, (u64, Arc<Page>, usize)>,
    library_preview_pending: HashSet<Id>,
    library_preview_failed: HashMap<Id, u64>,
    library_preview_recency: std::collections::VecDeque<Id>,
    math_service: math_solver::Service,
    math_generation: u64,
    pub math_session: Option<MathSession>,
    math_after_ocr: bool,
    math_ocr_next: bool,
    math_live_pending: HashSet<Id>,
    math_live_signatures: HashMap<Id, String>,
    last_math_scan: Instant,
    math_scan_cursor: usize,
    last_math_ink: Instant,
    math_live_ocr: Option<(Id, String)>,
    math_ink_signatures: HashMap<Id, String>,
    recognition_service: recognition::Service,
    recognition_generation: u64,
    pub recognition_pending: bool,
    pub recognition_status: String,
    pub recognition_replacing: bool,
    pub recognition_review: Option<RecognitionReview>,
    pub previews: HashMap<(Id, Id, Id), RasterPreview>,
    preview_pending: HashMap<(Id, Id, Id), Arc<Object>>,
    session_recency: std::collections::VecDeque<Id>,
    last_draft: Instant,
    last_cache_trim: Instant,
    pending_actions: HashMap<Id, Vec<NoteAction>>,
    pending_imports: HashMap<Id, usize>,
    pub library_imports: HashMap<Id, LibraryImport>,
    provisional_imports: HashSet<Id>,
    pdf_password_queue: std::collections::VecDeque<(Id, PathBuf)>,
    pending_note: Option<Id>,
    loading_notes: HashSet<Id>,
    pending_navigation: Option<(Id, Id)>,
    pending_search: Option<(String, u64)>,
    pub notes: Vec<NoteMetadata>,
    pub notebooks: Vec<Notebook>,
    pub sessions: HashMap<Id, Session>,
    pub active: Id,
    pub tool: Tool,
    temporary_selection: bool,
    pub style: PenStyle,
    pub settings: Settings,
    pub filter: NoteFilter,
    pub interaction: Option<Interaction>,
    pub cursor: Option<Point>,
    pub status: String,
    pub error: Option<String>,
    pub save_error: Option<String>,
    equation_generation: u64,
    pub equation_pending: bool,
    pub equation_result: Option<Result<(), String>>,
    equation_live_wait: Option<(Id, String)>,
    preview_errors: HashMap<(Id, Id), String>,
    pub busy: usize,
    pub pending_text: Option<Point>,
    pub pending_text_edit: Option<Id>,
    text_click: Option<(Id, Point)>,
    pub pending_pdf_password: Option<(Id, PathBuf)>,
    pdf_preview_pending: HashSet<String>,
    pdf_preview_failed: HashSet<String>,
    preview_failed: HashMap<(Id, Id, Id), Arc<Object>>,
    pub search_results: Vec<folio_search::SearchResult>,
    pub search_query: String,
    pub search_highlights: Vec<Rect>,
    pub data_dir: PathBuf,
    pub assets: PathBuf,
    pub database: PathBuf,
    persistence: Persistence,
    workers: Workers,
    queued: u64,
    saved: u64,
    dirty_notes: HashSet<Id>,
    save_notes: HashMap<u64, Id>,
    last_retry: Instant,
    search_generation: u64,
    // Keep the writer lease until persistence and all worker owners are dropped.
    _lock: folio_platform::DataDirLock,
}
impl Controller {
    pub fn open(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let lock = folio_platform::lock_data_dir(&data_dir).map_err(|e| e.to_string())?;
        let assets = data_dir.join("assets");
        std::fs::create_dir_all(&assets).map_err(|e| e.to_string())?;
        let database = data_dir.join("notes.sqlite3");
        let mut store = Store::open(&database).map_err(|e| e.to_string())?;
        let mut settings: Settings = store
            .setting("preferences")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        settings.normalize();
        let mut notes = store.list_notes().map_err(|e| e.to_string())?;
        let notebooks = store.notebooks().map_err(|e| e.to_string())?;
        let document = if let Some(n) = notes.iter().find(|n| !n.trashed) {
            store
                .load(n.id)
                .map_err(|e| e.to_string())?
                .ok_or("Missing note data")?
        } else {
            let mut doc = Document::new("Untitled note");
            doc.pages[0].properties.paper = settings.paper;
            store.save(&Delta::full(&doc)).map_err(|e| e.to_string())?;
            notes.insert(0, doc.metadata.clone());
            doc
        };
        let active = document.metadata.id;
        let mut sessions = HashMap::new();
        let history = store.history(active).map_err(|e| e.to_string())?;
        let mut session = Session::new(document);
        session.history = history;
        sessions.insert(active, session);
        let style = settings.default_pen.clone();
        let controller = Self {
            recognition_for_index: false,
            restored_library: None,
            library_previews: HashMap::new(),
            library_preview_pending: HashSet::new(),
            library_preview_failed: HashMap::new(),
            library_preview_recency: std::collections::VecDeque::new(),
            math_service: math_solver::Service::new(),
            math_generation: 0,
            math_session: None,
            math_after_ocr: false,
            math_ocr_next: false,
            math_live_pending: HashSet::new(),
            math_live_signatures: HashMap::new(),
            last_math_scan: Instant::now(),
            math_scan_cursor: 0,
            last_math_ink: Instant::now(),
            math_live_ocr: None,
            math_ink_signatures: HashMap::new(),
            recognition_service: recognition::Service::new(),
            recognition_generation: 0,
            recognition_pending: false,
            recognition_status: String::new(),
            recognition_replacing: false,
            recognition_review: None,
            _lock: lock,
            session_recency: std::collections::VecDeque::from([active]),
            last_draft: Instant::now(),
            last_cache_trim: Instant::now(),
            pending_actions: HashMap::new(),
            pending_imports: HashMap::new(),
            library_imports: HashMap::new(),
            provisional_imports: HashSet::new(),
            pdf_password_queue: std::collections::VecDeque::new(),
            pending_note: None,
            loading_notes: HashSet::new(),
            pending_navigation: None,
            pending_search: None,
            previews: HashMap::new(),
            preview_pending: HashMap::new(),
            notes,
            notebooks,
            sessions,
            active,
            tool: Tool::Pen,
            temporary_selection: false,
            style,
            settings,
            filter: NoteFilter::All,
            interaction: None,
            cursor: None,
            status: "All changes saved".into(),
            error: None,
            save_error: None,
            equation_generation: 0,
            equation_pending: false,
            equation_result: None,
            equation_live_wait: None,
            preview_errors: HashMap::new(),
            busy: 0,
            pending_text: None,
            pending_text_edit: None,
            text_click: None,
            pending_pdf_password: None,
            pdf_preview_pending: HashSet::new(),
            pdf_preview_failed: HashSet::new(),
            preview_failed: HashMap::new(),
            search_results: vec![],
            search_query: String::new(),
            search_highlights: vec![],
            data_dir,
            assets,
            database,
            persistence: Persistence::new(store),
            workers: Workers::new(),
            queued: 0,
            saved: 0,
            dirty_notes: HashSet::new(),
            save_notes: HashMap::new(),
            last_retry: Instant::now(),
            search_generation: 0,
        };
        Ok(controller)
    }
    pub fn session(&self) -> &Session {
        &self.sessions[&self.active]
    }
    pub fn session_mut(&mut self) -> &mut Session {
        self.sessions.get_mut(&self.active).unwrap()
    }
    pub fn page(&self) -> &Page {
        self.session().page()
    }
    pub fn visible_notes(&self) -> Vec<&NoteMetadata> {
        self.notes
            .iter()
            .filter(|n| match self.filter {
                NoteFilter::Trash => n.trashed,
                NoteFilter::Favorites => !n.trashed && n.favorite,
                NoteFilter::Notebook(id) => !n.trashed && n.notebook == Some(id),
                _ => !n.trashed,
            })
            .take(if self.filter == NoteFilter::Recent {
                20
            } else {
                usize::MAX
            })
            .collect()
    }
    fn persist(&mut self, mut delta: Delta) {
        let id = delta.metadata.id;
        if !self.settings.autosave {
            if let Some(session) = self.sessions.get_mut(&id) {
                session.pending_journal = vec![JournalEvent::Replace(session.history.clone())];
            }
            self.status = "Unsaved · autosave is off".into();
            return;
        }
        if let Some(session) = self.sessions.get_mut(&id) {
            if self.dirty_notes.contains(&id) {
                delta = Delta::full(&session.document);
                delta.journal = vec![JournalEvent::Replace(session.history.clone())];
                session.pending_journal.clear();
            } else if !session.pending_journal.is_empty() {
                delta.journal = std::mem::take(&mut session.pending_journal);
            }
        }
        match self.persistence.save(delta) {
            Ok(seq) => {
                self.save_notes.insert(seq, id);
                self.dirty_notes.remove(&id);
                self.queued = seq;
                self.status = "Saving…".into();
            }
            Err(e) => {
                self.dirty_notes.insert(id);
                if let Some(s) = self.sessions.get_mut(&id) {
                    s.pending_journal = vec![JournalEvent::Replace(s.history.clone())];
                }
                self.status = "Changes are not saved".into();
                self.save_error = Some(e);
            }
        }
    }
    fn refresh_metadata(&mut self, id: Id) {
        if let Some(session) = self.sessions.get(&id)
            && let Some(n) = self.notes.iter_mut().find(|n| n.id == id)
        {
            *n = session.document.metadata.clone()
        }
        self.notes.sort_by_key(|n| std::cmp::Reverse(n.updated_at));
    }
    pub fn commit(&mut self, label: &str, changes: Vec<Change>) {
        self.commit_to(self.active, label, changes)
    }
    pub fn read_only(&self) -> bool {
        self.session().document.metadata.trashed
    }
    fn commit_to(&mut self, note: Id, label: &str, mut changes: Vec<Change>) {
        if self
            .sessions
            .get(&note)
            .is_some_and(|s| s.document.metadata.trashed)
        {
            let restoring = changes.len() == 1
                && matches!(&changes[0], Change::Metadata {before,after} if {
                    let mut restored = before.clone(); restored.trashed = false; before.trashed && *after == restored
                });
            if !restoring {
                self.status = "In Trash · restore this document to edit".into();
                return;
            }
        }
        if changes.is_empty() {
            return;
        }
        if let Some(session) = self.sessions.get(&note) {
            handwriting_search::maintain_index(&session.document, &mut changes);
        }
        self.provisional_imports.remove(&note);
        let cmd = Command {
            label: label.into(),
            changes,
        };
        let Some(s) = self.sessions.get_mut(&note) else {
            return;
        };
        s.history.execute(cmd.clone(), &mut s.document);
        s.refresh_command(&cmd);
        let mut delta = Delta::command(&s.document, &cmd);
        delta.journal.push(JournalEvent::Execute(cmd));
        self.refresh_metadata(note);
        if note == self.active && self.read_only() {
            self.tool = Tool::Hand;
        }
        self.persist(delta);
    }
    pub fn undo(&mut self) {
        if self.read_only() {
            return;
        }
        self.cancel();
        let s = self.session_mut();
        let current_page = s.page().id;
        if let Some(cmd) = s.history.undo(&mut s.document) {
            if cmd
                .changes
                .iter()
                .any(|change| matches!(change, Change::Page { .. }))
                && let Some(index) = s.document.pages.iter().position(|p| p.id == current_page)
            {
                s.page = index;
            }
            s.refresh_command(&cmd);
            let mut delta = Delta::command(&s.document, &cmd);
            delta.journal.push(JournalEvent::Undo);
            self.refresh_metadata(self.active);
            self.persist(delta);
        }
    }
    pub fn redo(&mut self) {
        if self.read_only() {
            return;
        }
        self.cancel();
        let s = self.session_mut();
        let current_page = s.page().id;
        if let Some(cmd) = s.history.redo(&mut s.document) {
            if cmd
                .changes
                .iter()
                .any(|change| matches!(change, Change::Page { .. }))
                && let Some(index) = s.document.pages.iter().position(|p| p.id == current_page)
            {
                s.page = index;
            }
            s.refresh_command(&cmd);
            let mut delta = Delta::command(&s.document, &cmd);
            delta.journal.push(JournalEvent::Redo);
            self.refresh_metadata(self.active);
            self.persist(delta);
        }
    }
    pub fn save(&mut self) {
        if self.provisional_imports.contains(&self.active) {
            return;
        }
        let delta = Delta::full(&self.session().document);
        let autosave = self.settings.autosave;
        self.settings.autosave = true;
        self.persist(delta);
        self.settings.autosave = autosave;
    }
    pub fn flush(&mut self) -> Result<(), String> {
        self.finish();
        let documents = self
            .sessions
            .values()
            .filter(|s| !self.provisional_imports.contains(&s.document.metadata.id))
            .map(|s| {
                let mut delta = Delta::full(&s.document);
                delta.journal = vec![JournalEvent::Replace(s.history.clone())];
                delta
            })
            .collect::<Vec<_>>();
        for delta in documents {
            let note = delta.metadata.id;
            let sequence = self.persistence.save_blocking(delta)?;
            self.queued = sequence;
            self.save_notes.insert(sequence, note);
            self.dirty_notes.remove(&note);
            if let Some(session) = self.sessions.get_mut(&note) {
                session.pending_journal.clear();
            }
        }
        self.persistence.flush()
    }
    pub fn has_background_work(&self) -> bool {
        self.busy > 0 || self.pending_search.is_some()
    }
    pub fn create_note(&mut self) {
        let properties = PageProperties {
            paper: self.settings.paper,
            ..PageProperties::default()
        };
        self.create_note_with_properties("Untitled note".into(), properties)
            .expect("Default page properties are valid");
    }
    pub fn create_note_with_properties(
        &mut self,
        title: String,
        properties: PageProperties,
    ) -> Result<Id, String> {
        self.create_note_with_properties_inner(title, properties, true)
    }
    fn create_note_with_properties_inner(
        &mut self,
        title: String,
        properties: PageProperties,
        persist: bool,
    ) -> Result<Id, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Give your notebook a name.".into());
        }
        if !properties.width.is_finite()
            || !properties.height.is_finite()
            || !(64.0..=100000.0).contains(&properties.width)
            || !(64.0..=100000.0).contains(&properties.height)
            || properties.pdf.is_some()
        {
            return Err("Choose a valid canvas size.".into());
        }
        self.finish();
        self.end_temporary_selection();
        self.pending_note = None;
        self.pending_navigation = None;
        let mut d = Document::new(title);
        d.pages[0].properties = properties;
        if let NoteFilter::Notebook(id) = self.filter {
            d.metadata.notebook = Some(id)
        }
        let id = d.metadata.id;
        let delta = Delta::full(&d);
        self.notes.insert(0, d.metadata.clone());
        self.sessions.insert(id, Session::new(d));
        self.active = id;
        self.filter = NoteFilter::All;
        self.search_highlights.clear();
        if persist {
            self.persist(delta);
        } else {
            self.provisional_imports.insert(id);
        }
        Ok(id)
    }
    pub fn requested_note(&self) -> Id {
        self.pending_note.unwrap_or(self.active)
    }
    pub fn loading_note(&self) -> bool {
        self.pending_note.is_some()
    }
    pub fn switch_note(&mut self, id: Id) {
        self.finish();
        self.end_temporary_selection();
        self.pending_navigation = None;
        self.search_highlights.clear();
        self.pending_note = None;
        if self.sessions.contains_key(&id) {
            self.active = id;
            if self.read_only() {
                self.tool = Tool::Hand;
            }
            self.cursor = None;
        } else {
            if self.queue_load(id) {
                self.pending_note = Some(id);
            }
        }
    }
    fn queue_load(&mut self, id: Id) -> bool {
        if self.loading_notes.contains(&id) {
            return true;
        }
        match self.workers.submit(Job::Load {
            id,
            database: self.database.clone(),
        }) {
            Ok(()) => {
                self.loading_notes.insert(id);
                self.busy += 1;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }
    pub fn rename(&mut self, title: String) {
        let before = self.session().document.metadata.clone();
        let mut after = before.clone();
        after.title = if title.trim().is_empty() {
            "Untitled note".into()
        } else {
            title.trim().into()
        };
        self.commit("Rename note", vec![Change::Metadata { before, after }]);
    }
    pub fn metadata(&mut self, change: impl FnOnce(&mut NoteMetadata)) {
        let before = self.session().document.metadata.clone();
        let mut after = before.clone();
        change(&mut after);
        self.commit(
            "Change note details",
            vec![Change::Metadata { before, after }],
        );
    }
    pub fn duplicate_note(&mut self) {
        self.finish();
        self.pending_note = None;
        self.pending_navigation = None;
        let id = self.duplicate_loaded_note(self.active);
        self.active = id;
    }
    fn duplicate_loaded_note(&mut self, source: Id) -> Id {
        let old = self.sessions[&source].document.clone();
        let mut d = Document::new(format!("{} (copy)", old.metadata.title));
        d.metadata.tags = old.metadata.tags;
        d.metadata.notebook = old.metadata.notebook;
        let mut cover = None;
        d.pages = old
            .pages
            .iter()
            .map(|page| {
                let copy = page.duplicate();
                if Some(page.id) == old.metadata.cover_page {
                    cover = Some(copy.id);
                }
                copy
            })
            .collect();
        d.metadata.cover_page = cover;
        let id = d.metadata.id;
        self.notes.insert(0, d.metadata.clone());
        let delta = Delta::full(&d);
        self.sessions.insert(id, Session::new(d));
        self.persist(delta);
        id
    }
    pub fn create_notebook(&mut self, name: String, parent: Option<Id>) {
        if name.trim().is_empty() {
            return;
        }
        let n = Notebook {
            id: Id::new_v4(),
            name: name.trim().into(),
            parent,
        };
        self.persistence.notebook(n.clone());
        self.notebooks.push(n);
    }
    pub fn rename_notebook(&mut self, id: Id, name: String) {
        if let Some(n) = self.notebooks.iter_mut().find(|n| n.id == id) {
            n.name = name;
            self.persistence.notebook(n.clone());
        }
    }
    pub fn move_notebook(&mut self, id: Id, parent: Option<Id>) -> Result<(), String> {
        let mut cursor = parent;
        let mut visited = HashSet::new();
        while let Some(next) = cursor {
            if next == id || !visited.insert(next) {
                return Err("A folder cannot be moved inside itself or a descendant".into());
            }
            cursor = self
                .notebooks
                .iter()
                .find(|n| n.id == next)
                .ok_or("Destination folder no longer exists")?
                .parent;
        }
        let n = self
            .notebooks
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or("Folder no longer exists")?;
        n.parent = parent;
        self.persistence.notebook(n.clone());
        Ok(())
    }
    pub fn delete_empty_notebook(&mut self, id: Id) -> Result<(), String> {
        if self.notebooks.iter().any(|n| n.parent == Some(id))
            || self.notes.iter().any(|n| n.notebook == Some(id))
        {
            return Err("Move the notes and child folders before deleting this folder".into());
        }
        self.notebooks.retain(|n| n.id != id);
        self.persistence.delete_notebook(id);
        if self.filter == NoteFilter::Notebook(id) {
            self.filter = NoteFilter::All;
        }
        Ok(())
    }
    pub fn add_page(&mut self) {
        self.finish();
        let mut p = Page::new();
        p.properties = if self.page().properties.pdf.is_none() {
            self.page().properties.clone()
        } else {
            PageProperties {
                paper: self.settings.paper,
                ..PageProperties::default()
            }
        };
        let index = self.session().document.pages.len();
        self.commit(
            "Add page",
            vec![Change::Page {
                index,
                before: None,
                after: Some(p),
            }],
        );
        self.change_page(index);
    }
    pub fn delete_page(&mut self) {
        self.finish();
        if self.session().document.pages.len() < 2 {
            self.error = Some("A note must keep at least one page".into());
            return;
        }
        let index = self.session().page;
        let p = self.page().clone();
        self.commit(
            "Delete page",
            vec![Change::Page {
                index,
                before: Some(p),
                after: None,
            }],
        );
    }
    pub fn change_page(&mut self, index: usize) {
        self.finish();
        self.end_temporary_selection();
        let s = self.session_mut();
        let old_width = s.page().properties.width;
        s.page = index.min(s.document.pages.len() - 1);
        if !s.page().properties.infinite {
            s.viewport.pan.x += (old_width - s.page().properties.width) * s.viewport.zoom / 2.;
            s.viewport.pan.y = 36.;
        }
        s.selection.clear();
        s.refresh();
        self.cursor = None;
        self.search_highlights.clear();
    }
    pub fn paper(&mut self, paper: Paper) {
        let before = self.page().properties.clone();
        let mut after = before.clone();
        after.paper = paper;
        self.commit(
            "Change paper",
            vec![Change::Properties {
                page: self.page().id,
                before,
                after,
            }],
        );
    }
    /// Rebase onto the page in view without changing any page's screen position.
    pub fn synchronize_page_view(&mut self, width: f32, height: f32) {
        if self.interaction.is_some() || self.loading_note() {
            return;
        }
        let session = self.session();
        let Some(stack) = PageStack::new(&session.document.pages, session.page) else {
            return;
        };
        let active = session.page;
        let mut viewport = session.viewport;
        stack.clamp_vertical(&mut viewport, active, height);
        let target = stack.nearest(viewport, active, Point::new(width / 2., height / 2.));
        let viewport = stack.viewport(viewport, active, target);
        if target != active {
            self.change_page(target);
        }
        self.session_mut().viewport = viewport;
    }
    fn focus_page_at(&mut self, screen: Point) {
        let session = self.session();
        let Some(stack) = PageStack::new(&session.document.pages, session.page) else {
            return;
        };
        if let Some(target) = stack.hit(session.viewport, session.page, screen)
            && target != session.page
        {
            let viewport = stack.viewport(session.viewport, session.page, target);
            self.change_page(target);
            self.session_mut().viewport = viewport;
        }
    }
    pub fn page_size(&mut self, width: f32, height: f32, infinite: bool) {
        if !(64.0..=100000.0).contains(&width) || !(64.0..=100000.0).contains(&height) {
            self.error = Some("Page sizes must be between 64 and 100000 units".into());
            return;
        }
        let before = self.page().properties.clone();
        let mut after = before.clone();
        after.width = width;
        after.height = height;
        after.infinite = infinite;
        self.commit(
            "Page size",
            vec![Change::Properties {
                page: self.page().id,
                before,
                after,
            }],
        );
    }
    fn end_temporary_selection(&mut self) {
        if self.temporary_selection {
            self.tool = Tool::Pen;
            self.temporary_selection = false;
        }
    }
    pub fn set_tool(&mut self, tool: Tool) {
        if self.read_only() && !matches!(tool, Tool::Hand | Tool::Lasso | Tool::Rectangle) {
            return;
        }
        self.finish();
        self.tool = tool;
        self.temporary_selection = false;
        self.pending_text = None;
    }
    pub fn set_color(&mut self, color: Color) {
        self.style.color = color;
        self.settings.recent_colors.retain(|c| *c != color);
        self.settings.recent_colors.insert(0, color);
        self.settings.recent_colors.truncate(8);
        self.store_settings();
    }
    pub fn set_style(&mut self, style: PenStyle) {
        self.style = style;
    }
    pub fn store_settings(&mut self) {
        self.settings.default_pen = self.style.clone();
        match serde_json::to_string(&self.settings) {
            Ok(data) => self.persistence.settings(data),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn set_autosave(&mut self, enabled: bool) {
        let previously = self.settings.autosave;
        self.settings.autosave = enabled;
        if enabled && !previously {
            let deltas = self
                .sessions
                .values()
                .map(|s| Delta::full(&s.document))
                .collect::<Vec<_>>();
            for delta in deltas {
                self.persist(delta);
            }
        }
        self.store_settings();
    }
    pub fn save_preset(&mut self) {
        self.settings.presets.push(self.style.clone());
        self.store_settings();
    }
    pub fn cancel(&mut self) {
        if let Some(Interaction::Ink { builder, .. }) = &self.interaction {
            self.persistence
                .draft(self.active, self.page().id, builder.id(), None);
        }
        self.interaction = None;
        self.pending_text = None;
        self.pending_text_edit = None;
        self.text_click = None;
        self.cursor = None;
    }
    pub fn pointer(&mut self, event: PenEvent) {
        // A tab can be selected before its background load completes. Never
        // apply that tab's input to the document that is still displayed.
        if self.loading_note() {
            return;
        }
        let screen = event.position;
        if event.phase == Phase::Down && self.tool != Tool::Hand {
            self.finish();
            self.focus_page_at(screen);
        }
        let p = self.session().viewport.to_document(screen);
        self.cursor = Some(p);
        if event.phase == Phase::Down {
            self.finish();
            if self.session().document.metadata.trashed {
                if self.tool != Tool::Hand {
                    self.status = "In Trash · restore this document to edit".into();
                    return;
                }
            }
            if self.tool == Tool::Hand {
                self.interaction = Some(Interaction::Pan {
                    start: screen,
                    pan: self.session().viewport.pan,
                });
                return;
            }
            if !self.page().properties.infinite
                && !Rect::new(
                    0.,
                    0.,
                    self.page().properties.width,
                    self.page().properties.height,
                )
                .contains(p)
            {
                return;
            }
            let mut tool = if event.tool == PenTool::Eraser || event.buttons & 2 != 0 {
                Tool::Eraser
            } else {
                self.tool
            };
            let selection_hit = self
                .selection_bounds()
                .is_some_and(|r| r.expand(8.).contains(p));
            if matches!(tool, Tool::Lasso | Tool::Rectangle)
                && let Some(bounds) = self.selection_bounds()
            {
                let corners = [
                    bounds.min,
                    Point::new(bounds.max.x, bounds.min.y),
                    bounds.max,
                    Point::new(bounds.min.x, bounds.max.y),
                ];
                if let Some((i, corner)) = corners
                    .iter()
                    .enumerate()
                    .find(|(_, corner)| corner.distance(p) < 10. / self.session().viewport.zoom)
                {
                    self.interaction = Some(Interaction::Resize {
                        anchor: corners[(i + 2) % 4],
                        start: *corner,
                        end: p,
                    });
                    return;
                }
                let handle = Point::new(
                    bounds.center().x,
                    bounds.min.y - 24. / self.session().viewport.zoom,
                );
                if p.distance(handle) < 10. / self.session().viewport.zoom {
                    self.interaction = Some(Interaction::Rotate {
                        center: bounds.center(),
                        start: p,
                        end: p,
                    });
                    return;
                }
            }
            self.text_click = None;
            let text_hit = if !matches!(tool, Tool::Eraser | Tool::Hand | Tool::Shape)
                && (event.device == folio_input::Device::Mouse
                    || matches!(tool, Tool::Text | Tool::Lasso | Tool::Rectangle))
            {
                self.page().ordered_objects().rev().find_map(|o| {
                    if let Object::Text(t) = o.as_ref() {
                        t.transform
                            .inverse()
                            .filter(|inv| t.rect.contains(inv.apply(p)))
                            .map(|_| t.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            if let Some(id) = text_hit {
                if self.session().selection.len() == 1 && self.session().selection.contains(&id) {
                    self.text_click = Some((id, p));
                }
                self.session_mut().selection = HashSet::from([id]);
                self.tool = Tool::Lasso;
                self.temporary_selection = false;
                self.interaction = Some(Interaction::Move { start: p, end: p });
                return;
            }
            let dismiss_selection =
                self.temporary_selection && tool == Tool::Lasso && !selection_hit;
            if dismiss_selection {
                self.session_mut().selection.clear();
                self.tool = Tool::Pen;
                self.temporary_selection = false;
                tool = Tool::Pen;
            }
            self.interaction = match tool {
                Tool::Pen | Tool::Shape => {
                    let mut b = StrokeBuilder::new(self.style.clone());
                    b.push(event.sample(p));
                    Some(Interaction::Ink {
                        builder: b,
                        last_move: Instant::now(),
                        anchor: p,
                        fit_attempted: false,
                        preview: None,
                        suppress_tap: dismiss_selection,
                    })
                }
                Tool::Eraser => Some(Interaction::Erase {
                    ids: HashSet::new(),
                    last: p,
                    fragments: HashMap::new(),
                }),
                Tool::Lasso | Tool::Rectangle if selection_hit => {
                    Some(Interaction::Move { start: p, end: p })
                }
                Tool::Lasso => {
                    self.session_mut().selection.clear();
                    Some(Interaction::Lasso { points: vec![p] })
                }
                Tool::Rectangle => {
                    self.session_mut().selection.clear();
                    Some(Interaction::Rectangle { start: p, end: p })
                }
                Tool::Text => {
                    self.pending_text = Some(p);
                    None
                }
                Tool::Hand => None,
            };
        }
        match event.phase {
            Phase::Down | Phase::Move => {
                if event.phase == Phase::Move
                    && self.text_click.is_some_and(|(_, start)| {
                        start.distance(p) > 4. / self.session().viewport.zoom
                    })
                {
                    self.text_click = None;
                }
                let p = if self.page().properties.infinite {
                    p
                } else {
                    Point::new(
                        p.x.clamp(0., self.page().properties.width),
                        p.y.clamp(0., self.page().properties.height),
                    )
                };
                let eraser_radius = (self.style.width * 2.).max(10.);
                let page = self.page();
                let candidates = if let Some(Interaction::Erase { last, .. }) = &self.interaction {
                    self.session()
                        .index
                        .query(Rect::from_points([*last, p]).expand(eraser_radius))
                } else {
                    HashSet::new()
                };
                let hits = candidates
                    .iter()
                    .filter(|id| {
                        page.objects.get(id).is_some_and(|o| match o.as_ref() {
                            Object::Stroke(s) => {
                                if let Some(Interaction::Erase { last, .. }) = &self.interaction {
                                    folio_ink::swept_hit(s, *last, p, eraser_radius)
                                } else {
                                    false
                                }
                            }
                            _ => o.bounds().expand(eraser_radius).contains(p),
                        })
                    })
                    .copied()
                    .collect::<Vec<_>>();
                let viewport_zoom = self.session().viewport.zoom;
                if let Some(active) = self.interaction.as_mut() {
                    active.update_ink_endpoint(p, viewport_zoom);
                    match active {
                        Interaction::Ink { builder, .. } => {
                            if event.phase == Phase::Move {
                                builder.push(event.sample(p));
                            }
                        }
                        Interaction::Erase {
                            ids,
                            last,
                            fragments,
                        } => {
                            if self.settings.segment_eraser {
                                for id in &hits {
                                    if let Some(Object::Stroke(stroke)) = self.sessions
                                        [&self.active]
                                        .page()
                                        .objects
                                        .get(id)
                                        .map(|o| o.as_ref())
                                    {
                                        let parts = fragments
                                            .remove(id)
                                            .unwrap_or_else(|| vec![stroke.clone()]);
                                        fragments.insert(
                                            *id,
                                            parts
                                                .iter()
                                                .flat_map(|part| {
                                                    folio_ink::cut_segment(
                                                        part,
                                                        *last,
                                                        p,
                                                        eraser_radius,
                                                    )
                                                })
                                                .collect(),
                                        );
                                    }
                                }
                            }
                            ids.extend(hits);
                            *last = p;
                        }
                        Interaction::Lasso { points } => {
                            if points.last().is_none_or(|last| last.distance(p) > 1.) {
                                points.push(p)
                            }
                        }
                        Interaction::Rectangle { end, .. }
                        | Interaction::Move { end, .. }
                        | Interaction::Resize { end, .. }
                        | Interaction::Rotate { end, .. } => *end = p,
                        Interaction::Pan { start, pan } => {
                            let new =
                                Point::new(pan.x + screen.x - start.x, pan.y + screen.y - start.y);
                            self.session_mut().viewport.pan = new;
                        }
                    }
                }
            }
            Phase::Up => {
                if let Some((id, start)) = self.text_click.take()
                    && start.distance(p) <= 4. / self.session().viewport.zoom
                    && matches!(self.interaction, Some(Interaction::Move { .. }))
                {
                    self.interaction = None;
                    self.pending_text_edit = Some(id);
                    return;
                }
                let viewport_zoom = self.session().viewport.zoom;
                if let Some(interaction) = &mut self.interaction {
                    interaction.update_ink_endpoint(p, viewport_zoom);
                    match interaction {
                        Interaction::Ink { builder, .. } => {
                            if builder
                                .raw()
                                .last()
                                .is_some_and(|last| last.position().distance(p) > 0.12)
                            {
                                builder.push(event.sample(p));
                            }
                        }
                        Interaction::Lasso { points } => {
                            if points.last().is_none_or(|last| last.distance(p) > 1.) {
                                points.push(p);
                            }
                        }
                        Interaction::Move { end, .. }
                        | Interaction::Resize { end, .. }
                        | Interaction::Rotate { end, .. }
                        | Interaction::Rectangle { end, .. } => *end = p,
                        _ => {}
                    }
                }
                self.finish();
            }
            Phase::Leave => {
                if self.interaction.is_some() {
                    self.finish()
                }
                self.cursor = None
            }
            Phase::Cancel => self.cancel(),
            Phase::Hover => {}
        }
    }
    pub fn finish(&mut self) {
        let Some(active) = self.interaction.take() else {
            return;
        };
        match active {
            Interaction::Ink {
                builder,
                preview,
                last_move,
                suppress_tap,
                ..
            } => {
                if suppress_tap {
                    self.persistence
                        .draft(self.active, self.page().id, builder.id(), None);
                    return;
                }
                if let Some(stroke) = builder.finish() {
                    let points = stroke.raw.iter().map(|p| p.position()).collect::<Vec<_>>();
                    let smart_pen =
                        self.tool == Tool::Pen && stroke.style.tool != InkTool::Highlighter;
                    if smart_pen
                        && self.settings.scratch_erase
                        && let Some(gesture) = folio_gestures::scratch(&stroke.raw)
                    {
                        let mut ids = self
                            .session()
                            .index
                            .query(gesture.bounds.expand(3.))
                            .into_iter()
                            .filter(|id| {
                                self.page()
                                    .objects
                                    .get(id)
                                    .is_some_and(|o| gesture.erases(o))
                            })
                            .collect::<HashSet<_>>();
                        let crossed = ids
                            .iter()
                            .filter_map(|id| self.page().objects.get(id))
                            .filter(|o| matches!(o.as_ref(), Object::Stroke(s) if s.style.tool != InkTool::Highlighter))
                            .map(|o| o.bounds())
                            .collect::<Vec<_>>();
                        for body in &crossed {
                            for id in self
                                .session()
                                .index
                                .query(body.expand(body.height() * 0.45))
                            {
                                if self.page().objects.get(&id).is_some_and(|o| {
                                    gesture.completes_letter(o, std::slice::from_ref(body))
                                }) {
                                    ids.insert(id);
                                }
                            }
                        }
                        if !ids.is_empty() {
                            self.persistence
                                .draft(self.active, self.page().id, stroke.id, None);
                            self.delete_ids(&ids, "Scratch erase");
                            return;
                        }
                    }
                    if smart_pen
                        && self.settings.encircle_select
                        && last_move.elapsed() >= INK_HOLD_DELAY
                    {
                        let ids = self.encircled_ids(&points);
                        if !ids.is_empty() {
                            self.session_mut().selection = ids;
                            self.tool = Tool::Lasso;
                            self.temporary_selection = true;
                            self.persistence
                                .draft(self.active, self.page().id, stroke.id, None);
                            return;
                        }
                    }
                    let shape_intent = self.tool == Tool::Shape
                        || preview.is_some()
                        || smart_pen
                            && self.settings.hold_shapes
                            && last_move.elapsed() >= INK_HOLD_DELAY;
                    let mut fit = preview.or_else(|| {
                        if self.tool == Tool::Shape
                            || smart_pen
                                && self.settings.hold_shapes
                                && last_move.elapsed() >= INK_HOLD_DELAY
                        {
                            folio_shapes::fit(&points)
                        } else {
                            None
                        }
                    });
                    let joined = if shape_intent
                        && fit
                            .as_ref()
                            .is_none_or(|f| matches!(f.kind, ShapeKind::Line | ShapeKind::Polyline))
                    {
                        self.joined_arrow(&stroke)
                    } else {
                        None
                    };
                    if let Some(joined) = &joined {
                        fit = Some(joined.fit.clone());
                    }
                    let id = stroke.id;
                    let index = self.page().order.len();
                    let page = self.page().id;
                    let mut changes = vec![Change::Object {
                        page,
                        id,
                        before: None,
                        after: Some(Arc::new(Object::Stroke(stroke.clone()))),
                        index,
                    }];
                    let mut sources = vec![id];
                    if let Some(joined) = joined {
                        sources.splice(0..0, joined.sources);
                        if matches!(joined.previous.as_ref(), Object::Shape(_)) {
                            let previous_id = joined.previous.id();
                            changes.push(Change::Object {
                                page,
                                id: previous_id,
                                before: Some(joined.previous),
                                after: None,
                                index: self.session().order_positions[&previous_id],
                            });
                        }
                    }
                    if let Some(fit) = fit {
                        let shape = Shape {
                            id: Id::new_v4(),
                            kind: fit.kind,
                            vertices: fit.vertices,
                            style: stroke.style,
                            transform: Transform::default(),
                            source_strokes: sources,
                        };
                        changes.push(Change::Object {
                            page,
                            id: shape.id,
                            before: None,
                            after: Some(Arc::new(Object::Shape(shape))),
                            index: index + 1,
                        });
                    }
                    self.commit("Draw stroke", changes);
                }
            }
            Interaction::Erase { ids, fragments, .. } => {
                if !self.settings.segment_eraser {
                    self.delete_ids(&ids, "Erase strokes");
                } else {
                    let page = self.page();
                    let mut changes = vec![];
                    for id in &ids {
                        if let Some(before) = page.objects.get(id) {
                            let index = page.order.iter().position(|i| i == id).unwrap_or(0);
                            changes.push(Change::Object {
                                page: page.id,
                                id: *id,
                                before: Some(before.clone()),
                                after: None,
                                index,
                            });
                            if let Some(parts) = fragments.get(id) {
                                for (offset, part) in parts.iter().enumerate() {
                                    changes.push(Change::Object {
                                        page: page.id,
                                        id: part.id,
                                        before: None,
                                        after: Some(Arc::new(Object::Stroke(part.clone()))),
                                        index: index + offset,
                                    });
                                }
                            }
                        }
                    }
                    self.commit("Erase segments", changes);
                }
            }
            Interaction::Lasso { points } => self.select_polygon(&points),
            Interaction::Rectangle { start, end } => {
                let r = Rect::from_points([start, end]);
                let hidden = self.page().hidden_sources();
                let ids = self
                    .session()
                    .index
                    .query(r)
                    .into_iter()
                    .filter(|id| {
                        !hidden.contains(id)
                            && self
                                .page()
                                .objects
                                .get(id)
                                .is_some_and(|o| r.intersects(o.bounds()))
                    })
                    .collect();
                self.session_mut().selection = ids;
            }
            Interaction::Move { start, end } => self.transform_selection(
                Transform::translate(end.x - start.x, end.y - start.y),
                "Move selection",
            ),
            Interaction::Resize { anchor, start, end } => self.resize_selection(
                Self::resize_transform(anchor, start, end),
                "Resize selection",
            ),
            Interaction::Rotate { center, start, end } => {
                let angle = (end.y - center.y).atan2(end.x - center.x)
                    - (start.y - center.y).atan2(start.x - center.x);
                self.transform_selection(Transform::around(center, 1., angle), "Rotate selection");
            }
            Interaction::Pan { .. } => {}
        }
    }
    fn encircled_ids(&self, points: &[Point]) -> HashSet<Id> {
        let Some(polygon) = folio_gestures::selection_loop(points) else {
            return HashSet::new();
        };
        self.session()
            .index
            .query(Rect::from_points(polygon.iter().copied()).expand(2.))
            .into_iter()
            .filter(|id| {
                self.page()
                    .objects
                    .get(id)
                    .is_some_and(|o| folio_gestures::encloses(&polygon, o))
            })
            .collect()
    }
    fn select_polygon(&mut self, polygon: &[Point]) {
        let bounds = Rect::from_points(polygon.iter().copied());
        let page = self.page();
        let hidden = page.hidden_sources();
        let ids = self
            .session()
            .index
            .query(bounds)
            .into_iter()
            .filter(|id| {
                !hidden.contains(id)
                    && page.objects.get(id).is_some_and(|o| {
                        let r = o.bounds();
                        folio_ink::inside_polygon(r.center(), polygon)
                            || [r.min, r.max]
                                .iter()
                                .any(|p| folio_ink::inside_polygon(*p, polygon))
                    })
            })
            .collect();
        self.session_mut().selection = ids;
    }
    fn resize_transform(anchor: Point, start: Point, end: Point) -> Transform {
        let ratio = |original: f32, current: f32| {
            if original.abs() < 0.001 {
                1.
            } else {
                (current / original).clamp(0.05, 20.)
            }
        };
        let sx = ratio(start.x - anchor.x, end.x - anchor.x);
        let sy = ratio(start.y - anchor.y, end.y - anchor.y);
        Transform::translate(anchor.x, anchor.y)
            .compose(Transform {
                a: sx,
                d: sy,
                ..Transform::default()
            })
            .compose(Transform::translate(-anchor.x, -anchor.y))
    }
    pub fn interaction_transform(&self) -> Transform {
        match &self.interaction {
            Some(Interaction::Move { start, end }) => {
                Transform::translate(end.x - start.x, end.y - start.y)
            }
            Some(Interaction::Resize { anchor, start, end }) => {
                Self::resize_transform(*anchor, *start, *end)
            }
            Some(Interaction::Rotate { center, start, end }) => Transform::around(
                *center,
                1.,
                (end.y - center.y).atan2(end.x - center.x)
                    - (start.y - center.y).atan2(start.x - center.x),
            ),
            _ => Transform::default(),
        }
    }
    pub fn selection_bounds(&self) -> Option<Rect> {
        self.session()
            .selection
            .iter()
            .filter_map(|id| self.page().objects.get(id))
            .map(|o| o.bounds())
            .reduce(|a, b| a.union(b))
    }
    pub fn select_all(&mut self) {
        let hidden = self.page().hidden_sources();
        let ids = self
            .page()
            .order
            .iter()
            .copied()
            .filter(|id| !hidden.contains(id))
            .collect();
        self.session_mut().selection = ids;
        self.tool = Tool::Lasso;
        self.temporary_selection = false;
    }
    fn object_changes(
        &self,
        ids: &HashSet<Id>,
        mut change: impl FnMut(&Object) -> Option<Object>,
    ) -> Vec<Change> {
        let page = self.page();
        page.order
            .iter()
            .enumerate()
            .filter(|(_, id)| ids.contains(id))
            .filter_map(|(index, id)| {
                let before = page.objects.get(id)?.clone();
                let after = change(&before).map(Arc::new);
                Some(Change::Object {
                    page: page.id,
                    id: *id,
                    before: Some(before),
                    after,
                    index,
                })
            })
            .collect()
    }
    fn delete_ids(&mut self, ids: &HashSet<Id>, label: &str) {
        let mut ids = ids.clone();
        for id in ids.clone() {
            if let Some(o) = self.page().objects.get(&id) {
                match o.as_ref() {
                    Object::Shape(s) => ids.extend(&s.source_strokes),
                    Object::Equation(e) => ids.extend(&e.source_strokes),
                    _ => {}
                }
            }
        }
        let changes = self.object_changes(&ids, |_| None);
        self.commit(label, changes);
    }
    pub fn delete_selection(&mut self) {
        let ids = self.session().selection.clone();
        self.delete_ids(&ids, "Delete selection");
        self.session_mut().selection.clear();
    }
    pub fn transform_selection(&mut self, t: Transform, label: &str) {
        if t == Transform::default() {
            return;
        }
        let mut ids = self.session().selection.clone();
        for id in ids.clone() {
            if let Some(o) = self.page().objects.get(&id) {
                match o.as_ref() {
                    Object::Shape(s) => ids.extend(&s.source_strokes),
                    Object::Equation(e) => ids.extend(&e.source_strokes),
                    _ => {}
                }
            }
        }
        let changes = self.object_changes(&ids, |o| {
            let mut o = o.clone();
            o.set_transform(t.compose(o.transform()));
            Some(o)
        });
        self.commit(label, changes);
    }
    fn resize_selection(&mut self, resize: Transform, label: &str) {
        if resize == Transform::default() {
            return;
        }
        let mut ids = self.session().selection.clone();
        for id in ids.clone() {
            if let Some(o) = self.page().objects.get(&id) {
                match o.as_ref() {
                    Object::Shape(s) => ids.extend(&s.source_strokes),
                    Object::Equation(e) => ids.extend(&e.source_strokes),
                    _ => {}
                }
            }
        }
        let changes = self.object_changes(&ids, |object| {
            let mut object = object.clone();
            if let Object::Text(text) = &mut object {
                text.reflow(resize);
            } else {
                object.set_transform(resize.compose(object.transform()));
            }
            Some(object)
        });
        self.commit(label, changes);
    }
    pub fn preview_text(&self, text: &TextBlock) -> TextBlock {
        let mut text = text.clone();
        if self.session().selection.contains(&text.id) {
            if matches!(self.interaction, Some(Interaction::Resize { .. })) {
                text.reflow(self.interaction_transform());
            } else {
                text.transform = self.interaction_transform().compose(text.transform);
            }
        }
        text
    }
    pub fn scale_selection(&mut self, scale: f32) {
        if let Some(r) = self.selection_bounds() {
            self.resize_selection(Transform::around(r.center(), scale, 0.), "Resize selection")
        }
    }
    pub fn rotate_selection(&mut self, angle: f32) {
        if let Some(r) = self.selection_bounds() {
            self.transform_selection(Transform::around(r.center(), 1., angle), "Rotate selection")
        }
    }
    pub fn restyle_selection(&mut self) {
        let style = self.style.clone();
        let changes = self.object_changes(&self.session().selection, |o| {
            let mut o = o.clone();
            if let Some(s) = o.style_mut() {
                *s = style.clone();
                if let Object::Stroke(s) = &mut o {
                    folio_ink::rebuild(s)
                }
            }
            Some(o)
        });
        self.commit("Change ink style", changes);
    }
    pub fn refine_selection(&mut self) {
        let changes = self.object_changes(&self.session().selection, |o| {
            let mut o = o.clone();
            if let Object::Stroke(s) = &mut o {
                if s.refinement_enabled {
                    s.refinement_enabled = false
                } else {
                    folio_ink::refine(s)
                }
            }
            Some(o)
        });
        self.commit("Refine handwriting", changes);
    }
    pub fn crop_selection(&mut self, crop: Option<Rect>) {
        let changes = self.object_changes(&self.session().selection, |object| {
            let mut object = object.clone();
            if let Object::Image(image) = &mut object {
                image.crop = crop;
            }
            Some(object)
        });
        self.commit("Crop image", changes);
    }
    pub fn copy_objects(&self) -> Vec<Object> {
        let mut ids = self.session().selection.clone();
        for id in ids.clone() {
            if let Some(o) = self.page().objects.get(&id) {
                match o.as_ref() {
                    Object::Shape(s) => ids.extend(&s.source_strokes),
                    Object::Equation(e) => ids.extend(&e.source_strokes),
                    _ => {}
                }
            }
        }
        self.page()
            .ordered_objects()
            .filter(|o| ids.contains(&o.id()))
            .map(|o| o.as_ref().clone())
            .collect()
    }
    pub fn paste_objects(&mut self, mut objects: Vec<Object>) {
        let map: HashMap<Id, Id> = objects.iter().map(|o| (o.id(), Id::new_v4())).collect();
        let mut changes = vec![];
        let index = self.page().order.len();
        let mut ids = HashSet::new();
        for (i, o) in objects.iter_mut().enumerate() {
            o.set_id(map[&o.id()]);
            o.set_transform(Transform::translate(24., 24.).compose(o.transform()));
            match o {
                Object::Shape(s) => {
                    s.source_strokes = s
                        .source_strokes
                        .iter()
                        .filter_map(|id| map.get(id).copied())
                        .collect()
                }
                Object::Equation(e) => {
                    e.source_strokes = e
                        .source_strokes
                        .iter()
                        .filter_map(|id| map.get(id).copied())
                        .collect();
                    if let Some(link) = &mut e.math_link {
                        link.sources = link
                            .sources
                            .iter()
                            .filter_map(|id| map.get(id).copied())
                            .collect();
                        link.ink_region = if link.sources.is_empty() {
                            None
                        } else {
                            link.ink_region.map(|r| {
                                Rect::new(r.min.x + 24., r.min.y + 24., r.width(), r.height())
                            })
                        };
                    }
                }
                _ => {}
            }
            ids.insert(o.id());
            changes.push(Change::Object {
                page: self.page().id,
                id: o.id(),
                before: None,
                after: Some(Arc::new(o.clone())),
                index: index + i,
            });
        }
        self.commit("Paste objects", changes);
        self.session_mut().selection = ids;
        self.tool = Tool::Lasso;
        self.temporary_selection = false;
    }
    pub fn add_text(&mut self, text: String, position: Point) {
        if !text.is_empty() {
            self.insert_text_box(text, position);
        }
    }
    pub fn create_text_box(&mut self, position: Point) -> Id {
        if self.read_only() {
            return Id::nil();
        }
        self.insert_text_box(String::new(), position)
    }
    fn insert_text_box(&mut self, text: String, position: Point) -> Id {
        let object = Object::Text(TextBlock {
            id: Id::new_v4(),
            text,
            rect: Rect::new(position.x, position.y, 360., 160.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 20.,
            color: self.style.color,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        });
        let id = object.id();
        self.commit(
            "Insert text",
            vec![Change::Object {
                page: self.page().id,
                id,
                before: None,
                after: Some(Arc::new(object)),
                index: self.page().order.len(),
            }],
        );
        self.session_mut().selection = HashSet::from([id]);
        self.pending_text = None;
        id
    }
    pub fn edit_text(&mut self, id: Id, change: impl FnOnce(&mut TextBlock)) {
        let Some(before) = self.page().objects.get(&id).cloned() else {
            return;
        };
        let mut after = before.as_ref().clone();
        if let Object::Text(t) = &mut after {
            change(t);
            if &after == before.as_ref() {
                return;
            }
            let index = self.page().order.iter().position(|v| *v == id).unwrap_or(0);
            self.commit(
                "Edit text",
                vec![Change::Object {
                    page: self.page().id,
                    id,
                    before: Some(before),
                    after: Some(Arc::new(after)),
                    index,
                }],
            );
        }
    }
    pub fn insert_space(&mut self, at: f32, amount: f32, vertical: bool) {
        let ids = self
            .page()
            .ordered_objects()
            .filter(|o| {
                if vertical {
                    o.bounds().min.y >= at
                } else {
                    o.bounds().min.x >= at
                }
            })
            .map(|o| o.id())
            .collect::<HashSet<_>>();
        let t = if vertical {
            Transform::translate(0., amount)
        } else {
            Transform::translate(amount, 0.)
        };
        let changes = self.object_changes(&ids, |o| {
            let mut o = o.clone();
            o.set_transform(t.compose(o.transform()));
            Some(o)
        });
        self.commit("Insert handwriting space", changes);
    }
    fn submit(&mut self, job: Job) {
        match self.workers.submit(job) {
            Ok(()) => self.busy += 1,
            Err(e) => self.error = Some(e),
        }
    }
    pub fn cancel_equation_render(&mut self) {
        self.equation_generation += 1;
        self.equation_pending = false;
        self.equation_result = None;
        self.equation_live_wait = None;
    }
    fn begin_equation_render(&mut self) {
        self.cancel_equation_render();
        self.equation_pending = true;
    }
    fn submit_equation(&mut self, job: Job) {
        match self.workers.submit(job) {
            Ok(()) => self.busy += 1,
            Err(error) => {
                self.equation_pending = false;
                self.equation_result = Some(Err(error));
            }
        }
    }
    pub fn edit_equation(&mut self, id: Id, latex: String) {
        if self.read_only() {
            self.equation_result = Some(Err("Restore this document before editing".into()));
            return;
        }
        self.begin_equation_render();
        if self
            .page()
            .objects
            .get(&id)
            .is_some_and(|o| matches!(o.as_ref(), Object::Equation(e) if e.math_link.is_some()))
        {
            self.equation_live_wait = Some((id, latex.clone()));
            if let Err(error) = self.update_math_expression(id, latex) {
                self.equation_pending = false;
                self.equation_result = Some(Err(error));
            }
            return;
        }
        if let Some(object) = self.page().objects.get(&id).cloned()
            && let Object::Equation(e) = object.as_ref()
        {
            self.submit_equation(Job::Equation {
                generation: self.equation_generation,
                note: self.active,
                page: self.page().id,
                existing: Some(object.clone()),
                latex,
                rect: e.rect,
            });
        } else {
            self.equation_pending = false;
            self.equation_result = Some(Err("This equation is no longer available".into()));
        }
    }
    pub fn insert_equation(&mut self, latex: String) {
        if self.read_only() {
            self.equation_result = Some(Err("Restore this document before editing".into()));
            return;
        }
        self.begin_equation_render();
        let position = self.cursor.unwrap_or(Point::new(100., 100.));
        self.submit_equation(Job::Equation {
            generation: self.equation_generation,
            existing: None,
            note: self.active,
            page: self.page().id,
            latex,
            rect: Rect::new(position.x, position.y, 240., 70.),
        });
    }
    pub fn search(&mut self, query: String) {
        self.search_query = query.clone();
        self.search_generation += 1;
        if self.saved >= self.queued && self.dirty_notes.is_empty() {
            self.submit(Job::Search {
                query,
                database: self.database.clone(),
                generation: self.search_generation,
            });
        } else {
            self.pending_search = Some((query, self.search_generation));
        }
    }
    pub fn navigate_search(&mut self, note: Id, page: Id) {
        self.switch_note(note);
        self.pending_navigation = Some((note, page));
        if self.sessions.contains_key(&note) {
            let s = self.sessions.get_mut(&note).unwrap();
            s.page = s
                .document
                .pages
                .iter()
                .position(|p| p.id == page)
                .unwrap_or(0);
            s.refresh();
            let query = self.search_query.clone();
            let mut highlights: Vec<_> = s
                .page()
                .ordered_objects()
                .filter(|o| {
                    folio_search::matches_text(o.searchable_text(), &query)
                        && !o.searchable_text().is_empty()
                })
                .map(|o| o.bounds())
                .collect();
            highlights.extend(
                s.page()
                    .ink_text
                    .iter()
                    .filter(|entry| folio_search::matches_text(&entry.text, &query))
                    .map(|entry| entry.bounds),
            );
            if let Some(r) = highlights.first() {
                s.viewport.pan = Point::new(
                    100. - r.min.x * s.viewport.zoom,
                    100. - r.min.y * s.viewport.zoom,
                );
            }
            self.search_highlights = highlights;
            self.pending_navigation = None;
        }
    }
    pub fn prepare_export(&mut self) -> ExportSnapshot {
        self.finish();
        ExportSnapshot {
            document: self.session().document.clone(),
            page: self.page().id,
        }
    }
    pub fn export_prepared(&mut self, snapshot: ExportSnapshot, path: PathBuf, kind: ExportKind) {
        let extension = match kind {
            ExportKind::Notebook => "folio",
            ExportKind::Pdf => "pdf",
            ExportKind::Svg => "svg",
            ExportKind::Png => "png",
            ExportKind::Text => "txt",
        };
        if let Err(error) = self.validate_export_destination(&path, extension) {
            self.error = Some(error);
            return;
        }
        self.submit(Job::Export {
            doc: snapshot.document,
            page: snapshot.page,
            assets: self.assets.clone(),
            path,
            kind,
        });
    }
    pub fn export(&mut self, path: PathBuf, kind: ExportKind) {
        let snapshot = self.prepare_export();
        self.export_prepared(snapshot, path, kind);
    }
    pub fn import(&mut self, path: PathBuf) {
        self.import_into(self.active, path);
    }
    /// Home imports have their own document; editor imports retain their captured target.
    pub fn import_as_note(&mut self, path: PathBuf) -> Id {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported document")
            .to_owned();
        let properties = PageProperties {
            paper: self.settings.paper,
            ..Default::default()
        };
        let id = self
            .create_note_with_properties_inner(title, properties, false)
            .expect("Valid import placeholder");
        self.library_imports.insert(
            id,
            LibraryImport {
                path: path.clone(),
                pending: true,
                error: None,
            },
        );
        self.import_into(id, path);
        id
    }
    pub fn retry_library_import(&mut self, note: Id) {
        let Some(import) = self.library_imports.get_mut(&note) else {
            return;
        };
        if import.pending {
            return;
        }
        import.pending = true;
        import.error = None;
        let path = import.path.clone();
        self.import_into(note, path);
    }
    pub fn import_is_provisional(&self, note: Id) -> bool {
        self.provisional_imports.contains(&note)
    }
    pub fn dismiss_failed_import(&mut self, note: Id) {
        if !self
            .library_imports
            .get(&note)
            .is_some_and(|import| !import.pending && import.error.is_some())
        {
            return;
        }
        self.library_imports.remove(&note);
        if self.provisional_imports.remove(&note) {
            self.notes.retain(|n| n.id != note);
            self.sessions.remove(&note);
            if note == self.active {
                if let Some(id) = self.notes.iter().find(|n| !n.trashed).map(|n| n.id) {
                    self.switch_note(id);
                } else {
                    self.create_note();
                }
            }
            self.status = "Failed import removed".into();
        } else {
            self.status = "Document kept; failed import dismissed".into();
        }
    }
    fn library_import_failed(&mut self, note: Id, message: String) {
        self.import_finished(note);
        if let Some(import) = self.library_imports.get_mut(&note) {
            import.pending = false;
            import.error = Some(message);
        } else {
            self.error = Some(message);
        }
    }

    pub fn import_into(&mut self, note: Id, path: PathBuf) {
        self.manage_note(note, NoteAction::Import(path));
    }
    fn import_loaded(&mut self, note: Id, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let job = if ext == "folio" {
            Job::ImportNotebook {
                note,
                path,
                assets: self.assets.clone(),
            }
        } else if ext == "pdf" {
            Job::ImportPdf {
                note,
                path,
                assets: self.assets.clone(),
                password: None,
            }
        } else {
            Job::ImportImage {
                note,
                page: self.sessions[&note].page().id,
                path,
                assets: self.assets.clone(),
                position: Point::new(80., 80.),
            }
        };
        if let Err(e) = self.workers.submit(job) {
            self.library_import_failed(note, e);
        } else {
            self.busy += 1;
            *self.pending_imports.entry(note).or_default() += 1;
        }
    }
    fn import_finished(&mut self, note: Id) {
        if let Some(count) = self.pending_imports.get_mut(&note) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.pending_imports.remove(&note);
            }
        }
    }
    fn apply_pdf(&mut self, note: Id, pages: Vec<Page>) {
        self.apply_pages(note, pages, "Import PDF", None);
    }
    fn apply_pages(
        &mut self,
        note: Id,
        pages: Vec<Page>,
        label: &str,
        metadata: Option<NoteMetadata>,
    ) {
        if pages.is_empty() {
            return;
        }
        let Some(s) = self.sessions.get(&note) else {
            return;
        };
        // Replace only an untouched placeholder. Never replace a page with annotations.
        let placeholder = s.document.pages.len() == 1
            && s.document.pages[0].objects.is_empty()
            && s.document.pages[0].properties.pdf.is_none();
        let index = if placeholder {
            0
        } else {
            s.document.pages.len()
        };
        let mut changes = Vec::new();
        let mut pages = pages.into_iter();
        if placeholder {
            changes.push(Change::Page {
                index: 0,
                before: Some(s.document.pages[0].clone()),
                after: pages.next(),
            });
        }
        changes.extend(pages.enumerate().map(|(i, page)| Change::Page {
            index: index + i + usize::from(placeholder),
            before: None,
            after: Some(page),
        }));
        if let Some(metadata) = metadata {
            let before = self.sessions[&note].document.metadata.clone();
            let mut after = before.clone();
            after.title = metadata.title;
            after.tags = metadata.tags;
            after.cover_page = metadata.cover_page;
            changes.push(Change::Metadata { before, after });
        }
        self.commit_to(note, label, changes);
        if note == self.active {
            self.change_page(index);
            self.session_mut().viewport = Viewport::default();
        }
    }
    fn trim_caches(&mut self) {
        if self.session_recency.back() != Some(&self.active) {
            self.session_recency.retain(|id| *id != self.active);
            self.session_recency.push_back(self.active);
        }
        if self.settings.autosave
            && self.saved >= self.queued
            && self.dirty_notes.is_empty()
            && self.interaction.is_none()
        {
            while self.sessions.len() > 8 {
                let victim = self.session_recency.iter().copied().find(|id| {
                    *id != self.active
                        && Some(*id) != self.pending_note
                        && !self.pending_actions.contains_key(id)
                        && !self.pending_imports.contains_key(id)
                        && self
                            .sessions
                            .get(id)
                            .is_some_and(|s| s.pending_journal.is_empty())
                });
                let Some(victim) = victim else {
                    break;
                };
                self.session_recency.retain(|id| *id != victim);
                self.sessions.remove(&victim);
                self.previews.retain(|(note, _, _), _| *note != victim);
            }
        }
        const BUDGET: usize = 128 * 1024 * 1024;
        if self.previews.values().map(|p| p.bgra.len()).sum::<usize>() > BUDGET {
            let note = self.active;
            let page = self.page().id;
            self.previews
                .retain(|(n, p, _), _| *n == note && *p == page);
            let mut bytes = self.previews.values().map(|p| p.bgra.len()).sum::<usize>();
            if bytes > BUDGET {
                let visible = self
                    .session()
                    .index
                    .query(self.session().viewport.visible(1920., 1200.));
                self.previews.retain(|(_, _, id), p| {
                    let keep = visible.contains(id) || bytes <= BUDGET;
                    if !keep {
                        bytes = bytes.saturating_sub(p.bgra.len());
                    }
                    keep
                });
            }
        }
        let mut bytes = self.previews.values().map(|p| p.bgra.len()).sum::<usize>();
        if bytes > BUDGET {
            let mut candidates = self
                .previews
                .iter()
                .map(|(key, p)| (*key, p.pixels_budget))
                .collect::<Vec<_>>();
            candidates.sort_unstable_by_key(|(_, budget)| std::cmp::Reverse(*budget));
            for (key, _) in candidates {
                if bytes <= BUDGET {
                    break;
                }
                if let Some(preview) = self.previews.remove(&key) {
                    bytes = bytes.saturating_sub(preview.bgra.len());
                }
            }
        }
    }
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        changed |= self.poll_recognition();
        changed |= self.poll_math_solver();
        if self.pending_pdf_password.is_none() {
            self.pending_pdf_password = self.pdf_password_queue.pop_front();
            changed |= self.pending_pdf_password.is_some();
        }
        while let Ok(receipt) = self.persistence.receipts.try_recv() {
            changed = true;
            let note = self.save_notes.remove(&receipt.sequence);
            match receipt.result {
                Ok(()) => {
                    self.saved = self.saved.max(receipt.sequence);
                    if self.saved >= self.queued
                        && self.dirty_notes.is_empty()
                        && self
                            .sessions
                            .values()
                            .all(|session| session.pending_journal.is_empty())
                    {
                        self.save_error = None;
                        self.status = "All changes saved".into()
                    }
                }
                Err(e) => {
                    if let Some(note) = note {
                        self.dirty_notes.insert(note);
                    }
                    self.status = "Changes are not saved".into();
                    self.save_error = Some(e)
                }
            }
        }
        if self.settings.autosave
            && !self.dirty_notes.is_empty()
            && self.last_retry.elapsed() > Duration::from_secs(2)
            && self.interaction.is_none()
        {
            self.last_retry = Instant::now();
            let notes = self.dirty_notes.iter().copied().collect::<Vec<_>>();
            for note in notes {
                if let Some(s) = self.sessions.get(&note) {
                    self.persist(Delta::full(&s.document));
                }
            }
        }
        while let Ok(result) = self.workers.results.try_recv() {
            changed = true;
            self.busy = self.busy.saturating_sub(1);
            match result {
                Finished::RecognizedEquation {
                    generation,
                    review,
                    result,
                } => {
                    self.finish_recognized_equation(generation, review, result);
                }
                Finished::LibraryPreview {
                    note,
                    updated,
                    result,
                } => {
                    self.library_preview_pending.remove(&note);
                    if self
                        .notes
                        .iter()
                        .any(|n| n.id == note && n.updated_at == updated)
                    {
                        match result {
                            Ok(Some((page, count))) => {
                                self.cache_library_preview(note, updated, page, count)
                            }
                            Ok(None) => {}
                            Err(_) => {
                                self.library_preview_failed.insert(note, updated);
                            }
                        }
                    }
                }
                Finished::Cancelled => {}
                Finished::Cleaned(count) => {
                    self.status = format!(
                        "Quarantined {count} unused assets; originals can be restored from orphaned-assets"
                    )
                }
                Finished::EquationError {
                    generation,
                    message,
                } => {
                    if generation == self.equation_generation {
                        self.equation_pending = false;
                        self.equation_result = Some(Err(message));
                    }
                }
                Finished::Equation {
                    generation,
                    note,
                    page,
                    before,
                    after,
                } => {
                    if generation != self.equation_generation {
                        continue;
                    }
                    self.equation_pending = false;
                    self.equation_result =
                        Some(Err("The destination page is no longer available".into()));
                    if let Some(p) = self.sessions.get(&note).and_then(|s| s.document.page(page)) {
                        let id = after.id();
                        let current = p.objects.get(&id).cloned();
                        if current == before {
                            let index = p
                                .order
                                .iter()
                                .position(|v| *v == id)
                                .unwrap_or(p.order.len());
                            self.commit_to(
                                note,
                                "Edit equation",
                                vec![Change::Object {
                                    page,
                                    id,
                                    before,
                                    after: Some(Arc::new(after)),
                                    index,
                                }],
                            );
                            self.equation_result = Some(Ok(()));
                        } else {
                            self.equation_result =
                                Some(Err("Equation changed while rendering; edit it again".into()));
                        }
                    }
                }
                Finished::PreviewError {
                    note,
                    page,
                    object,
                    message,
                } => {
                    let key = (note, page, object.id());
                    self.preview_pending.remove(&key);
                    let current = self
                        .sessions
                        .get(&note)
                        .and_then(|s| s.document.page(page))
                        .and_then(|p| p.objects.get(&object.id()))
                        .is_some_and(|current| Arc::ptr_eq(current, &object));
                    if current {
                        self.preview_failed.insert(key, object);
                        self.preview_errors.insert((note, page), message);
                    }
                }
                Finished::PdfPreviewError { asset, message } => {
                    self.pdf_preview_pending.remove(&asset);
                    for (&note, session) in &self.sessions {
                        for page in &session.document.pages {
                            if page.properties.pdf.as_ref().is_some_and(|p| {
                                p.preview_asset.as_ref().is_some_and(|a| {
                                    a == &asset || format!("{a}.scroll.png") == asset
                                })
                            }) {
                                self.preview_errors.insert((note, page.id), message.clone());
                            }
                        }
                    }
                    self.pdf_preview_failed.insert(asset);
                }
                Finished::PdfPreview(asset) => {
                    self.pdf_preview_pending.remove(&asset);
                }
                Finished::PasswordRequired(note, path) => {
                    if self.pending_pdf_password.is_none() {
                        self.pending_pdf_password = Some((note, path));
                    } else {
                        self.pdf_password_queue.push_back((note, path));
                    }
                }
                Finished::Preview {
                    pixels_budget,
                    note,
                    page,
                    object,
                    bounds,
                    width,
                    height,
                    bgra,
                } => {
                    let key = (note, page, object.id());
                    self.preview_pending.remove(&key);
                    self.accept_preview(
                        note,
                        page,
                        RasterPreview {
                            pixels_budget,
                            object,
                            bounds,
                            width,
                            height,
                            bgra: Arc::new(bgra),
                        },
                    );
                }

                Finished::Search {
                    generation,
                    results,
                } => {
                    if generation == self.search_generation {
                        self.search_results = results
                    }
                }
                Finished::Loaded(d, history) => {
                    let id = d.metadata.id;
                    self.loading_notes.remove(&id);
                    self.session_recency.retain(|n| *n != id);
                    self.session_recency.push_back(id);
                    self.sessions.entry(id).or_insert_with(|| {
                        let mut s = Session::new(d);
                        s.history = history;
                        s
                    });
                    if let Some(actions) = self.pending_actions.remove(&id) {
                        for action in actions {
                            self.manage_note(id, action);
                        }
                    }
                    if self.pending_note == Some(id) {
                        self.finish();
                        self.active = id;
                        if self.read_only() {
                            self.tool = Tool::Hand;
                        }
                        self.pending_note = None;
                        if let Some((note, page)) = self.pending_navigation.take() {
                            self.navigate_search(note, page);
                        }
                    }
                }
                Finished::TemplateSaved(template) => {
                    self.settings.templates.push(template);
                    self.store_settings();
                    self.status = "Page template saved".into();
                }
                Finished::TemplatePage { note, page } => {
                    self.import_finished(note);
                    self.library_imports.remove(&note);
                    if let Some(session) = self.sessions.get(&note) {
                        let index = session.document.pages.len();
                        self.commit_to(
                            note,
                            "Add template page",
                            vec![Change::Page {
                                index,
                                before: None,
                                after: Some(page),
                            }],
                        );
                        if note == self.active {
                            self.change_page(index);
                        }
                    }
                }
                Finished::Restored(path) => {
                    self.status = format!("Backup restored to {}", path.display());
                    self.restored_library = Some(path);
                }
                Finished::NotebookImported { note, document } => {
                    self.import_finished(note);
                    self.library_imports.remove(&note);
                    let empty = self.sessions.get(&note).is_some_and(|s| {
                        s.document.pages.len() == 1
                            && s.document.pages[0].objects.is_empty()
                            && s.document.pages[0].properties.pdf.is_none()
                    });
                    self.apply_pages(
                        note,
                        document.pages,
                        "Import editable notebook",
                        empty.then_some(document.metadata),
                    );
                }
                Finished::Exported(path) => {
                    self.status = format!(
                        "Exported {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )
                }
                Finished::LoadError { id, message } => {
                    self.loading_notes.remove(&id);
                    self.pending_actions.remove(&id);
                    if self.pending_note == Some(id) {
                        self.pending_note = None;
                    }
                    self.error = Some(message);
                }
                Finished::ImportError { note, message } => {
                    self.library_import_failed(note, message);
                }
                Finished::Pdf { note, pages } => {
                    self.library_imports.remove(&note);
                    self.apply_pdf(note, pages);
                    self.import_finished(note);
                }
                Finished::Image { note, page, object } => {
                    self.import_finished(note);
                    self.library_imports.remove(&note);
                    if let Some(p) = self.sessions.get(&note).and_then(|s| s.document.page(page)) {
                        let index = p.order.len();
                        let id = object.id();
                        self.commit_to(
                            note,
                            "Insert object",
                            vec![Change::Object {
                                page,
                                id,
                                before: None,
                                after: Some(Arc::new(object)),
                                index,
                            }],
                        );
                    }
                }
                Finished::Error(e) => self.error = Some(e),
            }
        }
        if let Some((query, generation)) = self.pending_search.take() {
            if self.saved >= self.queued || !self.settings.autosave {
                self.submit(Job::Search {
                    query,
                    database: self.database.clone(),
                    generation,
                });
                changed = true;
            } else {
                self.pending_search = Some((query, generation));
            }
        }
        if self.settings.autosave
            && !self.provisional_imports.contains(&self.active)
            && self.last_draft.elapsed() > Duration::from_secs(1)
        {
            if let Some(Interaction::Ink {
                builder,
                suppress_tap: false,
                ..
            }) = &self.interaction
                && let Some(stroke) = builder.snapshot()
            {
                self.persistence.draft(
                    self.active,
                    self.page().id,
                    stroke.id,
                    Some(Arc::new(Object::Stroke(stroke))),
                );
            }
            self.last_draft = Instant::now();
        }
        let smart_pen = self.tool == Tool::Pen && self.style.tool != InkTool::Highlighter;
        if (self.settings.hold_shapes && self.tool == Tool::Shape
            || smart_pen && (self.settings.hold_shapes || self.settings.encircle_select))
            && let Some(Interaction::Ink {
                builder,
                last_move,
                fit_attempted,
                suppress_tap: false,
                ..
            }) = &self.interaction
            && !*fit_attempted
            && last_move.elapsed() >= INK_HOLD_DELAY
        {
            let points = builder
                .raw()
                .iter()
                .map(|p| p.position())
                .collect::<Vec<_>>();
            if smart_pen && self.settings.encircle_select && !self.encircled_ids(&points).is_empty()
            {
                self.finish();
                changed = true;
            } else if let Some(Interaction::Ink {
                preview,
                fit_attempted,
                ..
            }) = &mut self.interaction
            {
                *fit_attempted = true;
                if self.settings.hold_shapes {
                    *preview = folio_shapes::fit(&points);
                    changed |= preview.is_some();
                }
            }
        }
        if self.last_cache_trim.elapsed() >= Duration::from_millis(250) {
            self.trim_caches();
            self.last_cache_trim = Instant::now();
        }
        changed
    }
    pub fn recovery_checkpoint(&mut self) {
        self.save();
        self.persistence.checkpoint();
        self.status = "Creating recovery snapshot…".into();
    }
    pub fn cleanup_assets(&mut self) {
        if self.busy > 0
            || self.saved < self.queued
            || !self.dirty_notes.is_empty()
            || self.interaction.is_some()
            || !self.settings.autosave
        {
            self.error =
                Some("Wait for saved changes and background work before asset maintenance".into());
            return;
        }
        self.submit(Job::Cleanup {
            root: self.data_dir.clone(),
        });
    }
    pub fn page_preview_error(&self) -> Option<&str> {
        self.preview_errors
            .get(&(self.active, self.page().id))
            .map(String::as_str)
    }
    pub fn retry_save(&mut self) {
        let notes = self.dirty_notes.iter().copied().collect::<Vec<_>>();
        let autosave = self.settings.autosave;
        self.settings.autosave = true;
        for note in notes {
            if let Some(session) = self.sessions.get(&note) {
                self.persist(Delta::full(&session.document));
            }
        }
        self.settings.autosave = autosave;
    }
    pub fn retry_previews(&mut self) {
        self.preview_errors.clear();
        self.preview_failed.clear();
        self.pdf_preview_failed.clear();
    }
    pub fn cancel_pdf_import(&mut self, note: Id) {
        self.library_import_failed(note, "PDF import cancelled".into());
    }
    pub fn unlock_pdf(&mut self, note: Id, path: PathBuf, password: String) {
        self.submit(Job::ImportPdf {
            note,
            path,
            assets: self.assets.clone(),
            password: Some(password),
        });
    }
    pub fn request_pdf_preview(&mut self) {
        let Some(background) = self.page().properties.pdf.clone() else {
            return;
        };
        self.request_pdf_background(background);
    }
    pub fn request_pdf_background(&mut self, background: PdfBackground) {
        self.request_pdf_background_scaled(background, 2400);
    }
    pub fn pdf_scroll_preview_asset(background: &PdfBackground) -> Option<String> {
        background
            .preview_asset
            .as_ref()
            .map(|asset| format!("{asset}.scroll.png"))
    }
    pub fn request_pdf_scroll_preview(&mut self, mut background: PdfBackground) {
        if background
            .preview_asset
            .as_ref()
            .is_some_and(|asset| self.assets.join(asset).is_file())
        {
            return;
        }
        background.preview_asset = Self::pdf_scroll_preview_asset(&background);
        self.request_pdf_background_scaled(background, 960);
    }
    fn request_pdf_background_scaled(&mut self, background: PdfBackground, size: u32) {
        let Some(asset) = &background.preview_asset else {
            return;
        };
        if self.assets.join(asset).is_file()
            || self.pdf_preview_pending.contains(asset)
            || self.pdf_preview_failed.contains(asset)
        {
            return;
        }
        if self
            .workers
            .submit(Job::PdfPreview {
                background: background.clone(),
                assets: self.assets.clone(),
                size,
            })
            .is_ok()
        {
            self.pdf_preview_pending.insert(asset.clone());
            self.busy += 1;
        }
    }
    fn accept_preview(&mut self, note: Id, page: Id, preview: RasterPreview) -> bool {
        let id = preview.object.id();
        let current = self
            .sessions
            .get(&note)
            .and_then(|s| s.document.page(page))
            .and_then(|p| p.objects.get(&id))
            .is_some_and(|current| Arc::ptr_eq(current, &preview.object));
        if !current {
            return false;
        }
        if let Some(session) = self.sessions.get_mut(&note)
            && session.page().id == page
        {
            session.index.insert(id, preview.bounds);
        }
        self.previews.insert((note, page, id), preview);
        true
    }
    pub fn request_previews(&mut self, visible: &HashSet<Id>) {
        self.request_page_previews(self.session().page, visible);
    }
    pub fn request_page_previews(&mut self, index: usize, visible: &HashSet<Id>) {
        let Some(page) = self.session().document.pages.get(index) else {
            return;
        };
        let objects = &page.objects;
        let page = page.id;
        let objects = visible
            .iter()
            .filter_map(|id| objects.get(id))
            .filter(|o| !matches!(o.as_ref(), Object::Stroke(_) | Object::Shape(_)))
            .cloned()
            .collect::<Vec<_>>();
        let maximum =
            (24 * 1024 * 1024_u32 / (objects.len().max(1) as u32)).clamp(1, 4 * 1024 * 1024);
        let pixels_budget = 1 << (31 - maximum.leading_zeros());
        for object in objects {
            let key = (self.active, page, object.id());
            if self.previews.get(&key).is_some_and(|p| {
                Arc::ptr_eq(&p.object, &object) && p.pixels_budget >= pixels_budget
            }) || self.preview_pending.contains_key(&key)
                || self
                    .preview_failed
                    .get(&key)
                    .is_some_and(|failed| Arc::ptr_eq(failed, &object))
            {
                continue;
            }
            if self
                .workers
                .submit(Job::Preview {
                    pixels_budget,
                    note: self.active,
                    page,
                    object: object.clone(),
                    assets: self.assets.clone(),
                })
                .is_ok()
            {
                self.preview_pending.insert(key, object);
                self.busy += 1;
            } else {
                break;
            }
        }
    }
    pub fn encode_clipboard(&self) -> Option<String> {
        let objects = self.copy_objects();
        if objects.is_empty() {
            None
        } else {
            serde_json::to_string(&objects)
                .ok()
                .map(|s| format!("FOLIO-OBJECTS-V2\n{s}"))
        }
    }
    pub fn paste_text_or_objects(&mut self, text: String) {
        if let Some(data) = text
            .strip_prefix("FOLIO-OBJECTS-V2\n")
            .or_else(|| text.strip_prefix("FOLIO-OBJECTS-V1\n"))
        {
            match serde_json::from_str::<Vec<Object>>(data) {
                Ok(objects) => {
                    let mut page = Page::new();
                    for o in &objects {
                        page.order.push(o.id());
                        page.objects.insert(o.id(), Arc::new(o.clone()));
                    }
                    let mut doc = Document::new("clipboard");
                    doc.pages = vec![page];
                    if let Err(e) = doc.validate() {
                        self.error = Some(e)
                    } else {
                        self.paste_objects(objects)
                    }
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        } else {
            self.add_text(text, self.cursor.unwrap_or(Point::new(100., 100.)));
        }
    }
    pub fn paste_image(&mut self, data: &[u8], extension: &str) {
        let path =
            self.data_dir
                .join("jobs")
                .join(format!("clipboard-{}.{}", Id::new_v4(), extension));
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, data) {
            Ok(()) => self.import(path),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn asset_path(&self, name: &str) -> Option<PathBuf> {
        folio_export::asset_path(&self.assets, name).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_input::{Device, Tool as PenTool};
    fn app() -> Controller {
        Controller::open(std::env::temp_dir().join(format!("folio-app-{}", Id::new_v4()))).unwrap()
    }
    fn event(x: f32, y: f32, phase: Phase) -> PenEvent {
        PenEvent {
            device: Device::Tablet,
            tool: PenTool::Pen,
            phase,
            position: Point::new(x + 56., y + 36.),
            pressure: 0.75,
            tilt_x: 14.,
            tilt_y: -8.,
            buttons: 0,
            timestamp: (x * 8.) as u64,
        }
    }
    fn draw(a: &mut Controller) {
        a.pointer(event(20., 30., Phase::Down));
        for x in 21..100 {
            a.pointer(event(x as f32, 30., Phase::Move));
        }
        a.pointer(event(100., 30., Phase::Up));
    }
    #[test]
    fn draw_edit_undo_save_reload() {
        let mut a = app();
        draw(&mut a);
        let s = match a.page().ordered_objects().next().unwrap().as_ref() {
            Object::Stroke(s) => s.clone(),
            _ => panic!(),
        };
        assert_eq!(s.raw[0].tilt_x, 14.);
        a.select_all();
        a.transform_selection(Transform::translate(10., 40.), "move");
        assert_eq!(a.page().objects[&s.id].transform().tx, 10.);
        a.undo();
        assert_eq!(a.page().objects[&s.id].transform(), Transform::default());
        a.redo();
        a.flush().unwrap();
        let id = a.active;
        let dir = a.data_dir.clone();
        drop(a);
        let a = Controller::open(dir).unwrap();
        assert_eq!(a.active, id);
        assert_eq!(a.page().objects[&s.id].transform().ty, 40.);
    }
    #[test]
    fn erasing_is_one_command_and_cancel_is_safe() {
        let mut a = app();
        draw(&mut a);
        a.set_tool(Tool::Eraser);
        a.pointer(event(50., 30., Phase::Down));
        assert!(matches!(&a.interaction,Some(Interaction::Erase {ids,..})if !ids.is_empty()));
        a.cancel();
        assert_eq!(a.page().objects.len(), 1);
        a.pointer(event(50., 30., Phase::Down));
        a.pointer(event(50., 30., Phase::Up));
        assert!(a.page().objects.is_empty());
        a.undo();
        assert_eq!(a.page().objects.len(), 1);
    }
    #[test]
    fn page_operations_and_note_ids() {
        let mut a = app();
        let first = a.active;
        a.add_page();
        assert_eq!(a.session().document.pages.len(), 2);
        a.delete_page();
        assert_eq!(a.session().document.pages.len(), 1);
        a.undo();
        assert_eq!(a.session().document.pages.len(), 2);
        a.create_note();
        assert_ne!(first, a.active);
        a.switch_note(first);
        assert_eq!(a.session().document.pages.len(), 2);
    }
    #[test]
    fn clipboard_has_independent_raw_and_ids() {
        let mut a = app();
        draw(&mut a);
        a.select_all();
        let objects = a.copy_objects();
        let id = objects[0].id();
        a.paste_objects(objects);
        assert_eq!(a.page().objects.len(), 2);
        assert!(!a.session().selection.contains(&id));
        a.undo();
        assert_eq!(a.page().objects.len(), 1);
    }
    fn wait(a: &mut Controller) {
        let start = Instant::now();
        while a.has_background_work() {
            a.tick();
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(a.error.is_none(), "{:?}", a.error);
    }
    #[test]
    fn batched_delete_undo_restores_original_z_order() {
        let mut a = app();
        for i in 0..5 {
            a.add_text(i.to_string(), Point::new(20., i as f32 * 20.));
        }
        let original = a.page().order.clone();
        a.session_mut().selection = HashSet::from([original[1], original[3]]);
        a.delete_selection();
        a.undo();
        assert_eq!(a.page().order, original);
        a.redo();
        assert_eq!(a.page().order, vec![original[0], original[2], original[4]]);
    }
    #[test]
    fn save_with_autosave_off_removes_previously_saved_objects() {
        let mut a = app();
        draw(&mut a);
        a.flush().unwrap();
        a.settings.autosave = false;
        a.select_all();
        a.delete_selection();
        a.flush().unwrap();
        let d = Store::open(&a.database)
            .unwrap()
            .load(a.active)
            .unwrap()
            .unwrap();
        assert!(d.pages[0].objects.is_empty());
    }
    #[test]
    fn long_active_stroke_recovers_and_cancel_removes_draft() {
        let mut a = app();
        a.pointer(event(20., 30., Phase::Down));
        a.pointer(event(22., 31., Phase::Move));
        a.last_draft = Instant::now() - Duration::from_secs(2);
        a.tick();
        a.persistence.flush().unwrap();
        let store = Store::open(&a.database).unwrap();
        let d = store.load(a.active).unwrap().unwrap();
        assert_eq!(d.pages[0].objects.len(), 1);
        a.cancel();
        a.persistence.flush().unwrap();
        assert!(
            store.load(a.active).unwrap().unwrap().pages[0]
                .objects
                .is_empty()
        );
    }
    #[test]
    fn search_waits_for_new_text_to_be_durably_indexed() {
        let mut a = app();
        a.add_text("Freshly written heliotrope".into(), Point::new(10., 20.));
        a.search("heliotrope".into());
        wait(&mut a);
        assert_eq!(a.search_results.len(), 1);
        let page = a.page().id;
        a.navigate_search(a.active, page);
        assert!(!a.search_highlights.is_empty());
    }
    #[test]
    fn converted_shapes_keep_raw_sources_through_copy_and_undo() {
        let mut a = app();
        a.set_tool(Tool::Shape);
        draw(&mut a);
        assert_eq!(a.page().objects.len(), 2);
        let original = a
            .page()
            .objects
            .values()
            .find_map(|o| {
                if let Object::Stroke(s) = o.as_ref() {
                    Some(s.raw.clone())
                } else {
                    None
                }
            })
            .unwrap();
        a.select_all();
        assert_eq!(a.session().selection.len(), 1);
        let copy = a.copy_objects();
        a.paste_objects(copy);
        assert_eq!(a.page().objects.len(), 4);
        assert_eq!(a.page().hidden_sources().len(), 2);
        a.undo();
        assert_eq!(a.page().objects.len(), 2);
        let raw = a
            .page()
            .objects
            .values()
            .find_map(|o| {
                if let Object::Stroke(s) = o.as_ref() {
                    Some(&s.raw)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(raw, &original);
    }
    #[test]
    fn only_one_controller_can_write_a_data_directory() {
        let a = app();
        assert!(Controller::open(a.data_dir.clone()).is_err());
    }
    #[test]
    fn enabling_autosave_catches_up_all_pending_edits() {
        let mut a = app();
        a.set_autosave(false);
        draw(&mut a);
        a.add_text("pending text".into(), Point::new(40., 50.));
        a.set_autosave(true);
        a.persistence.flush().unwrap();
        let d = Store::open(&a.database)
            .unwrap()
            .load(a.active)
            .unwrap()
            .unwrap();
        assert_eq!(d.pages[0].objects.len(), 2);
    }
    #[test]
    fn local_equations_render_off_thread_and_are_undoable() {
        let mut a = app();
        a.insert_equation(r"E=mc^2".into());
        wait(&mut a);
        assert!(
            a.page()
                .ordered_objects()
                .any(|o| matches!(o.as_ref(),Object::Equation(e)if e.rendered_svg.is_some()))
        );
        a.undo();
        assert!(a.page().objects.is_empty());
    }
    fn with_unloaded_note() -> (Controller, Id) {
        let mut a = app();
        a.create_note();
        a.rename("Other note".into());
        a.flush().unwrap();
        let path = a.data_dir.clone();
        drop(a);
        let a = Controller::open(path).unwrap();
        let id = a.notes.iter().find(|n| n.id != a.active).unwrap().id;
        (a, id)
    }
    #[test]
    fn late_load_cannot_replace_a_new_note_or_overwrite_edits() {
        let (mut a, target) = with_unloaded_note();
        for _ in 0..3 {
            a.switch_note(target);
        }
        a.create_note();
        let created = a.active;
        a.add_text("Keep this edit".into(), Point::new(50., 50.));
        wait(&mut a);
        assert_eq!(a.active, created);
        assert_eq!(a.page().objects.len(), 1);
    }
    #[test]
    fn input_during_a_tab_load_cannot_edit_either_document() {
        let (mut a, target) = with_unloaded_note();
        let original = a.active;
        a.switch_note(target);
        a.pointer(event(30., 40., Phase::Down));
        a.pointer(event(35., 43., Phase::Move));
        wait(&mut a);
        assert_eq!(a.active, target);
        assert!(a.page().objects.is_empty());
        assert!(a.sessions[&original].document.pages[0].objects.is_empty());
        assert!(!a.loading_note());
        a.pointer(event(30., 40., Phase::Down));
        a.pointer(event(35., 43., Phase::Up));
        assert_eq!(a.page().objects.len(), 1);
    }
}
#[cfg(test)]
mod upgrade_tests {
    use super::*;
    fn root() -> PathBuf {
        std::env::temp_dir().join(format!("folio-upgrade-{}", Id::new_v4()))
    }
    #[test]
    fn undo_and_redo_survive_reopening_with_autosave_disabled() {
        let root = root();
        let mut a = Controller::open(root.clone()).unwrap();
        a.set_autosave(false);
        a.add_text("first".into(), Point::new(20., 20.));
        a.add_text("second".into(), Point::new(20., 60.));
        a.undo();
        a.flush().unwrap();
        drop(a);
        let mut a = Controller::open(root).unwrap();
        assert_eq!(a.page().objects.len(), 1);
        assert!(a.session().history.can_redo());
        a.redo();
        assert_eq!(a.page().objects.len(), 2);
        a.undo();
        a.undo();
        assert!(a.page().objects.is_empty());
    }
}

#[cfg(test)]
mod extended_tests {
    use super::*;
    #[test]
    fn resize_and_rotation_handles_create_reversible_commands() {
        let mut a = app();
        let id = insert(&mut a, ink());
        a.set_tool(Tool::Rectangle);
        a.session_mut().selection.insert(id);
        let before = a.page().objects[&id].clone();
        let bounds = before.bounds();
        let event = |position: Point, phase| PenEvent {
            device: folio_input::Device::Tablet,
            tool: folio_input::Tool::Pen,
            phase,
            position: Point::new(position.x + 56., position.y + 36.),
            pressure: 0.7,
            tilt_x: 0.,
            tilt_y: 0.,
            buttons: 0,
            timestamp: 1000,
        };
        let start = Point::new(bounds.max.x, bounds.min.y);
        let end = Point::new(start.x + bounds.width(), start.y - 10.);
        a.pointer(event(start, Phase::Down));
        assert!(matches!(a.interaction, Some(Interaction::Resize { .. })));
        a.pointer(event(end, Phase::Move));
        a.pointer(event(end, Phase::Up));
        assert!(a.page().objects[&id].bounds().width() > bounds.width() * 1.9);
        a.undo();
        assert_eq!(a.page().objects[&id], before);
        a.session_mut().selection.insert(id);
        let handle = Point::new(bounds.center().x, bounds.min.y - 24.);
        let end = Point::new(bounds.center().x + 30., bounds.center().y);
        a.pointer(event(handle, Phase::Down));
        assert!(matches!(a.interaction, Some(Interaction::Rotate { .. })));
        a.pointer(event(end, Phase::Move));
        a.pointer(event(end, Phase::Up));
        assert!(a.page().objects[&id].bounds().height() > bounds.width() * 0.9);
        a.undo();
        assert_eq!(a.page().objects[&id], before);
        let Object::Stroke(original) = before.as_ref() else {
            unreachable!()
        };
        let Object::Stroke(restored) = a.page().objects[&id].as_ref() else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(&restored.raw, &original.raw));
        assert!(Arc::ptr_eq(&restored.path, &original.path));
    }
    fn ink() -> InkStroke {
        let mut b = StrokeBuilder::new(PenStyle::default());
        for i in 0..80 {
            b.push(StrokePoint::new(
                Point::new(100. + i as f32 * 2., 100.),
                0.2 + i as f32 * 0.008,
                i * 8,
            ));
        }
        b.finish().unwrap()
    }
    fn app() -> Controller {
        Controller::open(std::env::temp_dir().join(format!("folio-extended-{}", Id::new_v4())))
            .unwrap()
    }
    fn insert(a: &mut Controller, s: InkStroke) -> Id {
        let id = s.id;
        let page = a.page().id;
        a.commit(
            "Ink",
            vec![Change::Object {
                page,
                id,
                before: None,
                after: Some(Arc::new(Object::Stroke(s))),
                index: 0,
            }],
        );
        id
    }
    #[test]
    fn undo_and_redo_survive_reopening() {
        let mut a = app();
        let root = a.data_dir.clone();
        a.add_text("Durable undo".into(), Point::new(100., 100.));
        a.undo();
        a.flush().unwrap();
        drop(a);
        let mut a = Controller::open(root.clone()).unwrap();
        assert!(a.session().history.can_redo());
        assert!(a.page().objects.is_empty());
        a.redo();
        assert_eq!(a.page().text(), "Durable undo");
        a.flush().unwrap();
        drop(a);
        let mut a = Controller::open(root.clone()).unwrap();
        assert!(a.session().history.can_undo());
        a.undo();
        assert!(a.page().objects.is_empty());
        drop(a);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn segment_erase_is_atomic_and_preserves_original_sensor_data() {
        let mut a = app();
        let original = ink();
        insert(&mut a, original.clone());
        a.settings.segment_eraser = true;
        a.set_tool(Tool::Eraser);
        let event = |phase| PenEvent {
            device: folio_input::Device::Tablet,
            tool: folio_input::Tool::Eraser,
            phase,
            position: Point::new(231., 136.),
            pressure: 0.8,
            tilt_x: 0.,
            tilt_y: 0.,
            buttons: 0,
            timestamp: 1000,
        };
        a.pointer(event(Phase::Down));
        a.pointer(event(Phase::Up));
        assert_eq!(a.page().objects.len(), 2);
        for object in a.page().objects.values() {
            let Object::Stroke(s) = object.as_ref() else {
                panic!("Expected fragments")
            };
            assert_eq!(s.raw, original.raw);
            assert!(s.fragment_path.is_some());
        }
        a.undo();
        assert_eq!(a.page().objects.len(), 1);
        assert_eq!(
            a.page().objects[&original.id].as_ref(),
            &Object::Stroke(original)
        );
    }
}
