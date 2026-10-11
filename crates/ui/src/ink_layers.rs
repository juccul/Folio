//! Display-only ink batches. Source strokes and vector exports stay unchanged.
use super::theme::CanvasTheme;
use folio_document::{Id, Object, Rect};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

pub(super) const PIXEL_BUDGET: usize = 16 * 1024 * 1024;
const BATCH_SIZE: usize = 32;
const MIN_BATCH: usize = 8;
const MAX_BATCH_PIXELS: usize = 1024 * 1024;

#[derive(Clone)]
pub(super) struct InkBatch {
    pub objects: Vec<Arc<Object>>,
    pub bounds: Rect,
    pub theme: CanvasTheme,
    pub density: f32,
    pub pixels: usize,
}
impl InkBatch {
    pub fn id(&self) -> Id {
        self.objects[0].id()
    }
    pub fn matches(&self, other: &Self) -> bool {
        self.theme == other.theme
            && self.density == other.density
            && self.objects.len() == other.objects.len()
            && self
                .objects
                .iter()
                .zip(&other.objects)
                .all(|(a, b)| Arc::ptr_eq(a, b))
    }
}

#[derive(Default)]
pub(super) struct Planner {
    bounds: HashMap<Id, (Arc<Object>, Rect)>,
}
impl Planner {
    fn bounds(&mut self, object: &Arc<Object>) -> Rect {
        let cached = self
            .bounds
            .entry(object.id())
            .or_insert_with(|| (object.clone(), object.bounds()));
        if !Arc::ptr_eq(&cached.0, object) {
            *cached = (object.clone(), object.bounds());
        }
        cached.1
    }
    pub fn batches(
        &mut self,
        objects: &[Arc<Object>],
        excluded: &HashSet<Id>,
        theme: CanvasTheme,
        density: f32,
    ) -> Vec<(Range<usize>, InkBatch)> {
        let visible = objects
            .iter()
            .map(|object| object.id())
            .collect::<HashSet<_>>();
        self.bounds.retain(|id, _| visible.contains(id));
        if !density.is_finite() || density <= 0. {
            return Vec::new();
        }
        // Round up so every cached image has at least one texel per device pixel.
        let density = (density * 2.).ceil() / 2.;
        let mut batches = Vec::new();
        let mut start = 0;
        while start < objects.len() {
            if !matches!(objects[start].as_ref(), Object::Stroke(_))
                || excluded.contains(&objects[start].id())
            {
                start += 1;
                continue;
            }
            let mut end = start;
            let mut bounds = self.bounds(&objects[start]);
            while end < objects.len()
                && end - start < BATCH_SIZE
                && matches!(objects[end].as_ref(), Object::Stroke(_))
                && !excluded.contains(&objects[end].id())
            {
                bounds = bounds.union(self.bounds(&objects[end]));
                end += 1;
            }
            bounds = bounds.expand(3.);
            let width = (bounds.width() * density).ceil();
            let height = (bounds.height() * density).ceil();
            let pixels = width * height;
            if end - start >= MIN_BATCH
                && width > 0.
                && height > 0.
                && width <= 4096.
                && height <= 4096.
                && pixels <= MAX_BATCH_PIXELS as f32
            {
                batches.push((
                    start..end,
                    InkBatch {
                        objects: objects[start..end].to_vec(),
                        bounds,
                        theme,
                        density,
                        pixels: pixels as usize,
                    },
                ));
            }
            start = end;
        }
        batches
    }
}
#[cfg(test)]
pub(super) fn batches(
    objects: &[Arc<Object>],
    excluded: &HashSet<Id>,
    theme: CanvasTheme,
    density: f32,
) -> Vec<(Range<usize>, InkBatch)> {
    Planner::default().batches(objects, excluded, theme, density)
}

pub(super) fn raster(batch: &InkBatch) -> Option<Arc<gpui::RenderImage>> {
    let mut body = String::new();
    for object in &batch.objects {
        let Object::Stroke(stroke) = object.as_ref() else {
            return None;
        };
        let mut stroke = stroke.clone();
        stroke.style.color =
            folio_document::Color::from_rgb(batch.theme.ink(stroke.style.color.rgb()));
        body.push_str(
            &folio_export::object_svg(&Object::Stroke(stroke), std::path::Path::new(".")).ok()?,
        );
    }
    let b = batch.bounds;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">{body}</svg>",
        b.width(),
        b.height(),
        b.min.x,
        b.min.y,
        b.width(),
        b.height()
    );
    let pixmap = folio_export::raster_svg(&svg, batch.density).ok()?;
    Some(super::graph::image(
        pixmap.width(),
        pixmap.height(),
        pixmap.take(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_document::*;
    fn fixture(count: usize) -> Vec<Arc<Object>> {
        (0..count)
            .map(|i| {
                let mut b = folio_ink::StrokeBuilder::new(PenStyle::default());
                for j in 0..12 {
                    b.push(StrokePoint::new(
                        Point::new(
                            (i % 16) as f32 * 12. + j as f32,
                            (i / 16) as f32 * 15. + (j as f32 * 0.5).sin() * 4.,
                        ),
                        0.7,
                        j * 8,
                    ));
                }
                Arc::new(Object::Stroke(b.finish().unwrap()))
            })
            .collect()
    }
    fn theme() -> CanvasTheme {
        CanvasTheme {
            paper: 0xffffff,
            foreground: 0x111111,
            grid: 0xeeeeee,
            dots: 0xeeeeee,
            adapt_ink: false,
        }
    }
    #[test]
    fn batches_preserve_order_and_split_at_editable_or_nonink_objects() {
        let mut objects = fixture(80);
        objects[40] = Arc::new(Object::Text(TextBlock {
            id: Id::new_v4(),
            text: "x".into(),
            rect: Rect::new(0., 0., 20., 20.),
            transform: Transform::default(),
            font_family: "Sans".into(),
            font_size: 14.,
            color: Color::INK,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        }));
        let excluded = HashSet::from([objects[20].id()]);
        let planned = batches(&objects, &excluded, theme(), 1.25);
        let ranges = planned.iter().map(|(r, _)| r.clone()).collect::<Vec<_>>();
        assert_eq!(ranges, vec![0..20, 21..40, 41..73]);
        for (range, batch) in planned {
            assert!(
                batch
                    .objects
                    .iter()
                    .zip(&objects[range])
                    .all(|(a, b)| Arc::ptr_eq(a, b))
            );
            assert_eq!(batch.density, 1.5);
        }
    }
    #[test]
    fn identity_theme_density_and_geometry_invalidate_cached_batches() {
        let mut objects = fixture(32);
        let original = batches(&objects, &HashSet::new(), theme(), 1.).remove(0).1;
        assert!(original.matches(&batches(&objects, &HashSet::new(), theme(), 1.).remove(0).1));
        objects[7] = Arc::new(objects[7].as_ref().clone());
        assert!(!original.matches(&batches(&objects, &HashSet::new(), theme(), 1.).remove(0).1));
        let mut changed = original.clone();
        changed.theme.adapt_ink = true;
        assert!(!original.matches(&changed));
        changed = original.clone();
        changed.density = 2.;
        assert!(!original.matches(&changed));
        objects.reverse();
        assert!(!original.matches(&batches(&objects, &HashSet::new(), theme(), 1.).remove(0).1));
        assert!(raster(&original).is_some());
    }
    #[test]
    fn oversized_images_fall_back_to_vectors_without_reducing_resolution() {
        let objects = fixture(32);
        assert!(batches(&objects, &HashSet::new(), theme(), 100.).is_empty());
        assert!(batches(&objects, &HashSet::new(), theme(), f32::NAN).is_empty());
    }
    #[test]
    fn raster_preserves_selected_colors_opacity_and_overlap_order() {
        let make = |color| {
            let mut builder = folio_ink::StrokeBuilder::new(PenStyle::default());
            builder.push(StrokePoint::new(Point::new(10., 20.), 1., 0));
            builder.push(StrokePoint::new(Point::new(50., 20.), 1., 10));
            let mut stroke = builder.finish().unwrap();
            stroke.path = Arc::new(vec![
                PathPoint {
                    position: Point::new(10., 20.),
                    radius: 4.,
                },
                PathPoint {
                    position: Point::new(50., 20.),
                    radius: 4.,
                },
            ]);
            stroke.style.color = Color::from_rgb(color);
            stroke.style.opacity = 0.5;
            Arc::new(Object::Stroke(stroke))
        };
        let objects = vec![make(0xff0000), make(0x0000ff)];
        for reverse in [false, true] {
            let mut objects = objects.clone();
            if reverse {
                objects.reverse();
            }
            let batch = InkBatch {
                objects,
                bounds: Rect::new(0., 10., 60., 20.),
                theme: theme(),
                density: 2.,
                pixels: 4800,
            };
            let image = raster(&batch).unwrap();
            let width = image.size(0).width.0 as usize;
            let pixel =
                &image.as_bytes(0).unwrap()[(20 * width + 60) * 4..(20 * width + 60) * 4 + 4];
            let (first, last) = if reverse {
                (pixel[0], pixel[2])
            } else {
                (pixel[2], pixel[0])
            };
            assert!((84..=86).contains(&first), "{pixel:?}");
            assert!((169..=171).contains(&last), "{pixel:?}");
            assert_eq!(pixel[1], 0);
            assert!((191..=193).contains(&pixel[3]), "{pixel:?}");
        }
    }
}
