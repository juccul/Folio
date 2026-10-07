use super::*;
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
        after.retain(|entry| !entry.sources.iter().any(|id| sources.contains(id)));
        after.push(InkText {
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
            let changed = edits.iter().any(|(id,(before,after))| {
                if entry.sources.contains(id) {
                    !matches!((before.as_deref(), after.as_deref()),(Some(Object::Stroke(a)),Some(Object::Stroke(b))) if (Arc::ptr_eq(&a.raw,&b.raw) || a.raw == b.raw) && a.display_path() == b.display_path())
                } else if let Some(object) = after {
                    match object.as_ref() {
                        Object::Shape(shape) => shape.source_strokes.iter().any(|id|entry.sources.contains(id)),
                        Object::Equation(equation) => equation.source_strokes.iter().any(|id|entry.sources.contains(id)),
                        Object::Stroke(_) if before.is_none() => object.bounds().intersects(entry.bounds.expand(2.)),
                        _ => false,
                    }
                } else { false }
            });
            if changed {
                continue;
            }
            let bounds = entry
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
                after.push(InkText {
                    bounds,
                    ..entry.clone()
                });
            }
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
