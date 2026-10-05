//! Library context menus and tab presentation; mutations are ID-addressed controller commands.
use super::*;
#[derive(Clone)]
pub(super) struct TabDrag {
    pub id: Id,
    pub title: String,
    pub theme: Theme,
    pub position: Point<Pixels>,
}
impl Render for TabDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().pl(self.position.x).pt(self.position.y).child(
            div()
                .px_4()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(self.theme.border)
                .shadow_sm()
                .bg(rgb(self.theme.popover))
                .text_color(rgb(self.theme.ink))
                .text_sm()
                .child(self.title.clone()),
        )
    }
}
/// Moving across a neighbor swaps its position; repeated drops allow either direction.
pub(super) fn reorder_tabs(tabs: &mut Vec<Id>, source: Id, target: Id) {
    let (Some(from), Some(to)) = (
        tabs.iter().position(|id| *id == source),
        tabs.iter().position(|id| *id == target),
    ) else {
        return;
    };
    if from != to {
        let id = tabs.remove(from);
        tabs.insert(to, id);
    }
}
impl NotesView {
    pub(super) fn open_document_menu(
        &mut self,
        id: Id,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.document_menu = Some((id, at));
        self.document_menu_folders = false;
        self.focus.focus(window);
        cx.notify();
    }
    pub(super) fn document_menu_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let Some((id, at)) = self.document_menu else {
            return div().into_any_element();
        };
        let Some(note) = self.controller.notes.iter().find(|n| n.id == id).cloned() else {
            self.document_menu = None;
            return div().into_any_element();
        };
        let size = window.viewport_size();
        let width = 260.;
        let height = if self.document_menu_folders {
            330.
        } else {
            350.
        };
        let x = f32::from(at.x).clamp(12., (f32::from(size.width) - width - 12.).max(12.));
        let y = f32::from(at.y).clamp(12., (f32::from(size.height) - height - 12.).max(12.));
        let mut menu = div()
            .id("document-context-menu")
            .occlude()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
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
                this.document_menu = None;
                cx.notify();
            }))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .truncate()
                    .text_color(rgb(theme.muted))
                    .child(note.title),
            );
        if self.document_menu_folders {
            menu = menu.child(
                self.button(
                    "doc-menu-back",
                    "‹  Document actions",
                    false,
                    cx,
                    |this, _, _| this.document_menu_folders = false,
                )
                .justify_start(),
            );
            let mut folders = div()
                .id("document-move-destinations")
                .max_h(px(240.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            let mut destinations = vec![(None, "All documents".to_owned())];
            destinations.extend(
                self.controller
                    .notebooks
                    .iter()
                    .map(|n| (Some(n.id), n.name.clone())),
            );
            for (folder, name) in destinations {
                folders = folders.child(
                    self.button(
                        format!("doc-move-{folder:?}"),
                        format!("Move to {name}"),
                        note.notebook == folder,
                        cx,
                        move |this, _, _| {
                            this.controller
                                .manage_note(id, folio_app::NoteAction::Move(folder));
                            this.document_menu = None;
                        },
                    )
                    .justify_start()
                    .w_full(),
                );
            }
            return menu.child(folders).into_any_element();
        }
        for (key, label, action) in [
            ("open", "Open document", 0),
            ("rename", "Rename…", 1),
            ("duplicate", "Duplicate", 2),
            (
                "favorite",
                if note.favorite {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                },
                3,
            ),
            ("move", "Move to folder…", 4),
            ("tags", "Edit tags…", 5),
            (
                "trash",
                if note.trashed {
                    "Restore document"
                } else {
                    "Move to trash"
                },
                6,
            ),
        ] {
            menu = menu.child(
                self.button(
                    format!("doc-menu-{key}"),
                    label,
                    false,
                    cx,
                    move |this, w, cx| {
                        if action == 4 {
                            this.document_menu_folders = true;
                            return;
                        }
                        this.document_menu = None;
                        match action {
                            0 => this.open_note(id),
                            1 => this.modal(Modal::RenameDocument(id), w, cx),
                            2 => this
                                .controller
                                .manage_note(id, folio_app::NoteAction::Duplicate),
                            3 => this
                                .controller
                                .manage_note(id, folio_app::NoteAction::Favorite),
                            5 => this.modal(Modal::DocumentTags(id), w, cx),
                            _ => {
                                this.controller
                                    .manage_note(id, folio_app::NoteAction::Trash);
                                if !note.trashed {
                                    this.open_tabs.retain(|tab| *tab != id);
                                }
                            }
                        }
                    },
                )
                .justify_start()
                .w_full()
                .when(action == 6 && !note.trashed, |s| {
                    s.text_color(rgb(theme.destructive))
                }),
            );
        }
        menu.into_any_element()
    }
}

impl NotesView {
    /// Synthetic native-window checks, used only by the explicit --smoke-test harness.
    pub fn navigation_smoke_setup(&mut self) -> (Id, Id) {
        self.controller.create_note();
        self.controller.rename("Smoke A".into());
        let a = self.controller.active;
        self.controller.create_note();
        self.controller.rename("Smoke B".into());
        let b = self.controller.active;
        self.open_tabs.clear();
        self.show_library();
        (a, b)
    }
    pub fn navigation_smoke_click(
        &self,
        label: &str,
        right: bool,
    ) -> Result<Vec<PlatformInput>, String> {
        let rect = self
            .accessibility
            .control_bounds(label)
            .ok_or_else(|| format!("No rendered control: {label}"))?;
        let position = point(
            px(((rect.x0 + rect.x1) * 0.5) as f32),
            px(((rect.y0 + rect.y1) * 0.5) as f32),
        );
        let button = if right {
            MouseButton::Right
        } else {
            MouseButton::Left
        };
        Ok(vec![
            PlatformInput::MouseMove(MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Default::default(),
            }),
            PlatformInput::MouseDown(MouseDownEvent {
                button,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }),
            PlatformInput::MouseUp(MouseUpEvent {
                button,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }),
        ])
    }
    pub fn navigation_smoke_context(&self, id: Id) -> Result<(), String> {
        if self.document_menu.is_some_and(|(note, _)| note == id) && self.library_open {
            Ok(())
        } else {
            Err("Right-click did not open the target card menu".into())
        }
    }
    pub fn navigation_smoke_open(&mut self, id: Id) {
        self.open_note(id);
    }
    pub fn navigation_smoke_drag(&self) -> Result<Vec<PlatformInput>, String> {
        let bounds = |label: &str| {
            self.accessibility
                .control_bounds(label)
                .map(|r| {
                    point(
                        px(((r.x0 + r.x1) * 0.5) as f32),
                        px(((r.y0 + r.y1) * 0.5) as f32),
                    )
                })
                .ok_or_else(|| format!("No tab: {label}"))
        };
        let start = bounds("Open Smoke A")?;
        let end = bounds("Open Smoke B")?;
        let mut events = vec![
            PlatformInput::MouseMove(MouseMoveEvent {
                position: start,
                pressed_button: None,
                modifiers: Default::default(),
            }),
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }),
        ];
        for i in 1..=12 {
            events.push(PlatformInput::MouseMove(MouseMoveEvent {
                position: point(start.x + (end.x - start.x) * (i as f32 / 12.), start.y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }));
        }
        events.push(PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: end,
            modifiers: Default::default(),
            click_count: 1,
        }));
        Ok(events)
    }
    pub fn navigation_smoke_verify(&mut self, a: Id, b: Id) -> Result<(), String> {
        if self.open_tabs != vec![b, a] {
            return Err(format!("Native tab drag failed: {:?}", self.open_tabs));
        }
        if self.controller.active != b {
            return Err("Dragging tabs changed the active document".into());
        }
        if self.modal.is_some() || self.library_open {
            return Err("Tab picker did not open the selected document".into());
        }
        if !self
            .controller
            .notes
            .iter()
            .any(|n| n.title == "Smoke A (copy)")
        {
            return Err("Card menu duplication failed".into());
        }
        self.controller.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::reorder_tabs;
    use core::prelude::v1::test;
    use folio_document::Id;
    #[test]
    fn tabs_reorder_in_both_directions_without_adding_or_losing_documents() {
        let [a, b, c] = [Id::new_v4(), Id::new_v4(), Id::new_v4()];
        let mut tabs = vec![a, b, c];
        reorder_tabs(&mut tabs, a, c);
        assert_eq!(tabs, vec![b, c, a]);
        reorder_tabs(&mut tabs, a, b);
        assert_eq!(tabs, vec![a, b, c]);
        reorder_tabs(&mut tabs, b, b);
        assert_eq!(tabs, vec![a, b, c]);
        reorder_tabs(&mut tabs, Id::new_v4(), a);
        assert_eq!(tabs, vec![a, b, c]);
    }
}
