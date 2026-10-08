//! AT-SPI/UI Automation bridge. Accessibility actions are queued to the UI thread.
use super::*;
#[cfg(not(windows))]
use accesskit::DeactivationHandler;
use accesskit::{
    Action, ActionRequest, ActivationHandler, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
};
pub type Callback = Rc<dyn Fn(&mut NotesView, &mut Window, &mut Context<NotesView>)>;
struct Activation(Arc<Mutex<Option<TreeUpdate>>>);
impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.lock().ok()?.clone()
    }
}
struct Actions(mpsc::Sender<ActionRequest>);
impl accesskit::ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self.0.send(request);
    }
}
#[cfg(not(windows))]
struct Deactivation;
#[cfg(not(windows))]
impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}

struct NativeAdapter {
    #[cfg(not(windows))]
    inner: accesskit_unix::Adapter,
    #[cfg(windows)]
    inner: Rc<RefCell<accesskit_windows::Adapter>>,
}
impl NativeAdapter {
    fn new(snapshot: Arc<Mutex<Option<TreeUpdate>>>, actions: Actions, _window: &Window) -> Self {
        #[cfg(not(windows))]
        let inner = accesskit_unix::Adapter::new(Activation(snapshot), actions, Deactivation);
        #[cfg(windows)]
        let inner = {
            let inner = Rc::new(RefCell::new(accesskit_windows::Adapter::new(
                accesskit_windows::HWND(_window.win32_handle() as *mut _),
                _window.is_window_active(),
                actions,
            )));
            let adapter = inner.clone();
            let mut activation = Activation(snapshot);
            _window.on_accessibility_object(move |wparam, lparam| {
                let response = adapter.borrow_mut().handle_wm_getobject(
                    accesskit_windows::WPARAM(wparam),
                    accesskit_windows::LPARAM(lparam),
                    &mut activation,
                );
                response.map(|response| {
                    let result: accesskit_windows::LRESULT = response.into();
                    result.0
                })
            });
            inner
        };
        Self { inner }
    }
    fn update_if_active(&mut self, update: impl FnOnce() -> TreeUpdate) {
        #[cfg(not(windows))]
        self.inner.update_if_active(update);
        #[cfg(windows)]
        {
            let events = self.inner.borrow_mut().update_if_active(update);
            if let Some(events) = events {
                events.raise();
            }
        }
    }
    fn update_window(&mut self, window: &Window) {
        #[cfg(not(windows))]
        {
            let bounds = window.bounds();
            let rect = accesskit::Rect {
                x0: f32::from(bounds.left()) as f64,
                y0: f32::from(bounds.top()) as f64,
                x1: f32::from(bounds.right()) as f64,
                y1: f32::from(bounds.bottom()) as f64,
            };
            self.inner.set_root_window_bounds(rect, rect);
            self.inner
                .update_window_focus_state(window.is_window_active());
        }
        #[cfg(windows)]
        {
            let events = self
                .inner
                .borrow_mut()
                .update_window_focus_state(window.is_window_active());
            if let Some(events) = events {
                events.raise();
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Semantics {
    role: Role,
    group: Option<(u64, &'static str, Role)>,
    toggle: bool,
}
fn semantics(key: &str) -> Semantics {
    let radio = |id, label| Semantics {
        role: Role::RadioButton,
        group: Some((id, label, Role::RadioGroup)),
        toggle: true,
    };
    let button = Semantics {
        role: Role::Button,
        group: None,
        toggle: false,
    };
    if key == "tab-library"
        || key
            .strip_prefix("tab-")
            .is_some_and(|id| Id::parse_str(id).is_ok())
    {
        return Semantics {
            role: Role::Tab,
            group: Some((20, "Open documents", Role::TabList)),
            toggle: false,
        };
    }
    if key.starts_with("folder-") && !key.starts_with("folder-destination-") {
        return Semantics {
            role: Role::TreeItem,
            group: Some((21, "Folders", Role::Tree)),
            toggle: false,
        };
    }
    if key
        .strip_prefix("page-")
        .is_some_and(|id| Id::parse_str(id).is_ok())
    {
        return Semantics {
            role: Role::ListBoxOption,
            group: Some((22, "Pages", Role::ListBox)),
            toggle: false,
        };
    }
    if matches!(key, "all" | "favorite" | "recent" | "trash") {
        return radio(45, "Library filter");
    }
    if matches!(key, "light-theme" | "dark-theme") {
        return radio(30, "Appearance");
    }
    if matches!(
        key,
        "pen" | "highlighter" | "eraser" | "lasso" | "select-rect" | "shape" | "text" | "hand"
    ) {
        return radio(31, "Writing and selection tools");
    }
    if key.starts_with("width-") {
        return radio(32, "Stroke width");
    }
    if key.starts_with("color-") {
        return radio(33, "Ink color");
    }
    if matches!(key, "canvas-pages" | "canvas-infinite") {
        return radio(34, "Canvas layout");
    }
    if matches!(
        key,
        "blank"
            | "ruled"
            | "grid"
            | "dots"
            | "paper-blank"
            | "paper-lines"
            | "paper-grid"
            | "paper-dots"
    ) {
        return radio(35, "Paper pattern");
    }
    if key.starts_with("size-") {
        return radio(36, "Page size");
    }
    if key.starts_with("orientation-") {
        return radio(37, "Page orientation");
    }
    if matches!(
        key,
        "paper-theme"
            | "paper-white"
            | "paper-cream"
            | "paper-sage"
            | "paper-blue"
            | "paper-charcoal"
            | "paper-black"
    ) {
        return radio(38, "Paper color");
    }
    if matches!(key, "export-original" | "export-visible" | "export-print") {
        return radio(46, "Export appearance");
    }
    if matches!(key, "grid-view" | "list-view") {
        return radio(39, "Library view");
    }
    if key.contains("-eraser-size-") {
        return radio(40, "Eraser size");
    }
    if key.contains("-eraser-") {
        return radio(41, "Eraser mode");
    }
    if key.starts_with("folder-destination-") {
        return radio(42, "Destination folder");
    }
    if key.starts_with("preset-") {
        return radio(43, "Pen presets");
    }
    if key.starts_with("pen-type-") {
        return radio(44, "Pen type");
    }
    if matches!(
        key,
        "canvas-theme"
            | "adapt-ink"
            | "scratch-toggle"
            | "hold-toggle"
            | "encircle-toggle"
            | "autosave-toggle"
            | "motion-toggle"
            | "reduce-motion"
    ) {
        return Semantics {
            role: Role::Switch,
            toggle: true,
            ..button
        };
    }
    if matches!(
        key,
        "favorite-note" | "refine" | "more" | "export" | "pen-options" | "pages" | "page-count"
    ) {
        return Semantics {
            toggle: true,
            ..button
        };
    }
    button
}
fn semantic_node(key: &str, label: &str, active: bool, enabled: bool) -> Node {
    let semantics = semantics(key);
    let mut node = Node::new(semantics.role);
    node.set_label(label);
    if semantics.toggle {
        node.set_toggled(if active {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    }
    if matches!(
        semantics.role,
        Role::Tab | Role::TreeItem | Role::ListBoxOption
    ) {
        node.set_selected(active);
    }
    if key.starts_with("toggle-folder-") {
        node.set_expanded(active);
    }
    if !enabled {
        node.set_disabled();
    } else {
        node.add_action(Action::Click);
        node.add_action(Action::Focus);
    }
    node
}
struct Control {
    id: NodeId,
    key: String,
    active: bool,
    label: String,
    callback: Callback,
    enabled: bool,
    focus: FocusHandle,
}
pub struct Accessibility {
    text_cache: RefCell<Option<(Id, u64, String)>>,
    adapter: RefCell<NativeAdapter>,
    snapshot: Arc<Mutex<Option<TreeUpdate>>>,
    controls: RefCell<Vec<Control>>,
    focus_handles: RefCell<HashMap<NodeId, FocusHandle>>,
    bounds: RefCell<HashMap<NodeId, accesskit::Rect>>,
    requests: mpsc::Receiver<ActionRequest>,
    pending: RefCell<Vec<ActionRequest>>,
}
impl Accessibility {
    pub fn new(window: &Window) -> Self {
        let snapshot = Arc::new(Mutex::new(None));
        let (tx, requests) = mpsc::channel();
        let adapter = NativeAdapter::new(snapshot.clone(), Actions(tx), window);
        Self {
            text_cache: RefCell::new(None),
            adapter: RefCell::new(adapter),
            snapshot,
            controls: RefCell::new(vec![]),
            focus_handles: RefCell::new(HashMap::new()),
            bounds: RefCell::new(HashMap::new()),
            requests,
            pending: RefCell::new(vec![]),
        }
    }
    pub fn begin(&self) {
        self.controls.borrow_mut().clear();
    }
    pub fn poll(&self) -> bool {
        while let Ok(request) = self.requests.try_recv() {
            self.pending.borrow_mut().push(request);
        }
        !self.pending.borrow().is_empty()
    }
    pub fn drain(&self) -> Vec<ActionRequest> {
        self.poll();
        std::mem::take(&mut *self.pending.borrow_mut())
    }
    pub fn action(&self, id: NodeId) -> Option<(Callback, FocusHandle)> {
        self.controls
            .borrow()
            .iter()
            .find(|c| c.id == id && c.enabled)
            .map(|c| (c.callback.clone(), c.focus.clone()))
    }
    pub fn control(
        &self,
        key: &str,
        label: &str,
        callback: Callback,
        enabled: bool,
        active: bool,
        visible: bool,
        cx: &mut Context<NotesView>,
    ) -> (NodeId, FocusHandle) {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hash);
        let id = NodeId(hash.finish() | 256);
        let focus = self
            .focus_handles
            .borrow_mut()
            .entry(id)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        if !visible {
            return (id, focus);
        }
        let label = match key {
            "undo" => "Undo",
            "redo" => "Redo",
            "add-notebook" => "New folder",
            "settings" => "Settings",
            "more" => "Document actions",
            "previous-page" => "Previous page",
            "next-page" => "Next page",
            "add-page" => "Add page",
            "zoom-out" => "Zoom out",
            "zoom-in" => "Zoom in",
            "help" => "Keyboard shortcuts and help",
            "ui-smaller" => "Decrease interface scale",
            "ui-larger" => "Increase interface scale",
            "cursor-smaller" => "Decrease cursor size",
            "cursor-larger" => "Increase cursor size",
            _ => label,
        };
        let control = Control {
            id,
            key: key.into(),
            active,
            label: label.into(),
            callback,
            enabled,
            focus: focus.clone(),
        };
        // A virtual list can measure and lay out the same row more than once.
        // AccessKit requires each child ID to occur exactly once in the tree.
        let mut controls = self.controls.borrow_mut();
        if let Some(existing) = controls.iter_mut().find(|c| c.id == id) {
            *existing = control;
        } else {
            controls.push(control);
        }
        (id, focus)
    }
    pub fn focus_control(&self, key: &str, window: &mut Window) {
        if let Some(control) = self
            .controls
            .borrow()
            .iter()
            .find(|c| c.key == key && c.enabled)
        {
            control.focus.focus(window);
        }
    }
    pub fn group_neighbor(&self, key: &str, forward: bool) -> Option<(Callback, FocusHandle)> {
        let group = semantics(key).group?;
        let controls = self.controls.borrow();
        let members: Vec<_> = controls
            .iter()
            .filter(|c| c.enabled && semantics(&c.key).group == Some(group))
            .collect();
        let index = members.iter().position(|c| c.key == key)?;
        let next = if forward {
            (index + 1) % members.len()
        } else {
            (index + members.len() - 1) % members.len()
        };
        Some((members[next].callback.clone(), members[next].focus.clone()))
    }
    pub fn control_bounds(&self, label: &str) -> Option<accesskit::Rect> {
        let id = self.controls.borrow().iter().find(|c| c.label == label)?.id;
        self.bounds.borrow().get(&id).copied()
    }
    pub fn set_bounds(&self, id: NodeId, bounds: Bounds<Pixels>) {
        let rect = accesskit::Rect {
            x0: f32::from(bounds.left()) as f64,
            y0: f32::from(bounds.top()) as f64,
            x1: f32::from(bounds.right()) as f64,
            y1: f32::from(bounds.bottom()) as f64,
        };
        if self.bounds.borrow().get(&id) == Some(&rect) {
            return;
        }
        self.bounds.borrow_mut().insert(id, rect);
        if let Ok(mut snapshot) = self.snapshot.lock()
            && let Some(snapshot) = snapshot.as_mut()
            && let Some((_, node)) = snapshot.nodes.iter_mut().find(|(n, _)| *n == id)
        {
            node.set_bounds(rect);
            let update = TreeUpdate {
                nodes: vec![(id, node.clone())],
                tree: None,
                tree_id: TreeId::ROOT,
                focus: snapshot.focus,
            };
            self.adapter.borrow_mut().update_if_active(|| update);
        }
    }
    pub fn publish(&self, view: &NotesView, window: &Window, cx: &App) {
        let mut root = Node::new(Role::Window);
        #[cfg(windows)]
        root.set_transform(accesskit::Affine::scale(window.scale_factor() as f64));
        root.set_label(if view.library_open {
            "Library — Folio".into()
        } else {
            format!(
                "{} — Folio",
                view.controller.session().document.metadata.title
            )
        });
        let mut document = Node::new(Role::Document);
        document.set_label(format!("Page {}", view.controller.session().page + 1));
        let page = view.controller.page();
        let mut cache = self.text_cache.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|(id, revision, _)| *id != page.id || *revision != page.revision)
        {
            *cache = Some((page.id, page.revision, page.text()));
        }
        document.set_value(cache.as_ref().unwrap().2.clone());
        if let Some(bounds) = view.canvas_bounds {
            document.set_bounds(accesskit::Rect {
                x0: f32::from(bounds.left()) as f64,
                y0: f32::from(bounds.top()) as f64,
                x1: f32::from(bounds.right()) as f64,
                y1: f32::from(bounds.bottom()) as f64,
            });
        }
        let mut nodes = vec![];
        let mut children = vec![];
        if !view.library_open && !view.blocking_overlay() {
            nodes.push((NodeId(2), document));
            children.push(NodeId(2));
        }
        let mut focus = NodeId(1);
        let controls = self.controls.borrow();
        for control in controls.iter() {
            let mut node = semantic_node(
                &control.key,
                &control.label,
                control.active,
                control.enabled,
            );
            if let Some(id) = control
                .key
                .strip_prefix("folder-")
                .and_then(|id| Id::parse_str(id).ok())
            {
                node.set_level(view.controller.folder_ancestors(id).len());
                if view
                    .controller
                    .notebooks
                    .iter()
                    .any(|n| n.parent == Some(id))
                {
                    node.set_expanded(!view.controller.settings.collapsed_folders.contains(&id));
                }
            }
            if let Some(bounds) = self.bounds.borrow().get(&control.id) {
                node.set_bounds(*bounds);
            }
            if control.focus.is_focused(window) {
                focus = control.id;
            }
            nodes.push((control.id, node));
            children.push(control.id);
        }
        if self.focus_handles.borrow().len() > 2048 {
            let ids = controls
                .iter()
                .map(|c| c.id)
                .collect::<std::collections::HashSet<_>>();
            self.focus_handles
                .borrow_mut()
                .retain(|id, _| ids.contains(id));
            self.bounds.borrow_mut().retain(|id, _| ids.contains(id));
        }
        if let Some(editor) = &view.inline_text
            && !view.blocking_overlay()
        {
            let field = editor.field.read(cx);
            let mut node = Node::new(Role::MultilineTextInput);
            node.set_label("Text box");
            node.set_value(field.content.clone());
            node.add_action(Action::Focus);
            node.add_action(Action::SetValue);
            if let Some(bounds) = field.bounds {
                node.set_bounds(accesskit::Rect {
                    x0: f32::from(bounds.left()) as f64,
                    y0: f32::from(bounds.top()) as f64,
                    x1: f32::from(bounds.right()) as f64,
                    y1: f32::from(bounds.bottom()) as f64,
                });
            }
            if field.focus.is_focused(window) {
                focus = NodeId(13);
            }
            nodes.push((NodeId(13), node));
            children.push(NodeId(13));
        }
        if let Some((modal, field)) = &view.modal
            && view.controller.error.is_none()
            && !view.settings_open
            && !view.help_open
            && !matches!(modal, Modal::MovePage | Modal::Templates)
        {
            let field = field.read(cx);
            let mut node = Node::new(if field.secret {
                Role::PasswordInput
            } else if field.multiline {
                Role::MultilineTextInput
            } else {
                Role::TextInput
            });
            node.set_label(
                if matches!(modal, Modal::MoveNotebook(_) | Modal::MoveDocument(_)) {
                    "Filter destination folders"
                } else {
                    modal.title()
                },
            );
            if view.modal_error.is_some() {
                node.set_invalid(accesskit::Invalid::True);
                node.set_error_message(NodeId(10));
            }
            if let Some(bounds) = field.bounds {
                node.set_bounds(accesskit::Rect {
                    x0: f32::from(bounds.left()) as f64,
                    y0: f32::from(bounds.top()) as f64,
                    x1: f32::from(bounds.right()) as f64,
                    y1: f32::from(bounds.bottom()) as f64,
                });
            }
            if !field.secret {
                node.set_value(field.content.clone());
            }
            node.add_action(Action::Focus);
            node.add_action(Action::SetValue);
            if field.focus.is_focused(window) {
                focus = NodeId(3);
            }
            nodes.push((NodeId(3), node));
            children.push(NodeId(3));
            if let Some(setup) = &view.notebook_setup
                && setup.custom_color_open
            {
                let field = setup.color.read(cx);
                let mut color = Node::new(Role::TextInput);
                color.set_label("Custom paper color");
                color.set_value(field.content.clone());
                color.add_action(Action::Focus);
                color.add_action(Action::SetValue);
                if let Some(bounds) = field.bounds {
                    color.set_bounds(accesskit::Rect {
                        x0: f32::from(bounds.left()) as f64,
                        y0: f32::from(bounds.top()) as f64,
                        x1: f32::from(bounds.right()) as f64,
                        y1: f32::from(bounds.bottom()) as f64,
                    });
                }
                if field.focus.is_focused(window) {
                    focus = NodeId(12);
                }
                nodes.push((NodeId(12), color));
                children.push(NodeId(12));
            }
            if let Some(error) = &view.modal_error {
                let mut status = Node::new(Role::Alert);
                status.set_live(accesskit::Live::Assertive);
                status.set_label(error.clone());
                nodes.push((NodeId(10), status));
                children.push(NodeId(10));
            }
        }
        if let Some(inputs) = &view.math_inputs
            && view.modal.is_none()
            && !view.blocking_overlay()
        {
            for ((id, label, entity), visible) in [
                (5, "Math expression", &inputs.expression),
                (6, "Target variable", &inputs.variable),
                (7, "Next mathematical line", &inputs.next_line),
                (8, "Graph x range", &inputs.range),
                (11, "Result LaTeX source", &inputs.result_latex),
            ]
            .into_iter()
            .zip(
                inputs
                    .visible_fields()
                    .into_iter()
                    .chain([inputs.latex_open]),
            ) {
                if !visible {
                    continue;
                }
                let field = entity.read(cx);
                let mut node = Node::new(if field.multiline {
                    Role::MultilineTextInput
                } else {
                    Role::TextInput
                });
                node.set_label(label);
                node.set_value(field.content.clone());
                if let Some(bounds) = field.bounds {
                    node.set_bounds(accesskit::Rect {
                        x0: f32::from(bounds.left()) as f64,
                        y0: f32::from(bounds.top()) as f64,
                        x1: f32::from(bounds.right()) as f64,
                        y1: f32::from(bounds.bottom()) as f64,
                    });
                }
                node.add_action(Action::Focus);
                node.add_action(Action::SetValue);
                if field.focus.is_focused(window) {
                    focus = NodeId(id);
                }
                nodes.push((NodeId(id), node));
                children.push(NodeId(id));
            }
        }
        if let Some(session) = &view.controller.math_session
            && !view.blocking_overlay()
            && view.modal.is_none()
        {
            let mut result = Node::new(Role::Status);
            let label = if view.controller.recognition_pending {
                view.controller.recognition_status.clone()
            } else if session.pending {
                if view.math_inputs.as_ref().is_some_and(|i| i.latex_open) {
                    "Updating the answer…".into()
                } else {
                    "Solving math…".into()
                }
            } else if let Some(error) = &session.error {
                format!("Math solver: {error}")
            } else if let Some(report) = &session.report {
                let guided = view.math_inputs.as_ref().is_some_and(|i| i.guided);
                let steps = if guided {
                    report
                        .steps
                        .get(session.revealed.saturating_sub(1))
                        .map(|s| {
                            format!(
                                "Step {} of {}: {}. {}",
                                session.revealed,
                                report.steps.len(),
                                s.explanation,
                                s.after
                            )
                        })
                        .unwrap_or_default()
                } else {
                    report
                        .steps
                        .iter()
                        .take(session.revealed)
                        .map(|s| format!("{}: {}", s.explanation, s.after))
                        .collect::<Vec<_>>()
                        .join(". ")
                };
                format!(
                    "{}: {}. {}. {}. {}",
                    report.title,
                    report.answer,
                    report.message,
                    report.restrictions.join(", "),
                    steps
                )
            } else {
                "Math solver: enter a problem and choose Solve problem".into()
            };
            result.set_label(label);
            nodes.push((NodeId(9), result));
            children.push(NodeId(9));
        }
        let mut status = Node::new(Role::Status);
        if view.controller.error.is_none() {
            status.set_live(accesskit::Live::Polite);
        }
        status.set_label(if let Some(error) = &view.controller.save_error {
            format!("Changes are not saved. {error}. Retry saving is available.")
        } else if let Some(error) = view
            .controller
            .page_preview_error()
            .filter(|_| !view.library_open)
        {
            format!("Preview unavailable: {error}. Retry preview is available.")
        } else if view.controller.recognition_pending {
            view.controller.recognition_status.clone()
        } else {
            view.controller.activity_status()
        });
        nodes.push((NodeId(4), status));
        children.push(NodeId(4));
        let mut groups: std::collections::BTreeMap<u64, (&str, Role, Vec<NodeId>)> =
            std::collections::BTreeMap::new();
        for control in controls.iter() {
            if let Some((id, label, role)) = semantics(&control.key).group {
                groups
                    .entry(id)
                    .or_insert((label, role, Vec::new()))
                    .2
                    .push(control.id);
                children.retain(|child| *child != control.id);
            }
        }
        for (id, (label, role, members)) in groups {
            let mut group = Node::new(role);
            group.set_label(label);
            group.set_children(members);
            nodes.push((NodeId(id), group));
            children.push(NodeId(id));
        }
        if let Some(error) = &view.controller.error {
            let mut alert = Node::new(Role::Alert);
            alert.set_label(error.clone());
            alert.set_live(accesskit::Live::Assertive);
            nodes.push((NodeId(15), alert));
            children.push(NodeId(15));
        }
        let dialog_label = if view.controller.error.is_some() {
            Some("Folio needs your attention")
        } else if let Some((modal, _)) = &view.modal {
            Some(modal.title())
        } else if view.settings_open {
            Some("Settings")
        } else if view.help_open {
            Some("Keyboard shortcuts and help")
        } else {
            None
        };
        if let Some(label) = dialog_label {
            let mut dialog = Node::new(if view.controller.error.is_some() {
                Role::AlertDialog
            } else {
                Role::Dialog
            });
            dialog.set_label(label);
            if view.controller.error.is_some() {
                dialog.set_described_by(vec![NodeId(15)]);
            }
            let window_controls: std::collections::HashSet<_> = controls
                .iter()
                .filter(|c| c.key.starts_with("window-"))
                .map(|c| c.id)
                .collect();
            let members: Vec<_> = children
                .iter()
                .copied()
                .filter(|id| !window_controls.contains(id) && *id != NodeId(4))
                .collect();
            children.retain(|id| !members.contains(id));
            dialog.set_children(members);
            nodes.push((NodeId(14), dialog));
            children.push(NodeId(14));
        }
        root.set_children(children);
        nodes.push((NodeId(1), root));
        let mut tree = TreeInfo::new(NodeId(1));
        tree.toolkit_name = Some("Folio GPUI".into());
        tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
        let update = TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus,
        };
        if let Ok(mut current) = self.snapshot.lock() {
            *current = Some(update.clone());
        }
        let mut adapter = self.adapter.borrow_mut();
        adapter.update_window(window);
        adapter.update_if_active(|| update);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn semantic_controls_expose_selection_toggle_and_disabled_states() {
        let tab = semantic_node(&format!("tab-{}", Id::new_v4()), "Document", true, true);
        assert_eq!(tab.role(), Role::Tab);
        assert_eq!(tab.is_selected(), Some(true));
        let radio = semantic_node("eraser", "Eraser", false, true);
        assert_eq!(radio.role(), Role::RadioButton);
        assert_eq!(radio.toggled(), Some(accesskit::Toggled::False));
        let toggle = semantic_node("adapt-ink", "Readable ink", true, true);
        assert_eq!(toggle.role(), Role::Switch);
        assert_eq!(toggle.toggled(), Some(accesskit::Toggled::True));
        let disabled = semantic_node("delete-page", "Delete page", false, false);
        assert!(disabled.is_disabled());
        assert!(!disabled.supports_action(Action::Click));
        assert_eq!(
            semantics("folder-destination-root").group.unwrap().1,
            "Destination folder"
        );
        assert_eq!(
            semantic_node("toggle-folder-test", "Expand", false, true).is_expanded(),
            Some(false)
        );
    }
}
