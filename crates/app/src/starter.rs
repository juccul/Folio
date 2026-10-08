use super::*;
impl Controller {
    pub(super) fn begin_bootstrap_writing(&mut self) {
        let Some(id) = self.bootstrap_document.take() else {
            return;
        };
        self.provisional_imports.remove(&id);
        let doc = &self.sessions[&id].document;
        self.notes.insert(0, doc.metadata.clone());
        let delta = Delta::full(doc);
        self.persist(delta);
        self.mark_note_opened(id);
    }
    pub(super) fn start_empty_session(&mut self) {
        let mut doc = Document::new("Untitled document");
        doc.pages[0].properties = self.default_page_properties();
        let id = doc.metadata.id;
        self.sessions.insert(id, Session::new(doc));
        self.activate_note(id);
        self.bootstrap_document = Some(id);
        self.provisional_imports.insert(id);
        self.pending_note = None;
        self.pending_navigation = None;
        self.tool = Tool::Pen;
    }
    /// An empty session keeps the editor usable without creating a library document.
    pub fn is_bootstrap_document(&self, id: Id) -> bool {
        self.bootstrap_document == Some(id)
    }
    /// An explicit editable tutorial; opening Help alone never creates a note.
    pub fn create_starter_notebook(&mut self) {
        if let Some(id) = self
            .notes
            .iter()
            .find(|n| {
                !n.trashed
                    && (Some(n.id) == self.settings.starter_document
                        || n.title == "Welcome to Folio")
            })
            .map(|n| n.id)
        {
            self.settings.starter_document = Some(id);
            self.store_settings();
            self.switch_note(id);
            return;
        }
        self.create_note();
        self.settings.starter_document = Some(self.active);
        self.store_settings();
        self.rename("Welcome to Folio".into());
        let pages = [
            (
                "Write and edit",
                "Try your pen in the space below.
Hold still at a stroke endpoint for about half a second to snap a line, circle or rectangle.
P: pen · E: eraser · L: lasso · Ctrl+Z: undo.
Select ink, then drag it or use its resize and rotation handles.",
            ),
            (
                "Recognize handwriting",
                "Write a short sentence below, then use Lasso to select it.
Choose Recognize text. Review and correct the result.
Copy text keeps the writing; Replace writing inserts editable text and supports undo.
Recognition asks before downloading its optional 1.47 GB model pack; your pages stay local.",
            ),
            (
                "Solve a problem",
                "Select the equation below and choose Solve.
The Math solver shows an answer, steps, graphs and Check work.
Guide me presents one step at a time. Add answer puts the result on the page.
Try changing 2*x + 3 = 11 to another equation.",
            ),
            (
                "Organize your notebook",
                "Ctrl+Shift+P opens the page strip.
Drag pages to reorder; duplicate a page or give it a named bookmark.
Ctrl+F searches titles, tags, typed text and equation source.
Import a PDF from the library; export an annotated PDF from your notebook.
Ctrl+, opens pen, appearance, tablet-pad and accessibility preferences.",
            ),
        ];
        for (index, (title, body)) in pages.into_iter().enumerate() {
            if index > 0 {
                self.add_page();
            }
            self.bookmark_page(title.into());
            self.add_text(title.into(), Point::new(48., 48.));
            self.add_text(body.into(), Point::new(48., 110.));
            if index == 2 {
                self.add_text("2*x + 3 = 11".into(), Point::new(48., 330.));
            }
        }
        self.change_page(0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_starter_is_editable_and_does_not_replace_existing_notes() {
        let root = std::env::temp_dir().join(format!("folio-starter-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        app.add_text("Existing work".into(), Point::new(10., 10.));
        let old = app.active;
        app.create_starter_notebook();
        assert_ne!(app.active, old);
        assert_eq!(app.sessions[&old].document.pages[0].text(), "Existing work");
        assert_eq!(app.session().document.pages.len(), 4);
        assert!(
            app.session().document.pages[2]
                .text()
                .contains("2*x + 3 = 11")
        );
        app.flush().unwrap();
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod onboarding_tests {
    use super::*;
    #[test]
    fn first_launch_stays_empty_until_an_intentional_action() {
        let root = std::env::temp_dir().join(format!("folio-onboarding-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let placeholder = app.active;
        assert!(app.visible_notes().is_empty());
        assert!(app.notes.is_empty());
        app.save();
        app.flush().unwrap();
        assert!(
            Store::open(&root.join("notes.sqlite3"))
                .unwrap()
                .list_notes()
                .unwrap()
                .is_empty()
        );
        app.create_note();
        assert!(!app.sessions.contains_key(&placeholder));
        assert_eq!(app.visible_notes().len(), 1);
        app.create_starter_notebook();
        let starter = app.active;
        app.rename("My tutorial".into());
        app.create_starter_notebook();
        assert_eq!(app.active, starter);
        assert_eq!(app.visible_notes().len(), 2);
        app.flush().unwrap();
        drop(app);
        let mut app = Controller::open(root.clone()).unwrap();
        app.create_starter_notebook();
        while app.has_background_work() {
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.active, starter);
        assert_eq!(app.visible_notes().len(), 2);
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn writing_materializes_the_ephemeral_document_with_its_page() {
        let root = std::env::temp_dir().join(format!("folio-first-writing-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let id = app.active;
        let page = app.page().id;
        app.add_text("First thought".into(), Point::new(10., 20.));
        assert!(!app.is_bootstrap_document(id));
        assert_eq!(app.visible_notes().len(), 1);
        app.flush().unwrap();
        drop(app);
        let app = Controller::open(root.clone()).unwrap();
        assert_eq!(app.active, id);
        assert_eq!(app.page().id, page);
        assert_eq!(app.page().text(), "First thought");
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod import_empty_tests {
    use super::*;
    #[test]
    fn removing_the_only_failed_import_restores_an_empty_library() {
        let root = std::env::temp_dir().join(format!("folio-empty-import-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let id = app.import_as_note(root.join("missing.pdf"));
        let start = Instant::now();
        while app.has_background_work() {
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
            assert!(start.elapsed() < Duration::from_secs(10));
        }
        app.dismiss_failed_import(id);
        assert!(app.notes.is_empty());
        assert!(app.visible_notes().is_empty());
        assert!(app.is_bootstrap_document(app.active));
        app.flush().unwrap();
        drop(app);
        let app = Controller::open(root.clone()).unwrap();
        assert!(app.notes.is_empty());
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
}
