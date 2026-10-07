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
            .is_some_and(|r| matches!((r.start,r.end),(Some(a),Some(b)) if a.distance(b)>10.))
        {
            return Err("Mouse crop drag did not update the live outline".into());
        }
        Ok(())
    }
    pub fn region_smoke_verify(&mut self) -> Result<(), String> {
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
            kind,
            start: None,
            end: None,
        });
        self.controller.status = "Drag a rectangle on the page · Escape cancels".into();
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
