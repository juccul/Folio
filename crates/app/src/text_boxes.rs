//! Typed text boxes.
use super::*;

impl Controller {
    pub fn add_text(&mut self, text: String, position: Point) {
        if !text.is_empty() {
            self.insert_text_box(text, position);
        }
    }
    pub fn create_text_box(&mut self, position: Point) -> Id {
        if self.read_only() {
            return Id::nil();
        }
        self.insert_text_box(String::new(), position)
    }
    fn insert_text_box(&mut self, text: String, position: Point) -> Id {
        let object = Object::Text(TextBlock {
            id: Id::new_v4(),
            text,
            rect: Rect::new(position.x, position.y, 360., 160.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 20.,
            color: self.style.color,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        });
        let id = object.id();
        self.commit(
            "Insert text",
            vec![Change::Object {
                page: self.page().id,
                id,
                before: None,
                after: Some(Arc::new(object)),
                index: self.page().order.len(),
            }],
        );
        self.session_mut().selection = HashSet::from([id]);
        self.pending_text = None;
        id
    }
    pub fn edit_text(&mut self, id: Id, change: impl FnOnce(&mut TextBlock)) {
        let Some(before) = self.page().objects.get(&id).cloned() else {
            return;
        };
        let mut after = before.as_ref().clone();
        if let Object::Text(t) = &mut after {
            change(t);
            if &after == before.as_ref() {
                return;
            }
            let index = self.page().order.iter().position(|v| *v == id).unwrap_or(0);
            self.commit(
                "Edit text",
                vec![Change::Object {
                    page: self.page().id,
                    id,
                    before: Some(before),
                    after: Some(Arc::new(after)),
                    index,
                }],
            );
        }
    }
}
impl Controller {
    /// Commit an inline editor's text as one undoable edit. A box left empty
    /// is removed; one created in this editing session leaves no history.
    pub fn finish_text_edit(&mut self, note: Id, page: Id, id: Id, text: String) {
        let Some(session) = self.sessions.get(&note) else {
            return;
        };
        let Some(p) = session.document.pages.iter().find(|p| p.id == page) else {
            return;
        };
        let Some(before) = p.objects.get(&id).cloned() else {
            return;
        };
        let Object::Text(block) = before.as_ref() else {
            return;
        };
        let index = p.order.iter().position(|v| *v == id).unwrap_or(0);
        let fresh = note == self.active
            && session.page().id == page
            && session.history.last().is_some_and(|cmd| {
                // Only a box inserted empty by the editor, with no edits since.
                matches!(cmd.changes.as_slice(),
                        [Change::Object { id: inserted, before: None, after: Some(object), .. }]
                            if *inserted == id
                                && matches!(object.as_ref(), Object::Text(t) if t.text.is_empty()))
            });
        if text.trim().is_empty() {
            if fresh {
                self.undo();
                if let Some(session) = self.sessions.get_mut(&note) {
                    session.history.forget_redo();
                    session.history_needs_save = true;
                    session.selection.remove(&id);
                }
                let delta = Delta::full(&self.sessions[&note].document);
                self.persist(delta);
            } else {
                self.commit_to(
                    note,
                    "Delete text",
                    vec![Change::Object {
                        page,
                        id,
                        before: Some(before),
                        after: None,
                        index,
                    }],
                );
                if let Some(session) = self.sessions.get_mut(&note) {
                    session.selection.remove(&id);
                }
            }
            return;
        }
        if block.text == text {
            return;
        }
        let mut after = block.clone();
        after.text = text;
        if fresh {
            // A box created by this editing session is inserted with its text
            // in one step, so undo removes it rather than leaving it empty.
            self.undo();
            if let Some(session) = self.sessions.get_mut(&note) {
                session.history.forget_redo();
            }
            self.commit_to(
                note,
                "Insert text",
                vec![Change::Object {
                    page,
                    id,
                    before: None,
                    after: Some(Arc::new(Object::Text(after))),
                    index,
                }],
            );
            if let Some(session) = self.sessions.get_mut(&note) {
                session.selection = HashSet::from([id]);
            }
            return;
        }
        self.commit_to(
            note,
            "Edit text",
            vec![Change::Object {
                page,
                id,
                before: Some(before),
                after: Some(Arc::new(Object::Text(after))),
                index,
            }],
        );
    }
}
