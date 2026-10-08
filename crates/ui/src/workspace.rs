//! Library, document chrome and page navigation. All mutations go through Controller.
use super::*;
use folio_document::{Object, Page};
use std::{collections::HashMap, sync::Arc};

fn edited_label(updated: u64) -> String {
    let minutes = folio_document::now_ms().saturating_sub(updated) / 60_000;
    match minutes {
        0 => "Edited just now".into(),
        1..=59 => format!("Edited {minutes} min ago"),
        60..=1439 => format!("Edited {} h ago", minutes / 60),
        _ => format!("Edited {} days ago", minutes / 1440),
    }
}

struct Hint(SharedString, Theme);
impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .shadow_md()
            .text_xs()
            .bg(rgb(self.1.ink))
            .text_color(rgb(self.1.surface))
            .child(self.0.clone())
    }
}

#[derive(Default)]
pub(super) struct Thumbnails {
    pages: HashMap<
        Id,
        (
            u64,
            folio_document::PageProperties,
            super::theme::CanvasTheme,
            Arc<Preview>,
        ),
    >,
}
struct Preview {
    paths: Vec<(Path<Pixels>, Rgba)>,
    labels: Vec<(String, folio_document::Rect, f32, u32)>,
    images: Vec<(std::path::PathBuf, folio_document::Rect)>,
    properties: folio_document::PageProperties,
}
impl Thumbnails {
    fn element(&mut self, page: &Page, controller: &Controller, width: f32) -> Div {
        let theme = Theme::new(&controller.settings);
        let canvas_theme = theme.canvas_for_page(&page.properties);
        let valid =
            self.pages
                .get(&page.id)
                .is_some_and(|(revision, properties, appearance, _)| {
                    *revision == page.revision
                        && *properties == page.properties
                        && *appearance == canvas_theme
                });
        if !valid {
            let mut preview = Preview {
                paths: vec![],
                labels: vec![],
                images: vec![],
                properties: page.properties.clone(),
            };
            let hidden = page.hidden_sources();
            for object in page
                .ordered_objects()
                .filter(|object| !hidden.contains(&object.id()))
            {
                let mut b;
                let color = match object.as_ref() {
                    Object::Stroke(s) => {
                        let points = s.display_path();
                        let Some(first) = points.first() else {
                            continue;
                        };
                        b = PathBuilder::stroke(px(s.style.width.max(0.5)));
                        b.move_to(point(px(first.position.x), px(first.position.y)));
                        for p in points.iter().skip(1) {
                            b.line_to(point(px(p.position.x), px(p.position.y)));
                        }
                        s.style.color
                    }
                    Object::Shape(s) => {
                        if let Some(path) =
                            super::painting::shape_path(&s.vertices, s.style.width.max(0.5))
                        {
                            let t = s.transform;
                            let mut color = rgb(canvas_theme.ink(s.style.color.rgb()));
                            color.a = s.style.opacity;
                            preview
                                .paths
                                .push((path.transformed([t.a, t.b, t.c, t.d, t.tx, t.ty]), color));
                        }
                        continue;
                    }
                    Object::Text(t) => {
                        preview.labels.push((
                            t.text.clone(),
                            object.bounds(),
                            t.font_size,
                            t.color.rgb(),
                        ));
                        continue;
                    }
                    Object::Equation(e) => {
                        preview
                            .labels
                            .push((e.latex.clone(), object.bounds(), 20., 0x273448));
                        continue;
                    }
                    Object::Image(i) => {
                        if let Some(path) = controller.asset_path(&i.asset) {
                            preview.images.push((path, object.bounds()));
                        }
                        continue;
                    }
                };
                if let Ok(path) = b.build() {
                    let t = object.transform();
                    let mut c = rgb(canvas_theme.ink(color.rgb()));
                    c.a = match object.as_ref() {
                        Object::Stroke(s) => s.style.opacity,
                        Object::Shape(s) => s.style.opacity,
                        _ => 1.,
                    };
                    preview
                        .paths
                        .push((path.transformed([t.a, t.b, t.c, t.d, t.tx, t.ty]), c));
                }
            }
            // Bounded cache, independent of the full-resolution canvas cache.
            if self.pages.len() >= 128 {
                self.pages.clear();
            }
            self.pages.insert(
                page.id,
                (
                    page.revision,
                    page.properties.clone(),
                    canvas_theme,
                    Arc::new(preview),
                ),
            );
        }
        let preview = self.pages[&page.id].3.clone();
        let scale = (width / preview.properties.width.max(1.))
            .min(200. / preview.properties.height.max(1.));
        let width = (preview.properties.width * scale).max(1.);
        let height = (preview.properties.height * scale).max(1.);
        let background = preview
            .properties
            .pdf
            .as_ref()
            .and_then(|pdf| pdf.preview_asset.as_ref())
            .and_then(|asset| controller.asset_path(asset));
        let drawing = preview.clone();
        let mut paper = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .overflow_hidden()
            .bg(rgb(if preview.properties.pdf.is_some() {
                0xffffff
            } else {
                canvas_theme.paper
            }))
            .rounded_sm()
            .shadow_sm()
            .when_some(background, |s, path| {
                s.child(
                    img(path)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            let x = f32::from(bounds.origin.x);
                            let y = f32::from(bounds.origin.y);
                            if drawing.properties.pdf.is_none() {
                                let mut b = PathBuilder::stroke(px(0.55));
                                match drawing.properties.paper {
                                    Paper::Ruled | Paper::Grid => {
                                        for i in 1..=(height / (32. * scale)) as i32 {
                                            let offset = i as f32 * 32. * scale;
                                            b.move_to(point(px(x), px(y + offset)));
                                            b.line_to(point(px(x + width), px(y + offset)));
                                        }
                                        if drawing.properties.paper == Paper::Grid {
                                            for i in 1..=(width / (32. * scale)) as i32 {
                                                let offset = i as f32 * 32. * scale;
                                                b.move_to(point(px(x + offset), px(y)));
                                                b.line_to(point(px(x + offset), px(y + height)));
                                            }
                                        }
                                    }
                                    Paper::Dots => {
                                        for row in 1..=(height / (32. * scale)) as i32 {
                                            for col in 1..=(width / (32. * scale)) as i32 {
                                                let (dx, dy) = (
                                                    x + col as f32 * 32. * scale,
                                                    y + row as f32 * 32. * scale,
                                                );
                                                b.move_to(point(px(dx), px(dy)));
                                                b.line_to(point(px(dx + 0.65), px(dy)));
                                            }
                                        }
                                    }
                                    Paper::Blank => {}
                                }
                                if let Ok(path) = b.build() {
                                    window.paint_path(path, rgb(canvas_theme.grid));
                                }
                            }
                            for (path, color) in &drawing.paths {
                                window.paint_path(
                                    path.clone().transformed([scale, 0., 0., scale, x, y]),
                                    *color,
                                );
                            }
                        });
                    },
                )
                .size_full(),
            );
        for (path, rect) in &preview.images {
            paper = paper.child(
                img(path.clone())
                    .absolute()
                    .left(px(rect.min.x * scale))
                    .top(px(rect.min.y * scale))
                    .w(px(rect.width() * scale))
                    .h(px(rect.height() * scale))
                    .object_fit(ObjectFit::Contain),
            );
        }
        for (text, rect, font, color) in &preview.labels {
            paper = paper.child(
                div()
                    .absolute()
                    .overflow_hidden()
                    .left(px(rect.min.x * scale))
                    .top(px(rect.min.y * scale))
                    .w(px(rect.width() * scale))
                    .h(px(rect.height() * scale))
                    .text_size(px((font * scale).max(3.)))
                    .text_color(rgb(canvas_theme.ink(*color)))
                    .child(text.clone()),
            );
        }
        paper
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
    /// Open a document without changing its model or viewport.
    pub fn show_editor(&mut self) {
        self.document_menu = None;
        self.library_open = false;
        let id = self.controller.active;
        if !self.open_tabs.contains(&id) {
            self.open_tabs.push(id);
        }
    }
    pub(super) fn open_note(&mut self, id: Id) {
        self.controller.switch_note(id);
        if !self.open_tabs.contains(&id) {
            self.open_tabs.push(id);
        }
        self.document_menu = None;
        self.library_open = false;
        self.more_open = false;
        self.export_open = false;
        self.pen_settings = false;
    }
    pub(super) fn show_library(&mut self) {
        self.controller.finish();
        self.document_menu = None;
        self.library_open = true;
        self.canvas_bounds = None;
        self.more_open = false;
        self.export_open = false;
        self.pen_settings = false;
    }
    pub(super) fn new_notebook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal(Modal::NewDocument, window, cx);
    }
    fn icon_button(
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
        .size(px(38.))
        .p_0()
        .rounded(px(theme.radius))
        .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone(), theme)).into())
    }
    fn chrome_button(
        &self,
        id: &'static str,
        label: &'static str,
        kind: Icon,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        self.control(
            id,
            label,
            icon(kind, theme.ink).into_any_element(),
            active,
            cx,
            action,
        )
        .size(px(36.))
        .p_0()
        .bg(rgb(super::theme::mix(
            if active {
                theme.chrome_active
            } else {
                theme.chrome
            },
            theme.chrome_active,
            self.motion.borrow().hover_value(id),
        )))
        .tooltip(move |_, cx| cx.new(|_| Hint(label.into(), theme)).into())
    }
    fn separator(&self) -> Div {
        div()
            .w(px(1.))
            .h(px(24.))
            .mx_2()
            .bg(Theme::new(&self.controller.settings).border)
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
            let active = self.controller.filter == filter;
            let content = div()
                .flex()
                .items_center()
                .gap_3()
                .child(icon(
                    kind,
                    if active {
                        theme.sidebar_accent
                    } else {
                        theme.muted
                    },
                ))
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
                .bg(rgb(if active {
                    theme.selected
                } else {
                    theme.sidebar
                })),
            );
        }
        let mut folders = div()
            .id("notebook-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .px_3();
        for n in self.controller.notebooks.clone() {
            let id = n.id;
            let active = self.controller.filter == NoteFilter::Notebook(id);
            let content = div()
                .flex()
                .min_w_0()
                .items_center()
                .gap_3()
                .when(n.parent.is_some(), |s| s.pl_3())
                .child(icon(Icon::Folder, theme.muted))
                .child(div().truncate().child(n.name.clone()));
            folders = folders.child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        self.control(
                            id.to_string(),
                            n.name,
                            content.into_any_element(),
                            active,
                            cx,
                            move |this, _, _| this.controller.filter = NoteFilter::Notebook(id),
                        )
                        .flex_1()
                        .min_w_0()
                        .justify_start()
                        .bg(rgb(if active {
                            theme.selected
                        } else {
                            theme.sidebar
                        })),
                    )
                    .child(
                        self.icon_button(
                            format!("rename-notebook-{id}"),
                            "Rename folder",
                            Icon::Pen,
                            false,
                            cx,
                            move |this, w, cx| this.modal(Modal::RenameNotebook(id), w, cx),
                        )
                        .size(px(28.))
                        .bg(rgb(theme.sidebar)),
                    )
                    .child(
                        self.icon_button(
                            format!("child-notebook-{id}"),
                            "New subfolder",
                            Icon::Plus,
                            false,
                            cx,
                            move |this, w, cx| this.modal(Modal::Notebook(Some(id)), w, cx),
                        )
                        .size(px(28.))
                        .bg(rgb(theme.sidebar)),
                    ),
            );
        }
        if self.controller.notebooks.is_empty() {
            folders = folders.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Keep related notes together."),
            );
        }
        if let NoteFilter::Notebook(id) = self.controller.filter {
            folders = folders
                .child(
                    self.button(
                        "move-folder",
                        "Move folder…",
                        false,
                        cx,
                        move |this, w, cx| this.modal(Modal::MoveNotebook(id), w, cx),
                    )
                    .text_xs()
                    .justify_start()
                    .bg(rgb(theme.sidebar)),
                )
                .child(
                    self.button(
                        "delete-folder",
                        "Delete empty folder",
                        false,
                        cx,
                        move |this, _, _| {
                            if let Err(e) = this.controller.delete_empty_notebook(id) {
                                this.controller.error = Some(e);
                            }
                        },
                    )
                    .text_xs()
                    .justify_start()
                    .bg(rgb(theme.sidebar)),
                );
        }
        div()
            .w(px(216.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_5()
            .bg(rgb(theme.sidebar))
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
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.muted))
                            .child("Folders"),
                    )
                    .child(
                        self.icon_button(
                            "add-notebook",
                            "New folder",
                            Icon::Plus,
                            false,
                            cx,
                            |this, w, cx| {
                                let parent =
                                    if let NoteFilter::Notebook(id) = this.controller.filter {
                                        Some(id)
                                    } else {
                                        None
                                    };
                                this.modal(Modal::Notebook(parent), w, cx)
                            },
                        )
                        .size(px(30.))
                        .bg(rgb(theme.sidebar)),
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
                        .justify_start()
                        .bg(rgb(theme.sidebar)),
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
                        .bg(rgb(theme.sidebar)),
                    ),
            )
    }
    fn library_items(
        &mut self,
        notes: Vec<folio_document::NoteMetadata>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut items = div()
            .flex()
            .when(self.list_view, |s| s.flex_col())
            .when(!self.list_view, |s| s.flex_wrap())
            .gap_6();
        for n in notes {
            let id = n.id;
            let session = self.controller.sessions.get(&id);
            let details = session
                .map(|s| {
                    format!(
                        "{} page{}",
                        s.document.pages.len(),
                        if s.document.pages.len() == 1 { "" } else { "s" }
                    )
                })
                .unwrap_or_else(|| "Document".into());
            let preview = self.controller.library_preview(id);
            let details = preview
                .as_ref()
                .map(|(_, count)| format!("{count} page{}", if *count == 1 { "" } else { "s" }))
                .unwrap_or(details);
            let details = self
                .controller
                .library_imports
                .get(&id)
                .map(|import| {
                    if import.pending {
                        "Importing…".to_owned()
                    } else {
                        "Import failed · open to retry or remove".to_owned()
                    }
                })
                .unwrap_or(details);
            let cover_content = if let Some((page, _)) = &preview {
                if let Some(pdf) = &page.properties.pdf {
                    self.controller.request_pdf_background(pdf.clone());
                }
                self.thumbnails.element(page, &self.controller, 144.)
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
                .w(px(148.))
                .h(px(198.))
                .overflow_hidden()
                .rounded(px(theme.radius))
                .border_1()
                .border_color(theme.border)
                .bg(rgb(theme.sidebar))
                .flex()
                .items_center()
                .justify_center()
                .child(cover_content)
                .when(n.favorite, |s| {
                    s.child(
                        div()
                            .absolute()
                            .right_2()
                            .bottom_2()
                            .child(icon(Icon::Star, theme.ink)),
                    )
                });
            let metadata = div()
                .flex()
                .flex_col()
                .gap_1()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(n.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .truncate()
                        .child(if n.tags.is_empty() {
                            details
                        } else {
                            n.tags.join(" · ")
                        }),
                );
            let metadata = metadata.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(edited_label(n.updated_at)),
            );
            let content = if self.list_view {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(icon(Icon::Book, theme.muted))
                    .child(metadata)
                    .child(div().flex_1())
                    .when(n.favorite, |s| s.child(icon(Icon::Star, theme.accent)))
                    .child(div().w(px(32.)))
            } else {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(cover)
                    .child(metadata.w(px(148.)))
            };
            items = items.child(
                div()
                    .relative()
                    .when(self.list_view, |s| s.w_full())
                    .child(
                        self.control(
                            format!("note-{id}"),
                            format!("Open {}", n.title),
                            content.into_any_element(),
                            false,
                            cx,
                            move |this, _, _| this.open_note(id),
                        )
                        .when(!self.list_view, |s| s.w(px(184.)).p_3())
                        .when(self.list_view, |s| {
                            s.w_full()
                                .p_4()
                                .justify_start()
                                .border_b_1()
                                .border_color(theme.border)
                        })
                        .bg(rgb(theme.bg))
                        .hover(move |s| s.bg(rgb(theme.sidebar)))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                                this.open_document_menu(id, event.position, w, cx);
                                cx.stop_propagation();
                            }),
                        ),
                    )
                    .child(
                        self.icon_button(
                            format!("manage-note-{id}"),
                            format!("Manage {}", n.title),
                            Icon::More,
                            false,
                            cx,
                            move |this, w, cx| {
                                this.open_document_menu(id, w.mouse_position(), w, cx);
                                cx.stop_propagation();
                            },
                        )
                        .absolute()
                        .top(px(12.))
                        .right(px(12.))
                        .size(px(28.))
                        .bg(rgb(theme.sidebar)),
                    ),
            );
        }
        items
    }
    pub(super) fn library(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let title = match self.controller.filter {
            NoteFilter::All => "Documents".to_string(),
            NoteFilter::Favorites => "Favorites".into(),
            NoteFilter::Recent => "Recent".into(),
            NoteFilter::Trash => "Trash".into(),
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
        if self.sort_by_name {
            notes.sort_by_key(|n| n.title.to_lowercase());
        } else {
            notes.sort_by_key(|n| std::cmp::Reverse(n.updated_at));
        }
        let count = notes.len();
        let mut shelf = div()
            .id("library-shelf")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p_7();
        let columns = if self.list_view {
            1
        } else {
            ((f32::from(window.viewport_size().width) / self.controller.settings.ui_scale - 272.)
                / 208.)
                .floor()
                .max(1.) as usize
        };
        let total = notes.len();
        let list = uniform_list(
            "library-rows",
            total.div_ceil(columns),
            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                let mut rows = Vec::new();
                for row in range {
                    let start = row * columns;
                    let end = (start + columns).min(total);
                    rows.push(
                        this.library_items(notes[start..end].to_vec(), cx)
                            .h(px(if this.list_view { 92. } else { 320. }))
                            .items_start(),
                    );
                }
                this.accessibility.publish(this, window, cx);
                rows
            }),
        )
        .flex_1()
        .min_h_0();
        if count == 0 {
            let (heading, message, symbol) = match self.controller.filter {
                NoteFilter::Trash => (
                    "Trash is empty",
                    "Deleted documents appear here. You can restore them whenever you need.",
                    Icon::Trash,
                ),
                NoteFilter::Favorites => (
                    "Keep your favorites close",
                    "Use the star on a document to find it here.",
                    Icon::Star,
                ),
                NoteFilter::Recent => (
                    "Your recent documents",
                    "Open a document to add it to this collection.",
                    Icon::Book,
                ),
                NoteFilter::Notebook(_) => (
                    "A fresh start",
                    "Create a document in this folder to begin writing.",
                    Icon::Book,
                ),
                NoteFilter::All => (
                    "Space for your next idea",
                    "Create a document or import a PDF to begin.",
                    Icon::Book,
                ),
            };
            shelf = shelf.child(
                div()
                    .py_6()
                    .mb_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(icon(symbol, theme.muted))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::MEDIUM)
                            .child(heading),
                    )
                    .child(div().text_sm().text_color(rgb(theme.muted)).child(message)),
            );
        }
        shelf = shelf.child(list);
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .child(
                div()
                    .h(px(72.))
                    .px_8()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(px(25.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(self.icon_button(
                                "search",
                                "Search all notes · Ctrl+F",
                                Icon::Search,
                                false,
                                cx,
                                |this, w, cx| this.modal(Modal::Search, w, cx),
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
                                    |this, w, cx| this.new_notebook(w, cx),
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
                    .child(
                        self.button(
                            "sort-notes",
                            if self.sort_by_name {
                                "Name  ↓"
                            } else {
                                "Last edited  ↓"
                            },
                            false,
                            cx,
                            |this, _, _| this.sort_by_name = !this.sort_by_name,
                        )
                        .text_xs()
                        .text_color(rgb(theme.muted)),
                    )
                    .child(div().text_xs().text_color(rgb(theme.muted)).child(format!(
                        "{count} document{}",
                        if count == 1 { "" } else { "s" }
                    )))
                    .child(div().flex_1())
                    .child(self.icon_button(
                        "grid-view",
                        "Grid view",
                        Icon::Grid,
                        !self.list_view,
                        cx,
                        |this, _, _| this.list_view = false,
                    ))
                    .child(self.icon_button(
                        "list-view",
                        "List view",
                        Icon::List,
                        self.list_view,
                        cx,
                        |this, _, _| this.list_view = true,
                    )),
            )
            .child(shelf)
            .child(
                div()
                    .h(px(36.))
                    .px_8()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Stored on this device"),
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
        if self.library_open {
            self.tab_target = None;
            self.tab_scroll.set_offset(Point::default());
        } else if let Some(index) = self.open_tabs.iter().position(|id| *id == active) {
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
            .gap_1();
        if self.library_open {
            tabs = tabs.child(
                div()
                    .px_3()
                    .h(px(34.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon(Icon::Book, theme.muted))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Folio"),
                    ),
            );
        }
        for id in self
            .open_tabs
            .clone()
            .into_iter()
            .filter(|_| !self.library_open)
        {
            let Some(n) = self.controller.notes.iter().find(|n| n.id == id) else {
                continue;
            };
            let selected = id == active;
            let drag = super::navigation::TabDrag {
                id,
                title: n.title.clone(),
                theme,
                position: Point::default(),
            };
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
                );
            tabs = tabs.child(
                div()
                    .h(px(34.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .rounded_t_md()
                    .border_b_2()
                    .border_color(if selected { rgb(theme.accent) } else { rgba(0) })
                    .bg(rgb(if selected {
                        theme.chrome_active
                    } else {
                        theme.chrome
                    }))
                    // The outer tab consumes presses after its child controls
                    // have registered clicks and tab drags.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .child(
                        self.control(
                            format!("tab-{id}"),
                            format!("Open {}", n.title),
                            content.into_any_element(),
                            false,
                            cx,
                            move |this, _, _| this.open_note(id),
                        )
                        .h_full()
                        .bg(rgb(if selected {
                            theme.chrome_active
                        } else {
                            theme.chrome
                        }))
                        .text_color(rgb(theme.ink))
                        .hover(move |s| s.bg(rgb(theme.chrome_active)))
                        .on_drag(drag, |drag, position, _, cx| {
                            cx.new(|_| {
                                let mut drag = drag.clone();
                                drag.position = position;
                                drag
                            })
                        })
                        .drag_over::<super::navigation::TabDrag>(move |s, _, _, _| {
                            s.bg(rgb(theme.selected))
                        })
                        .on_drop(cx.listener(
                            move |this, drag: &super::navigation::TabDrag, _, cx| {
                                super::navigation::reorder_tabs(&mut this.open_tabs, drag.id, id);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        self.control(
                            format!("close-tab-{id}"),
                            "Close tab",
                            icon(Icon::Close, theme.muted).into_any_element(),
                            false,
                            cx,
                            move |this, _, _| {
                                this.open_tabs.retain(|v| *v != id);
                                if this.controller.requested_note() == id {
                                    if let Some(next) = this.open_tabs.last().copied() {
                                        this.controller.switch_note(next);
                                    } else {
                                        this.show_library();
                                    }
                                }
                            },
                        )
                        .size(px(28.))
                        .p_1()
                        .mr_2()
                        .bg(rgb(if selected {
                            theme.chrome_active
                        } else {
                            theme.chrome
                        }))
                        .hover(move |s| s.bg(rgb(theme.chrome_active))),
                    ),
            );
        }
        div()
            .h(px(40.))
            .flex_shrink_0()
            .flex()
            .items_end()
            .gap_3()
            .px_3()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, super::titlebar::background_press)
            .on_mouse_down(MouseButton::Right, |event, window, cx| {
                if window.window_controls().window_menu {
                    window.show_window_menu(event.position);
                }
                cx.stop_propagation();
            })
            .child(tabs)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .mb(px(3.))
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
                        .size(px(30.)),
                    ),
            )
            .child(div().flex_1().h_full())
            .child(
                self.window_controls(window, cx)
                    .flex_shrink_0()
                    .mb(px(3.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation()),
            )
    }
    pub(super) fn header(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let favorite = self.controller.session().document.metadata.favorite;
        let title = self.controller.session().document.metadata.title.clone();
        div()
            .h(px(48.))
            .flex_shrink_0()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .bg(rgb(theme.chrome))
            .child(self.chrome_button(
                "library",
                "Library · Ctrl+Shift+L",
                Icon::Library,
                false,
                cx,
                |this, _, _| this.show_library(),
            ))
            .child(self.chrome_button(
                "toggle-pages",
                "Page thumbnails · Ctrl+Shift+P",
                Icon::Sidebar,
                self.pages_open,
                cx,
                |this, _, _| this.pages_open = !this.pages_open,
            ))
            .child(self.chrome_button(
                "search",
                "Search · Ctrl+F",
                Icon::Search,
                false,
                cx,
                |this, w, cx| this.modal(Modal::Search, w, cx),
            ))
            .child(
                self.control(
                    "note-title",
                    "Rename document",
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().max_w(px(320.)).truncate().child(title))
                        .child(icon(Icon::Down, theme.muted))
                        .into_any_element(),
                    false,
                    cx,
                    |this, w, cx| this.modal(Modal::Rename, w, cx),
                )
                .bg(rgb(theme.chrome))
                .text_color(rgb(theme.ink))
                .hover(move |s| s.bg(rgb(theme.chrome_active))),
            )
            .child(div().flex_1())
            .child(self.chrome_button(
                "favorite-note",
                "Favorite document",
                Icon::Star,
                favorite,
                cx,
                |this, _, _| this.controller.metadata(|m| m.favorite = !m.favorite),
            ))
            .child(self.chrome_button(
                "header-add-page",
                "Add page",
                Icon::Plus,
                false,
                cx,
                |this, _, _| this.controller.add_page(),
            ))
            .child(self.chrome_button(
                "export",
                "Export document",
                Icon::Export,
                self.export_open,
                cx,
                |this, _, _| {
                    this.export_open = !this.export_open;
                    this.more_open = false;
                    this.pen_settings = false;
                },
            ))
            .child(self.chrome_button(
                "more",
                "Document actions",
                Icon::More,
                self.more_open,
                cx,
                |this, _, _| {
                    this.more_open = !this.more_open;
                    this.export_open = false;
                    this.pen_settings = false;
                },
            ))
            .child(self.chrome_button(
                "editor-settings",
                "Settings",
                Icon::Settings,
                false,
                cx,
                |this, w, _| this.open_settings(w),
            ))
    }
    pub(super) fn toolbar(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let mut tools = div().flex().items_center().gap_1();
        let pen_icon = match self.controller.style.tool {
            InkTool::Pencil => Icon::Pencil,
            InkTool::Marker => Icon::Marker,
            _ => Icon::Pen,
        };
        for (id, label, kind, tool) in [
            ("pen", "Pen · P", pen_icon, Tool::Pen),
            ("eraser", "Eraser · E", Icon::Eraser, Tool::Eraser),
            ("lasso", "Lasso · L", Icon::Lasso, Tool::Lasso),
            (
                "select-rect",
                "Rectangular selection",
                Icon::Rectangle,
                Tool::Rectangle,
            ),
            ("shape", "Shapes · S", Icon::Shapes, Tool::Shape),
            ("text", "Text · T", Icon::Text, Tool::Text),
            ("hand", "Pan · H", Icon::Hand, Tool::Hand),
        ] {
            let active = self.controller.tool == tool
                && !(tool == Tool::Pen && self.controller.style.tool == InkTool::Highlighter);
            tools =
                tools.child(
                    self.icon_button(id, label, kind, active, cx, move |this, _, _| {
                        if tool == Tool::Pen && this.controller.style.tool == InkTool::Highlighter {
                            this.controller
                                .set_style(this.writing_style.take().unwrap_or_default());
                        }
                        this.region_selection = None;
                        this.controller.set_tool(tool);
                    }),
                );
            if tool == Tool::Pen {
                let selected = self.controller.tool == Tool::Pen
                    && self.controller.style.tool == InkTool::Highlighter;
                tools = tools.child(self.icon_button(
                    "highlighter",
                    "Highlighter",
                    Icon::Highlighter,
                    selected,
                    cx,
                    |this, _, _| {
                        if this.controller.style.tool != InkTool::Highlighter {
                            this.writing_style = Some(this.controller.style.clone());
                        }
                        let style = this
                            .controller
                            .settings
                            .presets
                            .iter()
                            .find(|s| s.tool == InkTool::Highlighter)
                            .cloned()
                            .unwrap_or(folio_document::PenStyle {
                                tool: InkTool::Highlighter,
                                width: 20.,
                                opacity: 0.3,
                                color: Color::from_rgb(0xecc75c),
                                ..Default::default()
                            });
                        this.controller.set_style(style);
                        this.controller.set_tool(Tool::Pen);
                    },
                ));
            }
        }
        tools = tools.child(self.icon_button(
            "toolbar-image",
            "Insert image or PDF",
            Icon::Image,
            false,
            cx,
            |this, _, cx| this.import(cx),
        ));
        let mut colors = div().flex().items_center().gap_2();
        for c in [0x273448, 0x3265a8, 0xc6605c, 0x55917e] {
            let selected = self.controller.style.color.rgb() == c;
            colors = colors.child(
                self.control(
                    format!("color-{c}"),
                    format!("Ink color #{c:06X}"),
                    div()
                        .size(px(18.))
                        .rounded_full()
                        .bg(rgb(c))
                        .into_any_element(),
                    selected,
                    cx,
                    move |this, _, _| this.controller.set_color(Color::from_rgb(c)),
                )
                .size(px(30.))
                .p_0()
                .rounded_full()
                .border_2()
                .border_color(rgb(if selected {
                    theme.accent
                } else {
                    theme.surface
                }))
                .bg(rgb(theme.surface)),
            );
        }
        colors = colors.child(
            self.icon_button(
                "custom-color",
                "Custom ink color",
                Icon::Plus,
                false,
                cx,
                |this, w, cx| this.modal(Modal::Color, w, cx),
            )
            .size(px(30.)),
        );
        let mut widths = div().flex().items_center().gap_1();
        let highlighter = self.controller.style.tool == InkTool::Highlighter;
        for (i, width) in if highlighter {
            [10., 20., 30.]
        } else {
            [1.5, 3., 6.]
        }
        .into_iter()
        .enumerate()
        {
            let selected = (self.controller.style.width - width).abs() < 0.1;
            widths = widths.child(
                self.control(
                    format!("width-{i}"),
                    format!("Stroke width {:.2} mm", width * 0.2646),
                    div()
                        .w(px(17.))
                        .h(px(1.5 + i as f32 * 1.8))
                        .rounded_full()
                        .bg(rgb(theme.ink))
                        .into_any_element(),
                    selected,
                    cx,
                    move |this, _, _| {
                        let mut style = this.controller.style.clone();
                        style.width = width;
                        this.controller.set_style(style);
                    },
                )
                .size(px(32.))
                .p_0(),
            );
        }
        div()
            .id("writing-toolbar")
            .h(px(52.))
            .flex_shrink_0()
            .px_4()
            .flex()
            .items_center()
            .gap_1()
            .bg(rgb(theme.surface))
            .border_b_1()
            .border_color(theme.border)
            .child(
                self.icon_button(
                    "undo",
                    "Undo · Ctrl+Z",
                    Icon::Undo,
                    false,
                    cx,
                    |this, _, _| this.controller.undo(),
                )
                .when(!self.controller.session().history.can_undo(), |s| {
                    s.opacity(0.35)
                }),
            )
            .child(
                self.icon_button(
                    "redo",
                    "Redo · Ctrl+Shift+Z",
                    Icon::Redo,
                    false,
                    cx,
                    |this, _, _| this.controller.redo(),
                )
                .when(!self.controller.session().history.can_redo(), |s| {
                    s.opacity(0.35)
                }),
            )
            .child(self.separator())
            .child(tools)
            .child(div().flex_1())
            .child(self.separator())
            .child(widths)
            .child(self.separator())
            .child(colors)
            .child(self.icon_button(
                "pen-options",
                "Pen settings and presets",
                Icon::Sliders,
                self.pen_settings,
                cx,
                |this, _, _| {
                    this.pen_settings = !this.pen_settings;
                    this.more_open = false;
                    this.export_open = false;
                },
            ))
    }
    pub(super) fn pages_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let total = self.controller.session().document.pages.len();
        let rows = uniform_list(
            "page-thumbnails",
            total,
            cx.processor(|this, range: std::ops::Range<usize>, window, cx| {
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
                    let page = &this.controller.session().document.pages[index];
                    let thumbnail = this.thumbnails.element(page, &this.controller, 122.);
                    let page_id = page.id;
                    let label = page.properties.bookmark.as_ref().map_or_else(
                        || format!("Go to page {}", index + 1),
                        |name| format!("Go to page {} · {name}", index + 1),
                    );
                    let active = index == this.controller.session().page;
                    let content = div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .child(thumbnail)
                        .child(
                            div()
                                .w(px(122.))
                                .truncate()
                                .text_xs()
                                .text_color(rgb(if active { theme.accent } else { theme.muted }))
                                .child(page.properties.bookmark.clone().map_or_else(
                                    || format!("{}", index + 1),
                                    |name| format!("{} · {name}", index + 1),
                                )),
                        );
                    rows.push(
                        div().h(px(260.)).px_3().py_2().child(
                            this.control(
                                format!("page-{}", page.id),
                                label,
                                content.into_any_element(),
                                active,
                                cx,
                                move |this, _, _| this.controller.change_page(index),
                            )
                            .on_drag(PageDrag(page_id), |drag, _, _, cx| cx.new(|_| drag.clone()))
                            .on_drop(cx.listener(move |this, drag: &PageDrag, _, _| {
                                this.controller.reorder_page(drag.0, page_id)
                            }))
                            .p_3()
                            .w_full()
                            .border_2()
                            .border_color(rgb(if active { theme.accent } else { theme.sidebar }))
                            .bg(rgb(if active {
                                theme.selected
                            } else {
                                theme.sidebar
                            })),
                        ),
                    );
                }
                // Virtual rows register during layout, after the root's render.
                this.accessibility.publish(this, window, cx);
                rows
            }),
        )
        .flex_1()
        .min_h_0();
        div()
            .w(px(184.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(theme.sidebar))
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(48.))
                    .px_4()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("Pages · {total}")),
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
                        .size(px(28.))
                        .bg(rgb(theme.sidebar)),
                    ),
            )
            .child(
                self.button(
                    "duplicate-page-panel",
                    "Duplicate page",
                    false,
                    cx,
                    |this, _, _| this.controller.duplicate_page(),
                )
                .m_2(),
            )
            .child(
                self.button(
                    "bookmark-page-panel",
                    "Name bookmark…",
                    false,
                    cx,
                    |this, w, cx| this.modal(Modal::PageBookmark, w, cx),
                )
                .m_2(),
            )
            .child(
                self.button(
                    "move-page-panel",
                    "Move to notebook…",
                    false,
                    cx,
                    |this, w, cx| this.modal(Modal::MovePage, w, cx),
                )
                .m_2(),
            )
            .child(
                self.button(
                    "page-from-template",
                    "Add from template…",
                    false,
                    cx,
                    |this, w, cx| this.modal(Modal::Templates, w, cx),
                )
                .m_2(),
            )
            .child(rows)
            .child(
                self.button("add-page", "＋  Add page", true, cx, |this, _, _| {
                    this.controller.add_page()
                })
                .m_3(),
            )
    }
    pub(super) fn footer(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let page = self.controller.session().page;
        let total = self.controller.session().document.pages.len();
        let zoom = self.controller.session().viewport.zoom;
        div()
            .h(px(34.))
            .flex_shrink_0()
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .bg(rgb(theme.surface))
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
                .size(px(28.))
                .when(page == 0, |s| s.opacity(0.35)),
            )
            .child(
                self.button(
                    "page-count",
                    format!("{} / {}", page + 1, total),
                    self.pages_open,
                    cx,
                    |this, _, _| this.pages_open = !this.pages_open,
                )
                .text_xs()
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
                .size(px(28.))
                .when(page + 1 == total, |s| s.opacity(0.35)),
            )
            .child(div().flex_1())
            .child(icon(
                if self.controller.busy > 0
                    || self.controller.recognition_pending
                    || !self.controller.status.to_lowercase().contains("saved")
                {
                    Icon::Clock
                } else {
                    Icon::Check
                },
                theme.muted,
            ))
            .child(div().text_xs().text_color(rgb(theme.muted)).child(
                if self.controller.recognition_pending {
                    format!("{} (Esc to cancel)", self.controller.recognition_status)
                } else if self.controller.busy > 0 {
                    format!(
                        "{} background task{}",
                        self.controller.busy,
                        if self.controller.busy == 1 { "" } else { "s" }
                    )
                } else {
                    self.controller.status.clone()
                },
            ))
            .child(div().flex_1())
            .child(
                self.button("zoom-out", "−", false, cx, |this, _, _| {
                    this.zoom(1. / 1.2)
                })
                .py_1(),
            )
            .child(
                self.button(
                    "fit-page",
                    format!("{:.0}%", zoom * 100.),
                    false,
                    cx,
                    |this, _, _| this.fit(),
                )
                .text_xs()
                .py_1(),
            )
            .child(
                self.button("zoom-in", "+", false, cx, |this, _, _| this.zoom(1.2))
                    .py_1(),
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
                .size(px(28.)),
            )
    }
}
