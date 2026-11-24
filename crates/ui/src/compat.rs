//! A small GPUI-flavoured element vocabulary over Rune.
//!
//! Folio's views were written against GPUI's styled `div()` builder, with
//! inherited text styles, block layout and listener closures over the view.
//! This module keeps that vocabulary so view code reads the same, and builds
//! plain Rune elements from it: [`Div::build`] resolves inherited text styles
//! and emits one `rune::El` per node. Handlers receive the app state through
//! [`Window`] and [`Context`], which queue the effects Rune applies through
//! its transaction once the handler returns.
#![allow(dead_code)]
use crate::NotesView;
use rune::{Color, Cursor, El, Shadow, TextStyle, Tx};
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

// ---------------------------------------------------------------- units

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Pixels(pub f32);
impl From<Pixels> for f32 {
    fn from(p: Pixels) -> f32 {
        p.0
    }
}
impl std::ops::Add for Pixels {
    type Output = Pixels;
    fn add(self, o: Pixels) -> Pixels {
        Pixels(self.0 + o.0)
    }
}
impl std::ops::Sub for Pixels {
    type Output = Pixels;
    fn sub(self, o: Pixels) -> Pixels {
        Pixels(self.0 - o.0)
    }
}
impl std::ops::Mul<f32> for Pixels {
    type Output = Pixels;
    fn mul(self, o: f32) -> Pixels {
        Pixels(self.0 * o)
    }
}
impl std::ops::Div<f32> for Pixels {
    type Output = Pixels;
    fn div(self, o: f32) -> Pixels {
        Pixels(self.0 / o)
    }
}
impl std::ops::Neg for Pixels {
    type Output = Pixels;
    fn neg(self) -> Pixels {
        Pixels(-self.0)
    }
}
impl Pixels {
    pub fn min(self, o: Pixels) -> Pixels {
        Pixels(self.0.min(o.0))
    }
    pub fn max(self, o: Pixels) -> Pixels {
        Pixels(self.0.max(o.0))
    }
    pub fn clamp(self, lo: Pixels, hi: Pixels) -> Pixels {
        Pixels(self.0.clamp(lo.0, hi.0))
    }
    pub fn abs(self) -> Pixels {
        Pixels(self.0.abs())
    }
    pub fn round(self) -> Pixels {
        Pixels(self.0.round())
    }
}
impl std::ops::AddAssign for Pixels {
    fn add_assign(&mut self, o: Pixels) {
        self.0 += o.0;
    }
}
impl std::ops::SubAssign for Pixels {
    fn sub_assign(&mut self, o: Pixels) {
        self.0 -= o.0;
    }
}
impl<T: std::ops::Add<Output = T>> std::ops::Add for Point<T> {
    type Output = Point<T>;
    fn add(self, o: Point<T>) -> Point<T> {
        Point {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}
impl<T: std::ops::Sub<Output = T>> std::ops::Sub for Point<T> {
    type Output = Point<T>;
    fn sub(self, o: Point<T>) -> Point<T> {
        Point {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }
}
pub fn px(v: f32) -> Pixels {
    Pixels(v)
}

/// A length as GPUI styles take it: pixels, rems or a fraction of the parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    Px(f32),
    Rems(f32),
    Relative(f32),
    Auto,
}
impl Length {
    fn resolve(self) -> Option<f32> {
        match self {
            Length::Px(v) => Some(v),
            Length::Rems(v) => Some(v * REM),
            _ => None,
        }
    }
}
/// One rem in points. The UI scale is applied as a Rune page zoom instead.
pub const REM: f32 = 16.;
pub fn rems(v: f32) -> Length {
    Length::Rems(v)
}
pub fn relative(v: f32) -> Length {
    Length::Relative(v)
}
impl From<Pixels> for Length {
    fn from(p: Pixels) -> Length {
        Length::Px(p.0)
    }
}
impl From<f32> for Length {
    fn from(p: f32) -> Length {
        Length::Px(p)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}
pub fn point<T>(x: T, y: T) -> Point<T> {
    Point { x, y }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}
pub fn size<T>(width: T, height: T) -> Size<T> {
    Size { width, height }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds<T> {
    pub origin: Point<T>,
    pub size: Size<T>,
}
impl Bounds<Pixels> {
    pub fn left(&self) -> Pixels {
        self.origin.x
    }
    pub fn top(&self) -> Pixels {
        self.origin.y
    }
    pub fn new(origin: Point<Pixels>, size: Size<Pixels>) -> Self {
        Self { origin, size }
    }
    pub fn from_rect(r: [f32; 4]) -> Self {
        Self {
            origin: point(px(r[0]), px(r[1])),
            size: size(px(r[2]), px(r[3])),
        }
    }
    pub fn contains(&self, p: &Point<Pixels>) -> bool {
        p.x.0 >= self.origin.x.0
            && p.y.0 >= self.origin.y.0
            && p.x.0 < self.origin.x.0 + self.size.width.0
            && p.y.0 < self.origin.y.0 + self.size.height.0
    }
    pub fn right(&self) -> Pixels {
        self.origin.x + self.size.width
    }
    pub fn bottom(&self) -> Pixels {
        self.origin.y + self.size.height
    }
    pub fn center(&self) -> Point<Pixels> {
        point(
            self.origin.x + self.size.width / 2.,
            self.origin.y + self.size.height / 2.,
        )
    }
}

// ---------------------------------------------------------------- colours

/// GPUI's colour: straight-alpha sRGB channels from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
impl Rgba {
    pub fn color(self) -> Color {
        Color {
            r: rune::color::srgb_decode(self.r),
            g: rune::color::srgb_decode(self.g),
            b: rune::color::srgb_decode(self.b),
            a: self.a,
        }
    }
    /// Premultiplied 8-bit RGBA for the CPU rasterizer.
    pub fn skia(self) -> resvg::tiny_skia::Color {
        resvg::tiny_skia::Color::from_rgba(
            self.r.clamp(0., 1.),
            self.g.clamp(0., 1.),
            self.b.clamp(0., 1.),
            self.a.clamp(0., 1.),
        )
        .unwrap_or(resvg::tiny_skia::Color::TRANSPARENT)
    }
    pub fn opacity(self, factor: f32) -> Self {
        Self {
            a: self.a * factor,
            ..self
        }
    }
}
pub type Hsla = Rgba;
/// sRGB `0xRRGGBB`.
pub fn rgb(c: u32) -> Rgba {
    rgba((c << 8) | 0xff)
}
/// sRGB `0xRRGGBBAA`.
pub fn rgba(c: u32) -> Rgba {
    Rgba {
        r: ((c >> 24) & 255) as f32 / 255.,
        g: ((c >> 16) & 255) as f32 / 255.,
        b: ((c >> 8) & 255) as f32 / 255.,
        a: (c & 255) as f32 / 255.,
    }
}
pub fn transparent_black() -> Rgba {
    Rgba::default()
}
pub fn white() -> Rgba {
    rgb(0xffffff)
}
pub fn black() -> Rgba {
    rgb(0)
}
pub enum Background {
    Color(Color),
    Gradient(LinearGradient),
}
impl From<Color> for Background {
    fn from(c: Color) -> Self {
        Background::Color(c)
    }
}
impl From<Rgba> for Background {
    fn from(c: Rgba) -> Self {
        Background::Color(c.color())
    }
}
impl From<LinearGradient> for Background {
    fn from(g: LinearGradient) -> Self {
        Background::Gradient(g)
    }
}
pub trait IntoColor {
    fn into_color(self) -> Color;
}
impl IntoColor for Color {
    fn into_color(self) -> Color {
        self
    }
}
impl IntoColor for Rgba {
    fn into_color(self) -> Color {
        self.color()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontWeight {
    Thin,
    Light,
    Normal,
    Medium,
    Semibold,
    Bold,
}
#[allow(non_upper_case_globals)]
impl FontWeight {
    pub const NORMAL: FontWeight = FontWeight::Normal;
    pub const MEDIUM: FontWeight = FontWeight::Medium;
    pub const SEMIBOLD: FontWeight = FontWeight::Semibold;
    pub const BOLD: FontWeight = FontWeight::Bold;
    pub const LIGHT: FontWeight = FontWeight::Light;
    fn value(self) -> f32 {
        match self {
            FontWeight::Thin => 200.,
            FontWeight::Light => 300.,
            FontWeight::Normal => 400.,
            FontWeight::Medium => 500.,
            FontWeight::Semibold => 600.,
            FontWeight::Bold => 700.,
        }
    }
}

// ---------------------------------------------------------------- text style

/// Text styles a node passes down to its text, as CSS inheritance does.
#[derive(Clone, Default, Debug)]
pub struct TextProps {
    pub size: Option<f32>,
    pub weight: Option<f32>,
    pub color: Option<Color>,
    pub italic: Option<bool>,
    pub mono: Option<bool>,
    pub line_height: Option<f32>,
    pub truncate: Option<bool>,
    pub nowrap: Option<bool>,
    pub align: Option<rune::Align>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub family: Option<&'static str>,
    pub max_lines: Option<usize>,
}
impl TextProps {
    fn merge(&self, child: &TextProps) -> TextProps {
        TextProps {
            size: child.size.or(self.size),
            weight: child.weight.or(self.weight),
            color: child.color.or(self.color),
            italic: child.italic.or(self.italic),
            mono: child.mono.or(self.mono),
            line_height: child.line_height.or(self.line_height),
            truncate: child.truncate.or(self.truncate),
            nowrap: child.nowrap.or(self.nowrap),
            align: child.align.or(self.align),
            underline: child.underline.or(self.underline),
            strike: child.strike.or(self.strike),
            family: child.family.or(self.family),
            max_lines: child.max_lines.or(self.max_lines),
        }
    }
    pub fn style(&self) -> TextStyle {
        let size = self.size.unwrap_or(REM);
        let mut style = TextStyle::new(size, self.weight.unwrap_or(400.));
        // GPUI's default line height is the golden ratio of the font size.
        style = style.lh(self.line_height.unwrap_or((size * 1.618).round()));
        if self.italic == Some(true) {
            style = style.italic();
        }
        if self.mono == Some(true) {
            style = style.mono();
        }
        if let Some(family) = self.family {
            style = style.family(rune::Family::Named(family));
        }
        style
    }
    pub fn apply(&self, mut t: El) -> El {
        t = t.font(self.style());
        if let Some(color) = self.color {
            t = t.color(color);
        }
        if self.truncate == Some(true) {
            t = t.truncate();
        } else if let Some(lines) = self.max_lines {
            t = t.max_lines(lines);
        } else if self.nowrap != Some(true) {
            t = t.wrap();
        }
        if let Some(align) = self.align {
            t = t.align(align);
        }
        t
    }
}

// ---------------------------------------------------------------- elements

pub type SharedString = String;

/// What building elements needs: the view, for lists that build their rows
/// from it, and where effects queued while building go.
pub struct BuildCx<'a> {
    pub view: &'a mut NotesView,
    pub window: &'a mut Window,
    pub cx: &'a mut Context<NotesView>,
}
pub type Lazy = Box<dyn FnOnce(&mut BuildCx) -> AnyElement>;

pub enum AnyElement {
    Div(Box<Div>),
    Text(String),
    Styled(String, Vec<rune::Span>),
    El(El),
    Lazy(Lazy),
    Empty,
}
impl AnyElement {
    pub fn build(self, inherited: &TextProps, bcx: &mut BuildCx) -> El {
        match self {
            AnyElement::Div(d) => d.build(inherited, bcx),
            AnyElement::Text(s) => inherited.apply(rune::text(s)).shrink(),
            AnyElement::Styled(s, spans) => inherited.apply(rune::rich(s, spans)).shrink(),
            AnyElement::El(e) => e,
            AnyElement::Lazy(f) => f(bcx).build(inherited, bcx),
            AnyElement::Empty => rune::el(),
        }
    }
    pub fn is_empty(&self) -> bool {
        matches!(self, AnyElement::Empty)
    }
}

pub trait IntoElement {
    fn into_any_element(self) -> AnyElement;
    fn into_element(self) -> AnyElement
    where
        Self: Sized,
    {
        self.into_any_element()
    }
}
impl IntoElement for AnyElement {
    fn into_any_element(self) -> AnyElement {
        self
    }
}
impl IntoElement for Div {
    fn into_any_element(self) -> AnyElement {
        AnyElement::Div(Box::new(self))
    }
}
impl IntoElement for El {
    fn into_any_element(self) -> AnyElement {
        AnyElement::El(self)
    }
}
impl IntoElement for &str {
    fn into_any_element(self) -> AnyElement {
        AnyElement::Text(self.to_owned())
    }
}
impl IntoElement for String {
    fn into_any_element(self) -> AnyElement {
        AnyElement::Text(self)
    }
}
impl IntoElement for &String {
    fn into_any_element(self) -> AnyElement {
        AnyElement::Text(self.clone())
    }
}
impl<T: IntoElement> IntoElement for Option<T> {
    fn into_any_element(self) -> AnyElement {
        self.map_or(AnyElement::Empty, IntoElement::into_any_element)
    }
}

/// Mouse buttons a handler can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}
impl Modifiers {
    pub fn from_mods(m: rune::Mods) -> Self {
        Self {
            control: m.ctrl || m.cmd,
            alt: m.alt,
            shift: m.shift,
            platform: m.logo,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClickEvent {
    pub keyboard: bool,
    pub position: Point<Pixels>,
    pub modifiers: Modifiers,
    pub click_count: usize,
}
impl ClickEvent {
    pub fn is_keyboard(&self) -> bool {
        self.keyboard
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct MouseDownEvent {
    pub position: Point<Pixels>,
    pub modifiers: Modifiers,
    pub click_count: usize,
}

type Handler<E> = Rc<dyn Fn(&mut NotesView, &E, &mut Window, &mut Context<NotesView>)>;

/// A handler over the view, as GPUI's `cx.listener` returns.
pub struct Listener<E>(pub Handler<E>);
impl<E> Clone for Listener<E> {
    fn clone(&self) -> Self {
        Listener(self.0.clone())
    }
}

thread_local! {
    static LAST_INPUT: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}
/// Whether input arrived in the last moment: what Folio settles a frame
/// after a click or key (a toolbar measured first, say) still animates as
/// that input's consequence.
pub fn recent_input() -> bool {
    LAST_INPUT.with(|t| t.get().is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(250)))
}
/// Runs `f` against the view inside a Rune handler, then applies what it asked for.
pub fn dispatch<R>(
    s: &mut crate::AppState,
    tx: &mut Tx,
    f: impl FnOnce(&mut NotesView, &mut Window, &mut Context<NotesView>) -> R,
) -> R {
    LAST_INPUT.with(|t| t.set(Some(std::time::Instant::now())));
    let view = s.view.get_mut();
    let mut window = Window::handler(view, tx);
    let mut cx = Context::fresh();
    let result = f(view, &mut window, &mut cx);
    view.apply_effects(window, cx, tx);
    result
}

#[derive(Clone, Copy, PartialEq)]
enum Display {
    Block,
    Flex,
}
#[derive(Clone, Copy, PartialEq)]
enum Dir {
    Row,
    Col,
}

/// GPUI's `div()`: a styled box with children and handlers.
pub struct Div {
    display: Display,
    dir: Dir,
    ops: Vec<Box<dyn FnOnce(El) -> El>>,
    text: TextProps,
    children: Vec<AnyElement>,
    border: [f32; 4],
    border_color: Option<Color>,
    radius: Option<[f32; 4]>,
    absolute: bool,
    inset: [Option<f32>; 4],
    clip: bool,
    key: Option<String>,
    hover_bg: Option<Color>,
    press_bg: Option<Color>,
    focus_border: Option<Color>,
    bg: Option<Color>,
    width: Option<Length>,
    height: Option<Length>,
    scroll_y: bool,
    scroll_x: bool,
    /// GPUI's `top: 100%` / `bottom: 100%`: placed beside the parent.
    full: Option<rune::Side>,
    anchor_to: Option<String>,
    /// The fill is settled; later `bg` calls leave it (a selection-group
    /// control whose choice is shown by its gliding highlight).
    bg_locked: bool,
    /// A member of a selection group (`selection_member`): whether it is
    /// the chosen one.
    selection: Option<bool>,
    /// Shows no hover or press tint; its parent draws them.
    tint_from_parent: bool,
    /// The point a menu grows from, in units of its own size (`grows_from`).
    grows: Option<(f32, f32)>,
}
thread_local! {
    static NEXT_ANCHOR: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// Starts a frame's generated anchor keys afresh.
pub fn begin_anchors() {
    NEXT_ANCHOR.with(|n| n.set(0));
}
pub fn div() -> Div {
    Div {
        display: Display::Block,
        dir: Dir::Row,
        ops: Vec::new(),
        text: TextProps::default(),
        children: Vec::new(),
        border: [0.; 4],
        border_color: None,
        radius: None,
        absolute: false,
        inset: [None; 4],
        clip: false,
        key: None,
        hover_bg: None,
        press_bg: None,
        focus_border: None,
        bg: None,
        width: None,
        height: None,
        scroll_y: false,
        scroll_x: false,
        full: None,
        anchor_to: None,
        bg_locked: false,
        selection: None,
        tint_from_parent: false,
        grows: None,
    }
}

macro_rules! spacing {
    ($($name:ident => $method:ident, $value:expr;)*) => {
        $(pub fn $name(self) -> Self { self.op(|e| e.$method($value * REM)) })*
    };
}

impl Div {
    fn op(mut self, f: impl FnOnce(El) -> El + 'static) -> Self {
        self.ops.push(Box::new(f));
        self
    }
    /// Applies an arbitrary Rune builder call to the element.
    pub fn el(self, f: impl FnOnce(El) -> El + 'static) -> Self {
        self.op(f)
    }
    pub fn build(self, inherited: &TextProps, bcx: &mut BuildCx) -> El {
        let text = inherited.merge(&self.text);
        let mut e = match (self.display, self.dir) {
            (Display::Flex, Dir::Row) => rune::row().items_stretch(),
            _ => rune::col(),
        };
        let mut key = self.key;
        let anchors_children = self
            .children
            .iter()
            .any(|c| matches!(c, AnyElement::Div(d) if d.full.is_some()));
        if anchors_children && key.is_none() {
            let n = NEXT_ANCHOR.with(|n| {
                let v = n.get();
                n.set(v + 1);
                v
            });
            key = Some(format!("anchor-parent-{n}"));
        }
        let own_key = key.clone();
        if let Some(key) = key {
            e = e.key(key);
        }
        if let Some(w) = self.width {
            e = match w {
                Length::Relative(f) if f >= 0.999 => e.w_full(),
                Length::Relative(f) => e.w_frac(f),
                other => match other.resolve() {
                    Some(v) => e.w(v),
                    None => e,
                },
            };
        }
        if let Some(h) = self.height {
            e = match h {
                Length::Relative(f) if f >= 0.999 => e.h_full(),
                Length::Relative(_) => e.h_full(),
                other => match other.resolve() {
                    Some(v) => e.h(v),
                    None => e,
                },
            };
        }
        for op in self.ops {
            e = op(e);
        }
        if let Some(bg) = self.bg {
            e = e.fill(bg);
        }
        if let Some(r) = self.radius {
            e = if r.iter().all(|v| *v == r[0]) {
                e.radius(r[0])
            } else {
                e.corners(r)
            };
        }
        if self.border.iter().any(|w| *w > 0.) {
            let color = self.border_color.unwrap_or(Color::TRANSPARENT);
            e = if self.border.iter().all(|w| *w == self.border[0]) {
                e.border(self.border[0], color)
            } else {
                // Rune orders sides top, right, bottom, left as CSS does.
                e.border_sides(self.border, color)
            };
        }
        if let Some(c) = self.focus_border {
            e = e.focus_ring(1., -1., c);
        }
        let mut children = self.children;
        let (hover_bg, press_bg) = if self.tint_from_parent {
            (None, None)
        } else if let Some(selected) = self.selection {
            // The hover tint lives on a layer of its own that stays mounted
            // and fades out smoothly when the member is chosen, instead of
            // snapping off during the press and gliding-highlight animation.
            let mut layer = div().absolute().inset_0();
            if let Some(c) = self.hover_bg {
                let mut wash = div()
                    .absolute()
                    .inset_0()
                    .opacity(if selected { 0. } else { 1. })
                    .el(|e| {
                        e.opacity_motion(crate::motion::HOVER_EXIT)
                            .animated_changes()
                    });
                wash.radius = self.radius;
                wash.hover_bg = Some(c);
                layer = layer.child(wash);
            }
            children.insert(0, layer.into_any_element());
            (None, None)
        } else {
            (self.hover_bg, self.press_bg)
        };
        if let Some(c) = hover_bg {
            e = e.hover(c);
        }
        if let Some(c) = press_bg {
            e = e.press(c);
        }
        if let (Some(side), Some(anchor)) = (self.full, &self.anchor_to) {
            e = e.anchored(anchor.clone(), side, rune::Align::Start, 0.);
        } else if self.absolute {
            let [t, r, b, l] = self.inset;
            e = e.pin(l, t, r, b);
        } else if self.inset.iter().any(Option::is_some) {
            // A relatively positioned element's insets move it visually.
            let [t, _, _, l] = self.inset;
            let (x, y) = (l.unwrap_or(0.), t.unwrap_or(0.));
            if x != 0. || y != 0. {
                e = e.offset(x, y);
            }
        }
        if self.clip {
            let radius = self.radius.map_or(0., |r| r[0]);
            e = e.clip_rounded(radius);
        }
        if self.scroll_y {
            e = e.scroll(rune::ScrollSpec {
                stick_bottom: false,
            });
        } else if self.scroll_x {
            e = e.scroll_x();
        }
        for mut child in children {
            if child.is_empty() {
                continue;
            }
            if let AnyElement::Div(d) = &mut child
                && d.full.is_some()
            {
                d.anchor_to = own_key.clone();
            }
            e = e.child(child.build(&text, bcx));
        }
        e
    }

    // ------------------------------------------------------------ tree
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
    /// Content that replaces other content in place (a settings section, a
    /// menu page): it rises in a few points as it fades in, and the content
    /// it replaces goes at once, so the two never overlap. Give the element
    /// a key that changes with what it shows.
    pub fn swap_in(self) -> Self {
        self.op(|e| {
            e.transition(
                rune::Transition::offset(0., 6.)
                    .combined(rune::Transition::opacity())
                    .springs(rune::SpringPair::new(crate::motion::SWAP, rune::Spring::NONE)),
            )
            .animated_changes()
        })
    }
    /// A menu page replacing another: deeper pages slide in from the right
    /// (`deeper`), the page gone back to from the left, fading in while the
    /// page it replaces goes at once.
    pub fn page_in(self, deeper: bool) -> Self {
        self.op(move |e| {
            e.transition(
                rune::Transition::offset(if deeper { 14. } else { -14. }, 0.)
                    .combined(rune::Transition::opacity())
                    .springs(rune::SpringPair::new(crate::motion::SWAP, rune::Spring::NONE)),
            )
            .animated_changes()
        })
    }
    /// The point a menu grows from as it opens (see
    /// `transitions::overlay_transition`), in units of its own size.
    pub fn grows_from(mut self, x: f32, y: f32) -> Self {
        let (x, y) = (x.clamp(0., 1.), y.clamp(0., 1.));
        self.grows = Some((x, y));
        self.op(move |e| e.origin(x, y))
    }
    /// The point set by [`Div::grows_from`], if any.
    pub fn growth_point(&self) -> Option<(f32, f32)> {
        self.grows
    }
    /// [`Div::grows_from`] the window point `at`, for a menu whose box is
    /// `rect` (x, y, width, height).
    pub fn grows_from_point(self, at: (f32, f32), rect: (f32, f32, f32, f32)) -> Self {
        let (x, y, w, h) = rect;
        self.grows_from((at.0 - x) / w.max(1.), (at.1 - y) / h.max(1.))
    }
    /// Keeps the current fill against later `bg` calls.
    pub fn lock_bg(mut self) -> Self {
        self.bg_locked = true;
        self
    }
    /// Marks a member of a selection group whose chosen member's highlight
    /// glides to the next choice (see [`Div::underlay`]). Only an unchosen
    /// member shows its hover tint, on a layer beneath its children that is
    /// dropped the moment it is chosen, so the hover never fades out beside
    /// the arriving highlight as a second box. Members show no press tint,
    /// which would fade there the same way; they still give under a press.
    pub fn selection_member(mut self, selected: bool) -> Self {
        self.selection = Some(selected);
        self
    }
    /// Shows no hover or press tint of its own: a control inside a
    /// [`Div::selection_member`] row, which draws them for the whole row.
    pub fn tint_from_parent(mut self) -> Self {
        self.tint_from_parent = true;
        self
    }
    /// Draws `child` beneath the element's other children (a selection
    /// highlight that glides from one control to the next).
    pub fn underlay(mut self, child: impl IntoElement) -> Self {
        self.children.insert(0, child.into_any_element());
        self
    }
    /// Turns the element a quarter turn clockwise while `turned`, on a
    /// spring: a disclosure arrow as its branch opens.
    pub fn quarter_turn(self, turned: bool) -> Self {
        self.op(move |e| {
            e.rotate(if turned { std::f32::consts::FRAC_PI_2 } else { 0. })
                .rotate_motion(crate::motion::POP)
                .animated_changes()
        })
    }
    /// Turns the element upside down while `flipped`, on a spring: a
    /// dropdown chevron while its menu is open.
    pub fn flipped(self, flipped: bool) -> Self {
        self.op(move |e| {
            e.rotate(if flipped { std::f32::consts::PI } else { 0. })
                .rotate_motion(crate::motion::POP)
                .animated_changes()
        })
    }
    pub fn children<I: IntoElement>(mut self, children: impl IntoIterator<Item = I>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }
    pub fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if condition { f(self) } else { self }
    }
    pub fn when_some<T>(self, value: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        match value {
            Some(v) => f(self, v),
            None => self,
        }
    }
    pub fn when_none<T>(self, value: &Option<T>, f: impl FnOnce(Self) -> Self) -> Self {
        if value.is_none() { f(self) } else { self }
    }
    pub fn map(self, f: impl FnOnce(Self) -> Self) -> Self {
        f(self)
    }
    pub fn id(mut self, id: impl IntoKey) -> Self {
        self.key = Some(id.into_key());
        self
    }
    pub fn key_context(self, _: &str) -> Self {
        self
    }

    // ------------------------------------------------------------ layout
    pub fn flex(mut self) -> Self {
        self.display = Display::Flex;
        self
    }
    pub fn flex_row(mut self) -> Self {
        self.display = Display::Flex;
        self.dir = Dir::Row;
        self
    }
    pub fn flex_col(mut self) -> Self {
        self.display = Display::Flex;
        self.dir = Dir::Col;
        self
    }
    pub fn block(mut self) -> Self {
        self.display = Display::Block;
        self
    }
    pub fn flex_1(self) -> Self {
        self.op(|e| e.grow(1.).shrink())
    }
    pub fn flex_grow(self) -> Self {
        self.op(|e| e.grow(1.))
    }
    pub fn flex_none(self) -> Self {
        self.op(|e| e.el_shrink0())
    }
    pub fn flex_shrink_0(self) -> Self {
        self.op(|e| e.el_shrink0())
    }
    pub fn flex_shrink(self) -> Self {
        self.op(|e| e.shrink())
    }
    pub fn flex_wrap(self) -> Self {
        self.op(|e| e.flex_wrap())
    }
    pub fn items_center(self) -> Self {
        self.op(|e| e.items_center())
    }
    pub fn items_start(self) -> Self {
        self.op(|e| e.items_start())
    }
    pub fn items_end(self) -> Self {
        self.op(|e| e.items_end())
    }
    pub fn items_stretch(self) -> Self {
        self.op(|e| e.items_stretch())
    }
    pub fn justify_center(self) -> Self {
        self.op(|e| e.justify_center())
    }
    pub fn justify_start(self) -> Self {
        self.op(|e| e.justify_start())
    }
    pub fn justify_end(self) -> Self {
        self.op(|e| e.justify_end())
    }
    pub fn justify_between(self) -> Self {
        self.op(|e| e.justify_between())
    }
    pub fn justify_around(self) -> Self {
        self.op(|e| e.justify_around())
    }
    pub fn self_center(self) -> Self {
        self.op(|e| e.self_center())
    }
    pub fn self_start(self) -> Self {
        self.op(|e| e.self_start())
    }
    pub fn self_end(self) -> Self {
        self.op(|e| e.self_end())
    }
    pub fn content_center(self) -> Self {
        self
    }
    pub fn gap(self, g: impl Into<Length>) -> Self {
        let g = g.into().resolve().unwrap_or(0.);
        self.op(move |e| e.gap(g))
    }
    pub fn gap_x(self, g: impl Into<Length>) -> Self {
        let g = g.into().resolve().unwrap_or(0.);
        self.op(move |e| e.gaps(0., g))
    }
    pub fn gap_y(self, g: impl Into<Length>) -> Self {
        let g = g.into().resolve().unwrap_or(0.);
        self.op(move |e| e.gaps(g, 0.))
    }
    spacing! {
        gap_0 => gap, 0.;
        gap_0p5 => gap, 0.125;
        gap_1 => gap, 0.25;
        gap_1p5 => gap, 0.375;
        gap_2 => gap, 0.5;
        gap_2p5 => gap, 0.625;
        gap_3 => gap, 0.75;
        gap_4 => gap, 1.;
        gap_5 => gap, 1.25;
        gap_6 => gap, 1.5;
        gap_8 => gap, 2.;
        p_0 => padding, 0.;
        p_0p5 => padding, 0.125;
        p_1 => padding, 0.25;
        p_1p5 => padding, 0.375;
        p_2 => padding, 0.5;
        p_2p5 => padding, 0.625;
        p_3 => padding, 0.75;
        p_4 => padding, 1.;
        p_5 => padding, 1.25;
        p_6 => padding, 1.5;
        p_8 => padding, 2.;
        px_0 => px, 0.;
        px_0p5 => px, 0.125;
        px_1 => px, 0.25;
        px_1p5 => px, 0.375;
        px_2 => px, 0.5;
        px_2p5 => px, 0.625;
        px_3 => px, 0.75;
        px_4 => px, 1.;
        px_5 => px, 1.25;
        px_6 => px, 1.5;
        px_7 => px, 1.75;
        px_8 => px, 2.;
        py_0 => py, 0.;
        py_0p5 => py, 0.125;
        py_1 => py, 0.25;
        py_1p5 => py, 0.375;
        py_2 => py, 0.5;
        py_2p5 => py, 0.625;
        py_3 => py, 0.75;
        py_4 => py, 1.;
        py_5 => py, 1.25;
        py_6 => py, 1.5;
        py_8 => py, 2.;
        pt_0 => pt, 0.;
        pt_1 => pt, 0.25;
        pt_1p5 => pt, 0.375;
        pt_2 => pt, 0.5;
        pt_3 => pt, 0.75;
        pt_4 => pt, 1.;
        pt_5 => pt, 1.25;
        pt_6 => pt, 1.5;
        pb_0 => pb, 0.;
        pb_1 => pb, 0.25;
        pb_2 => pb, 0.5;
        pb_3 => pb, 0.75;
        pb_4 => pb, 1.;
        pb_6 => pb, 1.5;
        pl_0 => pl, 0.;
        pl_1 => pl, 0.25;
        pl_2 => pl, 0.5;
        pl_3 => pl, 0.75;
        pl_4 => pl, 1.;
        pl_6 => pl, 1.5;
        pr_0 => pr, 0.;
        pr_1 => pr, 0.25;
        pr_3 => pr, 0.75;
        pr_4 => pr, 1.;
        mt_1 => mt, 0.25;
        mt_2 => mt, 0.5;
        mt_3 => mt, 0.75;
        mt_4 => mt, 1.;
        mt_6 => mt, 1.5;
        mb_1 => mb, 0.25;
        mb_2 => mb, 0.5;
        mb_3 => mb, 0.75;
        mb_4 => mb, 1.;
        ml_1 => ml, 0.25;
        ml_2 => ml, 0.5;
        ml_4 => ml, 1.;
        mr_1 => mr, 0.25;
        mr_2 => mr, 0.5;
        mx_1 => mx, 0.25;
        mx_1p5 => mx, 0.375;
        mx_0p5 => mx, 0.125;
        pb_0p5 => pb, 0.125;
        pt_0p5 => pt, 0.125;
        pb_10 => pb, 2.5;
        pt_10 => pt, 2.5;
        mb_5 => mb, 1.25;
        mt_5 => mt, 1.25;
        mb_6 => mb, 1.5;
        mt_8 => mt, 2.;
        mx_3 => mx, 0.75;
        mx_4 => mx, 1.;
        my_3 => my, 0.75;
        pl_5 => pl, 1.25;
        pr_2 => pr, 0.5;
        pr_6 => pr, 1.5;
        pt_8 => pt, 2.;
        pb_5 => pb, 1.25;
        pb_8 => pb, 2.;
        mx_2 => mx, 0.5;
        my_1 => my, 0.25;
        my_2 => my, 0.5;
    }
    pub fn p(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.padding(v))
    }
    pub fn px(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.px(v))
    }
    pub fn py(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.py(v))
    }
    pub fn pt(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.pt(v))
    }
    pub fn pb(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.pb(v))
    }
    pub fn pl(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.pl(v))
    }
    pub fn pr(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.pr(v))
    }
    pub fn mt(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.mt(v))
    }
    pub fn mb(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.mb(v))
    }
    pub fn ml(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.ml(v))
    }
    pub fn mr(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.mr(v))
    }
    pub fn mx(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.mx(v))
    }
    pub fn my(self, v: impl Into<Length>) -> Self {
        let v = v.into().resolve().unwrap_or(0.);
        self.op(move |e| e.my(v))
    }
    pub fn w(mut self, w: impl Into<Length>) -> Self {
        self.width = Some(w.into());
        self
    }
    pub fn h(mut self, h: impl Into<Length>) -> Self {
        self.height = Some(h.into());
        self
    }
    pub fn size(self, s: impl Into<Length> + Copy) -> Self {
        self.w(s).h(s)
    }
    pub fn w_full(self) -> Self {
        self.w(Length::Relative(1.))
    }
    pub fn h_full(self) -> Self {
        self.h(Length::Relative(1.))
    }
    pub fn size_full(self) -> Self {
        self.w_full().h_full()
    }
    pub fn w_auto(self) -> Self {
        self
    }
    pub fn min_w(self, w: impl Into<Length>) -> Self {
        let v = w.into().resolve().unwrap_or(0.);
        self.op(move |e| e.min_w(v))
    }
    pub fn min_h(self, h: impl Into<Length>) -> Self {
        let v = h.into().resolve().unwrap_or(0.);
        self.op(move |e| e.min_h(v))
    }
    pub fn max_w(self, w: impl Into<Length>) -> Self {
        match w.into() {
            Length::Relative(_) => self,
            other => {
                let v = other.resolve().unwrap_or(0.);
                self.op(move |e| e.max_w(v))
            }
        }
    }
    pub fn max_h(self, h: impl Into<Length>) -> Self {
        match h.into() {
            Length::Relative(_) => self,
            other => {
                let v = other.resolve().unwrap_or(0.);
                self.op(move |e| e.max_h(v))
            }
        }
    }
    pub fn min_w_0(self) -> Self {
        self.op(|e| e.min_w(0.))
    }
    pub fn min_h_0(self) -> Self {
        self.op(|e| e.min_h(0.))
    }
    pub fn absolute(mut self) -> Self {
        self.absolute = true;
        self
    }
    pub fn relative(self) -> Self {
        self
    }
    pub fn inset_0(mut self) -> Self {
        self.absolute = true;
        self.inset = [Some(0.); 4];
        self
    }
    pub fn top(mut self, v: impl Into<Length>) -> Self {
        self.inset[0] = v.into().resolve();
        self
    }
    pub fn right(mut self, v: impl Into<Length>) -> Self {
        self.inset[1] = v.into().resolve();
        self
    }
    pub fn bottom(mut self, v: impl Into<Length>) -> Self {
        self.inset[2] = v.into().resolve();
        self
    }
    pub fn left(mut self, v: impl Into<Length>) -> Self {
        self.inset[3] = v.into().resolve();
        self
    }
    pub fn top_0(self) -> Self {
        self.top(px(0.))
    }
    pub fn left_0(self) -> Self {
        self.left(px(0.))
    }
    pub fn right_0(self) -> Self {
        self.right(px(0.))
    }
    pub fn bottom_0(self) -> Self {
        self.bottom(px(0.))
    }
    pub fn overflow_hidden(mut self) -> Self {
        self.clip = true;
        self
    }
    pub fn overflow_x_hidden(self) -> Self {
        self.overflow_hidden()
    }
    pub fn overflow_y_hidden(self) -> Self {
        self.overflow_hidden()
    }
    pub fn overflow_y_scroll(mut self) -> Self {
        self.scroll_y = true;
        self
    }
    pub fn overflow_x_scroll(mut self) -> Self {
        self.scroll_x = true;
        self
    }
    pub fn overflow_scroll(self) -> Self {
        self.overflow_y_scroll()
    }

    // ------------------------------------------------------------ visuals
    pub fn bg(mut self, b: impl Into<Background>) -> Self {
        if self.bg_locked {
            return self;
        }
        match b.into() {
            Background::Color(c) => self.bg = Some(c),
            Background::Gradient(g) => return self.bg_gradient(g),
        }
        self
    }
    pub fn rounded(mut self, r: impl Into<Length>) -> Self {
        let r = r.into().resolve().unwrap_or(0.);
        self.radius = Some([r; 4]);
        self
    }
    pub fn rounded_full(self) -> Self {
        self.rounded(px(9999.))
    }
    pub fn rounded_sm(self) -> Self {
        self.rounded(px(2.))
    }
    pub fn rounded_md(self) -> Self {
        self.rounded(px(6.))
    }
    pub fn rounded_lg(self) -> Self {
        self.rounded(px(8.))
    }
    pub fn rounded_corners(mut self, r: [f32; 4]) -> Self {
        self.radius = Some(r);
        self
    }
    pub fn rounded_t(mut self, r: impl Into<Length>) -> Self {
        let r = r.into().resolve().unwrap_or(0.);
        let mut c = self.radius.unwrap_or([0.; 4]);
        c[0] = r;
        c[1] = r;
        self.radius = Some(c);
        self
    }
    pub fn rounded_b(mut self, r: impl Into<Length>) -> Self {
        let r = r.into().resolve().unwrap_or(0.);
        let mut c = self.radius.unwrap_or([0.; 4]);
        c[2] = r;
        c[3] = r;
        self.radius = Some(c);
        self
    }
    pub fn border_1(mut self) -> Self {
        self.border = [1.; 4];
        self
    }
    pub fn border_2(mut self) -> Self {
        self.border = [2.; 4];
        self
    }
    pub fn border(mut self, w: impl Into<Length>) -> Self {
        self.border = [w.into().resolve().unwrap_or(1.); 4];
        self
    }
    pub fn border_t_1(mut self) -> Self {
        self.border[0] = 1.;
        self
    }
    pub fn border_r_1(mut self) -> Self {
        self.border[1] = 1.;
        self
    }
    pub fn border_b_1(mut self) -> Self {
        self.border[2] = 1.;
        self
    }
    pub fn border_l_1(mut self) -> Self {
        self.border[3] = 1.;
        self
    }
    pub fn border_l_2(mut self) -> Self {
        self.border[3] = 2.;
        self
    }
    pub fn border_b_2(mut self) -> Self {
        self.border[2] = 2.;
        self
    }
    pub fn border_color(mut self, c: impl IntoColor) -> Self {
        self.border_color = Some(c.into_color());
        self
    }
    pub fn border_dashed(self) -> Self {
        self.op(|e| e.border_dash(4., 3.))
    }
    pub fn opacity(self, o: f32) -> Self {
        self.op(move |e| e.opacity(o))
    }
    pub fn invisible(self) -> Self {
        self.op(|e| e.opacity(0.))
    }
    pub fn visible(self) -> Self {
        self
    }
    pub fn shadow_sm(self) -> Self {
        self.op(|e| e.box_shadow(shadow(0., 1., 2., 0x0000000d)))
    }
    pub fn shadow_md(self) -> Self {
        self.op(|e| {
            e.box_shadow(shadow(0., 4., 6., 0x0000001a))
                .box_shadow(shadow(0., 2., 4., 0x0000000f))
        })
    }
    pub fn shadow_lg(self) -> Self {
        self.op(|e| {
            e.box_shadow(shadow(0., 10., 15., 0x0000001a))
                .box_shadow(shadow(0., 4., 6., 0x0000000d))
        })
    }
    pub fn shadow_xl(self) -> Self {
        self.op(|e| e.box_shadow(shadow(0., 20., 25., 0x0000001a)))
    }
    pub fn shadow_2xl(self) -> Self {
        self.op(|e| e.box_shadow(shadow(0., 25., 50., 0x00000040)))
    }
    pub fn shadow(self, shadows: Vec<BoxShadow>) -> Self {
        self.op(move |mut e| {
            for s in shadows {
                e = e.box_shadow(Shadow {
                    x: s.offset.x.0,
                    y: s.offset.y.0,
                    blur: s.blur_radius.0,
                    spread: s.spread_radius.0,
                    color: s.color.color(),
                    inset: false,
                });
            }
            e
        })
    }
    pub fn cursor_pointer(self) -> Self {
        self.op(|e| e.cursor(Cursor::Pointer))
    }
    pub fn cursor_default(self) -> Self {
        self.op(|e| e.cursor(Cursor::Default))
    }
    pub fn cursor_text(self) -> Self {
        self.op(|e| e.cursor(Cursor::Text))
    }
    pub fn cursor_grab(self) -> Self {
        self.op(|e| e.cursor(Cursor::Grab))
    }
    pub fn cursor_grabbing(self) -> Self {
        self.op(|e| e.cursor(Cursor::Grabbing))
    }
    pub fn cursor_move(self) -> Self {
        self.op(|e| e.cursor(Cursor::Move))
    }
    pub fn cursor_crosshair(self) -> Self {
        self.op(|e| e.cursor(Cursor::Crosshair))
    }
    pub fn cursor_not_allowed(self) -> Self {
        self.op(|e| e.cursor(Cursor::NotAllowed))
    }
    pub fn cursor_ew_resize(self) -> Self {
        self.op(|e| e.cursor(Cursor::ResizeColumn))
    }
    pub fn cursor_ns_resize(self) -> Self {
        self.op(|e| e.cursor(Cursor::ResizeRow))
    }
    pub fn cursor(self, c: Cursor) -> Self {
        self.op(move |e| e.cursor(c))
    }
    /// Takes the pointer so elements under this one see no hover or press.
    pub fn occlude(self) -> Self {
        self.op(|e| e.hit())
    }
    pub fn hover(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let s = f(StyleRefinement::default());
        if let Some(c) = s.bg {
            self.hover_bg = Some(c);
        }
        self
    }
    pub fn active(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let s = f(StyleRefinement::default());
        if let Some(c) = s.bg {
            self.press_bg = Some(c);
        }
        self
    }
    pub fn focus(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let s = f(StyleRefinement::default());
        if let Some(c) = s.border_color.or(s.bg) {
            self.focus_border = Some(c);
        }
        self
    }
    pub fn focus_visible(self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.focus(f)
    }

    // ------------------------------------------------------------ text
    pub fn text_xs(mut self) -> Self {
        self.text.size = Some(0.75 * REM);
        self
    }
    pub fn text_sm(mut self) -> Self {
        self.text.size = Some(0.875 * REM);
        self
    }
    pub fn text_base(mut self) -> Self {
        self.text.size = Some(REM);
        self
    }
    pub fn text_lg(mut self) -> Self {
        self.text.size = Some(1.125 * REM);
        self
    }
    pub fn text_xl(mut self) -> Self {
        self.text.size = Some(1.25 * REM);
        self
    }
    pub fn text_2xl(mut self) -> Self {
        self.text.size = Some(1.5 * REM);
        self
    }
    pub fn text_3xl(mut self) -> Self {
        self.text.size = Some(1.875 * REM);
        self
    }
    pub fn text_size(mut self, s: impl Into<Length>) -> Self {
        self.text.size = s.into().resolve();
        self
    }
    pub fn line_height(mut self, l: impl Into<Length>) -> Self {
        self.text.line_height = l.into().resolve();
        self
    }
    pub fn font_weight(mut self, w: FontWeight) -> Self {
        self.text.weight = Some(w.value());
        self
    }
    pub fn font_bold(self) -> Self {
        self.font_weight(FontWeight::Bold)
    }
    pub fn font_semibold(self) -> Self {
        self.font_weight(FontWeight::Semibold)
    }
    pub fn font_medium(self) -> Self {
        self.font_weight(FontWeight::Medium)
    }
    pub fn italic(mut self) -> Self {
        self.text.italic = Some(true);
        self
    }
    pub fn not_italic(mut self) -> Self {
        self.text.italic = Some(false);
        self
    }
    pub fn underline(mut self) -> Self {
        self.text.underline = Some(true);
        self
    }
    pub fn line_through(mut self) -> Self {
        self.text.strike = Some(true);
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        let family = family.into();
        self.text.family = Some(Box::leak(family.into_boxed_str()));
        self
    }
    pub fn font_mono(mut self) -> Self {
        self.text.mono = Some(true);
        self
    }
    pub fn text_color(mut self, c: impl IntoColor) -> Self {
        self.text.color = Some(c.into_color());
        self
    }
    pub fn truncate(mut self) -> Self {
        self.text.truncate = Some(true);
        self.op(|e| e.min_w(0.))
    }
    pub fn text_ellipsis(mut self) -> Self {
        self.text.truncate = Some(true);
        self
    }
    pub fn whitespace_nowrap(mut self) -> Self {
        self.text.nowrap = Some(true);
        self
    }
    pub fn whitespace_normal(mut self) -> Self {
        self.text.nowrap = Some(false);
        self.text.truncate = Some(false);
        self
    }
    pub fn line_clamp(mut self, n: usize) -> Self {
        self.text.max_lines = Some(n);
        self
    }
    pub fn text_center(mut self) -> Self {
        self.text.align = Some(rune::Align::Center);
        self
    }
    pub fn text_right(mut self) -> Self {
        self.text.align = Some(rune::Align::End);
        self
    }
    pub fn text_left(mut self) -> Self {
        self.text.align = Some(rune::Align::Start);
        self
    }

    // ------------------------------------------------------------ a11y
    pub fn role(self, role: &'static str) -> Self {
        self.op(move |e| e.role(role))
    }
    pub fn label(self, label: impl Into<String>) -> Self {
        let label = label.into();
        self.op(move |e| e.label(label))
    }
    pub fn tooltip_text(self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.op(move |e| e.help(text))
    }
    pub fn focusable(self) -> Self {
        self.op(|e| e.focusable(true))
    }
    pub fn tab_stop(self, on: bool) -> Self {
        self.op(move |e| e.focusable(on))
    }
    pub fn track_focus(self, focus: &FocusHandle) -> Self {
        let key = focus.0.clone();
        self.op(move |e| e.key(key).focusable(true))
    }

    // ------------------------------------------------------------ handlers
    pub fn on_click(self, listener: Listener<ClickEvent>) -> Self {
        self.op(move |e| {
            e.on_press_tx(move |s: &mut crate::AppState, tx: &mut Tx| {
                let event = ClickEvent {
                    keyboard: false,
                    position: s.view.get_mut().mouse_position,
                    modifiers: Modifiers::from_mods(tx.mods()),
                    click_count: 1,
                };
                let f = listener.0.clone();
                dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
            })
        })
    }
    pub fn on_mouse_down(self, button: MouseButton, listener: Listener<MouseDownEvent>) -> Self {
        self.op(move |e| match button {
            MouseButton::Left => {
                e.press_on_down()
                    .on_press_tx(move |s: &mut crate::AppState, tx| {
                        let event = MouseDownEvent {
                            position: s.view.get_mut().mouse_position,
                            modifiers: Modifiers::from_mods(tx.mods()),
                            click_count: 1,
                        };
                        let f = listener.0.clone();
                        dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
                    })
            }
            MouseButton::Right => {
                e.on_secondary_press(move |s: &mut crate::AppState, press, tx| {
                    let event = MouseDownEvent {
                        position: point(px(press.x), px(press.y)),
                        modifiers: Modifiers::from_mods(tx.mods()),
                        click_count: 1,
                    };
                    s.view.get_mut().mouse_position = event.position;
                    let f = listener.0.clone();
                    dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
                })
            }
            MouseButton::Middle => e.on_middle_press(move |s: &mut crate::AppState, tx| {
                let event = MouseDownEvent {
                    position: s.view.get_mut().mouse_position,
                    modifiers: Modifiers::from_mods(tx.mods()),
                    click_count: 1,
                };
                let f = listener.0.clone();
                dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
            }),
        })
    }
    pub fn on_double_click(self, listener: Listener<ClickEvent>) -> Self {
        self.op(move |e| {
            e.on_double_press_tx(move |s: &mut crate::AppState, tx: &mut Tx| {
                let event = ClickEvent {
                    click_count: 2,
                    position: s.view.get_mut().mouse_position,
                    ..Default::default()
                };
                let f = listener.0.clone();
                dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
            })
        })
    }
    pub fn on_hover(self, listener: Listener<bool>) -> Self {
        self.op(move |e| {
            e.on_hover_change(move |s: &mut crate::AppState, inside, tx| {
                let f = listener.0.clone();
                dispatch(s, tx, |view, window, cx| f(view, &inside, window, cx));
            })
        })
    }
    /// Records this element's window bounds under `key` each layout.
    pub fn track_bounds(self, key: impl Into<String>) -> Self {
        let key = key.into();
        self.op(move |e| {
            e.on_layout(move |s: &mut crate::AppState, r: [f32; 4]| {
                s.view.get_mut().accessibility.set_bounds(&key, r);
            })
        })
    }
    pub fn checked(self, on: bool) -> Self {
        self.op(move |e| e.checked(on))
    }
    pub fn on_key_down(self, listener: Listener<KeyDownEvent>) -> Self {
        self.op(move |e| {
            e.on_key(
                move |s: &mut crate::AppState, key: &rune::KeyEvent, tx: &mut Tx| {
                    let Some(event) = KeyDownEvent::from_rune(key) else {
                        return false;
                    };
                    let f = listener.0.clone();
                    let view = s.view.get_mut();
                    let mut window = Window::handler(view, tx);
                    let mut cx = Context::fresh();
                    f(view, &event, &mut window, &mut cx);
                    let stopped = cx.stopped;
                    view.apply_effects(window, cx, tx);
                    stopped
                },
            )
        })
    }
    pub fn on_scroll_wheel(self, listener: Listener<ScrollWheelEvent>) -> Self {
        self.op(move |e| {
            e.on_wheel(
                move |s: &mut crate::AppState, w: rune::WheelEvent, tx: &mut Tx| {
                    let event = ScrollWheelEvent::from_rune(&w, s.view.get_mut().mouse_position);
                    let f = listener.0.clone();
                    let view = s.view.get_mut();
                    let mut window = Window::handler(view, tx);
                    let mut cx = Context::fresh();
                    f(view, &event, &mut window, &mut cx);
                    let stopped = cx.stopped;
                    view.apply_effects(window, cx, tx);
                    stopped
                },
            )
        })
    }
    pub fn on_action<A: Action>(self, listener: Listener<A>) -> Self {
        register_action::<A>(listener);
        self
    }
}

trait Shrink0 {
    fn el_shrink0(self) -> Self;
}
impl Shrink0 for El {
    fn el_shrink0(self) -> Self {
        // Rune elements don't shrink below their content unless asked to.
        self
    }
}

fn shadow(x: f32, y: f32, blur: f32, color: u32) -> Shadow {
    Shadow {
        x,
        y,
        blur,
        spread: 0.,
        color: rgba(color).color(),
        inset: false,
    }
}

#[derive(Clone, Copy)]
pub struct BoxShadow {
    pub color: Rgba,
    pub offset: Point<Pixels>,
    pub blur_radius: Pixels,
    pub spread_radius: Pixels,
}

/// The style a `hover`, `active` or `focus` closure refines.
#[derive(Default)]
pub struct StyleRefinement {
    opacity_set: bool,
    bg: Option<Color>,
    border_color: Option<Color>,
    text_color: Option<Color>,
}
impl StyleRefinement {
    pub fn bg(mut self, b: impl Into<Background>) -> Self {
        self.bg = Some(match b.into() {
            Background::Color(c) => c,
            Background::Gradient(g) => g.from.0.color(),
        });
        self
    }
    pub fn border_color(mut self, c: impl IntoColor) -> Self {
        self.border_color = Some(c.into_color());
        self
    }
    pub fn text_color(mut self, c: impl IntoColor) -> Self {
        self.text_color = Some(c.into_color());
        self
    }
    pub fn opacity(mut self, _: f32) -> Self {
        self.opacity_set = true;
        self
    }
    pub fn cursor_pointer(self) -> Self {
        self
    }
}

// ---------------------------------------------------------------- focus

/// A focusable element's key, as GPUI's focus handle names an element.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FocusHandle(pub String);
impl FocusHandle {
    pub fn tab_stop(self, _: bool) -> Self {
        self
    }
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
    pub fn focus(&self, window: &mut Window) {
        window.effects.push(Effect::Focus(self.0.clone()));
    }
    /// [`Self::focus`] for a field the user is about to type in right
    /// where they tapped (text placed on the canvas), which takes focus
    /// even from the pen.
    pub fn focus_for_typing(&self, window: &mut Window) {
        window.effects.push(Effect::FocusTyping(self.0.clone()));
    }
    pub fn is_focused(&self, window: &Window) -> bool {
        window.focused.as_deref() == Some(self.0.as_str())
    }
}

// ---------------------------------------------------------------- effects

/// What a handler asks of the window, applied through Rune's transaction.
pub enum Effect {
    Focus(String),
    FocusTyping(String),
    Blur,
    Copy(String),
    Title(String),
    Quit,
    Minimize,
    ToggleMaximized,
    Close,
    ScrollIntoView(String),
    ScrollTo(String, f32, f32),
    OpenUrl(String),
    Paste(Rc<dyn Fn(&mut NotesView, rune::ClipboardContent, &mut Window, &mut Context<NotesView>)>),
    OpenFiles(
        rune::FileDialogOptions,
        Rc<
            dyn Fn(
                &mut NotesView,
                Option<std::path::PathBuf>,
                &mut Window,
                &mut Context<NotesView>,
            ),
        >,
    ),
    SaveFile(
        rune::FileDialogOptions,
        Rc<
            dyn Fn(
                &mut NotesView,
                Option<std::path::PathBuf>,
                &mut Window,
                &mut Context<NotesView>,
            ),
        >,
    ),
    OpenFolder(
        rune::FileDialogOptions,
        Rc<
            dyn Fn(
                &mut NotesView,
                Option<std::path::PathBuf>,
                &mut Window,
                &mut Context<NotesView>,
            ),
        >,
    ),
    WakeAfter(std::time::Duration),
    Instant,
    /// Animate this commit even where Rune would land it at once (a drag's
    /// moves), so rows a dragged row passes spring aside.
    Animated,
}

/// The window as handlers and views see it: its size, the pointer and focus,
/// and the effects queued for Rune.
pub struct Window {
    pub viewport: Size<Pixels>,
    pub mouse: Point<Pixels>,
    pub scale: f32,
    pub focused: Option<String>,
    pub dark: bool,
    pub hovered: bool,
    pub maximized: bool,
    pub effects: Vec<Effect>,
    pub raster: Option<crate::paint::Raster>,
}
impl Window {
    pub fn handler(view: &NotesView, tx: &Tx) -> Self {
        Self {
            viewport: view.viewport,
            mouse: view.mouse_position,
            scale: view.scale,
            focused: tx.focused().map(str::to_owned),
            dark: view.system_dark,
            hovered: view.window_hovered,
            maximized: crate::titlebar::window_maximized(),
            effects: Vec::new(),
            raster: None,
        }
    }
    pub fn render(view: &NotesView) -> Self {
        Self {
            viewport: view.viewport,
            mouse: view.mouse_position,
            scale: view.scale,
            focused: view.focused_key.clone(),
            dark: view.system_dark,
            hovered: view.window_hovered,
            maximized: crate::titlebar::window_maximized(),
            effects: Vec::new(),
            raster: None,
        }
    }
    /// A window that paints a canvas at `scale` device pixels per point.
    pub fn paint(scale: f32) -> Self {
        Self {
            viewport: Size::default(),
            mouse: Point::default(),
            scale,
            focused: None,
            dark: false,
            hovered: false,
            maximized: false,
            effects: Vec::new(),
            raster: None,
        }
    }
    pub fn viewport_size(&self) -> Size<Pixels> {
        self.viewport
    }
    pub fn mouse_position(&self) -> Point<Pixels> {
        self.mouse
    }
    pub fn scale_factor(&self) -> f32 {
        self.scale
    }
    pub fn is_window_hovered(&self) -> bool {
        self.hovered
    }
    pub fn is_maximized(&self) -> bool {
        self.maximized
    }
    pub fn refresh(&mut self) {}
    pub fn request_animation_frame(&mut self) {
        self.effects
            .push(Effect::WakeAfter(std::time::Duration::from_millis(16)));
    }
    pub fn set_window_title(&mut self, title: &str) {
        self.effects.push(Effect::Title(title.to_owned()));
    }
    pub fn minimize_window(&mut self) {
        self.effects.push(Effect::Minimize);
    }
    pub fn zoom_window(&mut self) {
        self.effects.push(Effect::ToggleMaximized);
    }
    pub fn remove_window(&mut self) {
        self.effects.push(Effect::Close);
    }
    pub fn blur(&mut self) {
        self.effects.push(Effect::Blur);
    }
    pub fn focus_key(&mut self, key: impl Into<String>) {
        self.effects.push(Effect::Focus(key.into()));
    }
    pub fn scroll_into_view(&mut self, key: impl Into<String>) {
        self.effects.push(Effect::ScrollIntoView(key.into()));
    }
    pub fn scroll_to(&mut self, key: impl Into<String>, x: f32, y: f32) {
        self.effects.push(Effect::ScrollTo(key.into(), x, y));
    }
}

/// The view's context: GPUI's `cx`, reduced to what handlers queue.
pub struct Context<T> {
    pub effects: Vec<Effect>,
    pub stopped: bool,
    _p: PhantomData<T>,
}
impl<T> Context<T> {
    pub fn fresh() -> Self {
        Self {
            effects: Vec::new(),
            stopped: false,
            _p: PhantomData,
        }
    }
    /// Rune re-runs the view after every handler.
    pub fn notify(&mut self) {}
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }
    pub fn propagate(&mut self) {
        self.stopped = false;
    }
    pub fn listener<E: 'static>(
        &self,
        f: impl Fn(&mut NotesView, &E, &mut Window, &mut Context<NotesView>) + 'static,
    ) -> Listener<E> {
        Listener(Rc::new(f))
    }
    pub fn write_to_clipboard(&mut self, item: ClipboardItem) {
        self.effects.push(Effect::Copy(item.0));
    }
    pub fn quit(&mut self) {
        self.effects.push(Effect::Quit);
    }
    pub fn open_url(&mut self, url: &str) {
        self.effects.push(Effect::OpenUrl(url.to_owned()));
    }
    /// Reads the clipboard; `f` runs with its content on a later frame.
    pub fn read_clipboard(
        &mut self,
        f: impl Fn(&mut NotesView, rune::ClipboardContent, &mut Window, &mut Context<NotesView>)
        + 'static,
    ) {
        self.effects.push(Effect::Paste(Rc::new(f)));
    }
    pub fn open_file(
        &mut self,
        options: rune::FileDialogOptions,
        f: impl Fn(&mut NotesView, Option<std::path::PathBuf>, &mut Window, &mut Context<NotesView>)
        + 'static,
    ) {
        self.effects.push(Effect::OpenFiles(options, Rc::new(f)));
    }
    pub fn save_file(
        &mut self,
        options: rune::FileDialogOptions,
        f: impl Fn(&mut NotesView, Option<std::path::PathBuf>, &mut Window, &mut Context<NotesView>)
        + 'static,
    ) {
        self.effects.push(Effect::SaveFile(options, Rc::new(f)));
    }
    pub fn open_folder(
        &mut self,
        options: rune::FileDialogOptions,
        f: impl Fn(&mut NotesView, Option<std::path::PathBuf>, &mut Window, &mut Context<NotesView>)
        + 'static,
    ) {
        self.effects.push(Effect::OpenFolder(options, Rc::new(f)));
    }
    pub fn wake_after(&mut self, delay: std::time::Duration) {
        self.effects.push(Effect::WakeAfter(delay));
    }
    pub fn animate(&mut self) {
        self.effects.push(Effect::Animated);
    }
}
impl<T> Default for Context<T> {
    fn default() -> Self {
        Self::fresh()
    }
}

pub struct ClipboardItem(pub String);
impl ClipboardItem {
    pub fn new_string(s: String) -> Self {
        Self(s)
    }
}

/// Effects queued during the view build, applied in the next handler or poll.
#[derive(Default)]
pub struct Deferred(pub RefCell<Vec<Effect>>);

/// Applies queued effects to Rune's transaction.
pub fn apply(effects: Vec<Effect>, tx: &mut Tx) {
    for effect in effects {
        match effect {
            // A text field a pen action opened (a rename, a search) takes no
            // focus of its own: with a pen as the last input, GNOME opens
            // its on-screen keyboard for any focused field. Tapping the
            // field still focuses it.
            Effect::Focus(key) if key.starts_with("field-") && crate::tablet::pen_active() => {}
            Effect::Focus(key) | Effect::FocusTyping(key) => tx.focus_when_shown(key, false),
            Effect::Blur => tx.blur(),
            Effect::Copy(text) => tx.copy(text),
            Effect::Title(title) => tx.set_title(title),
            Effect::Quit => tx.quit(),
            Effect::Minimize => tx.minimize(),
            Effect::ToggleMaximized => tx.toggle_maximized(),
            Effect::Close => tx.close_window(),
            Effect::ScrollIntoView(key) => tx.scroll_into_view(key),
            Effect::ScrollTo(key, x, y) => tx.scroll_to(key, x, y),
            Effect::OpenUrl(url) => {
                let _ = tx.open_url(&url);
            }
            Effect::Paste(f) => tx.read_clipboard(move |s: &mut crate::AppState, content, tx| {
                dispatch(s, tx, |view, window, cx| {
                    f(view, content.clone(), window, cx)
                });
            }),
            Effect::OpenFiles(options, f) => {
                tx.open_file(options, move |s: &mut crate::AppState, result, tx| {
                    let path = match result {
                        Ok(path) => path,
                        Err(error) => {
                            s.view.get_mut().controller.error = Some(error.to_string());
                            None
                        }
                    };
                    if path.is_some() {
                        dispatch(s, tx, |view, window, cx| f(view, path, window, cx));
                    }
                })
            }
            Effect::SaveFile(options, f) => {
                tx.save_file(options, move |s: &mut crate::AppState, result, tx| {
                    let path = match result {
                        Ok(path) => path,
                        Err(error) => {
                            s.view.get_mut().controller.error = Some(error.to_string());
                            None
                        }
                    };
                    if path.is_some() {
                        dispatch(s, tx, |view, window, cx| f(view, path, window, cx));
                    }
                })
            }
            Effect::OpenFolder(options, f) => {
                tx.open_folder(options, move |s: &mut crate::AppState, result, tx| {
                    let path = match result {
                        Ok(path) => path,
                        Err(error) => {
                            s.view.get_mut().controller.error = Some(error.to_string());
                            None
                        }
                    };
                    if path.is_some() {
                        dispatch(s, tx, |view, window, cx| f(view, path, window, cx));
                    }
                })
            }
            Effect::WakeAfter(delay) => tx.wake_after(delay),
            Effect::Instant => tx.instant(),
            Effect::Animated => tx.animated(),
        }
    }
}

// ---------------------------------------------------------------- keys

/// A key press as GPUI names it: `keystroke.key` is "left", "enter", "a".
#[derive(Clone, Debug)]
pub struct Keystroke {
    pub key: String,
    pub modifiers: Modifiers,
    pub key_char: Option<String>,
}
#[derive(Clone, Debug)]
pub struct KeyDownEvent {
    pub keystroke: Keystroke,
    pub is_held: bool,
}
impl KeyDownEvent {
    pub fn from_rune(ev: &rune::KeyEvent) -> Option<Self> {
        use rune::Key;
        let mut modifiers = Modifiers::from_mods(ev.mods);
        let (key, key_char) = match &ev.key {
            Key::Text(t) => {
                let lower = t.to_lowercase();
                (
                    if lower == " " {
                        "space".to_owned()
                    } else {
                        lower
                    },
                    Some(t.clone()),
                )
            }
            Key::Command(c) => {
                modifiers.control = true;
                (c.to_lowercase().to_string(), None)
            }
            Key::Enter => ("enter".into(), None),
            Key::Backspace => ("backspace".into(), None),
            Key::Delete => ("delete".into(), None),
            Key::Left => ("left".into(), None),
            Key::Right => ("right".into(), None),
            Key::Up => ("up".into(), None),
            Key::Down => ("down".into(), None),
            Key::Home => ("home".into(), None),
            Key::End => ("end".into(), None),
            Key::PageUp => ("pageup".into(), None),
            Key::PageDown => ("pagedown".into(), None),
            Key::Escape => ("escape".into(), None),
            Key::Tab => ("tab".into(), None),
            Key::Function(n) => (format!("f{n}"), None),
            _ => return None,
        };
        Some(Self {
            keystroke: Keystroke {
                key,
                modifiers,
                key_char,
            },
            is_held: false,
        })
    }
    /// GPUI's keystroke string, such as "ctrl-shift-z".
    pub fn chord(&self) -> String {
        let m = &self.keystroke.modifiers;
        let mut s = String::new();
        if m.control {
            s.push_str("ctrl-");
        }
        if m.alt {
            s.push_str("alt-");
        }
        // Shift is part of a typed character itself ("+"), not a modifier.
        if m.shift
            && (self.keystroke.key.chars().count() > 1
                || self.keystroke.key.chars().all(|c| c.is_ascii_alphabetic())
                || m.control)
        {
            s.push_str("shift-");
        }
        s.push_str(&self.keystroke.key);
        s
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ScrollDelta {
    Pixels(Point<Pixels>),
    Lines(Point<f32>),
}
impl ScrollDelta {
    pub fn pixel_delta(&self, line: Pixels) -> Point<Pixels> {
        match self {
            ScrollDelta::Pixels(p) => *p,
            ScrollDelta::Lines(l) => point(px(l.x * line.0), px(l.y * line.0)),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ScrollWheelEvent {
    pub position: Point<Pixels>,
    pub delta: ScrollDelta,
    pub modifiers: Modifiers,
    pub local: Point<Pixels>,
}
impl ScrollWheelEvent {
    pub fn from_rune(w: &rune::WheelEvent, window_position: Point<Pixels>) -> Self {
        let delta = if w.precise {
            ScrollDelta::Pixels(point(px(w.dx), px(w.dy)))
        } else {
            // Rune reports a notch as points; GPUI's lines are 30 px apart here.
            ScrollDelta::Lines(point(w.dx / 30., w.dy / 30.))
        };
        Self {
            position: window_position,
            delta,
            modifiers: Modifiers::from_mods(w.mods),
            local: point(px(w.x), px(w.y)),
        }
    }
}

// ---------------------------------------------------------------- actions

pub trait Action: 'static + Default {
    fn name() -> &'static str;
}

#[macro_export]
macro_rules! actions {
    ($ns:ident, [$($name:ident),* $(,)?]) => {
        $(
            #[derive(Clone, Copy, Default, Debug, PartialEq)]
            pub struct $name;
            impl $crate::compat::Action for $name {
                fn name() -> &'static str { stringify!($name) }
            }
        )*
    };
}

type ActionHandler = Rc<dyn Fn(&mut NotesView, &mut Window, &mut Context<NotesView>)>;
thread_local! {
    static ACTIONS: RefCell<std::collections::HashMap<&'static str, ActionHandler>> = RefCell::new(Default::default());
}
pub fn clear_actions() {
    ACTIONS.with(|a| a.borrow_mut().clear());
}
fn register_action<A: Action>(listener: Listener<A>) {
    let f: ActionHandler =
        Rc::new(move |view, window, cx| (listener.0)(view, &A::default(), window, cx));
    ACTIONS.with(|a| {
        a.borrow_mut().insert(A::name(), f);
    });
}
/// The handler the current view registered for the action named `name`.
pub fn action_handler(name: &str) -> Option<ActionHandler> {
    ACTIONS.with(|a| a.borrow().get(name).cloned())
}

/// A key binding: a GPUI keystroke string, an action and a context predicate.
pub struct KeyBinding {
    pub keys: &'static str,
    pub action: &'static str,
    pub context: Option<&'static str>,
}
impl KeyBinding {
    pub fn new<A: Action>(keys: &'static str, _: A, context: Option<&'static str>) -> Self {
        Self {
            keys,
            action: A::name(),
            context,
        }
    }
    /// Whether a predicate such as "Folio && !FolioField" holds for `contexts`.
    pub fn matches_context(&self, contexts: &[&str]) -> bool {
        let Some(predicate) = self.context else {
            return true;
        };
        predicate.split("&&").all(|term| {
            let term = term.trim();
            match term.strip_prefix('!') {
                Some(name) => !contexts.contains(&name.trim()),
                None => contexts.contains(&term),
            }
        })
    }
}

// ---------------------------------------------------------------- more GPUI

pub type Stateful<T> = T;
pub type EntityId = usize;
pub trait Render {}
pub struct Subscription;

/// Folio's lists built rows from the view lazily; here every row is built.
pub struct Processor<R>(
    Rc<
        dyn Fn(
            &mut NotesView,
            std::ops::Range<usize>,
            &mut Window,
            &mut Context<NotesView>,
        ) -> Vec<R>,
    >,
);
impl<T> Context<T> {
    pub fn processor<R: 'static>(
        &self,
        f: impl Fn(
            &mut NotesView,
            std::ops::Range<usize>,
            &mut Window,
            &mut Context<NotesView>,
        ) -> Vec<R>
        + 'static,
    ) -> Processor<R> {
        Processor(Rc::new(f))
    }
}
/// A list of `count` uniform rows from `processor`, in a scroll viewport.
/// Where each virtual list was scrolled and which rows it built.
#[derive(Clone, Copy, Default)]
struct VirtualState {
    /// Scroll offset and viewport height, in the list's layout units.
    offset: f32,
    viewport: f32,
    built: (usize, usize),
    /// The offset the rows were last built at.
    built_at: f32,
}
thread_local! {
    static VIRTUAL: RefCell<std::collections::HashMap<String, VirtualState>> =
        RefCell::new(std::collections::HashMap::new());
}
thread_local! {
    static SCROLL_BUILD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Whether the rows being built are there because their list scrolled:
/// they appear in place, without an entrance.
pub fn building_scrolled_rows() -> bool {
    SCROLL_BUILD.with(|b| b.get())
}
static VIRTUAL_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Whether a virtual list scrolled to rows it has not built, so the view
/// must be built again.
pub fn take_virtual_dirty() -> bool {
    VIRTUAL_DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed)
}
/// Rows `[start, end)` a list of `count` rows `row` tall shows at `offset`
/// in a `viewport`, with a viewport's worth of rows beyond each edge.
fn virtual_range(count: usize, row: f32, offset: f32, viewport: f32) -> (usize, usize) {
    if count == 0 || row <= 0. {
        return (0, 0);
    }
    let viewport = if viewport > 0. { viewport } else { 1600. };
    let first = ((offset - viewport) / row).floor().max(0.) as usize;
    let last = ((offset + viewport * 2.) / row).ceil().max(0.) as usize;
    (first.min(count), last.min(count).max(first.min(count)))
}
/// A scrolling list of `count` rows of equal height `row` (layout points)
/// that builds only the rows near the viewport, with spacers keeping the
/// scroll extent; scrolling past the built rows builds the view again.
pub fn virtual_list<R: IntoElement + 'static>(
    id: impl Into<String>,
    count: usize,
    row: f32,
    processor: Processor<R>,
) -> Div {
    let id: String = id.into();
    let key = id.clone();
    let row_prefix = id.clone();
    let watch = id.clone();
    div()
        .id(id)
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .op(move |e| {
            e.on_scroll_offset(move |at: rune::ScrollPos| {
                // Measure in rows: the content's height over the row count
                // is a row's real height, whatever the zoom.
                let content = at.max[1] + at.viewport[1];
                let per_row = if count > 0 && content > 0. { content / count as f32 } else { row };
                let (offset, viewport) = (at.offset[1] / per_row * row, at.viewport[1] / per_row * row);
                let stale = VIRTUAL.with(|v| {
                    let mut v = v.borrow_mut();
                    let state = v.entry(watch.clone()).or_default();
                    state.offset = offset;
                    state.viewport = viewport;
                    let need = virtual_range(count, row, offset, viewport * 0.34);
                    need.0 < state.built.0 || need.1 > state.built.1
                });
                if stale {
                    VIRTUAL_DIRTY.store(true, std::sync::atomic::Ordering::Relaxed);
                    crate::executor::wake();
                }
            })
        })
        .child(AnyElement::Lazy(Box::new(move |bcx: &mut BuildCx| {
            let state = VIRTUAL.with(|v| v.borrow().get(&key).copied().unwrap_or_default());
            let (start, end) = virtual_range(count, row, state.offset, state.viewport);
            VIRTUAL.with(|v| {
                let mut v = v.borrow_mut();
                let entry = v.entry(key.clone()).or_default();
                entry.built = (start, end);
                entry.built_at = state.offset;
            });
            let scrolled = (state.offset - state.built_at).abs() > 0.5;
            SCROLL_BUILD.with(|b| b.set(scrolled));
            let rows = (processor.0)(bcx.view, start..end, bcx.window, bcx.cx);
            SCROLL_BUILD.with(|b| b.set(false));
            let mut list = div()
                .flex()
                .flex_col()
                .w_full()
                .flex_shrink_0()
                .child(div().h(px(start as f32 * row)).flex_shrink_0());
            for (i, r) in rows.into_iter().enumerate() {
                list = list.child(
                    div()
                        .id(format!("{row_prefix}-row-{}", start + i))
                        .flex_shrink_0()
                        .child(r),
                );
            }
            list.child(div().h(px((count - end) as f32 * row)).flex_shrink_0())
                .into_any_element()
        })))
}
pub fn uniform_list<R: IntoElement + 'static>(
    id: impl Into<String>,
    count: usize,
    processor: Processor<R>,
) -> Div {
    let id = id.into();
    let row_prefix = id.clone();
    div()
        .id(id)
        .flex()
        .flex_col()
        .child(AnyElement::Lazy(Box::new(move |bcx: &mut BuildCx| {
            let rows = (processor.0)(bcx.view, 0..count, bcx.window, bcx.cx);
            let mut list = div().flex().flex_col().w_full().flex_shrink_0();
            for (i, row) in rows.into_iter().enumerate() {
                list = list.child(
                    div()
                        .id(format!("{row_prefix}-row-{i}"))
                        .flex_shrink_0()
                        .child(row),
                );
            }
            list.into_any_element()
        })))
}
impl Div {
    /// Ties this scroll viewport to `handle`: its key, and rows it was asked to reveal.
    pub fn track_scroll(self, handle: impl std::borrow::Borrow<crate::ScrollHandle>) -> Self {
        let handle = handle.borrow().clone();
        let key = handle.key.clone();
        let mut this = if key.is_empty() {
            self
        } else {
            self.id(key.clone())
        };
        this.scroll_y = this.scroll_y || !this.scroll_x;
        let position = handle.position.clone();
        let thumb = handle.thumb.clone();
        this = this.op(move |e| {
            e.on_scroll_offset(move |at: rune::ScrollPos| {
                position.set(Some(at));
                // The thumb Folio's settings draw: 4 pt inside the viewport.
                let (viewport, range) = (at.viewport[1], at.max[1]);
                if viewport > 0. && range > 0. {
                    let length = (viewport * viewport / (viewport + range)).max(32.);
                    let progress = (at.offset[1] / range).clamp(0., 1.);
                    thumb.set([0., (viewport - length - 8.) * progress]);
                }
            })
        });
        this.child(AnyElement::Lazy(Box::new(move |bcx: &mut BuildCx| {
            if let Some(index) = handle.take_request() {
                bcx.cx
                    .effects
                    .push(Effect::ScrollIntoView(format!("{key}-row-{index}")));
            }
            AnyElement::Empty
        })))
    }
    pub fn with_sizing_behavior<T>(self, _: T) -> Self {
        self
    }
    pub fn with_priority(self, _priority: i16) -> Self {
        // Deferred elements are raised in tree order.
        self
    }
    pub fn tooltip(self, text: impl Into<String>) -> Self {
        self.tooltip_text(text)
    }
    pub fn border_0(mut self) -> Self {
        self.border = [0.; 4];
        self
    }
}
thread_local! {
    static ROLLING: RefCell<std::collections::HashMap<String, f64>> = RefCell::new(std::collections::HashMap::new());
}
/// A label whose changed characters roll, up as `value` rises and down as it
/// falls (a page count, a zoom level). Characters count from the end, so a
/// value that gains a digit keeps its unchanged tail in place.
pub fn rolling_text(key: &str, text: &str, value: f64) -> Div {
    let rising = ROLLING.with(|last| {
        let mut last = last.borrow_mut();
        let previous = last.insert(key.to_owned(), value);
        previous.is_none_or(|p| value >= p)
    });
    let chars: Vec<char> = text.chars().collect();
    let count = chars.len();
    let mut row = div().flex().overflow_hidden().el(|e| {
        e.motion(crate::motion::GLIDE).animated_changes()
    });
    for (i, ch) in chars.into_iter().enumerate() {
        let from_end = count - i;
        let travel = if rising {
            rune::Transition::push(rune::Edge::Bottom)
        } else {
            rune::Transition::push(rune::Edge::Top)
        };
        row = row.child(
            div()
                .id(format!("{key}-{from_end}-{ch}"))
                .child(ch.to_string())
                .el(move |e| {
                    e.transition(travel.springs(rune::SpringPair::new(
                        crate::motion::POP,
                        crate::motion::GLIDE,
                    )))
                    .motion(crate::motion::GLIDE)
                    .animated_changes()
                }),
        );
    }
    row
}

/// Three small dots that pulse in turn: work under way (recognition,
/// solving), where a bare "…" in text would read as static.
pub fn working_dots(color: u32) -> Div {
    let mut dots = div().flex().items_center().gap(px(3.)).flex_shrink_0();
    for i in 0..3 {
        dots = dots.child(
            div()
                .size(px(5.))
                .rounded_full()
                .bg(rgb(color))
                .el(move |e| e.looping(rune::LoopSpec::opacity(1.1, i as f64 * 0.18, 0.25, 1.))),
        );
    }
    dots
}

/// Draws `child` above everything after it, as GPUI's deferred elements do.
pub fn deferred(child: impl IntoElement) -> Div {
    div().child(child).op_public(|e| e.raise())
}
impl Div {
    pub fn op_public(self, f: impl FnOnce(El) -> El + 'static) -> Self {
        self.op(f)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HighlightStyle {
    pub color: Option<Rgba>,
    pub font_weight: Option<FontWeight>,
    pub background_color: Option<Rgba>,
    pub underline: Option<()>,
}
/// Text with highlighted ranges.
pub struct StyledText {
    text: String,
    highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
}
impl StyledText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            highlights: Vec::new(),
        }
    }
    pub fn with_highlights(
        mut self,
        h: impl IntoIterator<Item = (std::ops::Range<usize>, HighlightStyle)>,
    ) -> Self {
        self.highlights = h.into_iter().collect();
        self
    }
}
impl IntoElement for StyledText {
    fn into_any_element(self) -> AnyElement {
        let spans = self
            .highlights
            .into_iter()
            .map(|(range, style)| {
                let mut span = rune::Span::new(range.start, range.end);
                if let Some(c) = style.color {
                    span.color = Some(c.color());
                }
                if let Some(w) = style.font_weight {
                    span.weight = Some(w.value());
                }
                if let Some(c) = style.background_color {
                    span.bg = Some(c.color());
                }
                span
            })
            .collect();
        AnyElement::Styled(self.text, spans)
    }
}

/// A two-stop gradient, as Folio's GPUI views used it.
#[derive(Clone, Copy, Debug)]
pub struct LinearGradient {
    pub angle: f32,
    pub from: (Rgba, f32),
    pub to: (Rgba, f32),
}
pub fn linear_color_stop(color: impl Into<Rgba>, at: f32) -> (Rgba, f32) {
    (color.into(), at)
}
pub fn linear_gradient(angle: f32, from: (Rgba, f32), to: (Rgba, f32)) -> LinearGradient {
    LinearGradient { angle, from, to }
}
impl Div {
    pub fn bg_gradient(self, g: LinearGradient) -> Self {
        let stops = vec![(g.from.1, g.from.0.color()), (g.to.1, g.to.0.color())];
        let angle = g.angle.to_radians();
        self.op(move |e| e.linear_gradient(angle, stops))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CursorStyle {
    Arrow,
    PointingHand,
    IBeam,
    Crosshair,
    None,
    ResizeLeftRight,
    ResizeUpDown,
    OpenHand,
    ClosedHand,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct MouseMoveEvent {
    pub position: Point<Pixels>,
    pub pressed_button: Option<MouseButton>,
    pub modifiers: Modifiers,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct MouseUpEvent {
    pub position: Point<Pixels>,
    pub button: Option<MouseButton>,
    pub modifiers: Modifiers,
    pub click_count: usize,
}
pub struct ExternalPaths(pub Vec<std::path::PathBuf>);
impl ExternalPaths {
    pub fn paths(&self) -> &[std::path::PathBuf] {
        &self.0
    }
}

impl Div {
    pub fn max_w_full(self) -> Self {
        self
    }
    pub fn flex_initial(self) -> Self {
        self
    }
    pub fn top_full(mut self) -> Self {
        self.full = Some(rune::Side::Bottom);
        self
    }
    pub fn object_fit(self, _: crate::paint::ObjectFit) -> Self {
        self
    }
}

// Pen and pad reports from Folio's winit tablet extension (`tablet.rs`),
// in view points with adapted force and tilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabletPhase {
    Hover,
    Down,
    Move,
    Up,
    Leave,
    Cancel,
}
#[derive(Clone, Debug)]
pub struct TabletEvent {
    pub phase: TabletPhase,
    pub position: Point<Pixels>,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub buttons: u32,
    pub timestamp: u32,
    pub eraser: bool,
}
#[derive(Clone, Debug)]
pub struct TabletPadEvent {
    pub button: Option<u32>,
    pub pressed: bool,
    pub strip: bool,
    pub delta: f32,
    pub timestamp: u32,
}

impl Div {
    /// GPUI's press-outside handler: here the element becomes modal, and a
    /// press outside it (or Escape) runs `listener`.
    pub fn on_mouse_down_out(self, listener: Listener<MouseDownEvent>) -> Self {
        self.op(move |e| {
            e.modal_tx(true, move |s: &mut crate::AppState, tx: &mut Tx| {
                let event = MouseDownEvent {
                    position: s.view.get_mut().mouse_position,
                    modifiers: Modifiers::from_mods(tx.mods()),
                    click_count: 1,
                };
                let f = listener.0.clone();
                dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
            })
        })
    }
}

pub type KeyUpEvent = KeyDownEvent;
impl Div {
    pub fn on_key_up(self, listener: Listener<KeyUpEvent>) -> Self {
        self.op(move |e| {
            e.on_key_up(
                move |s: &mut crate::AppState, key: &rune::KeyEvent, tx: &mut Tx| {
                    let Some(event) = KeyDownEvent::from_rune(key) else {
                        return false;
                    };
                    let f = listener.0.clone();
                    dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
                    false
                },
            )
        })
    }
}

// ---------------------------------------------------------------- drag and drop

/// A value dragged between elements, with the preview drawn at the pointer.
pub trait DragView: 'static {
    fn preview(&self) -> Div;
}
struct DragState {
    value: Rc<dyn std::any::Any>,
    preview: Rc<dyn Fn() -> Div>,
    type_id: std::any::TypeId,
    origin: Point<Pixels>,
    position: Point<Pixels>,
}
type DropHandler =
    Rc<dyn Fn(&mut NotesView, &dyn std::any::Any, &mut Window, &mut Context<NotesView>)>;
thread_local! {
    static DRAG: RefCell<Option<DragState>> = const { RefCell::new(None) };
    static DROP_TARGETS: RefCell<Vec<(String, std::any::TypeId, DropHandler)>> = const { RefCell::new(Vec::new()) };
    static DROP_RECTS: RefCell<std::collections::HashMap<String, [f32; 4]>> = RefCell::new(Default::default());
    static NEXT_DROP: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// Starts a frame's drop targets afresh.
pub fn begin_drops() {
    DROP_TARGETS.with(|t| t.borrow_mut().clear());
    NEXT_DROP.with(|n| n.set(0));
}
/// The innermost drop target for the dragged type under `p`.
fn drop_target_at(p: Point<Pixels>, type_id: std::any::TypeId) -> Option<(String, DropHandler)> {
    let targets = DROP_TARGETS.with(|t| t.borrow().clone());
    DROP_RECTS.with(|rects| {
        let rects = rects.borrow();
        targets
            .into_iter()
            .filter(|(_, ty, _)| *ty == type_id)
            .filter_map(|(key, _, handler)| {
                let r = rects.get(&key)?;
                (p.x.0 >= r[0] && p.y.0 >= r[1] && p.x.0 < r[0] + r[2] && p.y.0 < r[1] + r[3])
                    .then(|| (r[2] * r[3], key, handler))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, key, handler)| (key, handler))
    })
}
fn hovered_drop(type_id: std::any::TypeId) -> Option<String> {
    DRAG.with(|d| d.borrow().as_ref().map(|d| d.position))
        .and_then(|p| drop_target_at(p, type_id))
        .map(|(key, _)| key)
}
/// The dragged value's preview at the pointer, while something is dragged.
pub fn drag_preview() -> Option<Div> {
    DRAG.with(|d| {
        d.borrow().as_ref().map(|d| {
            div()
                .absolute()
                .left(d.position.x + px(8.))
                .top(d.position.y + px(8.))
                .child((d.preview)())
                .op_public(|e| e.paint_order(1000))
        })
    })
}
impl Div {
    pub fn on_drag<T: DragView + Clone, B>(self, value: T, _builder: B) -> Self {
        let value = Rc::new(value);
        self.op(move |e| {
            e.on_drag(
                move |s: &mut crate::AppState, drag: &mut rune::Drag, tx: &mut Tx| {
                    let view = s.view.get_mut();
                    match drag.phase {
                        rune::Phase::Down => {}
                        rune::Phase::Move => {
                            let started = DRAG.with(|d| d.borrow().is_some());
                            if !started {
                                let preview_value = value.clone();
                                let state = DragState {
                                    value: value.clone(),
                                    preview: Rc::new(move || preview_value.preview()),
                                    type_id: std::any::TypeId::of::<T>(),
                                    origin: view.mouse_position,
                                    position: view.mouse_position,
                                };
                                DRAG.with(|d| *d.borrow_mut() = Some(state));
                            }
                            DRAG.with(|d| {
                                if let Some(d) = d.borrow_mut().as_mut() {
                                    d.position =
                                        point(d.origin.x + px(drag.dx), d.origin.y + px(drag.dy));
                                }
                            });
                        }
                        rune::Phase::Up => {
                            let state = DRAG.with(|d| d.borrow_mut().take());
                            if let Some(state) = state
                                && !drag.cancelled()
                                && let Some((_, handler)) =
                                    drop_target_at(state.position, state.type_id)
                            {
                                let value = state.value.clone();
                                dispatch(s, tx, |view, window, cx| {
                                    handler(view, value.as_ref(), window, cx)
                                });
                            }
                        }
                    }
                },
            )
        })
    }
    pub fn on_drop<T: 'static>(mut self, listener: Listener<T>) -> Self {
        let key = self.key.clone().unwrap_or_else(|| {
            let n = NEXT_DROP.with(|n| {
                let v = n.get();
                n.set(v + 1);
                v
            });
            format!("drop-target-{n}")
        });
        self.key = Some(key.clone());
        let handler: DropHandler = Rc::new(move |view, value, window, cx| {
            if let Some(value) = value.downcast_ref::<T>() {
                (listener.0)(view, value, window, cx);
            }
        });
        DROP_TARGETS.with(|t| {
            t.borrow_mut()
                .push((key.clone(), std::any::TypeId::of::<T>(), handler))
        });
        self.op(move |e| {
            e.on_layout(move |_: &mut crate::AppState, r: [f32; 4]| {
                DROP_RECTS.with(|rects| {
                    rects.borrow_mut().insert(key.clone(), r);
                });
            })
        })
    }
    pub fn drag_over<T: 'static>(
        mut self,
        f: impl FnOnce(StyleRefinement, &T, &mut Window, &mut Context<NotesView>) -> StyleRefinement,
    ) -> Self {
        let Some(key) = self.key.clone() else {
            return self;
        };
        if hovered_drop(std::any::TypeId::of::<T>()).as_deref() == Some(key.as_str()) {
            let value = DRAG.with(|d| d.borrow().as_ref().map(|d| d.value.clone()));
            if let Some(value) = value
                && let Some(value) = value.downcast_ref::<T>()
            {
                let mut window = Window::paint(1.);
                let mut cx = Context::fresh();
                let style = f(StyleRefinement::default(), value, &mut window, &mut cx);
                if let Some(bg) = style.bg {
                    self.bg = Some(bg);
                }
                if let Some(c) = style.border_color {
                    self.border_color = Some(c);
                    if self.border == [0.; 4] {
                        self.border = [1.; 4];
                    }
                }
            }
        }
        self
    }
    pub fn can_drop<F>(self, _: F) -> Self {
        self
    }
}
impl StyleRefinement {
    pub fn border_1(self) -> Self {
        self
    }
}

impl Div {
    pub fn rounded_none(mut self) -> Self {
        self.radius = Some([0.; 4]);
        self
    }
    pub fn p_7(self) -> Self {
        self.p(px(1.75 * REM))
    }
    pub fn bottom_full(mut self) -> Self {
        self.full = Some(rune::Side::Top);
        self
    }
    /// GPUI's hover group: Rune tracks hover per element, so a group is
    /// the element itself.
    pub fn group(self, _: &str) -> Self {
        self.op(|e| e.hover_group())
    }
    pub fn group_hover(self, _: &str, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        let style = f(StyleRefinement::default());
        if style.opacity_set {
            self.op(|e| e.group_opacity(0., 1.))
        } else {
            self
        }
    }
    pub fn on_mouse_up(self, button: MouseButton, listener: Listener<MouseUpEvent>) -> Self {
        // Rune presses on release inside the element.
        match button {
            MouseButton::Left => self.op(move |e| {
                e.on_press_tx(move |s: &mut crate::AppState, tx: &mut Tx| {
                    let event = MouseUpEvent {
                        position: s.view.get_mut().mouse_position,
                        button: Some(MouseButton::Left),
                        modifiers: Modifiers::from_mods(tx.mods()),
                        click_count: 1,
                    };
                    let f = listener.0.clone();
                    dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
                })
            }),
            _ => self,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    Down,
    Move,
    Up,
}
/// A press-drag-release on an element, in window points.
#[derive(Clone, Copy, Debug)]
pub struct DragEvent {
    pub phase: DragPhase,
    pub position: Point<Pixels>,
    pub cancelled: bool,
}
thread_local! {
    static DRAG_ORIGIN: std::cell::Cell<Point<Pixels>> = const { std::cell::Cell::new(Point { x: Pixels(0.), y: Pixels(0.) }) };
}
impl Div {
    /// Every phase of a press that may move, in window points.
    pub fn on_pointer_drag(self, listener: Listener<DragEvent>) -> Self {
        self.op(move |e| {
            e.on_drag(
                move |s: &mut crate::AppState, drag: &mut rune::Drag, tx: &mut Tx| {
                    let view = s.view.get_mut();
                    if drag.phase == rune::Phase::Down {
                        DRAG_ORIGIN.with(|o| o.set(view.mouse_position));
                    }
                    let origin = DRAG_ORIGIN.with(|o| o.get());
                    let event = DragEvent {
                        phase: match drag.phase {
                            rune::Phase::Down => DragPhase::Down,
                            rune::Phase::Move => DragPhase::Move,
                            rune::Phase::Up => DragPhase::Up,
                        },
                        position: point(origin.x + px(drag.dx), origin.y + px(drag.dy)),
                        cancelled: drag.cancelled(),
                    };
                    view.mouse_position = event.position;
                    let f = listener.0.clone();
                    dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
                },
            )
        })
    }
}

/// Element ids as GPUI took them: strings, or a name and an index.
pub trait IntoKey {
    fn into_key(self) -> String;
}
impl IntoKey for String {
    fn into_key(self) -> String {
        self
    }
}
impl IntoKey for &String {
    fn into_key(self) -> String {
        self.clone()
    }
}
impl IntoKey for &str {
    fn into_key(self) -> String {
        self.to_owned()
    }
}
impl<T: std::fmt::Display> IntoKey for (&str, T) {
    fn into_key(self) -> String {
        format!("{}-{}", self.0, self.1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}
impl Div {
    pub fn text_align(self, align: TextAlign) -> Self {
        match align {
            TextAlign::Left => self.text_left(),
            TextAlign::Center => self.text_center(),
            TextAlign::Right => self.text_right(),
        }
    }
    pub fn on_mouse_move(self, listener: Listener<MouseMoveEvent>) -> Self {
        self.op(move |e| {
            e.on_pointer_move_local(
                move |s: &mut crate::AppState, p: rune::PointerEvent, tx: &mut Tx| {
                    let event = MouseMoveEvent {
                        position: point(px(p.window_x), px(p.window_y)),
                        pressed_button: None,
                        modifiers: Modifiers::from_mods(tx.mods()),
                    };
                    s.view.get_mut().mouse_position = event.position;
                    let f = listener.0.clone();
                    dispatch(s, tx, |view, window, cx| f(view, &event, window, cx));
                },
            )
        })
    }
}

#[cfg(test)]
mod virtual_list_tests {
    use super::virtual_range;

    #[test]
    fn virtual_lists_build_the_view_and_a_viewport_either_side() {
        // 100 rows of 50 points in a 200-point viewport scrolled to row 40.
        let (start, end) = virtual_range(100, 50., 2000., 200.);
        assert!(start <= 40 && end >= 44, "the visible rows are built");
        assert!(start >= 34 && end <= 54, "rows far off screen are not");
        assert_eq!(virtual_range(100, 50., 0., 200.).0, 0);
        assert_eq!(virtual_range(100, 50., 99_999., 200.), (100, 100));
        assert_eq!(virtual_range(0, 50., 0., 200.), (0, 0));
        // Before the first layout, a generous window stands in for the viewport.
        assert!(virtual_range(100, 50., 0., 0.).1 >= 20);
    }
}
