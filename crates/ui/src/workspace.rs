//! Library, document chrome and page navigation. All mutations go through Controller.
use super::*;
use folio_document::Page;
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
}
struct Thumbnail {
    key: ThumbnailKey,
    generation: u64,
    image: Option<Arc<RenderImage>>,
    error: Option<String>,
    aspect: f32,
    touched: u64,
}
impl NotesView {
    fn thumbnail(&mut self, mut page: Page, width: f32, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let palette = theme.canvas_for_page(&page.properties);
        let pdf_ready = page
            .properties
            .pdf
            .as_ref()
            .and_then(|p| p.preview_asset.as_ref())
            .is_none_or(|a| self.controller.assets.join(a).is_file());
        let key = ThumbnailKey {
            revision: page.revision,
            properties: page.properties.clone(),
            theme: palette,
            pdf_ready,
        };
        self.thumbnails.clock += 1;
        let touched = self.thumbnails.clock;
        let id = page.id;
        let default_aspect = page.properties.width / page.properties.height.max(1.);
        let valid = self
            .thumbnails
            .entries
            .get(&id)
            .is_some_and(|e| e.key == key);
        if !valid && self.thumbnails.pending.len() < 2 && !self.thumbnails.pending.contains_key(&id)
        {
            if self.thumbnails.entries.len() >= 48 {
                if let Some(oldest) = self
                    .thumbnails
                    .entries
                    .iter()
                    .filter(|(id, _)| !self.thumbnails.pending.contains_key(id))
                    .min_by_key(|(_, e)| e.touched)
                    .map(|(id, _)| *id)
                {
                    self.thumbnails.entries.remove(&oldest);
                }
            }
            let generation = touched;
            let aspect = page.properties.width / page.properties.height.max(1.);
            self.thumbnails.entries.insert(
                id,
                Thumbnail {
                    key,
                    generation,
                    image: None,
                    error: None,
                    aspect,
                    touched,
                },
            );
            self.thumbnails.pending.insert(id, generation);
            export_options::apply(&mut page, theme, export_options::Appearance::Visible);
            let assets = self.controller.assets.clone();
            let task = cx.background_executor().spawn(async move {
                folio_export::raster_page_limited(&page, &assets, 384)
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
        let height = (width / aspect.max(0.01)).min(200. * self.controller.settings.ui_scale);
        let mut element = div()
            .w(px((height * aspect).min(width)))
            .h(px(height))
            .overflow_hidden()
            .bg(rgb(palette.paper))
            .flex()
            .items_center()
            .justify_center();
        if valid && let Some(image) = image {
            element = element.child(img(image).size_full().object_fit(ObjectFit::Contain));
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
    /// Open a document without changing its model or viewport.
    pub fn show_editor(&mut self) {
        self.document_menu = None;
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
        .size(rems(2.375))
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
            let active = self.controller.filter == filter
                || (filter == NoteFilter::Trash
                    && matches!(self.controller.filter, NoteFilter::NotebookTrash(_)));
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
        let mut collapsed_depth = None;
        for (n, depth) in self.controller.folder_tree() {
            if collapsed_depth.is_some_and(|hidden| depth > hidden) {
                continue;
            }
            collapsed_depth = None;
            let id = n.id;
            let path = self.controller.folder_path(id);
            let branch = self
                .controller
                .notebooks
                .iter()
                .any(|child| child.parent == Some(id));
            let collapsed = self.controller.settings.collapsed_folders.contains(&id);
            if branch && collapsed {
                collapsed_depth = Some(depth);
            }
            let active = matches!(self.controller.filter, NoteFilter::Notebook(folder) | NoteFilter::NotebookTrash(folder) if folder == id);
            let content = div()
                .flex()
                .min_w_0()
                .items_center()
                .gap_3()
                .child(icon(Icon::Folder, theme.muted))
                .child(div().min_w_0().truncate().child(n.name.clone()));
            folders = folders.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .min_h(rems(2.75))
                    .items_center()
                    .pl(px(depth as f32 * 12.))
                    .child(if branch {
                        self.control(
                            format!("toggle-folder-{id}"),
                            format!("{} {path}", if collapsed { "Expand" } else { "Collapse" }),
                            div()
                                .child(if collapsed { "▸" } else { "▾" })
                                .into_any_element(),
                            !collapsed,
                            cx,
                            move |this, _, _| {
                                let folders = &mut this.controller.settings.collapsed_folders;
                                if folders.contains(&id) {
                                    folders.retain(|f| *f != id);
                                } else {
                                    folders.push(id);
                                }
                                this.controller.store_settings();
                            },
                        )
                        .size(px(24.))
                        .p_0()
                        .into_any_element()
                    } else {
                        div().w(px(24.)).into_any_element()
                    })
                    .child(
                        self.control(
                            format!("folder-{id}"),
                            path,
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
                    .child("Keep related documents together."),
            );
        }
        if let NoteFilter::Notebook(id) | NoteFilter::NotebookTrash(id) = self.controller.filter {
            folders = folders
                .child(
                    self.button(
                        format!("rename-notebook-{id}"),
                        "Rename folder…",
                        false,
                        cx,
                        move |this, w, cx| this.modal(Modal::RenameNotebook(id), w, cx),
                    )
                    .text_xs()
                    .justify_start(),
                )
                .child(
                    self.button(
                        format!("child-notebook-{id}"),
                        "New subfolder…",
                        false,
                        cx,
                        move |this, w, cx| this.modal(Modal::Notebook(Some(id)), w, cx),
                    )
                    .text_xs()
                    .justify_start(),
                )
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
            if let Some(reason) = self.controller.folder_deletion_reason(id) {
                folders = folders.child(
                    div()
                        .p_2()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(reason),
                );
            }
            let (_, trashed, _) = self.controller.folder_contents(id);
            if trashed > 0 {
                folders = folders.child(
                    self.button(
                        "view-folder-trash",
                        format!("View {trashed} in Trash"),
                        false,
                        cx,
                        move |this, _, _| {
                            this.controller.filter = NoteFilter::NotebookTrash(id);
                        },
                    )
                    .text_xs()
                    .justify_start(),
                );
            }
        }
        div()
            .w(rems(13.5))
            .max_w(relative(0.27))
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
                        .size(rems(1.875))
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
        list_view: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut items = div()
            .w_full()
            .flex()
            .when(list_view, |s| s.flex_col())
            .when(!list_view, |s| s.flex_wrap().gap_6());
        for n in notes {
            let id = n.id;
            let session_pages = self
                .controller
                .sessions
                .get(&id)
                .map(|s| s.document.pages.len());
            let preview = self.controller.library_preview(id);
            let pages = preview.as_ref().map(|(_, count)| *count).or(session_pages);
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
                    icon(Icon::Star, if n.favorite { theme.ink } else { theme.muted })
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
                .size(rems(2.))
                .p_0()
                .min_h(rems(2.))
                .flex_shrink_0()
                .bg(transparent_black())
                .hover(move |s| s.bg(rgb(theme.selected)))
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
                        this.open_document_menu(id, w.mouse_position(), w, cx);
                        cx.stop_propagation();
                    },
                )
                .size(rems(2.))
                .min_h(rems(2.))
                .flex_shrink_0()
                .bg(transparent_black())
                .hover(move |s| s.bg(rgb(theme.selected)));
            let open =
                move |this: &mut Self, _: &mut Window, _: &mut Context<Self>| this.open_note(id);
            let context_menu = cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                this.open_document_menu(id, event.position, w, cx);
                cx.stop_propagation();
            });
            let title = div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
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
                            .gap_1()
                            .child(title)
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.muted))
                                    .truncate()
                                    .child(format!("{details} · {}", edited_label(n.updated_at))),
                            ),
                    );
                items = items.child(
                    div()
                        .w_full()
                        .h(rems(4.5))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .border_b_1()
                        .border_color(theme.border)
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
                            .rounded_none()
                            .bg(transparent_black())
                            .hover(move |s| s.bg(rgb(theme.sidebar)))
                            .on_mouse_down(MouseButton::Right, context_menu),
                        )
                        .child(favorite)
                        .child(menu),
                );
            } else {
                let cover_content = if let Some((page, _)) = &preview {
                    if let Some(pdf) = &page.properties.pdf {
                        self.controller.request_pdf_background(pdf.clone());
                    }
                    self.thumbnail(
                        page.as_ref().clone(),
                        144. * self.controller.settings.ui_scale,
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
                    .w(rems(9.25))
                    .h(rems(12.375))
                    .overflow_hidden()
                    .rounded(px(theme.radius))
                    .border_1()
                    .border_color(theme.border)
                    .bg(rgb(theme.sidebar))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(cover_content);
                let content = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .items_center()
                    .child(cover)
                    .child(title.w_full());
                items = items.child(
                    div()
                        .w(rems(11.5))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
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
                            .p_3()
                            .bg(transparent_black())
                            .hover(move |s| s.bg(rgb(theme.sidebar)))
                            .on_mouse_down(MouseButton::Right, context_menu),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .px_3()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .text_xs()
                                        .text_color(rgb(theme.muted))
                                        .child(div().truncate().child(details))
                                        .child(div().truncate().child(edited_label(n.updated_at))),
                                )
                                .child(favorite)
                                .child(menu),
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
        let list_view = self.list_view
            || (compact
                && f32::from(window.viewport_size().height) / self.controller.settings.ui_scale
                    < 450.);
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
        let mut shelf = div()
            .id("library-shelf")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p_7()
            .when(count == 0 && compact, |shelf| shelf.p_4())
            .when(count == 0, |shelf| shelf.overflow_y_scroll());
        let columns = if list_view {
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
                let visible_start = range.start * columns;
                let visible_end = (range.end * columns).min(total);
                for row in range {
                    let start = row * columns;
                    let end = (start + columns).min(total);
                    rows.push(
                        this.library_items(notes[start..end].to_vec(), list_view, cx)
                            .h(rems(if list_view { 4.5 } else { 20. }))
                            .items_start(),
                    );
                }
                this.accessibility
                    .retain_library_items(&notes[visible_start..visible_end]);
                this.accessibility.publish(this, window, cx);
                rows
            }),
        )
        .flex_1()
        .min_h_0();
        if count == 0 {
            let (heading, message, symbol) = match self.controller.filter {
                NoteFilter::Trash | NoteFilter::NotebookTrash(_) => (
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
            let mut empty = div()
                .py_6()
                .mb_4()
                .when(compact, |empty| empty.py_2().mb_0())
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
                .child(div().text_sm().text_color(rgb(theme.muted)).child(message));
            if self.controller.filter == NoteFilter::All {
                empty = empty.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .mt_3()
                        .child(self.button(
                            "start-writing",
                            "Start writing",
                            true,
                            cx,
                            |this, w, cx| this.new_notebook(w, cx),
                        ))
                        .child(self.button(
                            "try-sample",
                            "Try the sample notebook",
                            false,
                            cx,
                            |this, w, cx| {
                                this.controller.create_starter_notebook();
                                this.show_editor();
                                this.fit();
                                this.focus.focus(w);
                                cx.notify();
                            },
                        )),
                );
            }
            shelf = shelf.child(empty);
        }
        shelf = shelf.child(list);
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
                .text_xs()
                .py_0()
                .px_1(),
            );
            for folder in self.controller.folder_ancestors(id) {
                let target = folder.id;
                row = row.child(div().text_xs().child("/")).child(
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
                    .max_w(px(150.))
                    .truncate(),
                );
            }
            Some(row)
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
            .bg(rgb(theme.bg))
            .child(
                div()
                    .min_h(rems(4.5))
                    .py_2()
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
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(rems(1.5625))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            )
                            .children(breadcrumb),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(self.icon_button(
                                "search",
                                "Search all documents · Ctrl+F",
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
                            )
                            .child(self.icon_button(
                                "library-new-options",
                                "New document with options…",
                                Icon::Sliders,
                                false,
                                cx,
                                |this, w, cx| this.modal(Modal::NewDocument, w, cx),
                            )),
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
                    .child(div().text_xs().text_color(rgb(theme.muted)).child(format!(
                        "{count} document{}",
                        if count == 1 { "" } else { "s" }
                    )))
                    .child(div().flex_1())
                    .child(self.icon_button(
                        "grid-view",
                        "Grid view",
                        Icon::Grid,
                        !list_view,
                        cx,
                        |this, _, _| this.list_view = false,
                    ))
                    .child(self.icon_button(
                        "list-view",
                        "List view",
                        Icon::List,
                        list_view,
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
                    .child(self.controller.activity_status()),
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
            .gap_1();
        tabs = tabs.child(
            self.icon_button(
                "tab-library",
                "Documents · Ctrl+Shift+L",
                Icon::Library,
                self.library_open,
                cx,
                |this, _, _| this.show_library(),
            )
            .h(rems(2.125)),
        );
        for id in self.open_tabs.clone() {
            let Some(n) = self.controller.notes.iter().find(|n| n.id == id) else {
                continue;
            };
            let selected = !self.library_open && id == active;
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
                    .h(rems(2.125))
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
                            selected,
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
                            move |this, w, cx| this.close_document_tab(id, w, cx),
                        )
                        .size(rems(1.75))
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
            .h(rems(2.5))
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
                        .size(rems(1.875)),
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
            .h(rems(3.0))
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
                    this.more_section = MoreSection::Document;
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
    pub(super) fn toolbar(&mut self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let mut tools = div().flex().flex_wrap().min_w_0().items_center().gap_1();
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
                            .tool_styles
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
        if self.controller.tool != Tool::Eraser {
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
                    .size(rems(1.875))
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
                .size(rems(1.875)),
            );
        }
        let mut widths = div().flex().items_center().gap_1();
        if self.controller.tool != Tool::Eraser {
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
                    .size(rems(2.0))
                    .p_0(),
                );
            }
        }
        let compact =
            f32::from(window.viewport_size().width) / self.controller.settings.ui_scale < 1100.;
        let primary = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .when(compact, |row| row.w_full())
            .child(self.icon_button(
                "undo",
                "Undo · Ctrl+Z",
                Icon::Undo,
                false,
                cx,
                |this, _, _| this.controller.undo(),
            ))
            .child(self.icon_button(
                "redo",
                "Redo · Ctrl+Shift+Z",
                Icon::Redo,
                false,
                cx,
                |this, _, _| this.controller.redo(),
            ))
            .child(self.separator())
            .child(tools);
        let mut options = div().flex().flex_wrap().items_center().gap_1();
        if self.controller.tool == Tool::Eraser {
            options = options.child(self.eraser_controls("toolbar", cx));
        } else {
            options = options.child(widths).child(self.separator()).child(colors);
        }
        options = options.child(self.icon_button(
            "pen-options",
            if self.controller.tool == Tool::Eraser {
                "Eraser modes and size"
            } else {
                "Pen settings and presets"
            },
            Icon::Sliders,
            self.pen_settings,
            cx,
            |this, _, _| {
                this.pen_settings = !this.pen_settings;
                this.more_open = false;
                this.export_open = false;
            },
        ));
        div()
            .id("writing-toolbar")
            .min_h(rems(3.25))
            .py_1()
            .flex_shrink_0()
            .px_4()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .bg(rgb(theme.surface))
            .border_b_1()
            .border_color(theme.border)
            .child(primary)
            .child(div().flex_1())
            .child(options)
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
                    let page = this.controller.session().document.pages[index].clone();
                    let thumbnail = this.thumbnail(page.clone(), 122., cx);
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
            .w(rems(11.5))
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
                            "page-options-panel",
                            "More page options…",
                            Icon::More,
                            false,
                            cx,
                            |this, _, _| {
                                this.more_section = MoreSection::Page;
                                this.more_open = true;
                                this.export_open = false;
                                this.pen_settings = false;
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
                        .size(rems(1.75))
                        .bg(rgb(theme.sidebar)),
                    ),
            )
            .child(rows)
            .child(
                self.button("add-page", "＋  Add page", true, cx, |this, _, _| {
                    this.controller.add_page()
                })
                .text_xs()
                .m_2(),
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
                .size(rems(1.75))
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
                .size(rems(1.75))
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
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(self.controller.activity_status()),
            )
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
                .size(rems(1.75)),
            )
    }
}
