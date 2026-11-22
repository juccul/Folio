//! Embedded Tabler Outline icons with custom Folio and window-control glyphs.
//! Attribution and upstream mappings: third_party/licenses/tabler-icons/NOTICE.md.
use super::compat::*;
include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// An icon element: GPUI's `svg()` with a path and a text colour.
#[allow(dead_code)]
pub struct Svg {
    kind: Icon,
    color: rune::Color,
    size: f32,
}
#[allow(dead_code)]
impl Svg {
    pub fn size(mut self, s: impl Into<Length>) -> Self {
        self.size = match s.into() {
            Length::Px(v) => v,
            Length::Rems(v) => v * REM,
            _ => self.size,
        };
        self
    }
    pub fn text_color(mut self, c: impl IntoColor) -> Self {
        self.color = c.into_color();
        self
    }
    pub fn flex_shrink_0(self) -> Self {
        self
    }
    pub fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if condition { f(self) } else { self }
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.color.a *= o;
        self
    }
    pub fn into_el(self) -> rune::El {
        rune::icon(self.kind.glyph(), self.size).color(self.color)
    }
}
impl IntoElement for Svg {
    fn into_any_element(self) -> AnyElement {
        AnyElement::El(self.into_el())
    }
}

/// An icon at Folio's standard size (1.375 rem) in `color`.
pub fn icon(kind: Icon, color: u32) -> Svg {
    Svg {
        kind,
        color: rgb(color).color(),
        size: 1.375 * REM,
    }
}
