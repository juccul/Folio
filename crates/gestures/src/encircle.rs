use crate::geometry;
use folio_document::*;
use std::f32::consts::{PI, TAU};

/// Accept a rough oval with a small gap or modest overrun. Endpoint distance
/// alone cannot distinguish a real encircling stroke from a hook or figure eight.
pub fn selection_loop(points: &[Point]) -> Option<Vec<Point>> {
    selection_loop_with_scale(points, 1.)
}

/// Apply the minimum gesture size in screen pixels while returning the polygon
/// in the original document coordinates. `scale` is the viewport zoom.
pub fn selection_loop_with_scale(points: &[Point], scale: f32) -> Option<Vec<Point>> {
    if !scale.is_finite() || scale <= 0. {
        return None;
    }
    let points = geometry::clean(points)?;
    if points.len() < 6 {
        return None;
    }
    let path = geometry::resample(&points, 128);
    let axis = geometry::principal_axis(&path);
    let projected: Vec<_> = path.iter().map(|&p| geometry::project(p, axis)).collect();
    let bounds = Rect::from_points(projected.iter().copied());
    let diagonal = bounds.width().hypot(bounds.height());
    if diagonal * scale < 32. || bounds.width().min(bounds.height()) * scale < 12. {
        return None;
    }
    let first = projected[0];
    let last = *projected.last()?;
    if ((first.x - last.x) * 2. / bounds.width()).hypot((first.y - last.y) * 2. / bounds.height())
        > 0.95
    {
        return None;
    }
    let center = bounds.center();
    let mut signed_turn = 0.;
    let mut absolute_turn = 0.;
    for pair in projected.windows(2) {
        let a = ((pair[0].y - center.y) / bounds.height())
            .atan2((pair[0].x - center.x) / bounds.width());
        let b = ((pair[1].y - center.y) / bounds.height())
            .atan2((pair[1].x - center.x) / bounds.width());
        let delta = (b - a + PI).rem_euclid(TAU) - PI;
        signed_turn += delta;
        absolute_turn += delta.abs();
    }
    if signed_turn.abs() < TAU * 0.78
        || signed_turn.abs() > TAU * 1.28
        || absolute_turn > signed_turn.abs() * 1.25
    {
        return None;
    }
    let area = path
        .iter()
        .zip(path.iter().cycle().skip(1))
        .take(path.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>()
        .abs()
        * 0.5;
    if area < bounds.width() * bounds.height() * 0.35 {
        return None;
    }
    // Trim an overrun to its first return near the beginning. Otherwise an
    // overdrawn oval produces a doubled wedge and even/odd containment cancels it.
    let mut end = path.len() - 1;
    if signed_turn.abs() > TAU * 1.02 {
        let mut best = f32::INFINITY;
        for (i, &p) in path.iter().enumerate().skip(path.len() * 3 / 4) {
            let distance = p.distance(path[0]);
            if distance < best {
                best = distance;
                end = i;
            }
        }
    }
    let mut polygon = path[..=end].to_vec();
    polygon.push(polygon[0]);
    Some(polygon)
}

pub fn closed_loop(points: &[Point]) -> bool {
    selection_loop(points).is_some()
}

/// Automatic selection needs substantially enclosed geometry, rather than an
/// intersecting axis-aligned bounding box. Manual lasso remains more permissive.
pub fn encloses(polygon: &[Point], object: &Object) -> bool {
    let transform = object.transform();
    let points: Vec<_> = match object {
        Object::Stroke(s) => s
            .display_path()
            .iter()
            .map(|p| transform.apply(p.position))
            .collect(),
        Object::Shape(s) => s.vertices.iter().map(|&p| transform.apply(p)).collect(),
        Object::Text(t) => rectangle(t.rect, transform),
        Object::Image(i) => rectangle(i.rect, transform),
        Object::Equation(e) => rectangle(e.rect, transform),
    };
    if points.is_empty() {
        return false;
    }
    let inside = |p| {
        folio_ink::inside_polygon(p, polygon)
            || polygon
                .windows(2)
                .any(|edge| folio_ink::segment_distance(p, edge[0], edge[1]) <= 2.)
    };
    if points.len() == 1 {
        return inside(points[0]);
    }
    let samples = geometry::resample(&points, 96);
    samples.iter().filter(|&&p| inside(p)).count() as f32 >= samples.len() as f32 * 0.9
}

fn rectangle(r: Rect, transform: Transform) -> Vec<Point> {
    [
        r.min,
        Point::new(r.max.x, r.min.y),
        r.max,
        Point::new(r.min.x, r.max.y),
        r.min,
    ]
    .map(|p| transform.apply(p))
    .to_vec()
}
