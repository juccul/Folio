//! AT-SPI bridge for GPUI controls. Accessibility requests are messages to the UI;
//! the D-Bus thread never touches the document or blocks the drawing thread.
use super::*;
use accesskit::{
    Action, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId, Role, TreeId,
    TreeInfo, TreeUpdate,
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
struct Deactivation;
impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}
struct Control {
    id: NodeId,
    label: String,
    callback: Callback,
    enabled: bool,
    focus: FocusHandle,
}
pub struct Accessibility {
    text_cache: RefCell<Option<(Id, u64, String)>>,
    adapter: RefCell<accesskit_unix::Adapter>,
    snapshot: Arc<Mutex<Option<TreeUpdate>>>,
    controls: RefCell<Vec<Control>>,
    focus_handles: RefCell<HashMap<NodeId, FocusHandle>>,
    bounds: RefCell<HashMap<NodeId, accesskit::Rect>>,
    requests: mpsc::Receiver<ActionRequest>,
    pending: RefCell<Vec<ActionRequest>>,
}
impl Accessibility {
    pub fn new() -> Self {
        let snapshot = Arc::new(Mutex::new(None));
        let (tx, requests) = mpsc::channel();
        let adapter =
            accesskit_unix::Adapter::new(Activation(snapshot.clone()), Actions(tx), Deactivation);
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
            "pen-options" => "Pen settings",
            "favorite-note" => "Toggle favorite",
            "settings" => "Settings",
            "more" => "Document actions",
            "previous-page" => "Previous page",
            "next-page" => "Next page",
            "add-page" => "Add page",
            "zoom-out" => "Zoom out",
            "zoom-in" => "Zoom in",
            "help" => "Keyboard shortcuts and help",
            "cursor-smaller" => "Decrease cursor size",
            "cursor-larger" => "Increase cursor size",
            _ => label,
        };
        let control = Control {
            id,
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
            let mut node = Node::new(Role::Button);
            node.set_label(control.label.clone());
            if !control.enabled {
                node.set_disabled();
            }
            node.add_action(Action::Click);
            node.add_action(Action::Focus);
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
        if let Some((modal, field)) = &view.modal {
            let field = field.read(cx);
            let mut node = Node::new(if field.secret {
                Role::PasswordInput
            } else if field.multiline {
                Role::MultilineTextInput
            } else {
                Role::TextInput
            });
            node.set_label(modal.title());
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
            if let Some(error) = &view.modal_error {
                let mut status = Node::new(Role::Status);
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
        status.set_label(if view.controller.recognition_pending {
            view.controller.recognition_status.clone()
        } else {
            view.controller.status.clone()
        });
        nodes.push((NodeId(4), status));
        children.push(NodeId(4));
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
        let bounds = window.bounds();
        let rect = accesskit::Rect {
            x0: f32::from(bounds.left()) as f64,
            y0: f32::from(bounds.top()) as f64,
            x1: f32::from(bounds.right()) as f64,
            y1: f32::from(bounds.bottom()) as f64,
        };
        let mut adapter = self.adapter.borrow_mut();
        adapter.set_root_window_bounds(rect, rect);
        adapter.update_window_focus_state(window.is_window_active());
        adapter.update_if_active(|| update);
    }
}
