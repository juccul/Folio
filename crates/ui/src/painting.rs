use super::{NotesView, Theme};
use super::{graph, theme::CanvasTheme};
use folio_app::{Controller, Interaction, Tool};
use folio_canvas::{PageStack, SpatialIndex, Viewport};
use folio_document::{
    Id, Object, Page, PageProperties, Paper, PathPoint, Point as DocPoint, Rect, Transform,
};
use gpui::{prelude::*, *};
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
    let objects: Vec<Arc<Object>> = if ids.len() > page.order.len() / 2 {
        // Dense views already have a cheap ordered traversal. Sorting their
        // hashed candidates repeats rank lookups on every content invalidation.
        page.order
            .iter()
            .filter(|id| ids.contains(id))
            .filter_map(|id| page.objects.get(id))
            .cloned()
            .collect()
    } else {
        let mut ordered = ids.iter().copied().collect::<Vec<_>>();
        ordered.sort_unstable_by_key(|id| positions.get(id).copied().unwrap_or(usize::MAX));
        ordered
            .iter()
            .filter_map(|id| page.objects.get(id))
            .cloned()
            .collect()
    };
    let preview_ids = objects
        .iter()
        .filter(|object| !matches!(object.as_ref(), Object::Stroke(_) | Object::Shape(_)))
        .map(|object| object.id())
        .collect();
    *cached = Some(Arc::new(VisibleFrame {
        page: page.id,
        revision: page.revision,
        generation: index.generation(),
        coverage,
        objects,
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
            self.path = ink_path(&folio_ink::outline(points));
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
    data: Arc<Vec<u8>>,
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
            && Arc::ptr_eq(&self.data, &other.data)
            && self.appearance == other.appearance
    }
}
struct CachedRaster {
    data: Arc<Vec<u8>>,
    image: Arc<RenderImage>,
    appearance: Option<RasterAppearance>,
}
#[derive(Default)]
pub struct Painter {
    pub(super) ink_layer_override: Option<bool>,
    ink_planner: super::ink_layers::Planner,
    ink_plan: Option<InkPlan>,
    ink_layers: HashMap<Id, CachedInkLayer>,
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
    appearance: Option<CanvasTheme>,
    graph_palette: Option<graph::Palette>,
    colors: HashMap<u32, u32>,
}
fn alpha(color: u32, opacity: f32) -> Rgba {
    let mut color = rgb(color);
    color.a = opacity;
    color
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
fn ink_path(contours: &[Vec<DocPoint>]) -> Option<Path<Pixels>> {
    if contours.is_empty() {
        return None;
    }
    let mut b = PathBuilder::fill().with_style(PathStyle::Fill(
        FillOptions::default().with_fill_rule(FillRule::NonZero),
    ));
    for points in contours {
        if let Some(first) = points.first() {
            b.move_to(point_px(*first));
            for p in points.iter().skip(1) {
                b.line_to(point_px(*p));
            }
            b.close();
        }
    }
    b.build().ok()
}
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
    ink_path(&shape_contours(points, width))
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
    let outline = rect_points(bounds.expand(4.)).map(|p| transform.apply(p));
    let rotation = [
        DocPoint::new(bounds.center().x, bounds.min.y),
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
    pub(super) fn clear(&mut self, window: &mut Window, cx: &mut Context<NotesView>) {
        for source in self.backgrounds.values() {
            Self::release_background(source, window, cx);
        }
        self.clear_page(window);
    }
    fn release_background(source: &ImageSource, window: &mut Window, cx: &mut Context<NotesView>) {
        // Sources are requested when added to the warm cache. Removing the
        // decoded asset alone does not remove its uploaded sprite-atlas image.
        if let Some(image) = source.use_render_image(window, cx) {
            let _ = window.drop_image(image);
        }
        source.remove_asset(cx);
    }
    fn clear_page(&mut self, window: &mut Window) {
        for painter in self.pages.values_mut() {
            painter.clear_page(window);
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
        let stack = PageStack::new(&session.document.pages, active);
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
        // Prepare visible PDFs first, then three neighbors on either side. Waiting
        // until a page is painted starts both Poppler and PNG decoding too late
        // for continuous scrolling; the paper flashes empty in the meantime.
        let mut nearby = frames.iter().map(|(index, _)| *index).collect::<Vec<_>>();
        if let Some(stack) = &stack {
            let first = nearby.iter().copied().min().unwrap_or(active);
            let last = nearby.iter().copied().max().unwrap_or(active);
            for distance in 1..=3 {
                for index in [last.checked_add(distance), first.checked_sub(distance)]
                    .into_iter()
                    .flatten()
                {
                    if index >= stack.pages[0].0
                        && index <= stack.pages.last().unwrap().0
                        && !nearby.contains(&index)
                    {
                        nearby.push(index);
                    }
                }
            }
        }
        let mut warm_assets = HashSet::new();
        for index in nearby {
            let Some(pdf) = controller.session().document.pages[index]
                .properties
                .pdf
                .clone()
            else {
                continue;
            };
            controller.request_pdf_scroll_preview(pdf.clone());
            if settled && ids.contains(&controller.session().document.pages[index].id) {
                controller.request_pdf_background(pdf.clone());
            }
            for asset in pdf
                .preview_asset
                .iter()
                .cloned()
                .chain(Controller::pdf_scroll_preview_asset(&pdf))
            {
                warm_assets.insert(asset.clone());
                if let Some(path) = controller.asset_path(&asset)
                    && path.is_file()
                {
                    let source = self
                        .backgrounds
                        .entry(asset)
                        .or_insert_with(|| ImageSource::from(path));
                    // Decode off-thread through GPUI's shared asset loader and
                    // request a repaint when it completes, even off screen.
                    let _ = source.use_render_image(window, cx);
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
        // Retain only visible pages, so scrolling a long document cannot grow
        // native path/image caches without bound.
        self.pages.retain(|id, painter| {
            if ids.contains(id) {
                true
            } else {
                painter.clear_page(window);
                false
            }
        });
        for (index, viewport) in frames {
            let id = controller.session().document.pages[index].id;
            let painter = self.pages.entry(id).or_default();
            painter.ink_layer_override = self.ink_layer_override;
            painter.paint_page(controller, index, viewport, bounds, editing, window, cx);
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
        let visible = viewport.visible(f32::from(bounds.size.width), f32::from(bounds.size.height));
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
        ) {
            let ids = &self.frame.as_ref().unwrap().ids;
            let trim_paths = self.paths.len() > 2048;
            self.paths
                .retain(|id, _| page.objects.contains_key(id) && (!trim_paths || ids.contains(id)));
            self.text_layouts.retain(|id, _| ids.contains(id));
        }
        let frame = self.frame.as_ref().unwrap().clone();
        if editing.is_some_and(|id| frame.preview_ids.contains(&id)) {
            let mut preview_ids = frame.preview_ids.clone();
            preview_ids.remove(&editing.unwrap());
            controller.request_page_previews(page_index, &preview_ids);
        } else {
            controller.request_page_previews(page_index, &frame.preview_ids);
        }
        let properties = controller.session().document.pages[page_index]
            .properties
            .clone();
        let visible_ids = &frame.ids;
        self.retain_rasters(
            |id, cache| {
                visible_ids.contains(&id)
                    && controller
                        .previews
                        .get(&(controller.active, page_id, id))
                        .is_some_and(|preview| Arc::ptr_eq(&preview.bgra, &cache.data))
            },
            |image| {
                let _ = window.drop_image(image);
            },
        );
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
        // erasing and edits immediately fall back to original vector objects.
        let mut ink_plan_changed = false;
        let ink_batches = if self.ink_layer_override.unwrap_or(true) {
            let mut excluded = selection.union(erasing).copied().collect::<HashSet<_>>();
            excluded.extend(editing);
            let density = viewport.zoom * window.scale_factor();
            if !self.ink_plan.as_ref().is_some_and(|plan| {
                Arc::ptr_eq(&plan.frame, &frame)
                    && plan.excluded == excluded
                    && plan.theme == canvas_theme
                    && plan.density == density
            }) {
                let batches = Arc::new(self.ink_planner.batches(
                    &frame.objects,
                    &excluded,
                    canvas_theme,
                    density,
                ));
                self.ink_plan = Some(InkPlan {
                    frame: frame.clone(),
                    excluded,
                    theme: canvas_theme,
                    density,
                    batches,
                });
                ink_plan_changed = true;
            }
            self.ink_plan.as_ref().unwrap().batches.clone()
        } else {
            ink_plan_changed = self.ink_plan.take().is_some();
            Arc::new(Vec::new())
        };
        self.prepare_ink_layers(&ink_batches, ink_plan_changed, page_id, window, cx);
        let mut batch_iter = ink_batches.iter().peekable();
        let mut skip_until = 0;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.paper(&properties, visible, world, window, cx, controller);
            for (index, object) in frame.objects.iter().enumerate() {
                if index < skip_until {
                    continue;
                }
                if batch_iter
                    .peek()
                    .is_some_and(|(range, _)| range.start == index)
                {
                    let (range, batch) = batch_iter.next().unwrap();
                    if let Some(layer) = self.ink_layers.get(&batch.id())
                        && window
                            .paint_image_transformed(
                                gpui_bounds(batch.bounds),
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
                            if let Some(path) = ink_path(&folio_ink::outline(part.display_path())) {
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
                        let color = self.display_color(s.style.color.rgb(), canvas_theme);
                        let valid = self
                            .paths
                            .get(&s.id)
                            .is_some_and(|cache| Arc::ptr_eq(&cache.object, object));
                        if !valid
                            && let Some(path) = ink_path(&folio_ink::outline(s.display_path()))
                        {
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
                        let color = self.display_color(text.color.rgb(), canvas_theme);
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
                        if let Some(preview) = controller
                            .previews
                            .get(&(controller.active, page_id, object.id()))
                            .filter(|preview| Arc::ptr_eq(&preview.object, object))
                        {
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
                            let valid = self.rasters.get(&object.id()).is_some_and(|cache| {
                                Arc::ptr_eq(&cache.data, &preview.bgra)
                                    && cache.appearance == appearance
                            });
                            if !valid {
                                self.request_raster(object.id(), page_id, preview, appearance, cx);
                            }
                            if let Some(image) = self.rasters.get(&object.id()).filter(|image| {
                                Arc::ptr_eq(&image.data, &preview.bgra)
                                    && image.appearance == appearance
                            }) {
                                let transform = world.compose(movement);
                                let _ = window.paint_image_transformed(
                                    gpui_bounds(preview.bounds),
                                    Default::default(),
                                    image.image.clone(),
                                    0,
                                    false,
                                    gpu_transform(transform),
                                );
                            }
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
                        let style = window.text_style();
                        let run = TextRun {
                            len: label.len(),
                            font: style.font(),
                            color: rgb(0xc28c36).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let line =
                            window
                                .text_system()
                                .shape_line(label.into(), px(12.), &[run], None);
                        let origin = world
                            .compose(movement)
                            .apply(DocPoint::new(rect.min.x, rect.min.y));
                        let _ = line.paint(
                            point(px(origin.x), px(origin.y - 18.)),
                            px(18.),
                            window,
                            cx,
                        );
                    }
                }
            }
            if !active {
                return;
            }
            for r in &controller.search_highlights {
                let points = rect_points(*r).map(|p| world.apply(p));
                if let Some(path) = fill_path(&points) {
                    window.paint_path(path, alpha(0xe7c25d, 0.18));
                }
                if let Some(path) = line_path(&points, 1.5, true) {
                    window.paint_path(path, rgb(0xb79134));
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
                                rgb(theme.accent),
                            );
                        }
                    }
                    Interaction::Rectangle { start, end } => {
                        let points = rect_points(Rect::from_points([*start, *end]));
                        if let Some(path) = line_path(&points, 1.2 / viewport.zoom, true) {
                            window.paint_path(
                                path.transformed(coefficients(world)),
                                rgb(theme.accent),
                            );
                        }
                    }
                    _ => {}
                }
            }
            if let Some(rect) = controller.selection_bounds() {
                let transform = world.compose(preview_move);
                let (points, rotation) = selection_overlay(rect, viewport.zoom, transform);
                if let Some(path) = line_path(&points, 1.2, true) {
                    window.paint_path(path, rgb(theme.accent));
                }
                for p in points.iter().take(4) {
                    window.paint_quad(quad(
                        Bounds::new(
                            point_px(DocPoint::new(p.x - 3., p.y - 3.)),
                            size(px(6.), px(6.)),
                        ),
                        px(1.),
                        rgb(theme.surface),
                        px(1.),
                        rgb(theme.accent),
                        Default::default(),
                    ));
                }
                if let Some(path) = line_path(&rotation, 1., false) {
                    window.paint_path(path, rgb(theme.accent));
                }
                if let Some(path) = fill_path(&folio_ink::circle(rotation[1], 4., 16)) {
                    window.paint_path(path, rgb(theme.accent));
                }
            }
            if let Some(cursor) = controller.cursor {
                let center = world.apply(cursor);
                let r = if controller.tool == Tool::Eraser {
                    controller.settings.eraser_radius * viewport.zoom
                } else {
                    controller.settings.cursor_size * 0.5
                };
                if let Some(path) = line_path(
                    &folio_ink::circle(center, r, 32)
                        .into_iter()
                        .chain(std::iter::once(DocPoint::new(center.x + r, center.y)))
                        .collect::<Vec<_>>(),
                    1.,
                    false,
                ) {
                    window.paint_path(path, alpha(theme.muted, 0.65));
                }
            }
        });
    }
    fn clear_ink_layers(&mut self, window: &mut Window) {
        for (_, layer) in self.ink_layers.drain() {
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
            self.ink_layers.retain(|id, layer| {
                if self
                    .ink_desired
                    .get(id)
                    .is_some_and(|batch| batch.matches(&layer.batch))
                {
                    true
                } else {
                    let _ = window.drop_image(layer.image.clone());
                    false
                }
            });
            self.ink_failed.retain(|id, failed| {
                self.ink_desired
                    .get(id)
                    .is_some_and(|batch| batch.matches(failed))
            });
        }
        for (_, batch) in batches {
            let id = batch.id();
            if self.ink_pending.len() >= 2 {
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
            data: preview.bgra.clone(),
            appearance,
        };
        self.raster_pending.insert(id, request.clone());
        let (width, height) = (preview.width, preview.height);
        let data = request.data.clone();
        let object = preview.object.clone();
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
                    .is_some_and(|preview| Arc::ptr_eq(&preview.bgra, &request.data));
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
                    if let Some(previous) = painter.rasters.insert(
                        id,
                        CachedRaster {
                            data: request.data,
                            image,
                            appearance,
                        },
                    ) {
                        // This entity update runs outside a window update, so
                        // all windows are available through the app context.
                        cx.drop_image(previous.image, None);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn paper(
        &mut self,
        properties: &PageProperties,
        visible: Rect,
        world: Transform,
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
            let mut rectangles = Vec::new();
            if !properties.infinite {
                pixel_rectangles(
                    Rect::new(
                        bounds.min.x + 2.,
                        bounds.min.y + 3.,
                        bounds.width(),
                        bounds.height(),
                    ),
                    alpha(0x000000, 0.08),
                    window.scale_factor(),
                    &mut rectangles,
                );
            }
            pixel_rectangles(
                bounds,
                rgb(paper_color),
                window.scale_factor(),
                &mut rectangles,
            );
            for (bounds, color) in rectangles {
                window.paint_quad(fill(gpui_bounds(bounds), color));
            }
        } else {
            let points = rect_points(page).map(|p| world.apply(p));
            if !properties.infinite {
                let shadow = points.map(|p| DocPoint::new(p.x + 2., p.y + 3.));
                if let Some(path) = fill_path(&shadow) {
                    window.paint_path(path, alpha(0x000000, 0.08));
                }
            }
            if let Some(path) = fill_path(&points) {
                window.paint_path(path, rgb(paper_color));
            }
        }
        if let Some(pdf) = &properties.pdf {
            for asset in pdf
                .preview_asset
                .iter()
                .cloned()
                .chain(Controller::pdf_scroll_preview_asset(pdf))
            {
                let Some(path) = controller.asset_path(&asset).filter(|path| path.is_file()) else {
                    continue;
                };
                let source = self
                    .backgrounds
                    .entry(asset.clone())
                    .or_insert_with(|| ImageSource::from(path));
                if let Some(image) = source.use_render_image(window, cx) {
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
        let key = PaperKey {
            paper: properties.paper,
            page,
            visible,
            world,
            canvas: canvas_theme,
        };
        if self
            .paper_cache
            .as_ref()
            .is_none_or(|cached| cached.key != key || cached.scale != window.scale_factor())
        {
            let scale = window.scale_factor();
            let geometry = paper_rectangles(&key, scale)
                .map(PaperGeometry::Rectangles)
                .unwrap_or_else(|| PaperGeometry::Paths(paper_paths(&key)));
            self.paper_cache = Some(CachedPaper {
                key,
                scale,
                geometry,
            });
        }
        match &self.paper_cache.as_ref().unwrap().geometry {
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
    let pixels = match appearance {
        Some(RasterAppearance::Ink(theme)) if theme.adapt_ink => {
            match theme.preview_pixels(&data) {
                std::borrow::Cow::Borrowed(_) => data,
                std::borrow::Cow::Owned(pixels) => Arc::new(pixels),
            }
        }
        _ => data,
    };
    RenderImage::from_shared_bgra(width, height, pixels).map(Arc::new)
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

fn pixel_rectangles(rect: Rect, color: Rgba, scale: f32, rectangles: &mut Vec<(Rect, Rgba)>) {
    // Quads have hard rectangular edges. Split fractional device pixels into
    // weighted edge strips so thin ruling retains its coverage during resizing.
    for (x0, x1, x_alpha) in pixel_bands(rect.min.x, rect.max.x, scale) {
        for (y0, y1, y_alpha) in pixel_bands(rect.min.y, rect.max.y, scale) {
            let mut color = color;
            color.a *= x_alpha * y_alpha;
            rectangles.push((Rect::new(x0, y0, x1 - x0, y1 - y0), color));
        }
    }
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

fn gpu_transform(t: Transform) -> gpui::TransformationMatrix {
    gpui::TransformationMatrix {
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
        let data = Arc::new(vec![0; 4]);
        for id in [visible, stale, offscreen] {
            let image = graph::image(1, 1, vec![0; 4]);
            images.push(Arc::downgrade(&image));
            atlas.insert(image.id);
            painter.rasters.insert(
                id,
                CachedRaster {
                    data: if id == stale {
                        Arc::new(vec![0; 4])
                    } else {
                        data.clone()
                    },
                    image,
                    appearance: None,
                },
            );
        }
        painter.retain_rasters(
            |id, raster| id != offscreen && Arc::ptr_eq(&raster.data, &data),
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
                    ink_path(&folio_ink::outline(s.display_path())).unwrap()
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
                        std::hint::black_box(planner.batches(
                            &cached.as_ref().unwrap().objects,
                            &HashSet::new(),
                            theme,
                            1.,
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
            assert!((dragged_rotation[0].distance(dragged_rotation[1]) - 24.).abs() < 0.001);
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
                if paper != Paper::Dots {
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
        // Still previews retain the original immutable allocation until their
        // last owner drops. Only appearance conversion needs another buffer.
        let data = Arc::new(vec![0, 0, 0, 255, 12, 34, 56, 128]);
        let weak = Arc::downgrade(&data);
        let image = preview_image(2, 1, data.clone(), None).unwrap();
        assert_eq!(image.as_bytes(0).unwrap().as_ptr(), data.as_ptr());
        assert_eq!(image.as_bytes(0).unwrap(), data.as_slice());
        assert!(image.as_bytes(1).is_none());
        assert_eq!(image.size(0), size(2.into(), 1.into()));
        assert_eq!(image.frame_count(), 1);
        assert_eq!(image.delay(0), image::Delay::from_numer_denom_ms(0, 1));
        let mut theme = Theme::new(&folio_app::Settings::default()).canvas;
        theme.adapt_ink = false;
        let original =
            preview_image(2, 1, data.clone(), Some(RasterAppearance::Ink(theme))).unwrap();
        assert_eq!(original.as_bytes(0).unwrap().as_ptr(), data.as_ptr());
        theme.adapt_ink = true;
        theme.paper = 0xffffff;
        {
            let readable =
                preview_image(2, 1, data.clone(), Some(RasterAppearance::Ink(theme))).unwrap();
            assert_eq!(readable.as_bytes(0).unwrap().as_ptr(), data.as_ptr());
        }
        theme.paper = 0x000000;
        theme.adapt_ink = true;
        let adapted =
            preview_image(2, 1, data.clone(), Some(RasterAppearance::Ink(theme))).unwrap();
        assert_eq!(
            adapted.as_bytes(0).unwrap(),
            theme.preview_pixels(&data).as_ref()
        );
        assert_ne!(adapted.as_bytes(0).unwrap().as_ptr(), data.as_ptr());
        assert_eq!(data.as_slice(), &[0, 0, 0, 255, 12, 34, 56, 128]);
        assert!(preview_image(2, 2, data.clone(), None).is_none());
        assert!(preview_image(u32::MAX, u32::MAX, data.clone(), None).is_none());
        assert!(preview_image(0, 1, Arc::new(Vec::new()), None).is_none());
        drop(data);
        drop(original);
        assert!(weak.upgrade().is_some());
        drop(image);
        assert!(weak.upgrade().is_none());
        assert_eq!(adapted.frame_count(), 1);

        // Animated image decoding still uses owned frames and their delays.
        let frames = smallvec::smallvec![
            image::Frame::from_parts(
                image::RgbaImage::from_raw(1, 1, vec![1, 2, 3, 4]).unwrap(),
                0,
                0,
                image::Delay::from_numer_denom_ms(20, 1),
            ),
            image::Frame::from_parts(
                image::RgbaImage::from_raw(1, 1, vec![5, 6, 7, 8]).unwrap(),
                0,
                0,
                image::Delay::from_numer_denom_ms(30, 1),
            ),
        ];
        let animation = RenderImage::new(frames);
        assert_eq!(animation.frame_count(), 2);
        assert_eq!(animation.size(1), size(1.into(), 1.into()));
        assert_eq!(animation.as_bytes(1).unwrap(), &[5, 6, 7, 8]);
        assert_eq!(animation.delay(0), image::Delay::from_numer_denom_ms(20, 1));
        assert_eq!(animation.delay(1), image::Delay::from_numer_denom_ms(30, 1));

        let request = RasterRequest {
            page: Id::new_v4(),
            data: Arc::new(vec![0; 16]),
            appearance: Some(RasterAppearance::Ink(
                Theme::new(&folio_app::Settings::default()).canvas,
            )),
        };
        assert!(request.matches(&request.clone()));
        let mut changed = request.clone();
        changed.page = Id::new_v4();
        assert!(!request.matches(&changed));
        let mut changed = request.clone();
        changed.data = Arc::new(request.data.as_ref().clone());
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
