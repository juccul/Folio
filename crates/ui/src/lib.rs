//! GPUI presentation and native input adapter. Document operations are delegated
//! to folio-app; ink geometry remains independent of GPUI.
mod accessibility;
mod diagnostics;
mod input_check;
mod recovery;
pub use recovery::RecoveryView;
mod appearance;
mod color_picker;
mod field;
mod graph;
mod help;
mod icons;
pub use icons::IconAssets;
mod inline_text;
mod math_panel;
mod motion;
mod navigation;
mod notebook_setup;
mod painting;
mod portable;
mod region;
mod templates;
mod text_render;
mod theme;
mod titlebar;
mod validation;
mod workspace;
use field::{Field, FieldBoundsChanged, Submitted};
use folio_app::{Controller, ExportKind, Interaction, NoteFilter, RecognitionKind, Tool};
use folio_document::{Color, Id, InkTool, Paper, Point as DocPoint};
use folio_input::{Device, PenEvent, Phase, TimestampExtender, Tool as PenTool};
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
        Export,
        FocusNext,
        FocusPrevious
    ]
);
#[derive(Clone)]
enum Modal {
    SaveTemplate,
    Templates,
    RenameTemplate(Id),
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
}
impl Modal {
    fn title(&self) -> &'static str {
        match self {
            Self::SaveTemplate => "Save page as template",
            Self::Templates => "Page templates",
            Self::RenameTemplate(_) => "Rename template",
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
        }
    }
}
pub struct NotesView {
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
    notebook_setup: Option<notebook_setup::Setup>,
    color_drag: Option<(EntityId, usize)>,
    inline_text: Option<inline_text::Editor>,
    subscriptions: Vec<Subscription>,
    motion: std::cell::RefCell<motion::Motion>,
    building_overlay: bool,
    settings_open: bool,
    theme_colors_open: bool,
    modal_error: Option<String>,
    pen_settings: bool,
    more_open: bool,
    export_open: bool,
    help_open: bool,
    library_open: bool,
    pages_open: bool,
    list_view: bool,
    sort_by_name: bool,
    open_tabs: Vec<Id>,
    tab_scroll: ScrollHandle,
    tab_target: Option<(Id, usize, Pixels, f32)>,
    document_menu: Option<(Id, Point<Pixels>)>,
    document_menu_folders: bool,
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
        let focus = cx.focus_handle();
        focus.focus(window);
        let entity = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            entity
                .update(cx, |view, cx| match view.controller.flush() {
                    Ok(()) => true,
                    Err(e) => {
                        view.controller.error = Some(e);
                        cx.notify();
                        false
                    }
                })
                .unwrap_or(true)
        });
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let result = view.update(cx, |view, cx| {
                    let changed = view.controller.tick();
                    let motion_changed = view.motion.borrow_mut().advance();
                    if changed || view.accessibility.poll() || motion_changed {
                        cx.notify();
                    }
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
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
            notebook_setup: None,
            color_drag: None,
            inline_text: None,
            subscriptions: vec![],
            motion: Default::default(),
            building_overlay: false,
            settings_open: false,
            theme_colors_open: false,
            modal_error: None,
            pen_settings: false,
            more_open: false,
            export_open: false,
            help_open: false,
            library_open: true,
            pages_open: false,
            list_view: false,
            sort_by_name: false,
            open_tabs: vec![],
            tab_scroll: ScrollHandle::new(),
            tab_target: None,
            document_menu: None,
            document_menu_folders: false,
            thumbnails: workspace::Thumbnails::default(),
            writing_style: None,
            timestamps: TimestampExtender::default(),
            last_tablet: None,
            pen_in_range: false,
            gesture_start: None,
            mouse_pan: false,
            start: Instant::now(),
            painter: painting::Painter::default(),
            accessibility: std::rc::Rc::new(accessibility::Accessibility::new(window)),
        }
    }
    pub fn bindings(cx: &mut App) {
        Field::bindings(cx);
        cx.bind_keys([
            KeyBinding::new("tab", FocusNext, None),
            KeyBinding::new("shift-tab", FocusPrevious, None),
            KeyBinding::new("ctrl-n", NewNote, Some("FolioLibrary")),
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
            KeyBinding::new("ctrl-s", Save, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-n", NewNote, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-n", NewPage, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-f", Search, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-l", Library, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-p", TogglePages, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-c", Copy, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-x", Cut, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-v", Paste, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-a", SelectAll, Some("Folio && !FolioField")),
            KeyBinding::new("delete", Delete, Some("Folio && !FolioField")),
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
            KeyBinding::new("ctrl-t", OpenTab, Some("Folio && !FolioField")),
            KeyBinding::new("ctrl-shift-e", Export, Some("Folio && !FolioField")),
        ]);
    }
    fn blocking_overlay(&self) -> bool {
        self.modal.is_some()
            || self.document_menu.is_some()
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
        self.more_open = false;
        self.pen_settings = false;
        self.notebook_setup = None;
        if matches!(modal, Modal::NewDocument) {
            self.prepare_notebook_setup(window, cx);
        }
        let content = match &modal {
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
            Modal::PageSize => format!(
                "{} × {}",
                self.controller.page().properties.width,
                self.controller.page().properties.height
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
        let field = cx.new(|cx| {
            let mut field = Field::new(content, multiline, cx);
            field.secret = secret;
            field.theme = Theme::new(&self.controller.settings);
            field
        });
        field.read(cx).focus.focus(window);
        self.subscriptions.clear();
        self.observe_notebook_setup(window, cx);
        self.subscriptions.push(cx.observe(&field, |this, _, cx| {
            this.modal_error = None;
            cx.notify();
        }));
        self.subscriptions
            .push(cx.subscribe(&field, |_, _, _: &FieldBoundsChanged, cx| cx.notify()));
        self.subscriptions.push(cx.subscribe_in(
            &field,
            window,
            |this, _, _: &Submitted, window, cx| this.submit_modal(window, cx),
        ));
        self.modal_error = None;
        self.color_drag = None;
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
            Modal::MovePage => {}
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
            Modal::Rename => self.controller.rename(content),
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
                let found = self
                    .controller
                    .notes
                    .iter()
                    .find(|n| {
                        !n.trashed && n.title.to_lowercase().contains(&content.to_lowercase())
                    })
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
            Modal::Tags => self.controller.metadata(|m| {
                m.tags = content
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            }),
            Modal::Search => {
                self.controller.search(content);
                field.read(cx).focus.focus(window);
                cx.notify();
                return;
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
                let (width, height) = match validation::page_size(&content) {
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
                        self.controller.paste_image(image.bytes(), ext);
                        pasted = true;
                        break;
                    }
                }
            }
            if !pasted && let Some(text) = item.text() {
                self.controller.paste_text_or_objects(text)
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
        let snapshot = self.controller.prepare_export();
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
    fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let label = label.into();
        self.control(
            id,
            label.clone(),
            div().child(label).into_any_element(),
            active,
            cx,
            action,
        )
    }
    fn control(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        content: AnyElement,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let id = id.into();
        let label = label.into();
        let window_control = id.as_ref().starts_with("window-");
        let navigation = self.library_open
            || id.as_ref().starts_with("tab-")
            || id.as_ref().starts_with("close-tab-")
            || matches!(id.as_ref(), "library" | "new-tab" | "settings" | "search");
        let editing = matches!(
            id.as_ref(),
            "undo"
                | "redo"
                | "rename-note"
                | "tags"
                | "favorite"
                | "add-page"
                | "delete-page"
                | "duplicate-page"
                | "bookmark-page"
                | "move-page"
                | "page-size"
                | "infinite"
                | "insert-space"
                | "remove-space"
                | "insert-equation"
                | "cover-page"
                | "page-templates"
                | "index-page-handwriting"
                | "clear-page-index"
                | "blank"
                | "ruled"
                | "grid"
                | "dots"
                | "toolbar-image"
                | "import"
        );
        let enabled = (!self.controller.read_only() || self.library_open || !editing)
            && (!self.blocking_overlay() || self.building_overlay || window_control)
            && (!self.controller.loading_note()
                || navigation
                || self.building_overlay
                || window_control)
            && match id.as_ref() {
                key if key.starts_with("folder-destination-") => {
                    let destination = key
                        .strip_prefix("folder-destination-")
                        .and_then(|id| Id::parse_str(id).ok());
                    if let Some((Modal::MoveNotebook(source), _)) = self.modal.as_ref() {
                        self.controller
                            .validate_folder_move(*source, destination)
                            .is_ok()
                    } else {
                        true
                    }
                }
                "submit-modal"
                    if matches!(self.modal.as_ref(), Some((Modal::MoveNotebook(_), _))) =>
                {
                    if let Some((Modal::MoveNotebook(source), _)) = self.modal.as_ref() {
                        self.controller
                            .validate_folder_move(*source, self.folder_destination)
                            .is_ok()
                    } else {
                        true
                    }
                }
                "open-restored-library" => !self.controller.has_background_work(),
                "submit-modal"
                    if matches!(self.modal.as_ref(), Some((Modal::Recognition, _)))
                        && self.controller.recognition_for_index =>
                {
                    self.controller.can_index_review()
                        && !self.controller.recognition_pending
                        && !self.controller.read_only()
                }
                "retry-linked-math" => !self
                    .controller
                    .session()
                    .selection
                    .iter()
                    .any(|id| self.controller.math_updating(*id)),
                "submit-modal" if self.equation_draft.is_some() => {
                    !self.controller.equation_pending
                }
                "undo" => self.controller.session().history.can_undo(),
                "redo" => self.controller.session().history.can_redo(),
                "math-apply-latex" => {
                    self.math_inputs
                        .as_ref()
                        .is_some_and(|inputs| inputs.result_dirty(cx))
                        && !self
                            .controller
                            .math_session
                            .as_ref()
                            .is_some_and(|s| s.pending)
                }
                "copy-math-result" | "insert-math-result" | "insert-math-worked"
                | "insert-math-live" => {
                    self.math_inputs
                        .as_ref()
                        .is_some_and(|inputs| !inputs.result_dirty(cx))
                        && !self
                            .controller
                            .math_session
                            .as_ref()
                            .is_some_and(|s| s.pending)
                }
                "math-previous" | "math-next" | "math-hint" => self
                    .controller
                    .math_session
                    .as_ref()
                    .is_some_and(|session| {
                        !session.pending
                            && session.report.as_ref().is_some_and(|report| {
                                if id.as_ref() == "math-previous" {
                                    session.revealed > 1
                                } else {
                                    session.revealed < report.steps.len()
                                }
                            })
                    }),
                "math-use-selection"
                | "math-read-selection"
                | "math-read-text"
                | "math-read-next" => {
                    !self.controller.session().selection.is_empty()
                        && !self.controller.recognition_pending
                        && !self
                            .controller
                            .math_session
                            .as_ref()
                            .is_some_and(|s| s.pending)
                }
                "math-primary" => {
                    self.math_inputs
                        .as_ref()
                        .is_some_and(|inputs| inputs.can_run(cx))
                        && !self.controller.recognition_pending
                        && !self
                            .controller
                            .math_session
                            .as_ref()
                            .is_some_and(|s| s.pending)
                }
                _ => true,
            };
        let hover_id = id.clone();
        let hover = self.motion.borrow().hover_value(id.as_ref());
        let resting = if active {
            theme.selected
        } else {
            theme.surface
        };
        let background = theme::mix(resting, theme.selected, hover);
        let destructive = label.as_ref().starts_with("Delete");
        let action: accessibility::Callback = std::rc::Rc::new(action);
        let (node, focus) = self.accessibility.control(
            id.as_ref(),
            &label,
            action.clone(),
            enabled,
            !self.blocking_overlay() || self.building_overlay || window_control,
            cx,
        );
        let bridge = self.accessibility.clone();
        let decoration = canvas(
            move |bounds, _, _| {
                bridge.set_bounds(node, bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let keyboard_action = action.clone();
        div()
            .id(id)
            .relative()
            .track_focus(&focus)
            .tab_stop(enabled)
            .key_context("FolioControl")
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if enabled && (event.keystroke.key == "enter" || event.keystroke.key == "space") {
                    keyboard_action(this, window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .flex()
            .items_center()
            .justify_center()
            .px_3()
            .py_2()
            .border_1()
            .border_color(transparent_black())
            .focus(move |s| s.border_color(rgb(theme.ring)))
            .rounded(px(theme.radius))
            .text_sm()
            .text_color(rgb(if destructive {
                theme.destructive
            } else {
                theme.ink
            }))
            .bg(rgb(background))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if enabled {
                    this.motion.borrow_mut().hover(
                        hover_id.as_ref(),
                        *hovered,
                        this.controller.settings.reduce_motion,
                    );
                    cx.notify();
                }
            }))
            .active(move |s| {
                if enabled {
                    s.bg(rgb(theme.selected))
                        .border_color(theme.border)
                        .opacity(0.8)
                } else {
                    s
                }
            })
            .opacity(if enabled { 1. } else { 0.4 })
            .when(enabled, |s| s.cursor_pointer())
            .child(content)
            .child(decoration)
            .on_click(cx.listener(move |this, _, window, cx| {
                if enabled {
                    action(this, window, cx);
                    cx.notify();
                }
            }))
    }
    fn selection_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let mut row = div()
            .occlude()
            .absolute()
            .left_4()
            .top_4()
            .max_w(px(self
                .canvas_bounds
                .map(|b| f32::from(b.size.width) - 32.)
                .unwrap_or(960.)))
            .flex_wrap()
            .flex()
            .items_center()
            .gap_1()
            .p_2()
            .rounded(px(theme.radius))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(theme.border)
            .shadow_sm();
        for (id, label, action) in [
            ("move-small", "↗", 0),
            ("scale-down", "Smaller", 1),
            ("scale-up", "Larger", 2),
            ("rotate", "Rotate", 3),
            ("restyle", "Apply pen", 4),
            ("duplicate-selection", "Duplicate", 5),
            ("refine", "Refine", 6),
            ("delete-selection", "Delete", 7),
        ] {
            row = row.child(
                self.button(id, label, false, cx, move |this, _, _| match action {
                    0 => this.controller.transform_selection(
                        folio_document::Transform::translate(12., -12.),
                        "Move selection",
                    ),
                    1 => this.controller.scale_selection(0.9),
                    2 => this.controller.scale_selection(1.1),
                    3 => this.controller.rotate_selection(15f32.to_radians()),
                    4 => this.controller.restyle_selection(),
                    5 => {
                        let objects = this.controller.copy_objects();
                        this.controller.paste_objects(objects);
                    }
                    6 => this.controller.refine_selection(),
                    7 => this.controller.delete_selection(),
                    _ => unreachable!("Unknown selection action"),
                })
                .text_xs()
                .px_2(),
            );
        }
        for id in self.controller.session().selection.clone() {
            if let Some(folio_document::Object::Equation(e)) =
                self.controller.page().objects.get(&id).map(|o| o.as_ref())
                && let Some(error) = e
                    .math_link
                    .as_ref()
                    .and_then(|link| link.last_error.clone())
            {
                row = row
                    .child(
                        div()
                            .text_xs()
                            .max_w(px(240.))
                            .child(format!("Out of date: {error}")),
                    )
                    .child(self.button(
                        "retry-linked-math",
                        "Retry calculation",
                        false,
                        cx,
                        move |this, _, _| {
                            if let Err(error) = this.controller.retry_linked_math(id) {
                                this.controller.status = error;
                            }
                        },
                    ));
            }
        }
        row = row.child(
            self.button("solve-selection", "Solve", false, cx, |this, window, cx| {
                this.start_math(window, cx)
            })
            .text_xs()
            .px_2(),
        );
        if self.controller.recognition_pending {
            row = row
                .child(div().text_xs().px_2().max_w(px(280.)).truncate().child(
                    if self.controller.recognition_replacing {
                        "Rendering equation…".to_string()
                    } else {
                        self.controller.recognition_status.clone()
                    },
                ))
                .child(self.button(
                    "cancel-recognition",
                    "Cancel recognition",
                    false,
                    cx,
                    |this, _, _| this.controller.cancel_recognition(),
                ));
        } else if self.controller.can_recognize_selection() {
            row = row.child(
                self.button(
                    "index-handwriting",
                    "Index handwriting…",
                    false,
                    cx,
                    |this, _, _| {
                        if let Err(e) = this.controller.index_selected_handwriting() {
                            this.controller.error = Some(e);
                        }
                    },
                )
                .text_xs()
                .px_2(),
            );
            for (id, label, kind) in [
                ("recognize-text", "Recognize text", RecognitionKind::Text),
                ("recognize-math", "Recognize math", RecognitionKind::Math),
            ] {
                row = row.child(
                    self.button(id, label, false, cx, move |this, _, _| {
                        if let Err(error) = this.controller.recognize_selection(kind) {
                            this.controller.error = Some(error);
                        }
                    })
                    .text_xs()
                    .px_2(),
                );
            }
        }
        if self.controller.recognition_review.is_some() {
            row = row.child(self.button(
                "review-recognition",
                "Review text",
                false,
                cx,
                |this, window, cx| this.modal(Modal::Recognition, window, cx),
            ));
        }
        if let Some(id) = self
            .controller
            .session()
            .selection
            .iter()
            .find(|id| {
                matches!(
                    self.controller.page().objects.get(id).map(|o| o.as_ref()),
                    Some(folio_document::Object::Equation(_))
                )
            })
            .copied()
        {
            row = row.child(
                self.button(
                    "edit-equation",
                    "Edit equation",
                    false,
                    cx,
                    move |this, window, cx| this.modal(Modal::EditEquation(id), window, cx),
                )
                .text_xs()
                .px_2(),
            );
        }
        if self.controller.session().selection.iter().any(|id| {
            self.controller
                .page()
                .objects
                .get(id)
                .is_some_and(|o| matches!(o.as_ref(), folio_document::Object::Image(_)))
        }) {
            row = row
                .child(
                    self.button("crop-image", "Crop…", false, cx, |this, w, cx| {
                        this.start_image_crop(w, cx)
                    })
                    .text_xs()
                    .px_2(),
                )
                .child(
                    self.button(
                        "crop-image-coordinates",
                        "Crop with numbers…",
                        false,
                        cx,
                        |this, w, cx| this.modal(Modal::Crop, w, cx),
                    )
                    .text_xs()
                    .px_2(),
                )
                .child(
                    self.button("uncrop-image", "Reset crop", false, cx, |this, _, _| {
                        this.controller.crop_selection(None)
                    })
                    .text_xs()
                    .px_2(),
                );
        }
        row.flex_wrap().max_w_full()
    }
    fn popover(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let mut panel = div()
            .occlude()
            .absolute()
            .right_4()
            .top_2()
            .w(px(286.))
            .p_3()
            .rounded(px(theme.radius))
            .border_1()
            .border_color(theme.border)
            .bg(rgb(theme.popover))
            .shadow_sm()
            .id("editor-popover")
            .max_h(px(self
                .canvas_bounds
                .map_or(440., |b| {
                    f32::from(b.size.height) / self.controller.settings.ui_scale - 24.
                })
                .max(120.)))
            .overflow_y_scroll()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                // Toolbar anchors handle their own toggles. Closing first would
                // make the same click immediately reopen the popover.
                if this
                    .canvas_bounds
                    .is_some_and(|b| event.position.y >= b.top())
                {
                    this.dismiss_popovers();
                    cx.notify();
                }
            }))
            .flex()
            .flex_col()
            .gap_2();
        if self.export_open {
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
            for (id, label, action) in [
                ("rename-note", "Rename document", 0),
                ("duplicate-note", "Duplicate document", 1),
                ("tags", "Edit tags", 2),
                ("import", "Import PDF or image", 3),
                (
                    "trash-note",
                    if self.controller.session().document.metadata.trashed {
                        "Restore document"
                    } else {
                        "Move to trash"
                    },
                    4,
                ),
                ("delete-page", "Delete current page", 5),
                ("duplicate-page", "Duplicate current page", 16),
                ("bookmark-page", "Name page bookmark…", 17),
                ("move-page", "Move page to document…", 18),
                ("page-size", "Custom page size", 6),
                ("infinite", "Toggle infinite canvas", 7),
                ("insert-space", "Insert 80 units below selection", 8),
                ("remove-space", "Remove 40 units below selection", 9),
                ("checkpoint", "Create recovery snapshot", 11),
                ("insert-equation", "Insert LaTeX equation", 12),
                ("retry-previews", "Retry failed previews", 13),
                ("cleanup-assets", "Quarantine unused assets", 14),
                ("open-math-solver", "Math solver", 15),
                ("cover-page", "Use current page as cover", 19),
                ("save-page-template", "Save page as template…", 20),
                ("page-templates", "Add page from template…", 21),
                ("index-page-handwriting", "Index page handwriting…", 22),
                ("clear-page-index", "Clear page handwriting index", 23),
            ] {
                panel = panel.child(
                    self.button(id, label, false, cx, move |this, window, cx| {
                        this.more_open = false;
                        match action {
                            0 => this.modal(Modal::Rename, window, cx),
                            1 => this.controller.duplicate_note(),
                            2 => this.modal(Modal::Tags, window, cx),
                            3 => this.import(cx),
                            4 => this.controller.metadata(|m| m.trashed = !m.trashed),
                            5 => this.controller.delete_page(),
                            19 => this.controller.use_page_as_cover(),
                            22 => {
                                if let Err(e) = this.controller.index_page_handwriting() {
                                    this.controller.error = Some(e);
                                }
                            }
                            23 => this.controller.clear_handwriting_index(),
                            20 => this.modal(Modal::SaveTemplate, window, cx),
                            21 => this.modal(Modal::Templates, window, cx),
                            16 => this.controller.duplicate_page(),
                            17 => this.modal(Modal::PageBookmark, window, cx),
                            18 => this.modal(Modal::MovePage, window, cx),
                            6 => this.modal(Modal::PageSize, window, cx),
                            7 => {
                                let p = this.controller.page().properties.clone();
                                this.controller.page_size(p.width, p.height, !p.infinite)
                            }
                            8 | 9 => {
                                let at = this
                                    .controller
                                    .selection_bounds()
                                    .map(|r| r.max.y)
                                    .unwrap_or(
                                        this.controller.cursor.unwrap_or(DocPoint::new(0., 200.)).y,
                                    );
                                this.controller.insert_space(
                                    at,
                                    if action == 8 { 80. } else { -40. },
                                    true,
                                )
                            }
                            12 => this.modal(Modal::Equation, window, cx),
                            13 => this.controller.retry_previews(),
                            14 => this.controller.cleanup_assets(),
                            15 => this.start_math(window, cx),
                            _ => this.controller.recovery_checkpoint(),
                        }
                    })
                    .justify_start()
                    .text_sm(),
                );
            }
            let mut paper = div().flex().gap_1();
            for (id, label, p) in [
                ("blank", "Blank", Paper::Blank),
                ("ruled", "Ruled", Paper::Ruled),
                ("grid", "Grid", Paper::Grid),
                ("dots", "Dots", Paper::Dots),
            ] {
                paper = paper.child(
                    self.button(
                        id,
                        label,
                        self.controller.page().properties.paper == p,
                        cx,
                        move |this, _, _| this.controller.paper(p),
                    )
                    .text_xs()
                    .px_2(),
                );
            }
            panel = panel.child(paper);
            panel = panel.child(
                self.button(
                    "move-document-folder",
                    "Move to folder…",
                    false,
                    cx,
                    |this, w, cx| {
                        this.more_open = false;
                        this.modal(Modal::MoveDocument(this.controller.active), w, cx);
                    },
                )
                .justify_start(),
            );
        } else if self.pen_settings && self.controller.tool == Tool::Eraser {
            panel = panel.child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("Eraser"))
                .child(self.eraser_controls("popover",cx))
                .child(div().text_xs().child("Whole stroke and Ink segments erase handwriting only. Whole object also deletes text, images, equations, and shapes. Undo restores erased content."));
        } else if self.pen_settings {
            panel = panel.child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Your pen"),
            );
            let mut tools = div().flex().flex_wrap().gap_1();
            for (id, label, tool) in [
                ("ballpoint", "Ballpoint", InkTool::Ballpoint),
                ("fountain", "Fountain", InkTool::Fountain),
                ("pencil", "Pencil", InkTool::Pencil),
                ("marker", "Marker", InkTool::Marker),
                ("highlighter", "Highlighter", InkTool::Highlighter),
            ] {
                tools = tools.child(
                    self.button(
                        id,
                        label,
                        self.controller.style.tool == tool,
                        cx,
                        move |this, _, _| {
                            this.controller.style.tool = tool;
                            if tool == InkTool::Highlighter {
                                this.controller.style.width = 20.;
                                this.controller.style.opacity = 0.3;
                            }
                        },
                    )
                    .text_xs()
                    .px_2(),
                );
            }
            panel = panel.child(tools);
            for (label, kind, value) in [
                ("Width", 0, self.controller.style.width),
                ("Opacity", 1, self.controller.style.opacity),
                ("Stabilization", 2, self.controller.style.stabilization),
                ("Pressure curve", 3, self.controller.style.pressure_gamma),
            ] {
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_sm()
                        .child(label)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    self.button(
                                        format!("minus-{kind}"),
                                        "−",
                                        false,
                                        cx,
                                        move |this, _, _| adjust_pen(this, kind, -1.),
                                    )
                                    .px_2(),
                                )
                                .child(format!("{value:.2}"))
                                .child(
                                    self.button(
                                        format!("plus-{kind}"),
                                        "＋",
                                        false,
                                        cx,
                                        move |this, _, _| adjust_pen(this, kind, 1.),
                                    )
                                    .px_2(),
                                ),
                        ),
                );
            }
            panel = panel.child(
                self.button(
                    "custom-color",
                    "Custom color…",
                    false,
                    cx,
                    |this, w, cx| this.modal(Modal::Color, w, cx),
                )
                .justify_start(),
            );
            panel = panel.child(
                self.button("save-preset", "Save as preset", false, cx, |this, _, _| {
                    this.controller.save_preset()
                })
                .justify_start(),
            );
            for (i, preset) in self
                .controller
                .settings
                .presets
                .clone()
                .into_iter()
                .enumerate()
            {
                let label = format!(
                    "{:?} · {:.1} · {}",
                    preset.tool,
                    preset.width,
                    preset.color.hex()
                );
                panel = panel.child(
                    self.button(
                        format!("preset-{i}"),
                        label,
                        false,
                        cx,
                        move |this, _, _| {
                            this.controller.set_style(preset.clone());
                            this.region_selection = None;
                            this.controller.set_tool(Tool::Pen)
                        },
                    )
                    .text_xs()
                    .justify_start(),
                );
            }
        }
        if self.pen_settings && !self.controller.settings.recent_colors.is_empty() {
            let mut recent = div().flex().gap_1().items_center().child("Recent colors");
            for color in self.controller.settings.recent_colors.clone() {
                recent = recent.child(
                    self.button(
                        format!("recent-{}", color.rgb()),
                        "●",
                        false,
                        cx,
                        move |this, _, _| this.controller.set_color(color),
                    )
                    .text_color(rgb(color.rgb()))
                    .px_2(),
                );
            }
            panel = panel.child(recent);
        }
        panel.id("popover").overflow_y_scroll()
    }
    fn settings_panel(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let mut body = div()
            .id("settings-body")
            .flex()
            .flex_col()
            .gap_3()
            .max_h(px((f32::from(window.viewport_size().height)
                / self.controller.settings.ui_scale
                - 140.)
                .clamp(180., 570.)))
            .min_h_0()
            .overflow_y_scroll()
            .child(self.appearance_panel(cx));
        body = body.child(div().mt_3().text_sm().font_weight(FontWeight::SEMIBOLD).child("Backup and restore"))
            .child(self.button("backup-library", "Back up library…", false, cx, |this, _, cx| this.backup_dialog(cx)).justify_start())
            .child(self.button("restore-library", "Restore backup to a new library…", false, cx, |this, _, cx| this.restore_dialog(cx)).justify_start())
            .child(div().text_xs().text_color(rgb(theme.muted)).child("Includes documents, assets, folders, preferences and undo history. Downloadable OCR/math runtimes are excluded."));
        if let Some(path) = &self.controller.restored_library {
            body = body
                .child(
                    div()
                        .text_xs()
                        .child(format!("Restored: {}", path.display())),
                )
                .child(self.button(
                    "open-restored-library",
                    "Open restored library",
                    true,
                    cx,
                    |this, w, cx| this.open_restored_library(w, cx),
                ));
        }
        for (id, label, enabled, kind) in [
            (
                "scratch-toggle",
                "Scratch to erase (deliberate scribbles)",
                self.controller.settings.scratch_erase,
                1,
            ),
            (
                "hold-toggle",
                "Hold to snap shapes",
                self.controller.settings.hold_shapes,
                2,
            ),
            (
                "encircle-toggle",
                "Circle and hold to select ink",
                self.controller.settings.encircle_select,
                3,
            ),
            (
                "autosave-toggle",
                "Autosave each completed command",
                self.controller.settings.autosave,
                6,
            ),
        ] {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .text_sm()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().flex_1().min_w_0().child(label)),
                    )
                    .child(self.appearance_switch(id, label, enabled, cx, move |this| {
                        if kind == 6 {
                            this.controller
                                .set_autosave(!this.controller.settings.autosave);
                            return;
                        }
                        let s = &mut this.controller.settings;
                        match kind {
                            1 => s.scratch_erase = !s.scratch_erase,
                            2 => s.hold_shapes = !s.hold_shapes,
                            3 => s.encircle_select = !s.encircle_select,

                            _ => s.autosave = !s.autosave,
                        }
                    })),
            )
        }
        body = body.child(
            self.button(
                "pad-buttons",
                "Tablet pad button actions…",
                false,
                cx,
                |this, w, cx| this.modal(Modal::PadButtons, w, cx),
            )
            .justify_start(),
        );
        body = body.child(
            div()
                .mt_3()
                .pt_3()
                .border_t_1()
                .border_color(theme.border)
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Accessibility"),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_sm()
                .child("Reduce motion")
                .child(self.appearance_switch(
                    "reduce-motion",
                    "Reduce motion",
                    self.controller.settings.reduce_motion,
                    cx,
                    |this| {
                        this.controller.settings.reduce_motion =
                            !this.controller.settings.reduce_motion;
                    },
                )),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child("UI scale")
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(self.button("ui-smaller", "−", false, cx, |this, _, _| {
                            this.controller.settings.ui_scale =
                                (this.controller.settings.ui_scale - 0.1).max(0.8);
                            this.controller.store_settings();
                        }))
                        .child(format!("{:.0}%", self.controller.settings.ui_scale * 100.))
                        .child(self.button("ui-larger", "＋", false, cx, |this, _, _| {
                            this.controller.settings.ui_scale =
                                (this.controller.settings.ui_scale + 0.1).min(1.6);
                            this.controller.store_settings();
                        })),
                ),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child("Cursor size")
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(self.button("cursor-smaller", "−", false, cx, |this, _, _| {
                            this.controller.settings.cursor_size =
                                (this.controller.settings.cursor_size - 2.).max(4.);
                            this.controller.store_settings();
                        }))
                        .child(format!("{:.0} px", self.controller.settings.cursor_size))
                        .child(self.button("cursor-larger", "＋", false, cx, |this, _, _| {
                            this.controller.settings.cursor_size =
                                (this.controller.settings.cursor_size + 2.).min(64.);
                            this.controller.store_settings();
                        })),
                ),
        );
        body=body.child(div().text_xs().text_color(rgb(theme.muted)).child("Draw with a stylus or mouse. Two-finger scrolling pans; Ctrl + scroll zooms. Pen input preserves pressure, tilt, buttons and timing."));
        div()
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000070))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(540.))
                    .max_w_full()
                    .p_6()
                    .bg(rgb(theme.popover))
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(theme.radius + 4.))
                    .shadow_sm()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Settings"),
                            )
                            .child(self.button(
                                "close-settings",
                                "Done",
                                true,
                                cx,
                                |this, w, _| {
                                    this.settings_open = false;
                                    this.focus.focus(w);
                                },
                            )),
                    )
                    .child(body),
            )
    }
    fn modal_panel(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let Some((modal, field)) = self.modal.clone() else {
            return div();
        };
        field.update(cx, |field, _| field.theme = theme);
        let search = matches!(modal, Modal::Search);
        let multiline = matches!(modal, Modal::Recognition);
        let mut panel = div()
            .id("modal-panel")
            .max_h(px((f32::from(window.viewport_size().height)
                / self.controller.settings.ui_scale
                - 48.)
                .max(240.)))
            .overflow_y_scroll()
            .w(px(if search || matches!(modal, Modal::NewDocument) { 660. } else { 520. }))
            .max_w_full()
            .p_6()
            .bg(rgb(theme.popover))
            .rounded(px(theme.radius + 4.))
            .border_1()
            .border_color(theme.border)
            .shadow_sm()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(modal.title()),
            )
            .when(matches!(modal, Modal::PageSize | Modal::FontSize | Modal::Color | Modal::MoveNotebook(_) | Modal::MoveDocument(_) | Modal::Crop), |panel| {
                panel.child(div().text_sm().text_color(rgb(theme.muted)).child(match modal {
                    Modal::PageSize => "Width × height in points. For A4, use 794 × 1123.",
                    Modal::FontSize => "Choose a size from 6 to 180 points.",
                    Modal::Color => "Choose a color below, or enter a hex value.",
                    Modal::MoveNotebook(_) | Modal::MoveDocument(_) => "Filter folders by path, choose a destination below, then press Move.",
                    _ => "Left, top, width and height as fractions from 0 to 1.",
                }))
            })
            .when(matches!(modal, Modal::NewDocument), |panel| panel.child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Name (optional)")))
            .when(!matches!(modal, Modal::MovePage | Modal::Templates), |panel| panel.child(field.clone()));
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
            for (folder, _) in self.controller.folder_tree() {
                let destination = folder.id;
                let path = self.controller.folder_path(destination);
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
            panel = panel.child(self.color_selector(field, opacity, cx));
        }
        if matches!(modal, Modal::Templates) {
            panel = panel.child(self.template_picker(cx)).child(self.button(
                "cancel-modal",
                "Close",
                false,
                cx,
                |this, w, cx| this.cancel_modal(w, cx),
            ));
            return div()
                .occlude()
                .absolute()
                .inset_0()
                .bg(rgba(0x00000070))
                .flex()
                .items_center()
                .justify_center()
                .child(panel);
        }
        if matches!(modal, Modal::MovePage) {
            let mut destinations = div()
                .id("move-page-destinations")
                .max_h(px(300.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
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
                        false,
                        cx,
                        move |this, w, cx| {
                            this.controller.move_page_to(id);
                            this.close_modal(w, cx);
                        },
                    )
                    .justify_start(),
                );
            }
            panel = panel.child(div().text_sm().child("The destination is saved before removing the source. Undo in the source restores a copy.")).child(destinations);
            if !self
                .controller
                .notes
                .iter()
                .any(|n| !n.trashed && n.id != self.controller.active)
            {
                panel = panel.child("Create another document to move pages into it.");
            }
            panel = panel.child(
                self.button("cancel-modal", "Cancel", false, cx, |this, w, cx| {
                    this.cancel_modal(w, cx)
                }),
            );
            return div()
                .occlude()
                .absolute()
                .inset_0()
                .bg(rgba(0x00000070))
                .flex()
                .items_center()
                .justify_center()
                .child(panel);
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
                                this.math_inputs = None;
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
            let query = self
                .modal
                .as_ref()
                .unwrap()
                .1
                .read(cx)
                .content
                .to_lowercase();
            panel = panel.child(
                self.button(
                    "tab-create-note",
                    "＋  Create new document",
                    false,
                    cx,
                    |this, w, cx| {
                        this.new_notebook(w, cx);
                    },
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
            for note in self
                .controller
                .notes
                .clone()
                .into_iter()
                .filter(|n| !n.trashed && n.title.to_lowercase().contains(&query))
            {
                let id = note.id;
                results = results.child(
                    self.button(
                        format!("pick-document-{id}"),
                        format!("Open {}", note.title),
                        self.open_tabs.contains(&id),
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
                    "Enter a query and press Enter to search documents and indexed handwriting."
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
                            "Showing the first 100 matching pages for “{}”. Refine your query for more specific results.",
                            self.controller.search_query
                        )
                    } else {
                        format!(
                            "{count} matching page{} for “{}”",
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
            for r in self.controller.search_results.clone() {
                let note = r.note;
                let page = r.page;
                let title = r.page_number.map_or_else(
                    || r.title.clone(),
                    |page| format!("{} · Page {page}", r.title),
                );
                results = results.child(
                    self.control(
                        format!("result-{note}-{page}"),
                        format!("Open result: {title} · {}", r.snippet),
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
                                    .child(r.snippet),
                            )
                            .into_any_element(),
                        false,
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
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_xs().text_color(rgb(theme.muted)).child(
                    if matches!(modal, Modal::Recognition) {
                        if self.controller.recognition_for_index {
                            "Ctrl + Enter to index"
                        } else {
                            "Ctrl + Enter to replace"
                        }
                    } else if multiline {
                        "Ctrl + Enter to save"
                    } else {
                        "Enter to confirm · Escape to close"
                    },
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            self.button("cancel-modal", "Cancel", false, cx, |this, w, cx| {
                                this.cancel_modal(w, cx)
                            }),
                        )
                        .child(self.button(
                            "submit-modal",
                            if self.equation_draft.is_some() && self.controller.equation_pending {
                                "Rendering…"
                            } else if search {
                                "Search"
                            } else if matches!(modal, Modal::Recognition) {
                                if self.controller.recognition_for_index {
                                    "Keep ink and index"
                                } else {
                                    "Replace writing"
                                }
                            } else if matches!(modal, Modal::OpenDocument) {
                                "Open"
                            } else if matches!(
                                modal,
                                Modal::MoveNotebook(_) | Modal::MoveDocument(_)
                            ) {
                                "Move"
                            } else if matches!(modal, Modal::NewDocument) {
                                "Create document"
                            } else {
                                "Save"
                            },
                            true,
                            cx,
                            |this, w, cx| this.submit_modal(w, cx),
                        )),
                ),
        );
        div()
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000070))
            .flex()
            .items_center()
            .justify_center()
            .child(panel)
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
    pub fn smoke_toolbar_events(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<Vec<PlatformInput>, String> {
        self.controller.select_all();
        cx.notify();
        let b = self.canvas_bounds.ok_or("Canvas not laid out")?;
        let position = point(b.origin.x + px(40.), b.origin.y + px(40.));
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
    pub fn smoke_verify(&mut self) -> Result<(), String> {
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
            || self.controller.session().selection.len() != 1
            || stroke.transform.tx != 12.
            || stroke.transform.ty != -12.
        {
            return Err("Stylus toolbar click must move the selected stroke without drawing through the overlay".into());
        }
        self.controller.select_all();
        self.controller.scale_selection(1.2);
        self.controller.undo();
        self.controller.redo();
        self.controller.add_page();
        self.controller.change_page(0);
        self.controller.flush()
    }
}
fn adjust_pen(this: &mut NotesView, kind: i32, sign: f32) {
    let s = &mut this.controller.style;
    match kind {
        0 => s.width = (s.width + sign * 0.5).clamp(0.5, 50.),
        1 => s.opacity = (s.opacity + sign * 0.1).clamp(0.1, 1.),
        2 => s.stabilization = (s.stabilization + sign * 0.1).clamp(0., 0.9),
        _ => s.pressure_gamma = (s.pressure_gamma + sign * 0.1).clamp(0.2, 3.),
    }
}
fn this_region(controller: &mut Controller, values: &[f32]) -> Result<(), String> {
    controller.read_pdf_math(folio_document::Rect::new(
        values[0], values[1], values[2], values[3],
    ))
}
impl Render for NotesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.equation_draft.is_some()
            && let Some(result) = self.controller.equation_result.take()
        {
            match result {
                Ok(()) => {
                    let submitted = self.equation_draft.take().unwrap();
                    if self
                        .modal
                        .as_ref()
                        .is_some_and(|(_, field)| field.read(cx).content == submitted)
                    {
                        self.close_modal(window, cx);
                    } else {
                        self.modal_error = Some(
                            "The submitted equation was saved. Your newer draft is still here."
                                .into(),
                        );
                    }
                }
                Err(error) => {
                    self.equation_draft = None;
                    self.modal_error = Some(error);
                }
            }
        }
        self.cancel_region_after_navigation();
        self.building_overlay = false;
        let accessibility = self.accessibility.clone();
        for request in accessibility.drain() {
            if request.target_node == accesskit::NodeId(3) {
                if let Some((_, field)) = &self.modal {
                    if request.action == accesskit::Action::Focus {
                        field.read(cx).focus.focus(window);
                    }
                    if let Some(accesskit::ActionData::Value(value)) = request.data {
                        field.update(cx, |field, cx| field.set_content(value.into(), cx));
                    }
                }
            } else if request.target_node == accesskit::NodeId(13) {
                if let Some(editor) = &self.inline_text {
                    if request.action == accesskit::Action::Focus {
                        editor.field.read(cx).focus.focus(window);
                    }
                    if let Some(accesskit::ActionData::Value(value)) = request.data {
                        editor
                            .field
                            .update(cx, |field, cx| field.set_content(value.into(), cx));
                    }
                }
            } else if request.target_node == accesskit::NodeId(12) {
                if let Some(setup) = &self.notebook_setup {
                    if request.action == accesskit::Action::Focus {
                        setup.color.read(cx).focus.focus(window);
                    }
                    if let Some(accesskit::ActionData::Value(value)) = request.data {
                        setup
                            .color
                            .update(cx, |field, cx| field.set_content(value.into(), cx));
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
                        field.update(cx, |field, cx| field.set_content(value.into(), cx));
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
        self.accessibility.begin();
        if self.modal.is_none()
            && !self.library_open
            && !self.settings_open
            && !self.help_open
            && self.controller.interaction.is_none()
            && self
                .controller
                .recognition_review
                .as_ref()
                .is_some_and(|r| {
                    r.note == self.controller.active && r.page == self.controller.page().id
                })
        {
            self.modal(Modal::Recognition, window, cx);
        }
        if self.modal.is_none()
            && let Some((note, path)) = self.controller.pending_pdf_password.take()
        {
            self.modal(Modal::PdfPassword(note, path), window, cx);
        }
        let reduced = self.controller.settings.reduce_motion;
        let (
            library_alpha,
            settings_alpha,
            modal_alpha,
            menu_alpha,
            help_alpha,
            popover_alpha,
            pages_alpha,
        ) = {
            let mut motion = self.motion.borrow_mut();
            motion.prune();
            if reduced {
                motion.settle();
            }
            (
                motion.panel(
                    "library-motion",
                    self.library_open.then(|| {
                        format!(
                            "{:?}-{}-{}",
                            self.controller.filter, self.list_view, self.sort_by_name
                        )
                    }),
                    reduced,
                ),
                motion.panel(
                    "settings-motion",
                    self.settings_open.then(|| "open".into()),
                    reduced,
                ),
                motion.panel(
                    "modal-motion",
                    self.modal.as_ref().map(|(m, _)| m.title().into()),
                    reduced,
                ),
                motion.panel(
                    "menu-motion",
                    self.document_menu
                        .map(|(id, _)| format!("{id}-{}", self.document_menu_folders)),
                    reduced,
                ),
                motion.panel(
                    "help-motion",
                    self.help_open.then(|| "open".into()),
                    reduced,
                ),
                motion.panel(
                    "popover-motion",
                    (!self.library_open
                        && (self.more_open || self.pen_settings || self.export_open))
                        .then(|| {
                            format!(
                                "{}-{}-{}",
                                self.more_open, self.pen_settings, self.export_open
                            )
                        }),
                    reduced,
                ),
                motion.panel(
                    "pages-motion",
                    (!self.library_open && self.pages_open).then(|| "open".into()),
                    reduced,
                ),
            )
        };
        let theme = Theme::new(&self.controller.settings);
        window.set_rem_size(px(16. * self.controller.settings.ui_scale));
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let canvas = canvas(
            move |bounds, window, cx| {
                entity.update(cx, |view, cx| {
                    let properties = &view.controller.page().properties;
                    let viewport = view.controller.session().viewport;
                    if !properties.infinite
                        && viewport.zoom == 1.
                        && viewport.rotation == 0.
                        && viewport.pan == DocPoint::new(56., 36.)
                    {
                        let x = (f32::from(bounds.size.width) - properties.width) / 2.;
                        view.controller.session_mut().viewport.pan.x = x;
                    } else if let Some(previous) = view.canvas_bounds
                        && previous.size.width != bounds.size.width
                        && view.controller.interaction.is_none()
                    {
                        view.controller.session_mut().viewport.pan.x +=
                            f32::from(bounds.size.width - previous.size.width) / 2.;
                    }
                    let canvas_document = (view.controller.active, view.controller.page().id);
                    if let Some(previous) = view.canvas_bounds
                        && previous.origin.y != bounds.origin.y
                        && view.canvas_document.is_some_and(|(note, page)| {
                            note == canvas_document.0
                                && (page == canvas_document.1
                                    || view.controller.interaction.is_some()
                                    || view.controller.session().viewport.pan.y != 36.)
                        })
                    {
                        let delta = f32::from(bounds.origin.y - previous.origin.y);
                        view.controller.session_mut().viewport.pan.y -= delta;
                        if let Some(Interaction::Pan { pan, .. }) = &mut view.controller.interaction
                        {
                            pan.y -= delta;
                        }
                        if let Some(start) = &mut view.gesture_start {
                            start.pan.y -= delta;
                        }
                        cx.notify();
                    }
                    if let Some(editor) = &view.inline_text {
                        let viewport = view.controller.session().viewport;
                        editor.field.update(cx, |field, _| {
                            if let Some(style) = &mut field.inline {
                                let text = &style.text;
                                style.transform = folio_document::Transform::translate(
                                    f32::from(bounds.origin.x),
                                    f32::from(bounds.origin.y),
                                )
                                .compose(viewport.transform())
                                .compose(text.transform)
                                .compose(
                                    folio_document::Transform::translate(
                                        text.rect.min.x,
                                        text.rect.min.y,
                                    ),
                                );
                                style.mask = bounds;
                            }
                        });
                    }
                    view.canvas_bounds = Some(bounds);
                    view.canvas_document = Some(canvas_document);
                });
                window.insert_hitbox(bounds, HitboxBehavior::Normal).id
            },
            move |bounds, hitbox, window, cx| {
                paint_entity.update(cx, |view, cx| {
                    let editing = view.inline_text.as_ref().map(|e| e.id);
                    view.painter
                        .paint(&mut view.controller, bounds, editing, window, cx);
                    view.paint_region(bounds, window);
                    view.diagnostics.painted();
                });
                let tablet_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &TabletEvent, phase, window, cx| {
                    if phase.bubble()
                        && (event.phase != TabletPhase::Down || hitbox.is_hovered(window))
                    {
                        tablet_entity.update(cx, |v, cx| v.tablet(event, window, cx));
                    }
                });
                let down_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase.bubble()
                        && hitbox.is_hovered(window)
                        && matches!(event.button, MouseButton::Left | MouseButton::Middle)
                    {
                        down_entity.update(cx, |v, cx| {
                            v.mouse(event.position, Phase::Down, event.button, window, cx)
                        });
                    }
                });
                let move_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase.bubble() {
                        move_entity.update(cx, |v, cx| {
                            let phase = if v.controller.interaction.is_some()
                                || (v.region_selection.is_some() && event.pressed_button.is_some())
                            {
                                Phase::Move
                            } else {
                                Phase::Hover
                            };
                            v.mouse(
                                event.position,
                                phase,
                                event.pressed_button.unwrap_or(MouseButton::Left),
                                window,
                                cx,
                            )
                        });
                    }
                });
                let up_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase.bubble() {
                        up_entity.update(cx, |v, cx| {
                            v.mouse(event.position, Phase::Up, event.button, window, cx)
                        });
                    }
                });
                let gesture_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &NavigationGesture, phase, _, cx| {
                    if phase.bubble() {
                        gesture_entity.update(cx, |v, cx| v.gesture(event, cx));
                    }
                });
                let pad_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &TabletPadEvent, phase, _, cx| {
                    if phase.bubble() {
                        pad_entity.update(cx, |v, cx| v.pad(event, cx));
                    }
                });
                let scroll_entity = paint_entity.clone();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase.bubble() && hitbox.should_handle_scroll(window) {
                        scroll_entity.update(cx, |v, cx| v.scroll(event, cx));
                    }
                });
            },
        )
        .size_full();
        let mut center = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(theme.selected))
            .child(canvas)
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                let target = this.controller.requested_note();
                for path in paths.paths() {
                    this.controller.import_into(target, path.clone());
                }
                cx.notify();
            }));
        if let Some(editor) = self.inline_element(cx) {
            center = center.child(editor);
        }
        if self.controller.loading_note() && !self.library_open {
            center = center.child(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(rgb(theme.canvas.paper))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_sm()
                            .text_color(rgb(theme.muted))
                            .child(icon(Icon::Book, theme.muted))
                            .child("Opening document…"),
                    ),
            );
        }
        if !self.library_open
            && !self.controller.read_only()
            && !self.controller.session().selection.is_empty()
            && self.controller.math_session.is_none()
            && self.controller.session().selection.iter().any(|id| {
                !matches!(
                    self.controller.page().objects.get(id).map(|o| o.as_ref()),
                    Some(folio_document::Object::Text(_))
                )
            })
        {
            center = center.child(self.selection_toolbar(cx));
        }
        if !self.library_open && (self.more_open || self.pen_settings || self.export_open) {
            center = center.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.popover(cx))
                    .opacity(popover_alpha),
            );
        }
        let mut root = div()
            .id("folio-root")
            .key_context(
                if self.blocking_overlay() || self.controller.loading_note() {
                    "FolioDialog"
                } else if self.library_open {
                    "FolioLibrary"
                } else {
                    "Folio"
                },
            )
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.ink))
            .font_family("Noto Sans")
            .on_action(cx.listener(|this, _: &Undo, _, cx| {
                this.controller.undo();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Redo, _, cx| {
                this.controller.redo();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Save, _, cx| {
                this.controller.save();
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &FocusNext, w, _| w.focus_next()))
            .on_action(cx.listener(|_, _: &FocusPrevious, w, _| w.focus_prev()))
            .on_action(cx.listener(|this, _: &NewNote, window, cx| {
                this.new_notebook(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewPage, _, cx| {
                this.controller.add_page();
                this.show_editor();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Library, _, cx| {
                this.show_library();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TogglePages, _, cx| {
                this.pages_open = !this.pages_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Search, w, cx| this.modal(Modal::Search, w, cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
            .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.controller.select_all();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Delete, _, cx| {
                this.controller.delete_selection();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Escape, w, cx| {
                if this.region_selection.take().is_some() {
                    this.controller.status = "Region selection cancelled".into();
                    cx.notify();
                    return;
                }
                if this.inline_text.is_some() && this.modal.is_none() {
                    this.finish_inline_text(w, cx);
                    return;
                }
                if this.modal.is_some() {
                    this.cancel_modal(w, cx)
                } else {
                    let dismissing_ui = this.settings_open
                        || this.help_open
                        || this.more_open
                        || this.pen_settings
                        || this.export_open
                        || this.document_menu.is_some()
                        || this.controller.error.is_some();
                    if !dismissing_ui && this.controller.math_session.is_some() {
                        this.controller.close_math_solver();
                        this.math_inputs = None;
                    } else if !dismissing_ui {
                        if this.controller.recognition_pending {
                            this.controller.cancel_recognition();
                        }
                        this.controller.cancel();
                        this.controller.session_mut().selection.clear();
                    }
                    this.settings_open = false;
                    this.dismiss_popovers();
                    this.help_open = false;
                    this.document_menu = None;
                    this.controller.error = None;
                    this.focus.focus(w);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &Pen, _, cx| {
                if this.controller.style.tool == InkTool::Highlighter {
                    this.controller
                        .set_style(this.writing_style.take().unwrap_or_default());
                }
                this.region_selection = None;
                this.controller.set_tool(Tool::Pen);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Eraser, _, cx| {
                this.region_selection = None;
                this.controller.set_tool(Tool::Eraser);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Lasso, _, cx| {
                this.region_selection = None;
                this.controller.set_tool(Tool::Lasso);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Hand, _, cx| {
                this.region_selection = None;
                this.controller.set_tool(Tool::Hand);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Text, _, cx| {
                this.region_selection = None;
                this.controller.set_tool(Tool::Text);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Shapes, _, cx| {
                this.region_selection = None;
                this.controller.set_tool(Tool::Shape);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                this.zoom(1.2);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                this.zoom(1. / 1.2);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FitPage, _, cx| {
                this.fit();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| {
                let p = this.controller.session().page;
                this.controller.change_page(p.saturating_sub(1));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| {
                let p = this.controller.session().page;
                this.controller.change_page(p + 1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Settings, w, cx| {
                this.open_settings(w);
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &OpenTab, w, cx| this.modal(Modal::OpenDocument, w, cx)),
            )
            .on_action(cx.listener(|this, _: &Import, _, cx| this.import(cx)))
            .on_action(cx.listener(|this, _: &Export, _, cx| this.export(ExportKind::Pdf, cx)));
        let mut body = div().relative().flex().flex_1().min_h_0().min_w_0();
        if self.library_open {
            self.canvas_bounds = None;
            body = body.child(self.library_sidebar(cx)).child(
                self.library(window, cx)
                    .opacity(library_alpha)
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                        for path in paths.paths() {
                            let id = this.controller.import_as_note(path.clone());
                            this.open_tabs.push(id);
                        }
                        this.show_editor();
                        cx.notify();
                    })),
            );
        } else {
            let header = self.header(cx);
            let toolbar = (!self.controller.read_only()).then(|| self.toolbar(cx));
            let formatting = if self.controller.read_only() {
                None
            } else {
                self.text_formatting_bar(cx)
            };
            let stale_index_notice = (self.controller.stale_handwriting_regions() > 0).then(|| {
                let count = self.controller.stale_handwriting_regions();
                div().px_4().py_2().flex().items_center().gap_3()
                    .child(div().flex_1().text_sm().child(format!("Handwriting changed in {count} indexed region{}. Reviewed text is retained and needs updating.",if count==1 {""} else {"s"})))
                    .child(self.button("review-stale-index","Review retained text",false,cx,|this,w,cx| {this.controller.review_stale_handwriting();this.modal(Modal::Recognition,w,cx);}))
                    .when(!self.controller.read_only(),|row| row.child(self.button("reindex-stale-page","Recognize page again",false,cx,|this,_,_| {if let Err(error)=this.controller.index_page_handwriting(){this.controller.status=error;}})))
            });
            let import_notice = self
                .controller
                .library_imports
                .get(&self.controller.active)
                .cloned()
                .map(|import| {
                    let id = self.controller.active;
                    let name = import
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let mut row = div().px_4().py_2().flex().items_center().gap_3().child(
                        div().flex_1().text_sm().child(if import.pending {
                            format!("Importing {name}…")
                        } else {
                            format!(
                                "Could not import {name}: {}",
                                import.error.unwrap_or_default()
                            )
                        }),
                    );
                    if !import.pending {
                        row = row
                            .child(self.button(
                                "retry-library-import",
                                "Retry import",
                                false,
                                cx,
                                move |this, _, _| this.controller.retry_library_import(id),
                            ))
                            .child(self.button(
                                "dismiss-library-import",
                                if self.controller.import_is_provisional(id) {
                                    "Remove failed import"
                                } else {
                                    "Keep document"
                                },
                                false,
                                cx,
                                move |this, _, _| this.controller.dismiss_failed_import(id),
                            ));
                    }
                    row
                });
            let trash_notice = self.controller.read_only().then(|| {
                div()
                    .px_4()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .child("In Trash · This document is read only"),
                    )
                    .child(self.button(
                        "restore-document",
                        "Restore document",
                        false,
                        cx,
                        |this, _, _| {
                            this.controller
                                .manage_note(this.controller.active, folio_app::NoteAction::Trash);
                            this.controller.set_tool(Tool::Pen);
                        },
                    ))
                    .child(
                        self.button("view-trash", "View Trash", false, cx, |this, _, _| {
                            this.controller.filter = NoteFilter::Trash;
                            this.show_library();
                        }),
                    )
            });
            let footer = self.footer(cx);
            let preview_notice =
                self.controller
                    .page_preview_error()
                    .map(str::to_owned)
                    .map(|message| {
                        div()
                            .px_4()
                            .py_2()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .child(format!("Preview unavailable: {message}")),
                            )
                            .child(self.button(
                                "retry-page-preview",
                                "Retry preview",
                                false,
                                cx,
                                |this, _, _| this.controller.retry_previews(),
                            ))
                    });
            let mut workspace = div().flex().flex_1().min_h_0();
            if self.pages_open {
                workspace = workspace.child(
                    div()
                        .flex()
                        .h_full()
                        .child(self.pages_panel(cx))
                        .opacity(pages_alpha),
                );
            }
            workspace = workspace.child(center);
            if self.controller.math_session.is_some() {
                workspace = workspace.child(self.math_panel(window, cx));
            }
            body = body.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .children(toolbar)
                    .children(trash_notice)
                    .children(import_notice)
                    .children(stale_index_notice)
                    .when(self.region_selection.is_some(), |body| {
                        body.child(
                            div()
                                .px_4()
                                .py_2()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child("Drag a rectangle on the page")
                                .child(self.button(
                                    "cancel-region",
                                    "Cancel · Esc",
                                    false,
                                    cx,
                                    |this, _, _| {
                                        this.region_selection = None;
                                        this.controller.status =
                                            "Region selection cancelled".into();
                                    },
                                )),
                        )
                    })
                    .when(self.diagnostics.check.is_some(), |body| {
                        body.child(self.input_check_panel(cx))
                    })
                    .children(formatting)
                    .children(preview_notice)
                    .child(workspace)
                    .child(footer),
            );
        }
        if self.document_menu.is_some() {
            self.building_overlay = self.modal.is_none()
                && !self.settings_open
                && !self.help_open
                && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.document_menu_panel(window, cx))
                    .opacity(menu_alpha),
            );
        }
        if self.settings_open {
            self.building_overlay =
                self.modal.is_none() && !self.help_open && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.settings_panel(window, cx))
                    .opacity(settings_alpha),
            );
        }
        if self.modal.is_some() {
            self.building_overlay = !self.help_open && self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.modal_panel(window, cx))
                    .opacity(modal_alpha),
            );
        }
        if self.help_open {
            self.building_overlay = self.controller.error.is_none();
            if self.building_overlay {
                self.accessibility.begin();
            }
            body = body.child(
                div()
                    .absolute()
                    .inset_0()
                    .child(self.help_panel(window, cx))
                    .opacity(help_alpha),
            );
        }
        if let Some(error) = self.controller.error.clone()
            && self.controller.interaction.is_none()
        {
            self.building_overlay = true;
            self.accessibility.begin();
            body = body.child(
                div()
                    .occlude()
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00000070))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(530.))
                            .p_6()
                            .bg(rgb(theme.surface))
                            .rounded(px(theme.radius + 4.))
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Folio needs your attention"),
                            )
                            .child(div().text_sm().child(error))
                            .child(self.button(
                                "dismiss-error",
                                "Close",
                                true,
                                cx,
                                |this, _, _| this.controller.error = None,
                            )),
                    ),
            );
        }
        self.building_overlay = false;
        root = root.child(self.document_tabs(window, cx));
        if let Some(error) = self.controller.save_error.clone() {
            root = root.child(
                div()
                    .px_4()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .bg(rgb(theme.surface))
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .child(format!("Changes are not saved. {error}")),
                    )
                    .child(
                        self.button("retry-save", "Retry saving", false, cx, |this, _, _| {
                            this.controller.retry_save()
                        }),
                    ),
            );
        }
        root = root.child(body);
        self.accessibility.publish(self, window, cx);
        titlebar::frame(root, window)
    }
}
