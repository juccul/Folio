//! Native window controls and compositor-driven movement/resizing. The tab
//! strip consumes only background presses; tabs keep their own drag behavior.
use super::*;

impl NotesView {
    /// Closing would leave edits unsaved because autosave is off.
    pub(super) fn needs_close_prompt(&self) -> bool {
        !self.controller.settings.autosave && self.controller.has_unsaved_documents()
    }
    /// Close the window, saving document edits or discarding those that
    /// autosave-off deferred. Folders and preferences are always kept.
    pub(super) fn close_window(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_inline_text(cx);
        self.store_workspace();
        let result = if save {
            self.controller.flush()
        } else {
            self.controller.close_discarding_documents()
        };
        match result {
            Ok(()) => {
                self.closing = true;
                window.remove_window();
            }
            Err(error) => {
                self.modal = None;
                self.controller.error = Some(error);
                cx.notify();
            }
        }
    }
    /// Room for the window's own controls. Rune draws the caption buttons
    /// over the titlebar band on Windows and macOS; on Linux the system frame
    /// keeps them, so the band needs none.
    pub(super) fn window_controls(&self, _: &Window, _: &mut Context<Self>) -> Div {
        div().w(px(self.chrome_trailing)).flex_shrink_0()
    }
}

/// Empty titlebar chrome: a press moves the window, a double press zooms it.
pub(super) fn drag_region() -> Div {
    div()
        .id("titlebar-drag-region")
        .flex_1()
        .h_full()
        .el(|e| e.window_drag())
}

pub(super) fn frame(content: impl IntoElement, _: &mut Window) -> Div {
    div().relative().size_full().child(content)
}
