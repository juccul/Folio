//! Committing commands, undo/redo and handing document changes to persistence.
use super::*;

impl Controller {
    fn defer_history_save(&mut self, note: Id) {
        if let Some(session) = self.sessions.get_mut(&note) {
            session.history_needs_save = true;
        }
        self.status = "Unsaved · autosave is off".into();
    }
    pub(crate) fn persist(&mut self, mut delta: Delta) {
        let id = delta.metadata.id;
        if !self.settings.autosave {
            self.defer_history_save(id);
            return;
        }
        if let Some(session) = self.sessions.get_mut(&id) {
            if self.dirty_notes.contains(&id) {
                delta = Delta::full(&session.document);
                delta.journal = vec![JournalEvent::Replace(session.history.clone())];
            } else if session.history_needs_save {
                delta.journal = vec![JournalEvent::Replace(session.history.clone())];
            }
        }
        match self.persistence.save(delta) {
            Ok(seq) => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    session.history_needs_save = false;
                }
                self.save_notes.insert(seq, id);
                self.dirty_notes.remove(&id);
                self.queued = seq;
                self.status = "Saving…".into();
            }
            Err(e) => {
                self.dirty_notes.insert(id);
                if let Some(s) = self.sessions.get_mut(&id) {
                    s.history_needs_save = true;
                }
                self.status = "Changes are not saved".into();
                self.save_error = Some(e);
            }
        }
    }
    fn refresh_metadata(&mut self, id: Id) {
        if let Some(session) = self.sessions.get(&id)
            && let Some(index) = self.notes.iter().position(|n| n.id == id)
        {
            self.notes[index] = session.document.metadata.clone();
            let edited = self.notes[index].updated_at;
            // Most strokes update the already-first note. Sort only when its
            // new timestamp crosses a neighbor, preserving stable ties.
            if (index == 0 || self.notes[index - 1].updated_at >= edited)
                && (index + 1 == self.notes.len() || self.notes[index + 1].updated_at <= edited)
            {
                return;
            }
        }
        self.notes.sort_by_key(|n| std::cmp::Reverse(n.updated_at));
    }
    pub fn commit(&mut self, label: &str, changes: Vec<Change>) {
        self.commit_to(self.active, label, changes)
    }
    pub(crate) fn commit_to(&mut self, note: Id, label: &str, mut changes: Vec<Change>) {
        if self
            .sessions
            .get(&note)
            .is_some_and(|s| s.document.metadata.trashed)
        {
            let restoring = changes.len() == 1
                && matches!(&changes[0], Change::Metadata {before,after} if {
                    let mut restored = before.clone(); restored.trashed = false; before.trashed && *after == restored
                });
            if !restoring {
                self.status = "In Trash · restore this document to edit".into();
                return;
            }
        }
        if changes.is_empty() {
            return;
        }
        if let Some(session) = self.sessions.get(&note) {
            handwriting_search::maintain_index(&session.document, &mut changes);
        }
        let provisional = self.provisional_imports.remove(&note);
        let first_writing = self.bootstrap_document == Some(note);
        if first_writing {
            self.bootstrap_document = None;
            if let Some(session) = self.sessions.get(&note) {
                self.notes.insert(0, session.document.metadata.clone());
            }
        }
        let cmd = Command {
            label: label.into(),
            changes,
        };
        let autosave = self.settings.autosave;
        let Some(s) = self.sessions.get_mut(&note) else {
            return;
        };
        s.history.execute(cmd.clone(), &mut s.document);
        s.refresh_command(&cmd);
        self.invalidate_previews(note, &cmd);
        if note == self.active {
            self.math_ink_edited(&cmd);
        }
        let s = &self.sessions[&note];
        // Deferred edits do not need a page header/search-text snapshot yet.
        let delta = autosave.then(|| {
            let mut delta = if provisional {
                Delta::full(&s.document)
            } else {
                Delta::command_with_text(&s.document, &cmd, |page| {
                    s.search_text_for(page).to_string()
                })
            };
            delta.journal.push(JournalEvent::Execute(cmd));
            delta
        });
        self.refresh_metadata(note);
        if note == self.active && self.read_only() {
            self.tool = Tool::Hand;
        }
        if let Some(delta) = delta {
            self.persist(delta);
        } else {
            self.defer_history_save(note);
        }
        if first_writing {
            self.mark_note_opened(note);
        }
    }
    pub fn undo(&mut self) {
        self.step_history(false);
    }
    pub fn redo(&mut self) {
        self.step_history(true);
    }
    /// Undo or redo one command on the active document, keeping the current
    /// page in view if the command added or removed pages.
    fn step_history(&mut self, redo: bool) {
        if self.read_only() {
            return;
        }
        self.cancel();
        let autosave = self.settings.autosave;
        let s = self.session_mut();
        let current_page = s.page().id;
        let (cmd, event) = if redo {
            (s.history.redo(&mut s.document), JournalEvent::Redo)
        } else {
            (s.history.undo(&mut s.document), JournalEvent::Undo)
        };
        let Some(cmd) = cmd else {
            return;
        };
        if cmd
            .changes
            .iter()
            .any(|change| matches!(change, Change::Page { .. }))
            && let Some(index) = s.document.pages.iter().position(|p| p.id == current_page)
        {
            s.page = index;
        }
        s.refresh_command(&cmd);
        let delta = autosave.then(|| {
            let mut delta = Delta::command_with_text(&s.document, &cmd, |page| {
                s.search_text_for(page).to_string()
            });
            delta.journal.push(event);
            delta
        });
        self.invalidate_previews(self.active, &cmd);
        self.math_ink_edited(&cmd);
        self.refresh_metadata(self.active);
        if let Some(delta) = delta {
            self.persist(delta);
        } else {
            self.defer_history_save(self.active);
        }
    }
    /// Persist `delta` now, even when autosave is off.
    fn persist_now(&mut self, delta: Delta) {
        let autosave = self.settings.autosave;
        self.settings.autosave = true;
        self.persist(delta);
        self.settings.autosave = autosave;
    }
    pub fn save(&mut self) {
        if self.provisional_imports.contains(&self.active) {
            return;
        }
        self.persist_now(Delta::full(&self.session().document));
    }
    pub fn flush(&mut self) -> Result<(), String> {
        self.finish();
        let documents = self
            .sessions
            .values()
            .filter(|s| !self.provisional_imports.contains(&s.document.metadata.id))
            .map(|s| {
                let mut delta = Delta::full(&s.document);
                delta.journal = vec![JournalEvent::Replace(s.history.clone())];
                delta
            })
            .collect::<Vec<_>>();
        for delta in documents {
            let note = delta.metadata.id;
            let sequence = self.persistence.save_blocking(delta)?;
            self.queued = sequence;
            self.save_notes.insert(sequence, note);
            self.dirty_notes.remove(&note);
            if let Some(session) = self.sessions.get_mut(&note) {
                session.history_needs_save = false;
            }
        }
        self.persistence.flush()
    }
    pub fn set_autosave(&mut self, enabled: bool) {
        let previously = self.settings.autosave;
        self.settings.autosave = enabled;
        if enabled && !previously {
            let deltas = self
                .sessions
                .values()
                .filter(|s| !self.provisional_imports.contains(&s.document.metadata.id))
                .map(|s| Delta::full(&s.document))
                .collect::<Vec<_>>();
            for delta in deltas {
                self.persist(delta);
            }
        }
        self.store_settings();
    }
    pub fn retry_save(&mut self) {
        let notes = self.dirty_notes.iter().copied().collect::<Vec<_>>();
        for note in notes {
            if let Some(session) = self.sessions.get(&note) {
                self.persist_now(Delta::full(&session.document));
            }
        }
    }
    /// Apply acknowledgements from the persistence writer.
    pub(crate) fn poll_save_receipts(&mut self) -> bool {
        let mut changed = false;
        while let Ok(receipt) = self.persistence.receipts.try_recv() {
            changed = true;
            let note = self.save_notes.remove(&receipt.sequence);
            match receipt.result {
                Ok(()) => {
                    self.saved = self.saved.max(receipt.sequence);
                    if self.saved >= self.queued
                        && self.dirty_notes.is_empty()
                        && self
                            .sessions
                            .values()
                            .all(|session| !session.history_needs_save)
                    {
                        self.save_error = None;
                        self.status = "All changes saved".into()
                    }
                }
                Err(e) => {
                    if let Some(note) = note {
                        self.dirty_notes.insert(note);
                    }
                    self.status = "Changes are not saved".into();
                    self.save_error = Some(e)
                }
            }
        }
        changed
    }
    /// Periodically resubmit documents whose last save failed.
    pub(crate) fn retry_dirty_saves(&mut self) {
        if self.settings.autosave
            && !self.dirty_notes.is_empty()
            && self.last_retry.elapsed() > Duration::from_secs(2)
            && self.interaction.is_none()
        {
            self.last_retry = Instant::now();
            let notes = self.dirty_notes.iter().copied().collect::<Vec<_>>();
            for note in notes {
                if let Some(s) = self.sessions.get(&note) {
                    self.persist(Delta::full(&s.document));
                }
            }
        }
    }
    pub fn recovery_checkpoint(&mut self) {
        self.save();
        self.persistence.checkpoint();
        self.status = "Creating recovery snapshot…".into();
    }
}
