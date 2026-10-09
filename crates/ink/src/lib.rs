//! GPUI-independent incremental ink processing. Velocity-adaptive filtering,
//! pressure width, distance interpolation and rounded vector outlines.
use folio_document::*;
use std::f32::consts::PI;

pub struct StrokeBuilder {
    id: Id,
    style: PenStyle,
    raw: Vec<StrokePoint>,
    path: Vec<PathPoint>,
    filtered: Option<Point>,
    pressure: f32,
}
impl StrokeBuilder {
    pub fn new(style: PenStyle) -> Self {
        Self {
            id: Id::new_v4(),
            style,
            raw: Vec::new(),
            path: Vec::new(),
            filtered: None,
            pressure: 0.5,
        }
    }
    pub fn push(&mut self, p: StrokePoint) {
        if !p.valid() {
            return;
        }
        let position = p.position();
        let prev = self.raw.last().copied();
        self.raw.push(p);
        let previous = self.filtered.unwrap_or(position);
        let dt = prev
            .map(|v| p.timestamp.saturating_sub(v.timestamp).max(1) as f32)
            .unwrap_or(8.);
        let velocity = prev
            .map(|v| position.distance(v.position()) / dt)
            .unwrap_or(0.);
        // Less lag at higher velocity, stronger noise suppression while moving slowly.
        let stabilization = self.style.stabilization.clamp(0., 0.9);
        let alpha = (1. - stabilization * 0.9 / (1. + velocity * 1.5)).clamp(0.12, 1.);
        let filtered = previous.lerp(position, alpha);
        self.pressure += (p.pressure - self.pressure) * 0.65;
        let radius = radius(&self.style, self.pressure, p.tilt_x, p.tilt_y);
        if let Some(last) = self.path.last().copied() {
            let distance = last.position.distance(filtered);
            if distance < 0.12 {
                if let Some(last) = self.path.last_mut() {
                    last.radius = radius
                }
                self.filtered = Some(filtered);
                return;
            }
            let steps = (distance / 1.8).ceil().clamp(1., 256.) as usize;
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                self.path.push(PathPoint {
                    position: last.position.lerp(filtered, t),
                    radius: last.radius + (radius - last.radius) * t,
                });
            }
        } else {
            self.path.push(PathPoint {
                position: filtered,
                radius,
            });
        }
        self.filtered = Some(filtered);
    }
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn snapshot(&self) -> Option<InkStroke> {
        if self.raw.is_empty() {
            return None;
        }
        Some(InkStroke {
            id: self.id,
            raw: self.raw.clone().into(),
            path: self.path.clone().into(),
            style: self.style.clone(),
            transform: Transform::default(),
            created_at: now_ms(),
            fragment_path: None,
            refined_path: None,
            refinement_enabled: false,
        })
    }
    pub fn path(&self) -> &[PathPoint] {
        &self.path
    }
    pub fn style(&self) -> &PenStyle {
        &self.style
    }
    pub fn raw(&self) -> &[StrokePoint] {
        &self.raw
    }
    pub fn finish(mut self) -> Option<InkStroke> {
        if self.raw.is_empty() {
            return None;
        }
        // Catch the endpoint without changing sensor data. Taper fountain/pencil ends.
        let endpoint = self.raw.last().unwrap().position();
        if let Some(last) = self.path.last().copied()
            && endpoint.distance(last.position) > 0.2
        {
            self.path.push(PathPoint {
                position: endpoint,
                ..last
            });
        }
        if matches!(self.style.tool, InkTool::Fountain | InkTool::Pencil) && self.path.len() > 3 {
            let n = self.path.len();
            let taper = 8.min(n / 2);
            for i in 0..taper {
                let factor = 0.35 + 0.65 * i as f32 / taper as f32;
                self.path[i].radius *= factor;
                self.path[n - 1 - i].radius *= factor;
            }
        }
        Some(InkStroke {
            id: self.id,
            raw: self.raw.into(),
            path: self.path.into(),
            style: self.style,
            transform: Transform::default(),
            created_at: now_ms(),
            fragment_path: None,
            refined_path: None,
            refinement_enabled: false,
        })
    }
}
fn radius(style: &PenStyle, pressure: f32, tx: f32, ty: f32) -> f32 {
    let pressure = pressure
        .clamp(0., 1.)
        .powf(style.pressure_gamma.clamp(0.2, 3.));
    let factor = match style.tool {
        InkTool::Ballpoint => 0.28 + 0.9 * pressure,
        InkTool::Fountain => 0.16 + 1.45 * pressure,
        InkTool::Pencil => 0.18 + pressure + (tx.abs() + ty.abs()) / 300.,
        InkTool::Marker => 0.8 + 0.2 * pressure,
        InkTool::Highlighter => 1.,
    };
    (style.width.clamp(0.2, 80.) * 0.5 * factor).max(0.1)
}
pub fn rebuild(stroke: &mut InkStroke) {
    let mut builder = StrokeBuilder::new(stroke.style.clone());
    for p in stroke.raw.iter() {
        builder.push(*p)
    }
    if let Some(s) = builder.finish() {
        if let Some(fragment) = &mut stroke.fragment_path {
            // Repeated cuts retain the whole original centerline. A long fragment
            // must not scan that centerline for every point during a width edit.
            let mut nearest = (fragment.len() >= 128 && s.path.len() >= 128)
                .then(|| s.path.iter().copied().enumerate().collect::<Vec<_>>());
            if let Some(points) = &mut nearest {
                build_nearest_tree(points, 0);
            }
            for point in fragment {
                if let Some(points) = &nearest {
                    point.radius = nearest_radius(points, point.position);
                } else if let Some(nearest) = s.path.iter().min_by(|a, b| {
                    a.position
                        .distance(point.position)
                        .total_cmp(&b.position.distance(point.position))
                }) {
                    point.radius = nearest.radius;
                }
            }
        }
        stroke.path = s.path;
    }
    stroke.refined_path = None;
    stroke.refinement_enabled = false;
}

fn coordinate(p: Point, axis: usize) -> f32 {
    if axis == 0 { p.x } else { p.y }
}

/// A balanced, implicit 2-D tree. Median partitioning uses one allocation and
/// bounds recursion to log2(samples), including traces that repeatedly retrace.
fn build_nearest_tree(points: &mut [(usize, PathPoint)], axis: usize) {
    if points.len() < 2 {
        return;
    }
    let middle = points.len() / 2;
    let (left, _, right) = points.select_nth_unstable_by(middle, |a, b| {
        coordinate(a.1.position, axis)
            .total_cmp(&coordinate(b.1.position, axis))
            .then_with(|| a.0.cmp(&b.0))
    });
    build_nearest_tree(left, 1 - axis);
    build_nearest_tree(right, 1 - axis);
}

fn nearest_radius(points: &[(usize, PathPoint)], position: Point) -> f32 {
    fn search(
        points: &[(usize, PathPoint)],
        position: Point,
        axis: usize,
        best: &mut (usize, PathPoint, f32),
    ) {
        if points.is_empty() {
            return;
        }
        let middle = points.len() / 2;
        let (index, point) = points[middle];
        let distance = position.distance(point.position);
        let comparison = distance.total_cmp(&best.2);
        // Match min_by's first-input tie handling despite the tree's reordering.
        if comparison.is_lt() || comparison.is_eq() && index < best.0 {
            *best = (index, point, distance);
        }
        let delta = coordinate(position, axis) - coordinate(point.position, axis);
        let (left, right) = (&points[..middle], &points[middle + 1..]);
        let (near, far) = if delta <= 0. {
            (left, right)
        } else {
            (right, left)
        };
        search(near, position, 1 - axis, best);
        if delta.abs() <= best.2 {
            search(far, position, 1 - axis, best);
        }
    }
    let (index, first) = points[0];
    let mut best = (index, first, position.distance(first.position));
    search(points, position, 0, &mut best);
    best.1.radius
}
/// Same-winding round brush contours, filled once using NonZero. Split at
/// cusps or turns whose curvature is tighter than the brush radius: offset
/// normals would fold there and cut holes through a retraced stroke. Regular
/// portions remain compact outlines, rather than thousands of overlapping disks.
/// Neither raw samples nor processed centerlines are modified by rendering.
pub fn outline(path: &[PathPoint]) -> Vec<Vec<Point>> {
    let samples = render_samples(path);
    let mut contours = vec![];
    let mut run = Vec::with_capacity(samples.len());
    for &p in &samples {
        if let Some(last) = run.last_mut() {
            let last: &mut PathPoint = last;
            if last.position.distance(p.position) < 1e-5 {
                last.radius = last.radius.max(p.radius);
                continue;
            }
        }
        if run.len() >= 2 {
            let a = run[run.len() - 2];
            let b = run[run.len() - 1];
            let incoming = a.position.distance(b.position);
            let outgoing = b.position.distance(p.position);
            let dot = ((b.position.x - a.position.x) * (p.position.x - b.position.x)
                + (b.position.y - a.position.y) * (p.position.y - b.position.y))
                / (incoming * outgoing);
            let angle = dot.clamp(-1., 1.).acos();
            let limit = (incoming.min(outgoing) / a.radius.max(b.radius).max(p.radius).max(0.1)
                * 0.8)
                .min(0.9);
            if angle > limit || (p.radius - b.radius).abs() > outgoing * 0.5 {
                contours.push(round_outline(&run));
                run.clear();
                run.push(b);
            }
        }
        run.push(p);
    }
    if !run.is_empty() {
        contours.push(round_outline(&run));
    }
    contours
}
/// Bound rendering error in both centerline and pressure radius. Process short
/// blocks so simplification cannot become quadratic in a long active stroke.
/// The document continues to own every original processed and raw sample.
fn render_samples(path: &[PathPoint]) -> Vec<PathPoint> {
    if path.len() < 3 {
        return path.to_vec();
    }
    let mut keep = vec![false; path.len()];
    let mut begin = 0;
    let mut pending = Vec::with_capacity(256);
    while begin < path.len() - 1 {
        let end = (begin + 256).min(path.len() - 1);
        keep[begin] = true;
        keep[end] = true;
        pending.push((begin, end));
        while let Some((a, b)) = pending.pop() {
            let start = path[a];
            let end = path[b];
            let dx = end.position.x - start.position.x;
            let dy = end.position.y - start.position.y;
            let length2 = dx * dx + dy * dy;
            let mut worst = 0.;
            let mut index = a;
            for (i, p) in path.iter().enumerate().take(b).skip(a + 1) {
                let t = if length2 < 1e-10 {
                    0.
                } else {
                    ((p.position.x - start.position.x) * dx
                        + (p.position.y - start.position.y) * dy)
                        / length2
                }
                .clamp(0., 1.);
                let error = p.position.distance(start.position.lerp(end.position, t))
                    + (p.radius - (start.radius + (end.radius - start.radius) * t)).abs();
                let tolerance =
                    (p.radius.min(start.radius).min(end.radius) * 0.015).clamp(0.002, 0.03);
                let relative = error / tolerance;
                if relative > worst {
                    worst = relative;
                    index = i;
                }
            }
            if worst > 1. {
                keep[index] = true;
                pending.push((a, index));
                pending.push((index, b));
            }
        }
        begin = end;
    }
    path.iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(*p))
        .collect()
}
fn round_outline(path: &[PathPoint]) -> Vec<Point> {
    if path.len() == 1 {
        return circle(path[0].position, path[0].radius, 32);
    }
    let mut right = Vec::with_capacity(path.len() * 2 + 32);
    let mut left = Vec::with_capacity(path.len());
    for (i, p) in path.iter().enumerate() {
        let a = path[i.saturating_sub(1)].position;
        let b = path[(i + 1).min(path.len() - 1)].position;
        let length = a.distance(b).max(1e-5);
        let n = Point::new(-(b.y - a.y) / length, (b.x - a.x) / length);
        right.push(Point::new(
            p.position.x - n.x * p.radius,
            p.position.y - n.y * p.radius,
        ));
        left.push(Point::new(
            p.position.x + n.x * p.radius,
            p.position.y + n.y * p.radius,
        ));
    }
    let end = path[path.len() - 1];
    let previous = path[path.len() - 2].position;
    let angle = (end.position.y - previous.y).atan2(end.position.x - previous.x);
    for i in 1..=16 {
        let a = angle - PI / 2. + PI * i as f32 / 16.;
        right.push(Point::new(
            end.position.x + a.cos() * end.radius,
            end.position.y + a.sin() * end.radius,
        ));
    }
    right.extend(left.into_iter().rev());
    let start = path[0];
    let next = path[1].position;
    let angle = (next.y - start.position.y).atan2(next.x - start.position.x);
    for i in 1..=16 {
        let a = angle + PI / 2. + PI * i as f32 / 16.;
        right.push(Point::new(
            start.position.x + a.cos() * start.radius,
            start.position.y + a.sin() * start.radius,
        ));
    }
    let area = right
        .iter()
        .zip(right.iter().cycle().skip(1))
        .take(right.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>();
    if area < 0. {
        right.reverse();
    }
    right
}
pub fn circle(center: Point, r: f32, n: usize) -> Vec<Point> {
    (0..n)
        .map(|i| {
            let a = 2. * PI * i as f32 / n as f32;
            Point::new(center.x + r * a.cos(), center.y + r * a.sin())
        })
        .collect()
}
pub fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let d = dx * dx + dy * dy;
    if d < 1e-8 {
        return p.distance(a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / d;
    p.distance(a.lerp(b, t.clamp(0., 1.)))
}
pub fn hit_stroke(stroke: &InkStroke, p: Point, radius: f32) -> bool {
    let Some(inv) = stroke.transform.inverse() else {
        return false;
    };
    let p = inv.apply(p);
    let r = radius / stroke.transform.scale().max(0.001);
    let path = stroke.display_path();
    if path.len() == 1 {
        return p.distance(path[0].position) <= r + path[0].radius;
    }
    path.windows(2).any(|v| {
        let radius = r + v[0].radius.max(v[1].radius);
        let a = v[0].position;
        let b = v[1].position;
        p.x >= a.x.min(b.x) - radius
            && p.x <= a.x.max(b.x) + radius
            && p.y >= a.y.min(b.y) - radius
            && p.y <= a.y.max(b.y) + radius
            && hit_tapered_point(p, a, b, v[0].radius, v[1].radius, r)
    })
}
/// Minimize distance minus the interpolated brush radius, rather than using
/// the wider endpoint radius for the whole segment. The brush is the convex
/// hull of its endpoint disks, including the disks' rounded caps.
fn hit_tapered_point(p: Point, a: Point, b: Point, ra: f32, rb: f32, r: f32) -> bool {
    if ra == rb {
        return segment_distance(p, a, b) <= r + ra;
    }
    let dx = b.x as f64 - a.x as f64;
    let dy = b.y as f64 - a.y as f64;
    let length = dx.hypot(dy);
    let change = rb as f64 - ra as f64;
    if length <= change.abs() {
        // The larger endpoint disk contains every intermediate disk.
        let (center, radius) = if ra >= rb { (a, ra) } else { (b, rb) };
        return p.distance(center) <= r + radius;
    }
    let px = p.x as f64 - a.x as f64;
    let py = p.y as f64 - a.y as f64;
    let along = (px * dx + py * dy) / length;
    let perpendicular = (px * dy - py * dx).abs() / length;
    let offset = change * perpendicular / (length * length - change * change).sqrt();
    let t = ((along + offset) / length).clamp(0., 1.);
    (px - dx * t).hypot(py - dy * t) <= r as f64 + ra as f64 + change * t
}
pub fn swept_hit(stroke: &InkStroke, a: Point, b: Point, r: f32) -> bool {
    let bounds = stroke.bounds();
    if !bounds.intersects(Rect::from_points([a, b]).expand(r)) {
        return false;
    }
    let Some(inv) = stroke.transform.inverse() else {
        return false;
    };
    let a = inv.apply(a);
    let b = inv.apply(b);
    let r = r / stroke.transform.scale().max(0.001);
    let path = stroke.display_path();
    if path.len() == 1 {
        return segment_distance(path[0].position, a, b) <= r + path[0].radius;
    }
    let sweep_bounds = Rect::from_points([a, b]).expand(r);
    path.windows(2).any(|pair| {
        let c = pair[0].position;
        let d = pair[1].position;
        let radius = pair[0].radius.max(pair[1].radius);
        if !sweep_bounds.intersects(Rect::from_points([c, d]).expand(radius)) {
            return false;
        }
        if pair[0].radius == pair[1].radius {
            return segment_pair_distance(a, b, c, d) <= r + radius;
        }
        // If neither centerline crosses, the minimum separation of two convex
        // swept brushes is attained at an endpoint or a tangent side parallel
        // to the sweep (whose endpoints have the same separation).
        segments_cross(a, b, c, d)
            || segment_distance(c, a, b) <= r + pair[0].radius
            || segment_distance(d, a, b) <= r + pair[1].radius
            || hit_tapered_point(a, c, d, pair[0].radius, pair[1].radius, r)
            || hit_tapered_point(b, c, d, pair[0].radius, pair[1].radius, r)
    })
}
fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let cross =
        |a: Point, b: Point, p: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let opposite = |a: f32, b: f32| (a <= 0. && b >= 0.) || (a >= 0. && b <= 0.);
    Rect::from_points([a, b]).intersects(Rect::from_points([c, d]))
        && opposite(cross(a, b, c), cross(a, b, d))
        && opposite(cross(c, d, a), cross(c, d, b))
}
fn segment_pair_distance(a: Point, b: Point, c: Point, d: Point) -> f32 {
    if segments_cross(a, b, c, d) {
        return 0.;
    }
    segment_distance(a, c, d)
        .min(segment_distance(b, c, d))
        .min(segment_distance(c, a, b))
        .min(segment_distance(d, a, b))
}
pub fn inside_polygon(p: Point, polygon: &[Point]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[j];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside
        }
        j = i;
    }
    inside
}
/// Optional, non-destructive refinement. Two local smoothing passes retain
/// personal letter structure; raw and ordinary paths remain untouched.
pub fn refine(stroke: &mut InkStroke) {
    let mut path = stroke
        .fragment_path
        .as_deref()
        .unwrap_or(stroke.path.as_slice())
        .to_vec();
    for _ in 0..2 {
        // Keep the original previous sample as we overwrite the output in place.
        // The next sample has not been changed yet, matching a separate input buffer.
        let Some(mut previous) = path.first().copied() else {
            break;
        };
        for i in 1..path.len().saturating_sub(1) {
            let current = path[i];
            path[i].position = current
                .position
                .lerp(previous.position.lerp(path[i + 1].position, 0.5), 0.35);
            previous = current;
        }
    }
    stroke.refined_path = Some(path);
    stroke.refinement_enabled = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stroke() -> InkStroke {
        let mut b = StrokeBuilder::new(PenStyle::default());
        for i in 0..10 {
            b.push(StrokePoint::new(Point::new(i as f32 * 10., 0.), 0.7, i * 8))
        }
        b.finish().unwrap()
    }
    #[test]
    fn raw_samples_survive() {
        let s = stroke();
        assert_eq!(s.raw.len(), 10);
        assert!(s.path.len() > s.raw.len());
        assert!(!outline(&s.path).is_empty());
    }
    #[test]
    fn pressure_changes_width() {
        let a = radius(&PenStyle::default(), 0.1, 0., 0.);
        let b = radius(&PenStyle::default(), 1., 0., 0.);
        assert!(b > 2. * a);
    }
    #[test]
    fn transformed_hit_testing() {
        let mut s = stroke();
        s.transform = Transform::translate(50., 100.);
        assert!(hit_stroke(&s, Point::new(90., 100.), 1.));
        assert!(!hit_stroke(&s, Point::new(90., 110.), 1.));
        assert!(swept_hit(
            &s,
            Point::new(90., 80.),
            Point::new(90., 120.),
            1.
        ));
    }
    #[test]
    fn polygon_selection() {
        let p = [
            Point::new(0., 0.),
            Point::new(10., 0.),
            Point::new(10., 10.),
            Point::new(0., 10.),
        ];
        assert!(inside_polygon(Point::new(4., 6.), &p));
        assert!(!inside_polygon(Point::new(14., 6.), &p));
    }
    #[test]
    fn reject_invalid_sample() {
        let mut b = StrokeBuilder::new(PenStyle::default());
        b.push(StrokePoint::new(Point::new(f32::NAN, 0.), 0.5, 0));
        assert!(b.finish().is_none());
    }
    #[test]
    fn refinement_keeps_original() {
        let mut s = stroke();
        let before = s.clone();
        refine(&mut s);
        assert_eq!(s.raw, before.raw);
        assert_eq!(s.path, before.path);
        assert!(s.refinement_enabled);
    }
    #[test]
    fn in_place_refinement_matches_simultaneous_neighbor_updates() {
        let mut s = stroke();
        for count in [0, 1, 2, 3, 4096] {
            let input = (0..count)
                .map(|i| PathPoint {
                    position: Point::new(i as f32, (i as f32 * 0.17).sin() * 8.),
                    radius: 1. + i as f32 * 0.001,
                })
                .collect::<Vec<_>>();
            s.fragment_path = Some(input.clone());
            let mut expected = input;
            for _ in 0..2 {
                let original = expected.clone();
                for i in 1..expected.len().saturating_sub(1) {
                    expected[i].position = original[i].position.lerp(
                        original[i - 1].position.lerp(original[i + 1].position, 0.5),
                        0.35,
                    );
                }
            }
            refine(&mut s);
            assert_eq!(s.refined_path.as_ref().unwrap(), &expected);
        }
    }
    #[test]
    fn indexed_fragment_radii_match_brute_force_with_retraces_and_ties() {
        let path = (0..2048)
            .map(|i| PathPoint {
                position: Point::new(
                    (i as f32 * 0.09).sin() * 100.,
                    (i as f32 * 0.13).cos() * 50.,
                ),
                radius: i as f32 + 0.1,
            })
            .collect::<Vec<_>>();
        let mut indexed = path.iter().copied().enumerate().collect::<Vec<_>>();
        build_nearest_tree(&mut indexed, 0);
        for i in 0..300 {
            let position = Point::new(
                (i as f32 * 0.17).cos() * 110.,
                (i as f32 * 0.11).sin() * 60.,
            );
            let nearest = path
                .iter()
                .min_by(|a, b| {
                    a.position
                        .distance(position)
                        .total_cmp(&b.position.distance(position))
                })
                .unwrap();
            assert_eq!(nearest_radius(&indexed, position), nearest.radius);
        }
        let tied = [
            PathPoint {
                position: Point::new(-1., 0.),
                radius: 2.,
            },
            PathPoint {
                position: Point::new(1., 0.),
                radius: 7.,
            },
            PathPoint {
                position: Point::new(-1., 0.),
                radius: 9.,
            },
        ];
        let mut indexed = tied.iter().copied().enumerate().collect::<Vec<_>>();
        build_nearest_tree(&mut indexed, 0);
        assert_eq!(nearest_radius(&indexed, Point::default()), 2.);
        assert_eq!(nearest_radius(&indexed, Point::new(-1., 0.)), 2.);
    }
    #[test]
    fn swept_hit_crosses_sparse_segments_and_handles_degenerate_sweeps() {
        let mut s = stroke();
        s.path = vec![
            PathPoint {
                position: Point::new(0., 0.),
                radius: 1.,
            },
            PathPoint {
                position: Point::new(100., 0.),
                radius: 1.,
            },
        ]
        .into();
        for transform in [
            Transform::default(),
            Transform::translate(100., 200.).compose(Transform::around(Point::default(), 2., 0.6)),
        ] {
            s.transform = transform;
            let apply = |x, y| transform.apply(Point::new(x, y));
            let scale = transform.scale();
            assert!(swept_hit(
                &s,
                apply(50., -10.),
                apply(50., 10.),
                0.5 * scale
            ));
            assert!(swept_hit(&s, apply(50., 1.), apply(50., 1.), 0.5 * scale));
            assert!(!swept_hit(&s, apply(50., 3.), apply(60., 3.), 0.5 * scale));
            assert!(!swept_hit(
                &s,
                apply(110., -10.),
                apply(110., 10.),
                0.5 * scale
            ));
        }
    }
    #[test]
    fn sparse_tapers_do_not_hit_near_the_narrow_end_or_miss_the_wide_cap() {
        let mut s = stroke();
        s.path = vec![
            PathPoint {
                position: Point::new(0., 0.),
                radius: 1.,
            },
            PathPoint {
                position: Point::new(100., 0.),
                radius: 20.,
            },
        ]
        .into();
        for transform in [
            Transform::default(),
            Transform::translate(200., 100.).compose(Transform::around(Point::default(), 2., 0.4)),
        ] {
            s.transform = transform;
            let apply = |x, y| transform.apply(Point::new(x, y));
            let radius = 0.5 * transform.scale();
            assert!(!swept_hit(&s, apply(5., 10.), apply(10., 10.), radius));
            assert!(!hit_stroke(&s, apply(5., 10.), radius));
            assert!(swept_hit(&s, apply(90., 10.), apply(95., 10.), radius));
            assert!(hit_stroke(&s, apply(119., 0.), radius));
            assert!(!hit_stroke(&s, apply(122., 0.), radius));
            assert!(swept_hit(&s, apply(50., -30.), apply(50., 30.), radius));
        }
        // A radius jump steeper than travel is contained by the larger disk.
        assert!(hit_tapered_point(
            Point::new(0., 4.),
            Point::default(),
            Point::new(1., 0.),
            1.,
            5.,
            0.
        ));
    }
    #[test]
    fn tapered_sweeps_agree_with_dense_disk_union_away_from_the_boundary() {
        let mut s = stroke();
        for (ra, rb) in [(1., 20.), (20., 1.), (1., 150.), (4., 4.)] {
            s.path = vec![
                PathPoint {
                    position: Point::default(),
                    radius: ra,
                },
                PathPoint {
                    position: Point::new(100., 0.),
                    radius: rb,
                },
            ]
            .into();
            for i in 0..64 {
                let a = Point::new(
                    (i as f32 * 0.37).sin() * 80. + 50.,
                    (i as f32 * 0.23).cos() * 40.,
                );
                let b = Point::new(
                    a.x + (i as f32 * 0.67).cos() * 20.,
                    a.y + (i as f32 * 0.13).sin() * 25.,
                );
                let radius = 0.5;
                let clearance = (0..=2048)
                    .map(|j| {
                        let t = j as f32 / 2048.;
                        segment_distance(Point::new(t * 100., 0.), a, b)
                            - (radius + ra + (rb - ra) * t)
                    })
                    .fold(f32::INFINITY, f32::min);
                // The sampled oracle can differ from the continuous minimum by
                // at most the travel/radius change in one subdivision.
                if clearance.abs() > 0.1 {
                    assert_eq!(
                        swept_hit(&s, a, b, radius),
                        clearance < 0.,
                        "radii={ra},{rb}, sweep={a:?}->{b:?}, clearance={clearance}"
                    );
                }
            }
        }
    }
}

/// Cached opaque chunks overlap by their shared round endpoint. Translucent
/// ink must use one compound fill for the entire active stroke instead.
pub fn outline_chunk(path: &[PathPoint], start: usize, end: usize) -> Vec<Vec<Point>> {
    if path.is_empty() || start >= path.len() {
        return vec![];
    }
    outline(&path[start..=end.min(path.len() - 1).max(start)])
}

/// Cut a swept capsule from the visible path. Every resulting object retains
/// the entire original raw trace and processed path for non-destructive editing.
pub fn cut_segment(stroke: &InkStroke, a: Point, b: Point, radius: f32) -> Vec<InkStroke> {
    if !stroke
        .bounds()
        .intersects(Rect::from_points([a, b]).expand(radius))
    {
        return vec![stroke.clone()];
    }
    let Some(inv) = stroke.transform.inverse() else {
        return vec![stroke.clone()];
    };
    let a = inv.apply(a);
    let b = inv.apply(b);
    let radius = radius / stroke.transform.scale().max(0.001);
    let mut runs: Vec<Vec<PathPoint>> = vec![];
    // Resample long segments as imported geometry may be sparsely sampled.
    let path = stroke.display_path();
    let mut run = Vec::with_capacity(path.len().min(4096));
    let mut removed = false;
    let sweep_bounds = Rect::from_points([a, b]).expand(radius);
    let mut consume = |p: PathPoint| {
        if p.position.x >= sweep_bounds.min.x - p.radius
            && p.position.x <= sweep_bounds.max.x + p.radius
            && p.position.y >= sweep_bounds.min.y - p.radius
            && p.position.y <= sweep_bounds.max.y + p.radius
            && segment_distance(p.position, a, b) <= radius + p.radius
        {
            removed = true;
            if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
        } else {
            run.push(p);
        }
    };
    if let Some(&p) = path.first() {
        consume(p);
    }
    for pair in path.windows(2) {
        let steps = (pair[0].position.distance(pair[1].position) / radius.clamp(0.5, 2.))
            .ceil()
            .clamp(1., 10000.) as usize;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            consume(PathPoint {
                position: pair[0].position.lerp(pair[1].position, t),
                radius: pair[0].radius + (pair[1].radius - pair[0].radius) * t,
            });
        }
    }
    if !removed {
        return vec![stroke.clone()];
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs.into_iter()
        .map(|run| {
            // Clone only the retained data. Existing fragments and refinements
            // can be large and are discarded in every newly cut piece.
            InkStroke {
                id: Id::new_v4(),
                raw: stroke.raw.clone(),
                path: stroke.path.clone(),
                style: stroke.style.clone(),
                transform: stroke.transform,
                created_at: stroke.created_at,
                fragment_path: Some(run),
                refined_path: None,
                refinement_enabled: false,
            }
        })
        .collect()
}
#[cfg(test)]
mod cut_tests {
    use super::*;
    #[test]
    fn cuts_sparse_and_refined_paths_without_mutating_originals() {
        let mut builder = StrokeBuilder::new(PenStyle::default());
        builder.push(StrokePoint::new(Point::new(0., 0.), 0.7, 0));
        builder.push(StrokePoint::new(Point::new(100., 0.), 0.7, 8));
        let mut original = builder.finish().unwrap();
        original.path = vec![
            PathPoint {
                position: Point::new(0., 0.),
                radius: 1.,
            },
            PathPoint {
                position: Point::new(100., 0.),
                radius: 1.,
            },
        ]
        .into();
        original.refined_path = Some(original.path.to_vec());
        original.refinement_enabled = true;
        original.transform = Transform::translate(50., 40.);
        let pieces = cut_segment(&original, Point::new(100., 20.), Point::new(100., 60.), 4.);
        assert_eq!(pieces.len(), 2);
        for part in &pieces {
            assert!(std::sync::Arc::ptr_eq(&original.raw, &part.raw));
            assert!(std::sync::Arc::ptr_eq(&original.path, &part.path));
            assert_eq!(part.transform, original.transform);
            assert_eq!(part.created_at, original.created_at);
            assert!(part.refined_path.is_none());
            assert!(!part.refinement_enabled);
            assert!(
                part.fragment_path
                    .as_ref()
                    .unwrap()
                    .iter()
                    .all(|p| p.position.distance(Point::new(50., 0.)) > 5.)
            );
        }
        let untouched = cut_segment(&original, Point::new(0., 200.), Point::new(200., 200.), 4.);
        assert_eq!(untouched, vec![original]);
    }
    #[test]
    fn segment_cut_preserves_originals_and_does_not_resurrect_on_restyle() {
        let mut b = StrokeBuilder::new(PenStyle::default());
        for i in 0..101 {
            b.push(StrokePoint::new(Point::new(i as f32, 20.), 0.8, i * 8));
        }
        let original = b.finish().unwrap();
        let mut pieces = cut_segment(&original, Point::new(50., 0.), Point::new(50., 40.), 5.);
        assert_eq!(pieces.len(), 2);
        for part in &mut pieces {
            assert_eq!(part.raw, original.raw);
            assert_eq!(part.path, original.path);
            part.style.width = 5.;
            rebuild(part);
            assert!(!hit_stroke(part, Point::new(50., 20.), 1.));
        }
    }
    #[test]
    fn opaque_chunks_share_round_endpoint_geometry() {
        let points = (0..1024)
            .map(|i| PathPoint {
                position: Point::new(i as f32, (i as f32 * 0.01).sin() * 30.),
                radius: 2.,
            })
            .collect::<Vec<_>>();
        let a = outline_chunk(&points, 256, 512);
        let b = outline_chunk(&points, 512, 768);
        let joint = points[512].position;
        assert!(a.iter().flatten().any(|p| p.distance(joint) <= 2.01));
        assert!(b.iter().flatten().any(|p| p.distance(joint) <= 2.01));
    }
}

#[cfg(test)]
mod contour_tests {
    use super::*;
    #[test]
    fn brush_contours_share_winding_even_at_zero_length_and_radius_jumps() {
        let points = [
            PathPoint {
                position: Point::new(10., 10.),
                radius: 2.,
            },
            PathPoint {
                position: Point::new(50., 10.),
                radius: 4.,
            },
            PathPoint {
                position: Point::new(10., 10.),
                radius: 2.,
            },
            PathPoint {
                position: Point::new(10., 10.),
                radius: 9.,
            },
            PathPoint {
                position: Point::new(12., 10.),
                radius: 1.,
            },
        ];
        for contour in outline(&points) {
            let area = contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
                .map(|(a, b)| a.x * b.y - b.x * a.y)
                .sum::<f32>();
            assert!(
                area > 0.,
                "All contours must add coverage rather than cancel it"
            );
            assert!(contour.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        }
    }
    #[test]
    fn rendering_simplification_bounds_position_and_pressure_error() {
        let input = (0..4096)
            .map(|i| PathPoint {
                position: Point::new(i as f32 * 1.2, (i as f32 * 0.025).sin() * 25.),
                radius: 2. + (i as f32 * 0.015).sin() * 0.5,
            })
            .collect::<Vec<_>>();
        let reduced = render_samples(&input);
        assert!(reduced.len() < input.len() / 2);
        for p in &input {
            let pair = reduced
                .windows(2)
                .find(|pair| {
                    pair[0].position.x <= p.position.x && pair[1].position.x >= p.position.x
                })
                .unwrap();
            let a = pair[0];
            let b = pair[1];
            let dx = b.position.x - a.position.x;
            let dy = b.position.y - a.position.y;
            let t = ((p.position.x - a.position.x) * dx + (p.position.y - a.position.y) * dy)
                / (dx * dx + dy * dy);
            let error = p.position.distance(a.position.lerp(b.position, t))
                + (p.radius - (a.radius + (b.radius - a.radius) * t)).abs();
            assert!(error <= 0.0301, "Render-only error {error}");
        }
        assert_eq!(input.len(), 4096);
    }
}
