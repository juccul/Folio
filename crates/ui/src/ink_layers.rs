//! Display-only ink batches. Source strokes and vector exports stay unchanged.
use super::theme::CanvasTheme;
use folio_document::{Id, Object, Page, Rect};
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
        self.density == other.density && self.same_ink(other)
    }
    /// The same strokes and colors, possibly rendered at another density.
    pub fn same_ink(&self, other: &Self) -> bool {
        self.theme == other.theme
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
    /// Batches for the visible `objects`, which are in page order at
    /// `ranks`. Membership follows fixed blocks of the page order rather
    /// than what is visible, so panning and zooming keep every batch (and
    /// its cached image) intact; a batch includes its offscreen members.
    pub fn batches(
        &mut self,
        page: &Page,
        objects: &[Arc<Object>],
        ranks: &[usize],
        excluded: &HashSet<Id>,
        theme: CanvasTheme,
        density: f32,
    ) -> Vec<(Range<usize>, InkBatch)> {
        if !density.is_finite() || density <= 0. {
            self.bounds.clear();
            return Vec::new();
        }
        // Round up so every cached image has at least one texel per device pixel.
        let density = (density * 2.).ceil() / 2.;
        let eligible = |object: &Object| {
            matches!(object, Object::Stroke(_)) && !excluded.contains(&object.id())
        };
        let at = |rank: usize| page.order.get(rank).and_then(|id| page.objects.get(id));
        let mut used = HashSet::new();
        let mut batches = Vec::new();
        let mut start = 0;
        while start < objects.len() {
            let rank = ranks.get(start).copied().unwrap_or(usize::MAX);
            if !eligible(&objects[start])
                || at(rank).is_none_or(|o| !Arc::ptr_eq(o, &objects[start]))
            {
                start += 1;
                continue;
            }
            // The run of eligible ink around `rank` within its block.
            let block = rank / BATCH_SIZE * BATCH_SIZE;
            let block_end = (block + BATCH_SIZE).min(page.order.len());
            let mut first = rank;
            while first > block && at(first - 1).is_some_and(|o| eligible(o)) {
                first -= 1;
            }
            let mut last = rank + 1;
            while last < block_end && at(last).is_some_and(|o| eligible(o)) {
                last += 1;
            }
            let mut end = start + 1;
            while end < objects.len() && ranks.get(end).is_some_and(|r| *r < last) {
                end += 1;
            }
            let members = (first..last).filter_map(at).cloned().collect::<Vec<_>>();
            if members.len() >= MIN_BATCH {
                let mut bounds = self.bounds(&members[0]);
                for member in &members[1..] {
                    bounds = bounds.union(self.bounds(member));
                }
                used.extend(members.iter().map(|o| o.id()));
                let bounds = bounds.expand(3.);
                let width = (bounds.width() * density).ceil();
                let height = (bounds.height() * density).ceil();
                let pixels = width * height;
                if width > 0.
                    && height > 0.
                    && width <= 4096.
                    && height <= 4096.
                    && pixels <= MAX_BATCH_PIXELS as f32
                {
                    batches.push((
                        start..end,
                        InkBatch {
                            objects: members,
                            bounds,
                            theme,
                            density,
                            pixels: pixels as usize,
                        },
                    ));
                }
            }
            start = end;
        }
        self.bounds.retain(|id, _| used.contains(id));
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
    let page = test_page(objects);
    let ranks = (0..objects.len()).collect::<Vec<_>>();
    Planner::default().batches(&page, objects, &ranks, excluded, theme, density)
}
#[cfg(test)]
pub(super) fn test_page(objects: &[Arc<Object>]) -> Page {
    let mut page = Page::new();
    for object in objects {
        page.order.push(object.id());
        page.objects.insert(object.id(), object.clone());
    }
    page
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
    let pixmap = folio_export::raster_geometry_svg(&svg, batch.density).ok()?;
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
        // Batches follow fixed 32-object blocks of the page order.
        assert_eq!(ranges, vec![0..20, 21..32, 32..40, 41..64, 64..80]);
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
        // Panning that shows only part of a block keeps the whole block's
        // batch, so its cached image survives.
        let page = test_page(&objects);
        let visible = &objects[5..30];
        let ranks = (5..30).collect::<Vec<_>>();
        let partial = Planner::default().batches(&page, visible, &ranks, &excluded, theme(), 1.25);
        let full = batches(&objects, &excluded, theme(), 1.25);
        assert_eq!(partial[0].0, 0..15);
        assert!(partial[0].1.matches(&full[0].1) && partial[1].1.matches(&full[1].1));
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
