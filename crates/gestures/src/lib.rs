//! Conservative smart gestures. Candidates must overlap real objects before
//! the application may act; ordinary letters cannot erase an empty region.
use folio_document::*;
mod encircle;
mod geometry;
mod scratch;
pub use encircle::{closed_loop, encloses, selection_loop};
pub use scratch::{Scratch, scratch};
#[cfg(test)]
mod tests;

/// Only a deliberate, nearly horizontal line spanning a whole semantic word.
/// Callers require an existing word and explicit opt-in before deleting anything.
pub fn strike(points: &[StrokePoint], word: Rect) -> bool {
    if points.len() < 8 || word.width() < 24. {
        return false;
    }
    let elapsed = points
        .last()
        .unwrap()
        .timestamp
        .saturating_sub(points[0].timestamp);
    if !(180..=1500).contains(&elapsed) {
        return false;
    }
    let bounds = Rect::from_points(points.iter().map(|p| p.position()));
    let y = word.center().y;
    bounds.width() > word.width() * 1.15
        && bounds.min.x < word.min.x
        && bounds.max.x > word.max.x
        && bounds.height() < word.height() * 0.2 + 1.
        && (bounds.center().y - y).abs() < word.height() * 0.2
        && points[0]
            .position()
            .distance(points.last().unwrap().position())
            > bounds.width() * 0.95
}

/// A caret in empty space between lines. Returns the insertion coordinate and
/// amount; normal handwritten carets overlapping ink are left untouched.
pub fn insertion(points: &[StrokePoint], lines: &[Rect]) -> Option<(f32, f32)> {
    if points.len() < 10 {
        return None;
    }
    let elapsed = points.last()?.timestamp.saturating_sub(points[0].timestamp);
    if !(180..=1800).contains(&elapsed) {
        return None;
    }
    let bounds = Rect::from_points(points.iter().map(|p| p.position()));
    if !(20.0..=90.0).contains(&bounds.width()) || !(15.0..=100.0).contains(&bounds.height()) {
        return None;
    }
    let first = points[0].position();
    let last = points.last()?.position();
    if (first.y - last.y).abs() > bounds.height() * 0.15
        || first.distance(last) < bounds.width() * 0.9
    {
        return None;
    }
    let apex = points.iter().min_by(|a, b| a.y.total_cmp(&b.y))?.position();
    if (apex.x - bounds.center().x).abs() > bounds.width() * 0.15
        || apex.y > first.y - bounds.height() * 0.85
    {
        return None;
    }
    if points.iter().any(|p| {
        folio_ink::segment_distance(p.position(), first, apex).min(folio_ink::segment_distance(
            p.position(),
            apex,
            last,
        )) > 3.
    }) {
        return None;
    }
    let above = lines
        .iter()
        .filter(|r| r.max.y < bounds.min.y)
        .max_by(|a, b| a.max.y.total_cmp(&b.max.y))?;
    let below = lines
        .iter()
        .filter(|r| r.min.y > bounds.max.y)
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))?;
    if bounds.min.y - above.max.y > 100. || below.min.y - bounds.max.y > 100. {
        return None;
    }
    Some((below.min.y, bounds.height().max(40.)))
}

#[cfg(test)]
mod deliberate_tests {
    use super::*;
    #[test]
    fn strikes_require_full_word_and_deliberate_timing() {
        let word = Rect::new(30., 30., 70., 30.);
        let mut points = (0..20)
            .map(|i| StrokePoint::new(Point::new(20. + i as f32 * 5., 45.), 0.6, i * 20))
            .collect::<Vec<_>>();
        assert!(strike(&points, word));
        assert!(!strike(&points, Rect::new(20., 30., 110., 30.)));
        points.iter_mut().for_each(|p| p.timestamp /= 10);
        assert!(!strike(&points, word));
    }
    #[test]
    fn insertion_caret_requires_empty_space_between_two_lines() {
        let points = (0..21)
            .map(|i| {
                let x = 20. + i as f32 * 2.;
                let y = 90. - (10. - (i as f32 - 10.).abs()) * 2.;
                StrokePoint::new(Point::new(x, y), 0.7, i * 20)
            })
            .collect::<Vec<_>>();
        let lines = [
            Rect::new(0., 30., 200., 20.),
            Rect::new(0., 110., 200., 20.),
        ];
        assert_eq!(insertion(&points, &lines), Some((110., 40.)));
        assert!(insertion(&points, &[lines[0]]).is_none());
        assert!(insertion(&points, &[Rect::new(0., 30., 200., 50.), lines[1]]).is_none());
    }
}
