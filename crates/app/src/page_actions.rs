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
impl Controller {
    pub fn add_page(&mut self) {
        self.finish();
        let mut p = Page::new();
        p.properties = if self.page().properties.pdf.is_none() {
            self.page().properties.clone()
        } else {
            PageProperties {
                paper: self.settings.paper,
                ..PageProperties::default()
            }
        };
        let index = self.session().document.pages.len();
        self.commit(
            "Add page",
            vec![Change::Page {
                index,
                before: None,
                after: Some(p),
            }],
        );
        self.change_page(index);
    }
    pub fn delete_page(&mut self) {
        self.finish();
        if self.session().document.pages.len() < 2 {
            self.error = Some("A note must keep at least one page".into());
            return;
        }
        let index = self.session().page;
        let p = self.page().clone();
        self.commit(
            "Delete page",
            vec![Change::Page {
                index,
                before: Some(p),
                after: None,
            }],
        );
    }
    pub fn change_page(&mut self, index: usize) {
        self.finish();
        self.end_temporary_selection();
        let s = self.session_mut();
        let old_width = s.page().properties.width;
        s.page = index.min(s.document.pages.len() - 1);
        if !s.page().properties.infinite {
            s.viewport.pan.x += (old_width - s.page().properties.width) * s.viewport.zoom / 2.;
            s.viewport.pan.y = 36.;
        }
        s.selection.clear();
        s.refresh();
        self.cursor = None;
        self.search_highlights.clear();
    }
    pub fn paper(&mut self, paper: Paper) {
        let before = self.page().properties.clone();
        let mut after = before.clone();
        after.paper = paper;
        self.commit(
            "Change paper",
            vec![Change::Properties {
                page: self.page().id,
                before,
                after,
            }],
        );
    }
    /// Rebase onto the page in view without changing any page's screen position.
    pub fn synchronize_page_view(&mut self, width: f32, height: f32) {
        if self.interaction.is_some() || self.loading_note() {
            return;
        }
        let session = self.session();
        let Some(stack) = session.page_stack() else {
            return;
        };
        let active = session.page;
        let mut viewport = session.viewport;
        stack.clamp_vertical(&mut viewport, active, height);
        let target = stack.nearest(viewport, active, Point::new(width / 2., height / 2.));
        let viewport = stack.viewport(viewport, active, target);
        if target != active {
            self.change_page(target);
        }
        self.session_mut().viewport = viewport;
    }
    pub(crate) fn focus_page_at(&mut self, screen: Point) {
        let session = self.session();
        let Some(stack) = session.page_stack() else {
            return;
        };
        if let Some(target) = stack.hit(session.viewport, session.page, screen)
            && target != session.page
        {
            let viewport = stack.viewport(session.viewport, session.page, target);
            self.change_page(target);
            self.session_mut().viewport = viewport;
        }
    }
    pub fn page_size(&mut self, width: f32, height: f32, infinite: bool) {
        if !valid_page_size(width, height) {
            self.error = Some("Page sizes must be between 64 and 100000 units".into());
            return;
        }
        let before = self.page().properties.clone();
        let mut after = before.clone();
        after.width = width;
        after.height = height;
        after.infinite = infinite;
        self.commit(
            "Page size",
            vec![Change::Properties {
                page: self.page().id,
                before,
                after,
            }],
        );
    }
    pub fn insert_space(&mut self, at: f32, amount: f32, vertical: bool) {
        let ids = self
            .page()
            .ordered_objects()
            .filter(|o| {
                if vertical {
                    o.bounds().min.y >= at
                } else {
                    o.bounds().min.x >= at
                }
            })
            .map(|o| o.id())
            .collect::<HashSet<_>>();
        let t = if vertical {
            Transform::translate(0., amount)
        } else {
            Transform::translate(amount, 0.)
        };
        let changes = self.object_changes(&ids, |o| {
            let mut o = o.clone();
            o.set_transform(t.compose(o.transform()));
            Some(o)
        });
        self.commit("Insert handwriting space", changes);
    }
}
/// Page dimensions, in document units, that the editor and renderer support.
const PAGE_SIZE_RANGE: std::ops::RangeInclusive<f32> = 64.0..=100000.0;
/// Whether a finite page of this size is supported. Rejects NaN and infinity.
pub(crate) fn valid_page_size(width: f32, height: f32) -> bool {
    PAGE_SIZE_RANGE.contains(&width) && PAGE_SIZE_RANGE.contains(&height)
}
