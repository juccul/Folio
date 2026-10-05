use crate::geometry;
use folio_document::*;

#[derive(Clone, Debug)]
pub struct Scratch {
    pub bounds: Rect,
    pub confidence: f32,
    passes: Vec<Vec<Point>>,
}

/// Repeated broad passes through the same corridor, in any orientation. Turning points use distance
/// hysteresis, so tremor and uneven sampling cannot manufacture reversals.
pub fn scratch(points: &[StrokePoint]) -> Option<Scratch> {
    if points.len() < 4 || points.last()?.timestamp.saturating_sub(points[0].timestamp) > 4500 {
        return None;
    }
    let positions = geometry::clean(&points.iter().map(|p| p.position()).collect::<Vec<_>>())?;
    let bounds = Rect::from_points(positions.iter().copied());
    let path = geometry::resample(&positions, 192);
    let axis = geometry::principal_axis(&path);
    let projection: Vec<f32> = path.iter().map(|p| p.x * axis.x + p.y * axis.y).collect();
    let perpendicular: Vec<f32> = path.iter().map(|p| -p.x * axis.y + p.y * axis.x).collect();
    let span = projection.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        - projection.iter().copied().fold(f32::INFINITY, f32::min);
    if span < 14. || geometry::length(&positions) < span * 2.3 {
        return None;
    }
    let threshold = span * 0.18;
    let mut turns = vec![0];
    let mut direction = 0.;
    let mut extreme = projection[0];
    let mut extreme_index = 0;
    for (i, &value) in projection.iter().enumerate().skip(1) {
        if direction == 0. {
            if (value - projection[0]).abs() > threshold {
                direction = (value - projection[0]).signum();
                extreme = value;
                extreme_index = i;
            }
        } else if (value - extreme) * direction >= 0. {
            extreme = value;
            extreme_index = i;
        } else if (extreme - value) * direction > threshold {
            turns.push(extreme_index);
            direction = -direction;
            extreme = value;
            extreme_index = i;
        }
    }
    turns.push(path.len() - 1);
    let passes: Vec<_> = turns
        .windows(2)
        .filter(|t| (projection[t[1]] - projection[t[0]]).abs() >= span * 0.40)
        .map(|t| path[t[0]..=t[1]].to_vec())
        .collect();
    let spread = perpendicular
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max)
        - perpendicular.iter().copied().fold(f32::INFINITY, f32::min);
    // Three passes suffice in a narrow corridor. For wider scribbles require
    // repeated traversal of the same area, rather than the advancing legs of
    // an ordinary W/M. Unequal-length passes and circular scribbles are valid.
    let centers = passes
        .iter()
        .map(|pass| {
            pass.iter()
                .map(|p| -p.x * axis.y + p.y * axis.x)
                .sum::<f32>()
                / pass.len() as f32
        })
        .collect::<Vec<_>>();
    let revisits = centers
        .iter()
        .enumerate()
        .skip(2)
        .filter(|(i, center)| {
            centers[..i - 1]
                .iter()
                .any(|previous| (*center - previous).abs() <= span * 0.18)
        })
        .count();
    if passes.len() < 3 || spread > span * 0.45 && (passes.len() < 4 || revisits < 2) {
        return None;
    }
    Some(Scratch {
        bounds,
        confidence: 0.85 + passes.len().min(7) as f32 * 0.02,
        passes,
    })
}

impl Scratch {
    /// Once the gesture itself is validated, one real crossing targets a stroke.
    /// Requiring every letter to be crossed repeatedly leaves spotty deletions.
    /// Bounding boxes alone never erase neighboring ink or non-ink objects.
    pub fn erases(&self, object: &Object) -> bool {
        if !self.bounds.expand(3.).intersects(object.bounds()) {
            return false;
        }
        let ink: Vec<(Point, f32)> = match object {
            Object::Stroke(s) => s
                .display_path()
                .iter()
                .map(|p| {
                    (
                        s.transform.apply(p.position),
                        p.radius * s.transform.scale(),
                    )
                })
                .collect(),
            Object::Shape(s) if !s.source_strokes.is_empty() => s
                .vertices
                .iter()
                .map(|&p| {
                    (
                        s.transform.apply(p),
                        s.style.width * 0.5 * s.transform.scale(),
                    )
                })
                .collect(),
            _ => return false,
        };
        for pass in &self.passes {
            let intersects = pass.windows(2).any(|p| {
                if ink.len() == 1 {
                    return folio_ink::segment_distance(ink[0].0, p[0], p[1]) <= ink[0].1 + 2.;
                }
                let bounds = Rect::from_points([p[0], p[1]]).expand(2.);
                ink.windows(2).any(|s| {
                    let radius = s[0].1.max(s[1].1);
                    bounds.intersects(Rect::from_points([s[0].0, s[1].0]).expand(radius))
                        && geometry::segment_distance(p[0], p[1], s[0].0, s[1].0) <= radius + 2.
                })
            });
            if intersects {
                return true;
            }
        }
        false
    }
    /// Attach small disconnected marks of a crossed letter (i/j dots and t bars).
    /// Restrict both size and horizontal proximity to substantive crossed ink;
    /// never recursively spread through an entire line or neighboring word.
    pub fn completes_letter(&self, mark: &Object, crossed: &[Rect]) -> bool {
        if !matches!(mark, Object::Stroke(s) if s.style.tool != InkTool::Highlighter) {
            return false;
        }
        let r = mark.bounds();
        crossed.iter().any(|body| {
            let h = body.height();
            let x_gap = (r.min.x - body.max.x).max(body.min.x - r.max.x).max(0.);
            let y_gap = (r.min.y - body.max.y).max(body.min.y - r.max.y).max(0.);
            h >= 8.
                && r.height() < h * 0.4
                && r.width() < h * 0.8
                && x_gap <= (h * 0.15).clamp(3., 8.)
                && y_gap <= h * 0.45
        })
    }
}
