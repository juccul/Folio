use super::*;

pub(crate) struct JoinedShape {
    pub fit: folio_shapes::Fit,
    /// Joined objects in drawing order.
    pub previous: Vec<Arc<Object>>,
    pub sources: Vec<Id>,
}
struct JoinCandidate {
    object: Arc<Object>,
    points: Vec<Point>,
    sources: Vec<Id>,
}
impl Controller {
    /// Join only recent, nearby, matching pen traces after an explicit shape
    /// intent (Shape tool or endpoint hold). Existing ink is never merged.
    /// A head and shaft may be one or two earlier traces (shaft and V head, or
    /// shaft and a separate barb); pairs are preferred over triples.
    pub(crate) fn joined_arrow(&self, stroke: &InkStroke) -> Option<JoinedShape> {
        let current = Object::Stroke(stroke.clone());
        let bounds = current.bounds();
        // A short barb may stop a small gap away from a much longer shaft.
        let candidates = self
            .session()
            .index
            .query(bounds.expand(bounds.width().hypot(bounds.height()) * 0.4 + 6.));
        let recent = self
            .page()
            .order
            .iter()
            .rev()
            // Ink already hidden behind a fitted shape is reached via that shape.
            .filter(|id| candidates.contains(id) && !self.session().index.is_hidden(id))
            .take(8)
            .filter_map(|id| self.join_candidate(id, stroke))
            .collect::<Vec<_>>();
        let new = stroke.raw.iter().map(|p| p.position()).collect::<Vec<_>>();
        let pairs = (0..recent.len()).map(|i| vec![i]);
        let triples =
            (0..recent.len()).flat_map(|i| (i + 1..recent.len()).map(move |j| vec![j, i]));
        pairs.chain(triples).find_map(|group| {
            let mut traces = group
                .iter()
                .map(|&i| recent[i].points.as_slice())
                .collect::<Vec<_>>();
            traces.push(&new);
            let fit = folio_shapes::fit_strokes(&traces)?;
            Some(JoinedShape {
                fit,
                previous: group.iter().map(|&i| recent[i].object.clone()).collect(),
                sources: group
                    .iter()
                    .flat_map(|&i| recent[i].sources.iter().copied())
                    .collect(),
            })
        })
    }
    fn join_candidate(&self, id: &Id, stroke: &InkStroke) -> Option<JoinCandidate> {
        let previous = self.page().objects.get(id)?;
        if previous.transform() != Transform::default() {
            return None;
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
                    return None;
                };
                (
                    s.vertices.clone(),
                    s.source_strokes.clone(),
                    source.created_at,
                    &s.style,
                )
            }
            _ => return None,
        };
        if stroke.created_at.saturating_sub(created) > 5000 || style != &stroke.style {
            return None;
        }
        Some(JoinCandidate {
            object: previous.clone(),
            points,
            sources,
        })
    }
}
