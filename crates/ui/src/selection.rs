use folio_document::{Point, Rect};
/// Keep the action row close to the selected bounds and inside the canvas.
pub(super) fn toolbar_origin(selection: Rect, canvas: (f32, f32), toolbar: (f32, f32)) -> Point {
    let margin = 12.;
    let width = toolbar.0.min((canvas.0 - margin * 2.).max(0.));
    let height = toolbar.1.min((canvas.1 - margin * 2.).max(0.));
    let above = selection.min.y - height - margin;
    let y = if above >= margin {
        above
    } else {
        selection.max.y + margin
    };
    Point::new(
        selection
            .min
            .x
            .clamp(margin, (canvas.0 - width - margin).max(margin)),
        y.clamp(margin, (canvas.1 - height - margin).max(margin)),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_follow_selection_without_leaving_small_or_edge_viewports() {
        assert_eq!(
            toolbar_origin(Rect::new(300., 300., 100., 80.), (1000., 700.), (500., 60.)),
            Point::new(300., 228.)
        );
        assert_eq!(
            toolbar_origin(Rect::new(900., 0., 100., 20.), (1000., 700.), (500., 60.)),
            Point::new(488., 32.)
        );
        assert_eq!(
            toolbar_origin(Rect::new(-300., 1000., 50., 20.), (200., 100.), (500., 60.)),
            Point::new(12., 28.)
        );
    }
}
