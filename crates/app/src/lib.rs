//! Application controller. Commands, input routing and worker scheduling live here;
//! the GPUI layer only displays state and forwards user actions.
//!
//! [`Controller`] is the single owner of application state. Its methods are
//! split across modules by concern:
//!
//! - `session`: per-document editor state ([`Session`]) and its derived indexes
//! - `history`: committing commands, undo/redo and persistence
//! - `notes`, `folders`, `library_actions`: documents and the library
//! - `page_actions`: pages, paper and page geometry
//! - `interaction`, `selection`, `clipboard`, `text_boxes`: editing input
//! - `background`: worker jobs and the per-frame `tick`
//! - `import_export`, `previews`, `memory`: files, raster previews and budgets
//! - `recognition`, `math_solver`, `equations`: OCR, solving and LaTeX
pub mod appearance;
mod background;
mod clipboard;
mod equations;
mod folders;
#[cfg(test)]
mod gesture_tests;
mod handwriting_search;
mod history;
mod import_export;
mod interaction;
mod library_actions;
mod library_previews;
#[cfg(test)]
mod management_tests;
mod math_solver;
mod memory;
mod notes;
#[cfg(test)]
mod optimization_tests;
mod page_actions;
mod pen_presets;
pub mod portable;
mod previews;
mod recognition;
mod search;
mod selection;
mod session;
mod settings;
#[cfg(test)]
mod shape_tests;
mod shape_tools;
mod starter;
mod templates;
#[cfg(test)]
mod tests;
mod text_boxes;
mod workers;
pub use folders::FolderDetails;
use folio_canvas::{PageStack, SpatialIndex, Viewport};
use folio_document::*;
use folio_ink::StrokeBuilder;
use folio_input::{PenEvent, Phase, Tool as PenTool};
pub use folio_math::{VectorCommand, VectorFormula};
use folio_storage::{Delta, JournalEvent, Persistence, Store};
pub use history::SaveState;
pub use import_export::ExportSnapshot;
pub use interaction::Interaction;
pub use library_actions::NoteAction;
pub use math_solver::{MathReport, MathRequest, MathSession, MathStep};
pub use previews::RasterPreview;
pub use recognition::{RecognitionKind, RecognitionReview, RecognitionTiming};
pub use session::Session;
pub use settings::{EraserMode, PageTemplate, PenPreset, Settings, WorkspacePreferences};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
pub use workers::ExportKind;
use workers::*;
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
    NotebookTrash(Id),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SearchState {
    #[default]
    Idle,
    Searching,
    Complete,
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct LibraryImport {
    pub path: PathBuf,
    pub pending: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Complete,
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct BackgroundTask {
    pub id: Id,
    pub kind: &'static str,
    pub label: String,
    pub state: TaskState,
}
/// Owner of all application state. The UI reads its public fields and calls
/// its methods; nothing else mutates documents.
pub struct Controller {
    // Library: data locations, documents, folders and their cover previews.
    pub data_dir: PathBuf,
    pub assets: PathBuf,
    pub database: PathBuf,
    pub notes: Vec<NoteMetadata>,
    pub notebooks: Vec<Notebook>,
    pub filter: NoteFilter,
    folder_details: RefCell<Option<folders::DetailsCache>>,
    /// A restored backup the user can switch to.
    pub restored_library: Option<PathBuf>,
    library_previews: HashMap<Id, (u64, Arc<Page>, usize)>,
    library_preview_pending: HashSet<Id>,
    library_preview_failed: HashMap<Id, u64>,
    library_preview_recency: VecDeque<Id>,

    // Open documents. `active` always has a session; others may be released
    // under memory pressure and reloaded from storage.
    pub sessions: HashMap<Id, Session>,
    pub active: Id,
    session_recency: VecDeque<Id>,
    /// Page and viewport of released sessions, restored when they reload.
    session_views: VecDeque<(Id, Id, Viewport)>,
    /// Tabs the user closed; released once their saves complete.
    closed_sessions: HashSet<Id>,
    /// Note the user switched to while it is still loading.
    pending_note: Option<Id>,
    loading_notes: HashSet<Id>,
    /// Search result to show once `pending_note` has loaded.
    pending_navigation: Option<(Id, Id)>,
    /// Library actions waiting for their note to load.
    pending_actions: HashMap<Id, Vec<NoteAction>>,
    /// Recent library organization changes, newest last.
    library_undo: Vec<library_actions::LibraryUndo>,
    /// Empty document shown before the user writes anything; it is not in
    /// the library until it receives its first edit.
    bootstrap_document: Option<Id>,

    // Editing state for the active document.
    pub tool: Tool,
    pub style: PenStyle,
    pub settings: Settings,
    pub interaction: Option<Interaction>,
    pub cursor: Option<Point>,
    /// The lasso was selected by a gesture or Select All and reverts to
    /// `selection_return_tool` afterwards.
    temporary_selection: bool,
    selection_return_tool: Tool,
    /// Second click on a selected text box, which opens its editor on release.
    text_click: Option<(Id, Point)>,
    /// Text tool press waiting for the UI to open an editor at this point.
    pub pending_text: Option<Point>,
    /// Text box the UI should open for editing.
    pub pending_text_edit: Option<Id>,
    last_draft: Instant,

    // User-visible status.
    pub status: String,
    pub error: Option<String>,
    pub save_error: Option<String>,
    /// Number of submitted worker jobs that have not reported back.
    pub busy: usize,
    pub tasks: Vec<BackgroundTask>,

    // Save tracking. `queued`/`saved` are persistence sequence numbers.
    queued: u64,
    saved: u64,
    /// Notes whose last save failed and must be saved in full.
    dirty_notes: HashSet<Id>,
    save_notes: HashMap<u64, Id>,
    last_retry: Instant,

    // Imports.
    pending_imports: HashMap<Id, usize>,
    pub library_imports: HashMap<Id, LibraryImport>,
    /// Notes not yet written to storage (imports in progress, bootstrap).
    provisional_imports: HashSet<Id>,
    pub pending_pdf_password: Option<(Id, PathBuf)>,
    pdf_password_queue: VecDeque<(Id, PathBuf)>,

    // Raster previews, keyed by (note, page, object).
    pub previews: HashMap<(Id, Id, Id), RasterPreview>,
    preview_pending: HashMap<(Id, Id, Id), Arc<Object>>,
    preview_failed: HashMap<(Id, Id, Id), Arc<Object>>,
    preview_errors: HashMap<(Id, Id), String>,
    pdf_preview_pending: HashSet<String>,
    pdf_preview_failed: HashSet<String>,
    last_cache_trim: Instant,

    // Search. Results from older generations are discarded.
    pending_search: Option<(String, u64)>,
    search_generation: u64,
    pub search_results: Vec<folio_search::SearchResult>,
    pub search_query: String,
    pub search_state: SearchState,
    pub search_highlights: Vec<Rect>,

    // Equation rendering. Results from older generations are discarded.
    equation_generation: u64,
    pub equation_pending: bool,
    pub equation_result: Option<Result<(), String>>,
    equation_live_wait: Option<(Id, String)>,

    // Handwriting recognition (OCR).
    recognition_generation: u64,
    /// The current review indexes handwriting instead of replacing it.
    pub recognition_for_index: bool,
    pub recognition_pending: bool,
    pub recognition_status: String,
    pub recognition_replacing: bool,
    pub recognition_review: Option<RecognitionReview>,
    pub recognition_timing: Option<RecognitionTiming>,

    // Math solver and live math linked to handwriting.
    math_generation: u64,
    pub math_session: Option<MathSession>,
    math_after_ocr: bool,
    math_ocr_next: bool,
    math_live_pending: HashSet<Id>,
    math_live_signatures: HashMap<Id, String>,
    last_math_scan: Instant,
    math_scan_cursor: usize,
    last_math_ink: Instant,
    math_ink_due: Option<Instant>,
    math_live_ocr: Option<(Id, String)>,
    math_ink_signatures: HashMap<Id, String>,

    // Services and workers. Fields drop in order: keep the writer lease last,
    // after persistence and all worker owners are dropped.
    math_service: math_solver::Service,
    recognition_service: recognition::Service,
    persistence: Persistence,
    workers: Workers,
    _lock: folio_platform::DataDirLock,
}
impl Controller {
    pub fn open(data_dir: PathBuf) -> Result<Self, String> {
        Self::open_inner(data_dir, false)
    }
    /// Restore the visible workspace without loading an editor hidden behind the
    /// library. Explicit startup actions can request an editor instead.
    pub fn open_for_ui(data_dir: PathBuf, editor_requested: bool) -> Result<Self, String> {
        Self::open_inner(data_dir, !editor_requested)
    }
    fn open_inner(data_dir: PathBuf, defer_library: bool) -> Result<Self, String> {
        let lock = prepare_data_dir(&data_dir)?;
        let assets = data_dir.join("assets");
        std::fs::create_dir_all(&assets).map_err(|e| e.to_string())?;
        let database = data_dir.join("notes.sqlite3");
        let store = Store::open(&database).map_err(|e| e.to_string())?;
        let settings = load_settings(&store)?;
        let notes = store.list_notes().map_err(|e| e.to_string())?;
        let notebooks = store.notebooks().map_err(|e| e.to_string())?;
        let preferred = settings
            .reopen_documents
            .then_some(settings.workspace.active_document)
            .flatten();
        let mut bootstrap_document = None;
        let load_editor =
            !defer_library || (settings.reopen_documents && !settings.workspace.library_open);
        let (document, history) = if let Some(n) = notes
            .iter()
            .find(|n| Some(n.id) == preferred)
            .or_else(|| notes.iter().find(|n| !n.trashed))
            .filter(|_| load_editor)
        {
            store
                .load_with_history(n.id)
                .map_err(|e| e.to_string())?
                .ok_or("Missing note data")?
        } else {
            let mut doc = Document::new("Untitled document");
            doc.pages[0].properties.paper = settings.paper;
            bootstrap_document = Some(doc.metadata.id);
            (doc, History::default())
        };
        let active = document.metadata.id;
        let mut sessions = HashMap::new();
        let mut session = Session::new(document);
        session.history = history;
        if let Some(page) = settings.remembered_page(active) {
            session.show_page(page);
        }
        let initial_tool = if session.document.metadata.trashed {
            Tool::Hand
        } else {
            Tool::Pen
        };
        sessions.insert(active, session);
        let style = settings.default_pen.clone();
        let mut controller = Self {
            recognition_for_index: false,
            restored_library: None,
            library_previews: HashMap::new(),
            library_preview_pending: HashSet::new(),
            library_preview_failed: HashMap::new(),
            library_preview_recency: VecDeque::new(),
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
            math_ink_due: None,
            math_live_ocr: None,
            math_ink_signatures: HashMap::new(),
            recognition_service: recognition::Service::new(recognition::pack_path(&data_dir)),
            recognition_generation: 0,
            recognition_pending: false,
            recognition_status: String::new(),
            recognition_replacing: false,
            recognition_review: None,
            recognition_timing: None,
            _lock: lock,
            session_recency: VecDeque::from([active]),
            session_views: VecDeque::new(),
            closed_sessions: HashSet::new(),
            last_draft: Instant::now(),
            last_cache_trim: Instant::now(),
            pending_actions: HashMap::new(),
            library_undo: Vec::new(),
            pending_imports: HashMap::new(),
            library_imports: HashMap::new(),
            provisional_imports: bootstrap_document.into_iter().collect(),
            bootstrap_document,
            pdf_password_queue: VecDeque::new(),
            pending_note: None,
            loading_notes: HashSet::new(),
            pending_navigation: None,
            pending_search: None,
            previews: HashMap::new(),
            preview_pending: HashMap::new(),
            notes,
            notebooks,
            folder_details: RefCell::new(None),
            sessions,
            active,
            tool: initial_tool,
            temporary_selection: false,
            selection_return_tool: Tool::Pen,
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
            tasks: Vec::new(),
            pending_text: None,
            pending_text_edit: None,
            text_click: None,
            pending_pdf_password: None,
            pdf_preview_pending: HashSet::new(),
            pdf_preview_failed: HashSet::new(),
            preview_failed: HashMap::new(),
            search_results: vec![],
            search_query: String::new(),
            search_state: SearchState::Idle,
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
        controller.refresh_default_ink();
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
    pub fn read_only(&self) -> bool {
        self.session().document.metadata.trashed
    }
    /// Restart must wait for operations that can still add or change documents.
    pub fn can_restart_for_update(&self) -> bool {
        !self.has_background_work()
            && !self.recognition_pending
            && !self.equation_pending
            && !self.math_session.as_ref().is_some_and(|s| s.pending)
            && self.math_live_pending.is_empty()
            && self.pending_note.is_none()
    }
    pub fn has_background_work(&self) -> bool {
        self.busy > 0 || self.pending_search.is_some()
    }
    /// Work that benefits from frame-rate completion polling. Failed saves and
    /// OCR awaiting a download choice can use the slower idle maintenance tick.
    pub fn needs_fast_poll(&self) -> bool {
        self.interaction.is_some()
            || self.has_background_work()
            || !self.save_notes.is_empty()
            || (self.recognition_pending && !self.recognition_setup_needed())
            || self.equation_pending
            || self.math_session.as_ref().is_some_and(|s| s.pending)
            || !self.math_live_pending.is_empty()
            || self.pending_note.is_some()
    }
    pub(crate) fn end_temporary_selection(&mut self) {
        if self.temporary_selection {
            self.tool = self.selection_return_tool;
            self.selection_return_tool = Tool::Pen;
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
        self.settings
            .theme_default_ink_tools
            .retain(|tool| *tool != self.style.tool);
        self.style.color = color;
        self.settings.recent_colors.retain(|c| *c != color);
        self.settings.recent_colors.insert(0, color);
        self.settings.recent_colors.truncate(8);
        self.store_settings();
    }
    pub fn set_style(&mut self, style: PenStyle) {
        if style == self.style {
            return;
        }
        if style.tool == self.style.tool && style.color != self.style.color {
            self.settings
                .theme_default_ink_tools
                .retain(|tool| *tool != style.tool);
        }
        self.settings
            .tool_styles
            .retain(|s| s.tool != self.style.tool);
        self.settings.tool_styles.push(self.style.clone());
        self.style = style;
        self.store_settings();
    }
    pub fn store_settings(&mut self) {
        self.refresh_default_ink();
        self.settings.default_pen = self.style.clone();
        self.settings
            .tool_styles
            .retain(|s| s.tool != self.style.tool);
        self.settings.tool_styles.push(self.style.clone());
        match serde_json::to_string(&self.settings) {
            Ok(data) => self.persistence.settings(data),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn asset_path(&self, name: &str) -> Option<PathBuf> {
        folio_export::asset_path(&self.assets, name).ok()
    }
}

/// Create the private data directory and take its single-writer lock.
fn prepare_data_dir(data_dir: &std::path::Path) -> Result<folio_platform::DataDirLock, String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(data_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    folio_platform::lock_data_dir(data_dir).map_err(|e| e.to_string())
}
/// Read saved preferences, migrating older profiles.
fn load_settings(store: &Store) -> Result<Settings, String> {
    let mut preferences: Option<serde_json::Value> =
        store.setting("preferences").map_err(|e| e.to_string())?;
    // Profiles with neither a saved pen nor per-theme ink choices get the
    // default per-theme ink tools.
    if let Some(profile) = preferences.as_mut().and_then(|value| value.as_object_mut())
        && !profile.contains_key("default_pen")
        && !profile.contains_key("theme_default_ink_tools")
    {
        profile.insert(
            "theme_default_ink_tools".into(),
            serde_json::to_value(Settings::default().theme_default_ink_tools)
                .map_err(|e| e.to_string())?,
        );
    }
    let mut settings: Settings = preferences
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    settings.normalize();
    Ok(settings)
}
