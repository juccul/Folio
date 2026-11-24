//! Library, document chrome and page navigation. All mutations go through Controller.
use super::thumbnails::ThumbnailSource;
use super::*;

/// Horizontal indentation per nested folder level and vertical rail offset.
const FOLDER_INDENT: f32 = 22.;
const FOLDER_RAIL_X: f32 = 14.;

/// For each visible folder row `(notebook, depth)`, returns a bitmask whose
/// bit `level` is set when ancestor level `level` has another sibling below
/// this row (so its vertical guide rail continues through this slot).
fn folder_tree_guides<T>(rows: &[(T, usize)]) -> Vec<u64> {
    let mut guides = vec![0u64; rows.len()];
    let mut open = 0u64;
    for i in (0..rows.len()).rev() {
        let d = rows[i].1;
        if d < 64 {
            open &= (1u64 << d).wrapping_sub(1);
        }
        guides[i] = open;
        if d > 0 && d - 1 < 64 {
            open |= 1u64 << (d - 1);
        }
    }
    guides
}

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
    /// The height, in window pixels, the library's grid draws its covers
    /// at for this window: what other views that show a document's cover
    /// beside the shelf ask for, so the two share one rendered image.
    pub(super) fn library_cover_height(&self, window: &Window) -> f32 {
        let scale = self.controller.settings.ui_scale;
        let width = f32::from(window.viewport_size().width) / scale;
        LibraryGrid::new(width, width < 900.).cover_height() * scale
    }
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
}

#[derive(Clone)]
struct PageDrag(Id);
impl DragView for PageDrag {
    fn preview(&self) -> Div {
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
            button.tooltip(hint.clone())
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
        let glyph = icon(kind, theme.ink).size(rems(1.375));
        // The favorite star pops as it fills.
        let glyph = if id == "favorite-note" {
            crate::icons::toggle_glyph("star", active, glyph).into_any_element()
        } else {
            glyph.into_any_element()
        };
        self.control(
            id,
            label,
            glyph,
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
        .bg(theme.tint(if active { 1. } else { 0. }))
        .when(self.toolbar_hints_enabled(), |button| button.tooltip(hint))
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
        let radius = rems(theme.radius / 16.);
        // An options chevron turns over while its menu is open.
        let glyph = div()
            .flex()
            .child(icon(kind, color).size(rems(size)))
            .when(chevron, |g| g.flipped(active));
        self.control(id, label, glyph.into_any_element(), active, cx, action)
            .w(rems(2.5))
            .h(rems(2.375))
            .min_h_0()
            .flex_shrink_0()
            .p_0()
            .rounded(radius)
            .bg(theme.tint(0.))
            .selection_member(current)
            // The active tool: a firm neutral tint with a full-strength icon.
            // It is one highlight, which glides to the tool chosen next. Every
            // tool keeps the slot it sits in, so the icons never change place.
            .underlay(div().absolute().inset_0().when(current, |slot| {
                slot.child(
                    div()
                        .id("current-tool")
                        .absolute()
                        .inset_0()
                        .rounded(radius)
                        .bg(rgba((theme.ink << 8) | if theme.dark { 0x38 } else { 0x24 }))
                        .el(|e| {
                            e.matched("current-tool")
                                .motion(crate::motion::GLIDE)
                                .animated_changes()
                        }),
                )
            }))
            .when(self.toolbar_hints_enabled(), |button| button.tooltip(hint))
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
            .bg(theme.tint(if active { 1. } else { 0. }))
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
        let arriving = self.library_arriving;
        let id = n.id;
        let path = details.path.clone();
        let branch = details.branch;
        let active = matches!(self.controller.filter, NoteFilter::Notebook(folder) | NoteFilter::NotebookTrash(folder) if folder == id);
        let gliding = matches!(self.controller.filter, NoteFilter::Notebook(folder) if folder == id);
        let name = n.name.clone();
        // A dragged folder lifts and follows the pointer like a tab, its
        // indent showing the parent it would land in; the other rows move
        // aside, and the folder it would nest in is highlighted.
        let dragging = self.folder_drag.as_ref().filter(|drag| drag.moving);
        let shift = dragging.map_or(0., |drag| drag.offset(id));
        let lifted = dragging.is_some_and(|drag| drag.lifts(id));
        let nesting = dragging.is_some_and(|drag| drag.nesting() == Some(id));
        let depth = dragging.map_or(depth, |drag| {
            let valid = drag.destination().is_some_and(|destination| {
                super::folder_menu::accepts_move(
                    &self.controller,
                    super::folder_menu::LibraryItem::Folder(drag.id),
                    destination,
                )
            });
            drag.depth(id, depth, valid)
        });
        let flying = self.folder_rows_fly();
        let visual_depth = depth.min(5);
        let nested = visual_depth > 0;
        let folder_icon = if nested && !branch {
            icon(Icon::Folder, theme.muted).size(rems(0.95))
        } else {
            icon(Icon::Folder, if active { theme.ink } else { theme.muted })
        };
        let content = div()
            .flex()
            .min_w_0()
            .items_center()
            .when(nested, |d| d.gap_2())
            .when(!nested, |d| d.gap_3())
            .child(folder_icon)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .when(!nested || branch, |d| d.font_weight(FontWeight::MEDIUM))
                    .when(nested && !branch && !active, |d| {
                        d.text_color(rgb(theme.secondary()))
                    })
                    .child(name),
            );
        let open = self
            .control(
                format!("folder-{id}"),
                path.clone(),
                content.into_any_element(),
                active,
                cx,
                move |this, _, _| {
                    // Releasing a dragged folder drops it; it does not open it.
                    if !this.folder_just_dropped() {
                        this.controller.filter = NoteFilter::Notebook(id);
                    }
                },
            )
            .flex_1()
            .min_w_0()
            .h_full()
            .when(nested, |d| d.px_2())
            .justify_start()
            .rounded_none()
            .bg(transparent_black())
            // The row draws the hover for all of it.
            .tint_from_parent()
            .on_pointer_drag(cx.listener(move |this, event: &DragEvent, _, cx| {
                let y = f32::from(event.position.y);
                match event.phase {
                    DragPhase::Down => this.press_folder(id, y),
                    DragPhase::Move => this.drag_folder(y, cx),
                    DragPhase::Up => this.drop_folder(event.cancelled, cx),
                }
            }));
        let row = div()
            .id(SharedString::from(format!("folder-row-{id}")))
            .flex()
            .flex_shrink_0()
            .h(rems(2.75))
            .items_center()
            .ml(px(visual_depth as f32 * FOLDER_INDENT))
            .rounded(px(theme.radius))
            .relative()
            // In a folder's Trash the Trash filter carries the highlight; the
            // folder keeps a plain tint so the two never claim it at once.
            .when(active && !gliding, move |row| row.bg(theme.tint(1.)))
            .selection_member(active)
            // The chosen folder shares the library filters' highlight, which
            // glides from Documents to the folder and back.
            .underlay(div().absolute().inset_0().when(gliding, move |slot| {
                slot.child(
                    div()
                        .id("glide-45")
                        .absolute()
                        .inset_0()
                        .rounded(px(theme.radius))
                        .bg(theme.tint(1.))
                        .el(|e| {
                            e.matched("glide-45")
                                .motion(crate::motion::GLIDE)
                                .animated_changes()
                        }),
                )
            }))
            .hover(move |s| s.bg(theme.tint(1.)))
            // Revealed subfolders slide down into place; coming back from a
            // document, the rows are simply there with the rest of the library.
            .el(move |e| {
                if arriving {
                    return e;
                }
                e.transition(rune::Transition::offset(0., -8.).combined(rune::Transition::opacity()))
                    .motion(crate::motion::SETTLE)
                    .animated_changes()
            })
            .when(nesting, move |row| {
                row.bg(theme.tint(1.))
                    .border_1()
                    .border_color(rgb(theme.sidebar_accent))
            })
            .when(lifted, move |row| row.glass(theme, theme.popover))
            // Moved in layout, not just drawn aside, so a row that changes
            // list slots on the drop flies from where it was seen.
            .mt(px(shift))
            .el(move |e| {
                let mut e = e;
                if lifted {
                    // Raised in place, so it keeps the press it is dragged
                    // by, and it follows the pointer at once.
                    e = e.raise().motion(rune::Spring::NONE);
                }
                // Rows change list slots when a drop reorders the tree; they
                // fly from where they were drawn to their new places.
                if flying {
                    e = e.matched(format!("folder-row-{id}"));
                }
                e
            })
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
                    // One arrow that turns as the branch opens.
                    div()
                        .flex()
                        .child(icon(Icon::Forward, theme.muted).size(rems(0.875)))
                        .quarter_turn(!collapsed)
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
                .min_h(px(24.))
                .p_0()
                .bg(transparent_black())
                .hover(move |s| s.bg(theme.tint(1.)))
                .into_any_element()
            } else {
                div().into_any_element()
            })
            .child(
                self.control(
                    format!("manage-folder-{id}"),
                    format!("Manage folder {path}"),
                    icon(Icon::More, theme.secondary()).into_any_element(),
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
        let arriving = self.library_arriving;
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
                    self.folder_drop_target(s, None, cx)
                        .tooltip("Drop here to remove from its folder")
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
            .px_3()
            // Rows below an opening or closing folder glide aside. Coming
            // back from a document, the list is simply there.
            .el(move |e| {
                if arriving {
                    e
                } else {
                    e.motion(crate::motion::SETTLE).animated_changes()
                }
            });
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
        let guides = folder_tree_guides(&rows);
        self.folder_rows = rows.iter().map(|(folder, _)| folder.id).collect();
        if !rows.is_empty() {
            folders = folders.child(
                uniform_list(
                    "folder-rows",
                    rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                        let theme = Theme::new(&this.controller.settings);
                        let dragging = this.folder_drag.as_ref().is_some_and(|drag| drag.moving);
                        let visible = &rows[range.clone()];
                        let visible_guides = &guides[range];
                        let details = this
                            .controller
                            .folder_details(visible.iter().map(|(folder, _)| folder.id));
                        let rail_color = if theme.dark {
                            rgba(0xffffff26)
                        } else {
                            rgba(0x00000024)
                        };
                        let rendered: Vec<_> = visible
                            .iter()
                            .zip(visible_guides)
                            .map(|((folder, depth), &guide_mask)| {
                                let mut slot = div().relative().h(rems(3.));
                                let visual_depth = (*depth).min(5);
                                if visual_depth > 0 && !dragging {
                                    for level in 0..visual_depth {
                                        let x = level as f32 * FOLDER_INDENT + FOLDER_RAIL_X;
                                        let continues = (guide_mask & (1u64 << level)) != 0;
                                        if level + 1 < visual_depth {
                                            if continues {
                                                slot = slot.child(
                                                    div()
                                                        .absolute()
                                                        .left(px(x))
                                                        .top_0()
                                                        .bottom_0()
                                                        .w(px(1.))
                                                        .bg(rail_color),
                                                );
                                            }
                                        } else {
                                            slot = slot.child(
                                                div()
                                                    .absolute()
                                                    .left(px(x))
                                                    .top_0()
                                                    .when(continues, |line| line.bottom_0())
                                                    .when(!continues, |line| line.h(px(23.)))
                                                    .w(px(1.))
                                                    .bg(rail_color),
                                            );
                                            let arm = (visual_depth as f32 * FOLDER_INDENT - x - 2.)
                                                .max(4.);
                                            slot = slot.child(
                                                div()
                                                    .absolute()
                                                    .left(px(x))
                                                    .top(px(22.))
                                                    .w(px(arm))
                                                    .h(px(1.))
                                                    .bg(rail_color),
                                            );
                                        }
                                    }
                                }
                                slot.child(this.folder_row(
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
                        let _ = &cx;
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
            // Nothing scrolls behind the library sidebar, so it needs no blur.
            .glass_fill_at(theme, theme.sidebar, theme.glass_opacity, 0.)
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
                // The same inset as the navigation above, so the icons line up.
                div()
                    .px_3()
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
        first: usize,
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
        for (offset, n) in notes.into_iter().enumerate() {
            let id = n.id;
            // Each document is one moving thing: it glides to its new place
            // when the shelf is sorted, filtered or reflowed (flying between
            // rows, and between the grid and the list), and new ones arrive
            // one after another.
            // Rows the shelf builds as it scrolls are simply there.
            let scrolled = building_scrolled_rows();
            let motion = move |e: rune::El| {
                if scrolled {
                    return e.transition(rune::Transition::none());
                }
                // Cards and rows keep their own flights: a card does not morph
                // into a row, the views cross-fade.
                e.matched(format!("library-{}-{id}", if list_view { "row" } else { "card" }))
                    .motion(crate::motion::SETTLE)
                    .transition(
                        rune::Transition::scale(0.92)
                            .combined(rune::Transition::opacity())
                            .springs(rune::SpringPair::new(crate::motion::POP, crate::motion::SETTLE)),
                    )
                    .enter_delay(((first + offset) as f32 * 0.018).min(0.22))
                    .animated_changes()
            };
            let drag = super::folder_menu::LibraryDrag {
                item: super::folder_menu::LibraryItem::Document(id),
                title: n.title.clone(),
                theme,
            };
            let session_pages = self
                .controller
                .sessions
                .get(&id)
                .map(|s| s.document.pages.len());
            let cover_w = (grid.card - LibraryGrid::INSET * 2.) * self.controller.settings.ui_scale;
            let cover_h = grid.cover_height() * self.controller.settings.ui_scale;
            let cached_cover = (!list_view)
                .then(|| self.cached_cover_thumbnail(&n, cover_w, cover_h, window))
                .flatten();
            let preview = if cached_cover.is_some() {
                None
            } else {
                self.controller.library_preview(id)
            };
            let pages = cached_cover
                .as_ref()
                .map(|(_, count)| *count)
                .or_else(|| preview.as_ref().map(|(_, count)| *count))
                .or(session_pages);
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
                    crate::icons::toggle_glyph(
                        "star",
                        n.favorite,
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
                        .size(rems(1.)),
                    )
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
                    .to_owned();
                    label
                });
            // Quieter than the title, like the outline star beside it: every
            // card carries these, so at full ink they crowded the shelf.
            let menu = self
                .control(
                    format!("manage-note-{id}"),
                    format!("Manage {}", n.title),
                    icon(Icon::More, theme.secondary()).into_any_element(),
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
                .p_0()
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
                            .when(!import_failed, |column| {
                                column.child(
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
                                )
                            }),
                    );
                items = items.child(
                    div()
                        .id(SharedString::from(format!("note-row-{id}")))
                        .el(motion)
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
                        .when(menu_owner, |row| row.bg(theme.tint(0.7)))
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
                            .when(!n.trashed, |s| s.on_drag(drag, ())),
                        )
                        .child(favorite)
                        .child(menu)
                        // A hairline as wide as the row highlight.
                        .child(div().absolute().bottom_0().left_0().right_0().h(px(1.)).bg(
                            if theme.dark {
                                theme.cover_edge()
                            } else {
                                theme.border
                            },
                        )),
                );
            } else {
                // The outline and shadow hug the page itself, so the page
                // never sits loose inside a frame of another shape.
                let edge = if menu_owner {
                    rgb(theme.muted).into()
                } else {
                    theme.cover_edge()
                };
                let page_corner = (theme.radius - 1.).max(0.);
                let cover_content = if let Some((cover_div, _)) = cached_cover {
                    super::thumbnails::outlined(cover_div.shadow_sm(), page_corner, edge)
                } else if let Some((page, page_count)) = &preview {
                    if let Some(pdf) = &page.properties.pdf {
                        self.controller.request_pdf_background(pdf.clone());
                    }
                    let frame = self.thumbnail(
                        ThumbnailSource::Cover {
                            note: id,
                            updated_at: n.updated_at,
                            cover_page: n.cover_page,
                            page: page.clone(),
                            page_count: *page_count,
                        },
                        cover_w,
                        cover_h,
                        window,
                        cx,
                    );
                    super::thumbnails::outlined(frame.shadow_sm(), page_corner, edge)
                } else {
                    div()
                        .size_full()
                        .rounded(px(theme.radius))
                        .border_1()
                        .border_color(edge)
                        .bg(rgb(theme.sidebar))
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
                let cover = div()
                    .relative()
                    .w_full()
                    .h(rems(grid.cover_height() / 16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(cover_content)
                    // Blank documents get a quiet mark, so the grid does not
                    // read as a wall of identical pages.
                    // A failed import shows how to recover; blank pages stay
                    // plain paper.
                    .when(import_failed, |cover| {
                        cover.child(
                            div()
                                .absolute()
                                .inset_0()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .items_center()
                                .justify_center()
                                .text_sm()
                                .text_color(rgb(theme.error_text()))
                                .child(icon(Icon::Alert, theme.error_text()).size(rems(1.75)))
                                .child("Open to retry"),
                        )
                    });
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
                        .el(motion)
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
                            .when(!n.trashed, |s| s.on_drag(drag, ())),
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
                                            "Import failed".to_owned()
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
        // The search bar filters the shelf by title as you type.
        let filter = self.library_filter(cx);
        let mut notes = self
            .controller
            .visible_notes()
            .into_iter()
            .filter(|n| filter.as_ref().is_none_or(|words| super::find::title_matches(&n.title, words)))
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
        // Only rows near the view are built: a large library would otherwise
        // build every card on every frame. The list scrolls itself.
        let row_height = if list_view { 3.75 * REM } else { grid.row_height() };
        let list = virtual_list(
            "library-rows",
            total.div_ceil(columns),
            row_height,
            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                let mut rows = Vec::new();
                let visible_start = range.start * columns;
                let visible_end = (range.end * columns).min(total);
                for row in range {
                    let start = row * columns;
                    let end = (start + columns).min(total);
                    rows.push(
                        this.library_items(notes[start..end].to_vec(), start, list_view, grid, window, cx)
                            // Rows belong to one view, so switching views
                            // replaces them whole instead of reflowing them.
                            .id(format!("library-{}-{row}", if list_view { "list" } else { "grid" }))
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
                let _ = &cx;
                this.accessibility.publish(this, window, cx);
                rows
            }),
        )
        .flex_1()
        .min_h_0()
        // The shelf fades in under its cards whenever it appears, coming back
        // from a document as when a filter replaces an empty folder.
        .el(|e| e.animated_changes());
        if count == 0 {
            let filtered = filter.is_some();
            let (heading, hint, kind) = match self.controller.filter {
                _ if filtered => (
                    "No matching documents",
                    "No document title here contains what you typed.",
                    Icon::Search,
                ),
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
                NoteFilter::Notebook(_) => ("Empty folder", "", Icon::Folder),
                NoteFilter::All => (
                    "No documents yet",
                    "Create a document, or import a PDF or image.",
                    Icon::Book,
                ),
            };
            let creates = !filtered && self.controller.filter == NoteFilter::All;
            // An empty folder shows only its icon, bare, and its name.
            let plain = matches!(self.controller.filter, NoteFilter::Notebook(_)) && !filtered;
            shelf = shelf.child(
                div()
                    .id(format!("library-empty-{heading}"))
                    .swap_in()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        div()
                            .size(rems(if plain { 2.5 } else { 4. }))
                            .when(!plain, |tile| {
                                tile.rounded(px(theme.corner_lg()))
                                    .bg(theme.raised())
                                    .border_1()
                                    .border_color(theme.glass_edge())
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(kind, theme.muted).size(rems(if plain { 2.25 } else { 1.75 }))),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(heading),
                    )
                    .when(!hint.is_empty(), |empty| {
                        empty.child(div().text_sm().text_color(rgb(theme.muted)).child(hint))
                    })
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
            row = row.child(icon(Icon::Back, theme.secondary()).size(rems(0.75)));
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
                        .text_size(rems(0.8125))
                        .text_color(rgb(theme.secondary()))
                        .py_0()
                        .px_1()
                        .min_h_0()
                        .h(rems(1.25))
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
            .child(
                div()
                    // Room for the folder breadcrumb in every view, so the
                    // shelf does not shift when a folder opens; otherwise a
                    // compact bar.
                    .min_h(rems(4.5))
                    .pt_3()
                    .pb_1()
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
                                    .left(rems(-0.25))
                                    .bottom_full()
                                    .mb_1()
                                    .h(rems(1.25))
                                    .flex()
                                    .items_center()
                                    .child(crumbs)
                            }))
                            .child(
                                div()
                                    .text_size(rems(1.25))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .id("library-actions")
                            .flex()
                            .items_center()
                            .gap_3()
                            // Springs as the search button widens, so the
                            // two move as one and the button's right edge
                            // stays beside Import.
                            .el(|e| e.motion(crate::motion::SETTLE).animated_changes())
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
                            .child({
                                let button = self.icon_button(
                                    "search",
                                    "Filter documents · Ctrl+F",
                                    Icon::Search,
                                    false,
                                    cx,
                                    |this, w, cx| this.open_find(w, cx),
                                );
                                self.search_slot(button, 2.375, 20., true, cx)
                            })
                            // Import is just its icon; hovering names it.
                            .child(
                                self.icon_button(
                                    "library-import",
                                    "Import PDF or image",
                                    Icon::Import,
                                    false,
                                    cx,
                                    |this, _, cx| this.import(cx),
                                )
                                .tooltip("Import PDF or image…"),
                            )
                            .child(
                                self.control(
                                    "library-new",
                                    "New document",
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1p5()
                                        // At 15 px the plus's strokes fall on
                                        // pixel centres and draw crisp.
                                        .child(
                                            icon(Icon::Plus, theme.primary_foreground)
                                                .size(px(15.)),
                                        )
                                        .child("New document")
                                        .into_any_element(),
                                    true,
                                    cx,
                                    |this, w, cx| this.modal(Modal::NewDocument, w, cx),
                                )
                                // A compact button, as tall as the icons beside it.
                                .min_h(rems(2.))
                                .h(rems(2.))
                                .px_3()
                                .py_0()
                                // Semibold at 13 px keeps the small label
                                // solid on its light fill.
                                .text_size(rems(0.8125))
                                .font_weight(FontWeight::SEMIBOLD)
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
                self.tab_scroll.scroll_to(index);
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
            .gap_1()
            // The strip widens and narrows with its tabs.
            .el(|e| e.motion(crate::motion::SETTLE).animated_changes());
        // The selected tab's tint is one highlight that glides to the tab
        // chosen next, Library included. Every tab keeps its slot.
        let glow = move |selected: bool| {
            div().absolute().inset_0().when(selected, move |slot| {
                slot.child(
                    div()
                        .id("active-tab")
                        .absolute()
                        .inset_0()
                        .rounded(rems(theme.radius / 16.))
                        .bg(theme.tint(1.))
                        .el(|e| {
                            e.matched("active-tab")
                                .motion(crate::motion::GLIDE)
                                .animated_changes()
                        }),
                )
            })
        };
        // Home stays pinned at the start of the bar while the tabs scroll.
        let home = div()
            .flex_shrink_0()
            .relative()
            .h_full()
            .flex()
            .items_center()
            // Let the button register its click before stopping the press at
            // this wrapper. Windows' native move loop otherwise steals release.
            .occlude()
            .occlude()
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
                .bg(theme.tint(0.))
                .selection_member(self.library_open)
                .underlay(glow(self.library_open))
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
        for id in self.open_tabs.clone() {
            let Some(n) = self.controller.notes.iter().find(|n| n.id == id) else {
                continue;
            };
            let selected = !self.library_open && id == active;
            let unsaved = self.controller.has_unsaved_changes(id);
            // Rune springs the others aside; the dragged tab follows the
            // pointer at once.
            let offset = targets.get(&id).copied().unwrap_or(0.);
            let dragging_this = dragged == Some(id);
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
                .bg(theme.tint(0.))
                .selection_member(selected)
                .underlay(glow(selected))
                // New tabs grow in from their leading edge; closed ones fade
                // while the rest close the gap.
                .el(move |e| {
                    let e = e.transition(
                        rune::Transition::scale_anchor(0.6, rune::Vec2::new(0., 0.5))
                            .combined(rune::Transition::opacity())
                            .allow_small_scale()
                            // A closed tab goes at once and the others
                            // glide into its place.
                            .springs(rune::SpringPair::new(
                                crate::motion::POP,
                                rune::Spring::NONE,
                            )),
                    );
                    if dragging_this {
                        e.motion(rune::Spring::NONE)
                    } else {
                        e.motion(crate::motion::SETTLE).animated_changes()
                    }
                })
                .child(
                    canvas(
                        move |layout, _, _| {
                            if offset == 0. {
                                bounds.borrow_mut().insert(
                                    id,
                                    (f32::from(layout.origin.x), f32::from(layout.size.width)),
                                );
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                // The outer tab consumes presses after its child controls
                // have registered clicks and tab drags.
                .occlude()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                        this.open_document_menu(id, event.position, true, false, w, cx);
                        cx.stop_propagation();
                    }),
                )
                // Middle-click closes a tab, as in browsers and editors.
                .occlude()
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
                    .on_double_click(cx.listener(move |this, _, w, cx| {
                        this.modal(Modal::RenameDocument(id), w, cx)
                    }))
                    .on_pointer_drag(cx.listener(move |this, event: &DragEvent, _, cx| {
                        match event.phase {
                            DragPhase::Down => this.press_tab(id, f32::from(event.position.x), cx),
                            DragPhase::Move => this.drag_tab(
                                f32::from(event.position.x),
                                Some(MouseButton::Left),
                                cx,
                            ),
                            DragPhase::Up => this.drop_tab(cx),
                        }
                    }))
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
                    .tint_from_parent(),
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
                // Raised in place, so the element keeps the press it is dragged by.
                tabs.child(surface.glass(theme, theme.popover).el(|e| e.raise()))
            } else {
                tabs.child(surface)
            };
        }
        let leading = self.chrome_leading;
        div()
            .h(rems(2.5))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .when(leading > 0., |row| row.pl(px(leading)))
            .border_b_1()
            .border_color(theme.border)
            .child(home)
            .child(tabs)
            .child(
                div()
                    .id("new-tab-slot")
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .occlude()
                    .occlude()
                    .el(|e| e.motion(crate::motion::SETTLE).animated_changes())
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
            // The tour, filling the window, shows the controls itself.
            .when(self.tour.is_none(), |strip| {
                strip.child(
                    self.window_controls(window, cx)
                        .flex_shrink_0()
                        .occlude()
                        .occlude(),
                )
            })
    }
    /// The footer's save mark: a check once everything is saved, a clock
    /// while saving or busy, and a dot for unsaved or failed saves.
    fn save_state_icon(
        &self,
        theme: Theme,
        state: folio_app::SaveState,
        settled: bool,
    ) -> AnyElement {
        match state {
            folio_app::SaveState::Saved if settled => icon(Icon::Check, theme.muted).into_any_element(),
            folio_app::SaveState::Saved | folio_app::SaveState::Saving => {
                icon(Icon::Clock, theme.muted).into_any_element()
            }
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
        }
    }
    fn page_navigation(&self, cx: &mut Context<Self>) -> Div {
        self.page_controls_group().child(self.page_control_button(
            "toggle-pages",
            "Page thumbnails · Ctrl+Shift+P",
            Icon::Sidebar,
            self.pages_open,
            cx,
            |this, _, _| this.pages_open = !this.pages_open,
        ))
    }
    /// The page actions, led by the search button beside the favorite star.
    /// The search bar widens to the left; the group springs with it, so
    /// the bar's right edge and the buttons after it stay put.
    fn page_actions(&mut self, cx: &mut Context<Self>) -> Div {
        let favorite = self.controller.session().document.metadata.favorite;
        let search = self.page_control_button(
            "search",
            "Search · Ctrl+F",
            Icon::Search,
            false,
            cx,
            |this, w, cx| this.open_find(w, cx),
        );
        let here = !self.library_open;
        // Narrower than the library's, so the writing tools keep their room.
        let search = self.search_slot(search, 2.5, 16., here, cx);
        self.page_controls_group()
            .id("page-actions")
            .el(|e| e.motion(crate::motion::SETTLE).animated_changes())
            .child(search)
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
                .bg(Theme::new(&self.controller.settings).tint(0.)),
            )
            .child(self.page_control_button(
                "header-add-page",
                "Add page",
                Icon::NewPage,
                false,
                cx,
                |this, _, _| this.add_page(),
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
                button.tooltip(hint.clone())
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
        let group = |id: &'static str, current: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .rounded(rems(theme.radius / 16.))
                .gap(rems(0.0625))
                // The tool and its options arrow highlight together.
                .when(!current, |group| {
                    group.hover(move |s| s.bg(theme.tint(0.6)))
                })
                .selection_member(current)
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
                    .w(rems(1.5)),
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
                    .w(rems(1.5)),
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
            .child(div().flex().child(icon(Icon::Down, theme.muted).size(rems(0.875))).flipped(
                self.tool_menu.is_some_and(|(menu, _)| menu == ToolMenu::Insert),
            ));
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
                .child(div().flex().child(icon(Icon::Down, theme.muted).size(rems(0.875))).flipped(
                    self.tool_menu.is_some_and(|(menu, _)| menu == ToolMenu::Width),
                ));
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
                .child(div().flex().child(icon(Icon::Down, theme.muted).size(rems(0.875))).flipped(
                    self.tool_menu.is_some_and(|(menu, _)| menu == ToolMenu::Color),
                ));
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
            // The tools slide aside as the search button widens into its
            // bar; changes inside them still land at once.
            .child(
                div()
                    .id("editing-tools-slide")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .el(|e| e.motion(crate::motion::SETTLE).animated_changes())
                    .child(editing.el(|e| e.instant_changes())),
            )
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
                    // The current page's ring is one ring that flies to the
                    // page chosen next. Other pages draw their hover and focus
                    // ring in the same slot, so a page's hover ring goes the
                    // moment it is chosen rather than fading beside the
                    // arriving one.
                    let ring_radius = px(theme.corner_sm() + 5.);
                    let page_view = div()
                        .relative()
                        .p(px(3.))
                        .rounded(ring_radius)
                        .border_2()
                        .border_color(transparent_black())
                        .child(super::thumbnails::outlined(
                            thumbnail.rounded(px(theme.corner_sm())).shadow_sm(),
                            theme.corner_sm(),
                            theme.cover_edge(),
                        ))
                        .child(div().absolute().top(px(-2.)).left(px(-2.)).right(px(-2.)).bottom(px(-2.)).when(!active, move |slot| {
                            slot.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .rounded(ring_radius)
                                    .border_2()
                                    .border_color(ring),
                            )
                        }).when(active, move |slot| {
                            slot.child(
                                div()
                                    .id("active-page")
                                    .absolute()
                                    .inset_0()
                                    .rounded(ring_radius)
                                    .border_2()
                                    .border_color(ring)
                                    .el(|e| {
                                        e.matched("active-page")
                                            .motion(crate::motion::GLIDE)
                                            .animated_changes()
                                    }),
                            )
                        }));
                    // The current page's number is a filled pill under its ring,
                    // the clearest mark of where you are.
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
                        .when(active, |n| {
                            n.bg(rgb(theme.selection()))
                                .text_color(rgb(theme.primary_foreground))
                        })
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
                                    move |this, _, _| this.go_to_page(index),
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
                                .on_drag(PageDrag(page_id), ())
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
                let _ = &cx;
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
            .glass_fill(self.chrome_theme(), theme.sidebar)
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
                            .items_end()
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
                        // A quiet row under the divider, not a framed button
                        // inside a framed bar.
                        self.control(
                            "add-page",
                            "Add page",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(Icon::NewPage, theme.muted).size(rems(1.)))
                                .child("Add page")
                                .into_any_element(),
                            false,
                            cx,
                            |this, _, _| this.add_page(),
                        )
                        .w_full()
                        .text_xs()
                        .text_color(rgb(theme.secondary())),
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
            .glass_fill_from_edge(self.chrome_theme(), theme.surface, false)
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
                        this.go_to_page(p.saturating_sub(1));
                    },
                )
                .size(rems(1.75)),
            )
            .child(
                self.control(
                    "page-count",
                    format!("{} / {}", page + 1, total),
                    rolling_text("page-count", &format!("{} / {}", page + 1, total), page as f64)
                        .into_any_element(),
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
                        this.go_to_page(p + 1);
                    },
                )
                .size(rems(1.75)),
            )
            .child(div().flex_1())
            .child({
                // The status icon and text swap with a soft blur.
                let state = self.controller.save_state();
                let settled = state == folio_app::SaveState::Saved
                    && self.controller.busy == 0
                    && !self.controller.recognition_pending;
                let status = self.controller.activity_status();
                let swap = |e: rune::El| {
                    e.transition(rune::Transition::blur_replace()).animated_changes()
                };
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id(format!("save-icon-{state:?}-{settled}"))
                            .flex()
                            .el(swap)
                            .child(self.save_state_icon(theme, state, settled)),
                    )
                    .child(
                        div()
                            .id(format!("save-text-{status}"))
                            .el(swap)
                            .text_sm()
                            .text_color(rgb(theme.secondary()))
                            .child(status),
                    )
            })
            .children(
                self.library_undo_button(cx)
                    .map(|undo| undo.text_xs().text_color(rgb(theme.muted))),
            )
            .child(div().flex_1())
            .child(
                self.button("zoom-out", "−", false, cx, |this, _, _| {
                    this.zoom(1. / 1.2)
                })
                .text_lg()
                .py_0()
                .tooltip("Zoom out · Ctrl+−"),
            )
            .child(
                self.control(
                    "fit-page",
                    format!("{:.0}%", zoom * 100.),
                    // The zoom a glide is heading for, so each step rolls
                    // once; a pinch changes it every frame and just shows it.
                    if self.pinching {
                        div().child(format!("{:.0}%", zoom * 100.)).into_any_element()
                    } else {
                        let target = self.canvas_ease.target.map_or(zoom, |t| t.zoom);
                        rolling_text("zoom-level", &format!("{:.0}%", target * 100.), target as f64)
                            .into_any_element()
                    },
                    false,
                    cx,
                    |this, _, _| this.fit_eased(),
                )
                .text_sm()
                .py_1()
                .tooltip("Fit page · Ctrl+0"),
            )
            .child(
                self.button("zoom-in", "+", false, cx, |this, _, _| this.zoom(1.2))
                    .text_lg()
                    .py_0()
                    .tooltip("Zoom in · Ctrl++"),
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
mod edited_label_tests {
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
    #[::core::prelude::v1::test]
    fn folder_tree_guides_mark_continuing_ancestor_levels() {
        // Projects (0) -> Personal (1) -> Deep (2), Work (1), Reading (0)
        let rows = [
            ("Projects", 0),
            ("Personal", 1),
            ("Deep", 2),
            ("Work", 1),
            ("Reading", 0),
        ];
        assert_eq!(folder_tree_guides(&rows), vec![0, 0b01, 0b01, 0, 0]);
    }
}
