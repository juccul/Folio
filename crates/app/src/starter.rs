use super::*;
impl Controller {
    /// An explicit editable tutorial; opening Help alone never creates a note.
    pub fn create_starter_notebook(&mut self) {
        self.create_note();
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
The optional model downloads on first use; your pages stay local.",
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
