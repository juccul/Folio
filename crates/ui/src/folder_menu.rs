//! Folder actions and ID-addressed moves shared by native library drag targets.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LibraryItem {
    Folder(Id),
    Document(Id),
}

#[derive(Clone)]
pub(super) struct LibraryDrag {
    pub item: LibraryItem,
    pub title: String,
    pub theme: Theme,
    pub position: Point<Pixels>,
}
impl Render for LibraryDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().pl(self.position.x).pt(self.position.y).child(
            div()
                .px_3()
                .py_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(self.theme.radius))
                .border_1()
                .border_color(self.theme.border)
                .shadow_sm()
                .bg(rgb(self.theme.popover))
                .text_color(rgb(self.theme.ink))
                .text_sm()
                .child(icon(
                    match self.item {
                        LibraryItem::Folder(_) => Icon::Folder,
                        LibraryItem::Document(_) => Icon::Book,
                    },
                    self.theme.muted,
                ))
                .child(self.title.clone()),
        )
    }
}

fn accepts_move(controller: &Controller, item: LibraryItem, destination: Option<Id>) -> bool {
    if destination.is_some_and(|id| !controller.notebooks.iter().any(|n| n.id == id)) {
        return false;
    }
    match item {
        LibraryItem::Folder(id) => {
            controller
                .notebooks
                .iter()
                .any(|n| n.id == id && n.parent != destination)
                && controller.validate_folder_move(id, destination).is_ok()
        }
        LibraryItem::Document(id) => controller
            .notes
            .iter()
            .any(|n| n.id == id && !n.trashed && n.notebook != destination),
    }
}

impl NotesView {
    pub(super) fn folder_drop_target(
        &self,
        element: Stateful<Div>,
        destination: Option<Id>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let entity = cx.entity();
        element
            .can_drop(move |value, _, cx| {
                value.downcast_ref::<LibraryDrag>().is_some_and(|drag| {
                    let view = entity.read(cx);
                    !view.update_preparing
                        && !view.blocking_overlay()
                        && accepts_move(&view.controller, drag.item, destination)
                })
            })
            .drag_over::<LibraryDrag>(move |s, _, _, _| {
                s.bg(rgb(theme.selected))
                    .border_1()
                    .border_color(rgb(theme.sidebar_accent))
            })
            .on_drop(cx.listener(move |this, drag: &LibraryDrag, _, cx| {
                // Recheck against current data: the library can finish background loads
                // between rendering the target and releasing the pointer.
                if this.update_preparing
                    || this.blocking_overlay()
                    || !accepts_move(&this.controller, drag.item, destination)
                {
                    return;
                }
                match drag.item {
                    LibraryItem::Folder(id) => {
                        if let Err(error) = this.controller.move_notebook(id, destination) {
                            this.controller.error = Some(error);
                        }
                    }
                    LibraryItem::Document(id) => this
                        .controller
                        .manage_note(id, folio_app::NoteAction::Move(destination)),
                }
                if let Some(id) = destination {
                    let collapsed = &mut this.controller.settings.collapsed_folders;
                    if collapsed.contains(&id) {
                        collapsed.retain(|folder| *folder != id);
                        this.controller.store_settings();
                    }
                }
                this.folder_menu = None;
                this.document_menu = None;
                cx.notify();
            }))
    }

    pub(super) fn open_folder_menu(
        &mut self,
        id: Id,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.controller.notebooks.iter().any(|n| n.id == id) {
            return;
        }
        self.dismiss_popovers();
        self.document_menu = None;
        self.folder_menu = Some((id, at));
        self.focus.focus(window);
        cx.notify();
    }

    pub(super) fn folder_menu_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Some((id, at)) = self.folder_menu else {
            return div().into_any_element();
        };
        if !self.controller.notebooks.iter().any(|n| n.id == id) {
            self.folder_menu = None;
            return div().into_any_element();
        }
        let theme = Theme::new(&self.controller.settings);
        let reason = self.controller.folder_deletion_reason(id);
        let (_, trashed, _) = self.controller.folder_contents(id);
        let size = window.viewport_size();
        let width = 260_f32.min((f32::from(size.width) - 24.).max(1.));
        let height = if reason.is_some() { 350. } else { 230. };
        let x = f32::from(at.x).clamp(12., (f32::from(size.width) - width - 12.).max(12.));
        let y = f32::from(at.y).clamp(12., (f32::from(size.height) - height - 12.).max(12.));
        let mut menu = div()
            .id("folder-context-menu")
            .occlude()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .max_h(px((f32::from(size.height) - y - 12.).max(1.)))
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(theme.radius + 2.))
            .bg(rgb(theme.popover))
            .border_1()
            .border_color(theme.border)
            .shadow_md()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.folder_menu = None;
                cx.notify();
            }))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .truncate()
                    .text_color(rgb(theme.muted))
                    .child(self.controller.folder_path(id)),
            );
        for (key, label, modal) in [
            ("rename", "Rename folder…", Modal::RenameNotebook(id)),
            ("child", "New subfolder…", Modal::Notebook(Some(id))),
            ("move", "Move folder…", Modal::MoveNotebook(id)),
        ] {
            menu = menu.child(
                self.button(
                    format!("folder-menu-{key}"),
                    label,
                    false,
                    cx,
                    move |this, w, cx| {
                        this.folder_menu = None;
                        this.modal(modal.clone(), w, cx);
                    },
                )
                .w_full()
                .justify_start(),
            );
        }
        menu = menu.child(
            self.button(
                "delete-folder",
                "Delete empty folder",
                false,
                cx,
                move |this, _, _| {
                    if let Err(error) = this.controller.delete_empty_notebook(id) {
                        this.controller.error = Some(error);
                    } else {
                        this.folder_menu = None;
                    }
                },
            )
            .w_full()
            .justify_start(),
        );
        if let Some(reason) = reason {
            menu = menu.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(reason),
            );
        }
        if trashed > 0 {
            menu = menu.child(
                self.button(
                    "view-folder-trash",
                    format!("View {trashed} in Trash"),
                    false,
                    cx,
                    move |this, _, _| {
                        this.controller.filter = NoteFilter::NotebookTrash(id);
                        this.folder_menu = None;
                    },
                )
                .w_full()
                .justify_start(),
            );
        }
        menu.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn drag_destinations_reject_cycles_collisions_trash_and_stale_items() {
        let root = std::env::temp_dir().join(format!("folio-library-drag-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let a = app.create_notebook("A".into(), None).unwrap();
        let b = app.create_notebook("B".into(), Some(a)).unwrap();
        let other = app.create_notebook("Other".into(), None).unwrap();
        let colliding = app.create_notebook("B".into(), Some(other)).unwrap();
        assert!(!accepts_move(&app, LibraryItem::Folder(a), Some(a)));
        assert!(!accepts_move(&app, LibraryItem::Folder(a), Some(b)));
        assert!(!accepts_move(&app, LibraryItem::Folder(b), Some(other)));
        assert!(!accepts_move(&app, LibraryItem::Folder(b), Some(a)));
        assert!(accepts_move(&app, LibraryItem::Folder(b), None));
        assert!(accepts_move(&app, LibraryItem::Folder(colliding), Some(b)));
        assert!(!accepts_move(&app, LibraryItem::Folder(Id::new_v4()), None));
        app.create_note();
        let note = app.active;
        assert!(accepts_move(&app, LibraryItem::Document(note), Some(b)));
        assert!(!accepts_move(
            &app,
            LibraryItem::Document(note),
            Some(Id::new_v4())
        ));
        app.manage_note(note, folio_app::NoteAction::Move(Some(b)));
        assert!(!accepts_move(&app, LibraryItem::Document(note), Some(b)));
        assert!(accepts_move(&app, LibraryItem::Document(note), None));
        app.manage_note(note, folio_app::NoteAction::Trash);
        assert!(!accepts_move(&app, LibraryItem::Document(note), None));
        app.flush().unwrap();
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
}
