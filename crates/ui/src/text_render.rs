//! Canvas text: a text block laid out and drawn through usvg, as SVG, PNG
//! and PDF export draw it, so the canvas matches exported pages.
use super::*;
use folio_document::{Alignment, ListStyle, Object, Rect, TextBlock, Transform};
use resvg::{tiny_skia as sk, usvg};
use std::sync::Arc;

/// A laid-out text block, drawn from its origin (the frame's top-left).
#[derive(Clone)]
pub struct Layout {
    tree: Option<Arc<usvg::Tree>>,
}
/// Only properties that affect glyph shaping, wrapping, or line positions.
#[derive(Clone)]
pub(super) struct LayoutKey {
    content: String,
    family: String,
    size: f32,
    width: f32,
    bold: bool,
    italic: bool,
    alignment: Alignment,
    list: ListStyle,
    color: u32,
    underline: bool,
}
impl LayoutKey {
    pub fn new(text: &TextBlock) -> Self {
        Self {
            content: text.text.clone(),
            family: text.font_family.clone(),
            size: text.font_size,
            width: text.rect.width(),
            bold: text.bold,
            italic: text.italic,
            alignment: text.alignment,
            list: text.list,
            color: text.color.rgb(),
            underline: text.underline,
        }
    }
    pub fn matches(&self, text: &TextBlock) -> bool {
        self.size == text.font_size
            && self.width == text.rect.width()
            && self.bold == text.bold
            && self.italic == text.italic
            && self.alignment == text.alignment
            && self.list == text.list
            && self.family == text.font_family
            && self.content == text.text
            && self.color == text.color.rgb()
            && self.underline == text.underline
    }
}
/// Lays out `text` in `color`, positioned at its frame's top-left.
pub fn layout(text: &TextBlock, color: u32, _: &mut Window) -> Layout {
    let mut local = text.clone();
    local.rect = Rect::new(0., 0., text.rect.width(), text.rect.height());
    local.transform = Transform::default();
    local.color = folio_document::Color::from_rgb(color);
    let object = Object::Text(local);
    let tree = folio_export::object_svg(&object, std::path::Path::new(""))
        .ok()
        .and_then(|body| {
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" overflow=\"visible\">{body}</svg>"
            );
            usvg::Tree::from_str(&svg, &folio_export::object_svg_options(&object)).ok()
        })
        .map(Arc::new);
    Layout { tree }
}
impl Layout {
    pub fn paint(&self, transform: Transform, _color: u32, _underline: bool, window: &mut Window) {
        let (Some(tree), Some(raster)) = (&self.tree, window.raster.as_mut()) else {
            return;
        };
        let t = raster.transform.pre_concat(sk::Transform::from_row(
            transform.a,
            transform.b,
            transform.c,
            transform.d,
            transform.tx,
            transform.ty,
        ));
        resvg::render(tree, t, &mut raster.pixmap.as_mut());
    }
}

/// A single line of interface text (a label on the canvas) at `origin`, its
/// baseline `size` below, in window points.
pub fn paint_label(label: &str, origin: Point<Pixels>, size: f32, color: u32, window: &mut Window) {
    let Some(raster) = window.raster.as_mut() else {
        return;
    };
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\" overflow=\"visible\"><text x=\"0\" y=\"{size}\" font-size=\"{size}\" fill=\"#{color:06x}\">{}</text></svg>",
        folio_export::escape(label)
    );
    if let Ok(tree) = usvg::Tree::from_str(&svg, &folio_export::system_svg_options()) {
        let t = raster.transform.pre_translate(origin.x.0, origin.y.0);
        resvg::render(&tree, t, &mut raster.pixmap.as_mut());
    }
}
