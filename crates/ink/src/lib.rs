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
            for point in fragment {
                if let Some(nearest) = s.path.iter().min_by(|a, b| {
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
/// Same-winding round brush contours, filled once using NonZero. Split at
/// cusps or turns whose curvature is tighter than the brush radius: offset
/// normals would fold there and cut holes through a retraced stroke. Regular
/// portions remain compact outlines, rather than thousands of overlapping disks.
/// Neither raw samples nor processed centerlines are modified by rendering.
pub fn outline(path: &[PathPoint]) -> Vec<Vec<Point>> {
    let samples = render_samples(path);
    let mut contours = vec![];
    let mut run = vec![];
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
    while begin < path.len() - 1 {
        let end = (begin + 256).min(path.len() - 1);
        keep[begin] = true;
        keep[end] = true;
        let mut pending = vec![(begin, end)];
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
    let mut right = Vec::with_capacity(path.len());
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
        segment_distance(p, v[0].position, v[1].position) <= r + v[0].radius.max(v[1].radius)
    })
}
pub fn swept_hit(stroke: &InkStroke, a: Point, b: Point, r: f32) -> bool {
    let bounds = Rect::from_points(
        stroke
            .display_path()
            .iter()
            .map(|p| stroke.transform.apply(p.position)),
    )
    .expand(
        stroke
            .display_path()
            .iter()
            .map(|p| p.radius)
            .fold(0., f32::max)
            * stroke.transform.scale(),
    );
    if !bounds.intersects(Rect::from_points([a, b]).expand(r)) {
        return false;
    }
    let Some(inv) = stroke.transform.inverse() else {
        return false;
    };
    let a = inv.apply(a);
    let b = inv.apply(b);
    let r = r / stroke.transform.scale().max(0.001);
    stroke
        .display_path()
        .iter()
        .any(|p| segment_distance(p.position, a, b) <= r + p.radius)
        || hit_stroke(
            stroke,
            stroke.transform.apply(a),
            r * stroke.transform.scale(),
        )
        || hit_stroke(
            stroke,
            stroke.transform.apply(b),
            r * stroke.transform.scale(),
        )
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
        let before = path.clone();
        for i in 1..path.len().saturating_sub(1) {
            path[i].position = before[i].position.lerp(
                before[i - 1].position.lerp(before[i + 1].position, 0.5),
                0.35,
            )
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
    let Some(inv) = stroke.transform.inverse() else {
        return vec![stroke.clone()];
    };
    let a = inv.apply(a);
    let b = inv.apply(b);
    let radius = radius / stroke.transform.scale().max(0.001);
    let mut runs: Vec<Vec<PathPoint>> = vec![];
    let mut run = vec![];
    // Resample long segments as imported geometry may be sparsely sampled.
    let path = stroke.display_path();
    let mut samples = Vec::new();
    if let Some(p) = path.first() {
        samples.push(*p);
    }
    for pair in path.windows(2) {
        let steps = (pair[0].position.distance(pair[1].position) / radius.clamp(0.5, 2.))
            .ceil()
            .clamp(1., 10000.) as usize;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            samples.push(PathPoint {
                position: pair[0].position.lerp(pair[1].position, t),
                radius: pair[0].radius + (pair[1].radius - pair[0].radius) * t,
            });
        }
    }
    let mut removed = false;
    for p in samples {
        if segment_distance(p.position, a, b) <= radius + p.radius {
            removed = true;
            if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
        } else {
            run.push(p);
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
            let mut part = stroke.clone();
            part.id = Id::new_v4();
            part.fragment_path = Some(run);
            part.refined_path = None;
            part.refinement_enabled = false;
            part
        })
        .collect()
}
#[cfg(test)]
mod cut_tests {
    use super::*;
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
