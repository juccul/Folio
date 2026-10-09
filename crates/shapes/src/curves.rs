use crate::{Fit, geometry};
use folio_document::*;
use std::f32::consts::{PI, TAU};

fn circle(points: &[Point]) -> Option<(Point, f32, f32)> {
    // Algebraic least squares in normalized coordinates; no model or SDK.
    let mut matrix = [[0_f64; 4]; 3];
    for p in points {
        let row = [p.x as f64, p.y as f64, 1.];
        let value = row[0] * row[0] + row[1] * row[1];
        for i in 0..3 {
            for j in 0..3 {
                matrix[i][j] += row[i] * row[j];
            }
            matrix[i][3] += row[i] * value;
        }
    }
    for column in 0..3 {
        let pivot = (column..3)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))?;
        matrix.swap(column, pivot);
        let divisor = matrix[column][column];
        if divisor.abs() < 1e-9 {
            return None;
        }
        for value in &mut matrix[column][column..] {
            *value /= divisor;
        }
        let pivot_row = matrix[column];
        for (i, row) in matrix.iter_mut().enumerate() {
            if i != column {
                let factor = row[column];
                for (value, pivot) in row[column..].iter_mut().zip(&pivot_row[column..]) {
                    *value -= factor * pivot;
                }
            }
        }
    }
    let center = Point::new(matrix[0][3] as f32 / 2., matrix[1][3] as f32 / 2.);
    let radius = points.iter().map(|p| p.distance(center)).sum::<f32>() / points.len() as f32;
    if !(0.08..=1.5).contains(&radius) {
        return None;
    }
    let error = points
        .iter()
        .map(|p| (p.distance(center) / radius - 1.).abs())
        .sum::<f32>()
        / points.len() as f32;
    Some((center, radius, error))
}
fn sweep(points: &[Point], center: Point, rx: f32, ry: f32) -> Option<(f32, f32)> {
    let angle = |p: Point| ((p.y - center.y) / ry).atan2((p.x - center.x) / rx);
    let start = angle(points[0]);
    let mut previous = start;
    let mut signed = 0.;
    let mut absolute = 0.;
    for &p in &points[1..] {
        let current = angle(p);
        let delta = (current - previous + PI).rem_euclid(TAU) - PI;
        signed += delta;
        absolute += delta.abs();
        previous = current;
    }
    if absolute > signed.abs() * 1.3 {
        return None;
    }
    Some((start, signed))
}
pub(crate) fn fit(points: &[Point]) -> Option<Fit> {
    if let Some((center, radius, error)) = circle(points)
        && error < 0.085
        && let Some((start, turn)) = sweep(points, center, radius, radius)
    {
        let bounds = Rect::from_points(points.iter().copied());
        let aspect = bounds.width() / bounds.height().max(1e-8);
        let closed = (0.80..=1.25).contains(&(turn.abs() / TAU));
        if closed && (0.78..=1.28).contains(&aspect) {
            return Some(Fit {
                kind: ShapeKind::Circle,
                vertices: (0..=96)
                    .map(|i| {
                        let a = i as f32 / 96. * TAU;
                        Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
                    })
                    .collect(),
                confidence: (1. - error * 1.2).min(0.99),
            });
        }
        if (0.20..0.80).contains(&(turn.abs() / TAU)) && error < 0.065 {
            return Some(Fit {
                kind: ShapeKind::Arc,
                vertices: (0..=80)
                    .map(|i| {
                        let a = start + turn * i as f32 / 80.;
                        Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
                    })
                    .collect(),
                confidence: (1. - error * 1.5).min(0.98),
            });
        }
    }
    // Arc-length balanced PCA gives the orientation of a closed ellipse even
    // when tablet reports or the closing dwell are unevenly distributed.
    let axis = geometry::axis(points);
    let projected = points
        .iter()
        .map(|p| Point::new(p.x * axis.x + p.y * axis.y, -p.x * axis.y + p.y * axis.x))
        .collect::<Vec<_>>();
    let bounds = Rect::from_points(projected.iter().copied());
    let center = bounds.center();
    let rx = bounds.width() / 2.;
    let ry = bounds.height() / 2.;
    if rx.min(ry) < 0.06 {
        return None;
    }
    let error = projected
        .iter()
        .map(|p| (((p.x - center.x) / rx).hypot((p.y - center.y) / ry) - 1.).abs())
        .sum::<f32>()
        / projected.len() as f32;
    let (_, turn) = sweep(&projected, center, rx, ry)?;
    if error > 0.09 || !(0.80..=1.25).contains(&(turn.abs() / TAU)) {
        return None;
    }
    let vertices = (0..=96)
        .map(|i| {
            let a = i as f32 / 96. * TAU;
            let p = Point::new(center.x + rx * a.cos(), center.y + ry * a.sin());
            Point::new(p.x * axis.x - p.y * axis.y, p.x * axis.y + p.y * axis.x)
        })
        .collect();
    Some(Fit {
        kind: ShapeKind::Ellipse,
        vertices,
        confidence: (1. - error * 1.2).min(0.99),
    })
}
