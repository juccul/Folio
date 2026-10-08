use super::*;
impl NotesView {
    pub(super) fn store_workspace(&mut self) {
        let mut state = self.controller.settings.workspace.clone();
        state.list_view = self.list_view;
        state.sort_by_name = self.sort_by_name;
        state.pages_open = self.pages_open;
        state.library_open = self.library_open;
        state.open_tabs = self.open_tabs.iter().rev().take(128).copied().collect();
        state.open_tabs.reverse();
        state.active_document = self
            .controller
            .notes
            .iter()
            .any(|n| n.id == self.controller.active)
            .then_some(self.controller.active);
        state
            .current_pages
            .retain(|(id, _)| self.controller.notes.iter().any(|n| n.id == *id));
        for (id, session) in &self.controller.sessions {
            state.current_pages.retain(|(note, _)| note != id);
            if self.controller.notes.iter().any(|n| n.id == *id) {
                state.current_pages.push((*id, session.page().id));
            }
        }
        state.current_pages.sort_unstable_by_key(|(id, _)| *id);
        state.current_pages.truncate(128);
        if state != self.controller.settings.workspace {
            self.controller.settings.workspace = state;
            self.controller.store_settings();
        }
    }
}
