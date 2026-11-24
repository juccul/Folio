//! Display-only ink batches. Source strokes and vector exports stay unchanged.
use super::RenderImage;
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
/// Path points that make a shorter run worth an image: drawn as vectors, one
/// long scribble costs a GPU segment per point every frame.
const HEAVY_POINTS: usize = 1500;
const MAX_BATCH_PIXELS: usize = 4 * 1024 * 1024;

/// Snap `rect` onto the global `1 / density` pixel lattice so every cached
/// batch raster maps 1:1 to texels without fractional stretch or sub-pixel
/// phase drift as strokes are appended to a block.
fn snap_bounds(rect: Rect, density: f32) -> (Rect, f32, f32) {
    let min_x = (rect.min.x * density).floor();
    let min_y = (rect.min.y * density).floor();
    let max_x = (rect.max.x * density).ceil().max(min_x + 1.);
    let max_y = (rect.max.y * density).ceil().max(min_y + 1.);
    let snapped = Rect {
        min: folio_document::Point::new(min_x / density, min_y / density),
        max: folio_document::Point::new(max_x / density, max_y / density),
    };
    (snapped, max_x - min_x, max_y - min_y)
}

#[derive(Clone)]
pub(super) struct InkBatch {
    pub objects: Vec<Arc<Object>>,
    pub bounds: Rect,
    pub theme: CanvasTheme,
    pub density: f32,
    pub pixels: usize,
    /// Nonzero for one tile of ink too large for a single image: its grid
    /// cell, which keeps the tile's id stable while the view pans.
    pub tile: u64,
}
impl InkBatch {
    pub fn id(&self) -> Id {
        let id = self.objects[0].id();
        if self.tile == 0 {
            return id;
        }
        let mix = (self.tile as u128).wrapping_mul(0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c835);
        Id::from_u128(id.as_u128() ^ mix)
    }
    pub fn matches(&self, other: &Self) -> bool {
        self.density == other.density && self.bounds == other.bounds && self.same_ink(other)
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
    /// Whether this batch's image can stand in for `other` while `other` is
    /// rasterized: the same strokes, drawn at any density, over an area that
    /// covers `other`'s (bounds snapped to another density differ by up to a
    /// pixel of either).
    pub fn stands_in_for(&self, other: &Self) -> bool {
        let slack = 1. / self.density.min(other.density).max(0.01);
        let outer = self.bounds.expand(slack);
        self.same_ink(other)
            && outer.min.x <= other.bounds.min.x
            && outer.min.y <= other.bounds.min.y
            && outer.max.x >= other.bounds.max.x
            && outer.max.y >= other.bounds.max.y
    }
    /// Whether `self` is a strict prefix of `other` with the same theme and
    /// density (the common case while appending new strokes to the tail block).
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        self.theme == other.theme
            && self.density == other.density
            && self.objects.len() < other.objects.len()
            && self
                .objects
                .iter()
                .zip(&other.objects)
                .all(|(a, b)| Arc::ptr_eq(a, b))
    }
}

pub(super) fn high_zoom_clip(visible: Rect, density: f32) -> Option<Rect> {
    if !density.is_finite() || density <= 2. {
        return None;
    }
    let margin = (visible.width().max(visible.height()) * 0.35).max(64.);
    let padded = visible.expand(margin);
    let step = (256. / density).max(32.);
    Some(Rect {
        min: folio_document::Point::new(
            (padded.min.x / step).floor() * step,
            (padded.min.y / step).floor() * step,
        ),
        max: folio_document::Point::new(
            (padded.max.x / step).ceil() * step,
            (padded.max.y / step).ceil() * step,
        ),
    })
}

fn intersect_rect(a: Rect, b: Rect) -> Option<Rect> {
    let min_x = a.min.x.max(b.min.x);
    let min_y = a.min.y.max(b.min.y);
    let max_x = a.max.x.min(b.max.x);
    let max_y = a.max.y.min(b.max.y);
    (max_x > min_x && max_y > min_y).then_some(Rect {
        min: folio_document::Point::new(min_x, min_y),
        max: folio_document::Point::new(max_x, max_y),
    })
}

/// Splits ink too large for one image (a long scribble seen up close) into
/// tiles of about a megapixel on a grid fixed in document space. With a clip,
/// only tiles inside it are made, nearest its centre first, within the pixel
/// budget; without one, every tile, or none when they exceed the budget.
fn tiles(
    members: &[Arc<Object>],
    bounds: Rect,
    clip: Option<Rect>,
    theme: CanvasTheme,
    density: f32,
) -> Option<Vec<InkBatch>> {
    let region = match clip {
        Some(clip) => intersect_rect(bounds, clip)?,
        None => bounds,
    };
    // A power-of-two cell keeps the grid (and the tiles' ids) unchanged
    // across nearby densities, so old tiles stand in while zooming.
    let cell = (1024. / density).log2().round().exp2();
    if !cell.is_finite() || cell <= 0. {
        return None;
    }
    let (x0, x1) = ((region.min.x / cell).floor() as i64, (region.max.x / cell).ceil() as i64);
    let (y0, y1) = ((region.min.y / cell).floor() as i64, (region.max.y / cell).ceil() as i64);
    if (x1 - x0).saturating_mul(y1 - y0) > 4096 {
        return None;
    }
    let centre = region.center();
    let mut cells = Vec::new();
    for ix in x0..x1 {
        for iy in y0..y1 {
            let rect = Rect {
                min: folio_document::Point::new(ix as f32 * cell, iy as f32 * cell),
                max: folio_document::Point::new((ix + 1) as f32 * cell, (iy + 1) as f32 * cell),
            };
            let Some(part) = intersect_rect(rect, region) else {
                continue;
            };
            let (snapped, w, h) = snap_bounds(part, density);
            let tile = ((ix as u64) << 32 ^ (iy as u32 as u64)) | 1 << 63;
            cells.push((snapped.center().distance(centre), snapped, (w * h) as usize, tile));
        }
    }
    cells.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut total = 0;
    let mut out = Vec::new();
    for (_, bounds, pixels, tile) in cells {
        if total + pixels > PIXEL_BUDGET {
            if clip.is_none() {
                return None;
            }
            break;
        }
        total += pixels;
        out.push(InkBatch {
            objects: members.to_vec(),
            bounds,
            theme,
            density,
            pixels,
            tile,
        });
    }
    (!out.is_empty()).then_some(out)
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
    #[allow(clippy::too_many_arguments)]
    fn emit_run(
        &mut self,
        page: &Page,
        ranks: &[usize],
        vis: Range<usize>,
        run: Range<usize>,
        theme: CanvasTheme,
        density: f32,
        clip: Option<Rect>,
        used: &mut HashSet<Id>,
        batches: &mut Vec<(Range<usize>, InkBatch)>,
    ) {
        if vis.is_empty() {
            return;
        }
        let at = |rank: usize| page.order.get(rank).and_then(|id| page.objects.get(id));
        let points = |run: &Range<usize>| -> usize {
            run.clone()
                .filter_map(at)
                .map(|o| match o.as_ref() {
                    Object::Stroke(s) => s.path.len(),
                    _ => 0,
                })
                .sum()
        };
        if run.len() < MIN_BATCH && points(&run) < HEAVY_POINTS {
            return;
        }
        let mut bounds: Option<Rect> = None;
        for rank in run.clone() {
            if let Some(object) = at(rank) {
                let rect = self.bounds(object);
                bounds = Some(match bounds {
                    Some(existing) => existing.union(rect),
                    None => rect,
                });
            }
        }
        let Some(bounds) = bounds else {
            return;
        };
        let (mut bounds, mut width, mut height) = snap_bounds(bounds.expand(3.), density);
        let mut pixels = width * height;
        let fits = |w: f32, h: f32, px: f32| {
            w > 0. && h > 0. && w <= 4096. && h <= 4096. && px <= MAX_BATCH_PIXELS as f32
        };
        if !fits(width, height, pixels)
            && let Some(clip_rect) = clip
            && let Some(clipped) = intersect_rect(bounds, clip_rect)
        {
            let (snapped, cw, ch) = snap_bounds(clipped, density);
            let cpx = cw * ch;
            if fits(cw, ch, cpx) {
                bounds = snapped;
                width = cw;
                height = ch;
                pixels = cpx;
            }
        }
        if fits(width, height, pixels) {
            let heavy = points(&run) >= HEAVY_POINTS;
            let members = run.filter_map(at).cloned().collect::<Vec<_>>();
            if members.len() >= MIN_BATCH || heavy {
                used.extend(members.iter().map(|o| o.id()));
                batches.push((
                    vis,
                    InkBatch {
                        objects: members,
                        bounds,
                        theme,
                        density,
                        pixels: pixels as usize,
                        tile: 0,
                    },
                ));
            }
            return;
        }
        if run.len() < MIN_BATCH * 2 {
            let members = run.filter_map(at).cloned().collect::<Vec<_>>();
            if let Some(tiles) = tiles(&members, bounds, clip, theme, density) {
                used.extend(members.iter().map(|o| o.id()));
                batches.extend(tiles.into_iter().map(|tile| (vis.clone(), tile)));
            }
            return;
        }
        if run.len() >= MIN_BATCH * 2 {
            let mid_rank = run.start + run.len() / 2;
            let mid_vis = vis.start + ranks[vis.clone()].partition_point(|&r| r < mid_rank);
            self.emit_run(
                page,
                ranks,
                vis.start..mid_vis,
                run.start..mid_rank,
                theme,
                density,
                clip,
                used,
                batches,
            );
            self.emit_run(
                page,
                ranks,
                mid_vis..vis.end,
                mid_rank..run.end,
                theme,
                density,
                clip,
                used,
                batches,
            );
        }
    }
    /// Batches for the visible `objects`, which are in page order at
    /// `ranks`. Membership follows fixed blocks of the page order rather
    /// than what is visible, so panning and zooming keep every batch (and
    /// its cached image) intact; a batch includes its offscreen members.
    #[allow(clippy::too_many_arguments)]
    pub fn batches(
        &mut self,
        page: &Page,
        objects: &[Arc<Object>],
        ranks: &[usize],
        excluded: &HashSet<Id>,
        theme: CanvasTheme,
        density: f32,
        clip: Option<Rect>,
    ) -> Vec<(Range<usize>, InkBatch)> {
        if !density.is_finite() || density <= 0. {
            self.bounds.clear();
            return Vec::new();
        }
        // Round up so every cached image has at least one texel per device pixel.
        let density = (density * 2.).ceil() / 2.;
        let stride = BATCH_SIZE;
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
            let block = rank / stride * stride;
            let block_end = (block + stride).min(page.order.len());
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
            self.emit_run(
                page,
                ranks,
                start..end,
                first..last,
                theme,
                density,
                clip,
                &mut used,
                &mut batches,
            );
            start = end;
        }
        self.bounds.retain(|id, _| used.contains(id));
        if self.bounds.capacity() > self.bounds.len().saturating_mul(2) + 128 {
            self.bounds.shrink_to_fit();
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
    let page = test_page(objects);
    let ranks = (0..objects.len()).collect::<Vec<_>>();
    Planner::default().batches(&page, objects, &ranks, excluded, theme, density, None)
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

pub(super) fn raster(batch: &InkBatch) -> Option<Arc<RenderImage>> {
    let mut strokes = Vec::with_capacity(batch.objects.len());
    for object in &batch.objects {
        let Object::Stroke(stroke) = object.as_ref() else {
            return None;
        };
        let color = folio_document::Color::from_rgb(batch.theme.ink(stroke.style.color.rgb()));
        strokes.push((stroke, color));
    }
    let pixmap = folio_export::raster_ink_strokes(strokes, batch.bounds, batch.density).ok()?;
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
        let partial =
            Planner::default().batches(&page, visible, &ranks, &excluded, theme(), 1.25, None);
        let full = batches(&objects, &excluded, theme(), 1.25);
        assert_eq!(partial[0].0, 0..15);
        assert!(partial[0].1.matches(&full[0].1) && partial[1].1.matches(&full[1].1));
    }
    #[test]
    fn identity_theme_density_and_geometry_invalidate_cached_batches() {
        let mut objects = fixture(32);
        let original = batches(&objects, &HashSet::new(), theme(), 1.25)
            .remove(0)
            .1;
        assert!(
            original.matches(
                &batches(&objects, &HashSet::new(), theme(), 1.25)
                    .remove(0)
                    .1
            )
        );
        // Snapped bounds must match raster pixmap dimensions 1:1 with zero stretch.
        let image = raster(&original).unwrap();
        let size = image.size(0);
        assert!(
            (original.bounds.width() * original.density - size.width.0 as f32).abs() < 1e-3,
            "width mismatch: {} vs {}",
            original.bounds.width() * original.density,
            size.width.0
        );
        assert!(
            (original.bounds.height() * original.density - size.height.0 as f32).abs() < 1e-3,
            "height mismatch: {} vs {}",
            original.bounds.height() * original.density,
            size.height.0
        );
        // Appending a stroke to a partial block keeps the prior batch as a valid prefix.
        let shorter = batches(&objects[..12], &HashSet::new(), theme(), 1.25)
            .remove(0)
            .1;
        let longer = batches(&objects[..13], &HashSet::new(), theme(), 1.25)
            .remove(0)
            .1;
        assert!(shorter.is_prefix_of(&longer));
        assert!(!longer.is_prefix_of(&shorter));
        // While zooming, the image made at the old density stands in for the
        // new one although snapping gives the batches different bounds.
        for density in [0.5, 2., 3.5] {
            let rezoomed = batches(&objects, &HashSet::new(), theme(), density)
                .remove(0)
                .1;
            assert!(!original.matches(&rezoomed));
            assert!(original.stands_in_for(&rezoomed), "density {density}");
        }
        assert!(!shorter.stands_in_for(&longer));

        objects[7] = Arc::new(objects[7].as_ref().clone());
        assert!(
            !original.matches(
                &batches(&objects, &HashSet::new(), theme(), 1.25)
                    .remove(0)
                    .1
            )
        );
        let mut changed = original.clone();
        changed.theme.adapt_ink = true;
        assert!(!original.matches(&changed));
        changed = original.clone();
        changed.density = 2.;
        assert!(!original.matches(&changed));
        objects.reverse();
        assert!(
            !original.matches(
                &batches(&objects, &HashSet::new(), theme(), 1.25)
                    .remove(0)
                    .1
            )
        );
        assert!(raster(&original).is_some());
    }
    #[test]
    fn oversized_images_fall_back_to_vectors_without_reducing_resolution() {
        let objects = fixture(32);
        assert!(batches(&objects, &HashSet::new(), theme(), 100.).is_empty());
        assert!(batches(&objects, &HashSet::new(), theme(), f32::NAN).is_empty());
        // One long scribble is worth an image on its own, and seen up close
        // it is tiled within the view's clip instead of drawn as vectors.
        let mut b = folio_ink::StrokeBuilder::new(PenStyle::default());
        for j in 0..2000 {
            let f = j as f32;
            b.push(StrokePoint::new(
                Point::new(200. + (f * 0.37).sin() * 150., 200. + (f * 0.31).cos() * 150.),
                0.7,
                j * 4,
            ));
        }
        let scribble = vec![Arc::new(Object::Stroke(b.finish().unwrap()))];
        let page = test_page(&scribble);
        let mut planner = Planner::default();
        let whole = planner.batches(&page, &scribble, &[0], &HashSet::new(), theme(), 2., None);
        assert_eq!(whole.len(), 1);
        assert_eq!(whole[0].1.tile, 0);
        let clip = Rect::new(100., 100., 200., 200.);
        let tiled = planner.batches(&page, &scribble, &[0], &HashSet::new(), theme(), 20., Some(clip));
        assert!(tiled.len() > 1);
        let ids = tiled.iter().map(|(_, t)| t.id()).collect::<HashSet<_>>();
        assert_eq!(ids.len(), tiled.len());
        assert!(!ids.contains(&scribble[0].id()));
        let area: f32 = tiled.iter().map(|(_, t)| t.bounds.width() * t.bounds.height()).sum();
        assert!(area >= clip.width() * clip.height() - 1.);
        for (_, tile) in &tiled {
            assert!(tile.pixels <= MAX_BATCH_PIXELS && raster(tile).is_some());
        }
        // The same view plans the same tiles, so their images survive.
        let again = planner.batches(&page, &scribble, &[0], &HashSet::new(), theme(), 20., Some(clip));
        assert!(again.iter().zip(&tiled).all(|((_, a), (_, b))| a.id() == b.id() && a.matches(b)));
    }
    #[test]
    fn raster_preserves_selected_colors_opacity_and_overlap_order() {
        let make = |color| {
            let mut builder = folio_ink::StrokeBuilder::new(PenStyle::default());
            builder.push(StrokePoint::new(Point::new(10., 20.), 1., 0));
            builder.push(StrokePoint::new(Point::new(50., 20.), 1., 10));
            let mut stroke = builder.finish().unwrap();
            stroke.path = Arc::new(
                vec![
                    PathPoint {
                        position: Point::new(10., 20.),
                        radius: 4.,
                    },
                    PathPoint {
                        position: Point::new(50., 20.),
                        radius: 4.,
                    },
                ]
                .into(),
            );
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
                tile: 0,
            };
            let image = raster(&batch).unwrap();
            let width = image.size(0).width.0 as usize;
            let pixel = image.with_bytes(0, |bytes| {
                let bytes = bytes.unwrap();
                let i = (20 * width + 60) * 4;
                [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
            });
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
