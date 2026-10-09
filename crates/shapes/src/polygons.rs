use crate::{Fit, geometry};
use folio_document::*;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

pub(crate) fn line(points: &[Point]) -> Option<Fit> {
    let axis = geometry::axis(points);
    let center = geometry::mean(points);
    let projection = |p: Point| (p.x - center.x) * axis.x + (p.y - center.y) * axis.y;
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut squared_error, mut worst_error) = (0., 0_f32);
    for &p in points {
        let along = projection(p);
        min = min.min(along);
        max = max.max(along);
        let error = ((p.x - center.x) * axis.y - (p.y - center.y) * axis.x).abs();
        squared_error += error * error;
        worst_error = worst_error.max(error);
    }
    if max - min < 0.8 || geometry::length(points) > (max - min) * 1.22 {
        return None;
    }
    let rms = (squared_error / points.len() as f32).sqrt();
    if rms > 0.026 || worst_error > 0.065 {
        return None;
    }
    let endpoint = |t| Point::new(center.x + axis.x * t, center.y + axis.y * t);
    let vertices = if projection(points[0]) < projection(*points.last()?) {
        vec![endpoint(min), endpoint(max)]
    } else {
        vec![endpoint(max), endpoint(min)]
    };
    Some(Fit {
        kind: ShapeKind::Line,
        vertices,
        confidence: (1. - rms * 3.).min(0.99),
    })
}

pub(crate) fn arrow(points: &[Point]) -> Option<Fit> {
    let corners = geometry::simplify(points, 0.035);
    if !(4..=7).contains(&corners.len()) {
        return None;
    }
    arrow_graph(points, &corners, &[corners[0], *corners.last()?])
}

pub(crate) fn arrow_graph(points: &[Point], tips: &[Point], tails: &[Point]) -> Option<Fit> {
    let mut best: Option<Fit> = None;
    for &tip in tips {
        for &tail in tails {
            let length = tail.distance(tip);
            if length < 0.65 {
                continue;
            }
            let axis = Point::new((tip.x - tail.x) / length, (tip.y - tail.y) / length);
            let side = |p: Point| (p.x - tip.x) * -axis.y + (p.y - tip.y) * axis.x;
            let back = |p: Point| (tip.x - p.x) * axis.x + (tip.y - p.y) * axis.y;
            let head: Vec<_> = tips
                .iter()
                .copied()
                .filter(|p| back(*p) > length * 0.035 && back(*p) < length * 0.48)
                .collect();
            let Some(left) = head
                .iter()
                .copied()
                .min_by(|a, b| side(*a).total_cmp(&side(*b)))
            else {
                continue;
            };
            let Some(right) = head
                .iter()
                .copied()
                .max_by(|a, b| side(*a).total_cmp(&side(*b)))
            else {
                continue;
            };
            if side(left) > -length * 0.055 || side(right) < length * 0.055 {
                continue;
            }
            let vertices = [tail, tip, left, tip, right];
            let (error, worst) = geometry::residual(points, &vertices, false);
            if error > 0.03 || worst > 0.07 {
                continue;
            }
            let depth = (back(left) + back(right)) * 0.5;
            let half = (side(left).abs() + side(right).abs()) * 0.5;
            if depth < length * 0.07 || half > depth * 1.6 {
                continue;
            }
            let arm = |sign: f32| {
                Point::new(
                    tip.x - axis.x * depth - axis.y * half * sign,
                    tip.y - axis.y * depth + axis.x * half * sign,
                )
            };
            let candidate = Fit {
                kind: ShapeKind::Arrow,
                vertices: vec![tail, tip, arm(-1.), tip, arm(1.)],
                confidence: (1. - error * 3.).min(0.98),
            };
            if best
                .as_ref()
                .is_none_or(|b| candidate.confidence > b.confidence)
            {
                best = Some(candidate);
            }
        }
    }
    best
}

pub(crate) fn closed(points: &[Point]) -> Option<Fit> {
    if points[0].distance(*points.last()?) > 0.28 {
        return None;
    }
    let corners = geometry::closed_corners(points, 0.055);
    if corners.len() == 3 {
        let (error, worst) = geometry::residual(points, &corners, true);
        let area = ((corners[1].x - corners[0].x) * (corners[2].y - corners[0].y)
            - (corners[1].y - corners[0].y) * (corners[2].x - corners[0].x))
            .abs();
        if error < 0.035 && worst < 0.08 && area > 0.10 {
            let mut vertices = corners;
            vertices.push(vertices[0]);
            return Some(Fit {
                kind: ShapeKind::Triangle,
                vertices,
                confidence: (1. - error * 3.).min(0.98),
            });
        }
    }
    // Fit in candidate edge frames, including tilted rectangles. Evidence near
    // all four corners keeps a rounded circle from becoming a bounding box.
    let mut angles = vec![0., FRAC_PI_4];
    angles.extend(
        corners
            .iter()
            .zip(corners.iter().cycle().skip(1))
            .take(corners.len())
            .map(|(a, b)| (b.y - a.y).atan2(b.x - a.x)),
    );
    let mut best: Option<Fit> = None;
    for angle in angles {
        let (sin, cos) = angle.sin_cos();
        let projected = points
            .iter()
            .map(|p| Point::new(p.x * cos + p.y * sin, -p.x * sin + p.y * cos))
            .collect::<Vec<_>>();
        let mut xs = projected.iter().map(|p| p.x).collect::<Vec<_>>();
        let mut ys = projected.iter().map(|p| p.y).collect::<Vec<_>>();
        // Small start/closure hooks should not expand the whole fitted box.
        let low = (points.len() - 1) * 5 / 100;
        let high = (points.len() - 1) * 95 / 100;
        let quantiles = |values: &mut [f32]| {
            let (below, upper, _) = values.select_nth_unstable_by(high, f32::total_cmp);
            let upper = *upper;
            let (_, lower, _) = below.select_nth_unstable_by(low, f32::total_cmp);
            (*lower, upper)
        };
        let (x0, x1) = quantiles(&mut xs);
        let (y0, y1) = quantiles(&mut ys);
        let r = Rect::new(x0, y0, x1 - x0, y1 - y0);
        if r.width().min(r.height()) < 0.08 {
            continue;
        }
        let vertices = [
            r.min,
            Point::new(r.max.x, r.min.y),
            r.max,
            Point::new(r.min.x, r.max.y),
        ];
        if vertices.iter().any(|c| {
            projected
                .iter()
                .map(|p| p.distance(*c))
                .fold(f32::INFINITY, f32::min)
                > 0.09
        }) {
            continue;
        }
        let (error, worst) = geometry::residual(&projected, &vertices, true);
        if error > 0.033 || worst > 0.075 {
            continue;
        }
        let near_square = (r.width() / r.height() - 1.).abs() < 0.2;
        let quarter = angle.rem_euclid(FRAC_PI_2);
        let kind = if near_square && (quarter - FRAC_PI_4).abs() < 0.20 {
            ShapeKind::Diamond
        } else if near_square {
            ShapeKind::Square
        } else {
            ShapeKind::Rectangle
        };
        let mut vertices = vertices
            .into_iter()
            .map(|p| Point::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos))
            .collect::<Vec<_>>();
        vertices.push(vertices[0]);
        let candidate = Fit {
            kind,
            vertices,
            confidence: (1. - error * 3.).min(0.99),
        };
        if best
            .as_ref()
            .is_none_or(|b| candidate.confidence > b.confidence)
        {
            best = Some(candidate);
        }
    }
    best
}

pub(crate) fn polyline(points: &[Point]) -> Option<Fit> {
    // Unlike a fitted circle/polygon, a generic path must not bridge a wide
    // endpoint gap: crossing angular strokes can finish near their start.
    let closed = points[0].distance(*points.last()?) < 0.08;
    let mut vertices = if closed {
        geometry::closed_corners(points, 0.075)
    } else {
        geometry::simplify(points, 0.075)
    };
    if !(3..=7).contains(&vertices.len()) {
        return None;
    }
    if vertices.windows(2).any(|p| p[0].distance(p[1]) < 0.10) {
        return None;
    }
    let (error, worst) = geometry::residual(points, &vertices, closed);
    if error > 0.026 || worst > 0.075 {
        return None;
    }
    // Snap a clear L to an exact right angle while retaining its orientation.
    if !closed && vertices.len() == 3 {
        let a = vertices[0];
        let b = vertices[1];
        let c = vertices[2];
        let incoming = Point::new((b.x - a.x) / a.distance(b), (b.y - a.y) / a.distance(b));
        let dot = ((c.x - b.x) * incoming.x + (c.y - b.y) * incoming.y) / b.distance(c);
        if dot.abs() < 0.20 {
            let cross = (c.x - b.x) * -incoming.y + (c.y - b.y) * incoming.x;
            vertices[2] = Point::new(b.x - incoming.y * cross, b.y + incoming.x * cross);
        }
    }
    if closed {
        vertices.push(vertices[0]);
    }
    Some(Fit {
        kind: ShapeKind::Polyline,
        vertices,
        confidence: (1. - error * 3.).min(0.96),
    })
}
