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
}
impl DragView for LibraryDrag {
    fn preview(&self) -> Div {
        div().child(
            div()
                .px_3()
                .py_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(self.theme.radius))
                .glass(self.theme, self.theme.popover)
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

pub(super) fn accepts_move(
    controller: &Controller,
    item: LibraryItem,
    destination: Option<Id>,
) -> bool {
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
        element
            .drag_over::<LibraryDrag>(move |s, _, _, _| {
                s.bg(theme.tint(1.))
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
        self.menu_focus = Some("folder-menu-rename".into());
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
        // `at` is in window coordinates, but the menu's layer fills the body
        // below the title bar and tabs.
        let viewport = window.viewport_size();
        let (left, top, room_w, room_h) = self
            .accessibility
            .control_bounds_for_key("chrome-body")
            .map_or(
                (0., 0., f32::from(viewport.width), f32::from(viewport.height)),
                |r| (r.x0 as f32, r.y0 as f32, (r.x1 - r.x0) as f32, (r.y1 - r.y0) as f32),
            );
        let at = point(at.x - px(left), at.y - px(top));
        let width = 260_f32.min((room_w - 24.).max(1.));
        // The menu's height in layout points (which the interface scale
        // does not change): its header, three actions and Delete with their
        // separators, then the deletion note and the Trash action when it
        // has them.
        let height = 182.
            + if reason.is_some() { 44. } else { 0. }
            + if trashed > 0 { 33. } else { 0. };
        let x = f32::from(at.x).clamp(12., (room_w - width - 12.).max(12.));
        let y = f32::from(at.y).clamp(12., (room_h - height - 12.).max(12.));
        // A dialog chosen here turns the menu into its page, which may take
        // what the window leaves below the menu.
        let hosted = self.modal_hosted_in(MenuHost::Folder);
        let width = if hosted { width.max(280_f32.min((room_w - 24.).max(1.))) } else { width };
        let x = x.min((room_w - width - 12.).max(12.));
        let menu = div()
            .id("folder-context-menu")
            .occlude()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .grows_from_point((f32::from(at.x), f32::from(at.y)), (x, y, width, height))
            .max_h(px((room_h - y - 12.).max(1.)))
            .overflow_y_scroll()
            .p_1p5()
            .flex()
            .flex_col()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .shadow_md()
            // It springs to the size of the page it turns into and back.
            .el(|e| e.motion(crate::motion::MORPH).animated_changes())
            .on_mouse_down_out(cx.listener(|this, _, w, cx| {
                if this.modal_hosted_in(MenuHost::Folder) {
                    this.cancel_modal(w, cx);
                }
                this.folder_menu = None;
                cx.notify();
            }));
        if hosted {
            return menu.child(self.modal_panel(true, window, cx)).into_any_element();
        }
        let mut items = div()
            .id("folder-menu-items")
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .page_in(false)
            .child(
                div()
                    .px_2()
                    .pt_1()
                    .pb(px(2.))
                    .text_xs()
                    .truncate()
                    .text_color(rgb(theme.muted))
                    .child(self.controller.folder_path(id)),
            );
        for (key, label, kind, modal) in [
            ("rename", "Rename folder…", Icon::Pencil, Modal::RenameNotebook(id)),
            ("child", "New subfolder…", Icon::Plus, Modal::Notebook(Some(id))),
            ("move", "Move folder…", Icon::Folder, Modal::MoveNotebook(id)),
        ] {
            items = items.child(self.menu_row(
                format!("folder-menu-{key}"),
                label.into(),
                div().child(label),
                kind,
                theme.muted,
                None,
                cx,
                // The menu turns into the action's page.
                move |this, w, cx| this.modal_in_menu(MenuHost::Folder, modal.clone(), w, cx),
            ));
        }
        items = items.child(document_actions::separator(theme)).child(
            self.menu_row(
                "delete-folder",
                "Delete empty folder".into(),
                div().text_color(rgb(theme.destructive)).child("Delete empty folder"),
                Icon::Trash,
                theme.destructive,
                None,
                cx,
                move |this, _, _| {
                    if let Err(error) = this.controller.delete_empty_notebook(id) {
                        this.controller.error = Some(error);
                    } else {
                        this.folder_menu = None;
                    }
                },
            ),
        );
        if let Some(reason) = reason {
            items = items.child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(reason),
            );
        }
        if trashed > 0 {
            let label = format!("View {trashed} in Trash");
            items = items.child(self.menu_row(
                "view-folder-trash",
                label.clone().into(),
                div().child(label),
                Icon::Trash,
                theme.muted,
                None,
                cx,
                move |this, _, _| {
                    this.controller.filter = NoteFilter::NotebookTrash(id);
                    this.folder_menu = None;
                },
            ));
        }
        menu.child(items).into_any_element()
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
