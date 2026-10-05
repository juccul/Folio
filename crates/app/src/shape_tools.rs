use super::*;

pub(crate) struct JoinedShape {
    pub fit: folio_shapes::Fit,
    pub previous: Arc<Object>,
    pub sources: Vec<Id>,
}
impl Controller {
    /// Join only a recent, nearby, matching pen trace after an explicit shape
    /// intent (Shape tool or endpoint hold). Existing ink is never merged.
    pub(crate) fn joined_arrow(&self, stroke: &InkStroke) -> Option<JoinedShape> {
        let current = Object::Stroke(stroke.clone());
        let bounds = current.bounds();
        let candidates = self
            .session()
            .index
            .query(bounds.expand(bounds.width().hypot(bounds.height()) * 0.08 + 4.));
        for id in self
            .page()
            .order
            .iter()
            .rev()
            .filter(|id| candidates.contains(id))
            .take(8)
        {
            let previous = self.page().objects.get(id)?;
            if previous.transform() != Transform::default() {
                continue;
            }
            let (points, sources, created, style) = match previous.as_ref() {
                Object::Stroke(s) => (
                    s.raw.iter().map(|p| p.position()).collect::<Vec<_>>(),
                    vec![s.id],
                    s.created_at,
                    &s.style,
                ),
                Object::Shape(s)
                    if matches!(s.kind, ShapeKind::Line | ShapeKind::Polyline)
                        && s.source_strokes.len() == 1 =>
                {
                    let Some(Object::Stroke(source)) = self
                        .page()
                        .objects
                        .get(&s.source_strokes[0])
                        .map(Arc::as_ref)
                    else {
                        continue;
                    };
                    (
                        s.vertices.clone(),
                        s.source_strokes.clone(),
                        source.created_at,
                        &s.style,
                    )
                }
                _ => continue,
            };
            if stroke.created_at.saturating_sub(created) > 5000 || style != &stroke.style {
                continue;
            }
            let new = stroke.raw.iter().map(|p| p.position()).collect::<Vec<_>>();
            if let Some(fit) = folio_shapes::fit_strokes(&[&points, &new]) {
                return Some(JoinedShape {
                    fit,
                    previous: previous.clone(),
                    sources,
                });
            }
        }
        None
    }
}
