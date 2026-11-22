//! GPUI's immediate-mode painting (`canvas`, `paint_path`, `paint_quad`,
//! `paint_image`), recorded as a display list: quads and images become Rune
//! elements drawn on the GPU, and paths and text are rasterized with
//! tiny-skia at the window's device scale, only within their bounds.
#![allow(dead_code)]
use super::compat::*;
use resvg::tiny_skia as sk;
use std::{cell::RefCell, collections::HashMap, sync::Arc};

// ---------------------------------------------------------------- paths

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FillOptions {
    rule: FillRule,
}
impl FillOptions {
    pub fn with_fill_rule(mut self, rule: FillRule) -> Self {
        self.rule = rule;
        self
    }
}
#[derive(Clone, Copy, Debug)]
pub enum PathStyle {
    Fill(FillOptions),
    Stroke(f32),
}

/// A vector path in window points, filled or stroked.
#[derive(Clone)]
pub struct Path<P = Pixels> {
    path: Option<Arc<sk::Path>>,
    stroke: Option<f32>,
    dash: Option<Vec<f32>>,
    rule: FillRule,
    // Raw triangles (`push_triangle`) and the open builder of `Path::new`.
    builder: Option<sk::PathBuilder>,
    _p: std::marker::PhantomData<P>,
}
impl<P> Path<P> {
    /// A path that starts at `start`, as GPUI's raw path does.
    pub fn new(start: Point<Pixels>) -> Self {
        let mut builder = sk::PathBuilder::new();
        builder.move_to(start.x.0, start.y.0);
        Self {
            path: None,
            stroke: None,
            dash: None,
            rule: FillRule::NonZero,
            builder: Some(builder),
            _p: Default::default(),
        }
    }
    pub fn move_to(&mut self, p: Point<Pixels>) {
        self.builder
            .get_or_insert_with(sk::PathBuilder::new)
            .move_to(p.x.0, p.y.0);
    }
    pub fn line_to(&mut self, p: Point<Pixels>) {
        self.builder
            .get_or_insert_with(sk::PathBuilder::new)
            .line_to(p.x.0, p.y.0);
    }
    pub fn curve_to(&mut self, to: Point<Pixels>, ctrl: Point<Pixels>) {
        self.builder
            .get_or_insert_with(sk::PathBuilder::new)
            .quad_to(ctrl.x.0, ctrl.y.0, to.x.0, to.y.0);
    }
    pub fn push_triangle(
        &mut self,
        (a, b, c): (Point<Pixels>, Point<Pixels>, Point<Pixels>),
        _: (Point<f32>, Point<f32>, Point<f32>),
    ) {
        let builder = self.builder.get_or_insert_with(sk::PathBuilder::new);
        builder.move_to(a.x.0, a.y.0);
        builder.line_to(b.x.0, b.y.0);
        builder.line_to(c.x.0, c.y.0);
        builder.close();
    }
    fn resolved(&self) -> Option<Arc<sk::Path>> {
        if let Some(builder) = &self.builder {
            let mut path = self.path.as_deref().cloned().map(|p| {
                let mut b = sk::PathBuilder::new();
                b.push_path(&p);
                b
            });
            let builder = builder.clone();
            let built = match path.as_mut() {
                Some(b) => {
                    if let Some(extra) = builder.finish() {
                        b.push_path(&extra);
                    }
                    path.and_then(|b| b.finish())
                }
                None => builder.finish(),
            };
            return built.map(Arc::new);
        }
        self.path.clone()
    }
    /// The path with each point mapped through `[a, b, c, d, tx, ty]`.
    pub fn transformed(self, m: [f32; 6]) -> Self {
        let t = sk::Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
        let path = self
            .resolved()
            .and_then(|p| (*p).clone().transform(t))
            .map(Arc::new);
        Self {
            path,
            builder: None,
            ..self
        }
    }
    pub fn scale(self, factor: f32) -> Self {
        self.transformed([factor, 0., 0., factor, 0., 0.])
    }
}

/// GPUI's path builder: contours, then `build()`.
#[derive(Clone)]
pub struct PathBuilder {
    builder: sk::PathBuilder,
    stroke: Option<f32>,
    dash: Option<Vec<f32>>,
    rule: FillRule,
}
impl PathBuilder {
    pub fn fill() -> Self {
        Self {
            builder: sk::PathBuilder::new(),
            stroke: None,
            dash: None,
            rule: FillRule::NonZero,
        }
    }
    pub fn stroke(width: Pixels) -> Self {
        Self {
            stroke: Some(width.0),
            ..Self::fill()
        }
    }
    pub fn with_style(mut self, style: PathStyle) -> Self {
        match style {
            PathStyle::Fill(options) => {
                self.stroke = None;
                self.rule = options.rule;
            }
            PathStyle::Stroke(width) => self.stroke = Some(width),
        }
        self
    }
    pub fn dash_array(mut self, dash: &[Pixels]) -> Self {
        self.dash = Some(dash.iter().map(|d| d.0).collect());
        self
    }
    pub fn move_to(&mut self, p: Point<Pixels>) {
        self.builder.move_to(p.x.0, p.y.0);
    }
    pub fn line_to(&mut self, p: Point<Pixels>) {
        self.builder.line_to(p.x.0, p.y.0);
    }
    pub fn curve_to(&mut self, to: Point<Pixels>, ctrl: Point<Pixels>) {
        self.builder.quad_to(ctrl.x.0, ctrl.y.0, to.x.0, to.y.0);
    }
    pub fn cubic_bezier_to(&mut self, to: Point<Pixels>, c1: Point<Pixels>, c2: Point<Pixels>) {
        self.builder
            .cubic_to(c1.x.0, c1.y.0, c2.x.0, c2.y.0, to.x.0, to.y.0);
    }
    pub fn arc_to(
        &mut self,
        radii: Point<Pixels>,
        _rotation: Pixels,
        large_arc: bool,
        sweep: bool,
        to: Point<Pixels>,
    ) {
        // Approximate the elliptical arc by its circular arc through the
        // current point and `to` (Folio draws circular arcs only).
        let Some(from) = self.builder.last_point() else {
            self.builder.move_to(to.x.0, to.y.0);
            return;
        };
        let r = radii.x.0.max(radii.y.0).max(0.001);
        let (x0, y0, x1, y1) = (from.x, from.y, to.x.0, to.y.0);
        let (dx, dy) = ((x1 - x0) / 2., (y1 - y0) / 2.);
        let d2 = dx * dx + dy * dy;
        let r = r.max(d2.sqrt());
        let h = ((r * r - d2).max(0.)).sqrt();
        let len = d2.sqrt().max(1e-6);
        let sign = if large_arc != sweep { 1. } else { -1. };
        let (cx, cy) = (x0 + dx - sign * h * dy / len, y0 + dy + sign * h * dx / len);
        let a0 = (y0 - cy).atan2(x0 - cx);
        let mut a1 = (y1 - cy).atan2(x1 - cx);
        if sweep && a1 < a0 {
            a1 += std::f32::consts::TAU;
        } else if !sweep && a1 > a0 {
            a1 -= std::f32::consts::TAU;
        }
        let steps = ((a1 - a0).abs() / 0.1).ceil().max(1.) as usize;
        for i in 1..=steps {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            self.builder.line_to(cx + r * a.cos(), cy + r * a.sin());
        }
    }
    pub fn close(&mut self) {
        self.builder.close();
    }
    pub fn build(self) -> Result<Path<Pixels>, ()> {
        let path = self.builder.finish().ok_or(())?;
        Ok(Path {
            path: Some(Arc::new(path)),
            stroke: self.stroke,
            dash: self.dash,
            rule: self.rule,
            builder: None,
            _p: Default::default(),
        })
    }
}

// ---------------------------------------------------------------- quads

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners<T> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}
impl<T: Copy> Corners<T> {
    pub fn all(v: T) -> Self {
        Self {
            top_left: v,
            top_right: v,
            bottom_right: v,
            bottom_left: v,
        }
    }
}
impl From<Pixels> for Corners<Pixels> {
    fn from(p: Pixels) -> Self {
        Corners::all(p)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}
impl<T: Copy> Edges<T> {
    pub fn all(v: T) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }
}
impl From<Pixels> for Edges<Pixels> {
    fn from(p: Pixels) -> Self {
        Edges::all(p)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BorderStyle {
    #[default]
    Solid,
    Dashed,
}
#[derive(Clone, Copy, Debug)]
pub struct PaintQuad {
    pub bounds: Bounds<Pixels>,
    pub corner_radii: Corners<Pixels>,
    pub background: Rgba,
    pub gradient: Option<LinearGradient>,
    pub border_widths: Edges<Pixels>,
    pub border_color: Rgba,
    pub border_style: BorderStyle,
}
impl PaintQuad {
    pub fn corner_radii(mut self, r: impl Into<Corners<Pixels>>) -> Self {
        self.corner_radii = r.into();
        self
    }
    pub fn border_widths(mut self, w: impl Into<Edges<Pixels>>) -> Self {
        self.border_widths = w.into();
        self
    }
    pub fn border_color(mut self, c: Rgba) -> Self {
        self.border_color = c;
        self
    }
}
pub fn fill(bounds: Bounds<Pixels>, background: impl Into<QuadFill>) -> PaintQuad {
    let (background, gradient) = match background.into() {
        QuadFill::Color(c) => (c, None),
        QuadFill::Gradient(g) => (Rgba::default(), Some(g)),
    };
    PaintQuad {
        bounds,
        corner_radii: Corners::default(),
        background,
        gradient,
        border_widths: Edges::default(),
        border_color: Rgba::default(),
        border_style: BorderStyle::Solid,
    }
}
pub fn quad(
    bounds: Bounds<Pixels>,
    corner_radii: impl Into<Corners<Pixels>>,
    background: Rgba,
    border_widths: impl Into<Edges<Pixels>>,
    border_color: Rgba,
    border_style: BorderStyle,
) -> PaintQuad {
    PaintQuad {
        bounds,
        corner_radii: corner_radii.into(),
        background,
        gradient: None,
        border_widths: border_widths.into(),
        border_color,
        border_style,
    }
}
pub fn outline(bounds: Bounds<Pixels>, color: Rgba, style: BorderStyle) -> PaintQuad {
    quad(
        bounds,
        Corners::default(),
        Rgba::default(),
        px(1.),
        color,
        style,
    )
}

pub enum QuadFill {
    Color(Rgba),
    Gradient(LinearGradient),
}
impl From<Rgba> for QuadFill {
    fn from(c: Rgba) -> Self {
        QuadFill::Color(c)
    }
}
impl From<LinearGradient> for QuadFill {
    fn from(g: LinearGradient) -> Self {
        QuadFill::Gradient(g)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ContentMask {
    pub bounds: Bounds<Pixels>,
}

/// GPUI's 2D affine transform: a row-major 2×2 matrix and a translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformationMatrix {
    pub rotation_scale: [[f32; 2]; 2],
    pub translation: [f32; 2],
}
impl TransformationMatrix {
    fn skia(self) -> sk::Transform {
        let [[a, c], [b, d]] = self.rotation_scale;
        sk::Transform::from_row(a, b, c, d, self.translation[0], self.translation[1])
    }
}

// ---------------------------------------------------------------- images

/// A decoded image: premultiplied RGBA for painting, and Rune pixels for
/// elements, made on first use.
pub struct RenderImage {
    pub id: u64,
    pixmap: Arc<sk::Pixmap>,
    pixels: std::sync::Mutex<Option<rune::Pixels>>,
}
static NEXT_IMAGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// A length in device pixels, as GPUI measured images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DevicePixels(pub i32);
impl RenderImage {
    pub fn from_pixmap(pixmap: sk::Pixmap) -> Self {
        Self {
            id: NEXT_IMAGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            pixmap: Arc::new(pixmap),
            pixels: std::sync::Mutex::new(None),
        }
    }
    fn from_bgra_straight(width: u32, height: u32, data: &[u8]) -> Option<Self> {
        if (width as u64)
            .checked_mul(height as u64)
            .and_then(|n| n.checked_mul(4))
            != Some(data.len() as u64)
        {
            return None;
        }
        let mut pixmap = sk::Pixmap::new(width, height)?;
        for (dst, src) in pixmap
            .pixels_mut()
            .iter_mut()
            .zip(data.as_chunks::<4>().0.iter())
        {
            let c = sk::ColorU8::from_rgba(src[2], src[1], src[0], src[3]);
            *dst = c.premultiply();
        }
        Some(Self::from_pixmap(pixmap))
    }
    pub fn from_ephemeral_bgra(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        Self::from_bgra_straight(width, height, &data)
    }
    pub fn from_ephemeral_shared_bgra(
        width: u32,
        height: u32,
        data: impl AsRef<[u8]>,
    ) -> Option<Self> {
        Self::from_bgra_straight(width, height, data.as_ref())
    }
    pub fn from_shared_bgra(width: u32, height: u32, data: impl AsRef<[u8]>) -> Option<Self> {
        Self::from_bgra_straight(width, height, data.as_ref())
    }
    /// Straight-alpha RGBA, as image decoders give it.
    pub fn from_rgba(width: u32, height: u32, data: &[u8]) -> Option<Self> {
        if (width as u64)
            .checked_mul(height as u64)
            .and_then(|n| n.checked_mul(4))
            != Some(data.len() as u64)
        {
            return None;
        }
        let mut pixmap = sk::Pixmap::new(width, height)?;
        for (dst, src) in pixmap
            .pixels_mut()
            .iter_mut()
            .zip(data.as_chunks::<4>().0.iter())
        {
            *dst = sk::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
        }
        Some(Self::from_pixmap(pixmap))
    }
    pub fn size(&self, _frame: usize) -> Size<DevicePixels> {
        size(
            DevicePixels(self.pixmap.width() as i32),
            DevicePixels(self.pixmap.height() as i32),
        )
    }
    /// The frame's pixels as straight-alpha BGRA, as GPUI stored images.
    pub fn with_bytes<R>(&self, frame: usize, f: impl FnOnce(Option<&[u8]>) -> R) -> R {
        if frame > 0 {
            return f(None);
        }
        let mut bgra = self.pixmap.data().to_vec();
        crate::graph::unpremultiply(&mut bgra);
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        f(Some(&bgra))
    }
    pub fn pixmap(&self) -> &sk::Pixmap {
        &self.pixmap
    }
    /// The image as Rune pixels (straight alpha).
    pub fn pixels(&self) -> rune::Pixels {
        self.pixels
            .lock()
            .expect("image pixels")
            .get_or_insert_with(|| {
                let mut rgba = self.pixmap.data().to_vec();
                crate::graph::unpremultiply(&mut rgba);
                let pixels = rune::Pixels::new();
                pixels.set(self.pixmap.width(), self.pixmap.height(), rgba);
                pixels
            })
            .clone()
    }
}

/// Where an image comes from, for `img()`.
#[derive(Clone)]
pub enum ImageSource {
    Path(std::path::PathBuf),
    Render(Arc<RenderImage>),
}
impl From<std::path::PathBuf> for ImageSource {
    fn from(p: std::path::PathBuf) -> Self {
        ImageSource::Path(p)
    }
}
impl From<Arc<RenderImage>> for ImageSource {
    fn from(i: Arc<RenderImage>) -> Self {
        ImageSource::Render(i)
    }
}
thread_local! {
    static FILE_IMAGES: RefCell<HashMap<std::path::PathBuf, Option<Arc<RenderImage>>>> = RefCell::new(HashMap::new());
}
impl ImageSource {
    /// The decoded image, decoding files once (synchronously) on first use.
    pub fn render_image(&self) -> Option<Arc<RenderImage>> {
        match self {
            ImageSource::Render(i) => Some(i.clone()),
            ImageSource::Path(path) => FILE_IMAGES.with(|cache| {
                let mut cache = cache.borrow_mut();
                if cache.len() > 64 {
                    cache.clear();
                }
                cache
                    .entry(path.clone())
                    .or_insert_with(|| {
                        let decoded = image::open(path).ok()?.to_rgba8();
                        RenderImage::from_rgba(decoded.width(), decoded.height(), decoded.as_raw())
                            .map(Arc::new)
                    })
                    .clone()
            }),
        }
    }
    pub fn forget(&self) {
        if let ImageSource::Path(path) = self {
            FILE_IMAGES.with(|cache| cache.borrow_mut().remove(path));
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ObjectFit {
    #[default]
    Contain,
    Cover,
    Fill,
    ScaleDown,
}

/// GPUI's `img()`: an image element.
pub fn img(source: impl Into<ImageSource>) -> Div {
    let source = source.into();
    match source.render_image() {
        Some(image) => {
            let pixels = image.pixels();
            div().el(move |e| e.pixels_fit(&pixels, rune::SurfaceFit::Fit))
        }
        None => div(),
    }
}

// ---------------------------------------------------------------- display list

/// What a canvas paints, in its own points (origin top-left). Quads and
/// images become Rune elements, composited on the GPU; images are uploaded
/// once and then only move. Paths, text and gradients are rasterized on the
/// CPU in runs no larger than their bounds, keeping the paint order.
pub enum Item {
    Quad {
        rect: [f32; 4],
        color: rune::Color,
        radii: [f32; 4],
        border: Option<(f32, rune::Color)>,
    },
    Image {
        pixels: rune::Pixels,
        id: u64,
        rect: [f32; 4],
        rotate: f32,
        clip: Option<[f32; 4]>,
    },
    Run {
        rect: [f32; 4],
        width: u32,
        height: u32,
        /// The run's pixels; `None` when last frame's run with the same
        /// `fingerprint` is reused without rasterizing it again.
        rgba: Option<Vec<u8>>,
        /// What the run paints, for runs made only of solid-colour paths.
        fingerprint: Option<u64>,
    },
    /// Polylines in the shape's own points, drawn opaque and faded as one
    /// layer, so overlapping segments of translucent ink blend once.
    Shape {
        shape: rune::Shape,
        opacity: f32,
        rect: [f32; 4],
        clip: Option<[f32; 4]>,
    },
}

/// How long a cached SVG's scale or rotation must rest before it is
/// rasterized again at the new one.
const SVG_SETTLE: std::time::Duration = std::time::Duration::from_millis(140);

struct SvgImage {
    tree: Arc<resvg::usvg::Tree>,
    /// The linear part of the transform the image was rasterized through.
    linear: sk::Transform,
    /// The image's top-left in that linear space.
    origin: (f32, f32),
    image: Arc<RenderImage>,
    /// The linear part last asked for, and when it last changed.
    wanted: sk::Transform,
    changed: std::time::Instant,
    frame: u64,
}
#[derive(Default)]
struct SvgCache {
    frame: u64,
    images: HashMap<usize, SvgImage>,
}
thread_local! {
    static SVG_CACHE: RefCell<SvgCache> = RefCell::new(SvgCache::default());
}

/// The cached image of `tree` drawn through `transform` (device pixels), and
/// the transform that places its pixels. None when it would be larger than
/// a few canvases (`area` device pixels each).
fn svg_image(
    tree: &Arc<resvg::usvg::Tree>,
    transform: sk::Transform,
    area: f32,
) -> Option<(Arc<RenderImage>, sk::Transform)> {
    let linear = sk::Transform::from_row(
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        0.,
        0.,
    );
    let now = std::time::Instant::now();
    SVG_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let frame = cache.frame;
        let key = Arc::as_ptr(tree) as usize;
        let entry = cache.images.get_mut(&key);
        let stale = match entry {
            Some(entry) => {
                if entry.wanted != linear {
                    entry.wanted = linear;
                    entry.changed = now;
                }
                entry.frame = frame;
                let ratio = (linear.sx * linear.sy - linear.kx * linear.ky).abs().sqrt()
                    / (entry.linear.sx * entry.linear.sy - entry.linear.kx * entry.linear.ky)
                        .abs()
                        .sqrt();
                entry.linear != linear
                    && (now.duration_since(entry.changed) >= SVG_SETTLE
                        || !(0.5..=2.).contains(&ratio))
            }
            None => true,
        };
        if stale {
            let bounds = tree
                .root()
                .abs_layer_bounding_box()
                .to_rect()
                .transform(linear)?;
            let (x0, y0) = (bounds.left().floor(), bounds.top().floor());
            let (w, h) = (bounds.right().ceil() - x0, bounds.bottom().ceil() - y0);
            if w < 1. || h < 1. || w * h > area * 4. {
                cache.images.remove(&key);
                return None;
            }
            let mut pixmap = sk::Pixmap::new(w as u32, h as u32)?;
            resvg::render(tree, linear.post_translate(-x0, -y0), &mut pixmap.as_mut());
            cache.images.insert(
                key,
                SvgImage {
                    tree: tree.clone(),
                    linear,
                    origin: (x0, y0),
                    image: Arc::new(RenderImage::from_pixmap(pixmap)),
                    wanted: linear,
                    changed: now,
                    frame,
                },
            );
        }
        let entry = cache.images.get(&key)?;
        let mut at = transform
            .pre_concat(entry.linear.invert()?)
            .pre_translate(entry.origin.0, entry.origin.1);
        // An image at its own scale lands on whole pixels, so text stays crisp.
        if (at.sx - 1.).abs() < 1e-4 && (at.sy - 1.).abs() < 1e-4 && at.kx == 0. && at.ky == 0. {
            at.tx = at.tx.round();
            at.ty = at.ty.round();
        }
        Some((entry.image.clone(), at))
    })
}

/// A CPU operation of a run, in device pixels of the whole canvas.
enum Op {
    Fill {
        path: Arc<sk::Path>,
        paint: sk::Paint<'static>,
        rule: sk::FillRule,
        transform: sk::Transform,
    },
    Stroke {
        path: Arc<sk::Path>,
        paint: sk::Paint<'static>,
        stroke: sk::Stroke,
        transform: sk::Transform,
    },
    Image {
        pixmap: Arc<sk::Pixmap>,
        quality: sk::FilterQuality,
        transform: sk::Transform,
    },
    Svg {
        tree: Arc<resvg::usvg::Tree>,
        transform: sk::Transform,
    },
}

/// A canvas being painted, in window points through `transform` (which maps
/// them to the canvas's device pixels). Clips are device-pixel rectangles.
pub struct Raster {
    pub transform: sk::Transform,
    width: f32,
    height: f32,
    scale: f32,
    clips: Vec<sk::Rect>,
    items: Vec<Item>,
    run: Vec<(Op, Option<sk::Rect>)>,
    run_bounds: Option<sk::Rect>,
    /// Fingerprints of the runs this canvas drew last frame.
    previous: std::collections::HashSet<u64>,
}
impl Raster {
    pub fn new(width: u32, height: u32, scale: f32, origin: Point<Pixels>) -> Option<Self> {
        Some(Self {
            transform: sk::Transform::from_scale(scale, scale)
                .pre_translate(-origin.x.0, -origin.y.0),
            width: width.max(1) as f32,
            height: height.max(1) as f32,
            scale,
            clips: Vec::new(),
            items: Vec::new(),
            run: Vec::new(),
            run_bounds: None,
            previous: Default::default(),
        })
    }
    fn full(&self) -> sk::Rect {
        sk::Rect::from_xywh(0., 0., self.width, self.height).expect("canvas rect")
    }
    /// The current clip, unless it leaves the whole canvas visible.
    fn clip(&self) -> Option<sk::Rect> {
        self.clips.last().copied().filter(|c| {
            !(c.left() <= 0.5
                && c.top() <= 0.5
                && c.right() >= self.width - 0.5
                && c.bottom() >= self.height - 0.5)
        })
    }
    /// Adds a CPU operation that paints within device `bounds` (`None`:
    /// anywhere) to the current run.
    fn record(&mut self, op: Op, bounds: Option<sk::Rect>) {
        let clip = self.clip();
        let area = clip.unwrap_or(self.full());
        let bounds = match bounds {
            Some(b) => b.outset(2., 2.).and_then(|b| b.intersect(&area)),
            None => Some(area),
        };
        let Some(bounds) = bounds else {
            return;
        };
        self.run_bounds = Some(match self.run_bounds {
            Some(r) => sk::Rect::from_ltrb(
                r.left().min(bounds.left()),
                r.top().min(bounds.top()),
                r.right().max(bounds.right()),
                r.bottom().max(bounds.bottom()),
            )
            .unwrap_or(r),
            None => bounds,
        });
        self.run.push((op, clip));
    }
    /// Rasterizes the pending run into an image of its bounds.
    fn flush(&mut self) {
        let ops = std::mem::take(&mut self.run);
        let Some(b) = self.run_bounds.take() else {
            return;
        };
        let (x0, y0) = (b.left().floor().max(0.), b.top().floor().max(0.));
        let (x1, y1) = (
            b.right().ceil().min(self.width),
            b.bottom().ceil().min(self.height),
        );
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let s = self.scale;
        let rect = [x0 / s, y0 / s, (x1 - x0) / s, (y1 - y0) / s];
        let fingerprint = run_fingerprint(&ops, [x0, y0, x1, y1]);
        // Each of last frame's runs is reused once; an identical second run is painted.
        if let Some(fingerprint) = fingerprint.filter(|f| self.previous.remove(f)) {
            self.items.push(Item::Run {
                rect,
                width: w,
                height: h,
                rgba: None,
                fingerprint: Some(fingerprint),
            });
            return;
        }
        let Some(mut pixmap) = sk::Pixmap::new(w, h) else {
            return;
        };
        let shift = |t: sk::Transform| t.post_translate(-x0, -y0);
        let whole = sk::Rect::from_xywh(0., 0., w as f32, h as f32).expect("run rect");
        let mut mask: Option<(sk::Rect, sk::Mask)> = None;
        for (op, clip) in ops {
            let clip = clip
                .and_then(|c| {
                    sk::Rect::from_ltrb(
                        c.left() - x0,
                        c.top() - y0,
                        c.right() - x0,
                        c.bottom() - y0,
                    )
                })
                .filter(|c| {
                    !(c.left() <= 0.
                        && c.top() <= 0.
                        && c.right() >= whole.right()
                        && c.bottom() >= whole.bottom())
                });
            if let Some(c) = clip
                && mask.as_ref().is_none_or(|(r, _)| *r != c)
                && let Some(mut m) = sk::Mask::new(w, h)
            {
                m.fill_path(
                    &sk::PathBuilder::from_rect(c),
                    sk::FillRule::Winding,
                    true,
                    sk::Transform::identity(),
                );
                mask = Some((c, m));
            }
            let mask = clip.and(mask.as_ref().map(|(_, m)| m));
            match op {
                Op::Fill {
                    path,
                    paint,
                    rule,
                    transform,
                } => pixmap.fill_path(&path, &paint, rule, shift(transform), mask),
                Op::Stroke {
                    path,
                    paint,
                    stroke,
                    transform,
                } => pixmap.stroke_path(&path, &paint, &stroke, shift(transform), mask),
                Op::Image {
                    pixmap: image,
                    quality,
                    transform,
                } => {
                    let paint = sk::PixmapPaint {
                        quality,
                        ..Default::default()
                    };
                    pixmap.draw_pixmap(
                        0,
                        0,
                        image.as_ref().as_ref(),
                        &paint,
                        shift(transform),
                        mask,
                    )
                }
                Op::Svg { tree, transform } => {
                    resvg::render(&tree, shift(transform), &mut pixmap.as_mut())
                }
            }
        }
        let mut rgba = pixmap.take();
        crate::graph::unpremultiply(&mut rgba);
        self.items.push(Item::Run {
            rect,
            width: w,
            height: h,
            rgba: Some(rgba),
            fingerprint,
        });
    }
    /// Everything painted, in order.
    pub fn finish(mut self) -> Vec<Item> {
        self.flush();
        self.items
    }
    pub fn fill_path(
        &mut self,
        path: &Arc<sk::Path>,
        color: Rgba,
        rule: FillRule,
        transform: sk::Transform,
    ) {
        let mut paint = sk::Paint::default();
        paint.set_color(color.skia());
        paint.anti_alias = true;
        self.fill_with(path, paint, rule, transform);
    }
    fn fill_with(
        &mut self,
        path: &Arc<sk::Path>,
        paint: sk::Paint<'static>,
        rule: FillRule,
        transform: sk::Transform,
    ) {
        let rule = match rule {
            FillRule::NonZero => sk::FillRule::Winding,
            FillRule::EvenOdd => sk::FillRule::EvenOdd,
        };
        let Some(bounds) = path.bounds().transform(transform) else {
            return;
        };
        self.record(
            Op::Fill {
                path: path.clone(),
                paint,
                rule,
                transform,
            },
            Some(bounds),
        );
    }
    pub fn stroke_path(
        &mut self,
        path: &Arc<sk::Path>,
        color: Rgba,
        width: f32,
        dash: Option<&[f32]>,
        transform: sk::Transform,
    ) {
        let mut paint = sk::Paint::default();
        paint.set_color(color.skia());
        paint.anti_alias = true;
        let stroke = sk::Stroke {
            width,
            line_cap: sk::LineCap::Round,
            line_join: sk::LineJoin::Round,
            dash: dash.and_then(|d| sk::StrokeDash::new(d.to_vec(), 0.)),
            ..Default::default()
        };
        let Some(bounds) = path
            .bounds()
            .outset(width, width)
            .and_then(|b| b.transform(transform))
        else {
            return;
        };
        self.record(
            Op::Stroke {
                path: path.clone(),
                paint,
                stroke,
                transform,
            },
            Some(bounds),
        );
    }
    /// Draws an SVG tree through `extra` (in window points). A `cached` tree
    /// (one kept while its content is unchanged, as text layouts are) is
    /// rasterized once into an image the GPU moves; after its scale or
    /// rotation changes, the old image is stretched until the change has
    /// rested for [`SVG_SETTLE`].
    pub fn render_svg(&mut self, tree: Arc<resvg::usvg::Tree>, extra: sk::Transform, cached: bool) {
        let transform = self.transform.pre_concat(extra);
        if cached && let Some((image, at)) = svg_image(&tree, transform, self.width * self.height) {
            let (w, h) = (image.pixmap.width() as f32, image.pixmap.height() as f32);
            self.draw_image(
                &image,
                Bounds::new(point(px(0.), px(0.)), size(px(w), px(h))),
                at,
            );
            return;
        }
        let bounds = tree
            .root()
            .abs_layer_bounding_box()
            .to_rect()
            .transform(transform);
        self.record(Op::Svg { tree, transform }, bounds);
    }
    /// Round-capped polylines (window points, widths in canvas points) as
    /// one GPU shape.
    pub fn polylines(&mut self, lines: Vec<(Vec<[f32; 2]>, f32)>, color: Rgba) {
        let s = self.scale;
        let t = self.transform;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut mapped = Vec::with_capacity(lines.len());
        for (points, width) in lines {
            let pad = width / 2. + 1.;
            let points = points
                .into_iter()
                .map(|[x, y]| {
                    let mut p = sk::Point::from_xy(x, y);
                    t.map_point(&mut p);
                    let p = [p.x / s, p.y / s];
                    x0 = x0.min(p[0] - pad);
                    y0 = y0.min(p[1] - pad);
                    x1 = x1.max(p[0] + pad);
                    y1 = y1.max(p[1] + pad);
                    p
                })
                .collect::<Vec<_>>();
            mapped.push((points, width));
        }
        if x1 <= x0 || y1 <= y0 || !x0.is_finite() || !y1.is_finite() {
            return;
        }
        let area = self.clips.last().copied().unwrap_or(self.full());
        let dest = sk::Rect::from_ltrb(x0 * s, y0 * s, x1 * s, y1 * s);
        let Some(dest) = dest.filter(|d| d.intersect(&area).is_some()) else {
            return;
        };
        self.flush();
        let opaque = Rgba { a: 1., ..color };
        let mut shape = rune::Shape::new();
        for (points, width) in mapped {
            let points = points
                .into_iter()
                .map(|[x, y]| [x - x0, y - y0])
                .collect::<Vec<_>>();
            shape = shape.stroke_with(points, false, width, rune::LineCap::Round, opaque.color());
        }
        let clip = self.clip().filter(|c| {
            !(dest.left() >= c.left()
                && dest.top() >= c.top()
                && dest.right() <= c.right()
                && dest.bottom() <= c.bottom())
        });
        self.items.push(Item::Shape {
            shape,
            opacity: color.a.clamp(0., 1.),
            rect: [x0, y0, x1 - x0, y1 - y0],
            clip: clip.map(|c| [c.left() / s, c.top() / s, c.width() / s, c.height() / s]),
        });
    }
    /// A quad as a GPU element. False when it needs the CPU (a rounded or
    /// bordered quad cut by a clip).
    fn quad(
        &mut self,
        bounds: Bounds<Pixels>,
        color: Rgba,
        radii: [f32; 4],
        border: Option<(f32, Rgba)>,
    ) -> bool {
        let t = self.transform;
        if t.kx != 0. || t.ky != 0. {
            return false;
        }
        let Some(mut dest) = sk::Rect::from_xywh(
            bounds.origin.x.0,
            bounds.origin.y.0,
            bounds.size.width.0,
            bounds.size.height.0,
        )
        .and_then(|r| r.transform(t)) else {
            return true;
        };
        if let Some(clip) = self.clips.last() {
            let inside = dest.left() >= clip.left()
                && dest.top() >= clip.top()
                && dest.right() <= clip.right()
                && dest.bottom() <= clip.bottom();
            if !inside {
                if radii != [0.; 4] || border.is_some() {
                    return dest.intersect(clip).is_none();
                }
                match dest.intersect(clip) {
                    Some(r) => dest = r,
                    None => return true,
                }
            }
        }
        if color.a <= 0. && border.is_none() {
            return true;
        }
        self.flush();
        let s = self.scale;
        let rect = [
            dest.left() / s,
            dest.top() / s,
            dest.width() / s,
            dest.height() / s,
        ];
        let max = rect[2].min(rect[3]) / 2.;
        self.items.push(Item::Quad {
            rect,
            color: color.color(),
            radii: radii.map(|r| r.clamp(0., max)),
            border: border.map(|(w, c)| (w, c.color())),
        });
        true
    }
    pub fn push_clip(&mut self, bounds: Bounds<Pixels>) {
        let rect = sk::Rect::from_xywh(
            bounds.origin.x.0,
            bounds.origin.y.0,
            bounds.size.width.0.max(0.),
            bounds.size.height.0.max(0.),
        )
        .and_then(|r| r.transform(self.transform));
        let parent = self.clips.last().copied().unwrap_or(self.full());
        let clip = match rect {
            Some(r) => r
                .intersect(&parent)
                .unwrap_or(sk::Rect::from_xywh(0., 0., 0.1, 0.1).unwrap()),
            None => parent,
        };
        self.clips.push(clip);
    }
    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }
    /// Draws `image` stretched over `bounds` through `transform`: as a GPU
    /// element when the transform keeps it a rectangle, else on the CPU.
    fn draw_image(
        &mut self,
        image: &RenderImage,
        bounds: Bounds<Pixels>,
        transform: sk::Transform,
    ) {
        let (w, h) = (image.pixmap.width() as f32, image.pixmap.height() as f32);
        if w <= 0. || h <= 0. {
            return;
        }
        let t = transform.pre_concat(sk::Transform::from_row(
            bounds.size.width.0 / w,
            0.,
            0.,
            bounds.size.height.0 / h,
            bounds.origin.x.0,
            bounds.origin.y.0,
        ));
        let Some(dest) = sk::Rect::from_xywh(0., 0., w, h).and_then(|r| r.transform(t)) else {
            return;
        };
        let area = self.clips.last().copied().unwrap_or(self.full());
        if dest.intersect(&area).is_none() {
            return;
        }
        // Image axes in points: they must stay perpendicular and unflipped.
        let s = self.scale;
        let (ux, uy) = (t.sx * w / s, t.ky * w / s);
        let (vx, vy) = (t.kx * h / s, t.sy * h / s);
        let (uw, vh) = (ux.hypot(uy), vx.hypot(vy));
        let det = ux * vy - uy * vx;
        if det <= 0. || (ux * vx + uy * vy).abs() > 1e-3 * uw * vh {
            let quality = sk::FilterQuality::Bilinear;
            self.record(
                Op::Image {
                    pixmap: image.pixmap.clone(),
                    quality,
                    transform: t,
                },
                Some(dest),
            );
            return;
        }
        self.flush();
        let mut centre = sk::Point::from_xy(w / 2., h / 2.);
        t.map_point(&mut centre);
        let (cx, cy) = (centre.x / s, centre.y / s);
        let clip = self.clip().filter(|c| {
            !(dest.left() >= c.left()
                && dest.top() >= c.top()
                && dest.right() <= c.right()
                && dest.bottom() <= c.bottom())
        });
        self.items.push(Item::Image {
            pixels: image.pixels(),
            id: image.id,
            rect: [cx - uw / 2., cy - vh / 2., uw, vh],
            rotate: uy.atan2(ux),
            clip: clip.map(|c| [c.left() / s, c.top() / s, c.width() / s, c.height() / s]),
        });
    }
}

fn rounded_rect(b: Bounds<Pixels>, r: Corners<Pixels>) -> Option<sk::Path> {
    let (x, y, w, h) = (b.origin.x.0, b.origin.y.0, b.size.width.0, b.size.height.0);
    if w <= 0. || h <= 0. {
        return None;
    }
    let max = w.min(h) / 2.;
    let [tl, tr, br, bl] = [
        r.top_left.0,
        r.top_right.0,
        r.bottom_right.0,
        r.bottom_left.0,
    ]
    .map(|v| v.clamp(0., max));
    if tl == 0. && tr == 0. && br == 0. && bl == 0. {
        return Some(sk::PathBuilder::from_rect(sk::Rect::from_xywh(x, y, w, h)?));
    }
    const K: f32 = 0.5523;
    let mut p = sk::PathBuilder::new();
    p.move_to(x + tl, y);
    p.line_to(x + w - tr, y);
    p.cubic_to(
        x + w - tr * (1. - K),
        y,
        x + w,
        y + tr * (1. - K),
        x + w,
        y + tr,
    );
    p.line_to(x + w, y + h - br);
    p.cubic_to(
        x + w,
        y + h - br * (1. - K),
        x + w - br * (1. - K),
        y + h,
        x + w - br,
        y + h,
    );
    p.line_to(x + bl, y + h);
    p.cubic_to(
        x + bl * (1. - K),
        y + h,
        x,
        y + h - bl * (1. - K),
        x,
        y + h - bl,
    );
    p.line_to(x, y + tl);
    p.cubic_to(x, y + tl * (1. - K), x + tl * (1. - K), y, x + tl, y);
    p.close();
    p.finish()
}

impl Window {
    pub fn paint_path<P>(&mut self, path: Path<P>, color: impl Into<Rgba>) {
        let color = color.into();
        let Some(raster) = self.raster.as_mut() else {
            return;
        };
        let Some(p) = path.resolved() else {
            return;
        };
        let t = raster.transform;
        match path.stroke {
            Some(width) => raster.stroke_path(&p, color, width, path.dash.as_deref(), t),
            None => raster.fill_path(&p, color, path.rule, t),
        }
    }
    /// Round-capped polylines in window points, each with its width.
    pub fn paint_polylines(&mut self, lines: Vec<(Vec<[f32; 2]>, f32)>, color: Rgba) {
        if let Some(raster) = self.raster.as_mut() {
            raster.polylines(lines, color);
        }
    }
    pub fn paint_quad(&mut self, quad: PaintQuad) {
        let Some(raster) = self.raster.as_mut() else {
            return;
        };
        let t = raster.transform;
        let c = quad.corner_radii;
        let radii = [
            c.top_left.0,
            c.top_right.0,
            c.bottom_right.0,
            c.bottom_left.0,
        ];
        let width = quad.border_widths.top.0;
        let border = (width > 0. && quad.border_color.a > 0.).then_some((width, quad.border_color));
        let dashed = border.is_some() && quad.border_style == BorderStyle::Dashed;
        if quad.gradient.is_none()
            && !dashed
            && raster.quad(quad.bounds, quad.background, radii, border)
        {
            return;
        }
        if quad.background.a > 0.
            && let Some(path) = rounded_rect(quad.bounds, quad.corner_radii)
        {
            raster.fill_path(&Arc::new(path), quad.background, FillRule::NonZero, t);
        }
        if let Some(g) = quad.gradient
            && let Some(path) = rounded_rect(quad.bounds, quad.corner_radii)
        {
            // CSS angles: 0° points up, 90° right.
            let b = quad.bounds;
            let (cx, cy) = (
                b.origin.x.0 + b.size.width.0 / 2.,
                b.origin.y.0 + b.size.height.0 / 2.,
            );
            let a = g.angle.to_radians();
            let (dx, dy) = (a.sin(), -a.cos());
            let half = (b.size.width.0 * dx.abs() + b.size.height.0 * dy.abs()) / 2.;
            let start = sk::Point::from_xy(cx - dx * half, cy - dy * half);
            let end = sk::Point::from_xy(cx + dx * half, cy + dy * half);
            if let Some(shader) = sk::LinearGradient::new(
                start,
                end,
                vec![
                    sk::GradientStop::new(g.from.1, g.from.0.skia()),
                    sk::GradientStop::new(g.to.1, g.to.0.skia()),
                ],
                sk::SpreadMode::Pad,
                sk::Transform::identity(),
            ) {
                let paint = sk::Paint {
                    shader,
                    anti_alias: true,
                    ..Default::default()
                };
                raster.fill_with(&Arc::new(path), paint, FillRule::NonZero, t);
            }
        }
        if let Some((width, color)) = border {
            let inset = width / 2.;
            let b = Bounds::new(
                point(
                    quad.bounds.origin.x + px(inset),
                    quad.bounds.origin.y + px(inset),
                ),
                size(
                    quad.bounds.size.width - px(width),
                    quad.bounds.size.height - px(width),
                ),
            );
            if let Some(path) = rounded_rect(b, quad.corner_radii) {
                let dash = dashed.then(|| vec![4., 3.]);
                raster.stroke_path(&Arc::new(path), color, width, dash.as_deref(), t);
            }
        }
    }
    pub fn paint_image(
        &mut self,
        bounds: Bounds<Pixels>,
        _corners: Corners<Pixels>,
        image: Arc<RenderImage>,
        _frame: usize,
        _grayscale: bool,
    ) -> Result<(), ()> {
        let raster = self.raster.as_mut().ok_or(())?;
        let t = raster.transform;
        raster.draw_image(&image, bounds, t);
        Ok(())
    }
    pub fn paint_image_transformed(
        &mut self,
        bounds: Bounds<Pixels>,
        _corners: Corners<Pixels>,
        image: Arc<RenderImage>,
        _frame: usize,
        _grayscale: bool,
        transformation: TransformationMatrix,
    ) -> Result<(), ()> {
        let raster = self.raster.as_mut().ok_or(())?;
        let t = raster.transform.pre_concat(transformation.skia());
        raster.draw_image(&image, bounds, t);
        Ok(())
    }
    pub fn with_content_mask<R>(
        &mut self,
        mask: Option<ContentMask>,
        f: impl FnOnce(&mut Window) -> R,
    ) -> R {
        let pushed = match (mask, self.raster.as_mut()) {
            (Some(mask), Some(raster)) => {
                raster.push_clip(mask.bounds);
                true
            }
            _ => false,
        };
        let result = f(self);
        if pushed && let Some(raster) = self.raster.as_mut() {
            raster.pop_clip();
        }
        result
    }
    pub fn drop_image(&mut self, _: Arc<RenderImage>) -> Result<(), ()> {
        Ok(())
    }
}

// ---------------------------------------------------------------- canvas

thread_local! {
    static CANVAS_RECTS: RefCell<HashMap<String, [f32; 4]>> = RefCell::new(HashMap::new());
    static CANVAS_RUNS: RefCell<HashMap<String, Vec<RunSlot>>> = RefCell::new(HashMap::new());
    static CANVAS_SITES: RefCell<HashMap<(&'static str, u32, u32), usize>> = RefCell::new(HashMap::new());
    static FRAME_SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.) };
}
/// Starts a frame's canvases: call sites number their canvases afresh, and
/// paint at `scale` device pixels per point.
pub fn begin_frame(scale: f32) {
    CANVAS_SITES.with(|s| s.borrow_mut().clear());
    SVG_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let frame = cache.frame;
        // Images not drawn last frame are released.
        cache.images.retain(|_, image| image.frame + 1 >= frame);
        cache.frame += 1;
    });
    FRAME_SCALE.with(|s| s.set(scale));
}
/// A canvas's window rect as last laid out.
pub fn canvas_rect(key: &str) -> Option<[f32; 4]> {
    CANVAS_RECTS.with(|r| r.borrow().get(key).copied())
}

/// The image a canvas shows for one CPU run, kept between frames so an
/// unchanged run is not uploaded again.
struct RunSlot {
    pixels: rune::Pixels,
    size: (u32, u32),
    /// The fingerprint of what the slot shows, or, for a run that has none,
    /// its pixels, so an unchanged run is not uploaded again.
    content: RunContent,
}
#[derive(PartialEq)]
enum RunContent {
    Fingerprint(u64),
    Pixels(Vec<u8>),
}

/// A hash of what a run of solid-colour fills and strokes paints, or `None`
/// when it holds images, SVG, gradients or dashes (which are compared by
/// their pixels instead).
fn run_fingerprint(ops: &[(Op, Option<sk::Rect>)], bounds: [f32; 4]) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let floats = |h: &mut std::collections::hash_map::DefaultHasher, values: &[f32]| {
        for v in values {
            v.to_bits().hash(h);
        }
    };
    let transform = |h: &mut std::collections::hash_map::DefaultHasher, t: &sk::Transform| {
        floats(h, &[t.sx, t.kx, t.ky, t.sy, t.tx, t.ty]);
    };
    let path = |h: &mut std::collections::hash_map::DefaultHasher, path: &sk::Path| {
        for p in path.points() {
            floats(h, &[p.x, p.y]);
        }
        for verb in path.verbs() {
            (*verb as u8).hash(h);
        }
    };
    let paint = |h: &mut std::collections::hash_map::DefaultHasher, paint: &sk::Paint| {
        let sk::Shader::SolidColor(color) = paint.shader else {
            return false;
        };
        floats(
            h,
            &[color.red(), color.green(), color.blue(), color.alpha()],
        );
        (paint.anti_alias, paint.blend_mode as u8).hash(h);
        true
    };
    floats(&mut h, &bounds);
    for (op, clip) in ops {
        if let Some(c) = clip {
            floats(&mut h, &[c.left(), c.top(), c.right(), c.bottom()]);
        }
        match op {
            Op::Fill {
                path: p,
                paint: q,
                rule,
                transform: t,
            } => {
                0u8.hash(&mut h);
                path(&mut h, p);
                if !paint(&mut h, q) {
                    return None;
                }
                (*rule as u8).hash(&mut h);
                transform(&mut h, t);
            }
            Op::Stroke {
                path: p,
                paint: q,
                stroke,
                transform: t,
            } => {
                if stroke.dash.is_some() {
                    return None;
                }
                1u8.hash(&mut h);
                path(&mut h, p);
                if !paint(&mut h, q) {
                    return None;
                }
                floats(&mut h, &[stroke.width, stroke.miter_limit]);
                (stroke.line_cap as u8, stroke.line_join as u8).hash(&mut h);
                transform(&mut h, t);
            }
            Op::Image { .. } | Op::Svg { .. } => return None,
        }
    }
    Some(h.finish())
}

/// Paints a canvas of window rect `[x, y, w, h]` and returns its elements,
/// which fill the canvas element.
fn paint_canvas(
    key: &str,
    [x, y, w, h]: [f32; 4],
    window: &mut Window,
    paint: impl FnOnce(Bounds<Pixels>, &mut Window),
) -> Option<rune::El> {
    if w < 1. || h < 1. {
        return None;
    }
    let scale = FRAME_SCALE.with(|s| s.get()).max(0.5);
    let bounds = Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
    window.raster = Raster::new(
        (w * scale).ceil() as u32,
        (h * scale).ceil() as u32,
        scale,
        bounds.origin,
    );
    if let Some(raster) = &mut window.raster {
        raster.previous = CANVAS_RUNS.with(|runs| {
            runs.borrow()
                .get(key)
                .into_iter()
                .flatten()
                .filter_map(|slot| match slot.content {
                    RunContent::Fingerprint(f) => Some(f),
                    RunContent::Pixels(_) => None,
                })
                .collect()
        });
    }
    paint(bounds, window);
    let items = window.raster.take()?.finish();
    Some(display_list(key, items))
}

/// Whether `FOLIO_PROFILE_CANVAS` asks for canvas and view build timings, read once.
pub(crate) fn profiling() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("FOLIO_PROFILE_CANVAS").is_some())
}

/// Rune elements for a canvas's items, in paint order.
fn display_list(key: &str, items: Vec<Item>) -> rune::El {
    let mut children = Vec::with_capacity(items.len());
    let mut images: HashMap<u64, u32> = HashMap::new();
    let (mut quads, mut runs, mut shapes) = (0usize, 0usize, 0usize);
    CANVAS_RUNS.with(|slots| {
        let mut slots = slots.borrow_mut();
        let slots = slots.entry(key.to_owned()).or_default();
        let mut previous: Vec<Option<RunSlot>> =
            std::mem::take(slots).into_iter().map(Some).collect();
        let mut kept_slots = Vec::with_capacity(previous.len());
        for item in items {
            match item {
                Item::Quad {
                    rect,
                    color,
                    radii,
                    border,
                } => {
                    let mut e = rune::el()
                        .key(format!("q{quads}"))
                        .absolute(rect[0], rect[1])
                        .size(rect[2], rect[3])
                        .fill(color);
                    if radii != [0.; 4] {
                        e = e.corners(radii);
                    }
                    if let Some((width, color)) = border {
                        e = e.border(width, color);
                    }
                    quads += 1;
                    children.push(e);
                }
                Item::Image {
                    pixels,
                    id,
                    rect,
                    rotate,
                    clip,
                } => {
                    let n = images.entry(id).or_insert(0);
                    let key = format!("i{id}-{n}");
                    *n += 1;
                    let mut e = rune::el().pixels(&pixels).size(rect[2], rect[3]);
                    if rotate != 0. {
                        e = e.rotate(rotate);
                    }
                    children.push(match clip {
                        None => e.key(key).absolute(rect[0], rect[1]),
                        Some(c) => rune::el()
                            .key(key)
                            .absolute(c[0], c[1])
                            .size(c[2], c[3])
                            .clip_rounded(0.)
                            .child(e.absolute(rect[0] - c[0], rect[1] - c[1])),
                    });
                }
                Item::Shape {
                    shape,
                    opacity,
                    rect,
                    clip,
                } => {
                    let key = format!("s{shapes}");
                    shapes += 1;
                    let mut e = rune::canvas(shape).size(rect[2], rect[3]);
                    if opacity < 1. {
                        e = e.opacity(opacity);
                    }
                    children.push(match clip {
                        None => e.key(key).absolute(rect[0], rect[1]),
                        Some(c) => rune::el()
                            .key(key)
                            .absolute(c[0], c[1])
                            .size(c[2], c[3])
                            .clip_rounded(0.)
                            .child(e.absolute(rect[0] - c[0], rect[1] - c[1])),
                    });
                }
                Item::Run {
                    rect,
                    width,
                    height,
                    rgba,
                    fingerprint,
                } => {
                    let size = (width, height);
                    // A run painted as last frame keeps its slot and image.
                    let kept = fingerprint.and_then(|f| {
                        previous.iter().position(|slot| {
                            slot.as_ref().is_some_and(|slot| {
                                slot.size == size && slot.content == RunContent::Fingerprint(f)
                            })
                        })
                    });
                    let slot = match (kept, rgba) {
                        (Some(i), _) => previous[i].take().expect("kept run slot"),
                        (None, Some(rgba)) => {
                            let mut slot = previous
                                .get_mut(runs)
                                .and_then(Option::take)
                                .unwrap_or_else(|| RunSlot {
                                    pixels: rune::Pixels::new(),
                                    size: (0, 0),
                                    content: RunContent::Pixels(Vec::new()),
                                });
                            match fingerprint {
                                Some(f) => {
                                    slot.pixels.set(width, height, rgba);
                                    slot.content = RunContent::Fingerprint(f);
                                }
                                None => {
                                    let unchanged = slot.size == size
                                        && matches!(&slot.content, RunContent::Pixels(p) if *p == rgba);
                                    if !unchanged {
                                        slot.pixels.set(width, height, rgba.clone());
                                        slot.content = RunContent::Pixels(rgba);
                                    }
                                }
                            }
                            slot.size = size;
                            slot
                        }
                        // Reused, but its slot is gone: nothing to show.
                        (None, None) => continue,
                    };
                    children.push(
                        rune::el()
                            .key(format!("r{runs}"))
                            .absolute(rect[0], rect[1])
                            .size(rect[2], rect[3])
                            .pixels(&slot.pixels),
                    );
                    kept_slots.push(slot);
                    runs += 1;
                }
            }
        }
        *slots = kept_slots;
    });
    rune::el()
        .pin(Some(0.), Some(0.), Some(0.), Some(0.))
        .clip_rounded(0.)
        .instant_changes()
        .children(children)
}

type Paint = Box<dyn FnOnce(Bounds<Pixels>, &mut Window, &mut Context<crate::NotesView>)>;

/// GPUI's `canvas(prepaint, paint)`: an element painted by closures, here
/// into a pixmap the size of its last layout.
pub struct Canvas {
    key: String,
    paint: Paint,
    style: Div,
}
#[track_caller]
pub fn canvas<T: 'static>(
    prepaint: impl FnOnce(Bounds<Pixels>, &mut Window, &mut Context<crate::NotesView>) -> T + 'static,
    paint: impl FnOnce(Bounds<Pixels>, T, &mut Window, &mut Context<crate::NotesView>) + 'static,
) -> Canvas {
    let site = std::panic::Location::caller();
    let id = (site.file(), site.line(), site.column());
    let n = CANVAS_SITES.with(|s| {
        let mut s = s.borrow_mut();
        let n = s.entry(id).or_insert(0);
        *n += 1;
        *n
    });
    Canvas {
        key: format!(
            "canvas-{}-{}-{}-{n}",
            site.line(),
            site.column(),
            site.file().len()
        ),
        paint: Box::new(move |bounds, window, cx| {
            let state = prepaint(bounds, window, cx);
            paint(bounds, state, window, cx);
        }),
        style: div(),
    }
}
macro_rules! canvas_style {
    ($($name:ident($($arg:ident: $ty:ty),*)),* $(,)?) => {
        $(pub fn $name(mut self, $($arg: $ty),*) -> Self {
            self.style = self.style.$name($($arg),*);
            self
        })*
    };
}
impl Canvas {
    canvas_style!(
        absolute(),
        inset_0(),
        size_full(),
        w_full(),
        h_full(),
        flex_1(),
        flex_shrink_0(),
        top(v: Pixels),
        left(v: Pixels),
        right(v: Pixels),
        bottom(v: Pixels),
    );
    pub fn w(mut self, v: impl Into<Length>) -> Self {
        self.style = self.style.w(v);
        self
    }
    pub fn h(mut self, v: impl Into<Length>) -> Self {
        self.style = self.style.h(v);
        self
    }
    pub fn size(mut self, v: impl Into<Length> + Copy) -> Self {
        self.style = self.style.size(v);
        self
    }
    /// Paints now, with the bounds of the canvas's last layout.
    pub fn paint_now(self) -> Div {
        let key = self.key.clone();
        let scale = FRAME_SCALE.with(|s| s.get()).max(0.5);
        let mut window = Window::paint(scale);
        let paint = self.paint;
        let painted = canvas_rect(&key).and_then(|rect| {
            paint_canvas(&key, rect, &mut window, |bounds, window| {
                paint(bounds, window, &mut Context::fresh())
            })
        });
        let layout_key = key.clone();
        self.style
            .id(key)
            .children(painted.map(AnyElement::El))
            .el(move |e| {
                e.on_layout(move |_: &mut crate::AppState, r: [f32; 4]| {
                    CANVAS_RECTS.with(|rects| {
                        rects.borrow_mut().insert(layout_key.clone(), r);
                    });
                })
            })
    }
}
impl IntoElement for Canvas {
    fn into_any_element(self) -> AnyElement {
        self.paint_now().into_any_element()
    }
}

/// A canvas painted by the view itself: `paint` gets the view and the
/// canvas's last laid-out bounds (window points) while the frame is built.
pub fn view_canvas(
    key: &'static str,
    paint: impl FnOnce(
        &mut crate::NotesView,
        Bounds<Pixels>,
        &mut Window,
        &mut Context<crate::NotesView>,
    ) + 'static,
) -> Div {
    div()
        .id(key)
        .size_full()
        .child(AnyElement::Lazy(Box::new(move |bcx: &mut BuildCx| {
            let Some(rect) = canvas_rect(key) else {
                return AnyElement::Empty;
            };
            let started = std::time::Instant::now();
            let mut window = Window::render(bcx.view);
            let mut painted = std::time::Duration::ZERO;
            let elements = paint_canvas(key, rect, &mut window, |bounds, window| {
                paint(bcx.view, bounds, window, bcx.cx);
                painted = started.elapsed();
            });
            if profiling() {
                eprintln!(
                    "FOLIO_CANVAS {key}: paint {:.2} ms, total {:.2} ms",
                    painted.as_secs_f64() * 1000.,
                    started.elapsed().as_secs_f64() * 1000.
                );
            }
            bcx.cx.effects.extend(window.effects);
            elements.map_or(AnyElement::Empty, AnyElement::El)
        })))
        .el(move |e| {
            e.on_layout(move |_: &mut crate::AppState, r: [f32; 4]| {
                CANVAS_RECTS.with(|rects| {
                    rects.borrow_mut().insert(key.to_owned(), r);
                });
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paints a disc of `color` and returns the canvas's runs as
    /// `(has pixels, fingerprint)`, after last frame's `previous` runs.
    fn runs(color: u32, previous: &[u64]) -> Vec<(bool, Option<u64>)> {
        let mut raster = Raster::new(64, 64, 1., point(px(0.), px(0.))).unwrap();
        raster.previous = previous.iter().copied().collect();
        let disc = Arc::new(sk::PathBuilder::from_circle(32., 32., 10.).unwrap());
        raster.fill_path(&disc, rgb(color), FillRule::NonZero, raster.transform);
        raster
            .finish()
            .into_iter()
            .filter_map(|item| match item {
                Item::Run {
                    rgba, fingerprint, ..
                } => Some((rgba.is_some(), fingerprint)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn unchanged_runs_are_reused_without_rasterizing() {
        let first = runs(0x336699, &[]);
        let [(true, Some(fingerprint))] = first[..] else {
            panic!("{first:?}")
        };
        assert_eq!(runs(0x336699, &[fingerprint]), [(false, Some(fingerprint))]);
        // A different colour paints again.
        let changed = runs(0x993366, &[fingerprint]);
        assert!(changed[0].0 && changed[0].1 != Some(fingerprint));
    }
}
