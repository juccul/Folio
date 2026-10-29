//! ID-addressed library actions never accidentally modify the hidden editor note.
use super::*;
#[derive(Clone, Debug)]
pub enum NoteAction {
    Rename(String),
    Tags(Vec<String>),
    Favorite,
    Trash,
    Move(Option<Id>),
    Duplicate,
    ReceivePage {
        source: Id,
        page: Id,
    },
    Import(PathBuf),
    /// Restore organization fields recorded by library undo.
    Revert(Box<NoteMetadata>),
}
/// A library change that Undo can revert.
#[derive(Clone, Debug)]
pub struct LibraryUndo {
    pub note: Id,
    pub before: NoteMetadata,
    pub label: String,
    pub at: Instant,
}
impl Controller {
    pub fn manage_note(&mut self, id: Id, action: NoteAction) {
        if !self.notes.iter().any(|n| n.id == id) {
            self.error = Some("This document no longer exists".into());
            return;
        }
        if self.notes.iter().any(|n| n.id == id && n.trashed)
            && !matches!(
                action,
                NoteAction::Trash | NoteAction::Duplicate | NoteAction::Revert(_)
            )
        {
            self.status = "In Trash · restore this document to edit".into();
            return;
        }
        if !self.sessions.contains_key(&id) {
            if !self.queue_load(id) {
                return;
            }
            self.pending_actions.entry(id).or_default().push(action);
            return;
        }
        if id == self.active {
            self.finish();
        }
        match action {
            NoteAction::ReceivePage { source, page } => {
                if let Err(error) = self.move_loaded_page(source, page, id) {
                    self.error = Some(error);
                }
            }
            NoteAction::Duplicate => {
                self.duplicate_loaded_note(id);
            }
            NoteAction::Import(path) => self.import_loaded(id, path),
            NoteAction::Revert(metadata) => {
                let mut after = self.sessions[&id].document.metadata.clone();
                after.title = metadata.title;
                after.tags = metadata.tags;
                after.favorite = metadata.favorite;
                after.trashed = metadata.trashed;
                after.notebook = metadata
                    .notebook
                    .filter(|folder| self.notebooks.iter().any(|n| n.id == *folder));
                self.apply_metadata(id, after, None);
            }
            action => {
                let before = self.sessions[&id].document.metadata.clone();
                let mut after = before.clone();
                let label = match action {
                    NoteAction::Rename(title) => {
                        after.title = if title.trim().is_empty() {
                            "Untitled document".into()
                        } else {
                            title.trim().into()
                        };
                        format!("Renamed “{}”", after.title)
                    }
                    NoteAction::Tags(tags) => {
                        after.tags = tags;
                        format!("Changed tags of “{}”", after.title)
                    }
                    NoteAction::Favorite => {
                        after.favorite = !after.favorite;
                        if after.favorite {
                            format!("Added “{}” to favorites", after.title)
                        } else {
                            format!("Removed “{}” from favorites", after.title)
                        }
                    }
                    NoteAction::Trash => {
                        after.trashed = !after.trashed;
                        if after.trashed {
                            format!("Moved “{}” to Trash", after.title)
                        } else {
                            format!("Restored “{}”", after.title)
                        }
                    }
                    NoteAction::Move(folder) => {
                        if folder.is_some_and(|f| !self.notebooks.iter().any(|n| n.id == f)) {
                            self.error = Some("The destination folder no longer exists".into());
                            return;
                        }
                        after.notebook = folder;
                        match folder {
                            Some(folder) => {
                                format!("Moved “{}” to {}", after.title, self.folder_path(folder))
                            }
                            None => format!("Removed “{}” from its folder", after.title),
                        }
                    }
                    _ => unreachable!(),
                };
                self.apply_metadata(id, after, Some(label));
            }
        }
    }
    /// Change library organization (title, tags, favorite, Trash, folder)
    /// outside the document's undo history. `undo` records an entry that
    /// [`Self::undo_library`] can revert from the library or the editor.
    fn apply_metadata(&mut self, id: Id, mut after: NoteMetadata, undo: Option<String>) {
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        let before = session.document.metadata.clone();
        // Organizing a document is not an edit: keep its edit time.
        after.updated_at = before.updated_at;
        if after == before {
            return;
        }
        session.document.metadata = after.clone();
        let cmd = Command {
            label: "Change document details".into(),
            changes: vec![Change::Metadata {
                before: before.clone(),
                after: after.clone(),
            }],
        };
        let s = &self.sessions[&id];
        let delta = Delta::command_with_text(&s.document, &cmd, |page| {
            s.search_text_for(page).to_string()
        });
        if let Some(label) = undo {
            self.library_undo.push(LibraryUndo {
                note: id,
                before,
                label: label.clone(),
                at: Instant::now(),
            });
            if self.library_undo.len() > 64 {
                self.library_undo.remove(0);
            }
            self.status = label;
        }
        self.refresh_metadata(id);
        if id == self.active && self.read_only() {
            self.tool = Tool::Hand;
        }
        self.persist(delta);
    }
    /// The most recent library change that can be undone, if it is recent
    /// enough to offer an Undo button.
    pub fn library_undo_offer(&self) -> Option<&str> {
        self.library_undo
            .last()
            .filter(|entry| entry.at.elapsed() < Duration::from_secs(12))
            .map(|entry| entry.label.as_str())
    }
    pub fn can_undo_library(&self) -> bool {
        !self.library_undo.is_empty()
    }
    /// Revert the most recent rename, tag, favorite, Trash or folder change.
    pub fn undo_library(&mut self) {
        let Some(entry) = self.library_undo.pop() else {
            return;
        };
        if !self.notes.iter().any(|n| n.id == entry.note) {
            return;
        }
        self.manage_note(entry.note, NoteAction::Revert(Box::new(entry.before)));
        self.status = format!("Undid: {}", entry.label);
    }
    /// Permanently delete a document from Trash. This cannot be undone.
    pub fn delete_permanently(&mut self, id: Id) {
        if !self.notes.iter().any(|n| n.id == id && n.trashed) {
            self.error = Some("Only documents in Trash can be deleted permanently".into());
            return;
        }
        if self.requested_note() == id || self.active == id {
            self.finish();
        }
        if self.pending_note == Some(id) {
            self.pending_note = None;
            self.pending_navigation = None;
        }
        if self.active == id {
            // Show an unlisted empty document, as at first launch, rather
            // than adding a new document to the library.
            let mut document = Document::new("Untitled document");
            document.pages[0].properties = self.default_page_properties();
            let next = document.metadata.id;
            self.sessions.insert(next, Session::new(document));
            self.activate_note(next);
            self.bootstrap_document = Some(next);
            self.provisional_imports.insert(next);
        }
        self.sessions.remove(&id);
        self.pending_actions.remove(&id);
        self.dirty_notes.remove(&id);
        self.closed_sessions.remove(&id);
        self.notes.retain(|n| n.id != id);
        self.library_undo.retain(|entry| entry.note != id);
        self.settings.workspace.open_tabs.retain(|tab| *tab != id);
        self.settings
            .workspace
            .current_pages
            .retain(|(note, _)| *note != id);
        self.persistence.delete_note(id);
        self.store_settings();
        self.status = "Deleted permanently".into();
    }
    /// Permanently delete every document in Trash, or only those from one
    /// folder.
    pub fn empty_trash(&mut self, folder: Option<Id>) {
        let ids: Vec<_> = self
            .notes
            .iter()
            .filter(|n| n.trashed && (folder.is_none() || n.notebook == folder))
            .map(|n| n.id)
            .collect();
        for id in &ids {
            self.delete_permanently(*id);
        }
        self.status = match ids.len() {
            0 => "Trash is empty".into(),
            1 => "Deleted 1 document permanently".into(),
            n => format!("Deleted {n} documents permanently"),
        };
    }
}
