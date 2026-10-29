//! Selection queries and commands that change the selected objects.
use super::*;

impl Controller {
    pub fn selection_bounds(&self) -> Option<Rect> {
        self.session()
            .selection
            .iter()
            .filter_map(|id| self.page().objects.get(id))
            .map(|o| o.bounds())
            .reduce(|a, b| a.union(b))
    }
    pub fn select_all(&mut self) {
        let hidden = self.page().hidden_sources();
        let ids = self
            .page()
            .order
            .iter()
            .copied()
            .filter(|id| !hidden.contains(id))
            .collect();
        self.session_mut().selection = ids;
        // Select All borrows the lasso; clearing the selection returns to
        // the tool that was in use.
        if !matches!(self.tool, Tool::Lasso | Tool::Rectangle) && !self.temporary_selection {
            self.selection_return_tool = self.tool;
            self.temporary_selection = true;
        }
        self.tool = Tool::Lasso;
    }
    /// Clear the selection, returning from a borrowed lasso to the previous
    /// tool.
    pub fn clear_selection(&mut self) {
        self.session_mut().selection.clear();
        self.end_temporary_selection();
    }
    pub(crate) fn ordered_selection(&self, ids: &HashSet<Id>) -> Vec<(usize, Id)> {
        let page = self.page();
        // Sparse actions scale with the selection; dense actions already arrive
        // in drawing order and do not need a sort.
        if self.session().indexed_page == page.id && ids.len() <= page.order.len() / 2 {
            let mut selected = ids
                .iter()
                .filter_map(|id| {
                    self.session()
                        .order_positions
                        .get(id)
                        .map(|index| (*index, *id))
                })
                .collect::<Vec<_>>();
            selected.sort_unstable_by_key(|(index, _)| *index);
            selected
        } else {
            // Session exposes its page selector; callers may change it directly.
            page.order
                .iter()
                .enumerate()
                .filter(|(_, id)| ids.contains(id))
                .map(|(index, id)| (index, *id))
                .collect()
        }
    }
    pub(crate) fn selected_objects(&self) -> impl Iterator<Item = &Arc<Object>> {
        let page = self.page();
        self.ordered_selection(&self.session().selection)
            .into_iter()
            .filter_map(move |(_, id)| page.objects.get(&id))
    }
    pub(crate) fn hidden_selection(&self) -> HashSet<Id> {
        let session = self.session();
        if session.indexed_page != self.page().id {
            return self.page().hidden_sources();
        }
        session
            .selection
            .iter()
            .copied()
            .filter(|id| session.index.is_hidden(id))
            .collect()
    }
    pub(crate) fn object_changes(
        &self,
        ids: &HashSet<Id>,
        mut change: impl FnMut(&Object) -> Option<Object>,
    ) -> Vec<Change> {
        let page = self.page();
        self.ordered_selection(ids)
            .into_iter()
            .filter_map(|(index, id)| {
                let before = page.objects.get(&id)?.clone();
                let after = change(&before).map(Arc::new);
                if after.as_deref() == Some(before.as_ref()) {
                    return None;
                }
                Some(Change::Object {
                    page: page.id,
                    id,
                    before: Some(before),
                    after,
                    index,
                })
            })
            .collect()
    }
    /// `ids` plus the hidden source strokes of any shapes and equations among
    /// them, so that derived objects and their sources are edited together.
    pub(crate) fn with_derived_sources(&self, ids: &HashSet<Id>) -> HashSet<Id> {
        let mut expanded = ids.clone();
        for id in ids {
            match self.page().objects.get(id).map(|o| o.as_ref()) {
                Some(Object::Shape(s)) => expanded.extend(&s.source_strokes),
                Some(Object::Equation(e)) => expanded.extend(&e.source_strokes),
                _ => {}
            }
        }
        expanded
    }
    pub(crate) fn delete_ids(&mut self, ids: &HashSet<Id>, label: &str) {
        let ids = self.with_derived_sources(ids);
        let changes = self.object_changes(&ids, |_| None);
        self.commit(label, changes);
    }
    pub fn delete_selection(&mut self) {
        let ids = self.session().selection.clone();
        self.delete_ids(&ids, "Delete selection");
        self.session_mut().selection.clear();
    }
    pub fn transform_selection(&mut self, t: Transform, label: &str) {
        if t == Transform::default() {
            return;
        }
        let ids = self.with_derived_sources(&self.session().selection);
        let changes = self.object_changes(&ids, |o| {
            let mut o = o.clone();
            o.set_transform(t.compose(o.transform()));
            Some(o)
        });
        self.commit(label, changes);
    }
    pub(crate) fn resize_selection(&mut self, resize: Transform, label: &str) {
        if resize == Transform::default() {
            return;
        }
        let ids = self.with_derived_sources(&self.session().selection);
        let changes = self.object_changes(&ids, |object| {
            let mut object = object.clone();
            if let Object::Text(text) = &mut object {
                text.reflow(resize);
            } else {
                object.set_transform(resize.compose(object.transform()));
            }
            Some(object)
        });
        self.commit(label, changes);
    }
    pub fn scale_selection(&mut self, scale: f32) {
        if let Some(r) = self.selection_bounds() {
            self.resize_selection(Transform::around(r.center(), scale, 0.), "Resize selection")
        }
    }
    pub fn rotate_selection(&mut self, angle: f32) {
        if let Some(r) = self.selection_bounds() {
            self.transform_selection(Transform::around(r.center(), 1., angle), "Rotate selection")
        }
    }
    pub fn restyle_selection(&mut self) {
        let style = self.style.clone();
        let changes = self.object_changes(&self.session().selection, |o| {
            let mut o = o.clone();
            if let Some(s) = o.style_mut() {
                *s = style.clone();
                if let Object::Stroke(s) = &mut o {
                    folio_ink::rebuild(s)
                }
            }
            Some(o)
        });
        self.commit("Change ink style", changes);
    }
    pub fn refine_selection(&mut self) {
        let strokes: Vec<_> = self
            .session()
            .selection
            .iter()
            .filter_map(|id| self.page().objects.get(id))
            .filter_map(|o| match o.as_ref() {
                Object::Stroke(s) => Some(s),
                _ => None,
            })
            .collect();
        if strokes.is_empty() {
            return;
        }
        let enable = !strokes.iter().all(|s| s.refinement_enabled);
        let changes = self.object_changes(&self.session().selection, |o| {
            let mut o = o.clone();
            if let Object::Stroke(s) = &mut o {
                if enable {
                    folio_ink::refine(s)
                } else {
                    s.refinement_enabled = false
                }
            }
            Some(o)
        });
        self.commit("Refine handwriting", changes);
    }
    pub fn crop_selection(&mut self, crop: Option<Rect>) {
        let changes = self.object_changes(&self.session().selection, |object| {
            let mut object = object.clone();
            if let Object::Image(image) = &mut object {
                image.crop = crop;
            }
            Some(object)
        });
        self.commit("Crop image", changes);
    }
}
