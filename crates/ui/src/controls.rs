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
    "import",
    "favorite-note",
    "header-add-page",
    "duplicate-page-panel",
    "bookmark-page-panel",
    "move-page-panel",
    "page-from-template",
    "move-document-folder",
    "paper-color",
    "selection-cut",
    "delete-selection",
    "canvas-paste",
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
        let id = id.into();
        let label = label.into();
        let window_control = id.as_ref().starts_with("window-");
        let enabled = self.control_enabled(id.as_ref(), cx);
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
            active,
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
        let control_key = id.clone();
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
                    if matches!(event.keystroke.key.as_str(), "left" | "right")
                        && let Some(id) = control_key
                            .as_ref()
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
                        .as_ref()
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
                        .group_neighbor(control_key.as_ref(), forward)
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
                    if *hovered
                        && matches!(
                            hover_id.as_ref(),
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
    /// Whether the control `id` can be used right now.
    ///
    /// Window controls always work. Other controls are disabled behind a
    /// blocking overlay (unless they belong to it), while an update is being
    /// prepared, and, except for navigation, while a document is loading.
    /// Editing controls are disabled for documents in the Trash.
    fn control_enabled(&self, id: &str, cx: &App) -> bool {
        let window_control = id.starts_with("window-");
        let navigation = self.library_open
            || id.starts_with("tab-")
            || id.starts_with("close-tab-")
            || matches!(id, "library" | "new-tab" | "settings" | "search");
        let editing = EDITING_CONTROLS.contains(&id);
        (!self.controller.read_only() || self.library_open || !editing)
            && (!self.blocking_overlay() || self.building_overlay || window_control)
            && (!self.controller.loading_note()
                || navigation
                || self.building_overlay
                || window_control)
            && !self.update_preparing
            && self.control_ready(id, cx)
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
