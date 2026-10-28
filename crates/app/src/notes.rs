//! Creating, loading, switching and renaming documents.
use super::*;

impl Controller {
    pub fn mark_note_opened(&mut self, id: Id) {
        if self.is_bootstrap_document(id) || !self.notes.iter().any(|n| n.id == id && !n.trashed) {
            return;
        }
        self.settings.recent_documents.retain(|n| *n != id);
        self.settings.recent_documents.insert(0, id);
        self.settings.recent_documents.truncate(20);
        self.store_settings();
    }
    pub fn visible_notes(&self) -> Vec<&NoteMetadata> {
        let mut notes: Vec<_> = self
            .notes
            .iter()
            .filter(|n| !self.is_bootstrap_document(n.id))
            .filter(|n| match self.filter {
                NoteFilter::Trash => n.trashed,
                NoteFilter::NotebookTrash(id) => n.trashed && n.notebook == Some(id),
                NoteFilter::Favorites => !n.trashed && n.favorite,
                NoteFilter::Notebook(id) => !n.trashed && n.notebook == Some(id),
                NoteFilter::Recent => !n.trashed && self.settings.recent_documents.contains(&n.id),
                _ => !n.trashed,
            })
            .collect();
        if self.filter == NoteFilter::Recent {
            notes.sort_by_key(|n| {
                self.settings
                    .recent_documents
                    .iter()
                    .position(|id| *id == n.id)
            });
        }
        notes
    }
    pub fn default_page_properties(&self) -> PageProperties {
        let mut properties = self
            .settings
            .default_page
            .clone()
            .filter(|p| page_actions::valid_page_size(p.width, p.height))
            .unwrap_or(PageProperties {
                paper: self.settings.paper,
                ..Default::default()
            });
        properties.pdf = None;
        properties.bookmark = None;
        properties
    }
    pub fn create_note(&mut self) {
        self.create_note_with_properties(
            "Untitled document".into(),
            self.default_page_properties(),
        )
        .expect("Valid default page properties");
    }
    pub fn create_note_with_properties(
        &mut self,
        title: String,
        properties: PageProperties,
    ) -> Result<Id, String> {
        let id = self.create_note_with_properties_inner(title, properties.clone(), true)?;
        self.settings.paper = properties.paper;
        self.settings.default_page = Some(properties);
        self.store_settings();
        Ok(id)
    }
    pub(crate) fn create_note_with_properties_inner(
        &mut self,
        title: String,
        properties: PageProperties,
        persist: bool,
    ) -> Result<Id, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Give your document a name.".into());
        }
        if !page_actions::valid_page_size(properties.width, properties.height)
            || properties.pdf.is_some()
        {
            return Err("Choose a valid canvas size.".into());
        }
        self.finish();
        self.end_temporary_selection();
        self.pending_note = None;
        self.pending_navigation = None;
        let mut d = Document::new(title);
        d.pages[0].properties = properties;
        if let NoteFilter::Notebook(id) = self.filter {
            d.metadata.notebook = Some(id)
        }
        let id = d.metadata.id;
        let delta = Delta::full(&d);
        self.notes.insert(0, d.metadata.clone());
        self.sessions.insert(id, Session::new(d));
        self.activate_note(id);
        self.filter = NoteFilter::All;
        self.search_highlights.clear();
        self.mark_note_opened(id);
        if persist {
            self.persist(delta);
        } else {
            self.provisional_imports.insert(id);
        }
        Ok(id)
    }
    pub fn requested_note(&self) -> Id {
        self.pending_note.unwrap_or(self.active)
    }
    pub fn loading_note(&self) -> bool {
        self.pending_note.is_some()
    }
    pub(crate) fn activate_note(&mut self, id: Id) {
        let previous = self.active;
        self.closed_sessions.remove(&id);
        self.session_recency.retain(|note| *note != id);
        self.session_recency.push_back(id);
        if previous != id
            && let Some(session) = self.sessions.get(&previous)
        {
            session.memory_usage.set(None);
        }
        self.active = id;
        if previous != id && self.bootstrap_document == Some(previous) {
            self.bootstrap_document = None;
            self.notes.retain(|n| n.id != previous);
        }
        if previous != id && !self.notes.iter().any(|n| n.id == previous) {
            self.sessions.remove(&previous);
            self.provisional_imports.remove(&previous);
        }
    }
    pub fn switch_note(&mut self, id: Id) {
        self.finish();
        self.end_temporary_selection();
        self.pending_navigation = None;
        self.search_highlights.clear();
        self.pending_note = None;
        if self.sessions.contains_key(&id) {
            self.show_note(id);
            self.cursor = None;
        } else if self.queue_load(id) {
            self.pending_note = Some(id);
        }
    }
    /// Make an already loaded session the visible document.
    fn show_note(&mut self, id: Id) {
        self.activate_note(id);
        self.mark_note_opened(id);
        if self.read_only() {
            self.tool = Tool::Hand;
        }
    }
    /// Install a document loaded by a worker, restoring its last view, and
    /// show it if it is the tab the user is waiting for.
    pub(crate) fn note_loaded(&mut self, document: Document, history: History) {
        let id = document.metadata.id;
        let view = self
            .session_views
            .iter()
            .position(|(note, _, _)| *note == id)
            .and_then(|index| self.session_views.remove(index));
        self.library_previews.remove(&id);
        self.library_preview_recency.retain(|note| *note != id);
        self.loading_notes.remove(&id);
        self.session_recency.retain(|n| *n != id);
        self.session_recency.push_back(id);
        if !self.sessions.contains_key(&id) {
            let mut session = Session::new(document);
            session.history = history;
            let page = view
                .map(|(_, page, _)| page)
                .or_else(|| self.settings.remembered_page(id));
            if let Some(page) = page {
                session.show_page(page);
            }
            if let Some((_, _, viewport)) = view {
                session.viewport = viewport;
            }
            self.sessions.insert(id, session);
        }
        if let Some(actions) = self.pending_actions.remove(&id) {
            for action in actions {
                self.manage_note(id, action);
            }
        }
        if self.pending_note == Some(id) {
            self.finish();
            self.show_note(id);
            self.pending_note = None;
            if let Some((note, page)) = self.pending_navigation.take() {
                self.navigate_search(note, page);
            }
        }
    }
    pub(crate) fn queue_load(&mut self, id: Id) -> bool {
        if self.loading_notes.contains(&id) {
            return true;
        }
        match self.workers.submit(Job::Load {
            id,
            database: self.database.clone(),
        }) {
            Ok(()) => {
                self.loading_notes.insert(id);
                self.busy += 1;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }
    pub fn rename(&mut self, title: String) {
        let before = self.session().document.metadata.clone();
        let mut after = before.clone();
        after.title = if title.trim().is_empty() {
            "Untitled note".into()
        } else {
            title.trim().into()
        };
        self.commit("Rename note", vec![Change::Metadata { before, after }]);
    }
    pub fn metadata(&mut self, change: impl FnOnce(&mut NoteMetadata)) {
        let before = self.session().document.metadata.clone();
        let mut after = before.clone();
        change(&mut after);
        self.commit(
            "Change note details",
            vec![Change::Metadata { before, after }],
        );
    }
    pub fn duplicate_note(&mut self) {
        self.finish();
        self.pending_note = None;
        self.pending_navigation = None;
        let id = self.duplicate_loaded_note(self.active);
        self.activate_note(id);
    }
    pub(crate) fn duplicate_loaded_note(&mut self, source: Id) -> Id {
        let old = self.sessions[&source].document.clone();
        let mut d = Document::new(format!("{} (copy)", old.metadata.title));
        d.metadata.tags = old.metadata.tags;
        d.metadata.notebook = old.metadata.notebook;
        let mut cover = None;
        d.pages = old
            .pages
            .iter()
            .map(|page| {
                let copy = page.duplicate();
                if Some(page.id) == old.metadata.cover_page {
                    cover = Some(copy.id);
                }
                copy
            })
            .collect();
        d.metadata.cover_page = cover;
        let id = d.metadata.id;
        self.notes.insert(0, d.metadata.clone());
        let delta = Delta::full(&d);
        self.sessions.insert(id, Session::new(d));
        self.persist(delta);
        id
    }
}
