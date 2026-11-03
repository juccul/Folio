use super::*;
use folio_document::{Object, Rect};
use std::sync::Arc;
#[derive(Clone)]
pub(super) enum Kind {
    Image(Arc<Object>),
    Pdf(folio_document::PdfBackground),
}
pub(super) struct Selection {
    note: Id,
    page: Id,
    kind: Kind,
    start: Option<DocPoint>,
    end: Option<DocPoint>,
    /// The adjustable crop box of an image, applied only when confirmed.
    crop: Option<CropBox>,
}
/// The part of a crop box that a press grabs.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    /// The edges that follow the pointer: left, top, right and bottom.
    Resize([bool; 4]),
    Move,
    /// A new box drawn from this corner.
    New(DocPoint),
}
/// A crop box in the image's own coordinates, inside its current frame, so
/// rotated and scaled images crop along their own edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CropBox {
    frame: Rect,
    kept: Rect,
    drag: Option<(Grip, DocPoint, Rect)>,
}
impl CropBox {
    fn new(frame: Rect) -> Self {
        Self {
            frame,
            kept: frame,
            drag: None,
        }
    }
    /// The smallest box kept, 1% of the frame on each axis.
    fn minimum(&self) -> (f32, f32) {
        (self.frame.width() * 0.01, self.frame.height() * 0.01)
    }
    fn clamp(&self, p: DocPoint) -> DocPoint {
        DocPoint::new(
            p.x.clamp(self.frame.min.x, self.frame.max.x),
            p.y.clamp(self.frame.min.y, self.frame.max.y),
        )
    }
    /// What a press at `p` grabs: an edge or corner within `reach`, the box
    /// itself once it has been adjusted, or otherwise the start of a new box.
    fn grip(&self, p: DocPoint, reach: f32) -> Grip {
        let k = self.kept;
        let across_x = p.x >= k.min.x - reach && p.x <= k.max.x + reach;
        let across_y = p.y >= k.min.y - reach && p.y <= k.max.y + reach;
        let near = |a: f32, b: f32| (a - b).abs() <= reach;
        // In a box narrower than the reach, the closer edge wins.
        let pick = |low: bool, high: bool, value: f32, min: f32, max: f32| match (low, high) {
            (true, true) if (value - min).abs() <= (value - max).abs() => (true, false),
            (true, true) => (false, true),
            other => other,
        };
        let (left, right) = pick(
            across_y && near(p.x, k.min.x),
            across_y && near(p.x, k.max.x),
            p.x,
            k.min.x,
            k.max.x,
        );
        let (top, bottom) = pick(
            across_x && near(p.y, k.min.y),
            across_x && near(p.y, k.max.y),
            p.y,
            k.min.y,
            k.max.y,
        );
        if left || top || right || bottom {
            Grip::Resize([left, top, right, bottom])
        } else if k.contains(p) && k != self.frame {
            Grip::Move
        } else {
            Grip::New(self.clamp(p))
        }
    }
    fn press(&mut self, p: DocPoint, reach: f32) {
        self.drag = Some((self.grip(p, reach), p, self.kept));
    }
    fn drag_to(&mut self, p: DocPoint) {
        let Some((grip, start, original)) = self.drag else {
            return;
        };
        let (min_w, min_h) = self.minimum();
        let f = self.frame;
        self.kept = match grip {
            Grip::Move => {
                let dx = (p.x - start.x).clamp(f.min.x - original.min.x, f.max.x - original.max.x);
                let dy = (p.y - start.y).clamp(f.min.y - original.min.y, f.max.y - original.max.y);
                Rect::new(
                    original.min.x + dx,
                    original.min.y + dy,
                    original.width(),
                    original.height(),
                )
            }
            Grip::Resize([left, top, right, bottom]) => {
                let p = self.clamp(p);
                let (mut min, mut max) = (original.min, original.max);
                if left {
                    min.x = p.x.min(max.x - min_w);
                }
                if right {
                    max.x = p.x.max(min.x + min_w);
                }
                if top {
                    min.y = p.y.min(max.y - min_h);
                }
                if bottom {
                    max.y = p.y.max(min.y + min_h);
                }
                Rect::from_points([min, max])
            }
            Grip::New(anchor) => Rect::from_points([anchor, self.clamp(p)]),
        };
    }
    /// End a drag. A tap or sliver instead of a new box keeps the old box.
    fn release(&mut self) {
        if let Some((Grip::New(_), _, original)) = self.drag.take() {
            let (min_w, min_h) = self.minimum();
            if self.kept.width() < min_w || self.kept.height() < min_h {
                self.kept = original;
            }
        }
    }
    /// The kept box as a crop of the whole image, in 0–1 fractions.
    fn crop(&self, existing: Option<Rect>) -> Result<Rect, String> {
        normalized_crop(
            self.kept,
            self.frame,
            folio_document::Transform::default(),
            existing,
        )
    }
}
impl NotesView {
    pub(super) fn cancel_region_after_navigation(&mut self) {
        if self.region_selection.as_ref().is_some_and(|region| {
            region.note != self.controller.active || region.page != self.controller.page().id
        }) {
            self.region_selection = None;
            self.controller.status = "Region selection cancelled after navigation".into();
        }
    }
    /// Only used by the explicit native --smoke-test on throwaway data.
    pub fn region_smoke_setup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.controller.create_note();
        self.show_editor();
        self.controller.session_mut().viewport.pan = DocPoint::new(40., 30.);
        let asset = format!("{}.png", Id::new_v4());
        image::RgbaImage::from_pixel(400, 300, image::Rgba([70, 110, 150, 255]))
            .save(self.controller.assets.join(&asset))
            .map_err(|e| e.to_string())?;
        let id = Id::new_v4();
        self.controller.commit(
            "Crop test image",
            vec![folio_document::Change::Object {
                page: self.controller.page().id,
                id,
                index: 0,
                before: None,
                after: Some(Arc::new(Object::Image(folio_document::ImageObject {
                    id,
                    asset,
                    rect: Rect::new(50., 50., 400., 300.),
                    transform: folio_document::Transform::default(),
                    crop: None,
                }))),
            }],
        );
        self.controller.session_mut().selection = std::collections::HashSet::from([id]);
        self.start_image_crop(window, cx);
        Ok(())
    }
    pub fn region_smoke_events(&self) -> Result<Vec<PlatformInput>, String> {
        let bounds = self.canvas_bounds.ok_or("Canvas not laid out")?;
        let position = |p: DocPoint| {
            let p = self.controller.session().viewport.to_screen(p);
            point(bounds.origin.x + px(p.x), bounds.origin.y + px(p.y))
        };
        let start = position(DocPoint::new(150., 150.));
        let end = position(DocPoint::new(350., 300.));
        Ok(vec![
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }),
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }),
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers: Default::default(),
                click_count: 1,
            }),
        ])
    }
    pub fn region_smoke_outline(&self) -> Result<(), String> {
        if !self
            .region_selection
            .as_ref()
            .and_then(|r| r.crop)
            .is_some_and(|crop| crop.kept.width() > 10. && crop.kept != crop.frame)
        {
            return Err("Mouse crop drag did not update the live crop box".into());
        }
        Ok(())
    }
    pub fn region_smoke_verify(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        // The dragged box waits for confirmation before cropping.
        let pending = self.region_selection.as_ref().and_then(|r| r.crop);
        if pending.is_none_or(|crop| crop.kept != Rect::new(150., 150., 200., 150.)) {
            return Err(format!("Unexpected pending crop box: {pending:?}"));
        }
        self.apply_crop(cx);
        if self.region_selection.is_some() || self.controller.page().objects.len() != 1 {
            return Err("Crop left active region or drew extra ink".into());
        }
        let crop = self
            .controller
            .page()
            .ordered_objects()
            .find_map(|o| match o.as_ref() {
                Object::Image(image) => image.crop,
                _ => None,
            })
            .ok_or("Crop was not applied")?;
        let expected = Rect::new(0.25, 1. / 3., 0.5, 0.5);
        if crop.min.distance(expected.min) > 0.001 || crop.max.distance(expected.max) > 0.001 {
            return Err(format!("Unexpected mouse crop: {crop:?}"));
        }
        // The kept part stays where it was instead of stretching to the frame.
        let kept = self
            .controller
            .page()
            .ordered_objects()
            .next()
            .map(|o| o.bounds());
        if kept != Some(Rect::new(150., 150., 200., 150.)) {
            return Err(format!("Cropped image moved or rescaled: {kept:?}"));
        }
        self.controller.undo();
        if !matches!(self.controller.page().ordered_objects().next().map(|o|o.as_ref()),Some(Object::Image(image)) if image.crop.is_none())
        {
            return Err("Crop undo did not restore the image".into());
        }
        self.controller.redo();
        self.controller.flush()
    }
    fn start_region(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.controller.cancel();
        self.dismiss_popovers();
        self.region_selection = Some(Selection {
            note: self.controller.active,
            page: self.controller.page().id,
            crop: match &kind {
                Kind::Image(object) => match object.as_ref() {
                    Object::Image(image) => Some(CropBox::new(image.rect)),
                    _ => None,
                },
                Kind::Pdf(_) => None,
            },
            kind,
            start: None,
            end: None,
        });
        self.controller.status = if self.cropping() {
            "Drag the handles or draw a new box · Enter applies · Escape cancels"
        } else {
            "Drag a rectangle on the page · Escape cancels"
        }
        .into();
        self.focus.focus(window);
        cx.notify();
    }
    pub(super) fn start_image_crop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = &self.controller.session().selection;
        if selected.len() != 1 {
            self.controller.error = Some("Select one image to crop".into());
            return;
        }
        let object = selected
            .iter()
            .next()
            .and_then(|id| self.controller.page().objects.get(id))
            .cloned();
        if let Some(object) = object.filter(|o| matches!(o.as_ref(), Object::Image(_))) {
            self.start_region(Kind::Image(object), window, cx);
        }
    }
    /// An image crop is being adjusted.
    pub(super) fn cropping(&self) -> bool {
        self.region_selection
            .as_ref()
            .is_some_and(|region| region.crop.is_some())
    }
    /// Apply the adjusted crop box and leave crop mode.
    pub(super) fn apply_crop(&mut self, cx: &mut Context<Self>) {
        let Some(region) = self.region_selection.take() else {
            return;
        };
        let (Some(crop), Kind::Image(expected)) = (region.crop, &region.kind) else {
            self.region_selection = Some(region);
            return;
        };
        let id = expected.id();
        let current = self.controller.page().objects.get(&id);
        let result = match expected.as_ref() {
            _ if region.note != self.controller.active
                || region.page != self.controller.page().id
                || !current.is_some_and(|current| Arc::ptr_eq(current, expected)) =>
            {
                Err("Image changed; crop it again".to_string())
            }
            Object::Image(image) if crop.kept == crop.frame => Ok(image.crop),
            Object::Image(image) => crop.crop(image.crop).map(Some),
            _ => Err("Select an image".into()),
        };
        match result {
            Ok(Some(crop)) => {
                self.controller.session_mut().selection = std::collections::HashSet::from([id]);
                self.controller.crop_selection(Some(crop));
                self.controller.status = "Image cropped".into();
            }
            // An unchanged box leaves the image as it was.
            Ok(None) => self.controller.status = "Crop unchanged".into(),
            Err(message) => self.controller.error = Some(message),
        }
        cx.notify();
    }
    pub(super) fn start_pdf_region(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pdf) = self.controller.page().properties.pdf.clone() {
            self.start_region(Kind::Pdf(pdf), window, cx);
        } else {
            self.controller.error = Some("Open a PDF page before reading a region".into());
        }
    }
    pub(super) fn region_input(
        &mut self,
        phase: Phase,
        screen: DocPoint,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut region) = self.region_selection.take() else {
            return false;
        };
        if region.note != self.controller.active || region.page != self.controller.page().id {
            self.controller.status = "Region selection cancelled after navigation".into();
            cx.notify();
            return true;
        }
        let position = self.controller.session().viewport.to_document(screen);
        if let (Some(crop), Kind::Image(object)) = (&mut region.crop, &region.kind)
            && let Object::Image(image) = object.as_ref()
        {
            let zoom = self.controller.session().viewport.zoom;
            let t = image.transform;
            let scale = (t.a * t.d - t.b * t.c).abs().sqrt().max(f32::EPSILON);
            let local = t
                .inverse()
                .map_or(position, |inverse| inverse.apply(position));
            match phase {
                Phase::Down => crop.press(local, 12. / (zoom * scale)),
                Phase::Move => crop.drag_to(local),
                Phase::Up | Phase::Cancel => crop.release(),
                _ => {}
            }
            self.region_selection = Some(region);
            cx.notify();
            return true;
        }
        match phase {
            Phase::Down => {
                region.start = Some(position);
                region.end = Some(position);
            }
            Phase::Move if region.start.is_some() => {
                region.end = Some(position);
            }
            Phase::Up if region.start.is_some() => {
                let rect = Rect::from_points([region.start.unwrap(), position]);
                let result = match &region.kind {
                    Kind::Image(expected) => {
                        let id = expected.id();
                        if !self
                            .controller
                            .page()
                            .objects
                            .get(&id)
                            .is_some_and(|current| Arc::ptr_eq(current, expected))
                        {
                            Err("Image changed; select a crop again".into())
                        } else if let Object::Image(image) = expected.as_ref() {
                            normalized_crop(rect, image.rect, image.transform, image.crop).map(
                                |crop| {
                                    self.controller.session_mut().selection =
                                        std::collections::HashSet::from([id]);
                                    self.controller.crop_selection(Some(crop));
                                },
                            )
                        } else {
                            Err("Select an image".into())
                        }
                    }
                    Kind::Pdf(expected) => {
                        if self.controller.page().properties.pdf.as_ref() != Some(expected) {
                            Err("PDF page changed; select a region again".into())
                        } else {
                            let page = &self.controller.page().properties;
                            normalized_crop(
                                rect,
                                Rect::new(0., 0., page.width, page.height),
                                folio_document::Transform::default(),
                                None,
                            )
                            .and_then(|rect| self.controller.read_pdf_math(rect))
                        }
                    }
                };
                if let Err(message) = result {
                    self.controller.error = Some(message);
                }
                cx.notify();
                return true;
            }
            Phase::Cancel | Phase::Leave => {
                self.controller.status = "Region selection cancelled".into();
                cx.notify();
                return true;
            }
            _ => {}
        }
        self.region_selection = Some(region);
        cx.notify();
        true
    }
    pub(super) fn paint_region(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let Some(region) = &self.region_selection else {
            return;
        };
        if let (Some(crop), Kind::Image(object)) = (&region.crop, &region.kind)
            && let Object::Image(image) = object.as_ref()
        {
            self.paint_crop(crop, image.transform, bounds, window);
            return;
        }
        let (Some(start), Some(end)) = (region.start, region.end) else {
            return;
        };
        let rect = Rect::from_points([start, end]);
        let viewport = self.controller.session().viewport;
        let points = [
            rect.min,
            DocPoint::new(rect.max.x, rect.min.y),
            rect.max,
            DocPoint::new(rect.min.x, rect.max.y),
        ];
        let mut path = PathBuilder::stroke(px(1.5));
        for (index, p) in points
            .into_iter()
            .chain(std::iter::once(rect.min))
            .enumerate()
        {
            let p = viewport.to_screen(p);
            let p = point(bounds.origin.x + px(p.x), bounds.origin.y + px(p.y));
            if index == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
        if let Ok(path) = path.build() {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                window.paint_path(path, rgb(Theme::new(&self.controller.settings).accent))
            });
        }
    }
}
impl NotesView {
    /// Shade the part of the image a crop discards, and outline the kept box
    /// with handles on its corners and edges.
    fn paint_crop(
        &self,
        crop: &CropBox,
        transform: folio_document::Transform,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        let viewport = self.controller.session().viewport;
        let screen = |p: DocPoint| {
            let p = viewport.to_screen(transform.apply(p));
            point(bounds.origin.x + px(p.x), bounds.origin.y + px(p.y))
        };
        let corners = |r: Rect| {
            [
                r.min,
                DocPoint::new(r.max.x, r.min.y),
                r.max,
                DocPoint::new(r.min.x, r.max.y),
            ]
        };
        let polygon = |points: &[DocPoint], mut path: PathBuilder| {
            for (index, p) in points.iter().enumerate() {
                if index == 0 {
                    path.move_to(screen(*p));
                } else {
                    path.line_to(screen(*p));
                }
            }
            path.close();
            path.build().ok()
        };
        let (f, k) = (crop.frame, crop.kept);
        // Four bands around the kept box cover the discarded area.
        let shade = [
            Rect::from_points([f.min, DocPoint::new(f.max.x, k.min.y)]),
            Rect::from_points([DocPoint::new(f.min.x, k.max.y), f.max]),
            Rect::from_points([
                DocPoint::new(f.min.x, k.min.y),
                DocPoint::new(k.min.x, k.max.y),
            ]),
            Rect::from_points([
                DocPoint::new(k.max.x, k.min.y),
                DocPoint::new(f.max.x, k.max.y),
            ]),
        ];
        let accent = rgb(Theme::new(&self.controller.settings).accent);
        let zoom = viewport.zoom;
        let scale = (transform.a * transform.d - transform.b * transform.c)
            .abs()
            .sqrt()
            .max(f32::EPSILON);
        let handle = 4. / (zoom * scale);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for band in shade {
                if band.width() > 0.
                    && band.height() > 0.
                    && let Some(path) = polygon(&corners(band), PathBuilder::fill())
                {
                    window.paint_path(path, rgba(0x0000_0080));
                }
            }
            if let Some(path) = polygon(&corners(k), PathBuilder::stroke(px(1.5))) {
                window.paint_path(path, accent);
            }
            let (mid_x, mid_y) = (k.center().x, k.center().y);
            for p in [
                k.min,
                DocPoint::new(mid_x, k.min.y),
                DocPoint::new(k.max.x, k.min.y),
                DocPoint::new(k.max.x, mid_y),
                k.max,
                DocPoint::new(mid_x, k.max.y),
                DocPoint::new(k.min.x, k.max.y),
                DocPoint::new(k.min.x, mid_y),
            ] {
                let square = corners(Rect::new(
                    p.x - handle,
                    p.y - handle,
                    2. * handle,
                    2. * handle,
                ));
                if let Some(path) = polygon(&square, PathBuilder::fill()) {
                    window.paint_path(path, rgb(0xffffff));
                }
                if let Some(path) = polygon(&square, PathBuilder::stroke(px(1.5))) {
                    window.paint_path(path, accent);
                }
            }
        });
    }
}
fn normalized_crop(
    selection: Rect,
    image: Rect,
    transform: folio_document::Transform,
    existing: Option<Rect>,
) -> Result<Rect, String> {
    let inverse = transform
        .inverse()
        .ok_or("Image transform cannot be inverted")?;
    let points = [
        selection.min,
        DocPoint::new(selection.max.x, selection.min.y),
        selection.max,
        DocPoint::new(selection.min.x, selection.max.y),
    ];
    let local = Rect::from_points(points.map(|p| inverse.apply(p)));
    let left = ((local.min.x - image.min.x) / image.width()).clamp(0., 1.);
    let top = ((local.min.y - image.min.y) / image.height()).clamp(0., 1.);
    let right = ((local.max.x - image.min.x) / image.width()).clamp(0., 1.);
    let bottom = ((local.max.y - image.min.y) / image.height()).clamp(0., 1.);
    if ![left, top, right, bottom].iter().all(|p| p.is_finite())
        || right - left < 0.005
        || bottom - top < 0.005
    {
        return Err("Drag a larger rectangle inside the image or page".into());
    }
    let original = existing.unwrap_or(Rect::new(0., 0., 1., 1.));
    Ok(Rect::new(
        original.min.x + left * original.width(),
        original.min.y + top * original.height(),
        (right - left) * original.width(),
        (bottom - top) * original.height(),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn crop_composes_previous_crop_and_handles_image_transform() {
        let crop = normalized_crop(
            Rect::new(30., 40., 50., 100.),
            Rect::new(0., 0., 100., 200.),
            folio_document::Transform::translate(30., 40.),
            Some(Rect::new(0.2, 0.1, 0.6, 0.8)),
        )
        .unwrap();
        assert_eq!(crop, Rect::new(0.2, 0.1, 0.3, 0.4));
        assert!(
            normalized_crop(
                Rect::new(200., 200., 20., 20.),
                Rect::new(0., 0., 100., 100.),
                folio_document::Transform::default(),
                None
            )
            .is_err()
        );
    }
    #[test]
    fn crop_box_resizes_moves_and_redraws_within_the_image() {
        let mut crop = CropBox::new(Rect::new(0., 0., 400., 300.));
        // Inside an untouched box, a drag draws a new box; taps keep the old.
        crop.press(DocPoint::new(100., 100.), 5.);
        crop.drag_to(DocPoint::new(100.5, 100.5));
        crop.release();
        assert_eq!(crop.kept, crop.frame);
        crop.press(DocPoint::new(300., 250.), 5.);
        crop.drag_to(DocPoint::new(100., 50.));
        crop.release();
        assert_eq!(crop.kept, Rect::new(100., 50., 200., 200.));
        // The right edge follows the pointer, but not past the image or the
        // left edge.
        crop.press(DocPoint::new(302., 150.), 5.);
        crop.drag_to(DocPoint::new(900., 150.));
        assert_eq!(crop.kept, Rect::new(100., 50., 300., 200.));
        crop.drag_to(DocPoint::new(0., 150.));
        crop.release();
        assert_eq!(crop.kept, Rect::new(100., 50., 4., 200.));
        // A corner moves two edges.
        crop.press(DocPoint::new(104., 250.), 3.);
        assert_eq!(
            crop.drag.unwrap().0,
            Grip::Resize([false, false, true, true])
        );
        crop.drag_to(DocPoint::new(200., 150.));
        crop.release();
        assert_eq!(crop.kept, Rect::new(100., 50., 100., 100.));
        // Dragging inside moves the box, stopping at the image edge.
        crop.press(DocPoint::new(150., 100.), 5.);
        crop.drag_to(DocPoint::new(-500., 110.));
        crop.release();
        assert_eq!(crop.kept, Rect::new(0., 60., 100., 100.));
        // The crop refers to the whole image, including an earlier crop.
        assert_eq!(
            crop.crop(Some(Rect::new(0.5, 0., 0.5, 1.))).unwrap(),
            Rect::new(0.5, 0.2, 0.125, 1. / 3.)
        );
    }
    #[test]
    fn pdf_region_clamps_and_rejects_taps() {
        assert_eq!(
            normalized_crop(
                Rect::new(-10., -10., 60., 110.),
                Rect::new(0., 0., 100., 200.),
                folio_document::Transform::default(),
                None
            )
            .unwrap(),
            Rect::new(0., 0., 0.5, 0.5)
        );
        assert!(
            normalized_crop(
                Rect::new(20., 20., 0., 0.),
                Rect::new(0., 0., 100., 200.),
                folio_document::Transform::default(),
                None
            )
            .is_err()
        );
    }
}
