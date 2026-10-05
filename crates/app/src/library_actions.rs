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
    Import(PathBuf),
}
impl Controller {
    pub fn manage_note(&mut self, id: Id, action: NoteAction) {
        if !self.notes.iter().any(|n| n.id == id) {
            self.error = Some("This document no longer exists".into());
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
            NoteAction::Duplicate => {
                self.duplicate_loaded_note(id);
            }
            NoteAction::Import(path) => self.import_loaded(id, path),
            action => {
                let before = self.sessions[&id].document.metadata.clone();
                let mut after = before.clone();
                let label = match action {
                    NoteAction::Rename(title) => {
                        after.title = if title.trim().is_empty() {
                            "Untitled note".into()
                        } else {
                            title.trim().into()
                        };
                        "Rename note"
                    }
                    NoteAction::Tags(tags) => {
                        after.tags = tags;
                        "Change tags"
                    }
                    NoteAction::Favorite => {
                        after.favorite = !after.favorite;
                        "Change favorite"
                    }
                    NoteAction::Trash => {
                        after.trashed = !after.trashed;
                        "Trash or restore note"
                    }
                    NoteAction::Move(folder) => {
                        if folder.is_some_and(|f| !self.notebooks.iter().any(|n| n.id == f)) {
                            self.error = Some("The destination folder no longer exists".into());
                            return;
                        }
                        after.notebook = folder;
                        "Move note"
                    }
                    _ => unreachable!(),
                };
                self.commit_to(id, label, vec![Change::Metadata { before, after }]);
            }
        }
    }
}
