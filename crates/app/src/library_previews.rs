use super::*;
impl Controller {
    pub fn library_preview(&mut self, note: Id) -> Option<(Arc<Page>, usize)> {
        let metadata = self.notes.iter().find(|n| n.id == note)?.clone();
        if let Some(session) = self.sessions.get(&note) {
            let page = metadata
                .cover_page
                .and_then(|id| session.document.page(id))
                .unwrap_or(&session.document.pages[0]);
            return Some((Arc::new(page.clone()), session.document.pages.len()));
        }
        if let Some((updated, page, count)) = self.library_previews.get(&note)
            && *updated == metadata.updated_at
        {
            self.library_preview_recency.retain(|id| *id != note);
            self.library_preview_recency.push_back(note);
            return Some((page.clone(), *count));
        }
        // Virtual library rows request only visible covers. Keep background load
        // bounded and never open an editor session merely to draw a cover.
        if self.library_preview_failed.get(&note) == Some(&metadata.updated_at) {
            return None;
        }
        if self.library_preview_pending.len() < 8
            && !self.library_preview_pending.contains(&note)
            && self
                .workers
                .submit(Job::LibraryPreview {
                    note,
                    updated: metadata.updated_at,
                    cover: metadata.cover_page,
                    database: self.database.clone(),
                })
                .is_ok()
        {
            self.library_preview_pending.insert(note);
            self.busy += 1;
        }
        None
    }
    pub(super) fn cache_library_preview(
        &mut self,
        note: Id,
        updated: u64,
        page: Page,
        count: usize,
    ) {
        self.library_preview_recency.retain(|id| *id != note);
        self.library_preview_recency.push_back(note);
        self.library_previews
            .insert(note, (updated, Arc::new(page), count));
        while self.library_previews.len() > 128 {
            if let Some(id) = self.library_preview_recency.pop_front() {
                self.library_previews.remove(&id);
            } else {
                break;
            }
        }
    }
    pub fn library_cover_unavailable(&self, note: Id) -> bool {
        self.notes
            .iter()
            .find(|n| n.id == note)
            .is_some_and(|n| self.library_preview_failed.get(&note) == Some(&n.updated_at))
    }
    pub fn use_page_as_cover(&mut self) {
        let page = self.page().id;
        self.metadata(|metadata| metadata.cover_page = Some(page));
    }
}
