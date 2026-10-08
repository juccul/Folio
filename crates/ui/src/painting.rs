use super::{NotesView, Theme};
use super::{graph, theme::CanvasTheme};
use folio_app::{Controller, Interaction, Tool};
use folio_canvas::{PageStack, SpatialIndex, Viewport};
use folio_document::{
    Id, Object, PageProperties, Paper, PathPoint, Point as DocPoint, Rect, Transform,
};
use gpui::{prelude::*, *};
use std::{
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
    visible: Rect,
    objects: Vec<Arc<Object>>,
    ids: HashSet<Id>,
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
    paths: Vec<(Path<Pixels>, Rgba)>,
}
struct CachedPath {
    object: Arc<Object>,
    path: Path<Pixels>,
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
    text_layouts: HashMap<Id, (folio_document::TextBlock, super::text_render::Layout)>,
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
impl Painter {
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
        let frames = if let Some(stack) = PageStack::new(&session.document.pages, active) {
            stack
                .pages
                .iter()
                .filter_map(|(index, rect)| {
                    let local = stack.viewport(viewport, active, *index);
                    let visible = local.visible(width, height);
                    (*index == active
                        || visible.intersects(Rect::new(0., 0., rect.width(), rect.height())))
                    .then_some((*index, local))
                })
                .collect::<Vec<_>>()
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
        if let Some(stack) = PageStack::new(&session.document.pages, active) {
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
                source.remove_asset(cx);
                false
            }
        });
        // Retain only visible pages, so scrolling a long document cannot grow
        // native path/image caches without bound.
        self.pages.retain(|id, _| ids.contains(id));
        for (index, viewport) in frames {
            let id = controller.session().document.pages[index].id;
            let painter = self.pages.entry(id).or_default();
            painter.paint_page(controller, index, viewport, bounds, editing, window, cx);
        }
    }
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
        let generation = if active {
            controller.session().index.generation()
        } else {
            self.index.generation()
        };
        let valid = self.frame.as_ref().is_some_and(|frame| {
            frame.page == page_id
                && frame.revision == page.revision
                && frame.generation == generation
                && frame.visible == visible
        });
        if !valid {
            let ids = if active {
                controller.session().index.query(visible)
            } else {
                self.index.query(visible)
            };
            let mut ordered = ids.iter().copied().collect::<Vec<_>>();
            let positions = if active {
                &controller.session().order_positions
            } else {
                &self.order_positions
            };
            ordered.sort_unstable_by_key(|id| positions.get(id).copied().unwrap_or(usize::MAX));
            let objects = ordered
                .iter()
                .filter_map(|id| page.objects.get(id))
                .cloned()
                .collect();
            let trim_paths = self.paths.len() > 2048;
            self.paths
                .retain(|id, _| page.objects.contains_key(id) && (!trim_paths || ids.contains(id)));
            self.text_layouts.retain(|id, _| ids.contains(id));
            self.frame = Some(Arc::new(VisibleFrame {
                page: page_id,
                revision: page.revision,
                generation,
                visible,
                objects,
                ids,
            }));
        }
        let frame = self.frame.as_ref().unwrap().clone();
        let preview_ids = frame
            .ids
            .iter()
            .copied()
            .filter(|id| Some(*id) != editing)
            .collect();
        controller.request_page_previews(page_index, &preview_ids);
        let properties = controller.session().document.pages[page_index]
            .properties
            .clone();
        let visible_ids = &frame.ids;
        self.rasters.retain(|id, cache| {
            visible_ids.contains(id)
                && controller
                    .previews
                    .get(&(controller.active, page_id, *id))
                    .is_some_and(|preview| Arc::ptr_eq(&preview.bgra, &cache.data))
        });
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
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.paper(&properties, visible, world, window, cx, controller);
            for object in &frame.objects {
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
                        let text = if active {
                            controller.preview_text(text)
                        } else {
                            text.clone()
                        };
                        let color = self.display_color(text.color.rgb(), canvas_theme);
                        if self
                            .text_layouts
                            .get(&text.id)
                            .is_none_or(|(cached, _)| cached != &text)
                        {
                            let layout = super::text_render::layout(&text, color, window);
                            self.text_layouts.insert(text.id, (text.clone(), layout));
                        }
                        let layout = &self.text_layouts[&text.id].1;
                        let transform = world
                            .compose(text.transform)
                            .compose(Transform::translate(text.rect.min.x, text.rect.min.y));
                        layout.paint(transform, color, text.underline, window);
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
                                if let Some(path) = ink_path(&folio_ink::outline(points)) {
                                    window.paint_path(path.transformed(coefficients(world)), color);
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
                    (controller.style.width * 2.).max(10.) * viewport.zoom
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
            let pixels = match appearance {
                Some(RasterAppearance::Ink(theme)) => theme.preview_pixels(&data),
                _ => data.as_ref().clone(),
            };
            image::RgbaImage::from_raw(width, height, pixels).map(|buffer| {
                Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
                    buffer
                )]))
            })
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
                    painter.rasters.insert(
                        id,
                        CachedRaster {
                            data: request.data,
                            image,
                            appearance,
                        },
                    );
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
            .is_none_or(|cached| cached.key != key)
        {
            let paths = paper_paths(&key);
            self.paper_cache = Some(CachedPaper { key, paths });
        }
        for (path, color) in &self.paper_cache.as_ref().unwrap().paths {
            window.paint_path(path.clone(), *color);
        }
    }
}

fn paper_paths(key: &PaperKey) -> Vec<(Path<Pixels>, Rgba)> {
    let PaperKey {
        page,
        visible,
        world,
        ..
    } = *key;
    // A paper completely outside the viewport needs no grid tessellation.
    if !page.intersects(visible) {
        return Vec::new();
    }
    let zoom = world.scale();
    let spacing = if zoom < 0.3 { 64. } else { 32. };
    let clip = Rect::new(
        visible.min.x.max(page.min.x),
        visible.min.y.max(page.min.y),
        (visible.max.x.min(page.max.x) - visible.min.x.max(page.min.x)).max(0.),
        (visible.max.y.min(page.max.y) - visible.min.y.max(page.min.y)).max(0.),
    );
    let x0 = (clip.min.x / spacing).floor() as i32;
    let x1 = (clip.max.x / spacing).ceil() as i32;
    let y0 = (clip.min.y / spacing).floor() as i32;
    let y1 = (clip.max.y / spacing).ceil() as i32;
    let mut lines = Vec::new();
    if matches!(key.paper, Paper::Ruled | Paper::Grid) {
        for y in y0..=y1 {
            let y = y as f32 * spacing;
            if y >= page.min.y && y <= page.max.y {
                lines.push([DocPoint::new(clip.min.x, y), DocPoint::new(clip.max.x, y)]);
            }
        }
    }
    if key.paper == Paper::Grid {
        for x in x0..=x1 {
            let x = x as f32 * spacing;
            if x >= page.min.x && x <= page.max.x {
                lines.push([DocPoint::new(x, clip.min.y), DocPoint::new(x, clip.max.y)]);
            }
        }
    }
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
        let mut builder = PathBuilder::fill();
        let mut has_dots = false;
        for x in x0..=x1 {
            for y in y0..=y1 {
                let p = DocPoint::new(
                    x as f32 * spacing + spacing / 2.,
                    y as f32 * spacing + spacing / 2.,
                );
                if page.contains(p) && clip.contains(p) {
                    let points = folio_ink::circle(world.apply(p), zoom.max(0.7), 8);
                    builder.move_to(point_px(points[0]));
                    for p in points.iter().skip(1) {
                        builder.line_to(point_px(*p));
                    }
                    builder.close();
                    has_dots = true;
                }
            }
        }
        if has_dots && let Ok(path) = builder.build() {
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
    fn paper_grids_use_a_single_cached_compound_path() {
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
    }
    #[::core::prelude::v1::test]
    fn raster_jobs_are_invalidated_by_pixels_page_and_theme() {
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
