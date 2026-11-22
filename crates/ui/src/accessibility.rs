//! Accessible semantics and keyboard navigation for Folio's controls. Rune
//! exposes elements to the platform accessibility API itself; this module
//! keeps each control's role, its callback for keyboard navigation and its
//! laid-out bounds, which menus anchor to.
use super::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
pub type Callback = Rc<dyn Fn(&mut NotesView, &mut Window, &mut Context<NotesView>)>;

/// Window bounds as `x0, y0, x1, y1`, as AccessKit's rectangles were.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}
impl Rect {
    pub fn from_layout(r: [f32; 4]) -> Self {
        Self {
            x0: r[0] as f64,
            y0: r[1] as f64,
            x1: (r[0] + r[2]) as f64,
            y1: (r[1] + r[3]) as f64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Semantics {
    role: &'static str,
    group: Option<(u64, &'static str, &'static str)>,
    toggle: bool,
}
fn semantics(key: &str) -> Semantics {
    let radio = |id, label| Semantics {
        role: "radio",
        group: Some((id, label, "radiogroup")),
        toggle: true,
    };
    let button = Semantics {
        role: "button",
        group: None,
        toggle: false,
    };
    if key.starts_with("settings-section-") {
        return Semantics {
            role: "tab",
            group: Some((49, "Settings sections", "tablist")),
            toggle: false,
        };
    }
    if key == "tab-library"
        || key
            .strip_prefix("tab-")
            .is_some_and(|id| Id::parse_str(id).is_ok())
    {
        return Semantics {
            role: "tab",
            group: Some((20, "Open documents", "tablist")),
            toggle: false,
        };
    }
    if key
        .strip_prefix("folder-")
        .is_some_and(|id| Id::parse_str(id).is_ok())
    {
        return Semantics {
            role: "treeitem",
            group: Some((21, "Folders", "tree")),
            toggle: false,
        };
    }
    if key
        .strip_prefix("page-")
        .is_some_and(|id| Id::parse_str(id).is_ok())
    {
        return Semantics {
            role: "option",
            group: Some((22, "Pages", "listbox")),
            toggle: false,
        };
    }
    if key.starts_with("pad-action-") {
        return radio(51, "Pad button action");
    }
    if matches!(key, "all" | "favorite" | "recent" | "trash") {
        return radio(45, "Library filter");
    }
    if matches!(key, "system-theme" | "light-theme" | "dark-theme") {
        return radio(30, "Appearance");
    }
    if matches!(key, "lasso" | "select-rect") {
        return radio(50, "Selection tools");
    }
    if matches!(
        key,
        "pen" | "highlighter" | "eraser" | "select-tool" | "shape" | "text" | "hand"
    ) {
        return radio(31, "Writing and selection tools");
    }
    if key
        .strip_prefix("width-")
        .is_some_and(|i| i.parse::<usize>().is_ok())
    {
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
    if key.starts_with("sort-name-") || key.starts_with("sort-time-") {
        return radio(47, "Document sort order");
    }
    if key.starts_with("page-unit-") {
        return radio(48, "Page dimension unit");
    }
    if key
        .strip_prefix("preset-")
        .is_some_and(|id| Id::parse_str(id).is_ok())
    {
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
            | "auto-math-ink-toggle"
            | "autosave-toggle"
            | "motion-toggle"
            | "reopen-documents"
            | "reduce-motion"
    ) {
        return Semantics {
            role: "switch",
            toggle: true,
            ..button
        };
    }
    if key.starts_with("favorite-card-") {
        return Semantics {
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
pub(super) fn role(key: &str) -> &'static str {
    semantics(key).role
}
pub(super) fn toggles(key: &str) -> bool {
    semantics(key).toggle
}
pub(super) fn selects(key: &str) -> bool {
    matches!(semantics(key).role, "tab" | "treeitem" | "option")
}
struct Control {
    key: String,
    callback: Callback,
    enabled: bool,
    focus: FocusHandle,
}
#[derive(Default)]
pub struct Accessibility {
    controls: RefCell<Vec<Control>>,
    bounds: RefCell<HashMap<String, Rect>>,
}
impl Accessibility {
    /// Whether the control `key` has keyboard focus.
    pub(super) fn is_focused(&self, key: &str, window: &Window) -> bool {
        window.focused.as_deref() == Some(key)
    }
    pub(super) fn focus_virtual_control(
        &self,
        key: &str,
        window: &mut Window,
        _: &mut Context<NotesView>,
    ) {
        FocusHandle::new(key).focus(window);
    }
    pub fn begin(&self) {
        self.controls.borrow_mut().clear();
    }
    pub fn label_for(key: &str, label: &str) -> String {
        match key {
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
        }
        .to_owned()
    }
    #[allow(clippy::too_many_arguments)]
    pub fn control(
        &self,
        key: &str,
        _label: &str,
        callback: Callback,
        enabled: bool,
        _active: bool,
        visible: bool,
        _: &mut Context<NotesView>,
    ) -> (String, FocusHandle) {
        let focus = FocusHandle::new(key);
        if !visible {
            return (key.to_owned(), focus);
        }
        let control = Control {
            key: key.into(),
            callback,
            enabled,
            focus: focus.clone(),
        };
        let mut controls = self.controls.borrow_mut();
        if let Some(existing) = controls.iter_mut().find(|c| c.key == key) {
            *existing = control;
        } else {
            controls.push(control);
        }
        (key.to_owned(), focus)
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
    /// The focus of the neighboring enabled item in a context menu, whose
    /// controls share `prefix`. Arrow keys move through menu items without
    /// invoking them, unlike radio groups.
    pub fn menu_neighbor(&self, key: &str, prefix: &str, forward: bool) -> Option<FocusHandle> {
        let controls = self.controls.borrow();
        let members: Vec<_> = controls
            .iter()
            .filter(|c| c.enabled && c.key.starts_with(prefix))
            .collect();
        let index = members.iter().position(|c| c.key == key)?;
        let next = if forward {
            (index + 1) % members.len()
        } else {
            (index + members.len() - 1) % members.len()
        };
        Some(members[next].focus.clone())
    }
    pub(super) fn control_bounds_for_key(&self, key: &str) -> Option<Rect> {
        self.bounds.borrow().get(key).copied()
    }
    pub fn set_bounds(&self, key: &str, rect: [f32; 4]) {
        self.bounds
            .borrow_mut()
            .insert(key.to_owned(), Rect::from_layout(rect));
    }
    pub fn retain_library_items(&self, _: &[folio_document::NoteMetadata]) {}
    pub fn retain_folder_items(&self, _: impl IntoIterator<Item = Id>) {}
    pub fn publish(&self, _: &NotesView, _: &Window, _: &Context<NotesView>) {}
}
