//! GPUI's immediate-mode painting (`canvas`, `paint_path`, `paint_quad`,
//! `paint_image`) on a CPU rasterizer. Each canvas paints into a tiny-skia
//! pixmap at the window's device scale, which Rune draws as an image.
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
        let (cx, cy) = (
            x0 + dx - sign * h * dy / len,
            y0 + dy + sign * h * dx / len,
        );
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
    quad(bounds, Corners::default(), Rgba::default(), px(1.), color, style)
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
    pixmap: Arc<sk::Pixmap>,
    pixels: std::sync::Mutex<Option<rune::Pixels>>,
}
impl RenderImage {
    pub fn from_pixmap(pixmap: sk::Pixmap) -> Self {
        Self {
            pixmap: Arc::new(pixmap),
            pixels: std::sync::Mutex::new(None),
        }
    }
    fn from_bgra_straight(width: u32, height: u32, data: &[u8]) -> Option<Self> {
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
    pub fn from_ephemeral_shared_bgra(width: u32, height: u32, data: impl AsRef<[u8]>) -> Option<Self> {
        Self::from_bgra_straight(width, height, data.as_ref())
    }
    pub fn from_shared_bgra(width: u32, height: u32, data: impl AsRef<[u8]>) -> Option<Self> {
        Self::from_bgra_straight(width, height, data.as_ref())
    }
    /// Straight-alpha RGBA, as image decoders give it.
    pub fn from_rgba(width: u32, height: u32, data: &[u8]) -> Option<Self> {
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
    pub fn size(&self) -> Size<u32> {
        size(self.pixmap.width(), self.pixmap.height())
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

// ---------------------------------------------------------------- raster

/// A pixmap being painted, in window points through `transform`.
pub struct Raster {
    pub pixmap: sk::Pixmap,
    pub transform: sk::Transform,
    clips: Vec<sk::Mask>,
    scale: f32,
}
impl Raster {
    pub fn new(width: u32, height: u32, scale: f32, origin: Point<Pixels>) -> Option<Self> {
        Some(Self {
            pixmap: sk::Pixmap::new(width.max(1), height.max(1))?,
            transform: sk::Transform::from_scale(scale, scale)
                .pre_translate(-origin.x.0, -origin.y.0),
            clips: Vec::new(),
            scale,
        })
    }
    fn mask(&self) -> Option<&sk::Mask> {
        self.clips.last()
    }
    pub fn fill_path(&mut self, path: &sk::Path, color: Rgba, rule: FillRule, transform: sk::Transform) {
        let mut paint = sk::Paint::default();
        paint.set_color(color.skia());
        paint.anti_alias = true;
        let rule = match rule {
            FillRule::NonZero => sk::FillRule::Winding,
            FillRule::EvenOdd => sk::FillRule::EvenOdd,
        };
        let mask = self.clips.last().cloned();
        self.pixmap
            .fill_path(path, &paint, rule, transform, mask.as_ref());
    }
    pub fn stroke_path(
        &mut self,
        path: &sk::Path,
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
        let mask = self.clips.last().cloned();
        self.pixmap
            .stroke_path(path, &paint, &stroke, transform, mask.as_ref());
    }
    pub fn push_clip(&mut self, bounds: Bounds<Pixels>) {
        let Some(rect) = sk::Rect::from_xywh(
            bounds.origin.x.0,
            bounds.origin.y.0,
            bounds.size.width.0.max(0.),
            bounds.size.height.0.max(0.),
        ) else {
            return;
        };
        let path = sk::PathBuilder::from_rect(rect);
        let mut mask = match self.clips.last() {
            Some(m) => m.clone(),
            None => {
                let Some(mut m) = sk::Mask::new(self.pixmap.width(), self.pixmap.height()) else {
                    return;
                };
                m.fill_path(
                    &sk::PathBuilder::from_rect(
                        sk::Rect::from_xywh(0., 0., self.pixmap.width() as f32, self.pixmap.height() as f32)
                            .unwrap(),
                    ),
                    sk::FillRule::Winding,
                    false,
                    sk::Transform::identity(),
                );
                m
            }
        };
        mask.intersect_path(&path, sk::FillRule::Winding, true, self.transform);
        self.clips.push(mask);
    }
    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }
    pub fn draw_image(&mut self, image: &sk::Pixmap, bounds: Bounds<Pixels>, transform: sk::Transform) {
        let (w, h) = (image.width() as f32, image.height() as f32);
        if w <= 0. || h <= 0. {
            return;
        }
        let sx = bounds.size.width.0 / w;
        let sy = bounds.size.height.0 / h;
        let t = transform.pre_concat(
            sk::Transform::from_row(sx, 0., 0., sy, bounds.origin.x.0, bounds.origin.y.0),
        );
        let paint = sk::PixmapPaint {
            quality: sk::FilterQuality::Bilinear,
            ..Default::default()
        };
        let mask = self.clips.last().cloned();
        self.pixmap
            .draw_pixmap(0, 0, image.as_ref(), &paint, t, mask.as_ref());
    }
}

fn rounded_rect(b: Bounds<Pixels>, r: Corners<Pixels>) -> Option<sk::Path> {
    let (x, y, w, h) = (b.origin.x.0, b.origin.y.0, b.size.width.0, b.size.height.0);
    if w <= 0. || h <= 0. {
        return None;
    }
    let max = w.min(h) / 2.;
    let [tl, tr, br, bl] = [r.top_left.0, r.top_right.0, r.bottom_right.0, r.bottom_left.0]
        .map(|v| v.clamp(0., max));
    if tl == 0. && tr == 0. && br == 0. && bl == 0. {
        return Some(sk::PathBuilder::from_rect(sk::Rect::from_xywh(x, y, w, h)?));
    }
    const K: f32 = 0.5523;
    let mut p = sk::PathBuilder::new();
    p.move_to(x + tl, y);
    p.line_to(x + w - tr, y);
    p.cubic_to(x + w - tr * (1. - K), y, x + w, y + tr * (1. - K), x + w, y + tr);
    p.line_to(x + w, y + h - br);
    p.cubic_to(x + w, y + h - br * (1. - K), x + w - br * (1. - K), y + h, x + w - br, y + h);
    p.line_to(x + bl, y + h);
    p.cubic_to(x + bl * (1. - K), y + h, x, y + h - bl * (1. - K), x, y + h - bl);
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
    pub fn paint_quad(&mut self, quad: PaintQuad) {
        let Some(raster) = self.raster.as_mut() else {
            return;
        };
        let t = raster.transform;
        if quad.background.a > 0.
            && let Some(path) = rounded_rect(quad.bounds, quad.corner_radii)
        {
            raster.fill_path(&path, quad.background, FillRule::NonZero, t);
        }
        if let Some(g) = quad.gradient
            && let Some(path) = rounded_rect(quad.bounds, quad.corner_radii)
        {
            // CSS angles: 0° points up, 90° right.
            let b = quad.bounds;
            let (cx, cy) = (b.origin.x.0 + b.size.width.0 / 2., b.origin.y.0 + b.size.height.0 / 2.);
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
                let mask = raster.clips.last().cloned();
                raster
                    .pixmap
                    .fill_path(&path, &paint, sk::FillRule::Winding, t, mask.as_ref());
            }
        }
        let width = quad.border_widths.top.0;
        if width > 0. && quad.border_color.a > 0. {
            let inset = width / 2.;
            let b = Bounds::new(
                point(quad.bounds.origin.x + px(inset), quad.bounds.origin.y + px(inset)),
                size(
                    quad.bounds.size.width - px(width),
                    quad.bounds.size.height - px(width),
                ),
            );
            if let Some(path) = rounded_rect(b, quad.corner_radii) {
                let dash = (quad.border_style == BorderStyle::Dashed).then(|| vec![4., 3.]);
                raster.stroke_path(&path, quad.border_color, width, dash.as_deref(), t);
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
        raster.draw_image(&image.pixmap, bounds, t);
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
        raster.draw_image(&image.pixmap, bounds, t);
        Ok(())
    }
    /// Paints a premultiplied pixmap (from a CPU renderer) at `bounds`.
    pub fn paint_pixmap(&mut self, bounds: Bounds<Pixels>, pixmap: &sk::Pixmap, transform: Option<sk::Transform>) {
        if let Some(raster) = self.raster.as_mut() {
            let t = match transform {
                Some(extra) => raster.transform.pre_concat(extra),
                None => raster.transform,
            };
            raster.draw_image(pixmap, bounds, t);
        }
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
    static CANVAS_PIXELS: RefCell<HashMap<String, rune::Pixels>> = RefCell::new(HashMap::new());
    static CANVAS_SITES: RefCell<HashMap<(&'static str, u32, u32), usize>> = RefCell::new(HashMap::new());
    static FRAME_SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.) };
}
/// Starts a frame's canvases: call sites number their canvases afresh, and
/// paint at `scale` device pixels per point.
pub fn begin_frame(scale: f32) {
    CANVAS_SITES.with(|s| s.borrow_mut().clear());
    FRAME_SCALE.with(|s| s.set(scale));
}
/// A canvas's window rect as last laid out.
pub fn canvas_rect(key: &str) -> Option<[f32; 4]> {
    CANVAS_RECTS.with(|r| r.borrow().get(key).copied())
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
        key: format!("canvas-{}-{}-{}-{n}", site.line(), site.column(), site.file().len()),
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
    /// Paints now, with the bounds of the canvas's last layout, into a pixmap
    /// at `scale` device pixels per point.
    pub fn paint_now(self) -> Div {
        let key = self.key.clone();
        let rect = canvas_rect(&key);
        let pixels = CANVAS_PIXELS.with(|p| {
            p.borrow_mut()
                .entry(key.clone())
                .or_insert_with(rune::Pixels::new)
                .clone()
        });
        if let Some([x, y, w, h]) = rect
            && w >= 1.
            && h >= 1.
        {
            let scale = FRAME_SCALE.with(|s| s.get()).max(0.5);
            let bounds = Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
            let mut window = Window::paint(scale);
            window.raster = Raster::new((w * scale).ceil() as u32, (h * scale).ceil() as u32, scale, bounds.origin);
            let mut cx = Context::fresh();
            (self.paint)(bounds, &mut window, &mut cx);
            if let Some(raster) = window.raster.take() {
                let (pw, ph) = (raster.pixmap.width(), raster.pixmap.height());
                let mut rgba = raster.pixmap.take();
                crate::graph::unpremultiply(&mut rgba);
                pixels.set(pw, ph, rgba);
            }
        }
        let layout_key = key.clone();
        self.style.id(key).el(move |e| {
            e.pixels(&pixels).on_layout(move |_: &mut crate::AppState, r: [f32; 4]| {
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
    paint: impl FnOnce(&mut crate::NotesView, Bounds<Pixels>, &mut Window, &mut Context<crate::NotesView>) + 'static,
) -> Div {
    let pixels = CANVAS_PIXELS.with(|p| {
        p.borrow_mut()
            .entry(key.to_owned())
            .or_insert_with(rune::Pixels::new)
            .clone()
    });
    let layout_pixels = pixels.clone();
    div()
        .id(key)
        .size_full()
        .child(AnyElement::Lazy(Box::new(move |bcx: &mut BuildCx| {
            if let Some([x, y, w, h]) = canvas_rect(key)
                && w >= 1.
                && h >= 1.
            {
                let scale = FRAME_SCALE.with(|s| s.get()).max(0.5);
                let bounds = Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
                let mut window = Window::render(bcx.view);
                window.raster = Raster::new(
                    (w * scale).ceil() as u32,
                    (h * scale).ceil() as u32,
                    scale,
                    bounds.origin,
                );
                paint(bcx.view, bounds, &mut window, bcx.cx);
                if let Some(raster) = window.raster.take() {
                    let (pw, ph) = (raster.pixmap.width(), raster.pixmap.height());
                    let mut rgba = raster.pixmap.take();
                    crate::graph::unpremultiply(&mut rgba);
                    layout_pixels.set(pw, ph, rgba);
                }
                bcx.cx.effects.extend(window.effects);
            }
            AnyElement::Empty
        })))
        .el(move |e| {
            e.pixels(&pixels).on_layout(move |_: &mut crate::AppState, r: [f32; 4]| {
                CANVAS_RECTS.with(|rects| {
                    rects.borrow_mut().insert(key.to_owned(), r);
                });
            })
        })
}
