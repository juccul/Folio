//! Viewport geometry and spatial indexing; no GPUI dependency.
use folio_document::*;
use std::collections::{HashMap, HashSet};
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub zoom: f32,
    pub pan: Point,
    pub rotation: f32,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 1.,
            pan: Point::new(56., 36.),
            rotation: 0.,
        }
    }
}
impl Viewport {
    pub fn transform(self) -> Transform {
        let (s, c) = self.rotation.sin_cos();
        Transform {
            a: self.zoom * c,
            b: self.zoom * s,
            c: -self.zoom * s,
            d: self.zoom * c,
            tx: self.pan.x,
            ty: self.pan.y,
        }
    }
    pub fn to_document(self, p: Point) -> Point {
        self.transform().inverse().unwrap_or_default().apply(p)
    }
    pub fn to_screen(self, p: Point) -> Point {
        self.transform().apply(p)
    }
    pub fn zoom_at(&mut self, factor: f32, anchor: Point) {
        let original = self.to_document(anchor);
        self.zoom = (self.zoom * factor).clamp(0.1, 8.);
        let screen = self.to_screen(original);
        self.pan.x += anchor.x - screen.x;
        self.pan.y += anchor.y - screen.y;
    }
    pub fn fit(&mut self, page: &PageProperties, width: f32, height: f32) {
        self.zoom = ((width - 80.) / page.width)
            .min((height - 64.) / page.height)
            .clamp(0.1, 2.);
        self.pan = Point::new((width - page.width * self.zoom) / 2., 32.);
        self.rotation = 0.;
    }
    pub fn visible(self, width: f32, height: f32) -> Rect {
        Rect::from_points(
            [
                Point::new(0., 0.),
                Point::new(width, 0.),
                Point::new(width, height),
                Point::new(0., height),
            ]
            .map(|p| self.to_document(p)),
        )
    }
}
/// Uniform-grid broad phase. Oversize objects have a separate bucket so malformed
/// or enormous imported geometry cannot create an unbounded allocation.
#[derive(Default)]
pub struct SpatialIndex {
    cells: HashMap<(i32, i32), Vec<Id>>,
    oversize: Vec<Id>,
    bounds: HashMap<Id, Rect>,
    sources: HashMap<Id, Vec<Id>>,
    hidden: HashMap<Id, usize>,
    generation: u64,
}
impl SpatialIndex {
    const CELL: f32 = 128.;
    fn cells(r: Rect) -> (i32, i32, i32, i32) {
        (
            (r.min.x / Self::CELL).floor() as i32,
            (r.min.y / Self::CELL).floor() as i32,
            (r.max.x / Self::CELL).floor() as i32,
            (r.max.y / Self::CELL).floor() as i32,
        )
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    fn valid(r: Rect) -> bool {
        [r.min.x, r.min.y, r.max.x, r.max.y]
            .iter()
            .all(|v| v.is_finite())
            && r.min.x <= r.max.x
            && r.min.y <= r.max.y
    }
    fn bounded(cells: (i32, i32, i32, i32)) -> bool {
        let (x0, y0, x1, y1) = cells;
        let width = (x1 as i64 - x0 as i64 + 1) as u64;
        let height = (y1 as i64 - y0 as i64 + 1) as u64;
        width.checked_mul(height).is_some_and(|count| count <= 4096)
    }
    fn source_ids(object: &Object) -> &[Id] {
        match object {
            Object::Shape(s) => &s.source_strokes,
            Object::Equation(e) => &e.source_strokes,
            _ => &[],
        }
    }
    pub fn remove(&mut self, id: Id) {
        if let Some(r) = self.bounds.remove(&id) {
            self.generation = self.generation.wrapping_add(1);
            let (x0, y0, x1, y1) = Self::cells(r);
            if Self::bounded((x0, y0, x1, y1)) {
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        if let Some(ids) = self.cells.get_mut(&(x, y)) {
                            ids.retain(|v| *v != id);
                            if ids.is_empty() {
                                self.cells.remove(&(x, y));
                            }
                        }
                    }
                }
            }
        }
        self.oversize.retain(|v| *v != id);
    }
    pub fn insert(&mut self, id: Id, r: Rect) {
        if self.bounds.get(&id) == Some(&r) {
            return;
        }
        self.remove(id);
        if !Self::valid(r) {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.bounds.insert(id, r);
        let (x0, y0, x1, y1) = Self::cells(r);
        if !Self::bounded((x0, y0, x1, y1)) {
            self.oversize.push(id);
            return;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.cells.entry((x, y)).or_default().push(id);
            }
        }
    }
    pub fn rebuild(&mut self, page: &Page) {
        self.cells.clear();
        self.oversize.clear();
        self.bounds.clear();
        self.sources.clear();
        self.hidden.clear();
        self.generation = self.generation.wrapping_add(1);
        for object in page.objects.values() {
            let sources = Self::source_ids(object);
            if !sources.is_empty() {
                self.sources.insert(object.id(), sources.to_vec());
                for id in sources {
                    *self.hidden.entry(*id).or_default() += 1;
                }
            }
        }
        for o in page.objects.values() {
            if !self.hidden.contains_key(&o.id()) {
                self.insert(o.id(), o.bounds());
            }
        }
    }
    /// Touch only changed objects and their derived-object sources.
    pub fn update(&mut self, page: &Page, command: &Command) {
        if command
            .changes
            .iter()
            .any(|c| matches!(c, Change::Page { .. }))
        {
            self.rebuild(page);
            return;
        }
        let changed = command
            .changes
            .iter()
            .filter_map(|c| match c {
                Change::Object { page: p, id, .. } if *p == page.id => Some(*id),
                _ => None,
            })
            .collect::<HashSet<_>>();
        if changed.is_empty() {
            return;
        }
        let mut affected = changed.clone();
        // Read the actual document state, so this works identically for execute,
        // undo and redo, including sources shared by multiple derived objects.
        for id in changed {
            if let Some(sources) = self.sources.remove(&id) {
                for source in sources {
                    affected.insert(source);
                    if let Some(count) = self.hidden.get_mut(&source) {
                        *count -= 1;
                        if *count == 0 {
                            self.hidden.remove(&source);
                        }
                    }
                }
            }
            if let Some(object) = page.objects.get(&id) {
                let sources = Self::source_ids(object);
                if !sources.is_empty() {
                    self.sources.insert(id, sources.to_vec());
                    for source in sources {
                        affected.insert(*source);
                        *self.hidden.entry(*source).or_default() += 1;
                    }
                }
            }
        }
        for id in affected {
            if !self.hidden.contains_key(&id)
                && let Some(object) = page.objects.get(&id)
            {
                self.insert(id, object.bounds());
            } else {
                self.remove(id);
            }
        }
    }
    pub fn query(&self, r: Rect) -> HashSet<Id> {
        let mut out = HashSet::new();
        if !Self::valid(r) {
            return out;
        }
        let (x0, y0, x1, y1) = Self::cells(r);
        if !Self::bounded((x0, y0, x1, y1)) {
            out.extend(
                self.bounds
                    .iter()
                    .filter(|(_, b)| b.intersects(r))
                    .map(|(id, _)| *id),
            );
            return out;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(ids) = self.cells.get(&(x, y)) {
                    out.extend(
                        ids.iter()
                            .copied()
                            .filter(|id| self.bounds[id].intersects(r)),
                    );
                }
            }
        }
        out.extend(
            self.oversize
                .iter()
                .copied()
                .filter(|id| self.bounds[id].intersects(r)),
        );
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_preserves_anchor() {
        let mut v = Viewport::default();
        let p = Point::new(340., 260.);
        let before = v.to_document(p);
        v.zoom_at(2., p);
        assert!(v.to_document(p).distance(before) < 0.001);
    }
    #[test]
    fn rotated_roundtrip() {
        let v = Viewport {
            rotation: 0.7,
            zoom: 2.,
            ..Default::default()
        };
        let p = Point::new(4., 80.);
        assert!(v.to_document(v.to_screen(p)).distance(p) < 0.001);
    }
}
#[cfg(test)]
mod incremental_tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn update_removes_old_cells_and_matches_a_rebuild() {
        let mut doc = Document::new("test");
        let id = Id::new_v4();
        let page = doc.pages[0].id;
        let object = Arc::new(Object::Image(ImageObject {
            id,
            asset: "test.png".into(),
            rect: Rect::new(0., 0., 100., 100.),
            transform: Transform::default(),
            crop: None,
        }));
        doc.pages[0].order.push(id);
        doc.pages[0].objects.insert(id, object.clone());
        let mut index = SpatialIndex::default();
        index.rebuild(&doc.pages[0]);
        let mut after = object.as_ref().clone();
        after.set_transform(Transform::translate(1000., 1000.));
        let command = Command {
            label: "Move".into(),
            changes: vec![Change::Object {
                page,
                id,
                before: Some(object),
                after: Some(Arc::new(after)),
                index: 0,
            }],
        };
        command.apply(&mut doc, true);
        index.update(&doc.pages[0], &command);
        let mut rebuilt = SpatialIndex::default();
        rebuilt.rebuild(&doc.pages[0]);
        for r in [
            Rect::new(0., 0., 128., 128.),
            Rect::new(900., 900., 300., 300.),
        ] {
            assert_eq!(index.query(r), rebuilt.query(r));
        }
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn huge_and_invalid_geometry_cannot_overflow_grid_cell_counts() {
        let mut index = SpatialIndex::default();
        let id = Id::new_v4();
        let huge = Rect {
            min: Point::new(-f32::MAX, -f32::MAX),
            max: Point::new(f32::MAX, f32::MAX),
        };
        index.insert(id, huge);
        assert_eq!(index.query(huge), HashSet::from([id]));
        assert_eq!(index.query(Rect::new(0., 0., 1., 1.)), HashSet::from([id]));
        index.remove(id);
        assert!(index.query(huge).is_empty());
        let invalid = Rect::new(f32::NAN, 0., 1., 1.);
        index.insert(id, invalid);
        assert!(index.query(huge).is_empty());
        assert!(index.query(invalid).is_empty());
    }

    #[test]
    fn unchanged_bounds_preserve_cache_generation() {
        let mut index = SpatialIndex::default();
        let id = Id::new_v4();
        let bounds = Rect::new(0., 0., 10., 10.);
        index.insert(id, bounds);
        let generation = index.generation();
        index.insert(id, bounds);
        index.remove(Id::new_v4());
        assert_eq!(index.generation(), generation);
        index.insert(id, Rect::new(200., 0., 10., 10.));
        assert_ne!(index.generation(), generation);
    }

    #[test]
    fn shared_sources_remain_hidden_until_last_derived_object_is_deleted() {
        let mut doc = Document::new("shared sources");
        let source = Id::new_v4();
        let image = Arc::new(Object::Image(ImageObject {
            id: source,
            asset: "fixture.png".into(),
            rect: Rect::new(0., 0., 20., 20.),
            transform: Transform::default(),
            crop: None,
        }));
        let shapes = (0..2)
            .map(|_| {
                Arc::new(Object::Shape(Shape {
                    id: Id::new_v4(),
                    kind: ShapeKind::Line,
                    vertices: vec![Point::new(0., 0.), Point::new(20., 20.)],
                    style: PenStyle::default(),
                    transform: Transform::default(),
                    source_strokes: vec![source],
                }))
            })
            .collect::<Vec<_>>();
        for object in std::iter::once(image).chain(shapes.iter().cloned()) {
            doc.pages[0].order.push(object.id());
            doc.pages[0].objects.insert(object.id(), object);
        }
        let mut index = SpatialIndex::default();
        index.rebuild(&doc.pages[0]);
        let region = Rect::new(-10., -10., 100., 100.);
        for (position, shape) in shapes.iter().enumerate() {
            let command = Command {
                label: "Delete derived".into(),
                changes: vec![Change::Object {
                    page: doc.pages[0].id,
                    id: shape.id(),
                    before: Some(shape.clone()),
                    after: None,
                    index: position + 1,
                }],
            };
            command.apply(&mut doc, true);
            index.update(&doc.pages[0], &command);
            let mut rebuilt = SpatialIndex::default();
            rebuilt.rebuild(&doc.pages[0]);
            assert_eq!(index.query(region), rebuilt.query(region));
            assert_eq!(index.query(region).contains(&source), position == 1);
            command.apply(&mut doc, false);
            index.update(&doc.pages[0], &command);
            assert!(!index.query(region).contains(&source));
            command.apply(&mut doc, true);
            index.update(&doc.pages[0], &command);
        }
    }
}
