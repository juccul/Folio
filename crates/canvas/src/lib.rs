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
        let inverse = self.transform().inverse().unwrap_or_default();
        Rect::from_points(
            [
                Point::new(0., 0.),
                Point::new(width, 0.),
                Point::new(width, height),
                Point::new(0., height),
            ]
            .map(|p| inverse.apply(p)),
        )
    }
}
/// A centered, continuous stack of finite pages. Infinite canvases remain
/// independent; mixed documents stack each contiguous run of finite pages.
pub struct PageStack {
    pub pages: Vec<(usize, Rect)>,
    pub height: f32,
}
impl PageStack {
    pub const GAP: f32 = 28.;
    pub fn new(pages: &[Page], active: usize) -> Option<Self> {
        if pages.get(active)?.properties.infinite {
            return None;
        }
        let start = (0..active)
            .rev()
            .find(|&i| pages[i].properties.infinite)
            .map_or(0, |i| i + 1);
        let end = (active + 1..pages.len())
            .find(|&i| pages[i].properties.infinite)
            .unwrap_or(pages.len());
        let width = pages[start..end]
            .iter()
            .map(|p| p.properties.width)
            .fold(0., f32::max);
        let mut y = 0.;
        let frames = (start..end)
            .map(|i| {
                let p = &pages[i].properties;
                let frame = Rect::new((width - p.width) / 2., y, p.width, p.height);
                y += p.height + Self::GAP;
                (i, frame)
            })
            .collect();
        Some(Self {
            pages: frames,
            height: y - Self::GAP,
        })
    }
    pub fn frame(&self, index: usize) -> Rect {
        self.pages[index - self.pages[0].0].1
    }
    /// Express the same screen transform in a different page's local coordinates.
    pub fn viewport(&self, viewport: Viewport, active: usize, target: usize) -> Viewport {
        let from = self.frame(active).min;
        let to = self.frame(target).min;
        Viewport {
            pan: viewport.to_screen(Point::new(to.x - from.x, to.y - from.y)),
            ..viewport
        }
    }
    pub fn hit(&self, viewport: Viewport, active: usize, screen: Point) -> Option<usize> {
        let p = viewport.to_document(screen);
        let origin = self.frame(active).min;
        let global = Point::new(p.x + origin.x, p.y + origin.y);
        let index = self.pages.partition_point(|(_, r)| r.max.y < global.y);
        self.pages
            .get(index)
            .filter(|(_, r)| r.contains(global))
            .map(|(i, _)| *i)
    }
    pub fn nearest(&self, viewport: Viewport, active: usize, screen: Point) -> usize {
        let y = viewport.to_document(screen).y + self.frame(active).min.y;
        let next = self.pages.partition_point(|(_, r)| r.max.y < y);
        if next == 0 {
            return self.pages[0].0;
        }
        if next == self.pages.len() {
            return self.pages[next - 1].0;
        }
        let before = self.pages[next - 1];
        let after = self.pages[next];
        if y - before.1.max.y <= (after.1.min.y - y).max(0.) {
            before.0
        } else {
            after.0
        }
    }
    pub fn clamp_vertical(&self, viewport: &mut Viewport, active: usize, height: f32) {
        if viewport.rotation.abs() > 0.0001 {
            return;
        }
        let offset = self.frame(active).min.y * viewport.zoom;
        let minimum = (height - self.height * viewport.zoom - 36.).min(36.);
        viewport.pan.y = (viewport.pan.y - offset).clamp(minimum, 36.) + offset;
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
            } else {
                self.oversize.retain(|v| *v != id);
            }
        }
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
        self.query_into(r, &mut out);
        out
    }
    /// Clear and reuse the caller's result storage for repeated broad-phase queries.
    pub fn query_into(&self, r: Rect, out: &mut HashSet<Id>) {
        out.clear();
        if !Self::valid(r) {
            return;
        }
        let (x0, y0, x1, y1) = Self::cells(r);
        if !Self::bounded((x0, y0, x1, y1)) {
            self.query_bounds(r, out);
            return;
        }
        let mut candidates = self.oversize.len();
        let budget = self.bounds.len().saturating_mul(2);
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(ids) = self.cells.get(&(x, y)) {
                    candidates = candidates.saturating_add(ids.len());
                    if candidates > budget {
                        // For dense multi-cell objects, inspecting each object's
                        // bounds once is cheaper than hashing repeated references.
                        out.clear();
                        self.query_bounds(r, out);
                        return;
                    }
                    out.extend(ids.iter().copied());
                }
            }
        }
        out.extend(self.oversize.iter().copied());
        // An object may occupy thousands of cells; test its actual bounds once.
        out.retain(|id| self.bounds[id].intersects(r));
    }
    fn query_bounds(&self, r: Rect, out: &mut HashSet<Id>) {
        out.extend(
            self.bounds
                .iter()
                .filter(|(_, b)| b.intersects(r))
                .map(|(id, _)| *id),
        );
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
    fn reused_queries_match_brute_force_and_clear_previous_results() {
        let mut index = SpatialIndex::default();
        let objects = (0..200)
            .map(|i| {
                let r = Rect::new(
                    (i % 20) as f32 * 47. - 450.,
                    (i / 20) as f32 * 71. - 300.,
                    300.,
                    230.,
                );
                (Id::new_v4(), r)
            })
            .chain(std::iter::once((
                Id::new_v4(),
                Rect::new(-10000., -10000., 20000., 20000.),
            )))
            .collect::<Vec<_>>();
        for &(id, r) in &objects {
            index.insert(id, r);
        }
        let mut output = HashSet::with_capacity(512);
        let capacity = output.capacity();
        for i in 0..100 {
            let r = Rect::new(i as f32 * 19. - 800., i as f32 * 7. - 500., 250., 310.);
            let expected = objects
                .iter()
                .filter(|(_, b)| b.intersects(r))
                .map(|(id, _)| *id)
                .collect::<HashSet<_>>();
            index.query_into(r, &mut output);
            assert_eq!(output, expected);
        }
        index.query_into(Rect::new(-20000., -20000., 40000., 40000.), &mut output);
        assert_eq!(output.len(), objects.len());
        index.query_into(Rect::new(f32::NAN, 0., 10., 10.), &mut output);
        assert!(output.is_empty());
        assert_eq!(output.capacity(), capacity);
        for &(id, _) in &objects {
            index.remove(id);
        }
        assert!(
            index
                .query(Rect::new(-20000., -20000., 40000., 40000.))
                .is_empty()
        );
    }

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

#[cfg(test)]
mod page_stack_tests {
    use super::*;
    fn pages() -> Vec<Page> {
        [(800., 1100.), (400., 600.), (1000., 500.)]
            .map(|(width, height)| {
                let mut p = Page::new();
                p.properties.width = width;
                p.properties.height = height;
                p
            })
            .into()
    }
    #[test]
    fn binary_page_lookup_matches_linear_lookup_at_edges_gaps_and_rotations() {
        let pages = (0..300)
            .map(|i| {
                let mut page = Page::new();
                page.properties.width = 200. + (i % 7) as f32 * 53.;
                page.properties.height = 100. + (i % 11) as f32 * 29.;
                page
            })
            .collect::<Vec<_>>();
        let stack = PageStack::new(&pages, 0).unwrap();
        for rotation in [0., 0.4] {
            let viewport = Viewport {
                rotation,
                zoom: 1.3,
                ..Default::default()
            };
            for &(_, frame) in &stack.pages {
                for y in [
                    frame.min.y - 15.,
                    frame.min.y,
                    frame.center().y,
                    frame.max.y,
                    frame.max.y + 14.,
                ] {
                    let origin = stack.frame(0).min;
                    let screen = viewport.to_screen(Point::new(frame.center().x - origin.x, y));
                    // Use the same roundtrip as the production path at f32 boundaries.
                    let local = viewport.to_document(screen);
                    let global = Point::new(local.x + origin.x, local.y + origin.y);
                    let hit = stack
                        .pages
                        .iter()
                        .find(|(_, r)| r.contains(global))
                        .map(|(i, _)| *i);
                    assert_eq!(stack.hit(viewport, 0, screen), hit);
                    let distance = |r: &Rect| (r.min.y - global.y).max(global.y - r.max.y).max(0.);
                    let nearest = stack
                        .pages
                        .iter()
                        .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
                        .unwrap()
                        .0;
                    assert_eq!(stack.nearest(viewport, 0, screen), nearest);
                }
            }
        }
    }
    #[test]
    fn different_sizes_center_and_hit_test_without_drawing_in_gaps() {
        let stack = PageStack::new(&pages(), 0).unwrap();
        assert_eq!(stack.frame(0), Rect::new(100., 0., 800., 1100.));
        assert_eq!(stack.frame(1), Rect::new(300., 1128., 400., 600.));
        assert_eq!(stack.height, 2256.);
        let viewport = Viewport::default();
        assert_eq!(
            stack.hit(viewport, 0, viewport.to_screen(Point::new(220., 1148.))),
            Some(1)
        );
        assert_eq!(
            stack.hit(viewport, 0, viewport.to_screen(Point::new(0., 1148.))),
            None
        );
        assert_eq!(
            stack.hit(viewport, 0, viewport.to_screen(Point::new(220., 1114.))),
            None
        );
    }
    #[test]
    fn switching_local_coordinates_preserves_screen_positions_even_when_rotated() {
        let stack = PageStack::new(&pages(), 0).unwrap();
        let viewport = Viewport {
            zoom: 1.7,
            rotation: 0.6,
            pan: Point::new(-270., -700.),
        };
        let local = stack.viewport(viewport, 0, 2);
        let p = Point::new(120., 80.);
        let offset = stack.frame(2).min;
        let origin = stack.frame(0).min;
        let screen = viewport.to_screen(Point::new(
            p.x + offset.x - origin.x,
            p.y + offset.y - origin.y,
        ));
        assert!(screen.distance(local.to_screen(p)) < 0.001);
        assert_eq!(stack.hit(local, 2, screen), Some(2));
        assert!(stack.viewport(local, 2, 0).pan.distance(viewport.pan) < 0.001);
    }
    #[test]
    fn vertical_scroll_limits_use_whole_stack_and_survive_rebasing() {
        let stack = PageStack::new(&pages(), 0).unwrap();
        let mut viewport = Viewport {
            pan: Point::new(80., -99999.),
            ..Default::default()
        };
        stack.clamp_vertical(&mut viewport, 0, 700.);
        assert_eq!(viewport.pan.y, 700. - 2256. - 36.);
        let mut last = stack.viewport(viewport, 0, 2);
        stack.clamp_vertical(&mut last, 2, 700.);
        assert_eq!(stack.viewport(last, 2, 0).pan.y, viewport.pan.y);
        viewport.pan.y = 99999.;
        stack.clamp_vertical(&mut viewport, 0, 700.);
        assert_eq!(viewport.pan.y, 36.);
        viewport.zoom = 0.1;
        viewport.pan.y = -99999.;
        stack.clamp_vertical(&mut viewport, 0, 700.);
        assert_eq!(viewport.pan.y, 36.);
    }
    #[test]
    fn infinite_pages_separate_finite_runs() {
        let mut pages = pages();
        pages[1].properties.infinite = true;
        assert!(PageStack::new(&pages, 1).is_none());
        assert_eq!(PageStack::new(&pages, 0).unwrap().pages.len(), 1);
        let last = PageStack::new(&pages, 2).unwrap();
        assert_eq!(last.pages.len(), 1);
        assert_eq!(last.frame(2), Rect::new(0., 0., 1000., 500.));
    }
}
