use super::*;
impl Session {
    pub fn page_text(&self) -> Arc<str> {
        self.search_text_for(self.page())
    }
    pub(super) fn search_text_for(&self, page: &Page) -> Arc<str> {
        let mut cached = self.search_text.borrow_mut();
        let entry = cached
            .entry(page.id)
            .or_insert_with(|| (page.revision, Arc::from(page.text())));
        if entry.0 != page.revision {
            *entry = (page.revision, Arc::from(page.text()));
        }
        entry.1.clone()
    }
    pub(super) fn refresh_search_text(&mut self, command: &Command) {
        let cached = self.search_text.get_mut();
        if cached.is_empty() {
            return;
        }
        if command
            .changes
            .iter()
            .any(|c| matches!(c, Change::Page { .. }))
        {
            cached.clear();
            return;
        }
        fn sources(object: &Option<Arc<Object>>) -> &[Id] {
            match object.as_deref() {
                Some(Object::Shape(shape)) => shape.source_strokes.as_slice(),
                Some(Object::Equation(equation)) => equation.source_strokes.as_slice(),
                _ => &[],
            }
        }
        fn text(object: &Option<Arc<Object>>) -> &str {
            object.as_ref().map_or("", |o| o.searchable_text())
        }
        let mut pages = HashMap::new();
        for change in &command.changes {
            let id = match change {
                Change::Object { page, .. }
                | Change::InkText { page, .. }
                | Change::Properties { page, .. }
                | Change::Groups { page, .. } => *page,
                _ => continue,
            };
            if !cached.contains_key(&id) {
                continue;
            }
            let current = *pages.entry(id).or_insert_with(|| self.document.page(id));
            let (page, invalid) = match change {
                Change::Object {
                    page,
                    before,
                    after,
                    ..
                } => {
                    let annotations = current.is_some_and(|p| !p.ink_text.is_empty());
                    (
                        *page,
                        text(before) != text(after)
                            || annotations
                                && (before.is_some() != after.is_some()
                                    || sources(before) != sources(after)),
                    )
                }
                Change::InkText { page, .. } => (*page, true),
                Change::Properties { page, .. } | Change::Groups { page, .. } => (*page, false),
                _ => continue,
            };
            if invalid {
                cached.remove(&page);
            } else if let Some(entry) = cached.get_mut(&page)
                && let Some(page) = current
            {
                entry.0 = page.revision;
            }
        }
    }
}
impl Controller {
    pub fn index_selected_handwriting(&mut self) -> Result<(), String> {
        self.recognize_selection(RecognitionKind::Text)?;
        self.recognition_for_index = true;
        Ok(())
    }
    pub fn index_page_handwriting(&mut self) -> Result<(), String> {
        self.finish();
        let hidden = self.page().hidden_sources();
        self.session_mut().selection = self
            .page()
            .ordered_objects()
            .filter(|o| matches!(o.as_ref(), Object::Stroke(_)) && !hidden.contains(&o.id()))
            .map(|o| o.id())
            .collect();
        self.index_selected_handwriting()
    }
    pub fn stale_handwriting_regions(&self) -> usize {
        self.page()
            .ink_text
            .iter()
            .filter(|entry| entry.stale)
            .count()
    }
    pub fn review_stale_handwriting(&mut self) {
        let entries: Vec<_> = self
            .page()
            .ink_text
            .iter()
            .filter(|entry| entry.stale)
            .collect();
        let Some(bounds) = entries.iter().map(|entry| entry.bounds).reduce(Rect::union) else {
            return;
        };
        let text = entries
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let hidden = self.page().hidden_sources();
        let sources = self
            .page()
            .ordered_objects()
            .filter(|o| {
                matches!(o.as_ref(), Object::Stroke(_))
                    && !hidden.contains(&o.id())
                    && o.bounds().intersects(bounds.expand(2.))
            })
            .cloned()
            .collect::<Vec<_>>();
        let bounds = sources
            .iter()
            .map(|o| o.bounds())
            .reduce(Rect::union)
            .unwrap_or(bounds);
        self.recognition_review = Some(RecognitionReview {
            kind: RecognitionKind::Text,
            text,
            note: self.active,
            page: self.page().id,
            bounds,
            sources,
            pdf_source: None,
        });
        self.recognition_for_index = true;
        self.status = "Review the retained text against the changed handwriting".into();
    }
    pub fn can_index_review(&self) -> bool {
        self.recognition_review.as_ref().is_some_and(|r| {
            r.kind == RecognitionKind::Text
                && r.pdf_source.is_none()
                && !r.sources.is_empty()
                && r.sources
                    .iter()
                    .all(|o| matches!(o.as_ref(), Object::Stroke(_)))
        })
    }
    pub fn keep_ink_and_index(&mut self, text: String) -> Result<(), String> {
        let review = self
            .recognition_review
            .clone()
            .ok_or("Recognize handwriting before indexing it")?;
        if review.kind != RecognitionKind::Text
            || review.pdf_source.is_some()
            || review.sources.is_empty()
            || !review
                .sources
                .iter()
                .all(|o| matches!(o.as_ref(), Object::Stroke(_)))
        {
            return Err("Select handwriting for the search index".into());
        }
        if review.sources.len() > 4096 {
            return Err("Select fewer than 4097 strokes for one search annotation".into());
        }
        if self.recognition_pending || !self.recognition_is_current(&review) {
            return Err("Writing changed; recognize it again before indexing".into());
        }
        if text.trim().is_empty() || text.len() > 65536 {
            return Err("Search text needs 1–65536 bytes".into());
        }
        let before = self.page().ink_text.clone();
        let mut after = before.clone();
        let sources: Vec<_> = review.sources.iter().map(|o| o.id()).collect();
        let source_ids: HashSet<_> = sources.iter().copied().collect();
        after.retain(|entry| {
            !entry.sources.iter().any(|id| source_ids.contains(id))
                && !(entry.stale && entry.bounds.intersects(review.bounds.expand(2.)))
        });
        after.push(InkText {
            stale: false,
            text: text.trim().into(),
            sources,
            bounds: review.bounds,
        });
        self.commit(
            "Index handwriting (keep ink)",
            vec![Change::InkText {
                page: review.page,
                before,
                after,
            }],
        );
        self.recognition_review = None;
        self.recognition_for_index = false;
        self.status = "Handwriting is searchable · original ink retained".into();
        Ok(())
    }
    pub fn clear_handwriting_index(&mut self) {
        let before = self.page().ink_text.clone();
        if before.is_empty() {
            return;
        }
        self.commit(
            "Clear page handwriting index",
            vec![Change::InkText {
                page: self.page().id,
                before,
                after: vec![],
            }],
        );
    }
}
/// Keep search regions attached through transforms; invalidate changed writing.
/// Derived index changes join the same command as the source edit, so undo
/// restores both the writing and its reviewed search text.
pub(super) fn maintain_index(document: &Document, changes: &mut Vec<Change>) {
    let mut additions = Vec::new();
    let touched: HashSet<_> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Object { page, .. } => Some(*page),
            _ => None,
        })
        .collect();
    for page in document
        .pages
        .iter()
        .filter(|p| !p.ink_text.is_empty() && touched.contains(&p.id))
    {
        if changes
            .iter()
            .any(|c| matches!(c, Change::InkText { page: id, .. } if *id == page.id))
        {
            continue;
        }
        let edits: HashMap<_, _> = changes
            .iter()
            .filter_map(|change| match change {
                Change::Object {
                    page: id,
                    id: object,
                    before,
                    after,
                    ..
                } if *id == page.id => Some((*object, (before, after))),
                _ => None,
            })
            .collect();
        if edits.is_empty() {
            continue;
        }
        let mut after = Vec::new();
        for entry in &page.ink_text {
            let source_ids = (edits.len() > 4 && entry.sources.len() > 16)
                .then(|| entry.sources.iter().copied().collect::<HashSet<_>>());
            let includes = |id: &Id| {
                source_ids
                    .as_ref()
                    .map_or_else(|| entry.sources.contains(id), |ids| ids.contains(id))
            };
            let changed = edits.iter().any(|(id,(before,after))| {
                if includes(id) {
                    !matches!((before.as_deref(), after.as_deref()),(Some(Object::Stroke(a)),Some(Object::Stroke(b))) if (Arc::ptr_eq(&a.raw,&b.raw) || a.raw == b.raw) && (std::ptr::eq(a.display_path(), b.display_path()) || a.display_path() == b.display_path()))
                } else if let Some(object) = after {
                    match object.as_ref() {
                        Object::Shape(shape) => shape.source_strokes.iter().any(&includes),
                        Object::Equation(equation) => equation.source_strokes.iter().any(&includes),
                        Object::Stroke(_) if before.is_none() => object.bounds().intersects(entry.bounds.expand(2.)),
                        _ => false,
                    }
                } else { false }
            });
            let mut retained = entry.clone();
            retained.stale |= changed;
            retained.sources.retain(|id| {
                let object = if let Some((_, after)) = edits.get(id) {
                    after.as_ref()
                } else {
                    page.objects.get(id)
                };
                object.is_some_and(|o| matches!(o.as_ref(), Object::Stroke(_)))
            });
            let bounds = retained
                .sources
                .iter()
                .filter_map(|id| {
                    if let Some((_, after)) = edits.get(id) {
                        after.as_ref().map(|o| o.bounds())
                    } else {
                        page.objects.get(id).map(|o| o.bounds())
                    }
                })
                .reduce(Rect::union);
            if let Some(bounds) = bounds {
                retained.bounds = bounds;
            }
            after.push(retained);
        }
        if after != page.ink_text {
            additions.push(Change::InkText {
                page: page.id,
                before: page.ink_text.clone(),
                after,
            });
        }
    }
    changes.extend(additions);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_handwriting_retains_reviewed_drafts_and_undo_restores_index() {
        let mut app = Controller::open(
            std::env::temp_dir().join(format!("folio-index-test-{}", Id::new_v4())),
        )
        .unwrap();
        let mut builder = StrokeBuilder::new(PenStyle::default());
        builder.push(StrokePoint::new(Point::new(10., 10.), 0.5, 0));
        builder.push(StrokePoint::new(Point::new(100., 20.), 0.5, 16));
        let stroke = builder.finish().unwrap();
        let id = stroke.id;
        let page = app.page().id;
        app.commit(
            "Ink",
            vec![Change::Object {
                page,
                id,
                before: None,
                after: Some(Arc::new(Object::Stroke(stroke))),
                index: 0,
            }],
        );
        app.recognition_review = Some(RecognitionReview {
            kind: RecognitionKind::Text,
            text: "reviewed draft".into(),
            note: app.active,
            page,
            bounds: app.page().objects[&id].bounds(),
            sources: vec![app.page().objects[&id].clone()],
            pdf_source: None,
        });
        app.keep_ink_and_index("reviewed draft".into()).unwrap();
        assert_eq!(app.session().page_text().as_ref(), "reviewed draft");
        let mut new_stroke = app.page().objects[&id].as_ref().clone();
        new_stroke.set_id(Id::new_v4());
        app.commit(
            "New nearby ink",
            vec![Change::Object {
                page,
                id: new_stroke.id(),
                before: None,
                after: Some(Arc::new(new_stroke)),
                index: 1,
            }],
        );
        assert_eq!(app.stale_handwriting_regions(), 1);
        assert_eq!(app.page().ink_text[0].text, "reviewed draft");
        assert!(!app.page().text().contains("reviewed draft"));
        assert_eq!(app.session().page_text().as_ref(), app.page().text());
        app.review_stale_handwriting();
        assert_eq!(
            app.recognition_review.as_ref().unwrap().text,
            "reviewed draft"
        );
        assert_eq!(app.recognition_review.as_ref().unwrap().sources.len(), 2);
        app.undo();
        assert_eq!(app.stale_handwriting_regions(), 0);
        assert_eq!(app.session().page_text().as_ref(), "reviewed draft");
        app.redo();
        assert_eq!(app.stale_handwriting_regions(), 1);
        assert_eq!(app.session().page_text().as_ref(), app.page().text());
        app.select_all();
        app.delete_selection();
        assert_eq!(app.page().ink_text[0].text, "reviewed draft");
        assert!(app.page().ink_text[0].sources.is_empty());
        app.session().document.validate().unwrap();
        app.session().document.pages[0].duplicate();
        app.flush().unwrap();
        let root = app.data_dir.clone();
        drop(app);
        let app = Controller::open(root).unwrap();
        assert_eq!(app.page().ink_text[0].text, "reviewed draft");
        assert_eq!(app.stale_handwriting_regions(), 1);
    }
}
