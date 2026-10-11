use crate::geometry;
use folio_document::*;

#[derive(Clone, Debug)]
pub struct Scratch {
    pub bounds: Rect,
    pub confidence: f32,
    passes: Vec<ScratchPass>,
    segments: Vec<ScratchSegment>,
    regions: Vec<ScratchRegion>,
    required_crossings: usize,
    baseline: Point,
}

#[derive(Clone, Debug)]
struct ScratchPass {
    bounds: Rect,
    segments: Vec<ScratchSegment>,
}

#[derive(Clone, Debug)]
struct ScratchRegion {
    bounds: Rect,
    hull: Vec<Point>,
}

#[derive(Clone, Debug)]
struct ScratchSegment {
    a: Point,
    b: Point,
    bounds: Rect,
}

/// A geometric scrub candidate, not permission to delete. `targets` also requires
/// repeated crossings of existing ink. Rounded advancing waves are ambiguous
/// with handwriting until that context is checked. Distance hysteresis suppresses tremor.
pub fn scratch(points: &[StrokePoint]) -> Option<Scratch> {
    if points.len() < 4 {
        return None;
    }
    let positions = geometry::clean_iter(points.iter().map(|p| p.position()))?;
    let bounds = Rect::from_points(positions.iter().copied());
    let length = geometry::length(&positions);
    // A fixed sample count shaved off the peaks of long/dense scrubs. Keep
    // local detail at document scale while bounding work on imported paths.
    let count = ((length / 3.).ceil() as usize).clamp(192, 1536);
    let path = geometry::resample(&positions, count);
    let principal = geometry::principal_axis(&path);
    let travel = geometry::traversal_axis(&positions);
    // The long axis of a word and the direction of its diagonal scrub legs can
    // both hide up/down reversals. Inspect both perpendicular directions too.
    [
        principal,
        travel,
        Point::new(-principal.y, principal.x),
        Point::new(-travel.y, travel.x),
    ]
    .into_iter()
    .filter_map(|axis| scratch_on_axis(&path, bounds, length, axis))
    .max_by_key(|candidate| candidate.passes.len())
}

fn scratch_on_axis(path: &[Point], bounds: Rect, length: f32, axis: Point) -> Option<Scratch> {
    let projection: Vec<f32> = path.iter().map(|p| p.x * axis.x + p.y * axis.y).collect();
    let perpendicular: Vec<f32> = path.iter().map(|p| -p.x * axis.y + p.y * axis.x).collect();
    let span = projection.iter().copied().fold(f32::NEG_INFINITY, f32::max)
        - projection.iter().copied().fold(f32::INFINITY, f32::min);
    if span < 14. || length < span * 2.3 {
        return None;
    }
    let threshold = span * 0.12;
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
    let pass_ranges: Vec<_> = turns
        .windows(2)
        .map(|t| {
            let (mut a, mut b) = (t[0], t[1]);
            // Stop the area footprint at the last real reversal, before a flat
            // entry/exit tail. The full path still has direct crossing coverage.
            let epsilon = span * 0.03;
            if a == 0 {
                a = (a..=b)
                    .find(|&i| (projection[i] - projection[0]).abs() > epsilon)
                    .map_or(a, |i| i.saturating_sub(1));
            }
            if b == path.len() - 1 {
                b = (a..b)
                    .rev()
                    .find(|&i| (projection[i] - projection[b]).abs() > epsilon)
                    .map_or(b, |i| i + 1);
            }
            (a, b)
        })
        .filter(|&(a, b)| {
            let excursion = (projection[b] - projection[a]).abs();
            excursion >= span * 0.28 && excursion >= geometry::length(&path[a..=b]) * 0.25
        })
        .collect();
    let passes = pass_ranges
        .iter()
        .map(|&(a, b)| &path[a..=b])
        .collect::<Vec<_>>();
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
    // A broad scrub can advance diagonally and turn smoothly. Counting sharp
    // corners or demanding almost entirely axial travel rejected real scrubs.
    // Six substantial legs distinguish it from a single W/M; existing ink is
    // required separately, rather than guessing intention from corners alone.
    let axial_travel = pass_ranges
        .iter()
        .map(|&(a, b)| {
            projection[a..=b]
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).abs())
                .sum::<f32>()
        })
        .sum::<f32>();
    let pass_length = passes
        .iter()
        .map(|pass| geometry::length(pass))
        .sum::<f32>();
    let advancing_scrub = passes.len() >= 6
        && axial_travel >= pass_length * 0.45
        && centers
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).abs() <= span * 1.5);
    let revisiting = spread <= span * 0.45 || passes.len() >= 4 && revisits >= 2;
    if passes.len() < 3 || !revisiting && !advancing_scrub {
        return None;
    }
    // Fill only pairs of adjacent passes, not the global rectangle or hull.
    // Thus the gaps between legs are swept, while nearby lines and the empty
    // corners of a large scribble's footprint remain outside the erase area.
    let first = pass_ranges.first()?.0;
    let last = pass_ranges.last()?.1;
    let core_turns = std::iter::once(first)
        .chain(turns.iter().copied().filter(|&i| i > first && i < last))
        .chain(std::iter::once(last))
        .collect::<Vec<_>>();
    let regions = core_turns
        .windows(3)
        .filter(|triple| triple[0] >= first && triple[2] <= last)
        .filter_map(|triple| {
            let hull = convex_hull(&path[triple[0]..=triple[2]]);
            (hull.len() >= 3).then(|| ScratchRegion {
                bounds: Rect::from_points(hull.iter().copied()).expand(2.),
                hull,
            })
        })
        .collect();
    Some(Scratch {
        bounds,
        baseline: geometry::principal_axis(path),
        confidence: 0.85 + passes.len().min(7) as f32 * 0.02,
        required_crossings: if revisiting { 1 } else { 3 },
        regions,
        // Recognition uses substantial passes, but short legs and exit tails
        // still erase what they actually cross once intent has been validated.
        segments: path
            .windows(2)
            .map(|pair| ScratchSegment {
                a: pair[0],
                b: pair[1],
                bounds: Rect::from_points([pair[0], pair[1]]).expand(2.),
            })
            .collect(),
        passes: passes
            .iter()
            .map(|pass| ScratchPass {
                bounds: Rect::from_points(pass.iter().copied()).expand(2.),
                segments: pass
                    .windows(2)
                    .map(|pair| ScratchSegment {
                        a: pair[0],
                        b: pair[1],
                        bounds: Rect::from_points([pair[0], pair[1]]).expand(2.),
                    })
                    .collect(),
            })
            .collect(),
    })
}

impl Scratch {
    /// Validate destructive intent against existing ink before using the swept
    /// area. An advancing wave in empty space or touching ink once remains
    /// drawing; compact retracing already supplies stronger geometric intent.
    /// One cursive stroke can supply all crossings; object count is irrelevant.
    pub fn targets(&self, objects: &[&Object]) -> Vec<Id> {
        let paths = objects
            .iter()
            .filter_map(|object| ink_path(object).map(|path| (*object, path)))
            .collect::<Vec<_>>();
        let crossings = self
            .passes
            .iter()
            .filter(|pass| {
                paths.iter().any(|(object, path)| {
                    !matches!(object, Object::Stroke(s) if s.style.tool == InkTool::Highlighter)
                        && pass.bounds.intersects(object.bounds())
                        && crosses(path, &pass.segments)
                })
            })
            .take(self.required_crossings)
            .count();
        if crossings < self.required_crossings {
            return Vec::new();
        }
        paths
            .into_iter()
            .filter(|(_, path)| self.erases_path(path))
            .map(|(object, _)| object.id())
            .collect()
    }

    /// Geometry-only coverage check. Application deletion must use `targets`
    /// first: accepting a repetitive candidate alone cannot establish intent.
    pub fn erases(&self, object: &Object) -> bool {
        self.bounds.expand(3.).intersects(object.bounds())
            && ink_path(object).is_some_and(|path| self.erases_path(&path))
    }

    fn erases_path(&self, path: &[(Point, f32)]) -> bool {
        if crosses(path, &self.segments) {
            return true;
        }
        path_segments(path).any(|(a, b, radius)| {
            let bounds = Rect::from_points([a, b]).expand(radius);
            self.regions.iter().any(|region| {
                region.bounds.intersects(bounds)
                    && (inside(a, &region.hull)
                        || inside(b, &region.hull)
                        || edges(&region.hull)
                            .any(|(c, d)| geometry::segment_distance(a, b, c, d) <= radius + 2.))
            })
        })
    }
    /// Attach small disconnected marks of a crossed letter (i/j dots and t bars).
    /// Restrict both size and horizontal proximity to substantive crossed ink;
    /// never recursively spread through an entire line or neighboring word.
    pub fn completes_letter(&self, mark: &Object, crossed: &[&Object]) -> bool {
        if !matches!(mark, Object::Stroke(s) if s.style.tool != InkTool::Highlighter) {
            return false;
        }
        let Some(r) = ink_bounds_on_axis(mark, self.baseline) else {
            return false;
        };
        crossed.iter().any(|body| {
            let Some(body) = ink_bounds_on_axis(body, self.baseline) else {
                return false;
            };
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

    /// Match the discovery margin to the same oriented ink geometry used for
    /// attachment, including sideways text whose screen-aligned height is tiny.
    pub fn letter_search_bounds(&self, body: &Object) -> Rect {
        let height = ink_bounds_on_axis(body, self.baseline).map_or(0., |r| r.height());
        body.bounds().expand(height * 0.45)
    }
}

// Test actual transformed ink, never an object's rectangle alone.
fn ink_path(object: &Object) -> Option<Vec<(Point, f32)>> {
    match object {
        Object::Stroke(s) => Some(
            s.display_path()
                .iter()
                .map(|p| {
                    (
                        s.transform.apply(p.position),
                        p.radius * s.transform.scale(),
                    )
                })
                .collect(),
        ),
        Object::Shape(s) if !s.source_strokes.is_empty() => Some(
            s.vertices
                .iter()
                .map(|&p| {
                    (
                        s.transform.apply(p),
                        s.style.width * 0.5 * s.transform.scale(),
                    )
                })
                .collect(),
        ),
        _ => None,
    }
}

fn path_segments(path: &[(Point, f32)]) -> impl Iterator<Item = (Point, Point, f32)> + '_ {
    // A one-point dot has a degenerate segment so it participates in both tests.
    (0..if path.len() == 1 {
        1
    } else {
        path.len().saturating_sub(1)
    })
        .map(|i| {
            let a = path[i];
            let b = path[(i + 1).min(path.len() - 1)];
            (a.0, b.0, a.1.max(b.1))
        })
}

fn crosses(path: &[(Point, f32)], segments: &[ScratchSegment]) -> bool {
    path_segments(path).any(|(a, b, radius)| {
        let bounds = Rect::from_points([a, b]).expand(radius);
        segments.iter().any(|segment| {
            segment.bounds.intersects(bounds)
                && geometry::segment_distance(a, b, segment.a, segment.b) <= radius + 2.
        })
    })
}

fn cross(a: Point, b: Point, c: Point) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    sorted.dedup();
    let mut hull = Vec::new();
    for &p in &sorted {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0. {
            hull.pop();
        }
        hull.push(p);
    }
    let lower = hull.len();
    for &p in sorted.iter().rev().skip(1) {
        while hull.len() > lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0. {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull
}
fn edges(hull: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    hull.iter()
        .copied()
        .zip(hull.iter().copied().cycle().skip(1))
}
fn inside(p: Point, hull: &[Point]) -> bool {
    edges(hull).all(|(a, b)| cross(a, b, p) >= -0.001)
}

fn ink_bounds_on_axis(object: &Object, axis: Point) -> Option<Rect> {
    let path = ink_path(object)?;
    path.into_iter()
        .map(|(p, radius)| {
            let p = geometry::project(p, axis);
            Rect::new(p.x - radius, p.y - radius, radius * 2., radius * 2.)
        })
        .reduce(Rect::union)
}
