//! Offline geometric fitting. Work is bounded by arc-length resampling; sensor
//! density, dwell points and drawing direction do not control classification.
mod curves;
mod geometry;
mod polygons;
#[cfg(test)]
mod tests;
use folio_document::*;

#[derive(Clone, Debug)]
pub struct Fit {
    pub kind: ShapeKind,
    pub vertices: Vec<Point>,
    pub confidence: f32,
}

pub fn fit(points: &[Point]) -> Option<Fit> {
    let clean = geometry::clean(points)?;
    let bounds = Rect::from_points(clean.iter().copied());
    let scale = bounds.width().hypot(bounds.height());
    if scale < 16. {
        return None;
    }
    let center = bounds.center();
    let points = geometry::resample(&clean, 192)
        .into_iter()
        .map(|p| Point::new((p.x - center.x) / scale, (p.y - center.y) / scale))
        .collect::<Vec<_>>();
    let fitted = polygons::line(&points)
        .or_else(|| polygons::arrow(&points))
        .or_else(|| polygons::closed(&points))
        .or_else(|| curves::fit(&points))
        .or_else(|| polygons::polyline(&points))?;
    Some(Fit {
        vertices: fitted
            .vertices
            .into_iter()
            .map(|p| Point::new(center.x + p.x * scale, center.y + p.y * scale))
            .collect(),
        ..fitted
    })
}

/// Fit an arrow drawn as two or three traces (a shaft plus a V head or two
/// separate barbs, in any order) without introducing an artificial segment
/// between traces. Each trace keeps its own source ID in the
/// application/document layer.
pub fn fit_strokes(strokes: &[&[Point]]) -> Option<Fit> {
    // Normalized distance a pen lift may leave between traces at the tip.
    const GAP: f32 = 0.08;
    if !(2..=3).contains(&strokes.len()) {
        return None;
    }
    let clean = strokes
        .iter()
        .map(|p| geometry::clean(p))
        .collect::<Option<Vec<_>>>()?;
    let bounds = Rect::from_points(clean.iter().flatten().copied());
    let scale = bounds.width().hypot(bounds.height());
    if scale < 16. {
        return None;
    }
    let center = bounds.center();
    let total = clean.iter().map(|p| geometry::length(p)).sum::<f32>();
    let paths = clean
        .iter()
        .map(|p| {
            // Sample by arc length so a short barb does not outweigh the shaft.
            let count = ((192. * geometry::length(p) / total) as usize).max(24);
            geometry::resample(p, count)
                .into_iter()
                .map(|p| Point::new((p.x - center.x) / scale, (p.y - center.y) / scale))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    // Each trace is a shaft, a barb or a V head; dots and scribbles are not.
    if paths.iter().any(|p| geometry::length(p) < 0.06) {
        return None;
    }
    let corners = paths
        .iter()
        .map(|p| {
            polygons::ARROW_CORNER_TOLERANCES
                .iter()
                .map(|&epsilon| geometry::simplify(p, epsilon))
                .find(|c| c.len() <= 4)
        })
        .collect::<Option<Vec<_>>>()?;
    // Cheaply reject traces that do not meet another one before fitting.
    let meets = |i: usize| {
        corners.iter().enumerate().any(|(j, b)| {
            j != i
                && corners[i]
                    .iter()
                    .any(|p| b.iter().any(|q| p.distance(*q) < GAP))
        })
    };
    if !(0..corners.len()).all(meets) {
        return None;
    }
    let mut tips = corners.iter().flatten().copied().collect::<Vec<_>>();
    // Traces that stop just short of each other meet midway across the gap.
    for (i, a) in corners.iter().enumerate() {
        for b in &corners[i + 1..] {
            for &p in a {
                tips.extend(
                    b.iter()
                        .filter(|q| p.distance(**q) < GAP)
                        .map(|&q| p.lerp(q, 0.5)),
                );
            }
        }
    }
    let tails = corners
        .iter()
        .flat_map(|p| [p[0], *p.last().unwrap()])
        .collect::<Vec<_>>();
    let points = paths.iter().flatten().copied().collect::<Vec<_>>();
    let fit = polygons::arrow_graph(&points, &tips, &tails)?;
    // Every trace must reach the tip, so nearby unrelated ink is not absorbed.
    let tip = fit.vertices[1];
    if paths
        .iter()
        .any(|p| p.iter().all(|q| q.distance(tip) > GAP))
    {
        return None;
    }
    Some(Fit {
        vertices: fit
            .vertices
            .into_iter()
            .map(|p| Point::new(center.x + p.x * scale, center.y + p.y * scale))
            .collect(),
        ..fit
    })
}
