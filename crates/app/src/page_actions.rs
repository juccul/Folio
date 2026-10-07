use super::*;
impl Controller {
    pub fn reorder_page(&mut self, page: Id, target: Id) {
        self.finish();
        let pages = &self.session().document.pages;
        let (Some(from), Some(to)) = (
            pages.iter().position(|p| p.id == page),
            pages.iter().position(|p| p.id == target),
        ) else {
            return;
        };
        if from == to {
            return;
        }
        let current = self.page().id;
        let moved = pages[from].clone();
        self.commit(
            "Reorder page",
            vec![
                Change::Page {
                    index: from,
                    before: Some(moved.clone()),
                    after: None,
                },
                Change::Page {
                    index: to,
                    before: None,
                    after: Some(moved),
                },
            ],
        );
        if let Some(index) = self
            .session()
            .document
            .pages
            .iter()
            .position(|p| p.id == current)
        {
            self.change_page(index);
        }
    }
    pub fn duplicate_page(&mut self) {
        self.finish();
        let index = self.session().page + 1;
        let page = self.page().duplicate();
        self.commit(
            "Duplicate page",
            vec![Change::Page {
                index,
                before: None,
                after: Some(page),
            }],
        );
        self.change_page(index);
    }
    pub fn bookmark_page(&mut self, name: String) {
        let before = self.page().properties.clone();
        let mut after = before.clone();
        after.bookmark = if name.trim().is_empty() {
            None
        } else {
            Some(name.trim().chars().take(120).collect())
        };
        self.commit(
            "Name page bookmark",
            vec![Change::Properties {
                page: self.page().id,
                before,
                after,
            }],
        );
    }
    pub fn move_page_to(&mut self, destination: Id) {
        if destination == self.active {
            return;
        }
        self.finish();
        self.manage_note(
            destination,
            NoteAction::ReceivePage {
                source: self.active,
                page: self.page().id,
            },
        );
    }
    pub(super) fn move_loaded_page(
        &mut self,
        source: Id,
        page: Id,
        destination: Id,
    ) -> Result<(), String> {
        if source == destination {
            return Ok(());
        }
        let source_session = self
            .sessions
            .get(&source)
            .ok_or("Source notebook is no longer open")?;
        let index = source_session
            .document
            .pages
            .iter()
            .position(|p| p.id == page)
            .ok_or("Source page no longer exists")?;
        let original = source_session.document.pages[index].clone();
        if self.sessions[&destination].document.metadata.trashed
            || source_session.document.metadata.trashed
        {
            return Err("Restore notebooks before moving pages".into());
        }
        // Save the destination copy before removing the source. A failed write or
        // interrupted operation can leave a copy, but cannot lose the only page.
        self.flush()?;
        let target = self.sessions[&destination].document.pages.len();
        self.commit_to(
            destination,
            "Receive moved page",
            vec![Change::Page {
                index: target,
                before: None,
                after: Some(original.duplicate()),
            }],
        );
        self.flush()?;
        let replacement = (self.sessions[&source].document.pages.len() == 1).then(Page::new);
        self.commit_to(
            source,
            "Move page (undo restores source)",
            vec![Change::Page {
                index,
                before: Some(original),
                after: replacement,
            }],
        );
        self.flush()?;
        self.status = "Page moved · undo in the source notebook restores a copy".into();
        Ok(())
    }
}
