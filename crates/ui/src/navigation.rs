//! Library context menus and tab presentation; mutations are ID-addressed controller commands.
use super::*;
/// A tab being dragged along the tab bar, as in web browsers: the tab
/// follows the pointer and its neighbors make room for it.
#[derive(Clone, Debug)]
pub(super) struct TabDrag {
    pub id: Id,
    /// Pointer x when the tab was pressed, and now.
    pub start: f32,
    pub pointer: f32,
    /// Set once the pointer has moved far enough to be a drag, not a click.
    pub moving: bool,
    /// Every tab's left edge and width when the drag started, in order.
    pub slots: Vec<(Id, f32, f32)>,
}
impl TabDrag {
    /// Movement before a press becomes a drag.
    pub const THRESHOLD: f32 = 4.;
    fn from(&self) -> Option<usize> {
        self.slots.iter().position(|(id, _, _)| *id == self.id)
    }
    /// Where the dragged tab would drop, and its left edge, kept within the
    /// tabs' extent.
    pub fn target(&self) -> Option<(usize, f32)> {
        let from = self.from()?;
        let (_, left, width) = self.slots[from];
        let first = self.slots[0].1;
        let (_, last_left, last_width) = *self.slots.last()?;
        let left = (left + self.pointer - self.start).clamp(first, (last_left + last_width - width).max(first));
        // A neighbor moves aside once the dragged tab's leading edge passes
        // its middle.
        let center = |(_, l, w): &(Id, f32, f32)| l + w / 2.;
        let passed_left = self.slots[..from].iter().filter(|s| center(s) > left).count();
        let passed_right = self.slots[from + 1..]
            .iter()
            .filter(|s| center(s) < left + width)
            .count();
        Some((from - passed_left + passed_right, left))
    }
    /// Horizontal offset of each tab from its place: the dragged tab follows
    /// the pointer, and tabs it has passed move over by its width.
    pub fn offsets(&self) -> Vec<(Id, f32)> {
        let (Some(from), Some((target, left))) = (self.from(), self.target()) else {
            return Vec::new();
        };
        let (_, origin, width) = self.slots[from];
        let gap = self
            .slots
            .windows(2)
            .next()
            .map_or(0., |pair| pair[1].1 - (pair[0].1 + pair[0].2));
        let room = width + gap;
        self.slots
            .iter()
            .enumerate()
            .map(|(index, (id, _, _))| {
                let offset = if index == from {
                    left - origin
                } else if from < target && index > from && index <= target {
                    -room
                } else if target < from && index >= target && index < from {
                    room
                } else {
                    0.
                };
                (*id, offset)
            })
            .collect()
    }
    /// Where the dragged tab's left edge lands once it is dropped.
    pub fn landing(&self) -> Option<f32> {
        let from = self.from()?;
        let (target, _) = self.target()?;
        let width = self.slots[from].2;
        Some(if target <= from {
            self.slots[target].1
        } else {
            let (_, left, w) = self.slots[target];
            left + w - width
        })
    }
}
/// Move a tab to `index` among the open tabs.
pub(super) fn move_tab(tabs: &mut Vec<Id>, id: Id, index: usize) {
    if let Some(from) = tabs.iter().position(|tab| *tab == id) {
        let tab = tabs.remove(from);
        tabs.insert(index.min(tabs.len()), tab);
    }
}
pub(super) fn close_tab(tabs: &mut Vec<Id>, id: Id) -> Option<Id> {
    let index = tabs.iter().position(|t| *t == id)?;
    tabs.remove(index);
    tabs.get(index).or_else(|| tabs.last()).copied()
}
/// Which open tabs a tab context-menu action closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CloseTabs {
    This,
    Others,
    ToTheRight,
}
/// The tabs closed by `which`, relative to the tab with `id`.
pub(super) fn tabs_to_close(tabs: &[Id], id: Id, which: CloseTabs) -> Vec<Id> {
    let Some(index) = tabs.iter().position(|t| *t == id) else {
        return Vec::new();
    };
    match which {
        CloseTabs::This => vec![id],
        CloseTabs::Others => tabs.iter().copied().filter(|t| *t != id).collect(),
        CloseTabs::ToTheRight => tabs[index + 1..].to_vec(),
    }
}
/// Where a context menu of `size` opens for a press at `at`: below and right
/// of the pointer, flipped above or left of it where it would leave the
/// viewport, and kept inside a small margin.
pub(super) fn context_menu_origin(
    at: (f32, f32),
    size: (f32, f32),
    viewport: (f32, f32),
) -> (f32, f32) {
    const MARGIN: f32 = 8.;
    let place = |at: f32, len: f32, room: f32| {
        let start = if at + len > room - MARGIN && at - len >= MARGIN {
            at - len
        } else {
            at
        };
        start.clamp(MARGIN, (room - len - MARGIN).max(MARGIN))
    };
    (
        place(at.0, size.0, viewport.0),
        place(at.1, size.1, viewport.1),
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocumentMenuAction {
    Close(CloseTabs),
    Open,
    Rename,
    Duplicate,
    Favorite,
    Move,
    Tags,
    Trash,
    Delete,
}
impl NotesView {
    /// Start a possible tab drag from a press on the tab.
    pub(super) fn press_tab(&mut self, id: Id, x: f32, cx: &mut Context<Self>) {
        let bounds = self.tab_bounds.borrow();
        let slots = self
            .open_tabs
            .iter()
            .filter_map(|tab| bounds.get(tab).map(|(left, width)| (*tab, *left, *width)))
            .collect();
        drop(bounds);
        self.tab_drag = Some(TabDrag {
            id,
            start: x,
            pointer: x,
            moving: false,
            slots,
        });
        cx.notify();
    }
    pub(super) fn drag_tab(&mut self, x: f32, button: Option<MouseButton>, cx: &mut Context<Self>) {
        // A release outside the window ends the drag where it was.
        if button != Some(MouseButton::Left) {
            self.drop_tab(cx);
            return;
        }
        let Some(drag) = &mut self.tab_drag else {
            return;
        };
        drag.pointer = x;
        if !drag.moving && (x - drag.start).abs() > TabDrag::THRESHOLD {
            drag.moving = true;
        }
        if drag.moving {
            cx.notify();
        }
    }
    /// Drop a dragged tab into the place it was moved to; it settles into
    /// its slot while its neighbors stay where they already are.
    pub(super) fn drop_tab(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.tab_drag.take() else {
            return;
        };
        let complete = drag.slots.len() == self.open_tabs.len()
            && drag.slots.iter().zip(&self.open_tabs).all(|((id, _, _), tab)| id == tab);
        if drag.moving
            && complete
            && let (Some((target, left)), Some(landing)) = (drag.target(), drag.landing())
        {
            let reduced = self.controller.settings.reduce_motion;
            let mut motion = self.motion.borrow_mut();
            for (id, _) in drag.offsets() {
                motion.jump(&format!("tab-offset-{id}"), 0.);
            }
            motion.slide(&format!("tab-offset-{}", drag.id), left - landing, 0., reduced);
            drop(motion);
            move_tab(&mut self.open_tabs, drag.id, target);
        }
        cx.notify();
    }
    pub(super) fn close_document_tab(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_inline_text(window, cx);
        self.controller.finish();
        let next = close_tab(&mut self.open_tabs, id);
        if self.controller.requested_note() == id {
            self.controller.close_math_solver();
            self.clear_math_inputs(window);
            if let Some(next) = next {
                self.controller.switch_note(next);
            } else {
                self.show_library();
            }
        }
        self.controller.close_session(id);
        self.focus.focus(window);
        cx.notify();
    }
    pub(super) fn cycle_document_tab(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_tabs.is_empty() {
            return;
        }
        self.finish_inline_text(window, cx);
        let index = self
            .open_tabs
            .iter()
            .position(|id| *id == self.controller.requested_note())
            .unwrap_or(0);
        let next = if self.library_open {
            if forward { 0 } else { self.open_tabs.len() - 1 }
        } else if forward {
            (index + 1) % self.open_tabs.len()
        } else {
            (index + self.open_tabs.len() - 1) % self.open_tabs.len()
        };
        self.open_note(self.open_tabs[next]);
        self.focus.focus(window);
        cx.notify();
    }

    pub(super) fn open_document_menu(
        &mut self,
        id: Id,
        at: Point<Pixels>,
        from_tab: bool,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.folder_menu = None;
        self.document_menu = Some((id, at));
        self.document_menu_tab = from_tab;
        self.document_menu_keyboard = keyboard;
        self.focus.focus(window);
        self.menu_focus = Some(
            if from_tab {
                "doc-menu-close"
            } else {
                "doc-menu-open"
            }
            .into(),
        );
        cx.notify();
    }
    /// Keyboard focus moved while the document menu is open: show its ring.
    pub(super) fn keyboard_in_document_menu(&mut self, cx: &mut Context<Self>) {
        if self.document_menu.is_some() && !self.document_menu_keyboard {
            self.document_menu_keyboard = true;
            cx.notify();
        }
    }
    /// Close the tabs a tab-menu action names. When the active document is
    /// among them, the clicked tab becomes active first, so closing does not
    /// load neighbors that are about to close too.
    fn close_tabs(
        &mut self,
        id: Id,
        which: CloseTabs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closing = tabs_to_close(&self.open_tabs, id, which);
        if which != CloseTabs::This && closing.contains(&self.controller.requested_note()) {
            self.open_note(id);
        }
        for tab in closing {
            self.close_document_tab(tab, window, cx);
        }
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
        let from_tab = self.document_menu_tab && self.open_tabs.contains(&id);
        let mut entries: Vec<(&str, &str, Option<Icon>, DocumentMenuAction)> = Vec::new();
        if from_tab {
            // Selecting the tab already opens it; offer the tab's own actions.
            entries.push((
                "close",
                "Close tab",
                Some(Icon::Close),
                DocumentMenuAction::Close(CloseTabs::This),
            ));
            if self.open_tabs.len() > 1 {
                entries.push((
                    "close-others",
                    "Close other tabs",
                    None,
                    DocumentMenuAction::Close(CloseTabs::Others),
                ));
            }
            if self.open_tabs.last() != Some(&id) {
                entries.push((
                    "close-right",
                    "Close tabs to the right",
                    None,
                    DocumentMenuAction::Close(CloseTabs::ToTheRight),
                ));
            }
        } else {
            entries.push((
                "open",
                "Open document",
                Some(Icon::Book),
                DocumentMenuAction::Open,
            ));
        }
        let duplicate = (
            "duplicate",
            "Duplicate",
            Some(Icon::Copy),
            DocumentMenuAction::Duplicate,
        );
        if note.trashed {
            entries.extend([
                duplicate,
                (
                    "trash",
                    "Restore document",
                    Some(Icon::Undo),
                    DocumentMenuAction::Trash,
                ),
                (
                    "delete",
                    "Delete permanently…",
                    Some(Icon::Trash),
                    DocumentMenuAction::Delete,
                ),
            ]);
        } else {
            entries.extend([
                (
                    "rename",
                    "Rename…",
                    Some(Icon::Pencil),
                    DocumentMenuAction::Rename,
                ),
                duplicate,
                if note.favorite {
                    (
                        "favorite",
                        "Remove from favorites",
                        Some(Icon::StarFilled),
                        DocumentMenuAction::Favorite,
                    )
                } else {
                    (
                        "favorite",
                        "Add to favorites",
                        Some(Icon::Star),
                        DocumentMenuAction::Favorite,
                    )
                },
                (
                    "move",
                    "Move to folder…",
                    Some(Icon::Folder),
                    DocumentMenuAction::Move,
                ),
                (
                    "tags",
                    "Edit tags…",
                    Some(Icon::Tag),
                    DocumentMenuAction::Tags,
                ),
                (
                    "trash",
                    "Move to trash",
                    Some(Icon::Trash),
                    DocumentMenuAction::Trash,
                ),
            ]);
        }
        // Separate tab actions from document actions, and the destructive
        // action from everyday ones.
        let separated = |action: DocumentMenuAction, previous: Option<DocumentMenuAction>| {
            matches!(previous, Some(DocumentMenuAction::Close(_)))
                && !matches!(action, DocumentMenuAction::Close(_))
                || (action == DocumentMenuAction::Trash && !note.trashed)
                || action == DocumentMenuAction::Delete
        };
        let separators = entries
            .iter()
            .enumerate()
            .filter(|(i, entry)| separated(entry.3, i.checked_sub(1).map(|i| entries[i].3)))
            .count();
        // Panel padding and edge, 2rem rows and separators with their margins,
        // following the interface scale.
        let scale = self.controller.settings.ui_scale;
        let size = window.viewport_size();
        let (width, height) = (f32::from(size.width), f32::from(size.height));
        let menu_width = (224. * scale).min((width - 16.).max(1.));
        let natural = (10. + entries.len() as f32 * 32. + separators as f32 * 9.) * scale;
        let menu_height = natural.min((height - 16.).max(1.));
        let (x, y) = context_menu_origin(
            (f32::from(at.x), f32::from(at.y)),
            (menu_width, menu_height),
            (width, height),
        );
        let mut menu = div()
            .id("document-context-menu")
            .occlude()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(menu_width))
            .max_h(px(menu_height))
            .overflow_y_scroll()
            .p_1()
            .flex()
            .flex_col()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .shadow_md()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                // Dismissing the menu must not also activate the tab, card or
                // canvas under the pointer.
                this.document_menu = None;
                cx.stop_propagation();
                cx.notify();
            }));
        let mut previous = None;
        for (key, label, kind, action) in entries {
            if separated(action, previous) {
                menu = menu.child(
                    div()
                        .flex_shrink_0()
                        .h(px(1.))
                        .mx_2()
                        .my_1()
                        .bg(theme.border),
                );
            }
            previous = Some(action);
            // Destructive actions are marked by a red icon; their text keeps
            // the menu's ink.
            let destructive = matches!(action, DocumentMenuAction::Delete)
                || (action == DocumentMenuAction::Trash && !note.trashed);
            let glyph = match kind {
                Some(kind) => icon(
                    kind,
                    if destructive {
                        theme.destructive
                    } else {
                        theme.muted
                    },
                )
                .size(rems(1.125))
                .into_any_element(),
                None => div().size(rems(1.125)).flex_shrink_0().into_any_element(),
            };
            menu = menu.child(
                self.control(
                    format!("doc-menu-{key}"),
                    label,
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(glyph)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(theme.ink))
                                .child(label),
                        )
                        .into_any_element(),
                    false,
                    cx,
                    move |this, w, cx| {
                        this.document_menu = None;
                        match action {
                            DocumentMenuAction::Close(which) => this.close_tabs(id, which, w, cx),
                            DocumentMenuAction::Open => this.open_note(id),
                            DocumentMenuAction::Rename => {
                                this.modal(Modal::RenameDocument(id), w, cx)
                            }
                            DocumentMenuAction::Duplicate => this
                                .controller
                                .manage_note(id, folio_app::NoteAction::Duplicate),
                            DocumentMenuAction::Favorite => this
                                .controller
                                .manage_note(id, folio_app::NoteAction::Favorite),
                            DocumentMenuAction::Move => this.modal(Modal::MoveDocument(id), w, cx),
                            DocumentMenuAction::Tags => this.modal(Modal::DocumentTags(id), w, cx),
                            DocumentMenuAction::Delete => this.modal(
                                Modal::Confirm(super::Confirm::DeletePermanently(id)),
                                w,
                                cx,
                            ),
                            DocumentMenuAction::Trash => {
                                this.controller
                                    .manage_note(id, folio_app::NoteAction::Trash);
                                if !note.trashed {
                                    this.open_tabs.retain(|tab| *tab != id);
                                }
                            }
                        }
                    },
                )
                .flex_shrink_0()
                .w_full()
                .justify_start()
                .min_h(rems(2.))
                .px_2()
                .py_1(),
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
    use super::{CloseTabs, TabDrag, close_tab, context_menu_origin, move_tab, tabs_to_close};
    use core::prelude::v1::test;
    use folio_document::Id;
    #[test]
    fn closing_tabs_chooses_the_right_then_left_neighbor() {
        let [a, b, c] = [Id::new_v4(), Id::new_v4(), Id::new_v4()];
        let mut tabs = vec![a, b, c];
        assert_eq!(close_tab(&mut tabs, b), Some(c));
        assert_eq!(close_tab(&mut tabs, c), Some(a));
        assert_eq!(close_tab(&mut tabs, a), None);
        assert!(tabs.is_empty());
        let tabs = [a, b, c];
        assert_eq!(tabs_to_close(&tabs, b, CloseTabs::This), vec![b]);
        assert_eq!(tabs_to_close(&tabs, b, CloseTabs::Others), vec![a, c]);
        assert_eq!(tabs_to_close(&tabs, a, CloseTabs::ToTheRight), vec![b, c]);
        assert!(tabs_to_close(&tabs, c, CloseTabs::ToTheRight).is_empty());
        assert!(tabs_to_close(&tabs, Id::new_v4(), CloseTabs::Others).is_empty());
        // The tab menu opens at the pointer, flips above or left of it near
        // the viewport's edges and stays inside a tiny window.
        let (menu, viewport) = ((224., 300.), (1000., 800.));
        assert_eq!(
            context_menu_origin((100., 40.), menu, viewport),
            (100., 40.)
        );
        assert_eq!(
            context_menu_origin((900., 700.), menu, viewport),
            (676., 400.)
        );
        assert_eq!(
            context_menu_origin((150., 600.), menu, viewport),
            (150., 300.)
        );
        assert_eq!(
            context_menu_origin((100., 200.), menu, (400., 350.)),
            (100., 42.)
        );
    }
    #[test]
    fn dragged_tabs_follow_the_pointer_and_neighbors_make_room() {
        let [a, b, c] = [Id::new_v4(), Id::new_v4(), Id::new_v4()];
        // Tabs 100, 60 and 80 wide, 4 apart.
        let mut drag = TabDrag {
            id: a,
            start: 50.,
            pointer: 50.,
            moving: true,
            slots: vec![(a, 0., 100.), (b, 104., 60.), (c, 168., 80.)],
        };
        assert_eq!(drag.target(), Some((0, 0.)));
        assert_eq!(drag.offsets(), vec![(a, 0.), (b, 0.), (c, 0.)]);
        // Past the middle of b: b moves left by a's width and the gap.
        drag.pointer = 140.;
        assert_eq!(drag.target(), Some((1, 90.)));
        assert_eq!(drag.offsets(), vec![(a, 90.), (b, -104.), (c, 0.)]);
        assert_eq!(drag.landing(), Some(64.));
        // Far beyond the end, the tab stops at the last edge.
        drag.pointer = 900.;
        assert_eq!(drag.target(), Some((2, 148.)));
        assert_eq!(drag.landing(), Some(148.));
        let mut tabs = vec![a, b, c];
        move_tab(&mut tabs, a, 2);
        assert_eq!(tabs, vec![b, c, a]);
        // Dragging the last tab left pushes the others right.
        let drag = TabDrag {
            id: a,
            start: 200.,
            pointer: 20.,
            moving: true,
            slots: vec![(b, 0., 60.), (c, 64., 80.), (a, 148., 100.)],
        };
        assert_eq!(drag.target(), Some((0, 0.)));
        assert_eq!(drag.offsets(), vec![(b, 104.), (c, 104.), (a, -148.)]);
        move_tab(&mut tabs, a, 0);
        assert_eq!(tabs, vec![a, b, c]);
        move_tab(&mut tabs, Id::new_v4(), 1);
        assert_eq!(tabs, vec![a, b, c]);
    }
}
