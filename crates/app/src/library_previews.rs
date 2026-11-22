use super::*;
impl Controller {
    pub fn library_preview(&mut self, note: Id) -> Option<(Arc<Page>, usize)> {
        if let Some(session) = self.sessions.get_mut(&note) {
            let page = session
                .document
                .metadata
                .cover_page
                .and_then(|id| session.document.page(id))
                .unwrap_or(&session.document.pages[0]);
            let cover = if let Some(cover) = session
                .library_cover
                .as_ref()
                .filter(|cover| cover.id == page.id && cover.revision == page.revision)
            {
                cover.clone()
            } else {
                let cover = Arc::new(page.clone());
                session.library_cover = Some(cover.clone());
                session.memory_usage.set(None);
                cover
            };
            return Some((cover, session.document.pages.len()));
        }
        let metadata = self.notes.iter().find(|n| n.id == note)?;
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
        while self.library_previews.len() > 24 {
            if let Some(id) = self.library_preview_recency.pop_front() {
                self.library_previews.remove(&id);
            } else {
                break;
            }
        }
    }
    pub fn release_library_preview(&mut self, note: Id) {
        if self.library_previews.remove(&note).is_some() {
            self.library_preview_recency.retain(|id| *id != note);
            self.needs_memory_trim = true;
        }
    }
    pub fn clear_library_previews(&mut self) {
        if !self.library_previews.is_empty() {
            self.library_previews.clear();
            self.library_preview_recency.clear();
            self.needs_memory_trim = true;
        }
        for session in self.sessions.values_mut() {
            if session.library_cover.take().is_some() {
                session.memory_usage.set(None);
                self.needs_memory_trim = true;
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
    pub(crate) fn library_preview_finished(
        &mut self,
        note: Id,
        updated: u64,
        result: Result<Option<(Page, usize)>, String>,
    ) {
        self.library_preview_pending.remove(&note);
        // Ignore previews for notes that were opened or edited meanwhile.
        if self.sessions.contains_key(&note)
            || !self
                .notes
                .iter()
                .any(|n| n.id == note && n.updated_at == updated)
        {
            return;
        }
        match result {
            Ok(Some((page, count))) => self.cache_library_preview(note, updated, page, count),
            Ok(None) => {}
            Err(_) => {
                self.library_preview_failed.insert(note, updated);
            }
        }
    }
}
