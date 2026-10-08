use folio_document::{Point, Rect};

pub(crate) fn clean(points: &[Point]) -> Option<Vec<Point>> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        if out.last().is_none_or(|last| last.distance(p) > 0.01) {
            out.push(p);
        }
    }
    (out.len() >= 2).then_some(out)
}

pub(crate) fn length(points: &[Point]) -> f32 {
    points.windows(2).map(|p| p[0].distance(p[1])).sum()
}

pub(crate) fn principal_axis(points: &[Point]) -> Point {
    let mean = Point::new(
        points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32,
        points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32,
    );
    let (mut xx, mut yy, mut xy) = (0., 0., 0.);
    for p in points {
        let (x, y) = (p.x - mean.x, p.y - mean.y);
        xx += x * x;
        yy += y * y;
        xy += x * y;
    }
    let angle = 0.5 * (2. * xy).atan2(xx - yy);
    Point::new(angle.cos(), angle.sin())
}

pub(crate) fn project(p: Point, axis: Point) -> Point {
    Point::new(p.x * axis.x + p.y * axis.y, -p.x * axis.y + p.y * axis.x)
}

/// The direction of repeated travel, rather than the longest side of the
/// scribble's footprint. Weight by arc length so report rate has no influence.
pub(crate) fn traversal_axis(points: &[Point]) -> Point {
    let (mut xx, mut yy, mut xy) = (0., 0., 0.);
    for pair in points.windows(2) {
        let (x, y) = (pair[1].x - pair[0].x, pair[1].y - pair[0].y);
        let length = x.hypot(y).max(0.001);
        xx += x * x / length;
        yy += y * y / length;
        xy += x * y / length;
    }
    let angle = 0.5 * (2. * xy).atan2(xx - yy);
    Point::new(angle.cos(), angle.sin())
}

/// Geometry decisions must not depend on the tablet's report rate or dwell
/// samples. Resample by arc length without modifying the document's raw ink.
pub(crate) fn resample(points: &[Point], count: usize) -> Vec<Point> {
    let total = length(points);
    if total <= 0.01 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    let mut segment = 0;
    let mut covered = 0.;
    for i in 1..count - 1 {
        let at = total * i as f32 / (count - 1) as f32;
        while segment + 2 < points.len()
            && covered + points[segment].distance(points[segment + 1]) < at
        {
            covered += points[segment].distance(points[segment + 1]);
            segment += 1;
        }
        let a = points[segment];
        let b = points[segment + 1];
        out.push(a.lerp(b, ((at - covered) / a.distance(b).max(0.001)).clamp(0., 1.)));
    }
    out.push(*points.last().unwrap());
    out
}

pub(crate) fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f32 {
    let cross =
        |a: Point, b: Point, p: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    if Rect::from_points([a, b]).intersects(Rect::from_points([c, d]))
        && cross(a, b, c) * cross(a, b, d) <= 0.
        && cross(c, d, a) * cross(c, d, b) <= 0.
    {
        return 0.;
    }
    folio_ink::segment_distance(a, c, d)
        .min(folio_ink::segment_distance(b, c, d))
        .min(folio_ink::segment_distance(c, a, b))
        .min(folio_ink::segment_distance(d, a, b))
}
