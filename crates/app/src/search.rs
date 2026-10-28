//! Library search and navigation to search results.
use super::*;

impl Controller {
    pub(crate) fn submit_search(&mut self, query: String, generation: u64) {
        match self.workers.submit(Job::Search {
            query,
            database: self.database.clone(),
            generation,
        }) {
            Ok(()) => self.busy += 1,
            Err(error) => self.search_state = SearchState::Failed(error),
        }
    }
    pub fn search(&mut self, query: String) {
        let query = query.trim().to_owned();
        self.search_query = query.clone();
        self.search_generation += 1;
        self.search_results.clear();
        self.pending_search = None;
        if query.is_empty() {
            self.search_state = SearchState::Idle;
            return;
        }
        self.search_state = SearchState::Searching;
        if self.saved >= self.queued && self.dirty_notes.is_empty() {
            self.submit_search(query, self.search_generation);
        } else {
            self.pending_search = Some((query, self.search_generation));
        }
    }
    /// Search waits until earlier edits are durably indexed.
    pub(crate) fn submit_pending_search(&mut self) -> bool {
        let Some((query, generation)) = self.pending_search.take() else {
            return false;
        };
        if self.saved >= self.queued || !self.settings.autosave {
            self.submit_search(query, generation);
            true
        } else {
            self.pending_search = Some((query, generation));
            false
        }
    }
    /// Open a search result and highlight its matches. If the note still has
    /// to load, navigation resumes once it arrives.
    pub fn navigate_search(&mut self, note: Id, page: Id) {
        self.switch_note(note);
        self.pending_navigation = Some((note, page));
        if let Some(s) = self.sessions.get_mut(&note) {
            s.show_page(page);
            let matcher = folio_search::TextMatcher::new(&self.search_query);
            let mut highlights: Vec<_> = s
                .page()
                .ordered_objects()
                .filter(|o| matcher.matches(o.searchable_text()) && !o.searchable_text().is_empty())
                .map(|o| o.bounds())
                .collect();
            highlights.extend(
                s.page()
                    .ink_text
                    .iter()
                    .filter(|entry| !entry.stale && matcher.matches(&entry.text))
                    .map(|entry| entry.bounds),
            );
            if let Some(r) = highlights.first() {
                s.viewport.pan = Point::new(
                    100. - r.min.x * s.viewport.zoom,
                    100. - r.min.y * s.viewport.zoom,
                );
            }
            self.search_highlights = highlights;
            self.pending_navigation = None;
        }
    }
}
