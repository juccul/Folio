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

/// Fit a two-stroke arrow without introducing an artificial segment between
/// traces. Each trace keeps its own source ID in the application/document layer.
pub fn fit_strokes(strokes: &[&[Point]]) -> Option<Fit> {
    if strokes.len() != 2 {
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
    let paths = clean
        .iter()
        .map(|p| {
            geometry::resample(p, 96)
                .into_iter()
                .map(|p| Point::new((p.x - center.x) / scale, (p.y - center.y) / scale))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let corners = paths
        .iter()
        .map(|p| geometry::simplify(p, 0.035))
        .collect::<Vec<_>>();
    if corners.iter().any(|p| !(2..=4).contains(&p.len())) {
        return None;
    }
    if !corners[0]
        .iter()
        .any(|a| corners[1].iter().any(|b| a.distance(*b) < 0.055))
    {
        return None;
    }
    let tips = corners.iter().flatten().copied().collect::<Vec<_>>();
    let tails = corners
        .iter()
        .flat_map(|p| [p[0], *p.last().unwrap()])
        .collect::<Vec<_>>();
    let points = paths.into_iter().flatten().collect::<Vec<_>>();
    let fit = polygons::arrow_graph(&points, &tips, &tails)?;
    Some(Fit {
        vertices: fit
            .vertices
            .into_iter()
            .map(|p| Point::new(center.x + p.x * scale, center.y + p.y * scale))
            .collect(),
        ..fit
    })
}
