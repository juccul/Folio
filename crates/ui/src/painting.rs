use super::{NotesView, Theme};
use super::{graph, theme::CanvasTheme};
use folio_app::{Controller, Interaction, Tool};
use folio_canvas::{PageStack, SpatialIndex, Viewport};
use folio_document::{
    Id, Object, Page, PageProperties, Paper, PathPoint, Point as DocPoint, Rect, Transform,
};
use super::*;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub struct CanvasFrame {
    pub objects: Vec<Arc<Object>>,
    pub viewport: Viewport,
    pub visible: Rect,
}
struct VisibleFrame {
    page: Id,
    revision: u64,
    generation: u64,
    coverage: Rect,
    objects: Vec<Arc<Object>>,
    /// Page-order position of each of `objects`.
    ranks: Vec<usize>,
    ids: HashSet<Id>,
    preview_ids: HashSet<Id>,
}
fn prepare_visible_frame(
    cached: &mut Option<Arc<VisibleFrame>>,
    page: &Page,
    index: &SpatialIndex,
    positions: &HashMap<Id, usize>,
    visible: Rect,
    zoom: f32,
) -> bool {
    // A small screen-scaled margin keeps culling and ink batches stable during
    // resize/pan. Large shrinks still release the old, larger working set.
    let margin = 64. / zoom.max(0.05);
    let limit = visible.expand(margin * 2.);
    if cached.as_ref().is_some_and(|frame| {
        frame.page == page.id
            && frame.revision == page.revision
            && frame.generation == index.generation()
            && frame.coverage.contains(visible.min)
            && frame.coverage.contains(visible.max)
            && limit.contains(frame.coverage.min)
            && limit.contains(frame.coverage.max)
    }) {
        return false;
    }
    let coverage = visible.expand(margin);
    let ids = index.query(coverage);
    let (objects, ranks): (Vec<Arc<Object>>, Vec<usize>) = if ids.len() > page.order.len() / 2 {
        // Dense views already have a cheap ordered traversal. Sorting their
        // hashed candidates repeats rank lookups on every content invalidation.
        page.order
            .iter()
            .enumerate()
            .filter(|(_, id)| ids.contains(id))
            .filter_map(|(rank, id)| page.objects.get(id).map(|o| (o.clone(), rank)))
            .unzip()
    } else {
        let mut ordered = ids
            .iter()
            .map(|id| (positions.get(id).copied().unwrap_or(usize::MAX), *id))
            .collect::<Vec<_>>();
        ordered.sort_unstable_by_key(|(rank, _)| *rank);
        ordered
            .iter()
            .filter_map(|(rank, id)| page.objects.get(id).map(|o| (o.clone(), *rank)))
            .unzip()
    };
    let preview_ids = objects
        .iter()
        .filter(|object| matches!(object.as_ref(), Object::Image(_) | Object::Equation(_)))
        .map(|object| object.id())
        .collect();
    *cached = Some(Arc::new(VisibleFrame {
        page: page.id,
        revision: page.revision,
        generation: index.generation(),
        coverage,
        objects,
        ranks,
        ids,
        preview_ids,
    }));
    true
}
struct InkPlan {
    frame: Arc<VisibleFrame>,
    excluded: HashSet<Id>,
    theme: CanvasTheme,
    density: f32,
    clip: Option<Rect>,
    batches: Arc<Vec<(std::ops::Range<usize>, super::ink_layers::InkBatch)>>,
}
#[derive(PartialEq)]
struct PaperKey {
    paper: Paper,
    page: Rect,
    visible: Rect,
    world: Transform,
    canvas: CanvasTheme,
}
struct CachedPaper {
    key: PaperKey,
    scale: f32,
    geometry: PaperGeometry,
}
enum PaperGeometry {
    Rectangles(Vec<(Rect, Rgba)>),
    Paths(Vec<(Path<Pixels>, Rgba)>),
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct DotTileKey {
    cells: u32,
    tile_px: u32,
    radius_tenths: u32,
    color: u32,
}
struct CachedDotTile {
    key: DotTileKey,
    image: Arc<RenderImage>,
}
#[derive(Clone, Copy, PartialEq)]
struct ActivePathKey {
    id: Id,
    len: usize,
    last: Option<PathPoint>,
}
#[derive(Default)]
struct ActivePathCache {
    key: Option<ActivePathKey>,
    path: Option<Path<Pixels>>,
    scratch: folio_ink::OutlineScratch,
}
impl ActivePathCache {
    fn path(&mut self, id: Id, points: &[PathPoint]) -> Option<&Path<Pixels>> {
        // StrokeBuilder appends points or adjusts only the final radius.
        // Include the final sample so stationary pressure changes invalidate.
        let key = ActivePathKey {
            id,
            len: points.len(),
            last: points.last().copied(),
        };
        if self.key != Some(key) {
            self.path = ink_path_from_points(points, &mut self.scratch);
            self.key = Some(key);
        }
        self.path.as_ref()
    }
}
struct CachedText {
    object: Arc<Object>,
    key: super::text_render::LayoutKey,
    layout: super::text_render::Layout,
    width: f32,
}
struct CachedPath {
    object: Arc<Object>,
    path: Path<Pixels>,
}
struct CachedInkLayer {
    batch: super::ink_layers::InkBatch,
    image: Arc<RenderImage>,
}
#[derive(Clone)]
struct RasterRequest {
    page: Id,
    token: Arc<()>,
    appearance: Option<RasterAppearance>,
}
#[derive(Clone, Copy, PartialEq)]
enum RasterAppearance {
    Ink(CanvasTheme),
    Graph(graph::Palette),
}
impl RasterRequest {
    fn matches(&self, other: &Self) -> bool {
        self.page == other.page
            && Arc::ptr_eq(&self.token, &other.token)
            && self.appearance == other.appearance
    }
}
struct CachedRaster {
    token: Arc<()>,
    image: Arc<RenderImage>,
    appearance: Option<RasterAppearance>,
    /// The object and bounds the raster was made for. After an edit that
    /// only changes the transform, the image stays on screen, transformed,
    /// until the fresh raster replaces it.
    object: Arc<Object>,
    bounds: Rect,
}
#[derive(Default)]
pub struct Painter {
    pub(super) ink_layer_override: Option<bool>,
    /// Leave out the selection outline and handles, as while cropping.
    pub(super) hide_selection: bool,
    ink_planner: super::ink_layers::Planner,
    ink_plan: Option<InkPlan>,
    ink_layers: HashMap<Id, CachedInkLayer>,
    /// Layers made for an earlier display density, painted (scaled) until
    /// their replacement is ready, so zooming never drops back to vectors.
    ink_stale: HashMap<Id, CachedInkLayer>,
    ink_pending: HashMap<Id, super::ink_layers::InkBatch>,
    ink_failed: HashMap<Id, super::ink_layers::InkBatch>,
    ink_desired: HashMap<Id, super::ink_layers::InkBatch>,
    text_layouts: HashMap<Id, CachedText>,
    pages: HashMap<Id, Box<Painter>>,
    index: SpatialIndex,
    index_revision: Option<u64>,
    order_positions: HashMap<Id, usize>,
    paths: HashMap<Id, CachedPath>,
    frame: Option<Arc<VisibleFrame>>,
    paper_cache: Option<CachedPaper>,
    dot_tile: Option<CachedDotTile>,
    rasters: HashMap<Id, CachedRaster>,
    raster_pending: HashMap<Id, RasterRequest>,
    backgrounds: HashMap<String, ImageSource>,
    pdf_view: Option<(Id, Id, Transform, Size<Pixels>)>,
    pdf_moved: Option<std::time::Instant>,
    pdf_idle_repaint: bool,
    page: Option<Id>,
    active_id: Option<Id>,
    active_chunks: Vec<Path<Pixels>>,
    active_path: ActivePathCache,
    outline_scratch: folio_ink::OutlineScratch,
    path_buf: Vec<PathPoint>,
    appearance: Option<CanvasTheme>,
    graph_palette: Option<graph::Palette>,
    colors: HashMap<u32, u32>,
    /// The zoom ink layers are rasterized for. It follows the viewport once
    /// zooming pauses, so a continuous zoom reuses its cached images.
    layer_zoom: f32,
    zoom_seen: f32,
    zoom_moved: Option<std::time::Instant>,
    zoom_repaint: bool,
    /// Whether the view is moving, so uncached ink is tessellated within
    /// [`TESSELLATION_BUDGET`].
    limit_tessellation: bool,
}
/// How long the zoom must rest before ink is rasterized at the new scale.
const ZOOM_SETTLE: std::time::Duration = std::time::Duration::from_millis(140);
/// Uncached strokes tessellated per page and frame. The rest stay hidden for
/// a frame or two instead of stalling the frame while their images render.
const TESSELLATION_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);
fn alpha(color: u32, opacity: f32) -> Rgba {
    let mut color = rgb(color);
    color.a = opacity;
    color
}
/// Warm golden-yellow ink color used while Auto Math Solve writes out an answer,
/// chosen for crisp contrast against the actual page paper brightness.
fn auto_math_yellow(paper_rgb: u32) -> u32 {
    let r = (paper_rgb >> 16) & 0xff;
    let g = (paper_rgb >> 8) & 0xff;
    let b = paper_rgb & 0xff;
    let dark_paper = r * 299 + g * 587 + b * 114 < 128_000;
    if dark_paper { 0xFFD43B } else { 0xE8A800 }
}
fn lerp_rgb(from: u32, to: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u32, b: u32| -> u32 {
        let af = a as f32;
        let bf = b as f32;
        (af + (bf - af) * t).round().clamp(0.0, 255.0) as u32
    };
    let r = mix((from >> 16) & 0xff, (to >> 16) & 0xff);
    let g = mix((from >> 8) & 0xff, (to >> 8) & 0xff);
    let b = mix(from & 0xff, to & 0xff);
    (r << 16) | (g << 8) | b
}
/// Slice `points` smoothly along cumulative arc length to fraction `progress in (0, 1]`.
fn partial_stroke_points(points: &[PathPoint], progress: f32, out: &mut Vec<PathPoint>) {
    out.clear();
    if points.is_empty() || progress <= 0.0 {
        return;
    }
    if progress >= 1.0 {
        out.extend_from_slice(points);
        return;
    }
    if points.len() == 1 {
        let mut p = points[0];
        p.radius *= 0.25 + 0.75 * progress.sqrt();
        out.push(p);
        return;
    }
    let mut total_len = 0.0_f32;
    for seg in points.windows(2) {
        total_len += seg[0].position.distance(seg[1].position);
    }
    if total_len <= 1e-4 {
        let scale = 0.25 + 0.75 * progress.sqrt();
        out.extend(points.iter().map(|p| PathPoint {
            position: p.position,
            radius: p.radius * scale,
        }));
        return;
    }
    let target = progress * total_len;
    out.push(points[0]);
    let mut acc = 0.0_f32;
    for seg in points.windows(2) {
        let seg_len = seg[0].position.distance(seg[1].position);
        if acc + seg_len >= target {
            let f = if seg_len > 1e-5 {
                ((target - acc) / seg_len).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let pos = seg[0].position.lerp(seg[1].position, f);
            let r = (seg[0].radius + (seg[1].radius - seg[0].radius) * f) * 0.85;
            out.push(PathPoint {
                position: pos,
                radius: r.max(0.1),
            });
            break;
        }
        acc += seg_len;
        out.push(seg[1]);
    }
}
fn point_px(p: DocPoint) -> Point<Pixels> {
    point(px(p.x), px(p.y))
}
fn coefficients(t: Transform) -> [f32; 6] {
    [t.a, t.b, t.c, t.d, t.tx, t.ty]
}
fn fill_path(points: &[DocPoint]) -> Option<Path<Pixels>> {
    let first = points.first()?;
    let mut b = PathBuilder::fill();
    b.move_to(point_px(*first));
    for p in points.iter().skip(1) {
        b.line_to(point_px(*p));
    }
    b.close();
    b.build().ok()
}
fn push_contour(builder: &mut PathBuilder, points: &[DocPoint]) {
    if let Some(first) = points.first() {
        builder.move_to(point_px(*first));
        for p in points.iter().skip(1) {
            builder.line_to(point_px(*p));
        }
        builder.close();
    }
}
fn ink_path(contours: &[Vec<DocPoint>]) -> Option<Path<Pixels>> {
    if contours.is_empty() {
        return None;
    }
    let mut b = PathBuilder::fill().with_style(PathStyle::Fill(
        FillOptions::default().with_fill_rule(FillRule::NonZero),
    ));
    for points in contours {
        push_contour(&mut b, points);
    }
    b.build().ok()
}
fn ink_path_from_points(
    points: &[PathPoint],
    scratch: &mut folio_ink::OutlineScratch,
) -> Option<Path<Pixels>> {
    if points.is_empty() {
        return None;
    }
    let mut b = PathBuilder::fill().with_style(PathStyle::Fill(
        FillOptions::default().with_fill_rule(FillRule::NonZero),
    ));
    let mut any = false;
    folio_ink::visit_outline(points, scratch, |contour| {
        if !contour.is_empty() {
            any = true;
            push_contour(&mut b, contour);
        }
    });
    any.then(|| b.build().ok()).flatten()
}
#[cfg(test)]
fn shape_contours(points: &[DocPoint], width: f32) -> Vec<Vec<DocPoint>> {
    let centerline = points
        .iter()
        .map(|&position| PathPoint {
            position,
            radius: width / 2.,
        })
        .collect::<Vec<_>>();
    folio_ink::outline(&centerline)
}
pub(super) fn shape_path(points: &[DocPoint], width: f32) -> Option<Path<Pixels>> {
    let centerline = points
        .iter()
        .map(|&position| PathPoint {
            position,
            radius: width / 2.,
        })
        .collect::<Vec<_>>();
    let mut scratch = folio_ink::OutlineScratch::default();
    ink_path_from_points(&centerline, &mut scratch)
}
fn line_path(points: &[DocPoint], width: f32, dashed: bool) -> Option<Path<Pixels>> {
    let first = points.first()?;
    let mut b = PathBuilder::stroke(px(width));
    if dashed {
        b = b.dash_array(&[px(5.), px(4.)]);
    }
    b.move_to(point_px(*first));
    for p in points.iter().skip(1) {
        b.line_to(point_px(*p));
    }
    b.build().ok()
}
fn rect_points(r: Rect) -> [DocPoint; 5] {
    [
        r.min,
        DocPoint::new(r.max.x, r.min.y),
        r.max,
        DocPoint::new(r.min.x, r.max.y),
        r.min,
    ]
}
fn selection_overlay(
    bounds: Rect,
    zoom: f32,
    transform: Transform,
) -> ([DocPoint; 5], [DocPoint; 2]) {
    // Clear of the content; handles are hit where they are drawn.
    let outline =
        rect_points(bounds.expand(folio_app::SELECTION_PADDING / zoom)).map(|p| transform.apply(p));
    let rotation = [
        DocPoint::new(
            bounds.center().x,
            bounds.min.y - folio_app::SELECTION_PADDING / zoom,
        ),
        DocPoint::new(bounds.center().x, bounds.min.y - 24. / zoom),
    ]
    .map(|p| transform.apply(p));
    (outline, rotation)
}
fn gpui_bounds(r: Rect) -> Bounds<Pixels> {
    Bounds::new(point_px(r.min), size(px(r.width()), px(r.height())))
}
fn visible_page_range(
    stack: &PageStack,
    viewport: Viewport,
    active: usize,
    width: f32,
    height: f32,
) -> std::ops::Range<usize> {
    let visible = viewport.visible(width, height);
    let origin = stack.frame(active).min;
    let minimum = visible.min.y + origin.y;
    let maximum = visible.max.y + origin.y;
    let start = stack
        .pages
        .partition_point(|(_, rect)| rect.max.y < minimum);
    let end = stack
        .pages
        .partition_point(|(_, rect)| rect.min.y <= maximum);
    start..end.max(start)
}
impl Painter {
    /// Whether rasterization started for the visible pages is still running.
    pub(super) fn busy(&self) -> bool {
        self.pages
            .values()
            .any(|p| !p.ink_pending.is_empty() || !p.raster_pending.is_empty())
    }
    pub(super) fn clear(&mut self, window: &mut Window, cx: &mut Context<NotesView>) {
        for source in self.backgrounds.values() {
            Self::release_background(source, window, cx);
        }
        self.clear_page(window);
    }
    fn release_background(source: &ImageSource, window: &mut Window, cx: &mut Context<NotesView>) {
        // Sources are requested when added to the warm cache. Removing the
        // decoded asset alone does not remove its uploaded sprite-atlas image.
        if let Some(image) = source.render_image() {
            let _ = window.drop_image(image);
        }
        source.forget();
    }
    fn clear_page(&mut self, window: &mut Window) {
        for painter in self.pages.values_mut() {
            painter.clear_page(window);
        }
        if let Some(tile) = self.dot_tile.take() {
            let _ = window.drop_image(tile.image);
        }
        self.clear_ink_layers(window);
        self.retain_rasters(
            |_, _| false,
            |image| {
                let _ = window.drop_image(image);
            },
        );
        // The root painter owns the warm PDF asset set. Page eviction must not
        // remove decoded assets still used by its neighboring-page prefetch.
        *self = Self {
            ink_layer_override: self.ink_layer_override,
            ..Self::default()
        };
    }
    fn retain_rasters(
        &mut self,
        mut keep: impl FnMut(Id, &CachedRaster) -> bool,
        mut release: impl FnMut(Arc<RenderImage>),
    ) {
        self.rasters.retain(|id, raster| {
            if keep(*id, raster) {
                true
            } else {
                release(raster.image.clone());
                false
            }
        });
    }
    pub(super) fn ink_layer_counts(&self) -> (usize, usize) {
        self.pages
            .values()
            .fold((0, 0), |(layers, strokes), painter| {
                (
                    layers + painter.ink_layers.len(),
                    strokes
                        + painter
                            .ink_layers
                            .values()
                            .map(|layer| layer.batch.objects.len())
                            .sum::<usize>(),
                )
            })
    }
    fn display_color(&mut self, color: u32, theme: CanvasTheme) -> u32 {
        *self.colors.entry(color).or_insert_with(|| theme.ink(color))
    }
    pub fn paint(
        &mut self,
        controller: &mut Controller,
        bounds: Bounds<Pixels>,
        editing: Option<Id>,
        window: &mut Window,
        cx: &mut Context<NotesView>,
    ) {
        let session = controller.session();
        let active = session.page;
        let viewport = session.viewport;
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        let stack = session.page_stack();
        let frames = if let Some(stack) = &stack {
            let range = visible_page_range(stack, viewport, active, width, height);
            let mut frames = stack.pages[range]
                .iter()
                .filter_map(|(index, rect)| {
                    let local = stack.viewport(viewport, active, *index);
                    let visible = local.visible(width, height);
                    visible
                        .intersects(Rect::new(0., 0., rect.width(), rect.height()))
                        .then_some((*index, local))
                })
                .collect::<Vec<_>>();
            // Selection/input overlays belong to the active page even when
            // a scroll places its paper outside the visible range.
            if !frames.iter().any(|(index, _)| *index == active) {
                frames.push((active, viewport));
            }
            frames
        } else {
            vec![(active, viewport)]
        };
        let ids = frames
            .iter()
            .map(|(i, _)| controller.session().document.pages[*i].id)
            .collect::<HashSet<_>>();
        let view = (
            controller.active,
            session.page().id,
            viewport.transform(),
            bounds.size,
        );
        if self.pdf_view != Some(view) {
            self.pdf_view = Some(view);
            self.pdf_moved = Some(std::time::Instant::now());
        }
        let settled = self
            .pdf_moved
            .is_some_and(|last| last.elapsed() >= std::time::Duration::from_millis(180));
        let now = std::time::Instant::now();
        if viewport.zoom != self.zoom_seen {
            self.zoom_seen = viewport.zoom;
            self.zoom_moved = Some(now);
        }
        let resting = self
            .zoom_moved
            .is_none_or(|moved| now.duration_since(moved) >= ZOOM_SETTLE);
        // During a zoom, cached ink is scaled on the GPU. It is rasterized
        // again once the zoom rests, or when it would be scaled by more than
        // two times.
        let ratio = viewport.zoom / self.layer_zoom;
        if self.layer_zoom <= 0. || resting || !(0.5..=2.).contains(&ratio) {
            self.layer_zoom = viewport.zoom;
        }
        if !resting && !self.zoom_repaint {
            self.zoom_repaint = true;
            cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .timer(ZOOM_SETTLE + std::time::Duration::from_millis(10))
                    .await;
                let _ = view.update(cx, |view, cx| {
                    view.painter.zoom_repaint = false;
                    cx.notify();
                });
            })
            .detach();
        }
        if !settled
            && !self.pdf_idle_repaint
            && frames
                .iter()
                .any(|(index, _)| session.document.pages[*index].properties.pdf.is_some())
        {
            self.pdf_idle_repaint = true;
            // Movement itself can be the last frame. Ensure sharp previews are
            // requested after it stops, even with no other background work.
            cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                let _ = view.update(cx, |view, cx| {
                    view.painter.pdf_idle_repaint = false;
                    cx.notify();
                });
            })
            .detach();
        }
        // Prepare visible PDFs first, then one neighbor on either side. Waiting
        // until a page is painted starts both Poppler and PNG decoding too late
        // for continuous scrolling; keeping only immediate neighbors bounded
        // prevents multi-page PDF sessions from holding excessive decoded PNGs.
        let mut nearby = frames.iter().map(|(index, _)| *index).collect::<Vec<_>>();
        if let Some(stack) = &stack
            && let (Some((first_page, _)), Some((last_page, _))) =
                (stack.pages.first(), stack.pages.last())
        {
            let first = nearby.iter().copied().min().unwrap_or(active);
            let last = nearby.iter().copied().max().unwrap_or(active);
            for index in [last.checked_add(1), first.checked_sub(1)]
                .into_iter()
                .flatten()
            {
                if index >= *first_page && index <= *last_page && !nearby.contains(&index) {
                    nearby.push(index);
                }
            }
        }
        let mut warm_assets = HashSet::new();
        let mut visible_decoding = false;
        for &index in &nearby {
            let page_id = controller.session().document.pages[index].id;
            let visible_page = ids.contains(&page_id);
            let Some(pdf) = controller.session().document.pages[index]
                .properties
                .pdf
                .clone()
            else {
                continue;
            };
            controller.request_pdf_scroll_preview(pdf.clone());
            if settled && visible_page {
                controller.request_pdf_background(pdf.clone());
            }
            let full_asset = pdf
                .preview_asset
                .as_ref()
                .and_then(|asset| controller.asset_path(asset).filter(|p| p.is_file()).map(|p| (asset, p)));
            let scroll_asset = Controller::pdf_scroll_preview_asset(&pdf)
                .filter(|asset| controller.asset_path(asset).is_some_and(|p| p.is_file()));

            let mut full_ready = false;
            if visible_page && let Some((asset, path)) = &full_asset {
                warm_assets.insert((*asset).clone());
                let source = self
                    .backgrounds
                    .entry((*asset).clone())
                    .or_insert_with(|| ImageSource::from(path.clone()));
                full_ready = source.render_image().is_some();
                if !full_ready {
                    visible_decoding = true;
                }
            }
            // Keep the lightweight scroll preview warm while full-res is still
            // decoding (preventing a blank-page flash) and for neighboring pages
            // once visible full-res pages finish decoding.
            if !full_ready {
                if visible_page && full_asset.is_some() {
                    if let Some(asset) = scroll_asset.as_deref()
                        && self.backgrounds.contains_key(asset)
                    {
                        warm_assets.insert(asset.to_owned());
                    }
                } else if !visible_decoding {
                    let fallback = if visible_page {
                        scroll_asset.as_deref()
                    } else {
                        scroll_asset
                            .as_deref()
                            .or(full_asset.map(|(asset, _)| asset.as_str()))
                    };
                    if let Some(asset) = fallback
                        && let Some(path) = controller.asset_path(asset)
                    {
                        warm_assets.insert(asset.to_owned());
                        let source = self
                            .backgrounds
                            .entry(asset.to_owned())
                            .or_insert_with(|| ImageSource::from(path));
                        let _ = source.render_image();
                    }
                }
            }
        }
        // GPUI otherwise retains every decoded PDF ever visited. Drop distant
        // pages while keeping the small neighboring working set warm.
        self.backgrounds.retain(|asset, source| {
            if warm_assets.contains(asset) {
                true
            } else {
                Self::release_background(source, window, cx);
                false
            }
        });
        // Retain visible pages plus one immediate neighbor on each side so
        // scrolling across page boundaries does not evict and re-rasterize
        // ink layers on every reversal, while still bounding long documents.
        let nearby_ids = nearby
            .iter()
            .map(|index| controller.session().document.pages[*index].id)
            .collect::<HashSet<_>>();
        self.pages.retain(|id, painter| {
            if nearby_ids.contains(id) {
                true
            } else {
                painter.clear_page(window);
                false
            }
        });
        let backgrounds = &mut self.backgrounds;
        let dot_tile = &mut self.dot_tile;
        for (index, viewport) in frames {
            let id = controller.session().document.pages[index].id;
            let painter = self.pages.entry(id).or_default();
            painter.ink_layer_override = self.ink_layer_override;
            painter.hide_selection = self.hide_selection;
            painter.layer_zoom = self.layer_zoom;
            painter.limit_tessellation = !settled;
            painter.paint_page(
                controller,
                index,
                viewport,
                bounds,
                editing,
                backgrounds,
                dot_tile,
                window,
                cx,
            );
        }
    }
    // GPUI painting needs both window/context and the complete page/viewport state.
    #[allow(clippy::too_many_arguments)]
    fn paint_page(
        &mut self,
        controller: &mut Controller,
        page_index: usize,
        viewport: Viewport,
        bounds: Bounds<Pixels>,
        editing: Option<Id>,
        backgrounds: &mut HashMap<String, ImageSource>,
        dot_tile: &mut Option<CachedDotTile>,
        window: &mut Window,
        cx: &mut Context<NotesView>,
    ) {
        let active = page_index == controller.session().page;
        if active && !matches!(controller.interaction, Some(Interaction::Ink { .. })) {
            self.active_id = None;
            self.active_chunks.clear();
            self.active_path = ActivePathCache::default();
        }
        let page_id = controller.session().document.pages[page_index].id;
        self.page = Some(page_id);
        let visible =
            viewport.visible(f32::from(bounds.size.width), f32::from(bounds.size.height));
        let page = &controller.session().document.pages[page_index];
        if !active && self.index_revision != Some(page.revision) {
            self.index.rebuild(page);
            self.index_revision = Some(page.revision);
            self.order_positions = page
                .order
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, i))
                .collect();
        }
        if !active {
            for ((note, preview_page, id), preview) in &controller.previews {
                if *note == controller.active
                    && *preview_page == page_id
                    && page
                        .objects
                        .get(id)
                        .is_some_and(|o| Arc::ptr_eq(o, &preview.object))
                {
                    self.index.insert(*id, preview.bounds);
                }
            }
        }
        let index = if active {
            &controller.session().index
        } else {
            &self.index
        };
        let positions = if active {
            &controller.session().order_positions
        } else {
            &self.order_positions
        };
        if prepare_visible_frame(
            &mut self.frame,
            page,
            index,
            positions,
            visible,
            viewport.zoom,
        ) && let Some(frame) = &self.frame
        {
            let ids = &frame.ids;
            let trim_paths = self.paths.len() > 512;
            self.paths
                .retain(|id, _| page.objects.contains_key(id) && (!trim_paths || ids.contains(id)));
            self.text_layouts.retain(|id, _| ids.contains(id));
        }
        let Some(frame) = self.frame.clone() else {
            return;
        };
        let visible_ids = &frame.ids;
        self.retain_rasters(
            |id, cache| {
                visible_ids.contains(&id)
                    && (controller
                        .previews
                        .get(&(controller.active, page_id, id))
                        .is_some_and(|preview| Arc::ptr_eq(&preview.token, &cache.token))
                        || controller.session().document.pages[page_index]
                            .objects
                            .get(&id)
                            .is_some_and(|object| {
                                folio_app::relative_transform(&cache.object, object).is_some()
                            }))
            },
            |image| {
                let _ = window.drop_image(image);
            },
        );
        // If an image raster was evicted after its CPU pixels were released,
        // drop the empty preview shell so `request_page_previews` reloads it.
        for &id in &frame.preview_ids {
            if !self.rasters.contains_key(&id)
                && controller
                    .previews
                    .get(&(controller.active, page_id, id))
                    .is_some_and(|preview| preview.bgra.is_empty())
            {
                controller.previews.remove(&(controller.active, page_id, id));
            }
        }
        if let Some(editing_id) = editing
            && frame.preview_ids.contains(&editing_id)
        {
            let mut preview_ids = frame.preview_ids.clone();
            preview_ids.remove(&editing_id);
            controller.request_page_previews(page_index, &preview_ids);
        } else {
            controller.request_page_previews(page_index, &frame.preview_ids);
        }
        let properties = controller.session().document.pages[page_index]
            .properties
            .clone();
        let world = Transform::translate(f32::from(bounds.origin.x), f32::from(bounds.origin.y))
            .compose(viewport.transform());
        let theme = Theme::new(&controller.settings);
        let canvas_theme = theme.canvas_for_page(&properties);
        let graph_palette = theme.graph_for_page(&properties);
        self.graph_palette = Some(graph_palette);
        if self.appearance != Some(canvas_theme) {
            self.colors.clear();
            self.appearance = Some(canvas_theme);
        }
        if self.colors.len() > 1024 {
            self.colors.clear();
        }
        let empty = HashSet::new();
        let selection = if active {
            &controller.session().selection
        } else {
            &empty
        };
        let preview_move = if active {
            controller.interaction_transform()
        } else {
            Transform::default()
        };
        let erasing =
            if active && let Some(Interaction::Erase { ids, .. }) = &controller.interaction {
                ids
            } else {
                &empty
            };
        // Native backends upload/rasterize every path triangle on each frame.
        // Cache consecutive settled strokes as display-only images. Selection,
        // erasing, active auto-math animations and edits immediately fall back
        // to original vector objects.
        let auto_math_anim =
            controller.auto_math_animation_states(page_id, std::time::Instant::now());
        if !auto_math_anim.is_empty() {
            window.request_animation_frame();
        }
        let mut partial_buf = Vec::new();
        let mut ink_plan_changed = false;
        let ink_batches = if self.ink_layer_override.unwrap_or(true) {
            let mut excluded = selection.union(erasing).copied().collect::<HashSet<_>>();
            excluded.extend(editing);
            excluded.extend(auto_math_anim.keys().copied());
            let density = ((self.layer_zoom * window.scale_factor()) * 2.).ceil() / 2.;
            let clip = super::ink_layers::high_zoom_clip(visible, density);
            if !self.ink_plan.as_ref().is_some_and(|plan| {
                Arc::ptr_eq(&plan.frame, &frame)
                    && plan.excluded == excluded
                    && plan.theme == canvas_theme
                    && plan.density == density
                    && plan.clip == clip
            }) {
                let batches = Arc::new(self.ink_planner.batches(
                    &controller.session().document.pages[page_index],
                    &frame.objects,
                    &frame.ranks,
                    &excluded,
                    canvas_theme,
                    density,
                    clip,
                ));
                self.ink_plan = Some(InkPlan {
                    frame: frame.clone(),
                    excluded,
                    theme: canvas_theme,
                    density,
                    clip,
                    batches: batches.clone(),
                });
                ink_plan_changed = true;
                batches
            } else if let Some(plan) = &self.ink_plan {
                plan.batches.clone()
            } else {
                Arc::new(Vec::new())
            }
        } else {
            ink_plan_changed = self.ink_plan.take().is_some();
            Arc::new(Vec::new())
        };
        self.prepare_ink_layers(&ink_batches, ink_plan_changed, page_id, window, cx);
        let mut batch_iter = ink_batches.iter().peekable();
        let mut skip_until = 0;
        let mut pending_batch_until = 0;
        let frame_start = std::time::Instant::now();
        let mut deferred = false;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.paper(
                &properties,
                visible,
                world,
                backgrounds,
                dot_tile,
                window,
                cx,
                controller,
            );
            for (index, object) in frame.objects.iter().enumerate() {
                if index < skip_until {
                    continue;
                }
                if let Some((range, batch)) = batch_iter.next_if(|(range, _)| range.start == index)
                {
                    if let Some(layer) = self.ink_layers.get(&batch.id()).or_else(|| {
                        self.ink_stale.get(&batch.id()).filter(|layer| {
                            layer.batch.same_ink(batch) && layer.batch.bounds == batch.bounds
                        })
                    }) && window
                        .paint_image_transformed(
                            gpui_bounds(layer.batch.bounds),
                            Default::default(),
                            layer.image.clone(),
                            0,
                            false,
                            gpu_transform(world),
                        )
                        .is_ok()
                    {
                        skip_until = range.end;
                        continue;
                    }
                    if self.ink_desired.contains_key(&batch.id())
                        && !self.ink_failed.contains_key(&batch.id())
                    {
                        pending_batch_until = range.end;
                    }
                    if let Some(layer) = self
                        .ink_stale
                        .get(&batch.id())
                        .filter(|layer| layer.batch.is_prefix_of(batch))
                        && window
                            .paint_image_transformed(
                                gpui_bounds(layer.batch.bounds),
                                Default::default(),
                                layer.image.clone(),
                                0,
                                false,
                                gpu_transform(world),
                            )
                            .is_ok()
                    {
                        let mut covered = range.start;
                        let mut li = 0;
                        while covered < range.end && li < layer.batch.objects.len() {
                            if Arc::ptr_eq(&frame.objects[covered], &layer.batch.objects[li]) {
                                covered += 1;
                            }
                            li += 1;
                        }
                        skip_until = covered;
                        if index < skip_until {
                            continue;
                        }
                    }
                }
                if editing == Some(object.id()) {
                    continue;
                }
                if erasing.contains(&object.id()) {
                    if let Some(Interaction::Erase { fragments, .. }) = &controller.interaction
                        && let Some(parts) = fragments.get(&object.id())
                    {
                        for part in parts {
                            let color = self.display_color(part.style.color.rgb(), canvas_theme);
                            let points = part.decode_display_path(&mut self.path_buf);
                            if let Some(path) =
                                ink_path_from_points(points, &mut self.outline_scratch)
                            {
                                window.paint_path(
                                    path.transformed(coefficients(world.compose(part.transform))),
                                    alpha(color, part.style.opacity),
                                );
                            }
                        }
                    }
                    continue;
                }
                let movement = if selection.contains(&object.id()) {
                    preview_move
                } else {
                    Transform::default()
                };
                let transform = world.compose(movement).compose(object.transform());
                match object.as_ref() {
                    Object::Stroke(s) => {
                        let base_color = self.display_color(s.style.color.rgb(), canvas_theme);
                        let color = if let Some(anim) = auto_math_anim.get(&s.id) {
                            if anim.progress <= 0.001 {
                                continue;
                            }
                            let tinted = lerp_rgb(
                                base_color,
                                auto_math_yellow(canvas_theme.paper),
                                anim.yellow_mix,
                            );
                            if anim.progress < 0.999 {
                                let points = s.decode_display_path(&mut self.path_buf);
                                partial_stroke_points(points, anim.progress, &mut partial_buf);
                                if let Some(path) =
                                    ink_path_from_points(&partial_buf, &mut self.outline_scratch)
                                {
                                    window.paint_path(
                                        path.transformed(coefficients(transform)),
                                        alpha(tinted, s.style.opacity),
                                    );
                                }
                                continue;
                            }
                            tinted
                        } else {
                            base_color
                        };
                        let valid = self
                            .paths
                            .get(&s.id)
                            .is_some_and(|cache| Arc::ptr_eq(&cache.object, object));
                        let awaiting_layer = index < pending_batch_until;
                        if !valid
                            && self.limit_tessellation
                            && frame_start.elapsed() > TESSELLATION_BUDGET
                        {
                            deferred = true;
                            continue;
                        }
                        // Ink in the margin around the view is loaded so its
                        // image is ready when revealed (by a resize, say).
                        // Until then, drawing it as a vector would be wasted.
                        if !valid
                            && !selection.contains(&s.id)
                            && !visible.intersects(object.bounds())
                        {
                            continue;
                        }
                        if valid {
                            if let Some(path) = self.paths.get(&s.id) {
                                window.paint_path(
                                    path.path.clone().transformed(coefficients(transform)),
                                    alpha(color, s.style.opacity),
                                );
                            }
                        } else if awaiting_layer {
                            // A raster layer for this batch is already rendering;
                            // paint transiently without retaining tessellated paths.
                            let points = s.decode_display_path(&mut self.path_buf);
                            if let Some(path) =
                                ink_path_from_points(points, &mut self.outline_scratch)
                            {
                                window.paint_path(
                                    path.transformed(coefficients(transform)),
                                    alpha(color, s.style.opacity),
                                );
                            }
                        } else {
                            let points = s.decode_display_path(&mut self.path_buf);
                            if let Some(path) =
                                ink_path_from_points(points, &mut self.outline_scratch)
                            {
                                window.paint_path(
                                    path.clone().transformed(coefficients(transform)),
                                    alpha(color, s.style.opacity),
                                );
                                self.paths.insert(
                                    s.id,
                                    CachedPath {
                                        object: object.clone(),
                                        path,
                                    },
                                );
                            }
                        }
                    }
                    Object::Text(text) => {
                        // Dragging/rotating changes only the paint transform. A
                        // resized frame changes wrapping and needs preview reflow.
                        let resizing = active
                            && selection.contains(&text.id)
                            && matches!(controller.interaction, Some(Interaction::Resize { .. }));
                        let text = if resizing {
                            Cow::Owned(controller.preview_text(text))
                        } else {
                            Cow::Borrowed(text)
                        };
                        // Thin glyphs need more contrast than strokes to look as
                        // bright on dark paper.
                        let color = canvas_theme.preview_ink(text.color.rgb());
                        let valid = self.text_layouts.get(&text.id).is_some_and(|cached| {
                            (Arc::ptr_eq(&cached.object, object)
                                && cached.width == text.rect.width())
                                || cached.key.matches(&text)
                        });
                        if !valid {
                            let layout = super::text_render::layout(&text, color, window);
                            self.text_layouts.insert(
                                text.id,
                                CachedText {
                                    object: object.clone(),
                                    key: super::text_render::LayoutKey::new(&text),
                                    layout,
                                    width: text.rect.width(),
                                },
                            );
                        } else if let Some(cached) = self.text_layouts.get_mut(&text.id) {
                            cached.object = object.clone();
                        }
                        let movement = if resizing {
                            Transform::default()
                        } else {
                            movement
                        };
                        let transform = world
                            .compose(movement)
                            .compose(text.transform)
                            .compose(Transform::translate(text.rect.min.x, text.rect.min.y));
                        self.text_layouts[&text.id].layout.paint(
                            transform,
                            color,
                            text.underline,
                            window,
                        );
                    }
                    Object::Shape(s) => {
                        let color = self.display_color(s.style.color.rgb(), canvas_theme);
                        let valid = self
                            .paths
                            .get(&s.id)
                            .is_some_and(|cache| Arc::ptr_eq(&cache.object, object));
                        if !valid && let Some(path) = shape_path(&s.vertices, s.style.width) {
                            self.paths.insert(
                                s.id,
                                CachedPath {
                                    object: object.clone(),
                                    path,
                                },
                            );
                        }
                        if let Some(path) = self.paths.get(&s.id) {
                            window.paint_path(
                                path.path.clone().transformed(coefficients(transform)),
                                alpha(color, s.style.opacity),
                            );
                        }
                    }
                    _ => {
                        let appearance = match object.as_ref() {
                            Object::Equation(e)
                                if e.math_link
                                    .as_ref()
                                    .is_some_and(|l| l.operation == "graph") =>
                            {
                                Some(RasterAppearance::Graph(graph_palette))
                            }
                            Object::Text(_) | Object::Equation(_) => {
                                Some(RasterAppearance::Ink(canvas_theme))
                            }
                            _ => None,
                        };
                        let preview = controller
                            .previews
                            .get(&(controller.active, page_id, object.id()))
                            .filter(|preview| Arc::ptr_eq(&preview.object, object));
                        if let Some(preview) = preview {
                            let valid = self.rasters.get(&object.id()).is_some_and(|cache| {
                                Arc::ptr_eq(&cache.token, &preview.token)
                                    && cache.appearance == appearance
                            });
                            if !valid && !preview.bgra.is_empty() {
                                self.request_raster(object.id(), page_id, preview, appearance, cx);
                            }
                        }
                        // The current raster, or the previous one carried onto
                        // the edited transform until the current one is ready.
                        let placement = self
                            .rasters
                            .get(&object.id())
                            .filter(|image| image.appearance == appearance)
                            .and_then(|image| match preview {
                                Some(preview) if Arc::ptr_eq(&image.token, &preview.token) => {
                                    Some((image, preview.bounds, Transform::default()))
                                }
                                _ => folio_app::relative_transform(&image.object, object)
                                    .map(|relative| (image, image.bounds, relative)),
                            });
                        if let Some((image, bounds, relative)) = placement {
                            let transform = world.compose(movement).compose(relative);
                            let _ = window.paint_image_transformed(
                                gpui_bounds(bounds),
                                Default::default(),
                                image.image.clone(),
                                0,
                                false,
                                gpu_transform(transform),
                            );
                        }
                    }
                }
                if let Object::Equation(e) = object.as_ref() {
                    let stale = e
                        .math_link
                        .as_ref()
                        .is_some_and(|link| link.last_error.is_some());
                    let updating = controller.math_updating(e.id);
                    if stale || updating {
                        let rect = object.bounds().expand(3.);
                        let points = rect_points(rect).map(|p| world.compose(movement).apply(p));
                        if let Some(path) = line_path(&points, 1.5, true) {
                            window.paint_path(path, rgb(0xc28c36));
                        }
                        let label = if updating {
                            "Updating calculation…"
                        } else {
                            "Out of date · select to retry"
                        };
                        let origin = world
                            .compose(movement)
                            .apply(DocPoint::new(rect.min.x, rect.min.y));
                        super::text_render::paint_label(
                            label,
                            point(px(origin.x), px(origin.y - 18.)),
                            12.,
                            0xc28c36,
                            window,
                        );
                    }
                }
            }
            if !active {
                return;
            }
            // As in a browser's find bar: every match is marked, and the
            // current one stands out in orange.
            for (index, r) in controller.search_highlights.iter().enumerate() {
                let current = index == controller.search_focus;
                let points = rect_points(r.expand(2.)).map(|p| world.apply(p));
                if let Some(path) = fill_path(&points) {
                    window.paint_path(
                        path,
                        if current {
                            alpha(0xff9632, 0.34)
                        } else {
                            alpha(0xe7c25d, 0.18)
                        },
                    );
                }
                if let Some(path) = line_path(&points, if current { 2. } else { 1.5 }, !current) {
                    window.paint_path(path, rgb(if current { 0xe8740c } else { 0xb79134 }));
                }
            }
            if let Some(active) = &controller.interaction {
                match active {
                    Interaction::Ink {
                        builder,
                        preview,
                        suppress_tap: false,
                        ..
                    } => {
                        let color = alpha(
                            self.display_color(builder.style().color.rgb(), canvas_theme),
                            builder.style().opacity,
                        );
                        if let Some(fit) = preview {
                            if let Some(path) = shape_path(&fit.vertices, builder.style().width) {
                                window.paint_path(path.transformed(coefficients(world)), color);
                            }
                        } else {
                            if self.active_id != Some(builder.id()) {
                                self.active_id = Some(builder.id());
                                self.active_chunks.clear();
                            }
                            let points = builder.path();

                            if builder.style().opacity < 1. {
                                // Translucent strokes still use one compound
                                // fill to preserve opacity at joins/retraces.
                                // Repaints without new geometry reuse tessellation.
                                if let Some(path) = self.active_path.path(builder.id(), points) {
                                    window.paint_path(
                                        path.clone().transformed(coefficients(world)),
                                        color,
                                    );
                                }
                            } else {
                                const CHUNK: usize = 256;
                                let count = points.len().saturating_sub(2) / CHUNK;
                                while self.active_chunks.len() < count {
                                    let start = self.active_chunks.len() * CHUNK;
                                    if let Some(path) = ink_path(&folio_ink::outline_chunk(
                                        points,
                                        start,
                                        start + CHUNK,
                                    )) {
                                        self.active_chunks.push(path);
                                    } else {
                                        break;
                                    }
                                }
                                for path in &self.active_chunks {
                                    window.paint_path(
                                        path.clone().transformed(coefficients(world)),
                                        color,
                                    );
                                }
                                let tail = self.active_chunks.len() * CHUNK;
                                let outline = if points.len() < 2 {
                                    folio_ink::outline(points)
                                } else {
                                    folio_ink::outline_chunk(points, tail, points.len() - 1)
                                };
                                if let Some(path) = ink_path(&outline) {
                                    window.paint_path(path.transformed(coefficients(world)), color);
                                }
                            }
                        }
                    }
                    Interaction::Lasso { points } => {
                        if let Some(path) = line_path(points, 1.2 / viewport.zoom, true) {
                            window.paint_path(
                                path.transformed(coefficients(world)),
                                rgb(theme.selection()),
                            );
                        }
                    }
                    Interaction::Rectangle { start, end } => {
                        let points = rect_points(Rect::from_points([*start, *end]));
                        if let Some(path) = line_path(&points, 1.2 / viewport.zoom, true) {
                            window.paint_path(
                                path.transformed(coefficients(world)),
                                rgb(theme.selection()),
                            );
                        }
                    }
                    _ => {}
                }
            }
            if let Some(rect) = controller
                .selection_bounds()
                .filter(|_| !self.hide_selection)
            {
                let transform = world.compose(preview_move);
                let (points, rotation) = selection_overlay(rect, viewport.zoom, transform);
                if let Some(path) = line_path(&points, 1.5, true) {
                    window.paint_path(path, rgb(theme.selection()));
                }
                // Images also get side handles that stretch one axis.
                let sides = controller
                    .selection_has_image()
                    .then(|| {
                        (0..4).map(|i| {
                            DocPoint::new(
                                (points[i].x + points[i + 1].x) * 0.5,
                                (points[i].y + points[i + 1].y) * 0.5,
                            )
                        })
                    })
                    .into_iter()
                    .flatten();
                for p in points.iter().take(4).copied().chain(sides) {
                    window.paint_quad(quad(
                        Bounds::new(
                            point_px(DocPoint::new(p.x - 5., p.y - 5.)),
                            size(px(10.), px(10.)),
                        ),
                        px(2.),
                        rgb(0xffffff),
                        px(1.5),
                        rgb(theme.selection()),
                        Default::default(),
                    ));
                }
                if let Some(path) = line_path(&rotation, 1.5, false) {
                    window.paint_path(path, rgb(theme.selection()));
                }
                if let Some(path) = fill_path(&folio_ink::circle(rotation[1], 5., 16)) {
                    window.paint_path(path, rgb(theme.selection()));
                }
            }
            if let Some(cursor) = controller.cursor {
                let center = world.apply(cursor);
                let r = if controller.tool == Tool::Eraser {
                    controller.settings.eraser_radius * viewport.zoom
                } else {
                    controller.settings.cursor_size * 0.5
                };
                let on_page = properties.infinite
                    || Rect::new(0., 0., properties.width, properties.height).contains(cursor);
                let background = if on_page {
                    canvas_theme.paper
                } else {
                    theme.desk()
                };
                let (ring, halo) = super::theme::cursor_colors(background);
                let circle = folio_ink::circle(center, r, 32)
                    .into_iter()
                    .chain(std::iter::once(DocPoint::new(center.x + r, center.y)))
                    .collect::<Vec<_>>();
                if let Some(path) = line_path(&circle, 3., false) {
                    window.paint_path(path, alpha(halo, 0.7));
                }
                if let Some(path) = line_path(&circle, 1., false) {
                    window.paint_path(path, rgb(ring));
                }
            }
        });
        if deferred {
            window.request_animation_frame();
        }
    }
    fn clear_ink_layers(&mut self, window: &mut Window) {
        for (_, layer) in self.ink_layers.drain().chain(self.ink_stale.drain()) {
            let _ = window.drop_image(layer.image);
        }
        self.ink_desired.clear();
        self.ink_plan = None;
        self.ink_planner = super::ink_layers::Planner::default();
    }
    fn prepare_ink_layers(
        &mut self,
        batches: &[(std::ops::Range<usize>, super::ink_layers::InkBatch)],
        changed: bool,
        page: Id,
        window: &mut Window,
        cx: &mut Context<NotesView>,
    ) {
        if changed {
            self.ink_desired.clear();
            let mut pixels = 0;
            for (_, batch) in batches {
                if pixels + batch.pixels <= super::ink_layers::PIXEL_BUDGET {
                    pixels += batch.pixels;
                    self.ink_desired.insert(batch.id(), batch.clone());
                }
            }
            // A layer made for another density keeps standing in for its
            // batch until the new image is ready.
            // Layers that scrolled out of view are kept too, within half the
            // budget, so zooming back out shows them at once.
            let desired = &self.ink_desired;
            let stale = &mut self.ink_stale;
            for (id, layer) in std::mem::take(&mut self.ink_layers) {
                if desired
                    .get(&id)
                    .is_some_and(|batch| batch.matches(&layer.batch))
                {
                    self.ink_layers.insert(id, layer);
                } else if let Some(previous) = stale.insert(id, layer) {
                    let _ = window.drop_image(previous.image);
                }
            }
            let mut offscreen = 0;
            stale.retain(|id, layer| {
                let keep = match desired.get(id) {
                    Some(batch) => {
                        (batch.same_ink(&layer.batch) && batch.bounds == layer.batch.bounds)
                            || layer.batch.is_prefix_of(batch)
                    }
                    None => {
                        offscreen += layer.batch.pixels;
                        offscreen <= super::ink_layers::PIXEL_BUDGET / 4
                    }
                };
                if !keep {
                    let _ = window.drop_image(layer.image.clone());
                }
                keep
            });
            self.ink_failed.retain(|id, failed| {
                self.ink_desired
                    .get(id)
                    .is_some_and(|batch| batch.matches(failed))
            });
        }
        // Rasterize visible batches in parallel, leaving cores for input,
        // recognition and other background work.
        let parallel = std::thread::available_parallelism()
            .map_or(2, |n| n.get() / 2)
            .clamp(2, 4);
        for (_, batch) in batches {
            let id = batch.id();
            if self.ink_pending.len() >= parallel {
                break;
            }
            if !self.ink_desired.contains_key(&id)
                || self.ink_layers.contains_key(&id)
                || self.ink_pending.contains_key(&id)
                || self.ink_failed.contains_key(&id)
            {
                continue;
            }
            self.ink_pending.insert(id, batch.clone());
            let request = batch.clone();
            let work = request.clone();
            let task = cx
                .background_executor()
                .spawn(async move { super::ink_layers::raster(&work) });
            cx.spawn(async move |view, cx| {
                let image = task.await;
                let _ = view.update(cx, |view, cx| {
                    let Some(painter) = view.painter.pages.get_mut(&page) else {
                        return;
                    };
                    if !painter
                        .ink_pending
                        .get(&id)
                        .is_some_and(|batch| batch.matches(&request))
                    {
                        return;
                    }
                    painter.ink_pending.remove(&id);
                    if painter
                        .ink_desired
                        .get(&id)
                        .is_some_and(|batch| batch.matches(&request))
                    {
                        if let Some(image) = image {
                            for object in &request.objects {
                                painter.paths.remove(&object.id());
                            }
                            if painter.paths.capacity() > painter.paths.len().saturating_mul(2) + 64
                            {
                                painter.paths.shrink_to_fit();
                            }
                            if let Some(stale) = painter.ink_stale.remove(&id) {
                                cx.drop_image(stale.image, None);
                            }
                            painter.ink_layers.insert(
                                id,
                                CachedInkLayer {
                                    batch: request,
                                    image,
                                },
                            );
                        } else {
                            painter.ink_failed.insert(id, request);
                        }
                    } else if let Some(image) = image
                        && painter
                            .ink_desired
                            .get(&id)
                            .is_some_and(|batch| request.is_prefix_of(batch))
                        && painter
                            .ink_stale
                            .get(&id)
                            .is_none_or(|existing| existing.batch.objects.len() < request.objects.len())
                    {
                        for object in &request.objects {
                            painter.paths.remove(&object.id());
                        }
                        if let Some(previous) = painter.ink_stale.insert(
                            id,
                            CachedInkLayer {
                                batch: request,
                                image,
                            },
                        ) {
                            cx.drop_image(previous.image, None);
                        }
                    }
                    if painter.ink_pending.is_empty() {
                        view.controller.request_memory_trim();
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }
    fn request_raster(
        &mut self,
        id: Id,
        page: Id,
        preview: &folio_app::RasterPreview,
        appearance: Option<RasterAppearance>,
        cx: &mut Context<NotesView>,
    ) {
        // Pixel adaptation and copies can involve millions of pixels. Bound the
        // in-flight work and perform both away from pen dispatch and painting.
        if self.raster_pending.contains_key(&id) || self.raster_pending.len() >= 2 {
            return;
        }
        let request = RasterRequest {
            page,
            token: preview.token.clone(),
            appearance,
        };
        self.raster_pending.insert(id, request.clone());
        let (width, height) = (preview.width, preview.height);
        let data = preview.bgra.clone();
        let object = preview.object.clone();
        let (source, bounds) = (preview.object.clone(), preview.bounds);
        let task = cx.background_executor().spawn(async move {
            if let Some(RasterAppearance::Graph(palette)) = appearance
                && let Object::Equation(equation) = object.as_ref()
                && let Some(svg) = &equation.rendered_svg
            {
                let mut equation = equation.clone();
                equation.rendered_svg = Some(palette.svg(svg));
                return folio_export::raster_object_limited(
                    &Object::Equation(equation),
                    std::path::Path::new("."),
                    width.saturating_mul(height),
                )
                .ok()
                .map(|(_, pixmap)| graph::image(pixmap.width(), pixmap.height(), pixmap.take()));
            }
            preview_image(width, height, data, appearance)
        });
        cx.spawn(async move |view, cx| {
            let image = task.await;
            let _ = view.update(cx, |view, cx| {
                let Some(painter) = view.painter.pages.get_mut(&page) else {
                    return;
                };
                if !painter
                    .raster_pending
                    .get(&id)
                    .is_some_and(|pending| pending.matches(&request))
                {
                    return;
                }
                painter.raster_pending.remove(&id);
                let current = view
                    .controller
                    .previews
                    .get(&(view.controller.active, page, id))
                    .is_some_and(|preview| Arc::ptr_eq(&preview.token, &request.token));
                if painter.page == Some(page)
                    && current
                    && request
                        .appearance
                        .is_none_or(|appearance| match appearance {
                            RasterAppearance::Ink(theme) => painter.appearance == Some(theme),
                            RasterAppearance::Graph(palette) => {
                                painter.graph_palette == Some(palette)
                            }
                        })
                    && let Some(image) = image
                {
                    let token = request.token;
                    if let Some(previous) = painter.rasters.insert(
                        id,
                        CachedRaster {
                            token: token.clone(),
                            image,
                            appearance,
                            object: source,
                            bounds,
                        },
                    ) {
                        // This entity update runs outside a window update, so
                        // all windows are available through the app context.
                        cx.drop_image(previous.image, None);
                    }
                    // Plain image rasters never need theme re-adaptation; free
                    // their CPU BGRA buffer once the GPU image is ready.
                    if appearance.is_none() {
                        view.controller.release_preview_pixels(page, id, &token);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    #[allow(clippy::too_many_arguments)]
    fn paper(
        &mut self,
        properties: &PageProperties,
        visible: Rect,
        world: Transform,
        backgrounds: &mut HashMap<String, ImageSource>,
        dot_tile: &mut Option<CachedDotTile>,
        window: &mut Window,
        cx: &mut Context<NotesView>,
        controller: &Controller,
    ) {
        let theme = Theme::new(&controller.settings);
        let canvas_theme = theme.canvas_for_page(properties);
        let paper_color = canvas_theme.paper;
        let page = if properties.infinite {
            visible.expand(32.)
        } else {
            Rect::new(0., 0., properties.width, properties.height)
        };
        if let Some(bounds) = axis_aligned_bounds(page, world) {
            let scale = window.scale_factor();
            let mut paint_rect = |rect: Rect, color: Rgba| {
                visit_pixel_rectangles(rect, color, scale, |bounds, color| {
                    window.paint_quad(fill(gpui_bounds(bounds), color));
                });
            };
            if !properties.infinite {
                paint_rect(
                    Rect::new(
                        bounds.min.x + 2.,
                        bounds.min.y + 3.,
                        bounds.width(),
                        bounds.height(),
                    ),
                    alpha(0x000000, if theme.dark { 0.45 } else { 0.08 }),
                );
                // A hairline keeps the page edge visible against the
                // workspace: light on dark paper, a faint shade on light.
                paint_rect(
                    Rect::new(
                        bounds.min.x - 1.,
                        bounds.min.y - 1.,
                        bounds.width() + 2.,
                        bounds.height() + 2.,
                    ),
                    if theme.dark {
                        alpha(0xffffff, 0.2)
                    } else {
                        alpha(0x000000, 0.1)
                    },
                );
            }
            paint_rect(bounds, rgb(paper_color));
        } else {
            let points = rect_points(page).map(|p| world.apply(p));
            if !properties.infinite {
                let shadow = points.map(|p| DocPoint::new(p.x + 2., p.y + 3.));
                if let Some(path) = fill_path(&shadow) {
                    window.paint_path(path, alpha(0x000000, if theme.dark { 0.45 } else { 0.08 }));
                }
            }
            if let Some(path) = fill_path(&points) {
                window.paint_path(path, rgb(paper_color));
            }
            if theme.dark && !properties.infinite {
                let mut edge = points.to_vec();
                edge.push(points[0]);
                if let Some(path) = line_path(&edge, 1., false) {
                    window.paint_path(path, alpha(0xffffff, 0.2));
                }
            }
        }
        if let Some(pdf) = &properties.pdf {
            for asset in pdf
                .preview_asset
                .iter()
                .cloned()
                .chain(Controller::pdf_scroll_preview_asset(pdf))
            {
                let Some(source) = backgrounds.get(&asset) else {
                    continue;
                };
                if let Some(image) = source.render_image() {
                    let _ = window.paint_image_transformed(
                        gpui_bounds(page),
                        Default::default(),
                        image,
                        0,
                        false,
                        gpu_transform(world),
                    );
                    // Keep the quick image visible while the sharper image is
                    // still decoding. A file existing does not mean it is ready
                    // for the GPU yet.
                    break;
                }
            }
            return;
        }
        let scale = window.scale_factor();
        if properties.paper == Paper::Dots {
            if !page.intersects(visible) {
                return;
            }
            // Infinite pages (including rotated) are clipped by the outer canvas
            // mask; axis-aligned finite pages clip cleanly to the page sheet.
            // Only rotated finite pages fall back to vector dot paths below.
            let tile_mask = if properties.infinite {
                Some(None)
            } else {
                axis_aligned_bounds(page, world).map(|b| Some(ContentMask { bounds: gpui_bounds(b) }))
            };
            let zoom = world.scale();
            let render_zoom = if self.layer_zoom > 0. {
                self.layer_zoom
            } else {
                zoom
            };
            if let Some(page_mask) = tile_mask
                && zoom.is_finite()
                && zoom > 0.
                && let Some((tile_key, tile_doc)) =
                    dot_tile_spec(paper_spacing(zoom), render_zoom, scale, canvas_theme.dots)
            {
                let spacing = paper_spacing(zoom);
                let clip = paper_clip(page, visible);
                if let Some(tiles) = dot_tile_rects(clip, spacing, tile_doc) {
                    if dot_tile.as_ref().is_none_or(|t| t.key != tile_key) {
                        if let Some(old) = dot_tile.take() {
                            let _ = window.drop_image(old.image);
                        }
                        if let Some(image) = raster_dot_tile(tile_key) {
                            *dot_tile = Some(CachedDotTile {
                                key: tile_key,
                                image,
                            });
                        }
                    }
                    if let Some(cached) = dot_tile.as_ref() {
                        let image = cached.image.clone();
                        let matrix = gpu_transform(world);
                        window.with_content_mask(page_mask, |window| {
                            for rect in tiles {
                                let _ = window.paint_image_transformed(
                                    gpui_bounds(rect),
                                    Default::default(),
                                    image.clone(),
                                    0,
                                    false,
                                    matrix,
                                );
                            }
                        });
                        return;
                    }
                }
            }
        }
        let key = PaperKey {
            paper: properties.paper,
            page,
            visible,
            world,
            canvas: canvas_theme,
        };
        let needs_rebuild = self
            .paper_cache
            .as_ref()
            .is_none_or(|cached| cached.key != key || cached.scale != scale);
        if needs_rebuild {
            let geometry = paper_rectangles(&key, scale)
                .map(PaperGeometry::Rectangles)
                .unwrap_or_else(|| PaperGeometry::Paths(paper_paths(&key)));
            self.paper_cache = Some(CachedPaper {
                key,
                scale,
                geometry,
            });
        }
        let Some(cache) = &self.paper_cache else {
            return;
        };
        match &cache.geometry {
            PaperGeometry::Rectangles(rectangles) => {
                for (bounds, color) in rectangles {
                    window.paint_quad(fill(gpui_bounds(*bounds), *color));
                }
            }
            PaperGeometry::Paths(paths) => {
                for (path, color) in paths {
                    window.paint_path(path.clone(), *color);
                }
            }
        }
    }
}

fn preview_image(
    width: u32,
    height: u32,
    data: Arc<Vec<u8>>,
    appearance: Option<RasterAppearance>,
) -> Option<Arc<RenderImage>> {
    match appearance {
        None => RenderImage::from_ephemeral_shared_bgra(width, height, data.as_slice()).map(Arc::new),
        Some(RasterAppearance::Ink(theme)) if theme.adapt_ink => {
            match theme.preview_pixels(&data) {
                std::borrow::Cow::Borrowed(_) => {
                    RenderImage::from_shared_bgra(width, height, data.as_slice()).map(Arc::new)
                }
                std::borrow::Cow::Owned(pixels) => {
                    RenderImage::from_ephemeral_bgra(width, height, pixels).map(Arc::new)
                }
            }
        }
        _ => RenderImage::from_shared_bgra(width, height, data.as_slice()).map(Arc::new),
    }
}

fn paper_spacing(zoom: f32) -> f32 {
    // Keep at least 16 logical pixels between marks. Power-of-two steps
    // preserve the document lattice while bounding work by screen area.
    32. * (16. / (32. * zoom)).log2().ceil().max(0.).exp2()
}

fn paper_dot_centers(clip: Rect, spacing: f32) -> impl Iterator<Item = DocPoint> {
    let x0 = (clip.min.x / spacing).floor() as i32;
    let x1 = (clip.max.x / spacing).ceil() as i32;
    let y0 = (clip.min.y / spacing).floor() as i32;
    let y1 = (clip.max.y / spacing).ceil() as i32;
    (x0..=x1).flat_map(move |x| {
        (y0..=y1).filter_map(move |y| {
            let p = DocPoint::new(x as f32 * spacing + 16., y as f32 * spacing + 16.);
            clip.contains(p).then_some(p)
        })
    })
}

/// Choose a repeating dot-grid tile sized ~256–512 logical px so panning and
/// scrolling reuse one small GPU texture instead of rebuilding thousands of
/// triangle-fan path vertices and running a fullscreen 4× MSAA path pass.
fn dot_tile_spec(spacing: f32, render_zoom: f32, scale: f32, color: u32) -> Option<(DotTileKey, f32)> {
    if !spacing.is_finite()
        || spacing <= 0.
        || !render_zoom.is_finite()
        || render_zoom <= 0.
        || !scale.is_finite()
        || scale <= 0.
    {
        return None;
    }
    let cell_logical = spacing * render_zoom;
    let cells: u32 = if cell_logical <= 24. {
        16
    } else if cell_logical <= 64. {
        8
    } else if cell_logical <= 128. {
        4
    } else {
        2
    };
    let tile_doc = spacing * cells as f32;
    let exact_tile_px = tile_doc * render_zoom * scale;
    let tile_px = exact_tile_px.round().clamp(32., 1024.) as u32;
    let dot_radius_px = render_zoom.max(0.7) * scale * (tile_px as f32 / exact_tile_px.max(1.));
    let max_radius = (tile_px as f32 / cells as f32) * 0.45;
    let radius_tenths = (dot_radius_px.min(max_radius) * 10.).round().max(5.) as u32;
    Some((
        DotTileKey {
            cells,
            tile_px,
            radius_tenths,
            color,
        },
        tile_doc,
    ))
}

/// Rasterize one repeating `cells × cells` dot tile in straight BGRA. Every
/// pixel carries the dot RGB so GPU bilinear sampling interpolates only alpha
/// without dark fringes on light or dark paper.
fn raster_dot_tile(key: DotTileKey) -> Option<Arc<RenderImage>> {
    let side = key.tile_px as usize;
    let r = ((key.color >> 16) & 0xff) as u8;
    let g = ((key.color >> 8) & 0xff) as u8;
    let b = (key.color & 0xff) as u8;
    let mut bgra = vec![0u8; side * side * 4];
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel[0] = b;
        pixel[1] = g;
        pixel[2] = r;
    }
    let cell_px = key.tile_px as f32 / key.cells as f32;
    let radius = key.radius_tenths as f32 / 10.;
    let pad = (radius + 1.).ceil() as i32;
    let max_coord = key.tile_px as i32 - 1;
    for cy in 0..key.cells {
        let cy_px = (cy as f32 + 0.5) * cell_px;
        let y0 = (cy_px.floor() as i32 - pad).max(0);
        let y1 = (cy_px.ceil() as i32 + pad).min(max_coord);
        for cx in 0..key.cells {
            let cx_px = (cx as f32 + 0.5) * cell_px;
            let x0 = (cx_px.floor() as i32 - pad).max(0);
            let x1 = (cx_px.ceil() as i32 + pad).min(max_coord);
            for iy in y0..=y1 {
                let dy = iy as f32 + 0.5 - cy_px;
                let row = iy as usize * side;
                for ix in x0..=x1 {
                    let dx = ix as f32 + 0.5 - cx_px;
                    let cov = (radius + 0.5 - dx.hypot(dy)).clamp(0., 1.);
                    let a = (cov * 255.).round() as u8;
                    if a > 0 {
                        let idx = (row + ix as usize) * 4 + 3;
                        bgra[idx] = bgra[idx].max(a);
                    }
                }
            }
        }
    }
    RenderImage::from_ephemeral_bgra(key.tile_px, key.tile_px, bgra).map(Arc::new)
}

/// Document-space tile rectangles covering `clip` such that every cell center
/// aligns identically with [`paper_dot_centers`] (`x * spacing + 16`).
fn dot_tile_rects(clip: Rect, spacing: f32, tile_doc: f32) -> Option<Vec<Rect>> {
    if clip.width() <= 0. || clip.height() <= 0. || !tile_doc.is_finite() || tile_doc <= 0. {
        return None;
    }
    let base = 16. - 0.5 * spacing;
    let tx0 = ((clip.min.x - base) / tile_doc).floor() as i32;
    let tx1 = ((clip.max.x - base) / tile_doc).ceil() as i32;
    let ty0 = ((clip.min.y - base) / tile_doc).floor() as i32;
    let ty1 = ((clip.max.y - base) / tile_doc).ceil() as i32;
    let cols = tx1.saturating_sub(tx0) as usize;
    let rows = ty1.saturating_sub(ty0) as usize;
    if cols == 0 || rows == 0 || cols.saturating_mul(rows) > 4096 {
        return None;
    }
    let mut rects = Vec::with_capacity(cols * rows);
    for ty in ty0..ty1 {
        let y = base + ty as f32 * tile_doc;
        for tx in tx0..tx1 {
            let x = base + tx as f32 * tile_doc;
            rects.push(Rect::new(x, y, tile_doc, tile_doc));
        }
    }
    Some(rects)
}

fn axis_aligned_bounds(rect: Rect, world: Transform) -> Option<Rect> {
    ([world.a, world.b, world.c, world.d, world.tx, world.ty]
        .iter()
        .all(|v| v.is_finite())
        && world.b == 0.
        && world.c == 0.
        && world.a > 0.
        && world.d > 0.)
        .then(|| Rect::from_points([world.apply(rect.min), world.apply(rect.max)]))
}

fn pixel_bands(min: f32, max: f32, scale: f32) -> smallvec::SmallVec<[(f32, f32, f32); 3]> {
    let mut bands = smallvec::SmallVec::new();
    let (min, max) = (min * scale, max * scale);
    if !min.is_finite() || !max.is_finite() || max <= min || !scale.is_finite() || scale <= 0. {
        return bands;
    }
    let (start, end) = (min.floor(), max.ceil());
    if end - start <= 1. {
        bands.push((start / scale, end / scale, max - min));
    } else {
        let (inner_start, inner_end) = (min.ceil(), max.floor());
        if min < inner_start {
            bands.push((start / scale, inner_start / scale, inner_start - min));
        }
        if inner_end > inner_start {
            bands.push((inner_start / scale, inner_end / scale, 1.));
        }
        if max > inner_end {
            bands.push((inner_end / scale, end / scale, max - inner_end));
        }
    }
    bands
}

fn visit_pixel_rectangles(
    rect: Rect,
    color: Rgba,
    scale: f32,
    mut emit: impl FnMut(Rect, Rgba),
) {
    // Quads have hard rectangular edges. Split fractional device pixels into
    // weighted edge strips so thin ruling retains its coverage during resizing.
    for (x0, x1, x_alpha) in pixel_bands(rect.min.x, rect.max.x, scale) {
        for (y0, y1, y_alpha) in pixel_bands(rect.min.y, rect.max.y, scale) {
            let mut color = color;
            color.a *= x_alpha * y_alpha;
            emit(Rect::new(x0, y0, x1 - x0, y1 - y0), color);
        }
    }
}

fn pixel_rectangles(rect: Rect, color: Rgba, scale: f32, rectangles: &mut Vec<(Rect, Rgba)>) {
    visit_pixel_rectangles(rect, color, scale, |bounds, color| {
        rectangles.push((bounds, color));
    });
}

fn paper_clip(page: Rect, visible: Rect) -> Rect {
    Rect::new(
        visible.min.x.max(page.min.x),
        visible.min.y.max(page.min.y),
        (visible.max.x.min(page.max.x) - visible.min.x.max(page.min.x)).max(0.),
        (visible.max.y.min(page.max.y) - visible.min.y.max(page.min.y)).max(0.),
    )
}

fn paper_lines(paper: Paper, page: Rect, clip: Rect, spacing: f32) -> Vec<[DocPoint; 2]> {
    let x0 = (clip.min.x / spacing).floor() as i32;
    let x1 = (clip.max.x / spacing).ceil() as i32;
    let y0 = (clip.min.y / spacing).floor() as i32;
    let y1 = (clip.max.y / spacing).ceil() as i32;
    let mut lines = Vec::new();
    if matches!(paper, Paper::Ruled | Paper::Grid) {
        for y in y0..=y1 {
            let y = y as f32 * spacing;
            if y >= page.min.y && y <= page.max.y {
                lines.push([DocPoint::new(clip.min.x, y), DocPoint::new(clip.max.x, y)]);
            }
        }
    }
    if paper == Paper::Grid {
        for x in x0..=x1 {
            let x = x as f32 * spacing;
            if x >= page.min.x && x <= page.max.x {
                lines.push([DocPoint::new(x, clip.min.y), DocPoint::new(x, clip.max.y)]);
            }
        }
    }
    lines
}

fn paper_rectangles(key: &PaperKey, scale: f32) -> Option<Vec<(Rect, Rgba)>> {
    axis_aligned_bounds(key.page, key.world)?;
    if key.paper == Paper::Dots {
        return None;
    }
    let mut rectangles = Vec::new();
    if key.paper == Paper::Blank || !key.page.intersects(key.visible) {
        return Some(rectangles);
    }
    let clip = paper_clip(key.page, key.visible);
    for [a, b] in paper_lines(key.paper, key.page, clip, paper_spacing(key.world.scale())) {
        let (a, b) = (key.world.apply(a), key.world.apply(b));
        let rect = if a.y == b.y {
            Rect::new(a.x, a.y - 0.5, b.x - a.x, 1.)
        } else {
            Rect::new(a.x - 0.5, a.y, 1., b.y - a.y)
        };
        pixel_rectangles(rect, rgb(key.canvas.grid), scale, &mut rectangles);
    }
    Some(rectangles)
}

fn paper_paths(key: &PaperKey) -> Vec<(Path<Pixels>, Rgba)> {
    let PaperKey {
        page,
        visible,
        world,
        ..
    } = *key;
    // A paper completely outside the viewport needs no grid tessellation.
    if key.paper == Paper::Blank || !page.intersects(visible) {
        return Vec::new();
    }
    let zoom = world.scale();
    if !zoom.is_finite() || zoom <= 0. {
        return Vec::new();
    }
    let spacing = paper_spacing(zoom);
    let clip = paper_clip(page, visible);
    let lines = paper_lines(key.paper, page, clip, spacing);
    let mut paths = Vec::new();
    if !lines.is_empty() {
        let mut builder = PathBuilder::stroke(px(1.));
        for line in lines {
            builder.move_to(point_px(world.apply(line[0])));
            builder.line_to(point_px(world.apply(line[1])));
        }
        if let Ok(path) = builder.build() {
            paths.push((path, rgb(key.canvas.grid)));
        }
    }
    if key.paper == Paper::Dots {
        let mut path = None;
        // Dot geometry is identical across the grid. Compute the offsets once
        // rather than allocating and evaluating trig for every visible dot.
        let offsets = folio_ink::circle(DocPoint::new(0., 0.), zoom.max(0.7), 8);
        for p in paper_dot_centers(clip, spacing) {
            let center = world.apply(p);
            let translated = |offset: &DocPoint| {
                point_px(DocPoint::new(center.x + offset.x, center.y + offset.y))
            };
            let first = translated(&offsets[0]);
            let path = path.get_or_insert_with(|| Path::new(first));
            // Each dot is convex and independent. A triangle fan avoids
            // tessellating thousands of disconnected contours together.
            for pair in offsets[1..].windows(2) {
                path.push_triangle(
                    (first, translated(&pair[0]), translated(&pair[1])),
                    (point(0., 1.), point(0., 1.), point(0., 1.)),
                );
            }
        }
        if let Some(path) = path {
            paths.push((path, rgb(key.canvas.dots)));
        }
    }
    paths
}

fn gpu_transform(t: Transform) -> TransformationMatrix {
    TransformationMatrix {
        rotation_scale: [[t.a, t.c], [t.b, t.d]],
        translation: [t.tx, t.ty],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn visible_scene_reuses_resize_steps_without_missing_content_or_retaining_large_views() {
        let mut page = Page::new();
        for x in [-160., -60., 50., 790., 850., 1100.] {
            let object = Arc::new(Object::Image(folio_document::ImageObject {
                id: Id::new_v4(),
                asset: "fixture.png".into(),
                rect: Rect::new(x, 70., 10., 10.),
                transform: Transform::default(),
                crop: None,
            }));
            page.order.push(object.id());
            page.objects.insert(object.id(), object);
        }
        let positions = page
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        let mut index = SpatialIndex::default();
        index.rebuild(&page);
        let mut cached = None;
        let visible = Rect::new(0., 0., 800., 500.);
        assert!(prepare_visible_frame(
            &mut cached,
            &page,
            &index,
            &positions,
            visible,
            1.
        ));
        let first = cached.as_ref().unwrap().clone();
        assert_eq!(
            first.objects.iter().map(|o| o.id()).collect::<Vec<_>>(),
            page.order[1..5]
        );
        assert!(!prepare_visible_frame(
            &mut cached,
            &page,
            &index,
            &positions,
            Rect::new(-20., 0., 840., 500.),
            1.
        ));
        assert!(Arc::ptr_eq(&first, cached.as_ref().unwrap()));

        // New right-edge content must appear before it becomes visible.
        assert!(prepare_visible_frame(
            &mut cached,
            &page,
            &index,
            &positions,
            Rect::new(-200., 0., 1400., 500.),
            1.
        ));
        assert!(cached.as_ref().unwrap().ids.contains(&page.order[5]));
        assert!(prepare_visible_frame(
            &mut cached,
            &page,
            &index,
            &positions,
            Rect::new(200., 0., 100., 200.),
            1.
        ));
        assert!(
            cached.as_ref().unwrap().ids.is_empty(),
            "A large shrink releases distant objects"
        );
        prepare_visible_frame(&mut cached, &page, &index, &positions, visible, 1.);
        let id = page.order[2];
        let mut edited = page.objects[&id].as_ref().clone();
        if let Object::Image(image) = &mut edited {
            image.asset = "edited.png".into();
        }
        page.objects.insert(id, Arc::new(edited));
        page.revision += 1;
        assert!(prepare_visible_frame(
            &mut cached,
            &page,
            &index,
            &positions,
            visible,
            1.
        ));
        assert!(
            cached
                .as_ref()
                .unwrap()
                .objects
                .iter()
                .any(|o| Arc::ptr_eq(o, &page.objects[&id]))
        );
        // Precise preview bounds can change without changing page revision.
        index.insert(page.order[5], Rect::new(100., 70., 10., 10.));
        assert!(prepare_visible_frame(
            &mut cached,
            &page,
            &index,
            &positions,
            visible,
            1.
        ));
        assert!(cached.as_ref().unwrap().ids.contains(&page.order[5]));
        page.id = Id::new_v4();
        assert!(prepare_visible_frame(
            &mut cached,
            &page,
            &index,
            &positions,
            visible,
            1.
        ));

        for zoom in [0.1, 0.5, 1., 4.] {
            for rotation in [0., 0.3, -1.2] {
                for width in (800..1600).step_by(7).chain((800..1600).rev().step_by(7)) {
                    let viewport = Viewport {
                        zoom,
                        rotation,
                        pan: DocPoint::new(width as f32 / 2., 30.),
                    };
                    let visible = viewport.visible(width as f32, 600.);
                    prepare_visible_frame(&mut cached, &page, &index, &positions, visible, zoom);
                    let frame = cached.as_ref().unwrap();
                    assert!(index.query(visible).is_subset(&frame.ids));
                    assert!(
                        frame.coverage.contains(visible.min)
                            && frame.coverage.contains(visible.max)
                    );
                    let limit = visible.expand(128. / zoom);
                    assert!(
                        limit.contains(frame.coverage.min) && limit.contains(frame.coverage.max)
                    );
                    assert_eq!(
                        frame.objects.iter().map(|o| o.id()).collect::<Vec<_>>(),
                        page.order
                            .iter()
                            .filter(|id| frame.ids.contains(id))
                            .copied()
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
    #[::core::prelude::v1::test]
    fn raster_cache_releases_offscreen_stale_and_cleared_images() {
        let mut painter = Painter::default();
        let visible = Id::new_v4();
        let stale = Id::new_v4();
        let offscreen = Id::new_v4();
        let mut atlas = HashSet::new();
        let mut images = Vec::new();
        let token = Arc::new(());
        for id in [visible, stale, offscreen] {
            let image = graph::image(1, 1, vec![0; 4]);
            images.push(Arc::downgrade(&image));
            atlas.insert(image.id);
            painter.rasters.insert(
                id,
                CachedRaster {
                    token: if id == stale {
                        Arc::new(())
                    } else {
                        token.clone()
                    },
                    image,
                    appearance: None,
                    object: Arc::new(Object::Image(folio_document::ImageObject {
                        id,
                        asset: "image.png".into(),
                        rect: Rect::new(0., 0., 1., 1.),
                        transform: Transform::default(),
                        crop: None,
                    })),
                    bounds: Rect::default(),
                },
            );
        }
        painter.retain_rasters(
            |id, raster| id != offscreen && Arc::ptr_eq(&raster.token, &token),
            |image| assert!(atlas.remove(&image.id)),
        );
        assert_eq!(
            painter.rasters.keys().copied().collect::<HashSet<_>>(),
            HashSet::from([visible])
        );
        assert!(images[0].upgrade().is_some());
        assert!(images[1].upgrade().is_none());
        assert!(images[2].upgrade().is_none());
        assert_eq!(atlas.len(), 1);
        // Repainting a retained image does not retire it. Clearing the page
        // releases the final allocation and does not double-release anything.
        painter.retain_rasters(|_, _| true, |_| panic!("Visible raster retired"));
        painter.retain_rasters(|_, _| false, |image| assert!(atlas.remove(&image.id)));
        painter.retain_rasters(|_, _| false, |_| panic!("Raster retired twice"));
        assert!(images[0].upgrade().is_none());
        assert!(atlas.is_empty());
    }
    #[::core::prelude::v1::test]
    #[ignore = "manual device-free scene preparation benchmark; not GPU frame latency"]
    fn benchmark_dense_saved_ink_scene_preparation() {
        let theme = Theme::new(&folio_app::Settings::default()).canvas;
        for count in [256, 1024, 4096] {
            let objects = (0..count)
                .map(|i| {
                    let mut builder =
                        folio_ink::StrokeBuilder::new(folio_document::PenStyle::default());
                    for j in 0..48 {
                        builder.push(folio_document::StrokePoint::new(
                            DocPoint::new(
                                (i % 32) as f32 * 20. + j as f32 * 0.25,
                                (i / 32) as f32 * 10. + (j as f32 * 0.2).sin() * 3.,
                            ),
                            0.7,
                            j * 8,
                        ));
                    }
                    Arc::new(Object::Stroke(builder.finish().unwrap()))
                })
                .collect::<Vec<_>>();
            let paths = objects
                .iter()
                .map(|o| {
                    let Object::Stroke(s) = o.as_ref() else {
                        unreachable!()
                    };
                    ink_path(&folio_ink::outline(&s.display_path())).unwrap()
                })
                .collect::<Vec<_>>();
            let batches = super::super::ink_layers::batches(&objects, &HashSet::new(), theme, 1.);
            let world = Transform::translate(30., 50.);
            let frames = 100;
            let start = std::time::Instant::now();
            for _ in 0..frames {
                for path in &paths {
                    std::hint::black_box(path.clone().transformed(coefficients(world)).scale(1.));
                }
            }
            let before = start.elapsed();
            let start = std::time::Instant::now();
            for _ in 0..frames {
                for (_, batch) in &batches {
                    std::hint::black_box((gpui_bounds(batch.bounds), gpu_transform(world)));
                }
            }
            let after = start.elapsed();
            println!(
                "CPU scene geometry only: strokes={count}, cached_layers={}, frames={frames}, vector_ms={:.3}, cached_ms={:.3}",
                batches.len(),
                before.as_secs_f64() * 1000.,
                after.as_secs_f64() * 1000.
            );
            assert_eq!(
                batches.iter().map(|(range, _)| range.len()).sum::<usize>(),
                count
            );
            let mut page = Page::new();
            for object in &objects {
                page.order.push(object.id());
                page.objects.insert(object.id(), object.clone());
            }
            let mut index = SpatialIndex::default();
            index.rebuild(&page);
            let positions = page
                .order
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, i))
                .collect();
            for reuse in [false, true] {
                let mut cached = None;
                let mut planner = super::super::ink_layers::Planner::default();
                let mut rebuilds = 0;
                let start = std::time::Instant::now();
                for width in (800..1200).chain((800..1200).rev()) {
                    if !reuse {
                        cached = None;
                    }
                    let viewport = Viewport {
                        pan: DocPoint::new(width as f32 / 2. - 400., 30.),
                        ..Default::default()
                    };
                    if prepare_visible_frame(
                        &mut cached,
                        &page,
                        &index,
                        &positions,
                        viewport.visible(width as f32, 900.),
                        1.,
                    ) {
                        rebuilds += 1;
                        let frame = cached.as_ref().unwrap();
                        std::hint::black_box(planner.batches(
                            &page,
                            &frame.objects,
                            &frame.ranks,
                            &HashSet::new(),
                            theme,
                            1.,
                            None,
                        ));
                    }
                    std::hint::black_box(cached.as_ref().unwrap());
                }
                println!(
                    "CPU resize scene preparation: strokes={count}, reuse={reuse}, frames=800, rebuilds={rebuilds}, ms={:.3}",
                    start.elapsed().as_secs_f64() * 1000.
                );
                assert!(if reuse {
                    rebuilds < 20
                } else {
                    rebuilds == 800
                });
            }
        }
    }
    #[::core::prelude::v1::test]
    fn translucent_live_path_cache_invalidates_append_pressure_and_identity() {
        let id = Id::new_v4();
        let mut points = vec![PathPoint {
            position: DocPoint::new(10., 20.),
            radius: 2.,
        }];
        let mut cache = ActivePathCache::default();
        assert!(cache.path(id, &points).is_some());
        let first = cache.key;
        assert!(cache.path(id, &points).is_some());
        assert!(cache.key == first);
        points[0].radius = 3.;
        assert!(cache.path(id, &points).is_some());
        assert!(cache.key != first);
        let pressure = cache.key;
        points.push(PathPoint {
            position: DocPoint::new(20., 20.),
            radius: 3.,
        });
        assert!(cache.path(id, &points).is_some());
        assert!(cache.key != pressure);
        let appended = cache.key;
        assert!(cache.path(Id::new_v4(), &points).is_some());
        assert!(cache.key != appended);
        assert!(cache.path(id, &[]).is_none());
    }
    #[::core::prelude::v1::test]
    #[ignore = "manual device-free performance measurement"]
    fn benchmark_translucent_live_path_repaints() {
        let id = Id::new_v4();
        let points = (0..12000)
            .map(|i| PathPoint {
                position: DocPoint::new(i as f32 * 1.2, (i as f32 * 0.025).sin() * 25.),
                radius: 2. + (i as f32 * 0.015).sin() * 0.5,
            })
            .collect::<Vec<_>>();
        let world = Transform::translate(50., 60.);
        let frames = 100;
        let start = std::time::Instant::now();
        for _ in 0..frames {
            std::hint::black_box(
                ink_path(&folio_ink::outline(&points))
                    .unwrap()
                    .transformed(coefficients(world)),
            );
        }
        let before = start.elapsed();
        let mut cache = ActivePathCache::default();
        let start = std::time::Instant::now();
        for _ in 0..frames {
            std::hint::black_box(
                cache
                    .path(id, &points)
                    .unwrap()
                    .clone()
                    .transformed(coefficients(world)),
            );
        }
        let after = start.elapsed();
        println!(
            "12,000-point translucent unchanged repaint, {frames} frames: baseline={before:?}, cached={after:?}"
        );
    }
    #[::core::prelude::v1::test]
    fn binary_page_candidates_match_full_scan_across_pan_zoom_and_rotation() {
        let mut pages = (0..250)
            .map(|i| {
                let mut page = folio_document::Page::new();
                page.properties.infinite = false;
                page.properties.width = 400. + (i % 5) as f32 * 80.;
                page.properties.height = 400. + (i % 7) as f32 * 120.;
                page
            })
            .collect::<Vec<_>>();
        pages[10].properties.infinite = true;
        for active in [0, 9, 11, 100, 249] {
            let stack = PageStack::new(&pages, active).unwrap();
            for zoom in [0.1, 0.5, 1., 3.] {
                for rotation in [0., 0.3, -1.2, std::f32::consts::PI] {
                    for y in [-2500., 0., 400., 5000.] {
                        let viewport = Viewport {
                            zoom,
                            rotation,
                            pan: DocPoint::new(80., y),
                        };
                        let actual = visible_page_range(&stack, viewport, active, 900., 700.);
                        let expected = stack
                            .pages
                            .iter()
                            .enumerate()
                            .filter_map(|(position, (index, rect))| {
                                let local = stack.viewport(viewport, active, *index);
                                local
                                    .visible(900., 700.)
                                    .intersects(Rect::new(0., 0., rect.width(), rect.height()))
                                    .then_some(position)
                            })
                            .collect::<Vec<_>>();
                        assert!(
                            expected.iter().all(|position| actual.contains(position)),
                            "missed visible page"
                        );
                        let filtered = stack.pages[actual]
                            .iter()
                            .filter_map(|(index, rect)| {
                                let local = stack.viewport(viewport, active, *index);
                                local
                                    .visible(900., 700.)
                                    .intersects(Rect::new(0., 0., rect.width(), rect.height()))
                                    .then_some(*index)
                            })
                            .collect::<Vec<_>>();
                        let expected = expected
                            .into_iter()
                            .map(|position| stack.pages[position].0)
                            .collect::<Vec<_>>();
                        assert_eq!(filtered, expected);
                    }
                }
            }
        }
    }
    #[::core::prelude::v1::test]
    fn selection_rotation_handle_tracks_drag_without_a_release_jump_at_any_zoom() {
        let bounds = Rect::new(100., 150., 90., 40.);
        for zoom in [0.5, 1., 4.] {
            let world = Transform::translate(37., -21.).compose(Transform::around(
                DocPoint::new(0., 0.),
                zoom,
                0.,
            ));
            let delta = DocPoint::new(65., 35.);
            let (original, original_rotation) = selection_overlay(bounds, zoom, world);
            let (dragged, dragged_rotation) = selection_overlay(
                bounds,
                zoom,
                world.compose(Transform::translate(delta.x, delta.y)),
            );
            for (before, after) in original
                .into_iter()
                .chain(original_rotation)
                .zip(dragged.into_iter().chain(dragged_rotation))
            {
                assert!(
                    after.distance(DocPoint::new(
                        before.x + delta.x * zoom,
                        before.y + delta.y * zoom
                    )) < 0.001
                );
            }
            let (released, released_rotation) = selection_overlay(
                Rect::new(
                    bounds.min.x + delta.x,
                    bounds.min.y + delta.y,
                    bounds.width(),
                    bounds.height(),
                ),
                zoom,
                world,
            );
            assert_eq!(dragged, released);
            assert_eq!(dragged_rotation, released_rotation);
            // The stem runs from the padded outline to the knob 24 px above.
            assert!(
                (dragged_rotation[0].distance(dragged_rotation[1])
                    - (24. - folio_app::SELECTION_PADDING))
                    .abs()
                    < 0.001
            );
        }
    }
    #[::core::prelude::v1::test]
    fn paper_patterns_preserve_lattice_thickness_and_rotated_path_fallback() {
        let mut key = PaperKey {
            paper: Paper::Blank,
            page: Rect::new(0., 0., 1000., 1000.),
            visible: Rect::new(0., 0., 900., 900.),
            world: Transform::default(),
            canvas: Theme::new(&folio_app::Settings::default()).canvas,
        };
        assert!(paper_paths(&key).is_empty());
        for paper in [Paper::Ruled, Paper::Grid, Paper::Dots] {
            key.paper = paper;
            assert_eq!(paper_paths(&key).len(), 1);
        }
        key.visible = Rect::new(2000., 2000., 900., 900.);
        assert!(paper_paths(&key).is_empty());

        // Weighted device-pixel edges preserve rectangular coverage, including
        // negative positions, subpixel widths and fractional display density.
        for scale in [0.75, 1., 1.25, 1.5, 2.] {
            for rect in [
                Rect::new(-0.25, 4.25, 2.5, 1.),
                Rect::new(1.2, -2.4, 0.1, 0.2),
                Rect::new(3., 4., 20., 30.),
            ] {
                let mut rectangles = Vec::new();
                let color = alpha(0x123456, 0.4);
                pixel_rectangles(rect, color, scale, &mut rectangles);
                let area: f32 = rectangles
                    .iter()
                    .map(|(r, c)| r.width() * r.height() * c.a)
                    .sum();
                assert!((area - rect.width() * rect.height() * color.a).abs() < 0.001);
                for (r, c) in &rectangles {
                    assert!(c.a > 0. && c.a <= color.a);
                    for edge in [r.min.x, r.min.y, r.max.x, r.max.y] {
                        assert!((edge * scale - (edge * scale).round()).abs() < 0.001);
                    }
                }
                for y in -5..40 {
                    for x in -3..30 {
                        let sample =
                            DocPoint::new((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
                        let actual: f32 = rectangles
                            .iter()
                            .filter(|(r, _)| r.contains(sample))
                            .map(|(_, c)| c.a)
                            .sum();
                        let x_coverage = ((rect.max.x * scale).min(x as f32 + 1.)
                            - (rect.min.x * scale).max(x as f32))
                        .clamp(0., 1.);
                        let y_coverage = ((rect.max.y * scale).min(y as f32 + 1.)
                            - (rect.min.y * scale).max(y as f32))
                        .clamp(0., 1.);
                        assert!((actual - color.a * x_coverage * y_coverage).abs() < 0.001);
                    }
                }
            }
        }

        // A fixed screen must not accumulate increasingly dense background
        // geometry as an infinite canvas exposes more document space.
        for rotation in [0., std::f32::consts::FRAC_PI_4] {
            for zoom in [1., 0.5, 0.3, 0.25, 0.125, 0.1] {
                let viewport = Viewport {
                    zoom,
                    rotation,
                    pan: DocPoint::new(317., -149.),
                };
                key.visible = viewport.visible(1920., 1080.);
                key.page = key.visible.expand(32.);
                key.world = viewport.transform();
                let spacing = paper_spacing(key.world.scale());
                let dots = paper_dot_centers(key.visible, spacing).collect::<Vec<_>>();
                let screen_width = key.visible.width() * zoom;
                let screen_height = key.visible.height() * zoom;
                let budget = ((screen_width / 16.).ceil() as usize + 1)
                    * ((screen_height / 16.).ceil() as usize + 1);
                assert!(
                    !dots.is_empty() && dots.len() <= budget,
                    "zoom={zoom}, rotation={rotation}: {} dots exceed {budget}",
                    dots.len()
                );
                for p in dots {
                    assert!(key.visible.contains(p));
                    // Coarser levels retain dots from the original lattice,
                    // including at negative canvas coordinates.
                    assert_eq!((p.x - 16.).rem_euclid(32.), 0.);
                    assert_eq!((p.y - 16.).rem_euclid(32.), 0.);
                }
                let (tile_key, tile_doc) =
                    dot_tile_spec(spacing, zoom, 1., key.canvas.dots).unwrap();
                let tiles = dot_tile_rects(key.visible, spacing, tile_doc).unwrap();
                assert!(!tiles.is_empty() && tiles.len() <= 256);
                for tile in &tiles {
                    // Every cell center inside every tile lands on the exact dot lattice.
                    let first_x = tile.min.x + 0.5 * spacing;
                    let first_y = tile.min.y + 0.5 * spacing;
                    assert!((first_x - 16.).rem_euclid(spacing).abs() < 1e-3);
                    assert!((first_y - 16.).rem_euclid(spacing).abs() < 1e-3);
                }
                if rotation == 0. && (zoom == 1. || zoom == 0.25) {
                    let image = raster_dot_tile(tile_key).unwrap();
                    let side = tile_key.tile_px as usize;
                    let cell = tile_key.tile_px as f32 / tile_key.cells as f32;
                    let cx = (0.5 * cell).floor() as usize;
                    let min_alpha = if zoom >= 1. { 180 } else { 100 };
                    image.with_bytes(0, |bytes| {
                        let bytes = bytes.unwrap();
                        assert!(bytes[(cx * side + cx) * 4 + 3] >= min_alpha);
                        assert_eq!(bytes[3], 0);
                    });
                }
                for paper in [Paper::Ruled, Paper::Grid, Paper::Dots] {
                    key.paper = paper;
                    assert_eq!(paper_paths(&key).len(), 1);
                    for scale in [1., 1.25, 2.] {
                        let rectangles = paper_rectangles(&key, scale);
                        if rotation != 0. || paper == Paper::Dots {
                            assert!(rectangles.is_none());
                        } else {
                            let rectangles = rectangles.unwrap();
                            let lines = paper_lines(
                                paper,
                                key.page,
                                paper_clip(key.page, key.visible),
                                spacing,
                            );
                            let expected_area: f32 =
                                lines.iter().map(|[a, b]| a.distance(*b) * zoom).sum();
                            let area: f32 = rectangles
                                .iter()
                                .map(|(r, c)| r.width() * r.height() * c.a)
                                .sum();
                            assert!((area - expected_area).abs() < expected_area * 0.001);
                            assert!(rectangles.len() <= lines.len() * 9);
                        }
                    }
                }
            }
        }
    }
    #[::core::prelude::v1::test]
    #[ignore = "manual paper tessellation benchmark; not GPU frame latency"]
    fn benchmark_infinite_paper_zoom_out() {
        let canvas = Theme::new(&folio_app::Settings::default()).canvas;
        for paper in [Paper::Grid, Paper::Dots] {
            for zoom in [1., 0.25, 0.1] {
                let viewport = Viewport {
                    zoom,
                    pan: DocPoint::new(317., -149.),
                    rotation: 0.,
                };
                let mut key = PaperKey {
                    paper,
                    page: viewport.visible(1920., 1080.).expand(32.),
                    visible: viewport.visible(1920., 1080.),
                    world: viewport.transform(),
                    canvas,
                };
                let frames = 10;
                let start = std::time::Instant::now();
                for _ in 0..frames {
                    std::hint::black_box(paper_paths(&key));
                    key.world.tx += 1.;
                    key.visible.min.x -= 1. / zoom;
                    key.visible.max.x -= 1. / zoom;
                    key.page = key.visible.expand(32.);
                }
                println!(
                    "CPU paper tessellation only: {paper:?}, zoom={zoom}, ms/frame={:.3}",
                    start.elapsed().as_secs_f64() * 1000. / frames as f64
                );
                if paper == Paper::Dots {
                    let spacing = paper_spacing(zoom);
                    let (tile_key, tile_doc) =
                        dot_tile_spec(spacing, zoom, 1., canvas.dots).unwrap();
                    let _tile = raster_dot_tile(tile_key).unwrap();
                    let start = std::time::Instant::now();
                    let mut tile_count = 0;
                    for _ in 0..frames {
                        let tiles = std::hint::black_box(
                            dot_tile_rects(key.visible, spacing, tile_doc).unwrap(),
                        );
                        tile_count = tiles.len();
                        key.world.tx += 1.;
                        key.visible.min.x -= 1. / zoom;
                        key.visible.max.x -= 1. / zoom;
                        key.page = key.visible.expand(32.);
                    }
                    println!(
                        "CPU dot-tile preparation (cached texture): zoom={zoom}, tiles={tile_count}, ms/frame={:.4}",
                        start.elapsed().as_secs_f64() * 1000. / frames as f64
                    );
                } else {
                    let start = std::time::Instant::now();
                    for _ in 0..frames {
                        std::hint::black_box(paper_rectangles(&key, 1.).unwrap());
                        key.world.tx += 1.;
                        key.visible.min.x -= 1. / zoom;
                        key.visible.max.x -= 1. / zoom;
                        key.page = key.visible.expand(32.);
                    }
                    println!(
                        "CPU paper rectangle preparation: {paper:?}, zoom={zoom}, ms/frame={:.3}",
                        start.elapsed().as_secs_f64() * 1000. / frames as f64
                    );
                }
            }
        }
    }
    #[::core::prelude::v1::test]
    fn raster_jobs_are_invalidated_by_pixels_page_and_theme() {
        // Previews keep their pixels; only appearance conversion changes them.
        let data = Arc::new(vec![0, 0, 0, 255, 12, 34, 56, 128]);
        let image = preview_image(2, 1, data.clone(), None).unwrap();
        image.with_bytes(0, |bytes| assert_eq!(bytes.unwrap(), data.as_slice()));
        assert!(image.with_bytes(1, |bytes| bytes.is_none()));
        assert_eq!(image.size(0), size(DevicePixels(2), DevicePixels(1)));
        let mut theme = Theme::new(&folio_app::Settings::default()).canvas;
        theme.adapt_ink = false;
        let original =
            preview_image(2, 1, data.clone(), Some(RasterAppearance::Ink(theme))).unwrap();
        original.with_bytes(0, |bytes| assert_eq!(bytes.unwrap(), data.as_slice()));
        theme.paper = 0x000000;
        theme.adapt_ink = true;
        let adapted =
            preview_image(2, 1, data.clone(), Some(RasterAppearance::Ink(theme))).unwrap();
        // Images are stored premultiplied, so translucent channels may
        // round by one step.
        adapted.with_bytes(0, |bytes| {
            let expected = theme.preview_pixels(&data);
            assert!(bytes.unwrap().iter().zip(expected.iter()).all(|(a, b)| a.abs_diff(*b) <= 1));
        });
        assert_eq!(data.as_slice(), &[0, 0, 0, 255, 12, 34, 56, 128]);
        assert!(preview_image(2, 2, data.clone(), None).is_none());
        assert!(preview_image(u32::MAX, u32::MAX, data.clone(), None).is_none());
        assert!(preview_image(0, 1, Arc::new(Vec::new()), None).is_none());

        let request = RasterRequest {
            page: Id::new_v4(),
            token: Arc::new(()),
            appearance: Some(RasterAppearance::Ink(
                Theme::new(&folio_app::Settings::default()).canvas,
            )),
        };
        assert!(request.matches(&request.clone()));
        let mut changed = request.clone();
        changed.page = Id::new_v4();
        assert!(!request.matches(&changed));
        let mut changed = request.clone();
        changed.token = Arc::new(());
        assert!(!request.matches(&changed));
        let mut changed = request.clone();
        changed.appearance = None;
        assert!(!request.matches(&changed));
        let mut settings = folio_app::Settings::default();
        let mut graph_request = request.clone();
        graph_request.appearance =
            Some(RasterAppearance::Graph(Theme::new(&settings).graph(false)));
        settings.appearance.set_color(
            false,
            folio_app::appearance::ThemeToken::Primary,
            folio_app::appearance::ThemeColor::opaque(0x52876c),
        );
        let mut changed = graph_request.clone();
        changed.appearance = Some(RasterAppearance::Graph(Theme::new(&settings).graph(false)));
        assert!(!graph_request.matches(&changed));
    }
    #[::core::prelude::v1::test]
    fn large_compound_ink_does_not_disappear_at_the_16_bit_vertex_limit() {
        let contours = (0..22_000)
            .map(|i| {
                let x = (i % 220) as f32 * 4.;
                let y = (i / 220) as f32 * 4.;
                vec![
                    DocPoint::new(x, y),
                    DocPoint::new(x + 2., y),
                    DocPoint::new(x, y + 2.),
                ]
            })
            .collect::<Vec<_>>();
        assert!(ink_path(&contours).is_some());
    }
}

#[cfg(test)]
mod shape_render_tests {
    use super::*;
    fn contains(points: &[DocPoint], p: DocPoint) -> bool {
        let mut inside = false;
        for (a, b) in points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
        {
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
        }
        inside
    }
    #[::core::prelude::v1::test]
    fn retraced_arrow_arms_keep_their_full_width() {
        let points = [
            DocPoint::new(452., 301.),
            DocPoint::new(517., 203.),
            DocPoint::new(480., 214.),
            DocPoint::new(517., 203.),
            DocPoint::new(532., 257.),
        ];
        for width in [1., 3., 8.] {
            let contours = shape_contours(&points, width);
            assert!(shape_path(&points, width).is_some());
            for pair in points.windows(2) {
                let length = pair[0].distance(pair[1]);
                for fraction in [0.2, 0.5, 0.8] {
                    let p = pair[0].lerp(pair[1], fraction);
                    for offset in [-0.3, 0., 0.3] {
                        let probe = DocPoint::new(
                            p.x - (pair[1].y - pair[0].y) / length * width * offset,
                            p.y + (pair[1].x - pair[0].x) / length * width * offset,
                        );
                        assert!(
                            contours.iter().any(|c| contains(c, probe)),
                            "missing arrow arm at {probe:?}"
                        );
                    }
                }
            }
        }
    }
}
