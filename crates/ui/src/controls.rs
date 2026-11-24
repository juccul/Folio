//! Buttons and other focusable controls, and the rules that enable them.
//!
//! Every control has a string id. Besides identifying the control for focus,
//! hover animation and accessibility, the id decides when it is enabled: see
//! [`NotesView::control_enabled`].
use super::*;

/// Controls that change the open document; disabled while it is in the Trash.
const EDITING_CONTROLS: &[&str] = &[
    "undo",
    "redo",
    "rename-note",
    "tags",
    "favorite",
    "add-page",
    "delete-page",
    "duplicate-page",
    "bookmark-page",
    "move-page",
    "page-size",
    "infinite",
    "insert-space",
    "remove-space",
    "insert-equation",
    "cover-page",
    "page-templates",
    "index-page-handwriting",
    "clear-page-index",
    "blank",
    "ruled",
    "grid",
    "dots",
    "toolbar-image",
    "favorite-note",
    "header-add-page",
    "duplicate-page-panel",
    "bookmark-page-panel",
    "move-page-panel",
    "move-document-folder",
    "paper-color",
    "selection-cut",
    "delete-selection",
    "canvas-paste",
    "canvas-cut",
    "canvas-delete",
];

impl NotesView {
    pub(crate) fn button(
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
            div().min_w_0().truncate().child(label).into_any_element(),
            active,
            cx,
            action,
        )
    }
    pub(crate) fn control(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        content: AnyElement,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let id: String = id.into();
        let label: String = label.into();
        let window_control = id.starts_with("window-");
        let id_is_doc_menu = id.starts_with("doc-menu-");
        let enabled = self.control_enabled(&id, cx);
        let hover_id = id.clone();
        // Controls sit on glass: clear at rest, tinted when hovered (Rune
        // fades the hover tint in and out) or active.
        let glide = accessibility::glide_group(&id);
        let background = theme.tint(if active && glide.is_none() { 1. } else { 0. });
        // Pressed controls give a little, as physical keys do; wide rows
        // (menu items) less than compact buttons, small steppers a touch more.
        let press_scale = if window_control {
            1.
        } else if matches!(
            id.as_str(),
            "radius-less"
                | "radius-more"
                | "glass-opacity-less"
                | "glass-opacity-more"
                | "glass-blur-less"
                | "glass-blur-more"
                | "ui-smaller"
                | "ui-larger"
                | "cursor-smaller"
                | "cursor-larger"
        ) || id.starts_with("minus-")
            || id.starts_with("plus-")
        {
            0.86
        } else if accessibility::role(&id) == "switch" {
            0.92
        } else if [
            "doc-menu-",
            "menu-",
            "note-",
            "folder-",
            "settings-section-",
            "page-",
        ]
        .iter()
        .any(|prefix| id.starts_with(prefix))
        {
            0.98
        } else {
            0.95
        };
        let destructive = [
            "Delete",
            "Move to trash",
            "Empty Trash",
            "Close without saving",
        ]
        .iter()
        .any(|prefix| label.starts_with(prefix));
        // Controls behind a context menu cannot be used, but keep their
        // appearance; only real dialogs dim what they cover.
        let behind_menu = !enabled
            && self.menu_only_overlay()
            && self.control_enabled_behind(id.as_str(), false, cx);
        let dimmed = !enabled && !behind_menu;
        let action: accessibility::Callback = std::rc::Rc::new(action);
        let (_node, focus) = self.accessibility.control(
            id.as_str(),
            &label,
            action.clone(),
            enabled,
            active,
            !self.blocking_overlay() || self.building_overlay || window_control,
            cx,
        );
        let control_key = id.clone();
        let pressed_key = id.clone();
        let id_for_bounds = id.clone();
        let role_key = id.clone();
        let accessible_label = accessibility::Accessibility::label_for(&id, &label);
        div()
            .id(id)
            .relative()
            .track_focus(&focus)
            .min_h(rems(2.))
            .tab_stop(enabled)
            .key_context("FolioControl")
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if enabled
                    && matches!(
                        event.keystroke.key.as_str(),
                        "left" | "right" | "up" | "down"
                    )
                {
                    let forward = matches!(event.keystroke.key.as_str(), "right" | "down");
                    // Document menu items and wheel tools only move focus;
                    // Enter activates.
                    let menu = ["doc-menu-", "wheel-"]
                        .into_iter()
                        .find(|prefix| control_key.starts_with(prefix));
                    if let Some(prefix) = menu {
                        if prefix == "doc-menu-" {
                            this.document_menu_keyboard = true;
                        }
                        let along = prefix == "wheel-"
                            || matches!(event.keystroke.key.as_str(), "up" | "down");
                        if along
                            && let Some(focus) = this.accessibility.menu_neighbor(
                                control_key.as_str(),
                                prefix,
                                forward,
                            )
                        {
                            focus.focus(window);
                            cx.stop_propagation();
                            cx.notify();
                        }
                        return;
                    }
                    if matches!(event.keystroke.key.as_str(), "left" | "right")
                        && let Some(id) = control_key
                            .as_str()
                            .strip_prefix("folder-")
                            .and_then(|id| Id::parse_str(id).ok())
                    {
                        let branch = this
                            .controller
                            .notebooks
                            .iter()
                            .any(|n| n.parent == Some(id));
                        if branch {
                            let collapsed = &mut this.controller.settings.collapsed_folders;
                            collapsed.retain(|f| *f != id);
                            if !forward {
                                collapsed.push(id);
                            }
                            this.controller.store_settings();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                    }
                    if let Some(id) = control_key
                        .as_str()
                        .strip_prefix("folder-")
                        .and_then(|id| Id::parse_str(id).ok())
                        && this.navigate_folder_control(id, forward, window, cx)
                    {
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    if let Some((callback, focus)) = this
                        .accessibility
                        .group_neighbor(control_key.as_str(), forward)
                    {
                        focus.focus(window);
                        callback(this, window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                }
                // GPUI invokes on_click once on Enter/Space release. Handling
                // those keys here as well would activate toggles twice.
            }))
            .flex()
            .items_center()
            .justify_center()
            .px_3()
            .py_2()
            .border_1()
            .border_color(transparent_black())
            // Document menu items opened by pointer show the ring only once
            // the keyboard is used in the menu.
            .when(!id_is_doc_menu || self.document_menu_keyboard, move |s| {
                s.focus(move |s| s.border_color(rgb(theme.ring)))
            })
            .rounded(px(theme.radius))
            .text_sm()
            .text_color(rgb(if destructive {
                theme.destructive
            } else {
                theme.ink
            }))
            .bg(background)
            .when(glide.is_some(), |s| s.lock_bg().selection_member(active))
            // A control that opened a menu keeps its hover tint, so the tint
            // fades out as the pointer leaves rather than vanishing with
            // the click.
            .when((enabled || behind_menu) && (!active || glide.is_some()), move |s| {
                s.hover(move |s| s.bg(theme.tint(1.)))
            })
            .when(enabled, move |s| s.el(move |e| e.press_scale(press_scale)))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                // Page thumbnails and the tool wheel draw their own hover
                // state (a ring, a painted segment), so they still follow it.
                if enabled && (hover_id.starts_with("page-") || hover_id.starts_with("wheel-")) {
                    this.motion.borrow_mut().hover(
                        hover_id.as_str(),
                        *hovered,
                        this.controller.settings.reduce_motion,
                    );
                    cx.notify();
                }
                if enabled
                    && *hovered
                    && matches!(
                        hover_id.as_str(),
                        "recognize-text"
                            | "recognize-math"
                            | "index-handwriting"
                            | "math-use-selection"
                            | "math-read-selection"
                            | "math-read-text"
                            | "math-read-next"
                    )
                {
                    this.controller.prepare_recognition();
                }
            }))
            // Held, a control's highlight dims below its hover tint; let go,
            // it fades back up while the control springs back to size.
            .active(move |s| if enabled { s.bg(theme.press_wash()) } else { s })
            .opacity(if dimmed { 0.4 } else { 1. })
            .when(enabled, |s| s.cursor_pointer())
            // In a selection group the chosen member's tint is one highlight
            // that glides to the next choice. Each member keeps the slot so
            // its content never changes place.
            .when_some(glide, move |s, group| {
                s.child(div().absolute().inset_0().when(active, move |slot| {
                    let id = format!("glide-{group}");
                    slot.child(
                        div()
                            .id(id.clone())
                            .absolute()
                            .inset_0()
                            .rounded(px(theme.radius))
                            .bg(theme.tint(1.))
                            .el(move |e| {
                                e.matched(id)
                                    .motion(crate::motion::GLIDE)
                                    .animated_changes()
                            }),
                    )
                }))
            })
            .child(content)
            .track_bounds(id_for_bounds)
            .role(accessibility::role(&role_key))
            .label(accessible_label)
            .when(
                accessibility::toggles(&role_key) || accessibility::selects(&role_key),
                |s| s.checked(active),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if enabled {
                    this.pointer_activation = !event.is_keyboard();
                    this.last_pressed = Some((pressed_key.clone(), std::time::Instant::now()));
                    action(this, window, cx);
                    this.pointer_activation = false;
                    cx.notify();
                }
            }))
    }
    /// Whether the control `id` can be used right now.
    ///
    /// Window controls always work. Other controls are disabled behind a
    /// blocking overlay (unless they belong to it), while an update is being
    /// prepared, and, except for navigation, while a document is loading.
    /// Editing controls are disabled for documents in the Trash.
    fn control_enabled(&self, id: &str, cx: &App) -> bool {
        self.control_enabled_behind(id, self.blocking_overlay(), cx)
    }
    /// Whether `id` is usable, given whether a blocking overlay covers it.
    fn control_enabled_behind(&self, id: &str, blocked: bool, cx: &App) -> bool {
        let window_control = id.starts_with("window-");
        let navigation = self.library_open
            || id.starts_with("tab-")
            || id.starts_with("close-tab-")
            || matches!(id, "library" | "new-tab" | "settings" | "search");
        let editing = EDITING_CONTROLS.contains(&id);
        (!self.controller.read_only() || self.library_open || !editing)
            && (!blocked || self.building_overlay || window_control)
            && (!self.controller.loading_note()
                || navigation
                || self.building_overlay
                || window_control)
            && !self.update_preparing
            && self.control_ready(id, cx)
    }
    /// The only overlay is a context menu, which should not dim the window.
    /// Dialogs shown as popovers (`modal_is_menu`) and Help count as menus.
    fn menu_only_overlay(&self) -> bool {
        (self.document_menu.is_some()
            || self.folder_menu.is_some()
            || self.canvas_menu.is_some()
            || self.help_open
            || self.modal_is_menu())
            && !self.update_preparing
            && (self.modal.is_none() || self.modal_is_menu())
            && !self.settings_open
            && self.controller.error.is_none()
    }
    /// Conditions specific to individual controls.
    fn control_ready(&self, id: &str, cx: &App) -> bool {
        let math_pending = self
            .controller
            .math_session
            .as_ref()
            .is_some_and(|s| s.pending);
        match id {
            "app-update" | "update-settings-action" => {
                self.updater.state.can_download() || self.updater.state.can_restart()
            }
            "update-check" => !self.updater.state.busy(),
            "canvas-paste" => self.canvas_menu.is_some_and(|menu| menu.can_paste),
            "canvas-cut" | "canvas-copy" | "canvas-delete" => {
                !self.controller.session().selection.is_empty()
            }
            "canvas-select-all" => !self.controller.page().objects.is_empty(),
            "delete-folder" => self
                .folder_menu
                .is_some_and(|(id, _)| self.controller.folder_deletion_reason(id).is_none()),
            key if key.starts_with("folder-destination-") => {
                let destination = key
                    .strip_prefix("folder-destination-")
                    .and_then(|id| Id::parse_str(id).ok());
                match self.modal.as_ref() {
                    Some((Modal::MoveNotebook(source), _)) => self
                        .controller
                        .validate_folder_move(*source, destination)
                        .is_ok(),
                    _ => true,
                }
            }
            "submit-modal" => self.can_submit_modal(),
            key if key.starts_with("favorite-card-") => key
                .strip_prefix("favorite-card-")
                .and_then(|id| Id::parse_str(id).ok())
                .is_some_and(|id| {
                    self.controller
                        .notes
                        .iter()
                        .any(|n| n.id == id && !n.trashed)
                }),
            "cleanup-assets-settings" => self.controller.can_cleanup_assets(),
            "backup-library" => !self.controller.task_running("backup"),
            "restore-library" => !self.controller.task_running("restore"),
            "export-pdf" | "export-png" | "export-svg" | "export-text" | "export-notebook" => {
                !self.controller.task_running("export")
            }
            "open-restored-library" | "switch-library" => !self.controller.has_background_work(),
            key if key.starts_with("retry-linked-math-") => !self
                .controller
                .session()
                .selection
                .iter()
                .any(|id| self.controller.math_updating(*id)),
            "undo" => self.controller.session().history.can_undo(),
            "redo" => self.controller.session().history.can_redo(),
            "previous-page" => self.controller.session().page > 0,
            "next-page" => {
                self.controller.session().page + 1 < self.controller.session().document.pages.len()
            }
            "delete-page" => self.controller.session().document.pages.len() > 1,
            "grid-view" => !self.list_forced,
            "ui-smaller" => self.controller.settings.ui_scale > 0.85,
            "ui-larger" => self.controller.settings.ui_scale < 1.55,
            "cursor-smaller" => self.controller.settings.cursor_size > 4.5,
            "cursor-larger" => self.controller.settings.cursor_size < 63.5,
            "undo-library" => self.controller.can_undo_library(),
            "radius-less" => self.controller.settings.appearance.radius() > 0.,
            "radius-more" => self.controller.settings.appearance.radius() < 20.,
            "glass-opacity-less" => self.controller.settings.appearance.glass_opacity() > 0.31,
            "glass-opacity-more" => self.controller.settings.appearance.glass_opacity() < 0.99,
            "glass-blur-less" => self.controller.settings.appearance.glass_blur() > 0.,
            "glass-blur-more" => self.controller.settings.appearance.glass_blur() < 40.,
            "math-apply-latex" => {
                self.math_inputs
                    .as_ref()
                    .is_some_and(|inputs| inputs.result_dirty(cx))
                    && !math_pending
            }
            "copy-math-result" | "insert-math-result" | "insert-math-worked"
            | "insert-math-live" => {
                self.math_inputs
                    .as_ref()
                    .is_some_and(|inputs| !inputs.result_dirty(cx))
                    && !math_pending
            }
            "math-previous" | "math-next" | "math-hint" => self
                .controller
                .math_session
                .as_ref()
                .is_some_and(|session| {
                    !session.pending
                        && session.report.as_ref().is_some_and(|report| {
                            if id == "math-previous" {
                                session.revealed > 1
                            } else {
                                session.revealed < report.steps.len()
                            }
                        })
                }),
            "math-use-selection" | "math-read-selection" | "math-read-text" | "math-read-next" => {
                !self.controller.session().selection.is_empty()
                    && !self.controller.recognition_pending
                    && !math_pending
            }
            "math-primary" => {
                self.math_inputs
                    .as_ref()
                    .is_some_and(|inputs| inputs.can_run(cx))
                    && !self.controller.recognition_pending
                    && !math_pending
            }
            _ => true,
        }
    }
    fn can_submit_modal(&self) -> bool {
        match self.modal.as_ref() {
            Some((Modal::MoveNotebook(source), _)) => self
                .controller
                .validate_folder_move(*source, self.folder_destination)
                .is_ok(),
            Some((Modal::MovePage, _)) => self.move_page_destination.is_some(),
            Some((Modal::Recognition, _)) if self.controller.recognition_for_index => {
                self.controller.can_index_review()
                    && !self.controller.recognition_pending
                    && !self.controller.read_only()
            }
            _ if self.equation_draft.is_some() => !self.controller.equation_pending,
            _ => true,
        }
    }
}
