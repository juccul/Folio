//! GPUI presentation and native input adapter. Document operations are delegated
//! to folio-app; ink geometry remains independent of GPUI.
mod accessibility;
mod diagnostics;
mod document_actions;
use document_actions::MoreSection;
mod input_check;
mod recovery;
pub use recovery::RecoveryView;
mod appearance;
mod canvas_menu;
mod color_picker;
mod controls;
mod field;
mod folder_menu;
mod frame;
mod graph;
mod help;
mod icons;
pub use icons::IconAssets;
mod export_options;
mod ink_layers;
mod inline_text;
mod library_sort;
mod math_panel;
mod motion;
mod navigation;
mod notebook_setup;
mod ocr_setup;
mod page_size;
mod painting;
mod pen_menu;
mod pen_presets;
mod portable;
mod region;
mod selection;
mod settings;
mod templates;
mod text_render;
mod theme;
mod titlebar;
mod toolbar_menu;
use toolbar_menu::ToolMenu;
mod updates;
mod validation;
mod workspace;
mod workspace_state;
use field::{Field, FieldBoundsChanged, Submitted};
use folio_app::{Controller, ExportKind, Interaction, NoteFilter, RecognitionKind, Tool};
use folio_document::{Color, Id, InkTool, Paper, Point as DocPoint};
use folio_input::{Device, PenEvent, Phase, TimestampExtender, Tool as PenTool};
use futures::StreamExt;
use gpui::{prelude::*, *};
use icons::{Icon, icon};
pub use painting::CanvasFrame;
use std::time::{Duration, Instant};
use theme::Theme;

actions!(
    folio,
    [
        Undo,
        Redo,
        Save,
        NewNote,
        NewPage,
        Search,
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
        ResetTitlebarSmoke
    ]
);
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
    Search,
    Color,
    ThemeColor {
        dark: bool,
        token: folio_app::appearance::ThemeToken,
    },
    CanvasColor,
    PageSize,
    Font,
    FontSize,
    TextColor,
    Equation,
    EditEquation(Id),
    MathPdfRegion,
    PadButtons,
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
            Self::Search => "Search",
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
    fn title(&self) -> &'static str {
        match self {
            Self::SaveTemplate => "Save page as template",
            Self::Templates => "Page templates",
            Self::RenameTemplate(_) => "Rename template",
            Self::SavePreset => "Name pen preset",
            Self::RenamePreset(_) => "Rename pen preset",
            Self::PageBookmark => "Name bookmark (empty removes it)",
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
            Self::Search => "Search your documents",
            Self::Color => "Custom ink color",
            Self::TextColor => "Text color",
            Self::ThemeColor { .. } => "Theme color",
            Self::CanvasColor => "Paper color",
            Self::PageSize => "Custom page size",
            Self::Font => "Font family",
            Self::FontSize => "Font size",
            Self::EditEquation(_) => "Edit LaTeX equation",
            Self::MathPdfRegion => "PDF region: left, top, width, height",
            Self::PadButtons => "Pad buttons: comma-separated actions",
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
    poll_task: Option<Task<()>>,
    poll_idle: bool,
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
    color_drag: Option<(EntityId, usize)>,
    color_picker: Option<color_picker::PickerState>,
    inline_text: Option<inline_text::Editor>,
    subscriptions: Vec<Subscription>,
    _appearance_subscription: Subscription,
    motion: std::cell::RefCell<motion::Motion>,
    building_overlay: bool,
    /// The window is closing after an explicit save or discard choice.
    closing: bool,
    settings_open: bool,
    settings_section: settings::Section,
    theme_colors_open: bool,
    modal_error: Option<String>,
    /// Highlighted row of the search or open-document picker.
    modal_cursor: usize,
    move_page_destination: Option<Id>,
    /// Menu control to focus once its menu has been built.
    menu_focus: Option<String>,
    /// Focused element missing from the last rendered frame.
    lost_focus: Option<FocusHandle>,
    /// Which toolbar control the Document actions popover opens from.
    more_anchor: String,
    attention_error: Option<String>,
    attention_setup: bool,
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
    folder_menu: Option<(Id, Point<Pixels>)>,
    folder_rows: Vec<Id>,
    folder_scroll: UniformListScrollHandle,
    canvas_menu: Option<canvas_menu::CanvasMenu>,
    thumbnails: workspace::Thumbnails,
    writing_style: Option<folio_document::PenStyle>,
    timestamps: TimestampExtender,
    last_tablet: Option<Instant>,
    pen_in_range: bool,
    gesture_start: Option<folio_canvas::Viewport>,
    mouse_pan: bool,
    start: Instant,
    painter: painting::Painter,
    accessibility: std::rc::Rc<accessibility::Accessibility>,
}
impl NotesView {
    pub fn new(controller: Controller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_titlebar_fixture(controller, window, cx, false)
    }
    /// Isolated real-pointer fixture: no update worker or installed update cache is opened.
    pub fn new_titlebar_smoke(
        controller: Controller,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self::new_with_titlebar_fixture(controller, window, cx, true);
        view.update_smoke_phase(0);
        view
    }
    fn new_with_titlebar_fixture(
        mut controller: Controller,
        window: &mut Window,
        cx: &mut Context<Self>,
        titlebar_smoke: bool,
    ) -> Self {
        controller.apply_system_theme(matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ));
        let entity = cx.entity().downgrade();
        let appearance_subscription = window.observe_window_appearance(move |window, cx| {
            let _ = entity.update(cx, |view, cx| {
                if view.controller.apply_system_theme(matches!(
                    window.appearance(),
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                )) {
                    cx.notify();
                }
            });
        });
        let focus = cx.focus_handle();
        focus.focus(window);
        let entity = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            entity
                .update(cx, |view, cx| {
                    if view.update_preparing {
                        return false;
                    }
                    if view.closing {
                        return true;
                    }
                    view.commit_inline_text(cx);
                    if view.needs_close_prompt() {
                        view.modal(Modal::UnsavedChanges, window, cx);
                        return false;
                    }
                    view.store_workspace();
                    match view.controller.flush() {
                        Ok(()) => true,
                        Err(e) => {
                            view.controller.error = Some(e);
                            cx.notify();
                            false
                        }
                    }
                })
                .unwrap_or(true)
        });
        let mut workspace = controller.settings.workspace.clone();
        workspace
            .open_tabs
            .retain(|id| controller.notes.iter().any(|n| n.id == *id));
        if !controller.settings.reopen_documents {
            workspace.open_tabs.clear();
            workspace.library_open = true;
        }
        let (accessibility, mut accessibility_requests) = accessibility::Accessibility::new(window);
        let mut view = Self {
            updater: if titlebar_smoke {
                folio_update::Updater::disabled()
            } else {
                folio_update::Updater::new()
            },
            titlebar_smoke,
            update_preparing: false,
            poll_task: None,
            poll_idle: false,
            region_selection: None,
            controller,
            math_inputs: None,
            diagnostics: diagnostics::Diagnostics::new(),
            canvas_bounds: None,
            canvas_document: None,
            focus,
            modal: None,
            equation_draft: None,
            folder_destination: None,
            page_size_unit: page_size::Unit::default(),
            notebook_setup: None,
            color_drag: None,
            color_picker: None,
            inline_text: None,
            subscriptions: vec![],
            _appearance_subscription: appearance_subscription,
            motion: Default::default(),
            building_overlay: false,
            closing: false,
            settings_open: false,
            settings_section: settings::Section::default(),
            theme_colors_open: false,
            modal_error: None,
            modal_cursor: 0,
            move_page_destination: None,
            menu_focus: None,
            lost_focus: None,
            more_anchor: "more".into(),
            attention_error: None,
            attention_setup: false,
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
            library_open: workspace.library_open,
            pages_open: workspace.pages_open,
            list_view: workspace.list_view,
            list_forced: false,
            sort_by_name: workspace.sort_by_name,
            sort_reverse: workspace.sort_reverse,
            sort_open: false,
            open_tabs: workspace.open_tabs,
            tab_scroll: ScrollHandle::new(),
            tab_target: None,
            document_menu: None,
            folder_menu: None,
            folder_rows: Vec::new(),
            folder_scroll: UniformListScrollHandle::new(),
            canvas_menu: None,
            thumbnails: workspace::Thumbnails::default(),
            writing_style: None,
            timestamps: TimestampExtender::default(),
            last_tablet: None,
            pen_in_range: false,
            gesture_start: None,
            mouse_pan: false,
            start: Instant::now(),
            painter: painting::Painter::default(),
            accessibility: std::rc::Rc::new(accessibility),
        };
        cx.spawn_in(window, async move |view, cx| {
            while let Some(request) = accessibility_requests.next().await {
                if view
                    .update_in(cx, |view, window, cx| {
                        view.accessibility_action(request, window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        view.subscriptions.push(cx.observe_self(|view, cx| {
            // Input and background render notifications cancel the idle timer,
            // so newly started animations/work never wait for the idle interval.
            if view.poll_idle {
                view.schedule_poll(Duration::from_millis(16), cx);
            }
        }));
        view.schedule_poll(Duration::from_millis(16), cx);
        view
    }
    fn accessibility_action(
        &mut self,
        request: accesskit::ActionRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let accessibility = self.accessibility.clone();
        if request.target_node == accesskit::NodeId(3) {
            if let Some((_, field)) = &self.modal {
                if request.action == accesskit::Action::Focus {
                    field.read(cx).focus.focus(window);
                }
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    field.update(cx, |field, cx| {
                        field.set_accessible_content(value.into(), cx)
                    });
                }
            }
        } else if request.target_node == accesskit::NodeId(13) {
            if let Some(editor) = &self.inline_text {
                if request.action == accesskit::Action::Focus {
                    editor.field.read(cx).focus.focus(window);
                }
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    editor.field.update(cx, |field, cx| {
                        field.set_accessible_content(value.into(), cx)
                    });
                }
            }
        } else if request.target_node == accesskit::NodeId(12) {
            if let Some(setup) = &self.notebook_setup {
                if request.action == accesskit::Action::Focus {
                    setup.color.read(cx).focus.focus(window);
                }
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    setup.color.update(cx, |field, cx| {
                        field.set_accessible_content(value.into(), cx)
                    });
                }
            }
        } else if ((5..=8).contains(&request.target_node.0) || request.target_node.0 == 11)
            && self.modal.is_none()
            && !self.blocking_overlay()
        {
            if let Some(inputs) = &self.math_inputs {
                let fields = [
                    &inputs.expression,
                    &inputs.variable,
                    &inputs.next_line,
                    &inputs.range,
                ];
                let field = if request.target_node.0 == 11 {
                    &inputs.result_latex
                } else {
                    fields[(request.target_node.0 - 5) as usize]
                };
                if request.action == accesskit::Action::Focus {
                    field.read(cx).focus.focus(window);
                }
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    field.update(cx, |field, cx| {
                        field.set_accessible_content(value.into(), cx)
                    });
                }
            }
        } else if let Some((callback, focus)) = accessibility.action(request.target_node) {
            match request.action {
                accesskit::Action::Click => callback(self, window, cx),
                accesskit::Action::Focus => focus.focus(window),
                _ => {}
            }
        }
    }
    fn schedule_poll(&mut self, delay: Duration, cx: &mut Context<Self>) {
        self.poll_idle = delay > Duration::from_millis(16);
        // Replacing the handle cancels the previous timer rather than creating
        // a second poll loop. The weak entity stops polling when the view closes.
        self.poll_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(delay).await;
            let _ = view.update(cx, |view, cx| {
                if let Some(task) = view.poll_task.take() {
                    task.detach();
                }
                let changed = !view.update_preparing && view.controller.tick();
                let update_changed = view.updater.poll();
                if view.updater.state == folio_update::State::Exit {
                    view.update_preparing = false;
                    cx.quit();
                    return;
                }
                if matches!(view.updater.state, folio_update::State::Failed { .. }) {
                    view.update_preparing = false;
                }
                let motion_changed = view.motion.borrow_mut().advance();
                let notify = changed || update_changed || motion_changed;
                let fast = notify
                    || view.controller.needs_fast_poll()
                    || view.motion.borrow().needs_frame()
                    || view.updater.state.busy()
                    || view.updater.state == folio_update::State::Checking;
                let delay = Duration::from_millis(if fast { 16 } else { 250 });
                let delay = view
                    .controller
                    .math_ink_poll_delay()
                    .map_or(delay, |deadline| delay.min(deadline));
                view.schedule_poll(delay, cx);
                if notify {
                    cx.notify();
                }
            });
        }));
    }
    fn resume_poll(&mut self, cx: &mut Context<Self>) {
        // Rendering/layout can start work after the latest maintenance tick
        // has selected the idle interval, without another entity notification.
        if self.poll_idle
            && (self.controller.needs_fast_poll() || self.motion.borrow().needs_frame())
        {
            self.schedule_poll(Duration::from_millis(16), cx);
        }
    }
    pub fn bindings(cx: &mut App) {
        Field::bindings(cx);
        cx.bind_keys([
            KeyBinding::new("tab", FocusNext, None),
            KeyBinding::new("shift-tab", FocusPrevious, None),
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
        ]);
        cx.bind_keys([
            KeyBinding::new("ctrl-z", Undo, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-z", Redo, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-y", Redo, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-s", Save, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-n", NewNote, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-n", NewPage, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-f", Search, Some("Folio && !FolioField")),
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
            KeyBinding::new("ctrl-=", ZoomIn, Some("Folio && !FolioField")),
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
        ]);
    }
    pub fn titlebar_smoke_bindings(cx: &mut App) {
        cx.bind_keys([KeyBinding::new("ctrl-alt-u", ResetTitlebarSmoke, None)]);
    }
    fn blocking_overlay(&self) -> bool {
        self.update_preparing
            || self.modal.is_some()
            || self.controller.recognition_setup_needed()
            || self.document_menu.is_some()
            || self.folder_menu.is_some()
            || self.canvas_menu.is_some()
            || self.settings_open
            || self.help_open
            || (self.controller.error.is_some() && self.controller.interaction.is_none())
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
        self.help_open = true;
        self.focus.focus(window);
    }
    fn dismiss_popovers(&mut self) {
        self.pen_menu_section = pen_menu::Section::Pen;
        self.tool_menu = None;
        self.folder_menu = None;
        self.canvas_menu = None;
        self.sort_open = false;
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
        self.notebook_setup = None;
        self.subscriptions.clear();
        if let Some(editor) = &self.inline_text {
            editor.field.read(cx).focus.focus(window);
        } else {
            self.focus.focus(window);
        }
        cx.notify();
    }
    fn modal(&mut self, modal: Modal, window: &mut Window, cx: &mut Context<Self>) {
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
            Modal::PadButtons => self.controller.settings.pad_buttons.join(", "),
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
            Modal::Search => self.controller.search_query.clone(),
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
            Modal::Font | Modal::FontSize | Modal::TextColor => self
                .controller
                .session()
                .selection
                .iter()
                .find_map(|id| {
                    if let Some(folio_document::Object::Text(text)) =
                        self.controller.page().objects.get(id).map(|o| o.as_ref())
                    {
                        Some(match modal {
                            Modal::Font => text.font_family.clone(),
                            Modal::FontSize => text.font_size.to_string(),
                            _ => text.color.hex(),
                        })
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        let multiline = matches!(modal, Modal::Recognition);
        let secret = matches!(modal, Modal::PdfPassword(..));
        let placeholder = match modal {
            Modal::Search => Some("Search titles, tags, text and handwriting"),
            Modal::OpenDocument => Some("Filter documents by name"),
            Modal::MoveNotebook(_) | Modal::MoveDocument(_) => Some("Filter folders"),
            Modal::NewDocument => Some(""),
            Modal::Notebook(_) => Some("Folder name"),
            Modal::Tags | Modal::DocumentTags(_) => Some("Comma-separated tags"),
            Modal::PdfPassword(..) => Some("Password"),
            _ => None,
        };
        let field = cx.new(|cx| {
            let mut field = Field::new(content, multiline, cx);
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
        self.move_page_destination = None;
        // Dialogs without a text field keep keyboard focus on their first
        // control, or on the root for Escape.
        if modal.has_field() {
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
        self.subscriptions.clear();
        self.observe_notebook_setup(window, cx);
        self.subscriptions.push(cx.observe(&field, |this, _, cx| {
            this.modal_error = None;
            cx.notify();
        }));
        self.subscriptions.push(
            cx.subscribe(&field, |this, _, navigate: &field::Navigate, cx| {
                this.navigate_modal(navigate.0, cx)
            }),
        );
        self.subscriptions
            .push(cx.subscribe(&field, |_, _, _: &FieldBoundsChanged, cx| cx.notify()));
        self.subscriptions.push(cx.subscribe_in(
            &field,
            window,
            |this, _, _: &Submitted, window, cx| this.submit_modal(window, cx),
        ));
        self.modal_error = None;
        self.clear_color_picker(window);
        self.modal = Some((modal, field));
        cx.notify();
    }
    fn submit_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((modal, field)) = self.modal.as_ref() else {
            return;
        };
        let modal = modal.clone();
        let field = field.clone();
        let content = field.read(cx).content.clone();
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
            Modal::MathPdfRegion => {
                let values = content
                    .split(',')
                    .map(str::trim)
                    .map(str::parse::<f32>)
                    .collect::<Result<Vec<_>, _>>();
                let result = values
                    .map_err(|_| "Enter four comma-separated fractions".to_string())
                    .and_then(|values| {
                        if values.len() != 4 {
                            return Err("Enter left, top, width and height".into());
                        }
                        this_region(&mut self.controller, &values)
                    });
                if let Err(error) = result {
                    self.modal_error = Some(error);
                    cx.notify();
                    return;
                }
            }
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
            Modal::PadButtons => {
                self.controller.settings.pad_buttons =
                    content.split(',').map(|s| s.trim().to_string()).collect()
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
            Modal::Search => {
                // Enter runs a new query, then opens the highlighted result.
                let results = &self.controller.search_results;
                if content.trim() == self.controller.search_query
                    && self.controller.search_state == folio_app::SearchState::Complete
                    && !results.is_empty()
                {
                    let result = &results[self.modal_cursor.min(results.len() - 1)];
                    let (note, page) = (result.note, result.page);
                    self.controller.navigate_search(note, page);
                    self.show_editor();
                } else {
                    self.modal_cursor = 0;
                    self.controller.search(content);
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
            }
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
            Modal::Font => {
                if content.trim().is_empty() {
                    self.modal_error = Some("Enter a font family, such as sans-serif.".into());
                    field.read(cx).focus.focus(window);
                    cx.notify();
                    return;
                }
                let ids = self.controller.session().selection.clone();
                for id in ids {
                    let name = content.clone();
                    self.controller.edit_text(id, |t| t.font_family = name);
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
        if let Some(bounds) = self.canvas_bounds {
            let props = self.controller.page().properties.clone();
            self.controller.session_mut().viewport.fit(
                &props,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            );
        }
    }
    fn zoom(&mut self, factor: f32) {
        let anchor = self
            .canvas_bounds
            .map(|b| DocPoint::new(f32::from(b.size.width) / 2., f32::from(b.size.height) / 2.))
            .unwrap_or(DocPoint::new(400., 300.));
        self.controller
            .session_mut()
            .viewport
            .zoom_at(factor, anchor);
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
    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.canvas_menu {
            if !menu.can_paste {
                return;
            }
            self.canvas_menu = None;
            self.paste_at(Some(menu.position), cx);
        } else {
            self.paste_at(None, cx);
        }
    }
    fn paste_at(&mut self, position: Option<DocPoint>, cx: &mut Context<Self>) {
        if self.controller.read_only() {
            return;
        }
        if let Some(item) = cx.read_from_clipboard() {
            let mut pasted = false;
            for entry in item.entries() {
                if let ClipboardEntry::Image(image) = entry {
                    let ext = match image.format() {
                        ImageFormat::Png => Some("png"),
                        ImageFormat::Jpeg => Some("jpg"),
                        ImageFormat::Webp => Some("webp"),
                        _ => None,
                    };
                    if let Some(ext) = ext {
                        if let Some(position) = position {
                            self.controller.paste_image_at(image.bytes(), ext, position);
                        } else {
                            self.controller.paste_image(image.bytes(), ext);
                        }
                        pasted = true;
                        break;
                    }
                }
            }
            if !pasted && let Some(text) = item.text() {
                if let Some(position) = position {
                    self.controller.paste_text_or_objects_at(text, position);
                } else {
                    self.controller.paste_text_or_objects(text);
                }
            }
        }
        cx.notify();
    }
    fn import(&mut self, cx: &mut Context<Self>) {
        let from_home = self.library_open;
        let target = self.controller.active;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import PDF, image or editable Folio document".into()),
        });
        cx.spawn(async move |view, cx| match paths.await {
            Ok(Ok(Some(paths))) => {
                let _ = view.update(cx, |view, cx| {
                    for path in paths {
                        if from_home {
                            let id = view.controller.import_as_note(path);
                            if !view.open_tabs.contains(&id) {
                                view.open_tabs.push(id);
                            }
                        } else {
                            view.controller.import_into(target, path);
                        }
                    }
                    if !from_home {
                        view.open_note(target);
                    } else {
                        view.show_editor();
                    }
                    cx.notify();
                });
            }
            Ok(Err(e)) => {
                let _ = view.update(cx, |v, cx| {
                    v.controller.error = Some(e.to_string());
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
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
        let path = cx.prompt_for_new_path(&self.controller.data_dir, Some(&suggested));
        cx.spawn(async move |view, cx| match path.await {
            Ok(Ok(Some(path))) => {
                let _ = view.update(cx, |view, cx| {
                    view.controller.export_prepared(snapshot, path, kind);
                    cx.notify();
                });
            }
            Ok(Err(e)) => {
                let _ = view.update(cx, |v, cx| {
                    v.controller.error = Some(e.to_string());
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
        cx.notify();
    }
    pub fn tablet(&mut self, event: &TabletEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.diagnostics.pen(event);
        self.last_tablet = Some(Instant::now());
        self.pen_in_range = !matches!(event.phase, TabletPhase::Leave | TabletPhase::Cancel);
        if self.library_open
            || self.blocking_overlay()
            || self.tool_menu.is_some()
            || self.modal.is_some()
            || self.settings_open
            || self.help_open
            || (self.controller.error.is_some() && self.controller.interaction.is_none())
        {
            return;
        }
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        if event.phase == TabletPhase::Down && !bounds.contains(&event.position) {
            return;
        }
        if self.controller.interaction.is_none()
            && self.region_selection.is_none()
            && !bounds.contains(&event.position)
        {
            return;
        }
        self.last_tablet = Some(Instant::now());
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
    fn mouse(
        &mut self,
        position: Point<Pixels>,
        phase: Phase,
        button: MouseButton,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.library_open
            || self.blocking_overlay()
            || self.tool_menu.is_some()
            || self.modal.is_some()
            || self.settings_open
            || self.help_open
            || (self.controller.error.is_some() && self.controller.interaction.is_none())
            || self.controller.loading_note()
            || self.pen_in_range
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
    fn pad(&mut self, event: &TabletPadEvent, cx: &mut Context<Self>) {
        if let Some(check) = &mut self.diagnostics.check {
            check.pad(event);
            cx.notify();
        }
        if self.library_open
            || self.blocking_overlay()
            || self.tool_menu.is_some()
            || self.controller.loading_note()
            || self.controller.interaction.is_some()
        {
            return;
        }
        if let Some(button) = event.button {
            if !event.pressed {
                return;
            }
            let action = self
                .controller
                .settings
                .pad_buttons
                .get(button as usize)
                .cloned()
                .unwrap_or_default();
            match action.as_str() {
                "undo" => self.controller.undo(),
                "redo" => self.controller.redo(),
                "eraser" => self.controller.set_tool(Tool::Eraser),
                "pen" => self.controller.set_tool(Tool::Pen),
                "previous-page" => self
                    .controller
                    .change_page(self.controller.session().page.saturating_sub(1)),
                "next-page" => self.controller.change_page(
                    (self.controller.session().page + 1)
                        .min(self.controller.session().document.pages.len() - 1),
                ),
                _ => {}
            }
        } else if event.strip {
            self.controller.session_mut().viewport.pan.y -= event.delta * 800.;
        } else {
            let anchor = self
                .canvas_bounds
                .map(|b| DocPoint::new(f32::from(b.size.width) / 2., f32::from(b.size.height) / 2.))
                .unwrap_or_default();
            self.controller
                .session_mut()
                .viewport
                .zoom_at((event.delta * 0.01).exp(), anchor);
        }
        if event.button.is_none() {
            self.synchronize_page_view();
        }
        cx.notify();
    }
    fn gesture(&mut self, event: &NavigationGesture, cx: &mut Context<Self>) {
        if self.modal.is_some()
            || self.settings_open
            || self.controller.interaction.is_some()
            || self.controller.loading_note()
            || self.pen_in_range
        {
            return;
        }
        let Some(bounds) = self.canvas_bounds else {
            return;
        };
        if !bounds.contains(&event.position) && self.gesture_start.is_none() {
            return;
        }
        match event.phase {
            TouchPhase::Started => self.gesture_start = Some(self.controller.session().viewport),
            TouchPhase::Moved => {
                if self.gesture_start.is_none() {
                    return;
                }
                let anchor = DocPoint::new(
                    f32::from(event.position.x - bounds.origin.x),
                    f32::from(event.position.y - bounds.origin.y),
                );
                let viewport = &mut self.controller.session_mut().viewport;
                let original = viewport.to_document(anchor);
                viewport.zoom_at(event.scale, anchor);
                viewport.rotation += event.rotation;
                let after = viewport.to_screen(original);
                viewport.pan.x += anchor.x - after.x + f32::from(event.translation.x);
                viewport.pan.y += anchor.y - after.y + f32::from(event.translation.y);
            }
            TouchPhase::Ended => {
                if event.cancelled
                    && let Some(original) = self.gesture_start
                {
                    self.controller.session_mut().viewport = original;
                }
                self.gesture_start = None;
                self.synchronize_page_view();
            }
        }
        cx.notify();
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
        if event.modifiers.control {
            let anchor = DocPoint::new(
                f32::from(event.position.x) - f32::from(bounds.origin.x),
                f32::from(event.position.y) - f32::from(bounds.origin.y),
            );
            self.controller
                .session_mut()
                .viewport
                .zoom_at((f32::from(delta.y) * 0.008).exp(), anchor);
        } else {
            let v = &mut self.controller.session_mut().viewport;
            v.pan.x += f32::from(delta.x);
            v.pan.y += f32::from(delta.y);
        }
        self.synchronize_page_view();
        cx.notify();
    }
    fn synchronize_page_view(&mut self) {
        if let Some(bounds) = self.canvas_bounds {
            self.controller
                .synchronize_page_view(f32::from(bounds.size.width), f32::from(bounds.size.height));
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
            .p_3()
            .rounded(px(theme.radius))
            .border_1()
            .border_color(theme.border)
            .bg(rgb(theme.popover))
            .shadow_sm()
            .id("editor-popover")
            .max_h(px((f32::from(viewport.height) - y - 40.).max(1.)))
            .overflow_y_scroll()
            .on_mouse_down_out(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                // Toolbar anchors handle their own toggles. Closing first would
                // make the same click immediately reopen the popover.
                if this
                    .accessibility
                    .control_bounds_for_key(&anchor_key)
                    .is_some_and(|r| {
                        let x = f32::from(event.position.x) as f64;
                        let y = f32::from(event.position.y) as f64;
                        x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1
                    })
                {
                    return;
                }
                this.dismiss_popovers();
                if f32::from(event.position.y) >= toolbar_bottom {
                    cx.stop_propagation();
                }
                cx.notify();
            }))
            .flex()
            .flex_col()
            .gap_2();
        if self.export_open {
            panel = panel.child(self.export_appearance(window, cx));
            panel = panel.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Export"),
            );
            for (id, label, kind) in [
                ("export-pdf", "PDF · all pages", ExportKind::Pdf),
                (
                    "export-notebook",
                    "Editable document · with assets",
                    ExportKind::Notebook,
                ),
                ("export-svg", "SVG · current page", ExportKind::Svg),
                ("export-png", "PNG · current page", ExportKind::Png),
                (
                    "export-text",
                    "Plain text · typed content",
                    ExportKind::Text,
                ),
            ] {
                panel = panel.child(
                    self.button(id, label, false, cx, move |this, _, cx| {
                        this.export(kind, cx)
                    })
                    .justify_start(),
                );
            }
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
    fn modal_panel(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let Some((modal, field)) = self.modal.clone() else {
            return div();
        };
        field.update(cx, |field, _| field.theme = theme);
        let search = matches!(modal, Modal::Search);
        let picker = matches!(modal, Modal::Search | Modal::OpenDocument);
        let multiline = matches!(modal, Modal::Recognition);
        let wide = picker || matches!(modal, Modal::NewDocument);
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(modal.title()),
            )
            .when(matches!(modal, Modal::FontSize | Modal::Color | Modal::MoveNotebook(_) | Modal::MoveDocument(_) | Modal::Crop), |panel| {
                panel.child(div().text_sm().text_color(rgb(theme.muted)).child(match modal {
                    Modal::FontSize => "Font size: 6–180 canvas pixels at 96 dpi. A 24px font exports as 18pt in PDF.",
                    Modal::Color => "Choose a color below, or enter a hex value.",
                    Modal::MoveNotebook(_) | Modal::MoveDocument(_) => "Filter folders by path, choose a destination below, then press Move.",
                    _ => "Left, top, width and height as fractions from 0 to 1.",
                }))
            })
            .when(matches!(modal, Modal::NewDocument), |panel| panel.child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Name (optional)")))
            .when(modal.has_field(), |panel| panel.child(field.clone()));
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
            let query = field.read(cx).content.to_lowercase();
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
            for (folder, _) in self.controller.folder_tree() {
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
                let label =
                    reason.map_or_else(|| path.clone(), |reason| format!("{path} · {reason}"));
                destinations = destinations.child(
                    self.button(
                        format!("folder-destination-{destination}"),
                        label,
                        self.folder_destination == Some(destination),
                        cx,
                        move |this, _, _| this.folder_destination = Some(destination),
                    )
                    .justify_start()
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
            let math = self
                .controller
                .recognition_review
                .as_ref()
                .is_some_and(|r| r.kind == RecognitionKind::Math);
            panel = panel.child(div().text_sm().text_color(rgb(theme.muted)).child(if math {
                "Check and edit the LaTeX. Copy keeps your ink; replacement inserts a rendered, editable equation."
            } else {
                "Review and correct the text. Make searchable attaches it to the original ink; replacement inserts typed text. Both can be undone."
            })).child(self.button("copy-recognized-text", "Copy text", false, cx, |this, _, cx| {
                if let Some((Modal::Recognition, field)) = &this.modal {
                    cx.write_to_clipboard(ClipboardItem::new_string(field.read(cx).content.clone()));
                    this.controller.status = "Recognized text copied".into();
                }
            }));
            if !math && self.controller.can_index_review() {
                panel = panel.child(self.button(
                    "keep-ink-index",
                    "Keep ink and make searchable",
                    false,
                    cx,
                    |this, w, cx| {
                        let text = this
                            .modal
                            .as_ref()
                            .map(|(_, field)| field.read(cx).content.clone())
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
            if math {
                panel = panel.child(self.button(
                    "solve-recognized-math",
                    "Solve with steps",
                    false,
                    cx,
                    |this, window, cx| {
                        let text = this
                            .modal
                            .as_ref()
                            .map(|(_, field)| field.read(cx).content.clone())
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
        if matches!(modal, Modal::OpenDocument) {
            panel = panel.child(
                self.control(
                    "tab-create-note",
                    "Create new document",
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(Icon::Plus, theme.ink).size(rems(1.)))
                        .child("Create new document")
                        .into_any_element(),
                    false,
                    cx,
                    |this, w, cx| this.new_notebook(w, cx),
                )
                .justify_start(),
            );
            let mut results = div()
                .id("tab-picker-results")
                .max_h(px(340.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            let details = self
                .controller
                .folder_details(self.controller.notebooks.iter().map(|folder| folder.id));
            let matches = self.open_document_matches(cx);
            let cursor = self.modal_cursor.min(matches.len().saturating_sub(1));
            if matches.is_empty() {
                results = results.child(
                    div()
                        .p_3()
                        .text_sm()
                        .text_color(rgb(theme.muted))
                        .child("No matching documents"),
                );
            }
            for (index, note) in matches.into_iter().enumerate() {
                let id = note.id;
                let mut detail = vec![workspace::edited_label(note.updated_at)];
                if let Some(folder) = note.notebook.and_then(|f| details.get(&f)) {
                    detail.insert(0, folder.path.clone());
                }
                if self.open_tabs.contains(&id) {
                    detail.push("Open in a tab".into());
                }
                let detail = detail.join(" · ");
                results = results.child(
                    self.control(
                        format!("pick-document-{id}"),
                        format!("Open {}", note.title),
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .min_w_0()
                            .child(div().truncate().child(note.title.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.muted))
                                    .truncate()
                                    .child(detail),
                            )
                            .into_any_element(),
                        index == cursor,
                        cx,
                        move |this, w, cx| {
                            this.open_note(id);
                            this.close_modal(w, cx);
                        },
                    )
                    .justify_start()
                    .w_full(),
                );
            }
            panel = panel.child(results);
        }
        if search {
            let mut results = div()
                .id("search-results")
                .max_h(px(360.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            let message = match &self.controller.search_state {
                folio_app::SearchState::Idle => {
                    "Press Enter to search titles, tags, typed text, equations and indexed handwriting."
                        .to_owned()
                }
                folio_app::SearchState::Searching => {
                    format!("Searching for “{}”…", self.controller.search_query)
                }
                folio_app::SearchState::Failed(error) => {
                    format!("Search failed: {error}. Press Search to retry.")
                }
                folio_app::SearchState::Complete => {
                    let count = self.controller.search_results.len();
                    if count == 0 {
                        format!(
                            "No matches for “{}”. Search covers titles, tags, typed text, equations, and indexed handwriting.",
                            self.controller.search_query
                        )
                    } else if count >= 100 {
                        format!(
                            "Showing the first 100 matching pages for “{}”. Use Up, Down and Enter to open a result.",
                            self.controller.search_query
                        )
                    } else {
                        format!(
                            "{count} matching page{} for “{}” · Use Up, Down and Enter to open a result",
                            if count == 1 { "" } else { "s" },
                            self.controller.search_query
                        )
                    }
                }
            };
            results = results.child(
                div()
                    .p_3()
                    .text_sm()
                    .text_color(rgb(theme.muted))
                    .child(message),
            );
            let cursor = self
                .modal_cursor
                .min(self.controller.search_results.len().saturating_sub(1));
            for (index, r) in self
                .controller
                .search_results
                .clone()
                .into_iter()
                .enumerate()
            {
                let note = r.note;
                let page = r.page;
                let title = r.page_number.map_or_else(
                    || r.title.clone(),
                    |page| format!("{} · Page {page}", r.title),
                );
                let mut snippet = String::new();
                let mut highlights = vec![];
                for (part, matched) in r.snippet_parts() {
                    let start = snippet.len();
                    snippet.push_str(&part);
                    if matched {
                        highlights.push((
                            start..snippet.len(),
                            HighlightStyle {
                                color: Some(rgb(theme.ink).into()),
                                font_weight: Some(FontWeight::SEMIBOLD),
                                background_color: Some(rgba((theme.accent << 8) | 0x30).into()),
                                ..Default::default()
                            },
                        ));
                    }
                }
                results = results.child(
                    self.control(
                        format!("result-{note}-{page}"),
                        format!("Open result: {title} · {}", r.snippet_text()),
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .items_start()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme.muted))
                                    .child(StyledText::new(snippet).with_highlights(highlights)),
                            )
                            .into_any_element(),
                        index == cursor
                            && field.read(cx).content.trim() == self.controller.search_query,
                        cx,
                        move |this, window, cx| {
                            this.controller.navigate_search(note, page);
                            this.show_editor();
                            this.close_modal(window, cx);
                        },
                    )
                    .p_3()
                    .w_full()
                    .justify_start(),
                );
            }
            panel = panel.child(results);
        }
        let hint = match modal {
            Modal::Recognition if self.controller.recognition_for_index => "Ctrl + Enter to index",
            Modal::Recognition => "Ctrl + Enter to replace",
            _ if multiline => "Ctrl + Enter to save",
            Modal::Search | Modal::OpenDocument => {
                "Up/Down to choose · Enter to open · Escape to close"
            }
            _ if modal.has_field() => "Enter to confirm · Escape to close",
            _ => "Escape to close",
        };
        let mut actions = div().flex().gap_2();
        match &modal {
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
                    .child(
                        self.button("cancel-modal", "Cancel", false, cx, |this, w, cx| {
                            this.cancel_modal(w, cx)
                        }),
                    )
                    .child(self.button(
                        "submit-modal",
                        if self.equation_draft.is_some() && self.controller.equation_pending {
                            "Rendering…"
                        } else {
                            modal.submit_label(&self.controller)
                        },
                        true,
                        cx,
                        |this, w, cx| this.submit_modal(w, cx),
                    ));
            }
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
            .child(div().text_xs().text_color(rgb(theme.muted)).child(hint))
            .child(actions);
        let max_height = (f32::from(window.viewport_size().height)
            / self.controller.settings.ui_scale
            - if picker { 96. } else { 48. })
        .max(240.);
        div()
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000070))
            .flex()
            .justify_center()
            // Pickers stay anchored while results load so the query field does
            // not move under the caret.
            .when(picker, |overlay| overlay.items_start().pt(px(64.)))
            .when(!picker, |overlay| overlay.items_center())
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(if wide { 660. } else { 520. }))
                    .max_w_full()
                    .max_h(px(max_height))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .bg(rgb(theme.popover))
                    .rounded(px(theme.radius + 4.))
                    .border_1()
                    .border_color(theme.border)
                    .shadow_sm()
                    .child(
                        div()
                            .id("modal-panel")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(panel),
                    )
                    .child(footer),
            )
    }
    /// Documents listed by the Ctrl+T picker for its current query.
    fn open_document_matches(&self, cx: &App) -> Vec<folio_document::NoteMetadata> {
        let query = self
            .modal
            .as_ref()
            .map(|(_, field)| field.read(cx).content.trim().to_lowercase())
            .unwrap_or_default();
        self.controller
            .notes
            .iter()
            .filter(|n| !n.trashed && n.title.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }
    /// Move the highlighted row of a picker dialog.
    fn navigate_modal(&mut self, down: bool, cx: &mut Context<Self>) {
        let count = match self.modal.as_ref().map(|(m, _)| m) {
            Some(Modal::OpenDocument) => self.open_document_matches(cx).len(),
            Some(Modal::Search) => self.controller.search_results.len(),
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
        cx.notify();
    }
    /// Prepare synthetic 120 Hz input. Dispatch is performed without borrowing
    /// this entity, exactly as the native platform event loop dispatches frames.
    pub fn smoke_events(&mut self) -> Result<Vec<TabletEvent>, String> {
        let b = self.canvas_bounds.ok_or("Canvas was not laid out")?;
        self.controller.create_note();
        self.controller.session_mut().viewport = folio_canvas::Viewport {
            zoom: 1.,
            pan: DocPoint::new(40., 30.),
            rotation: 0.,
        };
        let point = |x: f32, y: f32| {
            point(
                px(f32::from(b.origin.x) + 40. + x),
                px(f32::from(b.origin.y) + 30. + y),
            )
        };
        let mut events = (0..80)
            .map(|i| TabletEvent {
                position: point(50. + i as f32 * 3., 100. + (i as f32 * 0.12).sin() * 25.),
                pressure: 0.1 + i as f32 / 100.,
                tilt_x: 12.,
                tilt_y: -8.,
                timestamp: 101 + i * 8,
                buttons: 0,
                eraser: false,
                phase: if i == 0 {
                    TabletPhase::Down
                } else {
                    TabletPhase::Move
                },
                modifiers: Default::default(),
            })
            .collect::<Vec<_>>();
        events.push(TabletEvent {
            position: point(287., 100. + (79f32 * 0.12).sin() * 25.),
            pressure: 0.,
            tilt_x: 12.,
            tilt_y: -8.,
            timestamp: 900,
            buttons: 0,
            eraser: false,
            phase: TabletPhase::Up,
            modifiers: Default::default(),
        });
        Ok(events)
    }
    pub fn smoke_toolbar_setup(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
        self.controller.select_all();
        cx.notify();
    }
    pub fn smoke_toolbar_events(&self) -> Result<Vec<PlatformInput>, String> {
        let b = self.canvas_bounds.ok_or("Canvas not laid out")?;
        let button = self
            .accessibility
            .control_bounds("Copy")
            .ok_or("Contextual selection toolbar not laid out")?;
        let origin = self.selection_toolbar_origin();
        let left = f32::from(b.origin.x) + origin.x;
        let top = f32::from(b.origin.y) + origin.y;
        if button.x0 < (left - 1.) as f64
            || button.y0 < (top - 1.) as f64
            || button.x1 > (left + f32::from(self.selection_toolbar_size.width) + 1.) as f64
            || button.y1 > (top + f32::from(self.selection_toolbar_size.height) + 1.) as f64
        {
            return Err(format!(
                "Contextual selection toolbar is still laying out: button {button:?}; origin {left},{top}; size {:?}; canvas {b:?}; viewport {:?}",
                self.selection_toolbar_size,
                self.controller.session().viewport
            ));
        }
        let position = point(
            px(((button.x0 + button.x1) * 0.5) as f32),
            px(((button.y0 + button.y1) * 0.5) as f32),
        );
        let tablet = |phase| TabletEvent {
            position,
            pressure: 0.7,
            tilt_x: 12.,
            tilt_y: -8.,
            timestamp: 1000,
            buttons: 0,
            eraser: false,
            phase,
            modifiers: Default::default(),
        };
        Ok(vec![
            PlatformInput::Tablet(tablet(TabletPhase::Down)),
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }),
            PlatformInput::Tablet(tablet(TabletPhase::Up)),
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }),
            PlatformInput::Tablet(tablet(TabletPhase::Leave)),
            PlatformInput::NavigationGesture(NavigationGesture {
                position: point(b.origin.x + px(200.), b.origin.y + px(200.)),
                translation: point(px(0.), px(0.)),
                scale: 1.,
                rotation: 0.,
                phase: TouchPhase::Started,
                cancelled: false,
            }),
            PlatformInput::NavigationGesture(NavigationGesture {
                position: point(b.origin.x + px(200.), b.origin.y + px(200.)),
                translation: point(px(30.), px(20.)),
                scale: 1.25,
                rotation: 0.2,
                phase: TouchPhase::Moved,
                cancelled: false,
            }),
            PlatformInput::NavigationGesture(NavigationGesture {
                position: point(b.origin.x + px(200.), b.origin.y + px(200.)),
                translation: point(px(0.), px(0.)),
                scale: 1.,
                rotation: 0.,
                phase: TouchPhase::Ended,
                cancelled: true,
            }),
            PlatformInput::TabletPad(TabletPadEvent {
                button: Some(0),
                pressed: true,
                delta: 0.,
                strip: false,
                timestamp: 1100,
            }),
            PlatformInput::TabletPad(TabletPadEvent {
                button: Some(1),
                pressed: true,
                delta: 0.,
                strip: false,
                timestamp: 1110,
            }),
        ])
    }
    pub fn smoke_verify(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let viewport = self.controller.session().viewport;
        if (viewport.zoom - 1.).abs() > 0.001
            || viewport.rotation.abs() > 0.001
            || viewport.pan.distance(DocPoint::new(40., 30.)) > 0.001
        {
            return Err("Cancelled navigation did not restore the viewport".into());
        }
        let stroke = self
            .controller
            .page()
            .ordered_objects()
            .find_map(|o| {
                if let folio_document::Object::Stroke(s) = o.as_ref() {
                    Some(s)
                } else {
                    None
                }
            })
            .ok_or("Tablet dispatch produced no ink")?;
        if stroke.raw.len() != 80 || stroke.raw[0].tilt_x != 12. || stroke.raw[79].pressure < 0.88 {
            return Err("Rich tablet samples were lost".into());
        }
        if self.controller.page().objects.len() != 1
            || (stroke.transform.a - 1.).abs() > 0.001
            || (stroke.transform.d - 1.).abs() > 0.001
            || stroke.transform.b.abs() > 0.001
            || stroke.transform.c.abs() > 0.001
        {
            return Err(format!(
                "Stylus selection-toolbar Copy must preserve ink without drawing through the overlay: {} objects, {} selected, transform {:?}",
                self.controller.page().objects.len(),
                self.controller.session().selection.len(),
                stroke.transform
            ));
        }
        self.controller.select_all(); // Pad undo/redo clears selection; restore it for clipboard comparison.
        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
        if clipboard != self.controller.encode_clipboard() {
            return Err(
                "Selection Copy did not write the selected objects to the clipboard".into(),
            );
        }
        self.controller.select_all();
        self.controller.scale_selection(1.2);
        self.controller.undo();
        self.controller.redo();
        self.controller.add_page();
        self.controller.change_page(0);
        self.controller.flush()
    }
    /// Diagnostics for the isolated ink-layer example; does not open the updater.
    pub fn smoke_ink_layers(&mut self, enabled: Option<bool>) -> (usize, usize) {
        self.painter.ink_layer_override = enabled;
        self.painter.ink_layer_counts()
    }
}
fn this_region(controller: &mut Controller, values: &[f32]) -> Result<(), String> {
    controller.read_pdf_math(folio_document::Rect::new(
        values[0], values[1], values[2], values[3],
    ))
}
