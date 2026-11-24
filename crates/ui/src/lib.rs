//! Rune presentation and native input adapter. Document operations are delegated
//! to folio-app; ink geometry remains independent of the toolkit.
#[macro_use]
pub mod compat;
mod accessibility;
mod diagnostics;
mod document_actions;
mod executor;
use document_actions::MoreSection;
pub use executor::{AsyncApp, BackgroundExecutor, Task, WeakView};
mod input_check;
mod recovery;
pub use recovery::{RecoveryView, run_recovery};
mod appearance;
mod canvas_menu;
mod color_picker;
mod controls;
mod export_options;
mod field;
mod find;
mod folder_drag;
mod folder_menu;
mod font_picker;
mod frame;
mod graph;
mod help;
mod icons;
mod ink_layers;
mod inline_text;
mod library_sort;
mod math_panel;
mod motion;
mod navigation;
mod notebook_setup;
mod onboarding;
mod ocr_setup;
mod page_size;
mod paint;
mod painting;
pub use paint::*;
mod pen_menu;
mod pen_presets;
mod portable;
mod region;
mod selection;
mod settings;
mod shell;
mod tablet;
mod templates;
mod text_render;
mod theme;
mod thumbnails;
mod titlebar;
mod tool_wheel;
mod toolbar_menu;
mod transitions;
use toolbar_menu::ToolMenu;
mod updates;
mod validation;
mod workspace;
mod workspace_state;
pub use compat::*;
pub use field::{Entity, Field, FieldBoundsChanged, InlineStyle, Navigate, Submitted};
use folio_app::{Controller, ExportKind, Interaction, NoteFilter, RecognitionKind, Tool};
use folio_document::{Color, Id, InkTool, Paper, Point as DocPoint};
use folio_input::{Device, PenEvent, Phase, TimestampExtender, Tool as PenTool};
use icons::{Icon, icon};
pub use painting::CanvasFrame;
pub use shell::{AppState, run};
/// GPUI's app context, which helpers took for reading entities.
pub type App = Context<NotesView>;
use std::time::{Duration, Instant};
use theme::{Glass, Theme};

/// A mouse-wheel scroll or zoom of the canvas still easing toward its target.
#[derive(Default)]
struct CanvasEase {
    /// The view the eased wheel scroll or zoom is heading for.
    target: Option<folio_canvas::Viewport>,
    /// The view the last step left, so a change made elsewhere (a drag, a
    /// pinch) cancels the motion instead of being pulled back.
    left: Option<folio_canvas::Viewport>,
    /// The last frame that advanced the motion, while it is running.
    last: Option<Instant>,
    /// Screen-space anchor of an eased zoom, kept directly so zooming in and
    /// out never shifts the document point under the pointer or center.
    zoom_anchor: Option<DocPoint>,
}

/// A sideways glide of the page: `total` points over [`CanvasGlide::SECONDS`]
/// from `start`, of which `applied` have been moved so far.
#[derive(Clone, Copy)]
struct CanvasGlide {
    total: f32,
    applied: f32,
    start: Instant,
}

impl CanvasGlide {
    const SECONDS: f32 = 0.45;
}

static STARTUP: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
/// Records when the process started, for `FOLIO_PROFILE_STARTUP` marks made
/// after the window opens.
pub fn set_startup(at: Instant) {
    let _ = STARTUP.set(at);
}
pub(crate) fn startup_mark(stage: &str) {
    if let Some(start) = STARTUP.get()
        && std::env::var_os("FOLIO_PROFILE_STARTUP").is_some()
    {
        eprintln!(
            "FOLIO_STARTUP {stage}: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.
        );
    }
}

actions!(
    folio,
    [
        Undo,
        Redo,
        Save,
        NewNote,
        NewPage,
        Search,
        FindNext,
        FindPrevious,
        Copy,
        Cut,
        Paste,
        SelectAll,
        Delete,
        Escape,
        Pen,
        Eraser,
        Lasso,
        Hand,
        Text,
        Shapes,
        ZoomIn,
        ZoomOut,
        FitPage,
        NextPage,
        PreviousPage,
        Library,
        TogglePages,
        Settings,
        Import,
        OpenTab,
        CloseTab,
        NextTab,
        PreviousTab,
        Export,
        FocusNext,
        FocusPrevious,
        ResetTitlebarSmoke,
        ApplyCrop,
        EditText,
        ToggleBold,
        ToggleItalic,
        ToggleUnderline,
        OpenToolWheel
    ]
);
/// A menu a dialog opened from inside it turns into: the menu morphs into
/// the dialog's page instead of a second popover opening.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuHost {
    /// A tab's or library card's context menu.
    Document,
    /// A folder's context menu.
    Folder,
    /// The editor's popover: document actions or the pen menu.
    Popover,
    /// A toolbar dropdown (Insert, ink color).
    Tool,
    /// The selection's actions on the canvas (a recognition's review).
    Selection,
}
/// The height of a row of the document picker, in points.
const PICKER_ROW: f32 = 32.;
/// Where a menu-like popover sits (see `NotesView::popover_frame`).
#[derive(Clone, Copy)]
struct PopoverFrame {
    x: f32,
    y: f32,
    width: f32,
    /// The height it was placed for, and the most it may take.
    height: f32,
    room: f32,
    /// The point it grows from.
    grow: (f32, f32),
}
impl PopoverFrame {
    /// Places `popover` here, growing from the point it opened at.
    fn place(self, popover: Div) -> Div {
        popover
            .absolute()
            .left(px(self.x))
            .top(px(self.y))
            .w(px(self.width))
            .max_h(px(self.room))
            .grows_from_point(self.grow, (self.x, self.y, self.width, self.height))
    }
}
#[derive(Clone)]
enum Modal {
    SaveTemplate,
    Templates,
    RenameTemplate(Id),
    SavePreset,
    RenamePreset(Id),
    PageBookmark,
    MovePage,
    Recognition,
    Rename,
    RenameDocument(Id),
    DocumentTags(Id),
    OpenDocument,
    NewDocument,
    Notebook(Option<Id>),
    RenameNotebook(Id),
    MoveNotebook(Id),
    MoveDocument(Id),
    PdfPassword(Id, std::path::PathBuf),
    Crop,
    Tags,
    Color,
    ThemeColor {
        dark: bool,
        token: folio_app::appearance::ThemeToken,
    },
    CanvasColor,
    PageSize,
    FontSize,
    TextColor,
    Equation,
    EditEquation(Id),
    /// Closing the window with edits that autosave-off has not saved.
    UnsavedChanges,
    Confirm(Confirm),
}
/// Irreversible actions confirmed in a dialog.
#[derive(Clone, Copy)]
enum Confirm {
    DeletePermanently(Id),
    EmptyTrash(Option<Id>),
}
impl Modal {
    /// Whether the dialog edits text in its field.
    fn has_field(&self) -> bool {
        !matches!(
            self,
            Self::MovePage | Self::Templates | Self::UnsavedChanges | Self::Confirm(_)
        )
    }
    fn submit_label(&self, controller: &Controller) -> &'static str {
        match self {
            Self::Recognition if controller.recognition_for_index => "Keep ink and index",
            Self::Recognition => "Replace writing",
            Self::OpenDocument => "Open",
            Self::MoveNotebook(_) | Self::MoveDocument(_) | Self::MovePage => "Move",
            Self::NewDocument | Self::Notebook(_) => "Create",
            Self::SaveTemplate | Self::SavePreset => "Save",
            Self::Equation => "Insert",
            Self::Confirm(Confirm::DeletePermanently(_)) => "Delete permanently",
            Self::Confirm(Confirm::EmptyTrash(_)) => "Empty Trash",
            Self::PdfPassword(..) => "Unlock",
            Self::Crop => "Crop",
            Self::Rename
            | Self::RenameDocument(_)
            | Self::RenameNotebook(_)
            | Self::RenameTemplate(_)
            | Self::RenamePreset(_) => "Rename",
            _ => "Save",
        }
    }
    /// The icon a menu-like popover shows beside its title.
    fn icon(&self) -> Icon {
        match self {
            Self::Rename
            | Self::RenameDocument(_)
            | Self::RenameNotebook(_)
            | Self::RenameTemplate(_)
            | Self::RenamePreset(_) => Icon::Pencil,
            Self::Notebook(_) | Self::MoveNotebook(_) | Self::MoveDocument(_) => Icon::Folder,
            Self::MovePage => Icon::MovePage,
            Self::DocumentTags(_) | Self::Tags => Icon::Tag,
            Self::OpenDocument => Icon::Book,
            Self::NewDocument => Icon::NewPage,
            Self::PageBookmark => Icon::Bookmark,
            Self::SaveTemplate | Self::Templates => Icon::Template,
            Self::SavePreset => Icon::Pen,
            Self::PageSize => Icon::Ruler,
            Self::FontSize => Icon::Text,
            Self::Color | Self::TextColor | Self::CanvasColor | Self::ThemeColor { .. } => {
                Icon::Palette
            }
            Self::Equation | Self::EditEquation(_) => Icon::Math,
            Self::Recognition => Icon::TextScan,
            Self::Crop => Icon::Crop,
            Self::PdfPassword(..) | Self::UnsavedChanges | Self::Confirm(_) => Icon::Alert,
        }
    }
    fn title(&self) -> &'static str {
        match self {
            Self::SaveTemplate => "Save page as template",
            Self::Templates => "Page templates",
            Self::RenameTemplate(_) => "Rename template",
            Self::SavePreset => "Name pen preset",
            Self::RenamePreset(_) => "Rename pen preset",
            Self::PageBookmark => "Name bookmark",
            Self::MovePage => "Move page to document",
            Self::Recognition => "Review recognized writing",
            Self::Rename | Self::RenameDocument(_) => "Rename document",
            Self::DocumentTags(_) => "Document tags",
            Self::OpenDocument => "Open a document",
            Self::NewDocument => "New document options",
            Self::PdfPassword(..) => "Unlock PDF",
            Self::MoveNotebook(_) => "Move folder",
            Self::MoveDocument(_) => "Move document to folder",
            Self::RenameNotebook(_) => "Rename folder",
            Self::Crop => "Crop image",
            Self::Notebook(_) => "New folder",
            Self::Tags => "Document tags",
            Self::Color => "Custom ink color",
            Self::TextColor => "Text color",
            Self::ThemeColor { .. } => "Theme color",
            Self::CanvasColor => "Paper color",
            Self::PageSize => "Custom page size",
            Self::FontSize => "Font size",
            Self::EditEquation(_) => "Edit LaTeX equation",
            Self::Equation => "Insert LaTeX equation",
            Self::UnsavedChanges => "Save changes before closing?",
            Self::Confirm(Confirm::DeletePermanently(_)) => "Delete permanently?",
            Self::Confirm(Confirm::EmptyTrash(_)) => "Empty Trash?",
        }
    }
}
pub struct NotesView {
    updater: folio_update::Updater,
    titlebar_smoke: bool,
    update_preparing: bool,
    canvas_ease: CanvasEase,
    /// The page gliding sideways to stay centred after the canvas changed
    /// width in one step (a sidebar opening or closing).
    canvas_glide: Option<CanvasGlide>,
    /// A touchpad pinch is under way (its `Began` arrived, its `Ended` not yet).
    pinching: bool,
    /// The footer's save status as last drawn, so its change animates.
    last_status: String,
    /// Startup marks still to report: the first document loaded, its ink drawn.
    startup_pending: u8,
    /// The canvas was laid out at a new size since the view was last built.
    canvas_relaid: bool,
    /// A document tab being pressed or dragged along the tab bar.
    tab_drag: Option<navigation::TabDrag>,
    /// Each tab's left edge (without drag offsets) and width, as last laid out.
    tab_bounds: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<Id, (f32, f32)>>>,
    /// A folder row being pressed or dragged in the library sidebar.
    folder_drag: Option<folder_drag::FolderDrag>,
    /// When a dragged folder was last dropped, while it settles.
    folder_settling: Option<std::time::Instant>,
    /// Vector paths of typeset formulas, by formula.
    formula_cache: std::cell::RefCell<math_panel::FormulaCache>,
    /// The typeset recognized math in the review dialog.
    review_typeset: math_panel::Typeset,
    /// Whether the review dialog shows the recognized LaTeX source.
    review_editing: bool,
    region_selection: Option<region::Selection>,
    pub controller: Controller,
    math_inputs: Option<math_panel::Inputs>,
    diagnostics: diagnostics::Diagnostics,
    pub canvas_bounds: Option<Bounds<Pixels>>,
    canvas_document: Option<(Id, Id)>,
    focus: FocusHandle,
    modal: Option<(Modal, Entity<Field>)>,
    equation_draft: Option<String>,
    folder_destination: Option<Id>,
    page_size_unit: page_size::Unit,
    notebook_setup: Option<notebook_setup::Setup>,
    color_drag: Option<(usize, usize)>,
    color_picker: Option<color_picker::PickerState>,
    inline_text: Option<inline_text::Editor>,
    motion: std::cell::RefCell<motion::Motion>,
    transitions: std::cell::RefCell<transitions::Transitions>,
    building_overlay: bool,
    /// The library is being shown again after the editor: its shelf
    /// appears whole, without each document arriving in turn.
    library_arriving: bool,
    /// How far the dialog being built still has to rise into place, 0–1.
    /// Entrance progress of the selection toolbar and library sort menu.
    selection_appear: f32,
    sort_appear: f32,
    /// The window is closing after an explicit save or discard choice.
    closing: bool,
    settings_open: bool,
    settings_section: settings::Section,
    theme_colors_open: bool,
    /// The tablet-pad button whose action chooser is open in Settings.
    pad_button: Option<usize>,
    modal_error: Option<String>,
    /// Highlighted row of the search or open-document picker.
    modal_cursor: usize,
    /// The browser-style find bar, when open.
    find: Option<find::Find>,
    /// Keeps the highlighted picker row in view during keyboard navigation.
    picker_scroll: ScrollHandle,
    /// Scroll position of the Settings section, for its scrollbar.
    settings_scroll: ScrollHandle,
    /// The open-document filter the highlighted row was chosen for.
    picker_query: String,
    move_page_destination: Option<Id>,
    /// Menu control to focus once its menu has been built.
    menu_focus: Option<String>,
    /// Which toolbar control the Document actions popover opens from.
    more_anchor: String,
    attention_error: Option<String>,
    pen_settings: bool,
    pen_menu_section: pen_menu::Section,
    pen_preset_target: Option<Id>,
    tool_menu: Option<(ToolMenu, Point<Pixels>)>,
    selection_tool: Tool,
    more_open: bool,
    more_section: MoreSection,
    selection_toolbar_size: Size<Pixels>,
    export_open: bool,
    export_options: export_options::Options,
    help_open: bool,
    /// The tour of smart ink, while open.
    tour: Option<onboarding::Tour>,
    library_open: bool,
    pages_open: bool,
    list_view: bool,
    list_forced: bool,
    sort_by_name: bool,
    sort_reverse: bool,
    sort_open: bool,
    open_tabs: Vec<Id>,
    tab_scroll: ScrollHandle,
    tab_target: Option<(Id, usize, Pixels, f32)>,
    document_menu: Option<(Id, Point<Pixels>)>,
    /// The document menu was opened from a tab, so it offers tab actions.
    document_menu_tab: bool,
    /// The document menu was opened or navigated by keyboard, so its focused
    /// item shows the focus ring.
    document_menu_keyboard: bool,
    /// A control is being activated by a pointer click rather than a key.
    pointer_activation: bool,
    /// The control last clicked and when: what a menu-like dialog opened by
    /// that click (or by the work it started) grows from.
    last_pressed: Option<(String, std::time::Instant)>,
    /// The bounds of what the open dialog or help popover grew from, in
    /// window points; `None` opens it near the top of the window.
    popover_anchor: Option<accessibility::Rect>,
    /// The menu the open dialog shows inside (see [`MenuHost`]).
    modal_host: Option<MenuHost>,
    folder_menu: Option<(Id, Point<Pixels>)>,
    folder_rows: Vec<Id>,
    folder_scroll: ScrollHandle,
    /// The page thumbnails, scrolled to keep the current page in view.
    pages_scroll: ScrollHandle,
    /// Installed font families, read when the font dialog first opens.
    fonts: Option<std::rc::Rc<Vec<SharedString>>>,
    font_scroll: ScrollHandle,
    /// The formatting bar's open font menu.
    font_menu: Option<font_picker::FontMenu>,
    font_appear: f32,
    pages_followed: Option<(Id, usize)>,
    canvas_menu: Option<canvas_menu::CanvasMenu>,
    tool_wheel: Option<tool_wheel::ToolWheel>,
    /// W is held down; its key repeats must not toggle the wheel.
    wheel_key_held: bool,
    thumbnails: thumbnails::Thumbnails,
    writing_style: Option<folio_document::PenStyle>,
    last_tablet: Option<Instant>,
    pen_in_range: bool,
    /// The pen went down on a control floating over the canvas (the
    /// selection toolbar): its emulated click is the control's, and the
    /// canvas ignores the pen until it lifts.
    pen_on_control: bool,
    /// Whether the selection's menu showed a dialog's page last frame, and
    /// until when it springs to its new place after turning.
    selection_hosted_was: bool,
    selection_glide_until: Option<Instant>,
    /// Pen timestamps, extended past their 32-bit wrap.
    timestamps: TimestampExtender,
    /// The pen's barrel buttons at its last report.
    pen_buttons: u32,
    gesture_start: Option<folio_canvas::Viewport>,
    mouse_pan: bool,
    start: Instant,
    window_title: String,
    painter: painting::Painter,
    accessibility: std::rc::Rc<accessibility::Accessibility>,
    // Window state Rune reports, read by views and handlers.
    pub viewport: Size<Pixels>,
    pub mouse_position: Point<Pixels>,
    pub scale: f32,
    pub system_dark: bool,
    pub window_hovered: bool,
    pub focused_key: Option<String>,
    /// Width the window's caption buttons take over the titlebar band.
    pub chrome_trailing: f32,
    /// Width the window's controls take at the titlebar band's start (macOS).
    pub chrome_leading: f32,
    /// Effects queued while the view was built, applied by the next poll.
    deferred: Vec<Effect>,
}

/// A scroll viewport's key, the row a view asks it to reveal, and where it
/// last scrolled to.
#[derive(Clone)]
pub struct ScrollHandle {
    pub key: String,
    pub request: std::rc::Rc<std::cell::Cell<Option<usize>>>,
    pub position: std::rc::Rc<std::cell::Cell<Option<rune::ScrollPos>>>,
    /// A thumb's offset along the viewport, kept current while it scrolls.
    pub thumb: rune::Live<[f32; 2]>,
}
impl Default for ScrollHandle {
    fn default() -> Self {
        Self {
            key: String::new(),
            request: Default::default(),
            position: Default::default(),
            thumb: rune::Live::new([0., 0.]),
        }
    }
}
impl ScrollHandle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn keyed(key: &str) -> Self {
        Self {
            key: key.into(),
            ..Self::default()
        }
    }
    pub fn set_offset(&self, _: Point<Pixels>) {}
    pub fn scroll_to(&self, index: usize) {
        self.request.set(Some(index));
    }
    pub fn scroll_to_item(&self, index: usize, _: ScrollStrategy) {
        self.request.set(Some(index));
    }
    pub fn scroll_to_item_strict(&self, index: usize, _: ScrollStrategy) {
        self.request.set(Some(index));
    }
    pub fn offset(&self) -> Point<Pixels> {
        self.position.get().map_or_else(Point::default, |p| {
            point(px(-p.offset[0]), px(-p.offset[1]))
        })
    }
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.position.get().map_or_else(Bounds::default, |p| {
            Bounds::new(Point::default(), size(px(p.viewport[0]), px(p.viewport[1])))
        })
    }
    pub fn max_offset(&self) -> Size<Pixels> {
        self.position
            .get()
            .map_or_else(Size::default, |p| size(px(p.max[0]), px(p.max[1])))
    }
    pub fn take_request(&self) -> Option<usize> {
        self.request.take()
    }
}
#[derive(Clone, Copy)]
pub enum ScrollStrategy {
    Top,
    Bottom,
    Center,
}

impl NotesView {
    pub fn new(controller: Controller) -> Self {
        Self::new_with_titlebar_fixture(controller, false)
    }
    /// Isolated real-pointer fixture: no update worker or installed update cache is opened.
    pub fn new_titlebar_smoke(controller: Controller) -> Self {
        let mut view = Self::new_with_titlebar_fixture(controller, true);
        view.update_smoke_phase(0);
        view
    }
    fn new_with_titlebar_fixture(controller: Controller, titlebar_smoke: bool) -> Self {
        // The tour opens by itself until it has been seen once (not in the
        // title bar fixture).
        let first_tour = controller.settings.tour_due() && !titlebar_smoke;
        let mut workspace = controller.settings.workspace.clone();
        workspace
            .open_tabs
            .retain(|id| controller.notes.iter().any(|n| n.id == *id));
        if !controller.settings.reopen_documents {
            workspace.open_tabs.clear();
            workspace.library_open = true;
        }
        Self {
            updater: if titlebar_smoke {
                folio_update::Updater::disabled()
            } else {
                folio_update::Updater::new()
            },
            titlebar_smoke,
            update_preparing: false,
            canvas_ease: CanvasEase::default(),
            canvas_glide: None,
            pinching: false,
            last_status: String::new(),
            startup_pending: 2,
            canvas_relaid: false,
            tab_drag: None,
            tab_bounds: Default::default(),
            folder_drag: None,
            folder_settling: None,
            formula_cache: Default::default(),
            review_typeset: Default::default(),
            review_editing: false,
            region_selection: None,
            controller,
            math_inputs: None,
            diagnostics: diagnostics::Diagnostics::new(),
            canvas_bounds: None,
            canvas_document: None,
            focus: FocusHandle::new("folio-root"),
            modal: None,
            equation_draft: None,
            folder_destination: None,
            page_size_unit: page_size::Unit::default(),
            notebook_setup: None,
            color_drag: None,
            color_picker: None,
            inline_text: None,
            motion: Default::default(),
            transitions: Default::default(),
            building_overlay: false,
            library_arriving: false,
            selection_appear: 1.,
            sort_appear: 1.,
            closing: false,
            settings_open: false,
            settings_section: settings::Section::default(),
            theme_colors_open: false,
            pad_button: None,
            modal_error: None,
            modal_cursor: 0,
            find: None,
            picker_scroll: ScrollHandle::keyed("tab-picker-results"),
            settings_scroll: ScrollHandle::keyed("settings-section-scroll"),
            picker_query: String::new(),
            move_page_destination: None,
            menu_focus: None,
            more_anchor: "more".into(),
            attention_error: None,
            pen_settings: false,
            pen_menu_section: pen_menu::Section::default(),
            pen_preset_target: None,
            tool_menu: None,
            selection_tool: Tool::Lasso,
            more_open: false,
            more_section: MoreSection::Document,
            selection_toolbar_size: size(px(600.), px(80.)),
            export_open: false,
            export_options: export_options::Options::default(),
            help_open: false,
            tour: first_tour.then(onboarding::Tour::start),
            library_open: workspace.library_open,
            pages_open: workspace.pages_open,
            list_view: workspace.list_view,
            list_forced: false,
            sort_by_name: workspace.sort_by_name,
            sort_reverse: workspace.sort_reverse,
            sort_open: false,
            open_tabs: workspace.open_tabs,
            tab_scroll: ScrollHandle::keyed("document-tabs"),
            tab_target: None,
            document_menu: None,
            document_menu_tab: false,
            document_menu_keyboard: false,
            pointer_activation: false,
            last_pressed: None,
            popover_anchor: None,
            modal_host: None,
            folder_menu: None,
            folder_rows: Vec::new(),
            folder_scroll: ScrollHandle::keyed("folder-list"),
            pages_scroll: ScrollHandle::keyed("page-list"),
            fonts: None,
            font_scroll: ScrollHandle::keyed("font-list"),
            font_menu: None,
            font_appear: 1.,
            pages_followed: None,
            tool_wheel: None,
            wheel_key_held: false,
            canvas_menu: None,
            thumbnails: thumbnails::Thumbnails::default(),
            writing_style: None,
            last_tablet: None,
            pen_on_control: false,
            selection_hosted_was: false,
            selection_glide_until: None,
            pen_in_range: false,
            timestamps: TimestampExtender::default(),
            pen_buttons: 0,
            gesture_start: None,
            mouse_pan: false,
            start: Instant::now(),
            window_title: "Folio".into(),
            painter: painting::Painter::default(),
            accessibility: Default::default(),
            viewport: size(px(1320.), px(860.)),
            mouse_position: point(px(0.), px(0.)),
            scale: 1.,
            system_dark: false,
            window_hovered: false,
            focused_key: None,
            chrome_trailing: 0.,
            chrome_leading: 0.,
            deferred: Vec::new(),
        }
    }
    /// Background maintenance before each frame: document workers, saving,
    /// updates and animations. Returns whether anything visible changed.
    pub fn poll(&mut self, tx: &mut rune::Tx) -> bool {
        let changed = !self.update_preparing && self.controller.tick();
        let update_changed = self.updater.poll();
        if self.updater.state == folio_update::State::Exit {
            self.update_preparing = false;
            tx.quit();
            return true;
        }
        if self.update_preparing
            && update_changed
            && !matches!(
                self.updater.state,
                folio_update::State::Preparing { .. } | folio_update::State::Exit
            )
        {
            self.update_preparing = false;
        }
        let polled = executor::poll(self);
        let finished = polled.finished;
        let motion_changed =
            self.motion.borrow_mut().advance() || self.transitions.borrow().animating();
        // The canvas's own animations (a selecting loop's morph) and the
        // tour's demos redraw every frame while they play.
        let easing = self.canvas_ease.last.is_some()
            || self.canvas_glide.is_some()
            || self.controller.select_morph.is_some()
            || self.tour.is_some();
        let images_ready = paint::take_images_ready();
        // Ink and image rasters notify when they finish (`finished` / `images_ready`);
        // the view need not rebuild every frame while they run.
        let notify =
            changed || update_changed || motion_changed || easing || finished || images_ready;
        // Waiting futures need a poll only at their timer's deadline; finished
        // background work wakes the window itself.
        // The updater wakes the window itself whenever its state moves on.
        let fast = notify || self.controller.needs_fast_poll() || self.motion.borrow().needs_frame();
        let timer = polled
            .deadline
            .map(|at| at.saturating_duration_since(Instant::now()));
        if fast {
            tx.wake_after(Duration::from_millis(16));
        } else if let Some(next) = [
            self.controller
                .next_idle_wake()
                .map(|next| next.max(Duration::from_millis(16))),
            timer.map(|next| next.max(Duration::from_millis(1))),
        ]
        .into_iter()
        .flatten()
        .min()
        {
            tx.wake_after(next);
        }
        let deferred = std::mem::take(&mut self.deferred);
        compat::apply(deferred, tx);
        notify
    }
    /// Queues effects from outside a handler, applied by the next poll.
    pub fn queue_effects(&mut self, effects: Vec<Effect>) {
        self.deferred.extend(effects);
    }
    /// Applies what a handler queued through its window and context.
    pub fn apply_effects(&mut self, window: Window, cx: Context<NotesView>, tx: &mut rune::Tx) {
        compat::apply(window.effects, tx);
        compat::apply(cx.effects, tx);
        // Input resumes maintenance so newly started work runs at once.
        tx.wake_after(Duration::from_millis(16));
    }
    pub fn bindings() -> Vec<KeyBinding> {
        let mut bindings = vec![
            KeyBinding::new("ctrl-t", OpenTab, Some("FolioLibrary")),
            KeyBinding::new("ctrl-w", CloseTab, Some("FolioLibrary")),
            KeyBinding::new("ctrl-tab", NextTab, Some("FolioLibrary")),
            KeyBinding::new("ctrl-shift-tab", PreviousTab, Some("FolioLibrary")),
            KeyBinding::new("ctrl-n", NewNote, Some("FolioLibrary")),
            KeyBinding::new("ctrl-z", Undo, Some("FolioLibrary")),
            KeyBinding::new("ctrl-o", Import, Some("FolioLibrary")),
            KeyBinding::new("ctrl-f", Search, Some("FolioLibrary")),
            KeyBinding::new("ctrl-,", Settings, Some("FolioLibrary")),
            KeyBinding::new("ctrl-s", Save, Some("FolioLibrary")),
            KeyBinding::new("ctrl-shift-l", Library, Some("FolioLibrary")),
            KeyBinding::new("escape", Escape, Some("FolioLibrary")),
        ];
        bindings.extend([
            KeyBinding::new("ctrl-z", Undo, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-z", Redo, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-y", Redo, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-s", Save, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-n", NewNote, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-n", NewPage, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-f", Search, Some("Folio && !FolioField")),
            KeyBinding::new("f3", FindNext, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-g", FindNext, Some("Folio && !FolioField")),
            KeyBinding::new("shift-f3", FindPrevious, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-g", FindPrevious, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-l", Library, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-p", TogglePages, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-c", Copy, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-x", Cut, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-v", Paste, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-v", Paste, Some("FolioCanvasMenu")),
            KeyBinding::new("escape", Escape, Some("FolioCanvasMenu")),
            KeyBinding::new("ctrl-a", SelectAll, Some("Folio && !FolioField")),
            KeyBinding::new("delete", Delete, Some("Folio && !FolioField")),
            KeyBinding::new("backspace", Delete, Some("Folio && !FolioField")),
            KeyBinding::new("escape", Escape, Some("Folio && !FolioField")),
            KeyBinding::new("escape", Escape, Some("FolioDialog")),
            KeyBinding::new("escape", Escape, Some("FolioField")),
            KeyBinding::new("p", Pen, Some("Folio && !FolioField")),
            KeyBinding::new("e", Eraser, Some("Folio && !FolioField")),
            KeyBinding::new("l", Lasso, Some("Folio && !FolioField")),
            KeyBinding::new("h", Hand, Some("Folio && !FolioField")),
            KeyBinding::new("t", Text, Some("Folio && !FolioField")),
            KeyBinding::new("s", Shapes, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-+", ZoomIn, Some("Folio && !FolioField")),
            // The same key without Shift, as in other apps.
            KeyBinding::new("ctrl-=", ZoomIn, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-=", ZoomIn, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl--", ZoomOut, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-0", FitPage, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-pageup", PreviousPage, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-pagedown", NextPage, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-,", Settings, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-o", Import, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-t", OpenTab, Some("Folio && !FolioDialog")),
            KeyBinding::new("ctrl-w", CloseTab, Some("Folio && !FolioDialog")),
            KeyBinding::new("ctrl-tab", NextTab, Some("Folio && !FolioDialog")),
            KeyBinding::new("ctrl-shift-tab", PreviousTab, Some("Folio && !FolioDialog")),
            KeyBinding::new("ctrl-shift-e", Export, Some("Folio && !FolioField")),
            KeyBinding::new(
                "enter",
                ApplyCrop,
                Some("FolioCrop && !FolioField && !FolioControl"),
            ),
            KeyBinding::new(
                "enter",
                EditText,
                Some("Folio && !FolioCrop && !FolioField && !FolioControl"),
            ),
            KeyBinding::new("w", OpenToolWheel, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-b", ToggleBold, Some("Folio")),
            KeyBinding::new("ctrl-i", ToggleItalic, Some("Folio")),
            KeyBinding::new("ctrl-u", ToggleUnderline, Some("Folio")),
        ]);
        bindings
    }
    pub fn titlebar_smoke_bindings() -> Vec<KeyBinding> {
        vec![KeyBinding::new("ctrl-alt-u", ResetTitlebarSmoke, None)]
    }
    fn blocking_overlay(&self) -> bool {
        self.update_preparing
            || self.modal.is_some()
            || self.document_menu.is_some()
            || self.folder_menu.is_some()
            || self.canvas_menu.is_some()
            || self.settings_open
            || self.help_open
            || self.tour.is_some()
            || (self.controller.error.is_some() && self.controller.interaction.is_none())
    }
    /// Whether an overlay keeps pointer and pen input away from the canvas.
    fn canvas_input_blocked(&self) -> bool {
        self.library_open
            || self.blocking_overlay()
            || self.tool_menu.is_some()
            || self.tool_wheel.is_some()
    }
    fn open_settings(&mut self, window: &mut Window) {
        self.controller.finish();
        self.dismiss_popovers();
        self.document_menu = None;
        self.settings_open = true;
        self.focus.focus(window);
    }
    fn open_help(&mut self, window: &mut Window) {
        self.controller.finish();
        self.dismiss_popovers();
        self.document_menu = None;
        self.popover_anchor = self.pressed_anchor(false);
        self.help_open = true;
        self.focus.focus(window);
    }
    fn dismiss_popovers(&mut self) {
        self.pen_menu_section = pen_menu::Section::Pen;
        self.tool_menu = None;
        self.tool_wheel = None;
        self.folder_menu = None;
        self.canvas_menu = None;
        self.sort_open = false;
        self.font_menu = None;
        self.more_open = false;
        self.pen_settings = false;
        self.export_open = false;
    }
    fn cancel_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some((Modal::Recognition, _))) {
            self.controller.cancel_recognition();
        }
        if let Some((Modal::PdfPassword(note, _), _)) = &self.modal {
            self.controller.cancel_pdf_import(*note);
        }
        self.close_modal(window, cx);
    }
    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.equation_draft.take().is_some() {
            self.controller.cancel_equation_render();
        }
        self.clear_color_picker(window);
        self.modal = None;
        self.modal_host = None;
        self.notebook_setup = None;
        if let Some(editor) = &self.inline_text {
            // Back to the text being typed on the canvas.
            editor.field.read(cx).focus.focus_for_typing(window);
        } else {
            self.focus.focus(window);
        }
        cx.notify();
    }
    fn modal(&mut self, modal: Modal, window: &mut Window, cx: &mut Context<Self>) {
        self.modal_host = None;
        self.tool_menu = None;
        self.sort_open = false;
        self.folder_destination = match modal {
            Modal::MoveNotebook(id) => self
                .controller
                .notebooks
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.parent),
            Modal::MoveDocument(id) => self
                .controller
                .notes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.notebook),
            _ => None,
        };
        self.controller.finish();
        self.document_menu = None;
        self.folder_menu = None;
        self.more_open = false;
        self.pen_settings = false;
        self.notebook_setup = None;
        if matches!(modal, Modal::NewDocument) {
            self.prepare_notebook_setup(window, cx);
        }
        let content = match &modal {
            Modal::SavePreset => format!(
                "{:?} {}",
                self.controller.style.tool,
                self.controller.settings.pen_presets.len() + 1
            ),
            Modal::RenamePreset(id) => self
                .controller
                .settings
                .pen_presets
                .iter()
                .find(|p| p.id == *id)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            Modal::SaveTemplate => self
                .controller
                .page()
                .properties
                .bookmark
                .clone()
                .unwrap_or_else(|| self.controller.session().document.metadata.title.clone()),
            Modal::RenameTemplate(id) => self
                .controller
                .settings
                .templates
                .iter()
                .find(|t| t.id == *id)
                .map(|t| t.name.clone())
                .unwrap_or_default(),
            Modal::PageBookmark => self
                .controller
                .page()
                .properties
                .bookmark
                .clone()
                .unwrap_or_default(),
            Modal::Recognition => self
                .controller
                .recognition_review
                .as_ref()
                .map(|r| r.text.clone())
                .unwrap_or_default(),
            Modal::EditEquation(id) => self
                .controller
                .page()
                .objects
                .get(id)
                .map(|o| match o.as_ref() {
                    folio_document::Object::Equation(e) => e
                        .math_link
                        .as_ref()
                        .map_or_else(|| e.latex.clone(), |link| link.expression.clone()),
                    _ => o.searchable_text().to_string(),
                })
                .unwrap_or_default(),
            Modal::Rename => self.controller.session().document.metadata.title.clone(),
            Modal::RenameNotebook(id) => self
                .controller
                .notebooks
                .iter()
                .find(|n| n.id == *id)
                .map(|n| n.name.clone())
                .unwrap_or_default(),
            Modal::RenameDocument(id) => self
                .controller
                .notes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| n.title.clone())
                .unwrap_or_default(),
            Modal::DocumentTags(id) => self
                .controller
                .notes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| n.tags.join(", "))
                .unwrap_or_default(),
            Modal::Tags => self.controller.session().document.metadata.tags.join(", "),
            Modal::Color => self.controller.style.color.hex(),
            Modal::ThemeColor { dark, token } => {
                self.controller.settings.appearance.palette(*dark)[token].hex()
            }
            Modal::CanvasColor => format!(
                "#{:06x}",
                Theme::new(&self.controller.settings).canvas.paper
            ),
            Modal::PageSize => self.page_size_unit.format(
                self.controller.page().properties.width,
                self.controller.page().properties.height,
            ),
            Modal::FontSize | Modal::TextColor => self
                .controller
                .session()
                .selection
                .iter()
                .find_map(|id| {
                    if let Some(folio_document::Object::Text(text)) =
                        self.controller.page().objects.get(id).map(|o| o.as_ref())
                    {
                        Some(match modal {
                            Modal::FontSize => text.font_size.to_string(),
                            _ => text.color.hex(),
                        })
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
            // The current crop of the selected image, or the whole image.
            Modal::Crop => {
                let crop = self
                    .controller
                    .session()
                    .selection
                    .iter()
                    .find_map(
                        |id| match self.controller.page().objects.get(id)?.as_ref() {
                            folio_document::Object::Image(image) => Some(image.crop),
                            _ => None,
                        },
                    )
                    .flatten()
                    .unwrap_or(folio_document::Rect::new(0., 0., 1., 1.));
                let round = |v: f32| (v * 1000.).round() / 1000.;
                format!(
                    "{}, {}, {}, {}",
                    round(crop.min.x),
                    round(crop.min.y),
                    round(crop.width()),
                    round(crop.height())
                )
            }
            _ => String::new(),
        };
        let multiline = matches!(modal, Modal::Recognition);
        let secret = matches!(modal, Modal::PdfPassword(..));
        let placeholder = match modal {
            Modal::OpenDocument => Some("Open a document…"),
            Modal::MoveNotebook(_) | Modal::MoveDocument(_) => Some("Filter folders"),
            Modal::NewDocument => Some(""),
            Modal::Notebook(_) => Some("Folder name"),
            Modal::Tags | Modal::DocumentTags(_) => Some("Comma-separated tags"),
            Modal::PdfPassword(..) => Some("Password"),
            Modal::Recognition => Some("Type the text for this writing"),
            _ => None,
        };
        let field = Entity::new({
            let mut field = Field::new(content, multiline);
            field.secret = secret;
            field.theme = Theme::new(&self.controller.settings);
            field.placeholder = placeholder.filter(|p| !p.is_empty()).map(Into::into);
            // Typing replaces a prefilled single-line value, as in other
            // dialogs; the move filters start empty.
            if !multiline {
                field.select_all_content();
            }
            field
        });
        if matches!(modal, Modal::NewDocument) {
            let untitled = self.controller.untitled_title();
            field.update(cx, |field, _| field.placeholder = Some(untitled.into()));
        }
        self.modal_cursor = 0;
        self.picker_query.clear();
        self.picker_scroll.set_offset(Point::default());
        self.move_page_destination = None;
        self.review_editing = false;
        // Dialogs without a text field keep keyboard focus on their first
        // control, or on the root for Escape. Recognized math opens on its
        // rendered formula, with the actions in reach of the keyboard.
        if matches!(modal, Modal::Recognition) && self.reviewing_math() {
            self.focus.focus(window);
            self.menu_focus = Some("submit-modal".into());
        } else if modal.has_field() {
            field.read(cx).focus.focus(window);
        } else {
            self.focus.focus(window);
            self.menu_focus = Some(
                match modal {
                    Modal::Confirm(_) | Modal::MovePage => "cancel-modal",
                    Modal::UnsavedChanges => "save-and-close",
                    _ => "cancel-modal",
                }
                .into(),
            );
        }
        self.observe_notebook_setup(window, cx);
        {
            let mut f = field.0.borrow_mut();
            f.on_change = Some(std::rc::Rc::new(|this: &mut NotesView, _, _| {
                this.modal_error = None;
            }));
            f.on_navigate = Some(std::rc::Rc::new(|this: &mut NotesView, down, _, cx| {
                this.navigate_modal(down, cx)
            }));
            f.on_submit = Some(std::rc::Rc::new(|this: &mut NotesView, window, cx| {
                this.submit_modal(window, cx)
            }));
            f.on_escape = Some(std::rc::Rc::new(|this: &mut NotesView, window, cx| {
                this.escape(window, cx)
            }));
        }
        self.modal_error = None;
        self.clear_color_picker(window);
        // A dialog opened by work a click started (a recognition finishing)
        // still grows from that click's control.
        let deferred = matches!(modal, Modal::Recognition | Modal::PdfPassword(..));
        self.popover_anchor = self.pressed_anchor(deferred);
        self.modal = Some((modal, field));
        cx.notify();
    }
    /// Opens `modal` inside the menu `host` it was chosen from: the menu
    /// stays open and morphs into the dialog's page, whose back row returns
    /// to the menu.
    fn modal_in_menu(
        &mut self,
        host: MenuHost,
        modal: Modal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (document, tab, folder) =
            (self.document_menu, self.document_menu_tab, self.folder_menu);
        let (more, pen, section) = (self.more_open, self.pen_settings, self.more_section);
        let tool = self.tool_menu;
        self.modal(modal, window, cx);
        match host {
            MenuHost::Document => {
                self.document_menu = document;
                self.document_menu_tab = tab;
            }
            MenuHost::Folder => self.folder_menu = folder,
            MenuHost::Popover => {
                self.more_open = more;
                self.pen_settings = pen;
                self.more_section = section;
            }
            MenuHost::Tool => self.tool_menu = tool,
            // The selection, and so its menu, stays.
            MenuHost::Selection => {}
        }
        self.modal_host = Some(host);
        self.popover_anchor = None;
    }
    /// Whether the open dialog shows inside the menu `host`, which is open.
    fn modal_hosted_in(&self, host: MenuHost) -> bool {
        self.modal.is_some()
            && self.modal_host == Some(host)
            && match host {
                MenuHost::Document => self.document_menu.is_some(),
                MenuHost::Folder => self.folder_menu.is_some(),
                MenuHost::Popover => self.more_open || self.pen_settings,
                MenuHost::Tool => self.tool_menu.is_some(),
                MenuHost::Selection => !self.library_open && self.selection_toolbar_shown(),
            }
    }
    /// Whether the open dialog shows inside a menu rather than on its own.
    fn modal_hosted(&self) -> bool {
        self.modal_host.is_some_and(|host| self.modal_hosted_in(host))
    }
    /// Closes the menu a dialog showed in, once the dialog is done.
    fn close_menu_host(&mut self, host: MenuHost) {
        match host {
            MenuHost::Document => self.document_menu = None,
            MenuHost::Folder => self.folder_menu = None,
            MenuHost::Popover => self.dismiss_popovers(),
            MenuHost::Tool => self.tool_menu = None,
            // Done, the selection shows its actions again.
            MenuHost::Selection => {}
        }
    }
    /// The bounds of the control whose click is opening a popover, if one
    /// is (or, when `deferred`, was clicked in the last few seconds).
    fn pressed_anchor(&self, deferred: bool) -> Option<accessibility::Rect> {
        let (key, at) = self.last_pressed.as_ref()?;
        let recent = at.elapsed() < std::time::Duration::from_secs(20);
        if !(self.pointer_activation || deferred && recent) {
            return None;
        }
        self.accessibility.control_bounds_for_key(key)
    }
    /// The theme for the window's chrome (bars and sidebars). While a dialog
    /// covers it with its scrim, the chrome draws no blur of its own: Rune
    /// blurs only the first few glass surfaces in a frame
    /// (`rune::MAX_BACKDROP_BLURS`), and the bars would use them up before
    /// the dialog, which then showed only its tint over the canvas.
    fn chrome_theme(&self) -> Theme {
        let theme = Theme::new(&self.controller.settings);
        let covered = self.settings_open
            || self.tour.is_some()
            || (self.modal.is_some() && !self.modal_is_menu())
            || (self.controller.error.is_some() && self.controller.interaction.is_none());
        if covered { Theme { glass_blur: 0., ..theme } } else { theme }
    }
    /// Whether the open dialog shows as a menu-like popover beside what
    /// opened it. Confirmations stay dialogs over a scrim.
    fn modal_is_menu(&self) -> bool {
        self.modal.as_ref().is_some_and(|(m, _)| {
            !matches!(m, Modal::Confirm(_) | Modal::UnsavedChanges | Modal::PdfPassword(..))
        })
    }
    /// Where a menu-like popover `width` wide and about `height` tall goes,
    /// in the coordinates of the body it is laid over: under what opened it
    /// (above when there is no room below), else centred near the top.
    /// Returns its left and top, the height it may take and the point it
    /// grows from.
    fn popover_frame(&self, window: &Window, width: f32, height: f32) -> PopoverFrame {
        let viewport = window.viewport_size();
        let (left, top, room_w, room_h) = self
            .accessibility
            .control_bounds_for_key("chrome-body")
            .map_or(
                (0., 0., f32::from(viewport.width), f32::from(viewport.height)),
                |r| (r.x0 as f32, r.y0 as f32, (r.x1 - r.x0) as f32, (r.y1 - r.y0) as f32),
            );
        let width = width.min((room_w - 24.).max(1.));
        let height = height.min((room_h - 24.).max(1.));
        let max_x = (room_w - width - 12.).max(12.);
        let (x, y, room, grow) = match self.popover_anchor {
            Some(r) => {
                let (ax0, ay0) = (r.x0 as f32 - left, r.y0 as f32 - top);
                let (ax1, ay1) = (r.x1 as f32 - left, r.y1 as f32 - top);
                let x = ax0.clamp(12., max_x);
                // Below what opened it, else above when there is more room
                // there; never over it. Too tall for either, it scrolls.
                let below = room_h - 12. - (ay1 + 6.);
                let above = ay0 - 6. - 12.;
                let (y, room, gy) = if height <= below || below >= above {
                    (ay1 + 6., below, ay1)
                } else {
                    let h = height.min(above);
                    (ay0 - 6. - h, h, ay0)
                };
                // It grows from the middle of the edge of what opened it
                // nearest to it.
                (x, y, room, (((ax0 + ax1) / 2.).clamp(x, x + width), gy))
            }
            None => {
                let x = ((room_w - width) / 2.).clamp(12., max_x);
                let y = 64_f32.min((room_h - height - 12.).max(12.));
                (x, y, room_h - y - 12., (x + width / 2., y))
            }
        };
        let grow = (grow.0, grow.1.clamp(y - 6., y + height.min(room) + 6.));
        PopoverFrame { x, y, width, height: height.min(room), room: room.max(120.), grow }
    }
    /// Submits the open dialog. One shown inside a menu closes that menu
    /// too once it is done.
    fn submit_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.modal_host.filter(|_| self.modal_hosted());
        self.submit_modal_now(window, cx);
        if let Some(host) = host
            && self.modal.is_none()
        {
            self.close_menu_host(host);
        }
    }
    fn submit_modal_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((modal, field)) = self.modal.as_ref() else {
            return;
        };
        let modal = modal.clone();
        let field = field.clone();
        let content = field.read(cx).content().clone();
        if self.controller.equation_pending
            && matches!(modal, Modal::Equation | Modal::EditEquation(_))
        {
            return;
        }
        match modal {
            Modal::SaveTemplate | Modal::RenameTemplate(_) => {
                let result = if let Modal::RenameTemplate(id) = modal {
                    self.controller.rename_template(id, content)
                } else {
                    self.controller.save_page_template(content)
                };
                if let Err(error) = result {
                    self.modal_error = Some(error);
                    cx.notify();
                    return;
                }
            }
            Modal::Templates => {}
            Modal::NewDocument => match self.create_configured_notebook(content, cx) {
                Ok(()) => self.show_editor(),
                Err(error) => {
                    self.modal_error = Some(error);
                    cx.notify();
                    return;
                }
            },
            Modal::PageBookmark => self.controller.bookmark_page(content),
            Modal::MovePage => {
                let Some(destination) = self.move_page_destination else {
                    self.modal_error = Some("Choose a destination document.".into());
                    cx.notify();
                    return;
                };
                self.controller.move_page_to(destination);
            }
            Modal::UnsavedChanges => {
                self.close_window(true, window, cx);
                return;
            }
            Modal::Confirm(confirm) => match confirm {
                Confirm::DeletePermanently(id) => {
                    self.open_tabs.retain(|tab| *tab != id);
                    self.controller.delete_permanently(id);
                }
                Confirm::EmptyTrash(folder) => {
                    let ids: Vec<_> = self
                        .controller
                        .notes
                        .iter()
                        .filter(|n| n.trashed && (folder.is_none() || n.notebook == folder))
                        .map(|n| n.id)
                        .collect();
                    self.open_tabs.retain(|tab| !ids.contains(tab));
                    self.controller.empty_trash(folder);
                }
            },
            Modal::Recognition => {
                let result = if self.controller.recognition_for_index {
                    self.controller.keep_ink_and_index(content)
                } else {
                    self.controller.replace_recognized_writing(content)
                };
                if let Err(error) = result {
                    self.modal_error = Some(error);
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            }
            Modal::EditEquation(id) => {
                self.equation_draft = Some(content.clone());
                self.modal_error = None;
                self.controller.edit_equation(id, content);
                cx.notify();
                return;
            }
            Modal::PdfPassword(note, path) => self.controller.unlock_pdf(note, path, content),
            Modal::Equation => {
                self.equation_draft = Some(content.clone());
                self.modal_error = None;
                self.controller.insert_equation(content);
                cx.notify();
                return;
            }
            Modal::Rename => self.controller.manage_note(
                self.controller.active,
                folio_app::NoteAction::Rename(content),
            ),
            Modal::RenameDocument(id) => self
                .controller
                .manage_note(id, folio_app::NoteAction::Rename(content)),
            Modal::DocumentTags(id) => self.controller.manage_note(
                id,
                folio_app::NoteAction::Tags(
                    content
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                ),
            ),
            Modal::OpenDocument => {
                let matches = self.open_document_matches(cx);
                let found = matches
                    .get(self.modal_cursor.min(matches.len().saturating_sub(1)))
                    .map(|n| n.id);
                if let Some(id) = found {
                    self.open_note(id);
                } else {
                    self.modal_error = Some("No matching documents".into());
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            }
            Modal::Notebook(parent) => {
                if let Err(error) = self.controller.create_notebook(content, parent) {
                    self.modal_error = Some(error);
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            }
            Modal::MoveDocument(id) => self
                .controller
                .manage_note(id, folio_app::NoteAction::Move(self.folder_destination)),
            Modal::MoveNotebook(id) => {
                let parent = self.folder_destination;
                if let Err(e) = self.controller.move_notebook(id, parent) {
                    self.modal_error = Some(e);
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            }
            Modal::RenameNotebook(id) => {
                if let Err(error) = self.controller.rename_notebook(id, content) {
                    self.modal_error = Some(error);
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            }
            Modal::Crop => {
                let parts = match validation::crop(&content) {
                    Ok(parts) => parts,
                    Err(message) => {
                        self.modal_error = Some(message.into());
                        field.read(cx).focus.focus(window);
                        cx.notify();
                        return;
                    }
                };
                self.controller
                    .crop_selection(Some(folio_document::Rect::new(
                        parts[0], parts[1], parts[2], parts[3],
                    )));
            }
            Modal::Tags => self.controller.manage_note(
                self.controller.active,
                folio_app::NoteAction::Tags(
                    content
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                ),
            ),
            Modal::SavePreset | Modal::RenamePreset(_) => {
                let result = if let Modal::RenamePreset(id) = modal {
                    self.controller.rename_preset(id, content)
                } else {
                    self.controller.save_named_preset(content).map(|_| ())
                };
                if let Err(error) = result {
                    self.modal_error = Some(error);
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            }
            Modal::ThemeColor { dark, token } => {
                match folio_app::appearance::ThemeColor::parse(&content) {
                    Ok(color)
                        if color.alpha() == 1.
                            || matches!(
                                token,
                                folio_app::appearance::ThemeToken::Border
                                    | folio_app::appearance::ThemeToken::Input
                            ) =>
                    {
                        self.controller
                            .settings
                            .appearance
                            .set_color(dark, token, color)
                    }
                    Ok(_) => {
                        self.modal_error=Some("Use an opaque color here. Opacity is supported for border and input colors.".into());
                        field.read(cx).focus.focus(window);
                        cx.notify();
                        return;
                    }
                    Err(message) => {
                        self.modal_error = Some(message.into());
                        field.read(cx).focus.focus(window);
                        cx.notify();
                        return;
                    }
                }
            }
            Modal::CanvasColor => match folio_app::appearance::ThemeColor::parse(&content) {
                Ok(color) if color.alpha() == 1. => {
                    self.controller.settings.appearance.canvas_color =
                        Some(Color::from_rgb(color.rgb()))
                }
                _ => {
                    self.modal_error = Some("Paper needs an opaque color, such as #ffffff.".into());
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            },
            Modal::Color => match u32::from_str_radix(content.trim().trim_start_matches('#'), 16) {
                Ok(color) if content.trim().trim_start_matches('#').len() == 6 => {
                    self.controller.set_color(Color::from_rgb(color))
                }
                _ => {
                    self.modal_error = Some("Use a six-digit hex color, such as #426b52.".into());
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            },
            Modal::PageSize => {
                let (width, height) = match self.page_size_unit.parse(&content) {
                    Ok(size) => size,
                    Err(message) => {
                        self.modal_error = Some(message.into());
                        field.read(cx).focus.focus(window);
                        cx.notify();
                        return;
                    }
                };
                self.controller.page_size(
                    width,
                    height,
                    self.controller.page().properties.infinite,
                );
            }
            Modal::TextColor => {
                let color = match folio_app::appearance::ThemeColor::parse(&content) {
                    Ok(color) => Color::from_rgb(color.rgb()),
                    Err(error) => {
                        self.modal_error = Some(error.into());
                        cx.notify();
                        return;
                    }
                };
                for id in self.controller.session().selection.clone() {
                    self.controller.edit_text(id, |t| t.color = color);
                }
            }
            Modal::FontSize => {
                let size = match validation::font_size(&content) {
                    Ok(size) => size,
                    Err(message) => {
                        self.modal_error = Some(message.into());
                        field.read(cx).focus.focus(window);
                        cx.notify();
                        return;
                    }
                };
                let ids = self.controller.session().selection.clone();
                for id in ids {
                    self.controller.edit_text(id, |t| t.font_size = size);
                }
            }
        }
        self.controller.store_settings();
        self.close_modal(window, cx);
    }
    fn fit(&mut self) {
        self.canvas_ease = CanvasEase::default();
        if let Some(bounds) = self.canvas_bounds {
            let props = self.controller.page().properties.clone();
            self.controller.session_mut().viewport.fit(
                &props,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            );
        }
    }
    /// [`Self::fit`] for the Fit button and shortcut, easing into place.
    fn fit_eased(&mut self) {
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        let props = self.controller.page().properties.clone();
        let instant =
            self.controller.settings.reduce_motion || self.controller.interaction.is_some();
        let viewport = &mut self.controller.session_mut().viewport;
        let start = *viewport;
        viewport.fit(
            &props,
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        );
        let target = *viewport;
        // Unrotated, the move to the fitted view is a zoom about one fixed
        // point (or a pan, when the zoom stays), which eases like the wheel.
        if instant || start.rotation != 0. {
            return;
        }
        *viewport = start;
        *self.ease_target() = target;
    }
    /// Goes to page `index`, gliding there through the page stack rather
    /// than cutting. A far page is approached from one screen away.
    pub(crate) fn go_to_page(&mut self, index: usize) {
        let before_page = self.controller.session().page;
        let before = self.controller.session().viewport;
        self.controller.change_page(index);
        self.glide_from(before_page, before);
    }
    /// Adds a page after the last and glides down to it.
    pub(crate) fn add_page(&mut self) {
        let before_page = self.controller.session().page;
        let before = self.controller.session().viewport;
        self.controller.add_page();
        self.glide_from(before_page, before);
    }
    /// After the view jumped from `before` on `before_page` to another page,
    /// starts it where it was and glides to where the jump put it.
    fn glide_from(&mut self, before_page: usize, before: folio_canvas::Viewport) {
        let stack = self.controller.session().page_stack();
        let after_page = self.controller.session().page;
        let target = self.controller.session().viewport;
        let Some(stack) = stack else {
            return;
        };
        if after_page == before_page
            || self.controller.settings.reduce_motion
            || self.controller.interaction.is_some()
            || before.rotation != 0.
            || before.zoom != target.zoom
        {
            return;
        }
        let mut start = stack.viewport(before, before_page, after_page);
        let reach = self
            .canvas_bounds
            .map_or(600., |b| f32::from(b.size.height));
        start.pan.y = target.pan.y + (start.pan.y - target.pan.y).clamp(-reach, reach);
        self.controller.session_mut().viewport = start;
        self.canvas_ease = CanvasEase::default();
        *self.ease_target() = target;
    }
    /// The view eased motion is heading for, starting from the current view
    /// when nothing is moving (or something else moved the view).
    fn ease_target(&mut self) -> &mut folio_canvas::Viewport {
        let current = self.controller.session().viewport;
        let ease = &mut self.canvas_ease;
        if ease.target.is_none() || ease.left.is_some_and(|left| left != current) {
            ease.target = Some(current);
            ease.left = Some(current);
        }
        ease.zoom_anchor = None;
        ease.last.get_or_insert_with(Instant::now);
        ease.target.as_mut().unwrap()
    }
    /// Adds `delta` (a natural logarithm of the factor) to the eased zoom
    /// about `anchor`, anchoring on the document point currently under `anchor`
    /// and never aiming past the zoom limits.
    pub(crate) fn ease_zoom(&mut self, delta: f32, anchor: DocPoint) {
        let current = self.controller.session().viewport;
        let ease = &mut self.canvas_ease;
        let active = ease.target.is_some() && ease.left.is_none_or(|left| left == current);
        let base_zoom = if active {
            ease.target.unwrap().zoom
        } else {
            current.zoom
        };
        let next_zoom = (base_zoom * delta.exp()).clamp(
            folio_canvas::Viewport::MIN_ZOOM,
            folio_canvas::Viewport::MAX_ZOOM,
        );
        let mut target = current;
        if current.zoom > 1e-6 {
            target.zoom_at(next_zoom / current.zoom, anchor);
        }
        ease.target = Some(target);
        ease.left = Some(current);
        ease.zoom_anchor = Some(anchor);
        ease.last.get_or_insert_with(Instant::now);
    }
    fn zoom(&mut self, factor: f32) {
        let anchor = self
            .canvas_bounds
            .map(|b| DocPoint::new(f32::from(b.size.width) / 2., f32::from(b.size.height) / 2.))
            .unwrap_or(DocPoint::new(400., 300.));
        // Buttons and shortcuts ease into place, as wheel zoom does.
        if !self.controller.settings.reduce_motion && self.controller.interaction.is_none() {
            self.ease_zoom(factor.ln(), anchor);
            return;
        }
        self.canvas_ease = CanvasEase::default();
        self.controller
            .session_mut()
            .viewport
            .zoom_at(factor, anchor);
        self.synchronize_page_view_zoom();
    }
    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        if let Some(text) = self.controller.encode_clipboard() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            if cut {
                self.controller.delete_selection();
            }
        }
        cx.notify();
    }
    fn paste(&mut self, window: &Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.canvas_menu {
            if !menu.can_paste {
                return;
            }
            self.canvas_menu = None;
            self.paste_at(Some(menu.position), cx);
        } else {
            // The last hover position is stale once the pointer has left the
            // canvas or the window, or the view has scrolled since; the
            // controller then places pasted content in the visible canvas.
            let hovered = self.pen_in_range || window.is_window_hovered();
            let pointer = self.canvas_bounds.filter(|_| hovered).and_then(|bounds| {
                let position = window.mouse_position();
                bounds.contains(&position).then(|| {
                    self.controller
                        .session()
                        .viewport
                        .to_document(DocPoint::new(
                            f32::from(position.x - bounds.origin.x),
                            f32::from(position.y - bounds.origin.y),
                        ))
                })
            });
            self.controller.cursor = pointer;
            self.paste_at(None, cx);
        }
    }
    fn paste_at(&mut self, position: Option<DocPoint>, cx: &mut Context<Self>) {
        if self.controller.read_only() {
            return;
        }
        cx.read_clipboard(move |this, content, _, _| {
            if this.controller.read_only() {
                return;
            }
            if let Some(image) = content.image() {
                if let Some(position) = position {
                    this.controller.paste_image_at(image.png(), "png", position);
                } else {
                    this.controller.paste_image(image.png(), "png");
                }
                return;
            }
            let text = content.text();
            if !text.is_empty() {
                if let Some(position) = position {
                    this.controller
                        .paste_text_or_objects_at(text.to_owned(), position);
                } else {
                    this.controller.paste_text_or_objects(text.to_owned());
                }
            }
        });
    }
    fn import(&mut self, cx: &mut Context<Self>) {
        let from_home = self.library_open;
        let target = self.controller.active;
        let options = rune::FileDialogOptions {
            title: "Import PDF, image or editable Folio document".into(),
            ..Default::default()
        };
        cx.open_file(options, move |view, path, _, _| {
            let Some(path) = path else { return };
            if from_home {
                let id = view.controller.import_as_note(path);
                if !view.open_tabs.contains(&id) {
                    view.open_tabs.push(id);
                }
                view.show_editor();
            } else {
                view.controller.import_into(target, path);
                view.open_note(target);
            }
        });
    }
    fn export(&mut self, kind: ExportKind, cx: &mut Context<Self>) {
        self.export_open = false;
        let ext = match kind {
            ExportKind::Notebook => "folio",
            ExportKind::Svg => "svg",
            ExportKind::Png => "png",
            ExportKind::Pdf => "pdf",
            ExportKind::Text => "txt",
        };
        let title = self
            .controller
            .session()
            .document
            .metadata
            .title
            .replace(['/', '\\'], "-");
        let suggested = format!("{title}.{ext}");
        let mut snapshot = self.controller.prepare_export();
        if matches!(kind, ExportKind::Pdf | ExportKind::Svg | ExportKind::Png) {
            let theme = Theme::new(&self.controller.settings);
            let appearance = self.export_options.appearance;
            snapshot.map_pages(|page| export_options::apply(page, theme, appearance));
        }
        let snapshot = std::cell::RefCell::new(Some(snapshot));
        let options = rune::FileDialogOptions {
            title: "Export".into(),
            directory: Some(self.controller.data_dir.clone()),
            file_name: Some(suggested),
            ..Default::default()
        };
        cx.save_file(options, move |view, path, _, _| {
            if let (Some(path), Some(snapshot)) = (path, snapshot.borrow_mut().take()) {
                view.controller.export_prepared(snapshot, path, kind);
            }
        });
    }
    /// Files dropped on the window: imported as new documents from the
    /// library, or into the open document.
    pub(crate) fn drop_files(
        &mut self,
        files: Vec<std::path::PathBuf>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if self.library_open {
            for path in files {
                let id = self.controller.import_as_note(path);
                self.open_tabs.push(id);
            }
            self.show_editor();
        } else {
            let target = self.controller.active;
            for path in files {
                self.controller.import_into(target, path);
            }
        }
    }
    /// A pen report from the tablet layer (`tablet.rs`), in view points with adapted force
    /// and tilt.
    /// Whether the canvas point (`x`, `y`) lies on a control drawn over the
    /// canvas: the selection toolbar.
    fn over_canvas_control(&self, x: f32, y: f32) -> bool {
        if self.library_open || !self.selection_toolbar_shown() {
            return false;
        }
        let origin = self.selection_toolbar_origin();
        let (w, h) = (
            f32::from(self.selection_toolbar_size.width),
            f32::from(self.selection_toolbar_size.height),
        );
        x >= origin.x && x <= origin.x + w && y >= origin.y && y <= origin.y + h
    }
    pub(crate) fn tablet(
        &mut self,
        event: &TabletEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.diagnostics.pen(event);
        self.last_tablet = Some(Instant::now());
        self.pen_in_range = !matches!(event.phase, TabletPhase::Leave | TabletPhase::Cancel);
        if self.pen_tool_wheel(event, window, cx) {
            return;
        }
        if self.canvas_input_blocked() || self.controller.loading_note() {
            // A stroke the pen began still ends, though a dialog opened meanwhile.
            if !matches!(
                event.phase,
                TabletPhase::Up | TabletPhase::Cancel | TabletPhase::Leave
            ) || self.controller.interaction.is_none()
            {
                return;
            }
        }
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        if event.phase == TabletPhase::Down && !bounds.contains(&event.position) {
            return;
        }
        // A press on a control over the canvas is that control's (the pen
        // also clicks it as a mouse); fed to the canvas too, it cleared the
        // selection and took the toolbar away before the click landed.
        if event.phase == TabletPhase::Down {
            self.pen_on_control = self.over_canvas_control(
                f32::from(event.position.x - bounds.origin.x),
                f32::from(event.position.y - bounds.origin.y),
            );
        }
        if self.pen_on_control {
            if matches!(event.phase, TabletPhase::Up | TabletPhase::Cancel | TabletPhase::Leave) {
                self.pen_on_control = false;
            }
            return;
        }
        if self.controller.interaction.is_none()
            && self.region_selection.is_none()
            && !bounds.contains(&event.position)
        {
            // Leaving the canvas clears the pen cursor it paints.
            if event.phase == TabletPhase::Hover && self.controller.cursor.is_some() {
                self.controller.pointer(PenEvent {
                    device: Device::Tablet,
                    tool: PenTool::Pen,
                    phase: Phase::Leave,
                    position: DocPoint::new(
                        f32::from(event.position.x - bounds.origin.x),
                        f32::from(event.position.y - bounds.origin.y),
                    ),
                    pressure: 0.,
                    tilt_x: 0.,
                    tilt_y: 0.,
                    buttons: 0,
                    timestamp: self.timestamps.extend(event.timestamp),
                });
                cx.notify();
            }
            return;
        }
        self.mouse_position = event.position;
        let position = DocPoint::new(
            f32::from(event.position.x) - f32::from(bounds.origin.x),
            f32::from(event.position.y) - f32::from(bounds.origin.y),
        );
        let phase = match event.phase {
            TabletPhase::Down => Phase::Down,
            TabletPhase::Move => Phase::Move,
            TabletPhase::Up => Phase::Up,
            TabletPhase::Hover => Phase::Hover,
            TabletPhase::Leave => Phase::Leave,
            TabletPhase::Cancel => Phase::Cancel,
        };
        let input = PenEvent {
            device: Device::Tablet,
            tool: if event.eraser {
                PenTool::Eraser
            } else {
                PenTool::Pen
            },
            phase,
            position,
            pressure: event.pressure,
            tilt_x: event.tilt_x,
            tilt_y: event.tilt_y,
            buttons: event.buttons,
            timestamp: self.timestamps.extend(event.timestamp),
        };
        if phase == Phase::Down {
            self.focus.focus(window);
        }
        if self.region_input(input.phase, input.position, cx) {
            return;
        }
        let was_panning = matches!(self.controller.interaction, Some(Interaction::Pan { .. }));
        self.controller.pointer(input);
        if phase == Phase::Up && was_panning {
            self.synchronize_page_view();
        }
        self.check_text(phase, window, cx);
        cx.notify();
    }
    /// A tablet pad control: express keys run their configured action (or choose a row in
    /// the Writing settings), rings zoom and strips scroll the canvas.
    pub(crate) fn pad(
        &mut self,
        event: &TabletPadEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(check) = &mut self.diagnostics.check {
            check.pad(event);
            cx.notify();
        }
        if self.settings_open {
            self.pad_button_pressed(event, cx);
            return;
        }
        if self.library_open
            || self.blocking_overlay()
            || self.tool_menu.is_some()
            || self.controller.loading_note()
            || self.controller.interaction.is_some()
        {
            return;
        }
        let pad = folio_input::PadInput::default();
        let action = match event.button {
            Some(button) => {
                pad.button(button, event.pressed, &self.controller.settings.pad_buttons)
            }
            None if event.strip => pad.strip(event.delta),
            None => pad.ring(event.delta),
        };
        let Some(action) = action else {
            return;
        };
        match action {
            folio_input::PadAction::Button(slug) => match slug.as_str() {
                "undo" => self.controller.undo(),
                "redo" => self.controller.redo(),
                "wheel" => self.toggle_tool_wheel(self.mouse_position, window, cx),
                "eraser" => self.controller.set_tool(Tool::Eraser),
                "pen" => self.controller.set_tool(Tool::Pen),
                "previous-page" => self.go_to_page(self.controller.session().page.saturating_sub(1)),
                "next-page" => self.go_to_page(
                    (self.controller.session().page + 1).min(
                        self.controller
                            .session()
                            .document
                            .pages
                            .len()
                            .saturating_sub(1),
                    ),
                ),
                _ => {}
            },
            folio_input::PadAction::Scroll(points) => {
                self.controller.session_mut().viewport.pan.y -= points;
                self.synchronize_page_view();
            }
            folio_input::PadAction::Zoom(factor) => {
                let anchor = self
                    .canvas_bounds
                    .map(|b| {
                        DocPoint::new(f32::from(b.size.width) / 2., f32::from(b.size.height) / 2.)
                    })
                    .unwrap_or_default();
                self.canvas_ease = CanvasEase::default();
                self.controller
                    .session_mut()
                    .viewport
                    .zoom_at(factor, anchor);
                self.synchronize_page_view_zoom();
            }
        }
        cx.notify();
    }
    fn mouse(
        &mut self,
        position: Point<Pixels>,
        phase: Phase,
        button: MouseButton,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A pen also moves the pointer for the window's controls; the canvas takes the pen's
        // own reports instead (`tablet`).
        if self.canvas_input_blocked()
            || self.controller.loading_note()
            || self.pen_in_range
            || tablet::pen_active()
            || self
                .last_tablet
                .is_some_and(|t| t.elapsed() < Duration::from_millis(200))
        {
            return;
        }
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        if phase == Phase::Down && !bounds.contains(&position) {
            return;
        }
        if self.controller.interaction.is_none()
            && self.region_selection.is_none()
            && !bounds.contains(&position)
        {
            if phase == Phase::Hover && self.controller.cursor.is_some() {
                self.controller.cursor = None;
                cx.notify();
            }
            return;
        }
        let position = DocPoint::new(
            f32::from(position.x) - f32::from(bounds.origin.x),
            f32::from(position.y) - f32::from(bounds.origin.y),
        );
        if phase == Phase::Down {
            self.focus.focus(window);
            if button == MouseButton::Middle {
                self.controller.finish();
                self.mouse_pan = true;
                let pan = self.controller.session().viewport.pan;
                self.controller.interaction = Some(Interaction::Pan {
                    start: position,
                    pan,
                });
                cx.notify();
                return;
            }
        }
        let event = PenEvent {
            device: Device::Mouse,
            tool: PenTool::Pen,
            phase,
            position,
            pressure: 0.6,
            tilt_x: 0.,
            tilt_y: 0.,
            buttons: 0,
            timestamp: self.start.elapsed().as_millis() as u64,
        };
        let was_panning = matches!(self.controller.interaction, Some(Interaction::Pan { .. }));
        if phase == Phase::Up && self.mouse_pan {
            self.mouse_pan = false;
        }
        if self.region_input(event.phase, event.position, cx) {
            return;
        }
        self.controller.pointer(event);
        if phase == Phase::Up && was_panning {
            self.synchronize_page_view();
        }
        self.check_text(phase, window, cx);
        cx.notify();
    }
    fn check_text(&mut self, phase: Phase, window: &mut Window, cx: &mut Context<Self>) {
        if phase != Phase::Up {
            return;
        }
        if let Some(id) = self.controller.pending_text_edit.take() {
            self.begin_inline_text(id, window, cx);
        }
        if let Some(position) = self.controller.pending_text.take() {
            let id = self.controller.create_text_box(position);
            self.begin_inline_text(id, window, cx);
        }
    }
    /// Whether a touchpad pinch or rotation may move the canvas: not over a dialog or
    /// settings, during a stroke or drag, or while a pen is in range (a palm on the touchpad).
    pub(crate) fn canvas_gesture_allowed(&self) -> bool {
        !(self.canvas_input_blocked()
            || self.modal.is_some()
            || self.settings_open
            || self.controller.loading_note()
            || self.controller.interaction.is_some()
            || self.pen_in_range)
    }
    fn scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.modal.is_some()
            || self.settings_open
            || self.help_open
            || (self.controller.error.is_some() && self.controller.interaction.is_none())
        {
            return;
        }
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        if !bounds.contains(&event.position) || self.controller.interaction.is_some() {
            return;
        }
        let delta = event.delta.pixel_delta(px(30.));
        // Wheel notches ease into place; touchpads already move smoothly and
        // follow the fingers directly.
        let ease =
            matches!(event.delta, ScrollDelta::Lines(_)) && !self.controller.settings.reduce_motion;
        if event.modifiers.control {
            let anchor = DocPoint::new(
                f32::from(event.position.x) - f32::from(bounds.origin.x),
                f32::from(event.position.y) - f32::from(bounds.origin.y),
            );
            // Reversed from the wheel's scroll direction, as requested.
            let zoom = -f32::from(delta.y) * 0.008;
            if ease {
                self.ease_zoom(zoom, anchor);
            } else {
                self.canvas_ease = CanvasEase::default();
                self.controller
                    .session_mut()
                    .viewport
                    .zoom_at(zoom.exp(), anchor);
                self.synchronize_page_view_zoom();
            }
        } else if ease {
            let previous = self.canvas_ease.target.map(|t| t.pan).unwrap_or_else(|| self.controller.session().viewport.pan);
            let mut target = *self.ease_target();
            target.pan.x += f32::from(delta.x);
            target.pan.y += f32::from(delta.y);
            self.controller.clamp_viewport(
                &mut target,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                Some(previous),
            );
            self.canvas_ease.target = Some(target);
        } else {
            let v = &mut self.controller.session_mut().viewport;
            v.pan.x += f32::from(delta.x);
            v.pan.y += f32::from(delta.y);
            self.synchronize_page_view();
        }
        cx.notify();
    }
    /// Advance an eased wheel scroll or zoom of the canvas by one frame.
    fn advance_canvas_ease(&mut self, window: &mut Window) {
        let Some(last) = self.canvas_ease.last else {
            return;
        };
        let current = self.controller.session().viewport;
        let ease = &self.canvas_ease;
        let Some(mut target) = ease.target else {
            self.canvas_ease = CanvasEase::default();
            return;
        };
        let zoom_anchor = ease.zoom_anchor;
        if self.controller.interaction.is_some()
            || self.library_open
            || ease.left.is_some_and(|left| left != current)
        {
            self.canvas_ease = CanvasEase::default();
            return;
        }
        let now = Instant::now();
        let fraction = 1. - (-now.saturating_duration_since(last).as_secs_f32() / 0.045).exp();
        let ratio = target.zoom / current.zoom.max(1e-6);
        let offset = DocPoint::new(target.pan.x - current.pan.x, target.pan.y - current.pan.y);
        let done = target.rotation != current.rotation
            || (ratio.ln().abs() < 0.0005 && offset.x.hypot(offset.y) < 0.5);
        let mut next = current;
        if done {
            next = target;
        } else if let Some(anchor) = zoom_anchor {
            next.zoom_at(ratio.powf(fraction), anchor);
        } else if (ratio - 1.).abs() > 1e-3 && (target.zoom - current.zoom).abs() > 1e-3 {
            // Views with one rotation differ by a zoom about a fixed point:
            // current.pan + current.zoom·d = target.pan + target.zoom·d.
            let dz = target.zoom - current.zoom;
            let fixed = DocPoint::new(
                current.pan.x + current.zoom * (current.pan.x - target.pan.x) / dz,
                current.pan.y + current.zoom * (current.pan.y - target.pan.y) / dz,
            );
            next.zoom_at(ratio.powf(fraction), fixed);
        } else {
            next.zoom = (current.zoom * ratio.powf(fraction)).clamp(
                folio_canvas::Viewport::MIN_ZOOM,
                folio_canvas::Viewport::MAX_ZOOM,
            );
            next.pan.x += offset.x * fraction;
            next.pan.y += offset.y * fraction;
        }
        let page = self.controller.session().page;
        self.controller.session_mut().viewport = next;
        if zoom_anchor.is_some() {
            self.synchronize_page_view_zoom();
        } else {
            self.synchronize_page_view();
        }
        let after = self.controller.session().viewport;
        // Follow the view onto another page's coordinates, and let a scroll
        // limit shorten the motion instead of holding it open.
        let mut expected = next;
        let now_page = self.controller.session().page;
        if now_page != page
            && let Some(stack) = self.controller.session().page_stack()
        {
            target = stack.viewport(target, page, now_page);
            expected = stack.viewport(next, page, now_page);
        }
        target.pan.x += after.pan.x - expected.pan.x;
        target.pan.y += after.pan.y - expected.pan.y;
        if done {
            self.canvas_ease = CanvasEase::default();
        } else {
            self.canvas_ease = CanvasEase {
                target: Some(target),
                left: Some(after),
                last: Some(now),
                zoom_anchor,
            };
            window.request_animation_frame();
        }
    }
    fn synchronize_page_view(&mut self) {
        if let Some(bounds) = self.canvas_bounds {
            self.controller
                .synchronize_page_view(f32::from(bounds.size.width), f32::from(bounds.size.height));
        }
    }
    fn synchronize_page_view_zoom(&mut self) {
        if let Some(bounds) = self.canvas_bounds {
            self.controller.synchronize_page_view_zoom(
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            );
        }
    }
    fn popover(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let anchor_key: String = if self.pen_settings {
            if self.controller.tool == Tool::Eraser {
                "eraser".into()
            } else {
                "pen-options".into()
            }
        } else if self.more_open {
            self.more_anchor.clone()
        } else {
            "more".into()
        };
        let anchor = self.accessibility.control_bounds_for_key(&anchor_key);
        let viewport = window.viewport_size();
        let margin = 8.;
        let width = (286. * self.controller.settings.ui_scale)
            .min((f32::from(viewport.width) - margin * 2.).max(1.));
        let x = anchor
            .map_or(f32::from(viewport.width) - width - margin, |r| r.x0 as f32)
            .clamp(
                margin,
                (f32::from(viewport.width) - width - margin).max(margin),
            );
        let toolbar_bottom = anchor.map_or(84., |r| r.y1 as f32);
        let y = (toolbar_bottom + 4.).min((f32::from(viewport.height) - margin - 1.).max(margin));
        let mut panel = div()
            .occlude()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            // It grows from the middle of the button that opened it.
            .grows_from(
                anchor.map_or(1., |r| ((r.x0 + r.x1) as f32 / 2. - x) / width),
                0.,
            )
            .p_3()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .id("editor-popover")
            .max_h(px((f32::from(viewport.height) - y - 40.).max(1.)))
            .overflow_y_scroll()
            // It springs to the size of each page it turns to.
            .el(|e| e.motion(crate::motion::MORPH).animated_changes())
            // Modal: a press outside, its own toolbar button included, only
            // closes the popover.
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, w, cx| {
                if this.modal_hosted_in(MenuHost::Popover) {
                    this.cancel_modal(w, cx);
                }
                this.dismiss_popovers();
                cx.notify();
            }))
            .flex()
            .flex_col()
            .gap_2();
        if self.modal_hosted_in(MenuHost::Popover) {
            // A dialog chosen here: the popover turns into its page.
            panel = panel.p_1p5().child(self.modal_panel(true, window, cx));
        } else if self.export_open {
            panel = panel.p_1p5().child(self.export_menu(window, cx));
        } else if self.more_open {
            panel = self.document_actions_menu(panel, cx);
        } else if self.pen_settings && self.controller.tool == Tool::Eraser {
            panel = panel.child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("Eraser"))
                .child(self.eraser_controls("popover",cx))
                .child(div().text_xs().child("Whole stroke and Ink segments erase handwriting only. Whole object also deletes text, images, equations, and shapes. Undo restores erased content."));
        } else if self.pen_settings {
            panel = panel.child(self.pen_menu(cx));
        }
        panel.id("popover").overflow_y_scroll()
    }
    /// The open dialog. Most are menus: a popover beside what opened them,
    /// or (`hosted`) the page the menu they were chosen from turns into,
    /// with menu rows for their actions. Confirmations are dialogs over a
    /// scrim.
    fn modal_panel(
        &mut self,
        hosted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let Some((modal, field)) = self.modal.clone() else {
            return div();
        };
        let picker = matches!(modal, Modal::OpenDocument);
        let multiline = matches!(modal, Modal::Recognition);
        let wide = matches!(modal, Modal::OpenDocument | Modal::NewDocument);
        // A press outside or Escape dismisses a menu, so menus carry no
        // close button, Cancel or key hints.
        let menu = self.modal_is_menu();
        field.update(cx, |field, _| {
            field.theme = theme;
            field.compact |= menu;
            // Recognized text reads a little larger, in a box that grows
            // with it from one line.
            if menu && multiline {
                field.text_size = Some(16.);
                field.height = Some(24.);
            }
        });
        if picker {
            return self.document_picker(&field, window, cx);
        }
        // Menu rows under the dialog's contents: its actions.
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut panel = div()
            .flex()
            .flex_col()
            .gap(rems(if menu { 0.5 } else { 1. }))
            // The picker's rows span the menu as other menu rows do.
            .when(menu, |panel| panel.py_1p5().when(!picker, |panel| panel.px_2()))
            .when(!menu, |panel| panel.child(
                // Every dialog closes from its title row, as Settings does.
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(modal.title()),
                    )
                    .child(
                        self.control(
                            "close-modal",
                            "Close",
                            icon(Icon::Close, theme.muted).into_any_element(),
                            false,
                            cx,
                            |this, w, cx| this.cancel_modal(w, cx),
                        )
                        .size(rems(2.))
                        .min_h_0()
                        .p_0()
                        .flex_shrink_0()
                        .bg(transparent_black()),
                    ),
            ))
            .when(matches!(modal, Modal::FontSize | Modal::Color | Modal::MoveNotebook(_) | Modal::MoveDocument(_) | Modal::Crop | Modal::PageBookmark), |panel| {
                panel.child(div().text_sm().text_color(rgb(theme.muted)).child(match modal {
                    Modal::PageBookmark => "Leave it empty to remove the bookmark.",
                    Modal::FontSize => "Font size: 6–180 canvas pixels at 96 dpi. A 24px font exports as 18pt in PDF.",
                    Modal::Color => "Choose a color below, or enter a hex value.",
                    Modal::MoveNotebook(_) | Modal::MoveDocument(_) => "Filter folders by path, choose a destination below, then press Move.",
                    _ => "Left, top, width and height of the original image, as fractions from 0 to 1.",
                }))
            })
            .when(matches!(modal, Modal::NewDocument), |panel| panel.child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Name (optional)")))
            .when(self.modal_field_visible(), |panel| panel.child(field.clone()));
        if matches!(modal, Modal::PageSize) {
            panel = panel.child(self.page_size_options(cx));
        }
        if let Modal::UnsavedChanges = modal {
            panel = panel.child(div().text_sm().child(
                "Autosave is off. Documents with unsaved edits are marked with a dot in their tab.",
            ));
        }
        if let Modal::Confirm(confirm) = &modal {
            let message = match confirm {
                Confirm::DeletePermanently(id) => format!(
                    "“{}” will be deleted permanently. This cannot be undone.",
                    self.controller
                        .notes
                        .iter()
                        .find(|n| n.id == *id)
                        .map_or("This document", |n| n.title.as_str())
                ),
                Confirm::EmptyTrash(folder) => {
                    let count = self
                        .controller
                        .notes
                        .iter()
                        .filter(|n| n.trashed && (folder.is_none() || n.notebook == *folder))
                        .count();
                    format!(
                        "{count} document{} in Trash will be deleted permanently. This cannot be undone.",
                        if count == 1 { "" } else { "s" }
                    )
                }
            };
            panel = panel.child(div().text_sm().child(message));
        }
        if matches!(modal, Modal::MoveNotebook(_) | Modal::MoveDocument(_)) {
            let query = field.read(cx).content().to_lowercase();
            let mut destinations = div()
                .id("folder-picker")
                .max_h(px(280.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            destinations = destinations.child(
                self.button(
                    "folder-destination-root",
                    "Documents (no folder)",
                    self.folder_destination.is_none(),
                    cx,
                    |this, _, _| this.folder_destination = None,
                )
                .justify_start(),
            );
            let details = self
                .controller
                .folder_details(self.controller.notebooks.iter().map(|folder| folder.id));
            for (folder, depth) in self.controller.folder_tree() {
                let destination = folder.id;
                let path = details[&destination].path.clone();
                if !path.to_lowercase().contains(&query) {
                    continue;
                }
                let reason = if let Modal::MoveNotebook(source) = modal {
                    self.controller
                        .validate_folder_move(source, Some(destination))
                        .err()
                } else {
                    None
                };
                let display = if query.is_empty() {
                    folder.name.clone()
                } else {
                    path.clone()
                };
                let label =
                    reason.map_or_else(|| display.clone(), |reason| format!("{display} · {reason}"));
                let indent = if query.is_empty() {
                    12. + depth.min(5) as f32 * 16.
                } else {
                    12.
                };
                destinations = destinations.child(
                    self.button(
                        format!("folder-destination-{destination}"),
                        label,
                        self.folder_destination == Some(destination),
                        cx,
                        move |this, _, _| this.folder_destination = Some(destination),
                    )
                    .justify_start()
                    .pl(px(indent))
                    .w_full(),
                );
            }
            panel = panel.child(destinations);
        }
        if matches!(
            modal,
            Modal::Color | Modal::TextColor | Modal::CanvasColor | Modal::ThemeColor { .. }
        ) {
            let opacity = matches!(
                modal,
                Modal::ThemeColor {
                    token: folio_app::appearance::ThemeToken::Border
                        | folio_app::appearance::ThemeToken::Input,
                    ..
                }
            );
            panel = panel.child(self.color_selector(field.clone(), opacity, cx));
        }
        if matches!(modal, Modal::Templates) {
            panel = panel.child(self.template_picker(cx));
        }
        if matches!(modal, Modal::MovePage) {
            let mut destinations = div()
                .id("move-page-destinations")
                .max_h(px(300.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            for note in self
                .controller
                .notes
                .clone()
                .into_iter()
                .filter(|n| !n.trashed && n.id != self.controller.active)
            {
                let id = note.id;
                destinations = destinations.child(
                    self.button(
                        format!("move-page-{id}"),
                        note.title,
                        self.move_page_destination == Some(id),
                        cx,
                        move |this, _, _| this.move_page_destination = Some(id),
                    )
                    .justify_start()
                    .w_full(),
                );
            }
            panel = panel.child(div().text_sm().text_color(rgb(theme.muted)).child("Choose a destination, then press Move. The destination is saved before removing the source. Undo in the source restores a copy.")).child(destinations);
            if !self
                .controller
                .notes
                .iter()
                .any(|n| !n.trashed && n.id != self.controller.active)
            {
                panel = panel.child("Create another document to move pages into it.");
            }
        }
        if matches!(modal, Modal::NewDocument) {
            panel = panel.child(self.notebook_setup_options(cx));
        }
        if matches!(modal, Modal::Recognition) {
            let math = self.reviewing_math();
            if math {
                panel = panel.child(self.review_preview(field.read(cx).content().clone(), cx));
            }
            // Secondary actions: menu rows in a menu, else outlined buttons
            // in one row (the footer keeps the primary Replace).
            type Action = fn(&mut NotesView, &mut Window, &mut Context<NotesView>);
            let mut secondary: Vec<(&'static str, &'static str, Icon, bool, Action)> = Vec::new();
            if math {
                secondary.push((
                    "edit-recognized-math",
                    if self.review_editing { "Done editing" } else { "Edit LaTeX" },
                    Icon::Pencil,
                    self.review_editing,
                    |this, window, cx| this.toggle_review_editing(window, cx),
                ));
            }
            secondary.push(("copy-recognized-text", if math { "Copy LaTeX" } else { "Copy text" }, Icon::Copy, false, |this, _, cx| {
                if let Some((Modal::Recognition, field)) = &this.modal {
                    cx.write_to_clipboard(ClipboardItem::new_string(field.read(cx).content().clone()));
                    this.controller.status = "Recognized text copied".into();
                }
            }));
            if math {
                secondary.push((
                    "solve-recognized-math",
                    "Solve with steps",
                    Icon::Calculator,
                    false,
                    |this, window, cx| {
                        let text = this
                            .modal
                            .as_ref()
                            .map(|(_, field)| field.read(cx).content().clone())
                            .unwrap_or_default();
                        match this.controller.use_math_review(text) {
                            Ok(()) => {
                                this.clear_math_inputs(window);
                                this.close_modal(window, cx);
                            }
                            Err(error) => {
                                this.modal_error = Some(error);
                                cx.notify();
                            }
                        }
                    },
                ));
            }
            if !math && self.controller.can_index_review() {
                secondary.push((
                    "keep-ink-index",
                    "Keep ink, make searchable",
                    Icon::TextScan,
                    false,
                    |this, w, cx| {
                        let text = this
                            .modal
                            .as_ref()
                            .map(|(_, field)| field.read(cx).content().clone())
                            .unwrap_or_default();
                        match this.controller.keep_ink_and_index(text) {
                            Ok(()) => this.close_modal(w, cx),
                            Err(e) => {
                                this.modal_error = Some(e);
                                cx.notify();
                            }
                        }
                    },
                ));
            }
            let outline = move |b: Stateful<Div>| {
                b.px_3().py_1().min_h(rems(2.)).border_1().border_color(theme.cover_edge())
            };
            let mut actions = div().flex().flex_wrap().gap_2();
            for (id, label, kind, active, action) in secondary {
                if menu {
                    rows.push(
                        self.menu_row(id, label.into(), div().child(label), kind, theme.muted, None, cx, action)
                            .when(active, |row| row.bg(theme.tint(1.)))
                            .into_any_element(),
                    );
                } else {
                    actions = actions.child(outline(self.button(id, label, active, cx, action)));
                }
            }
            if !menu {
            panel = panel.child(actions);
            }
        }
        if let Some(error) = &self.modal_error {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.destructive))
                    .child(error.clone()),
            );
        }
        if let Modal::ThemeColor { dark, token } = modal {
            panel = panel.child(div().text_xs().text_color(rgb(theme.muted)).child(format!(
                "{} · {} appearance",
                token.label(),
                if dark { "Dark" } else { "Light" }
            )));
        }
        // Every dialog spells its keys the same way: keycaps, then what they do.
        let hint: &[(&[&'static str], &'static str)] = match modal {
            Modal::Recognition if self.controller.recognition_for_index => &[(&["Ctrl", "Enter"], "index")],
            Modal::Recognition if !self.modal_field_visible() => &[(&["Esc"], "close")],
            Modal::Recognition => &[(&["Ctrl", "Enter"], "replace")],
            _ if multiline => &[(&["Ctrl", "Enter"], "save")],
            Modal::OpenDocument => &[(&["↑", "↓"], "choose"), (&["Enter"], "open"), (&["Esc"], "close")],
            _ if modal.has_field() => &[(&["Enter"], "confirm"), (&["Esc"], "close")],
            _ => &[(&["Esc"], "close")],
        };
        let mut actions = div().flex().gap_2();
        match &modal {
            Modal::Templates if menu => {}
            Modal::Templates => {
                actions = actions.child(self.button(
                    "cancel-modal",
                    "Close",
                    false,
                    cx,
                    |this, w, cx| this.cancel_modal(w, cx),
                ));
            }
            Modal::UnsavedChanges => {
                actions = actions
                    .child(
                        self.button("cancel-modal", "Cancel", false, cx, |this, w, cx| {
                            this.cancel_modal(w, cx)
                        }),
                    )
                    .child(self.button(
                        "discard-and-close",
                        "Close without saving",
                        false,
                        cx,
                        |this, w, cx| this.close_window(false, w, cx),
                    ))
                    .child(self.button(
                        "save-and-close",
                        "Save and close",
                        true,
                        cx,
                        |this, w, cx| this.close_window(true, w, cx),
                    ));
            }
            _ => {
                actions = actions
                    .when(!menu, |actions| actions.child(
                        self.button("cancel-modal", "Cancel", false, cx, |this, w, cx| {
                            this.cancel_modal(w, cx)
                        }),
                    ))
                    .child(
                        self.button(
                            "submit-modal",
                            if self.equation_draft.is_some() && self.controller.equation_pending {
                                "Rendering…"
                            } else {
                                modal.submit_label(&self.controller)
                            },
                            true,
                            cx,
                            |this, w, cx| this.submit_modal(w, cx),
                        )
                        // The confirming action uses the filled primary style.
                        .bg(rgb(theme.accent))
                        .text_color(rgb(theme.primary_foreground)),
                    );
            }
        }
        if menu {
            let title = modal.title();
            // Its action is the last row, beside the others it has; the
            // picker opens a document from its list instead.
            if !matches!(modal, Modal::Templates | Modal::OpenDocument) {
                let label = if self.equation_draft.is_some() && self.controller.equation_pending {
                    "Rendering…"
                } else {
                    modal.submit_label(&self.controller)
                };
                rows.push(
                    self.menu_row(
                        "submit-modal",
                        label.into(),
                        div().font_weight(FontWeight::MEDIUM).child(label),
                        Icon::Check,
                        theme.ink,
                        None,
                        cx,
                        |this, w, cx| this.submit_modal(w, cx),
                    )
                    .into_any_element(),
                );
            }
            // In a menu its first row leads back to the menu; on its own it
            // names the dialog.
            let header = if hosted {
                self.menu_row(
                    "modal-back",
                    format!("‹ {title}").into(),
                    div().font_weight(FontWeight::SEMIBOLD).child(title),
                    Icon::Back,
                    theme.muted,
                    None,
                    cx,
                    |this, w, cx| this.cancel_modal(w, cx),
                )
                .into_any_element()
            } else {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .min_h(rems(2.))
                    .px_2()
                    .py_1()
                    .border_1()
                    .border_color(transparent_black())
                    .text_sm()
                    // The picker's title stands alone over its list.
                    .when(!picker, |row| {
                        row.child(icon(modal.icon(), theme.muted).size(rems(1.125)))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .into_any_element()
            };
            let estimate = match &modal {
                Modal::OpenDocument => {
                    let rows = ((f32::from(window.viewport_size().height) - 400.) / PICKER_ROW)
                        .floor()
                        .clamp(4., 8.);
                    rows * PICKER_ROW + 150.
                }
                Modal::NewDocument => 440.,
                Modal::Recognition if self.reviewing_math() => 330.,
                Modal::Recognition => 260.,
                Modal::Templates
                | Modal::MovePage
                | Modal::MoveNotebook(_)
                | Modal::MoveDocument(_)
                | Modal::Color
                | Modal::TextColor
                | Modal::CanvasColor
                | Modal::ThemeColor { .. } => 420.,
                Modal::PageSize => 340.,
                _ => 130.,
            };
            let frame = (!hosted).then(|| {
                let width = if picker {
                    340.
                } else if matches!(modal, Modal::NewDocument) {
                    540.
                } else if wide || multiline {
                    420.
                } else {
                    300.
                };
                self.popover_frame(window, width, estimate)
            });
            let room = frame.map_or(
                (f32::from(window.viewport_size().height) / self.controller.settings.ui_scale - 160.)
                    .max(160.),
                |frame| frame.room - 16.,
            );
            let rows_height = 8. + rows.len() as f32 * 33. + 42.;
            let page = div()
                .id(format!("modal-page-{title}"))
                .flex()
                .flex_col()
                .gap(px(1.))
                .min_h_0()
                .child(header)
                .child(document_actions::separator(theme))
                .child(
                    // A block scroll container leaves the text field's
                    // percentage width unresolved when wrapped text follows
                    // it, collapsing the field.
                    div()
                        .id("modal-panel")
                        .max_h(px((room - rows_height).max(100.)))
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .overflow_y_scroll()
                        .child(panel.flex_shrink_0()),
                )
                .when(!rows.is_empty(), |page| {
                    page.child(document_actions::separator(theme)).children(rows)
                });
            let Some(frame) = frame else {
                // The menu's own page slides away as this one slides in.
                return page.page_in(true);
            };
            return frame
                .place(div().id("modal-popover"))
                .occlude()
                .p_1p5()
                .flex()
                .flex_col()
                .rounded(px(theme.corner_md()))
                .glass(theme, theme.popover)
                .shadow_md()
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, w, cx| {
                    this.cancel_modal(w, cx)
                }))
                .child(page);
        }
        let footer = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .items_center()
            .justify_between()
            .pt_4()
            .border_t_1()
            .border_color(theme.border)
            .child({
                let mut keys = div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(rgb(theme.muted));
                for (caps, what) in hint {
                    for cap in caps.iter() {
                        keys = keys.child(keycap(*cap, theme));
                    }
                    keys = keys.child(div().mr_2().child(*what));
                }
                keys
            })
            .child(actions);
        let max_height = (f32::from(window.viewport_size().height)
            / self.controller.settings.ui_scale
            - if picker { 96. } else { 48. })
        .max(240.);
        transitions::Transitions::dialog_layer(div())
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .justify_center()
            // Pickers stay anchored while results load so the query field does
            // not move under the caret.
            .when(picker, |overlay| overlay.items_start().pt(px(64.)))
            .when(!picker, |overlay| overlay.items_center())
            .child(transitions::Transitions::dialog_box(
                &format!("modal-{}", self.modal.as_ref().map_or("", |(m, _)| m.title())),
                div()
                    .flex_shrink_0()
                    .w(px(if wide { 660. } else { 520. }))
                    .max_w_full()
                    .max_h(px(max_height))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .glass_dialog(theme, theme.popover)
                    .rounded(px(theme.corner_lg()))
                    .child(
                        // A block scroll container leaves the text field's
                        // percentage width unresolved when wrapped text
                        // follows it, collapsing the field.
                        div()
                            .id("modal-panel")
                            // Rune's grown children have no basis in an
                            // auto-height column; bound the scroll area by
                            // what the dialog leaves after padding and footer.
                            .max_h(px((max_height - 140.).max(120.)))
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(panel.flex_shrink_0()),
                    )
                    .child(footer),
            ))
    }
    /// Documents listed by the Ctrl+T picker for its current query, most
    /// recent first. Every word must appear in the title or folder path;
    /// titles starting with the query come first.
    fn open_document_matches(&self, cx: &App) -> Vec<folio_document::NoteMetadata> {
        let query = self
            .modal
            .as_ref()
            .map(|(_, field)| field.read(cx).content().trim().to_lowercase())
            .unwrap_or_default();
        let notes = self.controller.notes.iter().filter(|n| !n.trashed);
        if query.is_empty() {
            return notes.cloned().collect();
        }
        let words = query.split_whitespace().collect::<Vec<_>>();
        let folders = self
            .controller
            .folder_details(self.controller.notebooks.iter().map(|folder| folder.id));
        let mut matches = notes
            .filter_map(|n| {
                let title = n.title.to_lowercase();
                let folder = n
                    .notebook
                    .and_then(|f| folders.get(&f))
                    .map(|f| f.path.to_lowercase())
                    .unwrap_or_default();
                words
                    .iter()
                    .all(|w| title.contains(w) || folder.contains(w))
                    .then(|| (!title.starts_with(&query), n.clone()))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(rank, _)| *rank);
        matches.into_iter().map(|(_, n)| n).collect()
    }
    /// Rows of the Ctrl+T picker: each document with its folder, last edit
    /// and whether it is already open, then a row to create a new one.
    /// The Ctrl+T document picker, a command palette: the filter is its
    /// header, the matching documents sit beside a preview of the one
    /// chosen (by the keyboard or the pointer), and key hints run along the
    /// bottom beside New document.
    fn document_picker(
        &mut self,
        field: &Entity<Field>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let query = field.read(cx).content().trim().to_lowercase();
        if query != self.picker_query {
            self.picker_query = query.clone();
            self.modal_cursor = 0;
            self.picker_scroll.set_offset(Point::default());
        }
        field.update(cx, |field, _| {
            field.bare = true;
            field.compact = false;
        });
        let matches = self.open_document_matches(cx);
        let total = self.controller.notes.iter().filter(|n| !n.trashed).count();
        let cursor = self.modal_cursor.min(matches.len().saturating_sub(1));
        // Whole rows of the list fit what the window leaves; the preview
        // needs about ten.
        let rows = ((f32::from(window.viewport_size().height) / self.controller.settings.ui_scale
            - 220.)
            / PICKER_ROW)
            .floor()
            .clamp(6., 10.);
        let room = rows * PICKER_ROW;
        let mut results = div()
            .id("tab-picker-results")
            .track_scroll(&self.picker_scroll)
            .h(px(room))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if matches.is_empty() {
            results = results.child(
                div()
                    .px_2()
                    .py_6()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(rgb(theme.muted))
                    .child(if total == 0 {
                        "No documents yet".to_owned()
                    } else {
                        format!("No documents match “{}”", query)
                    }),
            );
        }
        let selected = matches.get(cursor).cloned();
        for (index, note) in matches.into_iter().enumerate() {
            let id = note.id;
            let chosen = index == cursor;
            let failed = self
                .controller
                .library_imports
                .get(&id)
                .is_some_and(|import| !import.pending);
            let current = id == self.controller.active && self.open_tabs.contains(&id);
            let open = self.open_tabs.contains(&id);
            // Emphasize the matched part of the title when lowercasing keeps
            // its byte offsets.
            let lower = note.title.to_lowercase();
            let highlights = lower
                .find(&query)
                .filter(|_| !query.is_empty() && lower.len() == note.title.len())
                .map(|start| {
                    vec![(
                        start..start + query.len(),
                        HighlightStyle {
                            color: Some(rgb(theme.accent).into()),
                            font_weight: Some(FontWeight::SEMIBOLD),
                            ..Default::default()
                        },
                    )]
                })
                .unwrap_or_default();
            let mut label = format!("Open {}", note.title);
            if failed {
                label.push_str(" · Import failed, open to retry");
            }
            if open {
                label.push_str(" · Open in a tab");
            }
            let status = if current {
                Some("Current")
            } else if open && !failed {
                Some("In a tab")
            } else {
                None
            };
            results = results.child(
                self.control(
                    format!("pick-document-{id}"),
                    label,
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .w_full()
                        .min_w_0()
                        .child(
                            div().flex_1().min_w_0().truncate().child(
                                StyledText::new(note.title.clone()).with_highlights(highlights),
                            ),
                        )
                        .when(failed, |row| {
                            row.child(icon(Icon::Alert, theme.error_text()).size(rems(0.875)))
                        })
                        .when(note.favorite, |row| {
                            row.child(icon(Icon::StarFilled, theme.muted).size(rems(0.875)))
                        })
                        .children(status.map(|status| {
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(rgb(if current { theme.muted } else { theme.secondary() }))
                                .child(status)
                        }))
                        // Only the chosen row says what Enter does.
                        .when(chosen, |row| row.child(keycap("↵", theme).text_xs()))
                        .into_any_element(),
                    chosen,
                    cx,
                    move |this, w, cx| {
                        this.open_note(id);
                        this.close_modal(w, cx);
                    },
                )
                .h(px(PICKER_ROW))
                .min_h_0()
                .px_2()
                .py_0()
                .justify_start()
                .w_full()
                // The chosen row is lit; the pointer chooses what it is over,
                // so rows draw no hover of their own.
                .bg(theme.tint(if chosen { 1. } else { 0. }))
                .hover(|s| s.bg(transparent_black()))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered && this.modal_cursor != index {
                        this.modal_cursor = index;
                        cx.notify();
                    }
                }))
                // Rows keep their height; the list scrolls instead.
                .flex_shrink_0(),
            );
        }
        let preview_width = 210.;
        let preview = div()
            .w(px(preview_width))
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .p_3()
            .flex()
            .flex_col()
            .gap(px(10.))
            .children(selected.map(|note| self.picker_preview(&note, preview_width - 24., room - 110., window, cx)));
        let new = self
            .control(
                "tab-create-note",
                "New document · Ctrl+N",
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .child(icon(Icon::Plus, theme.muted).size(rems(0.875)))
                    .child("New")
                    .child(keycap("Ctrl", theme))
                    .child(keycap("N", theme))
                    .into_any_element(),
                false,
                cx,
                |this, w, cx| this.new_notebook(w, cx),
            )
            .min_h_0()
            .px_1p5()
            .py_0p5()
            .text_xs()
            .text_color(rgb(theme.muted));
        let hint = |caps: &[&'static str], what: &'static str| {
            let mut hint = div().flex().items_center().gap_1();
            for cap in caps {
                hint = hint.child(keycap(*cap, theme));
            }
            hint.child(div().child(what))
        };
        let frame = self.popover_frame(window, 600., room + 104.);
        frame
            .place(div().id("modal-popover"))
            .occlude()
            .flex()
            .flex_col()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .shadow_md()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, w, cx| {
                this.cancel_modal(w, cx)
            }))
            .child(
                div()
                    .h(rems(3.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(theme.border)
                    .text_base()
                    .child(icon(Icon::Search, theme.muted).size(rems(1.)))
                    .child(div().flex_1().min_w_0().child(field.clone())),
            )
            .child(
                div()
                    .flex()
                    .min_h_0()
                    .child(div().flex_1().min_w_0().p_1p5().child(results))
                    .child(preview),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .px_3()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(rgba(0x0000002e))
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(hint(&["↑", "↓"], "choose"))
                    .child(hint(&["↵"], "open"))
                    .child(hint(&["Esc"], "close"))
                    .child(div().flex_1())
                    .child(new),
            )
    }
    /// The chosen document's first page (its cover), name and details for
    /// the picker's preview pane.
    fn picker_preview(
        &mut self,
        note: &folio_document::NoteMetadata,
        width: f32,
        max_height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let id = note.id;
        let scale = self.controller.settings.ui_scale;
        // Over the library the cover is asked for at the size its cards
        // use: a page has one cached thumbnail, and two sizes would each
        // discard the other's every frame.
        let h = if self.library_open {
            self.library_cover_height(window)
        } else {
            max_height.max(120.) * scale
        };
        let w = width * scale;
        let page_corner = (theme.radius - 1.).max(0.);
        let cached = self.cached_cover_at(note, w, h, true, window);
        let preview = cached.is_none().then(|| self.controller.library_preview(id)).flatten();
        let pages = cached
            .as_ref()
            .map(|(_, count)| *count)
            .or_else(|| preview.as_ref().map(|(_, count)| *count));
        let page = if let Some((cover, _)) = cached {
            thumbnails::outlined(cover.shadow_sm(), page_corner, theme.cover_edge())
        } else if let Some((page, page_count)) = &preview {
            if let Some(pdf) = &page.properties.pdf {
                self.controller.request_pdf_background(pdf.clone());
            }
            let frame = self.thumbnail(
                thumbnails::ThumbnailSource::Cover {
                    note: id,
                    updated_at: note.updated_at,
                    cover_page: note.cover_page,
                    page: page.clone(),
                    page_count: *page_count,
                },
                w,
                h,
                window,
                cx,
            );
            thumbnails::outlined(frame.shadow_sm(), page_corner, theme.cover_edge())
        } else {
            div()
                .w_full()
                .h(px(h / scale * 0.75))
                .rounded(px(theme.radius))
                .border_1()
                .border_color(theme.cover_edge())
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(rgb(theme.muted))
                .child(if self.controller.library_cover_unavailable(id) {
                    "Preview unavailable"
                } else {
                    "Loading preview…"
                })
        };
        let folder = note.notebook.map(|folder| self.controller.folder_path(folder));
        let fact = |kind: Icon, text: String| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(icon(kind, theme.muted).size(rems(0.8125)))
                .child(div().min_w_0().truncate().child(text))
        };
        div()
            .id(format!("picker-preview-{id}"))
            .flex()
            .flex_col()
            .gap(px(10.))
            // The chosen document's details are simply there; they follow
            // the selection without an entrance.
            .el(|e| e.instant_changes())
            .child(div().flex().justify_center().child(page))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(rems(1.25))
                    .child(note.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .children(folder.map(|path| fact(Icon::Folder, path)))
                    .children(pages.map(|count| {
                        fact(Icon::Page, format!("{count} page{}", if count == 1 { "" } else { "s" }))
                    }))
                    .child(fact(Icon::Clock, workspace::edited_label(note.updated_at))),
            )
    }
    /// Move the highlighted row of a picker dialog.
    fn navigate_modal(&mut self, down: bool, cx: &mut Context<Self>) {
        let count = match self.modal.as_ref().map(|(m, _)| m) {
            Some(Modal::OpenDocument) => self.open_document_matches(cx).len(),
            _ => return,
        };
        if count == 0 {
            return;
        }
        let current = self.modal_cursor.min(count - 1);
        self.modal_cursor = if down {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        self.picker_scroll.scroll_to(self.modal_cursor);
        cx.notify();
    }
}

/// A keyboard key, as shown in hints.
pub(crate) fn keycap(label: impl Into<SharedString>, theme: Theme) -> Div {
    let label: SharedString = label.into();
    div()
        .min_w(px(18.))
        .px_1()
        .flex()
        .justify_center()
        .rounded(px(theme.corner_sm().max(3.)))
        .border_1()
        .border_color(theme.border)
        .bg(theme.raised())
        .text_color(rgb(theme.ink))
        .child(label)
}

/// Scrolls a viewport to a marked element; here the viewport's key.
#[derive(Clone, Default)]
pub struct ScrollAnchor(ScrollHandle);
impl ScrollAnchor {
    pub fn for_handle(handle: ScrollHandle) -> Self {
        Self(handle)
    }
    pub fn scroll_to(&self, window: &mut Window, _: &mut Context<NotesView>) {
        if !self.0.key.is_empty() {
            window.scroll_into_view(format!("{}-anchor", self.0.key));
        }
    }
}

impl Div {
    /// Marks the element a [`ScrollAnchor`] scrolls to.
    pub fn anchor_scroll(self, anchor: ScrollAnchor) -> Self {
        if anchor.0.key.is_empty() {
            self
        } else {
            self.id(format!("{}-anchor", anchor.0.key))
        }
    }
}
