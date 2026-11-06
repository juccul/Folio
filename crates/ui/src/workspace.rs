//! Library, document chrome and page navigation. All mutations go through Controller.
use super::*;
use folio_document::Page;
use std::{collections::HashMap, sync::Arc};

/// How long ago a document changed, abbreviated: "Edited 3d ago".
pub(super) fn edited_label(updated: u64) -> String {
    let minutes = folio_document::now_ms().saturating_sub(updated) / 60_000;
    let age = match minutes {
        0 => return "Edited just now".into(),
        1..=59 => format!("{minutes}min"),
        60..=1439 => format!("{}h", minutes / 60),
        1440..=10079 => format!("{}d", minutes / 1440),
        10080..=43199 => format!("{}w", minutes / 10080),
        43200..=525_599 => format!("{}m", minutes / 43200),
        _ => format!("{}y", minutes / 525_600),
    };
    format!("Edited {age} ago")
}

pub(super) struct Hint(pub(super) SharedString, pub(super) Theme);
impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(self.1.radius))
            .shadow_md()
            .text_xs()
            .bg(rgb(self.1.ink))
            .text_color(rgb(self.1.surface))
            .child(self.0.clone())
    }
}

#[derive(Default)]
pub(super) struct Thumbnails {
    entries: HashMap<Id, Thumbnail>,
    pending: HashMap<Id, u64>,
    clock: u64,
}
#[derive(Clone, PartialEq)]
struct ThumbnailKey {
    revision: u64,
    properties: folio_document::PageProperties,
    theme: super::theme::CanvasTheme,
    pdf_ready: bool,
    /// Raster size in device pixels, so covers are drawn at the size shown
    /// instead of being downsampled until thin ink fades.
    edge: u32,
}
struct Thumbnail {
    key: ThumbnailKey,
    generation: u64,
    image: Option<Arc<RenderImage>>,
    error: Option<String>,
    aspect: f32,
    touched: u64,
}
impl Thumbnails {
    fn touch(&mut self, id: Id, key: &ThumbnailKey) -> (u64, bool) {
        self.clock += 1;
        let touched = self.clock;
        let valid = self.entries.get(&id).is_some_and(|entry| &entry.key == key);
        (touched, valid)
    }
    fn start(
        &mut self,
        id: Id,
        key: ThumbnailKey,
        aspect: f32,
        touched: u64,
        mut release: impl FnMut(Arc<RenderImage>),
    ) {
        if !self.entries.contains_key(&id)
            && self.entries.len() >= 48
            && let Some(oldest) = self
                .entries
                .iter()
                .filter(|(id, _)| !self.pending.contains_key(id))
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(id, _)| *id)
            && let Some(image) = self.entries.remove(&oldest).and_then(|entry| entry.image)
        {
            release(image);
        }
        if let Some(image) = self
            .entries
            .insert(
                id,
                Thumbnail {
                    key,
                    generation: touched,
                    image: None,
                    error: None,
                    aspect,
                    touched,
                },
            )
            .and_then(|entry| entry.image)
        {
            release(image);
        }
        self.pending.insert(id, touched);
    }
}
enum ThumbnailSource {
    Cover(Arc<Page>),
    Current(usize),
}
/// Library grid geometry, in logical pixels: cards stretch so each row
/// fills the shelf, between a minimum and maximum width.
#[derive(Clone, Copy)]
struct LibraryGrid {
    columns: usize,
    card: f32,
}
impl LibraryGrid {
    const GAP: f32 = 20.;
    const MIN: f32 = 172.;
    const MAX: f32 = 236.;
    /// Inner padding of a card around its cover.
    const INSET: f32 = 8.;
    fn new(window_width: f32, compact: bool) -> Self {
        // The folder sidebar is 13.5rem, at most 27% of the window, and the
        // shelf is padded by 1.75rem on each side.
        let sidebar = (13.5 * 16_f32).min(window_width * 0.27);
        let shelf = (window_width - sidebar - if compact { 32. } else { 56. }).max(Self::MIN);
        let fit = |width: f32| ((shelf + Self::GAP) / (width + Self::GAP)).floor().max(1.);
        let columns = fit(Self::MAX).max(fit(Self::MIN).min(fit(Self::MAX) + 1.)) as usize;
        let card = ((shelf - Self::GAP * (columns as f32 - 1.)) / columns as f32).min(Self::MAX);
        Self { columns, card }
    }
    /// Covers keep the proportions of a portrait page.
    fn cover_height(self) -> f32 {
        ((self.card - Self::INSET * 2.) * 1.35).round()
    }
    fn row_height(self) -> f32 {
        // Cover, title, details row and spacing.
        self.cover_height() + 108.
    }
}
impl NotesView {
    pub(super) fn navigate_folder_control(
        &mut self,
        id: Id,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.folder_rows.iter().position(|row| *row == id) else {
            return false;
        };
        let count = self.folder_rows.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        let target = self.folder_rows[next];
        self.folder_scroll.scroll_to_item(
            next,
            if forward {
                ScrollStrategy::Bottom
            } else {
                ScrollStrategy::Top
            },
        );
        self.controller.filter = NoteFilter::Notebook(target);
        self.accessibility
            .focus_virtual_control(&format!("folder-{target}"), window, cx);
        true
    }
    fn thumbnail(
        &mut self,
        source: ThumbnailSource,
        width: f32,
        max_height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let page = match &source {
            ThumbnailSource::Cover(page) => page.as_ref(),
            ThumbnailSource::Current(index) => &self.controller.session().document.pages[*index],
        };
        let theme = Theme::new(&self.controller.settings);
        let palette = theme.canvas_for_page(&page.properties);
        let bright_raster = theme.dark
            && (page.properties.pdf.is_some()
                || page
                    .objects
                    .values()
                    .any(|o| matches!(o.as_ref(), folio_document::Object::Image(_))));
        let pdf_ready = page
            .properties
            .pdf
            .as_ref()
            .and_then(|p| p.preview_asset.as_ref())
            .is_none_or(|a| self.controller.assets.join(a).is_file());
        let edge = ((max_height * window.scale_factor()).ceil() as u32).clamp(128, 768);
        // PDF previews are already rasters; resampling them small blurs text.
        let edge = if page.properties.pdf.is_some() {
            edge.max(384)
        } else {
            edge
        };
        let key = ThumbnailKey {
            revision: page.revision,
            properties: page.properties.clone(),
            theme: palette,
            pdf_ready,
            edge,
        };
        let id = page.id;
        let default_aspect = page.properties.width / page.properties.height.max(1.);
        let (touched, valid) = self.thumbnails.touch(id, &key);
        if !valid && self.thumbnails.pending.len() < 2 && !self.thumbnails.pending.contains_key(&id)
        {
            let generation = touched;
            let aspect = page.properties.width / page.properties.height.max(1.);
            self.thumbnails.start(id, key, aspect, touched, |image| {
                // Dropping the Arc alone leaves its sprite-atlas allocation alive.
                let _ = window.drop_image(image);
            });
            // Snapshot only when a raster job is needed. Cached redraws avoid
            // cloning the entire page/object map, and appearance adaptation runs
            // on the worker with the rasterization.
            let source = match source {
                ThumbnailSource::Cover(page) => page,
                ThumbnailSource::Current(_) => Arc::new(page.clone()),
            };
            let assets = self.controller.assets.clone();
            let task = cx.background_executor().spawn(async move {
                let mut page = source.as_ref().clone();
                export_options::apply(&mut page, theme, export_options::Appearance::Visible);
                // Covers are drawn at a fraction of page size; thicken ink so
                // handwriting stays legible instead of fading into the paper.
                let palette = theme.canvas_for_page(&page.properties);
                for object in page.objects.values_mut() {
                    match Arc::make_mut(object) {
                        folio_document::Object::Stroke(stroke) => {
                            stroke.style.width *= if theme.dark { 1.8 } else { 1.4 };
                            stroke.style.color = folio_document::Color::from_rgb(
                                palette.preview_ink(stroke.style.color.rgb()),
                            );
                        }
                        folio_document::Object::Text(text) => {
                            text.color = folio_document::Color::from_rgb(
                                palette.preview_ink(text.color.rgb()),
                            );
                        }
                        _ => {}
                    }
                }
                folio_export::raster_page_thumbnail(&page, &assets, edge)
                    .map(|p| {
                        let aspect = p.width() as f32 / p.height() as f32;
                        (graph::image(p.width(), p.height(), p.take()), aspect)
                    })
                    .map_err(|e| e.to_string())
            });
            cx.spawn(async move |view, cx| {
                let result = task.await;
                let _ = view.update(cx, |view, cx| {
                    view.thumbnails.pending.remove(&id);
                    if let Some(entry) = view.thumbnails.entries.get_mut(&id)
                        && entry.generation == generation
                    {
                        match result {
                            Ok((image, aspect)) => {
                                entry.image = Some(image);
                                entry.aspect = aspect;
                            }
                            Err(error) => entry.error = Some(error),
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let entry = self.thumbnails.entries.get_mut(&id);
        let (image, error, aspect) = if let Some(entry) = entry {
            entry.touched = touched;
            (entry.image.clone(), entry.error.clone(), entry.aspect)
        } else {
            (None, None, default_aspect)
        };
        let height = (width / aspect.max(0.01)).min(max_height);
        let corner = (theme.radius - 1.).max(0.);
        let mut element = div()
            .rounded(px(corner))
            .w(px((height * aspect).min(width)))
            .h(px(height))
            .overflow_hidden()
            .relative()
            .bg(rgb(palette.paper))
            .flex()
            .items_center()
            .justify_center();
        if valid && let Some(image) = image {
            // White PDF pages and images would otherwise glare beside dark
            // notebooks; a dark veil brings them to about 80 % brightness.
            let veil = bright_raster.then(|| {
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(corner))
                    .bg(rgba(0x00000059))
            });
            element = element.children(veil).child(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .rounded(px(corner)),
            );
        } else {
            element = element.child(
                div()
                    .p_2()
                    .text_xs()
                    .text_color(rgb(palette.foreground))
                    .child(if error.is_some() {
                        "Preview unavailable"
                    } else {
                        "Rendering preview…"
                    }),
            );
        }
        element
    }
}

#[derive(Clone)]
struct PageDrag(Id);
impl Render for PageDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_3()
            .bg(rgb(0xf0f0f0))
            .text_color(rgb(0x171717))
            .child("Move page")
    }
}
impl NotesView {
    /// Where a menu opened from control `key` appears: below the control, or
    /// at the pointer when the control has not been laid out.
    /// Below a control's right edge, so a menu that opens leftward covers
    /// the trailing action column instead of leaving it peeking out.
    pub(super) fn menu_anchor_end(&self, key: &str, window: &Window) -> Point<Pixels> {
        self.accessibility
            .control_bounds_for_key(key)
            .map_or(window.mouse_position(), |r| {
                point(px(r.x1 as f32), px(r.y1 as f32 + 4.))
            })
    }
    pub(super) fn menu_anchor(&self, key: &str, window: &Window) -> Point<Pixels> {
        self.accessibility
            .control_bounds_for_key(key)
            .map_or(window.mouse_position(), |r| {
                point(px(r.x0 as f32), px(r.y1 as f32 + 4.))
            })
    }
    /// Open a document without changing its model or viewport.
    pub fn show_editor(&mut self) {
        self.tool_menu = None;
        self.document_menu = None;
        self.folder_menu = None;
        self.library_open = false;
        let id = self.controller.active;
        self.controller.mark_note_opened(id);
        if !self.open_tabs.contains(&id) {
            self.open_tabs.push(id);
        }
    }
    pub(super) fn open_note(&mut self, id: Id) {
        self.controller.switch_note(id);
        if !self.open_tabs.contains(&id) {
            self.open_tabs.push(id);
        }
        self.tool_menu = None;
        self.document_menu = None;
        self.folder_menu = None;
        self.library_open = false;
        self.more_open = false;
        self.export_open = false;
        self.pen_settings = false;
    }
    pub(super) fn show_library(&mut self) {
        self.controller.finish();
        self.tool_menu = None;
        self.document_menu = None;
        self.folder_menu = None;
        self.library_open = true;
        self.canvas_bounds = None;
        self.more_open = false;
        self.export_open = false;
        self.pen_settings = false;
    }
    /// Quick creation from the tab picker, with the default page settings.
    pub(super) fn new_notebook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_inline_text(window, cx);
        if self.modal.is_some() {
            self.close_modal(window, cx);
        }
        self.controller.create_note();
        self.controller.set_tool(Tool::Pen);
        self.show_editor();
        self.fit();
        self.focus.focus(window);
        cx.notify();
    }
    fn toolbar_hints_enabled(&self) -> bool {
        !self.blocking_overlay()
            && self.tool_menu.is_none()
            && !self.pen_settings
            && !self.more_open
            && !self.export_open
    }
    pub(super) fn icon_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        kind: Icon,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let label = label.into();
        let hint = label.clone();
        self.control(
            id,
            label,
            icon(kind, theme.ink).into_any_element(),
            active,
            cx,
            action,
        )
        .size(rems(2.375))
        .p_0()
        .rounded(px(theme.radius))
        .when(self.toolbar_hints_enabled(), |button| {
            button.tooltip(move |_, cx| cx.new(|_| Hint(hint.clone(), theme)).into())
        })
    }
    fn editor_button(
        &self,
        id: &'static str,
        label: &'static str,
        kind: Icon,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let hint = if id == "eraser" {
            "Eraser · E · Click again for modes and size"
        } else {
            label
        };
        self.control(
            id,
            label,
            // A favorite on-state uses the accent, as in the library.
            icon(kind, theme.ink).size(rems(1.375)).into_any_element(),
            active,
            cx,
            action,
        )
        .w(rems(2.5))
        .h(rems(2.375))
        .min_h_0()
        .flex_shrink_0()
        .p_0()
        .rounded(rems(theme.radius / 16.))
        .bg(theme.tint(if active {
            1.
        } else {
            self.motion.borrow_mut().hover_value(id)
        }))
        .when(self.toolbar_hints_enabled(), |button| {
            button.tooltip(move |_, cx| cx.new(|_| Hint(hint.into(), theme)).into())
        })
    }
    /// A drawing tool. The current tool is filled with the primary color so
    /// it stands apart from neutral toggles such as Favorite.
    fn tool_button(
        &self,
        id: &'static str,
        label: &'static str,
        kind: Icon,
        current: bool,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let hint = if id == "eraser" {
            "Eraser · E · Click again for modes and size"
        } else {
            label
        };
        // Chevrons stay neutral beside a filled tool.
        let chevron = matches!(kind, Icon::Down);
        let current = current && !chevron;
        let (color, size) = if chevron {
            (theme.muted, 0.875)
        } else {
            (theme.ink, 1.5)
        };
        self.control(
            id,
            label,
            icon(kind, color).size(rems(size)).into_any_element(),
            active,
            cx,
            action,
        )
        .w(rems(2.5))
        .h(rems(2.375))
        .min_h_0()
        .flex_shrink_0()
        .p_0()
        .rounded(rems(theme.radius / 16.))
        // The active tool: a firm neutral tint with a full-strength icon.
        .bg(if current {
            rgba((theme.ink << 8) | if theme.dark { 0x38 } else { 0x24 })
        } else {
            theme.tint(self.motion.borrow_mut().hover_value(id))
        })
        .when(self.toolbar_hints_enabled(), |button| {
            button.tooltip(move |_, cx| cx.new(|_| Hint(hint.into(), theme)).into())
        })
    }
    fn editor_separator(&self) -> Div {
        let theme = Theme::new(&self.controller.settings);
        div()
            .w(px(1.))
            .h(rems(1.125))
            .mx(rems(0.3125))
            .flex_shrink_0()
            .bg(rgba((theme.ink << 8) | 0x29))
    }
    fn page_controls_group(&self) -> Div {
        div()
            .h(rems(2.625))
            .px(rems(0.25))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(rems(0.1875))
    }
    fn page_control_button(
        &self,
        id: &'static str,
        label: &'static str,
        kind: Icon,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        self.editor_button(id, label, kind, active, cx, action)
            // The group supplies the surface; buttons only highlight.
            .bg(theme.tint(if active {
                1.
            } else {
                self.motion.borrow_mut().hover_value(id)
            }))
    }
    fn folder_row(
        &self,
        n: &folio_document::Notebook,
        depth: usize,
        details: &folio_app::FolderDetails,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let id = n.id;
        let path = details.path.clone();
        let branch = details.branch;
        let active = matches!(self.controller.filter, NoteFilter::Notebook(folder) | NoteFilter::NotebookTrash(folder) if folder == id);
        let name = n.name.clone();
        let drag = super::folder_menu::LibraryDrag {
            item: super::folder_menu::LibraryItem::Folder(id),
            title: name.clone(),
            theme,
            position: Point::default(),
        };
        let content = div()
            .flex()
            .min_w_0()
            .items_center()
            .gap_3()
            .child(icon(Icon::Folder, theme.muted))
            .child(div().min_w_0().truncate().child(name));
        let open = self
            .control(
                format!("folder-{id}"),
                path.clone(),
                content.into_any_element(),
                active,
                cx,
                move |this, _, _| this.controller.filter = NoteFilter::Notebook(id),
            )
            .flex_1()
            .min_w_0()
            .h_full()
            .justify_start()
            .rounded_none()
            .bg(transparent_black())
            .on_drag(drag, |drag, position, _, cx| {
                cx.new(|_| {
                    let mut drag = drag.clone();
                    drag.position = position;
                    drag
                })
            });
        let row = div()
            .id(SharedString::from(format!("folder-row-{id}")))
            .flex()
            .flex_shrink_0()
            .h(rems(2.75))
            .items_center()
            .ml(px(depth as f32 * 12.))
            .rounded(px(theme.radius))
            .bg(theme.tint(if active { 1. } else { 0. }))
            .hover(move |s| s.bg(theme.tint(1.)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                    this.open_folder_menu(id, event.position, w, cx);
                    cx.stop_propagation();
                }),
            )
            .child(open)
            // Disclosure sits after the name so folder icons align with the
            // navigation icons above.
            .child(if branch {
                self.control(
                    format!("toggle-folder-{id}"),
                    format!("{} {path}", if collapsed { "Expand" } else { "Collapse" }),
                    icon(
                        if collapsed { Icon::Forward } else { Icon::Down },
                        theme.muted,
                    )
                    .size(rems(0.875))
                    .into_any_element(),
                    !collapsed,
                    cx,
                    move |this, _, cx| {
                        let folders = &mut this.controller.settings.collapsed_folders;
                        if folders.contains(&id) {
                            folders.retain(|f| *f != id);
                        } else {
                            folders.push(id);
                        }
                        this.controller.store_settings();
                        cx.stop_propagation();
                    },
                )
                .size(px(24.))
                .p_0()
                .bg(transparent_black())
                .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(
                self.control(
                    format!("manage-folder-{id}"),
                    format!("Manage folder {path}"),
                    icon(Icon::More, theme.ink).into_any_element(),
                    false,
                    cx,
                    move |this, w, cx| {
                        let at = this.menu_anchor(&format!("manage-folder-{id}"), w);
                        this.open_folder_menu(id, at, w, cx);
                        cx.stop_propagation();
                    },
                )
                .size(rems(1.75))
                .min_h(rems(1.75))
                .p_0()
                .mr_1()
                .bg(transparent_black())
                .hover(move |s| s.bg(theme.tint(1.))),
            );
        self.folder_drop_target(row, Some(id), cx)
    }
    pub(super) fn library_sidebar(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut filters = div().flex().flex_col().gap_1().px_3();
        for (id, label, kind, filter) in [
            ("all", "Documents", Icon::Library, NoteFilter::All),
            ("favorite", "Favorites", Icon::Star, NoteFilter::Favorites),
            ("recent", "Recent", Icon::Clock, NoteFilter::Recent),
            ("trash", "Trash", Icon::Trash, NoteFilter::Trash),
        ] {
            let active = self.controller.filter == filter
                || (filter == NoteFilter::Trash
                    && matches!(self.controller.filter, NoteFilter::NotebookTrash(_)));
            let content = div()
                .flex()
                .items_center()
                .gap_3()
                .child(icon(kind, if active { theme.ink } else { theme.muted }))
                .child(label);
            filters = filters.child(
                self.control(
                    id,
                    label,
                    content.into_any_element(),
                    active,
                    cx,
                    move |this, _, _| this.controller.filter = filter,
                )
                .w_full()
                .h(px(44.))
                .justify_start()
                .px_3()
                .bg(theme.tint(if active { 1. } else { 0. }))
                .hover(move |s| s.bg(theme.tint(1.)))
                .when(filter == NoteFilter::All, |s| {
                    self.folder_drop_target(s, None, cx).tooltip(move |_, cx| {
                        cx.new(|_| Hint("Drop here to remove from its folder".into(), theme))
                            .into()
                    })
                }),
            );
        }
        let mut folders = div()
            .id("notebook-list")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_1()
            .px_3();
        let mut collapsed_depth = None;
        let collapsed: std::collections::HashSet<_> = self
            .controller
            .settings
            .collapsed_folders
            .iter()
            .copied()
            .collect();
        let rows: Vec<_> = self
            .controller
            .folder_tree()
            .into_iter()
            .filter(|(folder, depth)| {
                if collapsed_depth.is_some_and(|hidden| *depth > hidden) {
                    return false;
                }
                collapsed_depth = collapsed.contains(&folder.id).then_some(*depth);
                true
            })
            .collect();
        self.folder_rows = rows.iter().map(|(folder, _)| folder.id).collect();
        if !rows.is_empty() {
            folders = folders.child(
                uniform_list(
                    "folder-rows",
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                        let visible = &rows[range];
                        let details = this
                            .controller
                            .folder_details(visible.iter().map(|(folder, _)| folder.id));
                        let rendered: Vec<_> = visible
                            .iter()
                            .map(|(folder, depth)| {
                                div().h(rems(3.)).child(this.folder_row(
                                    folder,
                                    *depth,
                                    &details[&folder.id],
                                    collapsed.contains(&folder.id),
                                    cx,
                                ))
                            })
                            .collect();
                        this.accessibility
                            .retain_folder_items(visible.iter().map(|(folder, _)| folder.id));
                        this.resume_poll(cx);
                        this.accessibility.publish(this, window, cx);
                        rendered
                    }),
                )
                .track_scroll(self.folder_scroll.clone())
                .flex_1()
                .min_h_0(),
            );
        }
        if self.controller.notebooks.is_empty() {
            folders = folders.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Keep related documents together."),
            );
        }
        div()
            .w(rems(13.5))
            .max_w(relative(0.27))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_5()
            .glass_fill(theme, theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .pt_5()
            .child(filters)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_5()
                    .pt_3()
                    .child(
                        div()
                            .text_size(rems(0.8125))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.secondary()))
                            .child("Folders"),
                    )
                    .child(
                        self.icon_button(
                            "add-notebook",
                            "New folder",
                            Icon::Plus,
                            false,
                            cx,
                            |this, w, cx| this.modal(Modal::Notebook(None), w, cx),
                        )
                        .size(rems(1.875)),
                    ),
            )
            .child(folders)
            .child(
                div()
                    .px_4()
                    .py_4()
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        self.control(
                            "settings",
                            "Settings",
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(icon(Icon::Settings, theme.muted))
                                .child("Settings")
                                .into_any_element(),
                            false,
                            cx,
                            |this, w, _| this.open_settings(w),
                        )
                        .flex_1()
                        .justify_start(),
                    )
                    .child(self.icon_button(
                        "help",
                        "Keyboard shortcuts",
                        Icon::Help,
                        false,
                        cx,
                        |this, w, _| this.open_help(w),
                    )),
            )
    }
    fn library_items(
        &mut self,
        notes: Vec<folio_document::NoteMetadata>,
        list_view: bool,
        grid: LibraryGrid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut items = div()
            .w_full()
            .flex()
            .when(list_view, |s| s.flex_col())
            .when(!list_view, |s| s.gap(rems(LibraryGrid::GAP / 16.)));
        for n in notes {
            let id = n.id;
            let drag = super::folder_menu::LibraryDrag {
                item: super::folder_menu::LibraryItem::Document(id),
                title: n.title.clone(),
                theme,
                position: Point::default(),
            };
            let session_pages = self
                .controller
                .sessions
                .get(&id)
                .map(|s| s.document.pages.len());
            let preview = self.controller.library_preview(id);
            let pages = preview.as_ref().map(|(_, count)| *count).or(session_pages);
            // The document whose menu is open keeps its highlight.
            let menu_open = self.document_menu.is_some();
            let menu_owner =
                !self.document_menu_tab && self.document_menu.is_some_and(|(menu, _)| menu == id);
            let import_failed = self
                .controller
                .library_imports
                .get(&id)
                .is_some_and(|import| !import.pending);
            let details = self
                .controller
                .library_imports
                .get(&id)
                .map(|import| {
                    if import.pending {
                        "Importing…".to_owned()
                    } else {
                        "Import failed · open to retry".to_owned()
                    }
                })
                .unwrap_or_else(|| {
                    pages
                        .map(|count| format!("{count} page{}", if count == 1 { "" } else { "s" }))
                        .unwrap_or_else(|| "Document".into())
                });
            let page_count =
                pages.map(|count| format!("{count} page{}", if count == 1 { "" } else { "s" }));
            let details = if n.tags.is_empty() {
                details
            } else {
                format!("{details} · {}", n.tags.join(" · "))
            };
            let favorite = self
                .control(
                    format!("favorite-card-{id}"),
                    format!(
                        "{} {} {} favorites",
                        if n.favorite { "Remove" } else { "Add" },
                        n.title,
                        if n.favorite { "from" } else { "to" }
                    ),
                    icon(
                        if n.favorite {
                            Icon::StarFilled
                        } else {
                            Icon::Star
                        },
                        if n.favorite {
                            theme.ink
                        } else {
                            theme.secondary()
                        },
                    )
                    .size(rems(1.))
                    .into_any_element(),
                    n.favorite,
                    cx,
                    move |this, _, cx| {
                        this.controller
                            .manage_note(id, folio_app::NoteAction::Favorite);
                        cx.stop_propagation();
                    },
                )
                .occlude()
                .size(rems(2.))
                .p_0()
                .min_h(rems(2.))
                .flex_shrink_0()
                .bg(transparent_black())
                .hover(move |s| s.bg(theme.tint(1.)))
                .tooltip({
                    let label: SharedString = if n.favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    }
                    .into();
                    move |_, cx| cx.new(|_| Hint(label.clone(), theme)).into()
                });
            let menu = self
                .icon_button(
                    format!("manage-note-{id}"),
                    format!("Manage {}", n.title),
                    Icon::More,
                    false,
                    cx,
                    move |this, w, cx| {
                        let at = this.menu_anchor_end(&format!("manage-note-{id}"), w);
                        let keyboard = !this.pointer_activation;
                        this.open_document_menu(id, at, false, keyboard, w, cx);
                        cx.stop_propagation();
                    },
                )
                .size(rems(2.))
                .min_h(rems(2.))
                .flex_shrink_0()
                .bg(transparent_black())
                .hover(move |s| s.bg(theme.tint(1.)));
            let open =
                move |this: &mut Self, _: &mut Window, _: &mut Context<Self>| this.open_note(id);
            let context_menu = cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                this.open_document_menu(id, event.position, false, false, w, cx);
                cx.stop_propagation();
            });
            let title = div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(n.title.clone());
            if list_view {
                let content = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(icon(Icon::Book, theme.muted).size(rems(1.25)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(title.text_size(rems(0.9375)))
                            // A second line only when there is something to say:
                            // an import error or tags.
                            .when(import_failed || !n.tags.is_empty(), |column| {
                                column.child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(theme.secondary()))
                                        .truncate()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .when(import_failed, |line| {
                                            line.child(
                                                icon(Icon::Alert, theme.error_text())
                                                    .size(rems(0.875)),
                                            )
                                        })
                                        .child({
                                            let text = if import_failed {
                                                details.clone()
                                            } else {
                                                n.tags.join(" · ")
                                            };
                                            // Only the failure is red.
                                            let failure = if import_failed {
                                                vec![(
                                                    0..details.len(),
                                                    HighlightStyle {
                                                        color: Some(rgb(theme.error_text()).into()),
                                                        font_weight: Some(FontWeight::MEDIUM),
                                                        ..Default::default()
                                                    },
                                                )]
                                            } else {
                                                Vec::new()
                                            };
                                            StyledText::new(text).with_highlights(failure)
                                        }),
                                )
                            }),
                    )
                    // Size and age, right-aligned beside the row actions.
                    .child(
                        div()
                            .w(rems(14.))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .pr_4()
                            .text_sm()
                            .text_color(rgb(theme.secondary()))
                            .truncate()
                            .child(
                                page_count
                                    .clone()
                                    .map(|count| {
                                        format!(
                                            "{count} · {}",
                                            edited_label(n.updated_at)
                                                .trim_start_matches("Edited ")
                                        )
                                    })
                                    .unwrap_or_else(|| edited_label(n.updated_at)),
                            ),
                    );
                items = items.child(
                    div()
                        .id(SharedString::from(format!("note-row-{id}")))
                        .w_full()
                        .h(rems(3.75))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .relative()
                        .rounded(px(theme.corner_md()))
                        .border_1()
                        .border_color(transparent_black())
                        .when(menu_owner, |row| {
                            row.bg(theme.tint(0.7)).border_color(theme.cover_edge())
                        })
                        .when(!menu_open, |row| row.hover(move |s| s.bg(theme.tint(0.45))))
                        .pr_2()
                        .child(
                            self.control(
                                format!("note-{id}"),
                                format!("Open {}", n.title),
                                content.into_any_element(),
                                false,
                                cx,
                                open,
                            )
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .px_3()
                            .py_0()
                            .justify_start()
                            .rounded(px(theme.corner_md()))
                            .bg(transparent_black())
                            .hover(|s| s.bg(transparent_black()))
                            .on_mouse_down(MouseButton::Right, context_menu)
                            .when(!n.trashed, |s| {
                                s.on_drag(drag, |drag, position, _, cx| {
                                    cx.new(|_| {
                                        let mut drag = drag.clone();
                                        drag.position = position;
                                        drag
                                    })
                                })
                            }),
                        )
                        .child(favorite)
                        .child(menu)
                        // An inset hairline ties each title to its actions.
                        .child(div().absolute().bottom_0().left_3().right_3().h(px(1.)).bg(
                            if theme.dark {
                                theme.cover_edge()
                            } else {
                                theme.border
                            },
                        )),
                );
            } else {
                let cover_content = if let Some((page, _)) = &preview {
                    if let Some(pdf) = &page.properties.pdf {
                        self.controller.request_pdf_background(pdf.clone());
                    }
                    self.thumbnail(
                        ThumbnailSource::Cover(page.clone()),
                        (grid.card - LibraryGrid::INSET * 2.) * self.controller.settings.ui_scale,
                        grid.cover_height() * self.controller.settings.ui_scale,
                        window,
                        cx,
                    )
                } else {
                    div().text_xs().text_color(rgb(theme.muted)).child(
                        if self.controller.library_cover_unavailable(id) {
                            "Preview unavailable"
                        } else {
                            "Loading preview…"
                        },
                    )
                };
                let cover = div()
                    .relative()
                    .w_full()
                    .h(rems(grid.cover_height() / 16.))
                    .overflow_hidden()
                    .rounded(px(theme.radius))
                    .border_1()
                    .border_color(if menu_owner {
                        rgb(theme.muted).into()
                    } else {
                        theme.cover_edge()
                    })
                    .bg(rgb(theme.sidebar))
                    .shadow_sm()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(cover_content)
                    // Blank documents get a quiet mark, so the grid does not
                    // read as a wall of identical pages.
                    // A failed import tints its cover and says so; blank
                    // documents show their title as a quiet page heading.
                    .when(import_failed, |cover| {
                        cover.child(
                            div()
                                .absolute()
                                .inset_0()
                                .bg(rgba((theme.destructive << 8) | 0x14))
                                .flex()
                                .flex_col()
                                .gap_2()
                                .items_center()
                                .justify_center()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.error_text()))
                                .child(icon(Icon::Alert, theme.error_text()).size(rems(1.75)))
                                .child("Import failed"),
                        )
                    })
                    .when(
                        !import_failed
                            && preview.as_ref().is_some_and(|(page, _)| {
                                page.objects.is_empty() && page.properties.pdf.is_none()
                            }),
                        |cover| {
                            cover.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .p_4()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div()
                                            .text_center()
                                            .text_size(rems(1.0625))
                                            .font_weight(FontWeight::MEDIUM)
                                            .line_clamp(3)
                                            .text_color(rgb(theme.muted))
                                            .child(n.title.clone()),
                                    ),
                            )
                        },
                    );
                let content = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(cover)
                    .child(title.w_full().text_size(rems(0.9375)));
                let edited = edited_label(n.updated_at)
                    .trim_start_matches("Edited ")
                    .to_owned();
                // The whole card highlights together: cover, title, details
                // and actions.
                items = items.child(
                    div()
                        .id(SharedString::from(format!("note-card-{id}")))
                        .w(rems(grid.card / 16.))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .pb_2()
                        .rounded(px(theme.corner_md()))
                        .group("note-card")
                        .border_1()
                        .border_color(transparent_black())
                        .when(menu_owner, |card| {
                            card.bg(theme.tint(0.7)).border_color(theme.cover_edge())
                        })
                        .when(!menu_open, |card| {
                            card.hover(move |s| s.bg(theme.tint(0.45)))
                        })
                        .child(
                            self.control(
                                format!("note-{id}"),
                                format!("Open {}", n.title),
                                content.into_any_element(),
                                false,
                                cx,
                                open,
                            )
                            .w_full()
                            .min_w_0()
                            .p_2()
                            .pb_1()
                            .rounded(px(theme.corner_md()))
                            .bg(transparent_black())
                            .hover(|s| s.bg(transparent_black()))
                            .on_mouse_down(MouseButton::Right, context_menu)
                            .when(!n.trashed, |s| {
                                s.on_drag(drag, |drag, position, _, cx| {
                                    cx.new(|_| {
                                        let mut drag = drag.clone();
                                        drag.position = position;
                                        drag
                                    })
                                })
                            }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .pl_2()
                                .pr_1()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(rems(0.8125))
                                        .text_color(rgb(if import_failed {
                                            theme.error_text()
                                        } else {
                                            theme.secondary()
                                        }))
                                        .truncate()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .when(import_failed, |line| {
                                            line.font_weight(FontWeight::MEDIUM).child(
                                                icon(Icon::Alert, theme.error_text())
                                                    .size(rems(0.875)),
                                            )
                                        })
                                        .child(if import_failed {
                                            "Import failed · Retry".to_owned()
                                        } else {
                                            format!("{details} · {edited}")
                                        }),
                                )
                                .child(favorite.size(rems(1.75)).min_h(rems(1.75)))
                                .child(menu.size(rems(1.75)).min_h(rems(1.75))),
                        ),
                );
            }
        }
        items
    }
    pub(super) fn library(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let compact =
            f32::from(window.viewport_size().width) / self.controller.settings.ui_scale < 900.;
        // Short, narrow windows only fit the list; Grid is disabled there.
        self.list_forced = compact
            && f32::from(window.viewport_size().height) / self.controller.settings.ui_scale < 450.;
        let list_view = self.list_view || self.list_forced;
        let title = match self.controller.filter {
            NoteFilter::All => "Documents".to_string(),
            NoteFilter::Favorites => "Favorites".into(),
            NoteFilter::Recent => "Recent".into(),
            NoteFilter::Trash => "Trash".into(),
            NoteFilter::NotebookTrash(id) => format!("Trash · {}", self.controller.folder_path(id)),
            NoteFilter::Notebook(id) => self
                .controller
                .notebooks
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.name.clone())
                .unwrap_or("Folder".into()),
        };
        let mut notes = self
            .controller
            .visible_notes()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        library_sort::sort_notes(
            &mut notes,
            self.sort_by_name,
            self.sort_reverse,
            self.controller.filter == NoteFilter::Recent,
        );
        let count = notes.len();
        let trash_folder = match self.controller.filter {
            NoteFilter::NotebookTrash(folder) => Some(folder),
            _ => None,
        };
        let trash_count = if matches!(
            self.controller.filter,
            NoteFilter::Trash | NoteFilter::NotebookTrash(_)
        ) {
            count
        } else {
            0
        };
        let mut shelf = div()
            .id("library-shelf")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p_7()
            .pt_2()
            .when(count == 0 && compact, |shelf| shelf.p_4())
            .when(count == 0, |shelf| shelf.overflow_y_scroll());
        let grid = LibraryGrid::new(
            f32::from(window.viewport_size().width) / self.controller.settings.ui_scale,
            compact,
        );
        let columns = if list_view { 1 } else { grid.columns };
        let total = notes.len();
        let list = uniform_list(
            "library-rows",
            total.div_ceil(columns),
            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                let mut rows = Vec::new();
                let visible_start = range.start * columns;
                let visible_end = (range.end * columns).min(total);
                for row in range {
                    let start = row * columns;
                    let end = (start + columns).min(total);
                    rows.push(
                        this.library_items(notes[start..end].to_vec(), list_view, grid, window, cx)
                            .h(rems(if list_view {
                                3.75
                            } else {
                                grid.row_height() / 16.
                            }))
                            .items_start(),
                    );
                }
                this.accessibility
                    .retain_library_items(&notes[visible_start..visible_end]);
                this.resume_poll(cx);
                this.accessibility.publish(this, window, cx);
                rows
            }),
        )
        .flex_1()
        .min_h_0();
        if count == 0 {
            let (heading, hint, kind) = match self.controller.filter {
                NoteFilter::Trash | NoteFilter::NotebookTrash(_) => (
                    "Trash is empty",
                    "Documents you move to Trash appear here.",
                    Icon::Trash,
                ),
                NoteFilter::Favorites => (
                    "No favorites yet",
                    "Star a document to keep it here.",
                    Icon::Star,
                ),
                NoteFilter::Recent => (
                    "No recent documents",
                    "Documents you open appear here.",
                    Icon::Clock,
                ),
                NoteFilter::Notebook(_) => (
                    "This folder is empty",
                    "Create a document here, or drag documents into the folder.",
                    Icon::Folder,
                ),
                NoteFilter::All => (
                    "No documents yet",
                    "Create a document, or import a PDF or image.",
                    Icon::Book,
                ),
            };
            let creates = matches!(
                self.controller.filter,
                NoteFilter::All | NoteFilter::Notebook(_)
            );
            shelf = shelf.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        div()
                            .size(rems(4.))
                            .rounded(px(theme.corner_lg()))
                            .bg(theme.raised())
                            .border_1()
                            .border_color(theme.glass_edge())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(kind, theme.muted).size(rems(1.75))),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(heading),
                    )
                    .child(div().text_sm().text_color(rgb(theme.muted)).child(hint))
                    .when(creates, |empty| {
                        empty.child(
                            self.button(
                                "empty-new-document",
                                "New document",
                                false,
                                cx,
                                |this, w, cx| this.modal(Modal::NewDocument, w, cx),
                            )
                            .mt_2()
                            .px_4()
                            .border_1()
                            .border_color(theme.glass_edge()),
                        )
                    }),
            );
        } else {
            shelf = shelf.child(list);
        }
        let breadcrumb = if let NoteFilter::Notebook(id) = self.controller.filter {
            let mut row = div()
                .id("folder-breadcrumb")
                .flex()
                .items_center()
                .gap_1()
                .overflow_x_scroll();
            row = row.child(
                self.button("breadcrumb-root", "Documents", false, cx, |this, _, _| {
                    this.controller.filter = NoteFilter::All
                })
                .text_size(rems(0.8125))
                .text_color(rgb(theme.secondary()))
                .py_0()
                .px_1()
                // Short enough that the header keeps the height of other views.
                .min_h_0()
                .h(rems(1.25)),
            );
            for folder in self
                .controller
                .folder_ancestors(id)
                .into_iter()
                .filter(|folder| folder.id != id)
            {
                let target = folder.id;
                row = row
                    .child(icon(Icon::Forward, theme.muted).size(rems(0.75)))
                    .child(
                        self.button(
                            format!("breadcrumb-{target}"),
                            folder.name,
                            target == id,
                            cx,
                            move |this, _, _| this.controller.filter = NoteFilter::Notebook(target),
                        )
                        .text_xs()
                        .py_0()
                        .px_1()
                        .min_h_0()
                        .h(rems(1.25))
                        .max_w(px(150.))
                        .truncate(),
                    );
            }
            Some(row.child(icon(Icon::Forward, theme.secondary()).size(rems(0.75))))
        } else if let NoteFilter::NotebookTrash(id) = self.controller.filter {
            Some(
                div()
                    .id("folder-trash-navigation")
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(self.button(
                        "back-to-folder",
                        "Back to folder",
                        false,
                        cx,
                        move |this, _, _| this.controller.filter = NoteFilter::Notebook(id),
                    ))
                    .child(self.button(
                        "view-all-trash",
                        "View all Trash",
                        false,
                        cx,
                        |this, _, _| this.controller.filter = NoteFilter::Trash,
                    )),
            )
        } else {
            None
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    // Room for the folder breadcrumb in every view, so the
                    // shelf does not shift when a folder opens.
                    .min_h(rems(5.75))
                    .pt_4()
                    .pb_2()
                    .flex_wrap()
                    .px_8()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex_1()
                            .when(compact, |title| title.flex_initial().w_full())
                            .min_w_0()
                            .relative()
                            .flex()
                            .flex_col()
                            // Parent folders sit just above the title, outside
                            // the flow, so the title never moves between views.
                            .children(breadcrumb.map(|crumbs| {
                                div()
                                    .absolute()
                                    .left_0()
                                    .bottom_full()
                                    .h(rems(1.25))
                                    .flex()
                                    .items_center()
                                    .child(crumbs)
                            }))
                            .child(
                                div()
                                    .text_size(rems(1.5625))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .when(trash_count > 0, |row| {
                                row.child(self.button(
                                    "empty-trash",
                                    "Empty Trash…",
                                    false,
                                    cx,
                                    move |this, w, cx| {
                                        this.modal(
                                            Modal::Confirm(super::Confirm::EmptyTrash(
                                                trash_folder,
                                            )),
                                            w,
                                            cx,
                                        )
                                    },
                                ))
                            })
                            .child(self.icon_button(
                                "search",
                                "Search all documents · Ctrl+F",
                                Icon::Search,
                                self.find.is_some(),
                                cx,
                                |this, w, cx| this.open_find(w, cx),
                            ))
                            .child(
                                self.control(
                                    "library-import",
                                    "Import PDF or image",
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(icon(Icon::Import, theme.accent))
                                        .child("Import…")
                                        .into_any_element(),
                                    false,
                                    cx,
                                    |this, _, cx| this.import(cx),
                                ),
                            )
                            .child(
                                self.control(
                                    "library-new",
                                    "New document",
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            icon(Icon::Plus, theme.primary_foreground)
                                                .size(px(24.)),
                                        )
                                        .child("New document")
                                        .into_any_element(),
                                    true,
                                    cx,
                                    |this, w, cx| this.modal(Modal::NewDocument, w, cx),
                                )
                                .bg(rgb(theme.accent))
                                .text_color(rgb(theme.primary_foreground)),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(58.))
                    .px_8()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.sort_control(cx))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.secondary()))
                            .child(format!(
                                "{count} document{}",
                                if count == 1 { "" } else { "s" }
                            )),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .gap(px(2.))
                            .p(px(2.))
                            .rounded(px(theme.corner_md()))
                            .bg(theme.raised())
                            .border_1()
                            .border_color(theme.glass_edge())
                            .child(
                                self.icon_button(
                                    "grid-view",
                                    "Grid view",
                                    Icon::Grid,
                                    !list_view,
                                    cx,
                                    |this, _, _| this.list_view = false,
                                )
                                .size(rems(2.)),
                            )
                            .child(
                                self.icon_button(
                                    "list-view",
                                    "List view",
                                    Icon::List,
                                    list_view,
                                    cx,
                                    |this, _, _| this.list_view = true,
                                )
                                .size(rems(2.)),
                            ),
                    ),
            )
            .child(shelf)
            .child(
                div()
                    .h(px(36.))
                    .px_8()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(theme.secondary()))
                    // The idle save state belongs to the editor; the library
                    // shows only activity.
                    .child({
                        let status = self.controller.activity_status();
                        if status == "All changes saved" {
                            String::new()
                        } else {
                            status
                        }
                    })
                    .children(self.library_undo_button(cx)),
            )
    }
    /// "Undo" next to the status after a rename, move, favorite or Trash.
    pub(super) fn library_undo_button(&self, cx: &mut Context<Self>) -> Option<Div> {
        let label = self.controller.library_undo_offer()?.to_owned();
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().child("·"))
                .child(label)
                .child(
                    self.button("undo-library", "Undo", false, cx, |this, _, _| {
                        this.controller.undo_library()
                    })
                    .text_xs()
                    .py_0()
                    .px_2()
                    .min_h_0(),
                ),
        )
    }
    pub(super) fn document_tabs(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let active = self.controller.requested_note();
        if !self.library_open && !self.open_tabs.contains(&active) {
            self.open_tabs.push(active);
        }
        self.open_tabs
            .retain(|id| self.controller.notes.iter().any(|n| n.id == *id));
        if let Some(index) = self.open_tabs.iter().position(|id| *id == active) {
            let target = (
                active,
                index,
                window.viewport_size().width,
                self.controller.settings.ui_scale,
            );
            if self.tab_target != Some(target) {
                self.tab_scroll.scroll_to_item(index);
                self.tab_target = Some(target);
            }
        }
        let mut tabs = div()
            .id("document-tabs")
            .track_scroll(&self.tab_scroll)
            .flex_initial()
            .min_w_0()
            .overflow_x_scroll()
            .flex()
            .items_center()
            .h_full()
            .gap_1();
        // Home stays pinned at the start of the bar while the tabs scroll.
        let home = div()
                .flex_shrink_0()
                .relative()
                .h_full()
                .flex()
                .items_center()
                // Let the button register its click before stopping the press at
                // this wrapper. Windows' native move loop otherwise steals release.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .child(
                    self.icon_button(
                        "tab-library",
                        "Library · Ctrl+Shift+L",
                        Icon::Library,
                        self.library_open,
                        cx,
                        |this, _, _| this.show_library(),
                    )
                    .w(rems(2.25))
                    .p_0()
                    .h(rems(1.75))
                    .min_h_0()
                    .border_0()
                    .rounded(rems(theme.radius / 16.))
                    .bg(theme.tint(if self.library_open { 1. } else { 0. }))
                    .focus(move |s| s.bg(theme.tint(1.)))
                    .hover(move |s| s.bg(theme.tint(1.))),
                );
        // While a tab is dragged it follows the pointer and the others slide
        // out of its way.
        let dragging = self.tab_drag.as_ref().filter(|drag| drag.moving);
        let dragged = dragging.map(|drag| drag.id);
        let targets: std::collections::HashMap<Id, f32> = dragging
            .map(|drag| drag.offsets().into_iter().collect())
            .unwrap_or_default();
        let reduced = self.controller.settings.reduce_motion;
        let now = Instant::now();
        for id in self.open_tabs.clone() {
            let Some(n) = self.controller.notes.iter().find(|n| n.id == id) else {
                continue;
            };
            let selected = !self.library_open && id == active;
            let unsaved = self.controller.has_unsaved_changes(id);
            let offset_key = format!("tab-offset-{id}");
            let offset = {
                let mut motion = self.motion.borrow_mut();
                if dragged == Some(id) {
                    let offset = targets.get(&id).copied().unwrap_or(0.);
                    motion.jump(&offset_key, offset);
                    offset
                } else {
                    motion.value(
                        &offset_key,
                        targets.get(&id).copied().unwrap_or(0.),
                        reduced,
                        now,
                    )
                }
            };
            let bounds = self.tab_bounds.clone();
            let content = div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(icon(Icon::Book, theme.muted))
                .child(
                    div()
                        .max_w(px(190.))
                        .truncate()
                        .text_sm()
                        .child(n.title.clone()),
                )
                .when(unsaved, |content| {
                    content.child(
                        div()
                            .size(px(7.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(rgb(theme.muted)),
                    )
                });
            let tab_label = if unsaved {
                format!("Open {} · unsaved changes", n.title)
            } else {
                format!("Open {}", n.title)
            };
            let surface = div()
                    .id(SharedString::from(format!("tab-surface-{id}")))
                    .relative()
                    .left(px(offset))
                    .h(rems(1.75))
                    .rounded(rems(theme.radius / 16.))
                    .hover(move |s| s.bg(theme.tint(1.)))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .bg(theme.tint(if selected { 1. } else { 0. }))
                    .child(
                        canvas(
                            move |layout, _, _| {
                                bounds.borrow_mut().insert(
                                    id,
                                    (
                                        f32::from(layout.origin.x) - offset,
                                        f32::from(layout.size.width),
                                    ),
                                );
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    // The outer tab consumes presses after its child controls
                    // have registered clicks and tab drags.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                            this.open_document_menu(id, event.position, true, false, w, cx);
                            cx.stop_propagation();
                        }),
                    )
                    // Middle-click closes a tab, as in browsers and editors.
                    .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(move |this, _, w, cx| {
                            this.close_document_tab(id, w, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        self.control(
                            format!("tab-{id}"),
                            tab_label,
                            content.into_any_element(),
                            selected,
                            cx,
                            move |this, _, _| {
                                // A completed drag reorders tabs without also
                                // activating the tab as a normal click.
                                if this.tab_drag.as_ref().is_none_or(|drag| !drag.moving) {
                                    this.open_note(id);
                                }
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                                if event.click_count == 2 {
                                    // Open after this press has focused the tab,
                                    // so the dialog's field keeps the focus.
                                    cx.on_next_frame(w, move |this, w, cx| {
                                        this.modal(Modal::RenameDocument(id), w, cx)
                                    });
                                } else {
                                    this.press_tab(id, f32::from(event.position.x), cx);
                                }
                            }),
                        )
                        .h(rems(1.75))
                        .min_h_0()
                        .pl_2()
                        .pr_1()
                        .py_0()
                        .border_0()
                        .rounded(rems(theme.radius / 16.))
                        .bg(transparent_black())
                        .text_color(rgb(theme.ink))
                        .focus(move |s| s.bg(theme.tint(1.)))
                        .hover(|s| s.bg(transparent_black())),
                    )
                    .child(
                        self.control(
                            format!("close-tab-{id}"),
                            format!("Close {}", n.title),
                            icon(Icon::Close, theme.muted).into_any_element(),
                            false,
                            cx,
                            move |this, w, cx| this.close_document_tab(id, w, cx),
                        )
                        .size(rems(1.5))
                        .min_h_0()
                        .p_0()
                        .mr_1()
                        .border_0()
                        .rounded(rems(theme.radius / 16.))
                        .bg(transparent_black())
                        .focus(move |s| s.bg(theme.tint(1.)))
                        .hover(|s| s.bg(transparent_black())),
                    );
            tabs = if dragged == Some(id) {
                // Lifted above its neighbors while it moves.
                tabs.child(deferred(surface.glass(theme, theme.popover)).with_priority(1))
            } else {
                tabs.child(surface)
            };
        }
        let drag_events = self.tab_drag.is_some().then(|| {
            let entity = cx.entity();
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let moved = entity.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase.capture() {
                            moved.update(cx, |this, cx| {
                                this.drag_tab(f32::from(event.position.x), event.pressed_button, cx)
                            });
                        }
                    });
                    let released = entity.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        // Leave the drag state available to click handlers,
                        // then finish after they have handled this release.
                        if phase.bubble() {
                            released.update(cx, |this, cx| this.drop_tab(cx));
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        });
        div()
            .h(rems(2.5))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(theme.border)
            .children(drag_events)
            .child(home)
            .child(tabs)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .child(
                        self.icon_button(
                            "new-tab",
                            "Open or create a document · Ctrl+T",
                            Icon::Plus,
                            false,
                            cx,
                            |this, w, cx| this.modal(Modal::OpenDocument, w, cx),
                        )
                        .size(rems(1.75))
                        .p_0()
                        .min_h_0()
                        .border_0()
                        .rounded(rems(theme.radius / 16.))
                        .bg(transparent_black())
                        .focus(move |s| s.bg(theme.tint(1.)))
                        .hover(move |s| s.bg(theme.tint(1.))),
                    ),
            )
            .child(super::titlebar::drag_region())
            .child(self.update_notice(cx))
            .child(
                self.window_controls(window, cx)
                    .flex_shrink_0()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation()),
            )
    }
    fn page_navigation(&self, cx: &mut Context<Self>) -> Div {
        self.page_controls_group()
            .child(self.page_control_button(
                "toggle-pages",
                "Page thumbnails · Ctrl+Shift+P",
                Icon::Sidebar,
                self.pages_open,
                cx,
                |this, _, _| this.pages_open = !this.pages_open,
            ))
            .child(self.page_control_button(
                "search",
                "Search · Ctrl+F",
                Icon::Search,
                false,
                cx,
                |this, w, cx| this.open_find(w, cx),
            ))
    }
    fn page_actions(&self, cx: &mut Context<Self>) -> Div {
        let favorite = self.controller.session().document.metadata.favorite;
        let favorite_hover = Theme::new(&self.controller.settings)
            .tint(self.motion.borrow_mut().hover_value("favorite-note"));
        self.page_controls_group()
            .child(
                self.page_control_button(
                    "favorite-note",
                    if favorite {
                        "Remove document from favorites"
                    } else {
                        "Add document to favorites"
                    },
                    if favorite {
                        Icon::StarFilled
                    } else {
                        Icon::Star
                    },
                    favorite,
                    cx,
                    |this, _, _| {
                        this.controller
                            .manage_note(this.controller.active, folio_app::NoteAction::Favorite)
                    },
                )
                // A document state, shown by the filled star alone.
                .bg(favorite_hover),
            )
            .child(self.page_control_button(
                "header-add-page",
                "Add page",
                Icon::NewPage,
                false,
                cx,
                |this, _, _| this.controller.add_page(),
            ))
            .child(self.page_control_button(
                "more",
                "Document actions",
                Icon::More,
                self.more_open,
                cx,
                |this, _, _| {
                    this.more_open = !this.more_open;
                    this.more_section = MoreSection::Document;
                    this.more_anchor = "more".into();
                    this.export_open = false;
                    this.pen_settings = false;
                },
            ))
            .child(self.editor_separator().h(rems(1.25)).mx(rems(0.375)))
            .child(self.page_control_button(
                "editor-settings",
                "Settings",
                Icon::Settings,
                false,
                cx,
                |this, w, _| this.open_settings(w),
            ))
    }
    pub(super) fn eraser_controls(&self, prefix: &'static str, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().flex_wrap().items_center().gap_1();
        for (label, mode) in [
            ("Whole stroke", folio_app::EraserMode::Stroke),
            ("Ink segments", folio_app::EraserMode::Segment),
            ("Whole object", folio_app::EraserMode::Object),
        ] {
            row = row.child(
                self.button(
                    format!("{prefix}-eraser-{mode:?}"),
                    label,
                    self.controller.settings.effective_eraser_mode() == mode,
                    cx,
                    move |this, _, _| {
                        this.controller.finish();
                        this.controller.settings.segment_eraser = false;
                        this.controller.settings.eraser_mode = mode;
                        this.controller.store_settings();
                    },
                )
                .text_xs()
                .px_2(),
            );
        }
        row = row.child(div().text_xs().child("Size"));
        for radius in [5., 10., 20.] {
            row = row.child(
                self.button(
                    format!("{prefix}-eraser-size-{radius}"),
                    format!("{} px", radius * 2.),
                    (self.controller.settings.eraser_radius - radius).abs() < 0.1,
                    cx,
                    move |this, _, _| {
                        this.controller.finish();
                        this.controller.settings.eraser_radius = radius;
                        this.controller.store_settings();
                    },
                )
                .text_xs()
                .px_2(),
            );
        }
        row
    }
    fn editor_setting_button(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        content: AnyElement,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let label = label.into();
        let hint = label.clone();
        self.control(id, label, content, active, cx, action)
            .h(rems(1.875))
            .min_h_0()
            .flex_shrink_0()
            .px_2()
            .py_0()
            .rounded(rems(theme.radius / 16.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.tint(if active { 1. } else { 0. }))
            .hover(move |s| s.bg(theme.tint(1.)))
            .when(self.toolbar_hints_enabled(), |button| {
                button.tooltip(move |_, cx| cx.new(|_| Hint(hint.clone(), theme)).into())
            })
    }
    fn editing_tools(&mut self, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        if matches!(self.controller.tool, Tool::Lasso | Tool::Rectangle) {
            self.selection_tool = self.controller.tool;
        }
        let pen_icon = match self.controller.style.tool {
            InkTool::Pencil => Icon::Pencil,
            InkTool::Marker => Icon::Marker,
            InkTool::Highlighter => Icon::Highlighter,
            _ => Icon::Pen,
        };
        let selected_pen = self.controller.tool == Tool::Pen;
        let selected_selection = matches!(self.controller.tool, Tool::Lasso | Tool::Rectangle);
        let selection_icon = if self.selection_tool == Tool::Rectangle {
            Icon::Rectangle
        } else {
            Icon::Lasso
        };
        let mut tools = div().flex().flex_shrink_0().items_center().gap(rems(0.5));
        let group = |id: &'static str, _current: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .rounded(rems(theme.radius / 16.))
                .gap(rems(0.0625))
        };
        tools = tools.child(
            group("pen-tool-group", selected_pen)
                .child(self.tool_button(
                    "pen",
                    "Pen · P",
                    pen_icon,
                    selected_pen,
                    selected_pen,
                    cx,
                    |this, _, _| this.select_tool(Tool::Pen),
                ))
                .child(
                    self.tool_button(
                        "pen-options",
                        "Pen settings and presets",
                        Icon::Down,
                        selected_pen,
                        self.pen_settings && selected_pen,
                        cx,
                        |this, _, _| {
                            let open = !this.pen_settings || this.controller.tool != Tool::Pen;
                            this.select_tool(Tool::Pen);
                            this.pen_settings = open;
                        },
                    )
                    .w(rems(1.375)),
                ),
        );
        tools = tools.child(self.tool_button(
            "eraser",
            "Eraser · E",
            Icon::Eraser,
            self.controller.tool == Tool::Eraser,
            self.controller.tool == Tool::Eraser,
            cx,
            |this, _, _| {
                let options = this.controller.tool == Tool::Eraser && !this.pen_settings;
                this.select_tool(Tool::Eraser);
                this.pen_settings = options;
            },
        ));
        tools = tools.child(
            group("selection-tool-group", selected_selection)
                .child(self.tool_button(
                    "select-tool",
                    "Select · L",
                    selection_icon,
                    selected_selection,
                    selected_selection,
                    cx,
                    |this, _, _| {
                        this.dismiss_popovers();
                        this.region_selection = None;
                        this.controller.set_tool(this.selection_tool);
                    },
                ))
                .child(
                    self.tool_button(
                        "select-options",
                        "Selection tools",
                        Icon::Down,
                        selected_selection,
                        self.tool_menu
                            .is_some_and(|(menu, _)| menu == ToolMenu::Selection),
                        cx,
                        |this, window, cx| this.toggle_tool_menu(ToolMenu::Selection, window, cx),
                    )
                    .w(rems(1.375)),
                ),
        );
        for (id, label, kind, tool) in [
            ("text", "Text · T", Icon::Text, Tool::Text),
            ("hand", "Pan · H", Icon::Hand, Tool::Hand),
        ] {
            tools = tools.child(self.tool_button(
                id,
                label,
                kind,
                self.controller.tool == tool,
                self.controller.tool == tool,
                cx,
                move |this, _, _| this.select_tool(tool),
            ));
        }
        let insert = div()
            .flex()
            .items_center()
            .gap(rems(0.375))
            .child(icon(Icon::Image, theme.ink).size(rems(1.25)))
            .when(!compact, |row| row.child("Insert"))
            .child(icon(Icon::Down, theme.muted).size(rems(0.875)));
        tools = tools.child(
            self.editor_setting_button(
                "insert-options",
                "Insert",
                insert.into_any_element(),
                self.controller.tool == Tool::Shape
                    || self
                        .tool_menu
                        .is_some_and(|(menu, _)| menu == ToolMenu::Insert),
                cx,
                |this, window, cx| this.toggle_tool_menu(ToolMenu::Insert, window, cx),
            ),
        );
        let history = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(rems(0.125))
            .child(self.editor_button(
                "undo",
                "Undo · Ctrl+Z",
                Icon::Undo,
                false,
                cx,
                |this, w, cx| this.step_history(false, w, cx),
            ))
            .child(self.editor_button(
                "redo",
                "Redo · Ctrl+Shift+Z or Ctrl+Y",
                Icon::Redo,
                false,
                cx,
                |this, w, cx| this.step_history(true, w, cx),
            ));
        let primary = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(rems(0.25))
            .child(history)
            .child(self.editor_separator())
            .child(tools);
        let has_options = matches!(self.controller.tool, Tool::Pen | Tool::Shape | Tool::Text);
        let mut options = div().flex().flex_shrink_0().items_center().gap(rems(0.375));
        if matches!(self.controller.tool, Tool::Pen | Tool::Shape) {
            let width = self.controller.style.width;
            let width_content = div()
                .flex()
                .items_center()
                .gap(rems(0.375))
                .child(format!("{width:.1} px"))
                .child(icon(Icon::Down, theme.muted).size(rems(0.875)));
            options = options.child(
                self.editor_setting_button(
                    "width-options",
                    format!("Stroke width {width:.1} px"),
                    width_content.into_any_element(),
                    self.tool_menu
                        .is_some_and(|(menu, _)| menu == ToolMenu::Width),
                    cx,
                    |this, window, cx| this.toggle_tool_menu(ToolMenu::Width, window, cx),
                ),
            );
        }
        if matches!(self.controller.tool, Tool::Pen | Tool::Shape | Tool::Text) {
            let color = self.controller.style.color.rgb();
            let color_content = div()
                .flex()
                .items_center()
                .gap(rems(0.375))
                .child(
                    // The ink as the page shows it, ringed so dark ink stays
                    // visible on a dark toolbar.
                    div()
                        .size(rems(1.125))
                        .rounded_full()
                        .border(px(1.5))
                        .border_color(rgba((theme.ink << 8) | 0x80))
                        .bg(rgb(theme.canvas.ink(color))),
                )
                .child(icon(Icon::Down, theme.muted).size(rems(0.875)));
            options = options.child(
                self.editor_setting_button(
                    "custom-color",
                    format!("Ink color #{color:06X}"),
                    color_content.into_any_element(),
                    self.tool_menu
                        .is_some_and(|(menu, _)| menu == ToolMenu::Color),
                    cx,
                    |this, window, cx| this.toggle_tool_menu(ToolMenu::Color, window, cx),
                ),
            );
        }
        div()
            .id("editing-tools")
            .flex_1()
            .min_w_0()
            .h(rems(2.625))
            .px(rems(0.3125))
            .flex()
            .items_center()
            .gap(rems(0.5))
            .overflow_x_scroll()
            .child(primary)
            .when(has_options, |row| row.child(self.editor_separator()))
            .child(options)
            .child(div().flex_1())
    }
    pub(super) fn editor_header(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let navigation = self.page_navigation(cx);
        let editing = if self.controller.read_only() {
            div().id("editing-tools").flex_1()
        } else {
            let compact =
                f32::from(window.viewport_size().width) / self.controller.settings.ui_scale < 1100.;
            self.editing_tools(compact, cx)
        };
        div()
            .id("writing-toolbar")
            .h(rems(3.))
            .flex_shrink_0()
            .px(rems(0.625))
            .flex()
            .items_center()
            .gap(rems(0.625))
            .border_b_1()
            .border_color(theme.border)
            .child(navigation)
            .child(editing)
            .child(self.page_actions(cx))
    }
    pub(super) fn pages_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let scale = self.controller.settings.ui_scale;
        let session = self.controller.session();
        let total = session.document.pages.len();
        let current = session.page;
        // Thumbnails fill the sidebar's width; rows share the height of the
        // tallest page so every page lines up.
        let width = 140. * scale;
        let max_height = 200. * scale;
        let tallest = session
            .document
            .pages
            .iter()
            .map(|page| {
                let aspect = page.properties.width / page.properties.height.max(1.);
                (width / aspect.max(0.01)).min(max_height)
            })
            .fold(0., f32::max);
        let row_height = tallest + 58. * scale;
        // Keep the current page in view as it changes, from the canvas or
        // the keyboard, without fighting the user's own scrolling.
        let follow = (self.controller.active, current);
        if self.pages_followed != Some(follow) {
            self.pages_followed = Some(follow);
            self.pages_scroll
                .scroll_to_item(current, ScrollStrategy::Top);
        }
        let rows = uniform_list(
            "page-thumbnails",
            total,
            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                let theme = Theme::new(&this.controller.settings);
                let mut rows = Vec::new();
                for index in range {
                    let background = this.controller.session().document.pages[index]
                        .properties
                        .pdf
                        .clone();
                    if let Some(background) = background {
                        this.controller.request_pdf_background(background);
                    }
                    let thumbnail = this.thumbnail(
                        ThumbnailSource::Current(index),
                        width,
                        max_height,
                        window,
                        cx,
                    );
                    let page = &this.controller.session().document.pages[index];
                    let page_id = page.id;
                    let bookmark = page.properties.bookmark.clone();
                    let label = bookmark.as_ref().map_or_else(
                        || format!("Go to page {}", index + 1),
                        |name| format!("Go to page {} · {name}", index + 1),
                    );
                    let active = index == this.controller.session().page;
                    let key = format!("page-{page_id}");
                    let hover = this.motion.borrow_mut().hover_value(&key);
                    let focused = this.accessibility.is_focused(&key, window);
                    // The ring frames the page itself: accent when current,
                    // the focus color for the keyboard, a soft tint while
                    // hovered.
                    let ring = if active {
                        rgb(theme.selection())
                    } else if focused {
                        rgb(theme.ring)
                    } else {
                        theme.tint(hover * 2.5)
                    };
                    let page_view = div()
                        .p(px(3.))
                        .rounded(px(theme.corner_sm() + 5.))
                        .border_2()
                        .border_color(ring)
                        .child(
                            thumbnail
                                .rounded(px(theme.corner_sm()))
                                .border_1()
                                .border_color(theme.cover_edge())
                                .shadow_sm(),
                        );
                    let number = div()
                        .min_w(rems(1.5))
                        .px_2()
                        .flex()
                        .justify_center()
                        .rounded_full()
                        .text_xs()
                        .font_weight(if active {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .when(active, |n| n.text_color(rgb(theme.ink)))
                        .when(!active, |n| n.text_color(rgb(theme.muted)))
                        .child(format!("{}", index + 1));
                    let caption = div()
                        .max_w(px(width))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .child(number)
                        .children(bookmark.map(|name| {
                            div()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .text_color(rgb(if active { theme.ink } else { theme.muted }))
                                .child(icon(Icon::Bookmark, theme.muted).size(rems(0.75)))
                                .child(div().min_w_0().truncate().child(name))
                        }));
                    let content = div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1p5()
                        .child(page_view)
                        .child(caption);
                    rows.push(
                        div()
                            .h(px(row_height))
                            .px_2()
                            .flex()
                            .justify_center()
                            .child(
                                this.control(
                                    key,
                                    label,
                                    content.into_any_element(),
                                    active,
                                    cx,
                                    move |this, _, _| this.controller.change_page(index),
                                )
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                        this.controller.change_page(index);
                                        this.dismiss_popovers();
                                        this.more_section = MoreSection::Page;
                                        this.more_anchor = format!("page-{page_id}");
                                        this.more_open = true;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .on_drag(PageDrag(page_id), |drag, _, _, cx| {
                                    cx.new(|_| drag.clone())
                                })
                                .drag_over::<PageDrag>(move |s, _, _, _| {
                                    s.border_color(rgb(theme.accent)).bg(theme.tint(1.))
                                })
                                .on_drop(cx.listener(move |this, drag: &PageDrag, _, _| {
                                    this.controller.reorder_page(drag.0, page_id)
                                }))
                                .items_start()
                                .px_1()
                                .py_2()
                                // The ring and number show state; no card.
                                .bg(transparent_black())
                                .hover(|s| s.bg(transparent_black()))
                                .active(|s| s.bg(transparent_black()))
                                .focus(|s| s.border_color(transparent_black())),
                            ),
                    );
                }
                // Virtual rows register during layout, after the root's render.
                this.resume_poll(cx);
                this.accessibility.publish(this, window, cx);
                rows
            }),
        )
        .track_scroll(self.pages_scroll.clone())
        .flex_1()
        .min_h_0();
        div()
            .w(rems(11.5))
            .h_full()
            .flex_shrink_0()
            .glass_fill(theme, theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(48.))
                    .pl_4()
                    .pr_2()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Pages"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.muted))
                                    .child(format!("{} of {total}", current + 1)),
                            ),
                    )
                    .child(
                        self.icon_button(
                            "page-options-panel",
                            "More page options…",
                            Icon::More,
                            false,
                            cx,
                            |this, _, _| {
                                this.dismiss_popovers();
                                this.more_section = MoreSection::Page;
                                this.more_anchor = "page-options-panel".into();
                                this.more_open = true;
                            },
                        )
                        .size(rems(1.75)),
                    )
                    .child(
                        self.icon_button(
                            "close-pages",
                            "Hide pages",
                            Icon::Close,
                            false,
                            cx,
                            |this, _, _| this.pages_open = false,
                        )
                        .size(rems(1.75)),
                    ),
            )
            .child(rows)
            .child(
                div()
                    .p_2()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        self.control(
                            "add-page",
                            "Add page",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(Icon::NewPage, theme.ink).size(rems(1.)))
                                .child("Add page")
                                .into_any_element(),
                            false,
                            cx,
                            |this, _, _| this.controller.add_page(),
                        )
                        .w_full()
                        .text_xs()
                        .border_color(theme.border),
                    ),
            )
    }
    pub(super) fn footer(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let page = self.controller.session().page;
        let total = self.controller.session().document.pages.len();
        let zoom = self.controller.session().viewport.zoom;
        div()
            .h(rems(2.125))
            .flex_shrink_0()
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .glass_fill_from_edge(theme, theme.surface, false)
            .border_t_1()
            .border_color(theme.border)
            .child(
                self.icon_button(
                    "previous-page",
                    "Previous page",
                    Icon::Back,
                    false,
                    cx,
                    |this, _, _| {
                        let p = this.controller.session().page;
                        this.controller.change_page(p.saturating_sub(1));
                    },
                )
                .size(rems(1.75)),
            )
            .child(
                self.button(
                    "page-count",
                    format!("{} / {}", page + 1, total),
                    self.pages_open,
                    cx,
                    |this, _, _| this.pages_open = !this.pages_open,
                )
                .text_sm()
                .py_1(),
            )
            .child(
                self.icon_button(
                    "next-page",
                    "Next page",
                    Icon::Forward,
                    false,
                    cx,
                    |this, _, _| {
                        let p = this.controller.session().page;
                        this.controller.change_page(p + 1);
                    },
                )
                .size(rems(1.75)),
            )
            .child(div().flex_1())
            .child(match self.controller.save_state() {
                folio_app::SaveState::Saved
                    if self.controller.busy == 0 && !self.controller.recognition_pending =>
                {
                    icon(Icon::Check, theme.muted).into_any_element()
                }
                folio_app::SaveState::Saved | folio_app::SaveState::Saving => {
                    icon(Icon::Clock, theme.muted).into_any_element()
                }
                // A filled dot marks unsaved or failed saves instead of a check.
                state => div()
                    .size(px(8.))
                    .mx(px(4.))
                    .rounded_full()
                    .bg(rgb(if state == folio_app::SaveState::Failed {
                        theme.destructive
                    } else {
                        theme.muted
                    }))
                    .into_any_element(),
            })
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.secondary()))
                    .child(self.controller.activity_status()),
            )
            .children(
                self.library_undo_button(cx)
                    .map(|undo| undo.text_xs().text_color(rgb(theme.muted))),
            )
            .child(div().flex_1())
            .child(
                self.button("zoom-out", "−", false, cx, |this, _, _| {
                    this.zoom(1. / 1.2)
                })
                .py_1()
                .tooltip(move |_, cx| cx.new(|_| Hint("Zoom out · Ctrl+−".into(), theme)).into()),
            )
            .child(
                self.button(
                    "fit-page",
                    format!("{:.0}%", zoom * 100.),
                    false,
                    cx,
                    |this, _, _| this.fit(),
                )
                .text_sm()
                .py_1()
                .tooltip(move |_, cx| cx.new(|_| Hint("Fit page · Ctrl+0".into(), theme)).into()),
            )
            .child(
                self.button("zoom-in", "+", false, cx, |this, _, _| this.zoom(1.2))
                    .py_1()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint("Zoom in · Ctrl++".into(), theme)).into()
                    }),
            )
            .child(
                self.icon_button(
                    "help",
                    "Keyboard shortcuts",
                    Icon::Help,
                    false,
                    cx,
                    |this, w, _| this.open_help(w),
                )
                .size(rems(1.75)),
            )
    }
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn edited_labels_abbreviate_every_age() {
        let now = folio_document::now_ms();
        let ago = |minutes: u64| edited_label(now - minutes * 60_000);
        assert_eq!(ago(0), "Edited just now");
        assert_eq!(ago(5), "Edited 5min ago");
        assert_eq!(ago(120), "Edited 2h ago");
        assert_eq!(ago(1500), "Edited 1d ago");
        assert_eq!(ago(7 * 1440), "Edited 1w ago");
        assert_eq!(ago(65 * 1440), "Edited 2m ago");
        assert_eq!(ago(400 * 1440), "Edited 1y ago");
    }
    fn key(page: &Page) -> ThumbnailKey {
        ThumbnailKey {
            revision: page.revision,
            properties: page.properties.clone(),
            theme: Theme::new(&folio_app::Settings::default()).canvas,
            pdf_ready: true,
            edge: 384,
        }
    }
    #[::core::prelude::v1::test]
    fn thumbnail_cache_keys_invalidate_content_properties_theme_and_pdf_readiness() {
        let page = Page::new();
        let original = key(&page);
        let mut cache = Thumbnails::default();
        let mut atlas = std::collections::HashSet::new();
        cache.start(page.id, original.clone(), 1., 1, |_| {
            panic!("A new cache entry cannot retire an image")
        });
        cache.pending.remove(&page.id);
        let image = graph::image(1, 1, vec![0; 4]);
        let old_image = Arc::downgrade(&image);
        atlas.insert(image.id);
        cache.entries.get_mut(&page.id).unwrap().image = Some(image);
        assert!(cache.touch(page.id, &original).1);
        for change in [
            |k: &mut ThumbnailKey| k.revision += 1,
            |k: &mut ThumbnailKey| k.properties.width += 10.,
            |k: &mut ThumbnailKey| k.theme.paper ^= 0xffffff,
            |k: &mut ThumbnailKey| k.pdf_ready = false,
        ] {
            let mut changed = original.clone();
            change(&mut changed);
            assert!(!cache.touch(page.id, &changed).1);
        }
        assert!(!cache.touch(Id::new_v4(), &original).1);
        let mut changed = original.clone();
        changed.revision += 1;
        let (generation, _) = cache.touch(page.id, &changed);
        cache.start(page.id, changed.clone(), 1., generation, |image| {
            assert!(atlas.remove(&image.id), "Image retired more than once");
        });
        assert!(old_image.upgrade().is_none());
        assert!(cache.touch(page.id, &changed).1);
        assert!(atlas.is_empty());

        // Scrolling through many covers retires old atlas allocations while
        // keeping an in-flight replacement pinned until its result arrives.
        for _ in 0..80 {
            let next = Id::new_v4();
            let (touched, _) = cache.touch(next, &original);
            cache.start(next, original.clone(), 1., touched, |image| {
                assert!(atlas.remove(&image.id), "Image retired more than once");
            });
            let image = graph::image(1, 1, vec![0; 4]);
            atlas.insert(image.id);
            cache.entries.get_mut(&next).unwrap().image = Some(image);
            cache.pending.remove(&next);
        }
        assert!(cache.entries.contains_key(&page.id));
        assert_eq!(cache.pending.get(&page.id), Some(&generation));
        assert_eq!(
            atlas,
            cache
                .entries
                .values()
                .filter_map(|entry| entry.image.as_ref().map(|image| image.id))
                .collect(),
            "Evicted covers must not leave sprite-atlas images behind"
        );
        assert!(atlas.len() < 80, "Scrolling must evict old cover images");
    }
}
