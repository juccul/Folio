//! Native text layout shared by canvas text and the inline editor.
use folio_document::{Alignment, ListStyle, Point as DocPoint, Rect, TextBlock, Transform};
use gpui::*;
use std::ops::Range;

#[derive(Clone)]
pub struct Line {
    pub offset: usize,
    pub shaped: ShapedLine,
    pub prefix: Option<ShapedLine>,
    pub x: f32,
    pub baseline: f32,
}
#[derive(Clone)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub font_size: f32,
    pub line_height: f32,
}
/// Only properties that affect glyph shaping, wrapping, or line positions.
/// Moving, recoloring, and underlining text reuse the same native layout.
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
        }
    }
    pub fn matches(&self, text: &TextBlock) -> bool {
        self.matches_content(text, &text.text)
    }
    pub fn matches_content(&self, text: &TextBlock, content: &str) -> bool {
        self.size == text.font_size
            && self.width == text.rect.width()
            && self.bold == text.bold
            && self.italic == text.italic
            && self.alignment == text.alignment
            && self.list == text.list
            && self.family == text.font_family
            && self.content == content
    }
}
pub fn layout(text: &TextBlock, color: u32, window: &mut Window) -> Layout {
    let mut font = font(text.font_family.clone());
    if text.bold {
        font = font.bold();
    }
    if text.italic {
        font = font.italic();
    }
    let shape = |value: &str| {
        window.text_system().shape_line(
            value.to_string().into(),
            px(text.font_size),
            &[TextRun {
                len: value.len(),
                font: font.clone(),
                color: rgb(color).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
    };
    let reserve = if text.list == ListStyle::None {
        0.
    } else {
        text.font_size * 2.
    };
    let ranges = folio_document::text_wrap_ranges(
        &text.text,
        (text.rect.width() - reserve).max(text.font_size),
        |value| f32::from(shape(value.trim_end()).width),
    );
    let line_height = text.font_size * 1.4;
    let lines = ranges
        .into_iter()
        .enumerate()
        .map(|(row, range)| {
            let shaped = shape(&text.text[range.clone()]);
            let prefix = match text.list {
                ListStyle::None => None,
                ListStyle::Bullet => Some(shape("• ")),
                ListStyle::Numbered => Some(shape(&format!("{}. ", row + 1))),
            };
            let prefix_width = prefix.as_ref().map_or(0., |p| f32::from(p.width));
            let width = f32::from(shaped.width) + prefix_width;
            let x = match text.alignment {
                Alignment::Left => 0.,
                Alignment::Center => (text.rect.width() - width) / 2.,
                Alignment::Right => text.rect.width() - width,
            };
            Line {
                offset: range.start,
                shaped,
                prefix,
                x,
                baseline: text.font_size + row as f32 * line_height,
            }
        })
        .collect::<Vec<_>>();
    Layout {
        lines,
        font_size: text.font_size,
        line_height,
    }
}
fn matrix(t: Transform) -> TransformationMatrix {
    TransformationMatrix {
        rotation_scale: [[t.a, t.c], [t.b, t.d]],
        translation: [t.tx, t.ty],
    }
}
fn paint_line(
    line: &ShapedLine,
    origin: DocPoint,
    transform: Transform,
    color: u32,
    window: &mut Window,
) {
    for run in &line.runs {
        for glyph in &run.glyphs {
            let p = point(
                px(origin.x) + glyph.position.x,
                px(origin.y) + glyph.position.y,
            );
            if glyph.is_emoji {
                let _ = window.paint_emoji_transformed(
                    p,
                    run.font_id,
                    glyph.id,
                    line.font_size,
                    matrix(transform),
                );
            } else {
                let _ = window.paint_glyph_transformed(
                    p,
                    run.font_id,
                    glyph.id,
                    line.font_size,
                    rgb(color).into(),
                    matrix(transform),
                );
            }
        }
    }
}
pub fn fill_rect(rect: Rect, transform: Transform, color: Rgba, window: &mut Window) {
    let points = [
        rect.min,
        DocPoint::new(rect.max.x, rect.min.y),
        rect.max,
        DocPoint::new(rect.min.x, rect.max.y),
    ]
    .map(|p| transform.apply(p));
    let mut path = PathBuilder::fill();
    path.move_to(point(px(points[0].x), px(points[0].y)));
    for p in &points[1..] {
        path.line_to(point(px(p.x), px(p.y)));
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
impl Layout {
    pub fn index_at(&self, p: DocPoint) -> usize {
        let row = (p.y / self.line_height).floor().max(0.) as usize;
        let line = &self.lines[row.min(self.lines.len() - 1)];
        let prefix = line.prefix.as_ref().map_or(0., |p| f32::from(p.width));
        line.offset + line.shaped.closest_index_for_x(px(p.x - line.x - prefix))
    }
    pub fn caret(&self, index: usize) -> Rect {
        let line = self
            .lines
            .iter()
            .rev()
            .find(|l| l.offset <= index)
            .unwrap_or(&self.lines[0]);
        let prefix = line.prefix.as_ref().map_or(0., |p| f32::from(p.width));
        let x = line.x
            + prefix
            + f32::from(
                line.shaped
                    .x_for_index(index.saturating_sub(line.offset).min(line.shaped.len())),
            );
        Rect::new(x, line.baseline - self.font_size, 1.3, self.line_height)
    }
    pub fn paint(&self, transform: Transform, color: u32, underline: bool, window: &mut Window) {
        for line in &self.lines {
            let mut x = line.x;
            if let Some(prefix) = &line.prefix {
                paint_line(
                    prefix,
                    DocPoint::new(x, line.baseline),
                    transform,
                    color,
                    window,
                );
                x += f32::from(prefix.width);
            }
            paint_line(
                &line.shaped,
                DocPoint::new(x, line.baseline),
                transform,
                color,
                window,
            );
            if underline {
                fill_rect(
                    Rect::new(
                        line.x,
                        line.baseline + self.font_size * 0.1,
                        x - line.x + f32::from(line.shaped.width),
                        (self.font_size * 0.06).max(1.),
                    ),
                    transform,
                    rgb(color),
                    window,
                );
            }
        }
    }
    pub fn paint_selection(
        &self,
        range: Range<usize>,
        transform: Transform,
        color: Rgba,
        window: &mut Window,
    ) {
        for line in &self.lines {
            let end = line.offset + line.shaped.len();
            if range.start <= end && range.end >= line.offset && !range.is_empty() {
                let prefix = line.prefix.as_ref().map_or(0., |p| f32::from(p.width));
                let caret = |index: usize| {
                    Rect::new(
                        line.x
                            + prefix
                            + f32::from(line.shaped.x_for_index(
                                index.saturating_sub(line.offset).min(line.shaped.len()),
                            )),
                        line.baseline - self.font_size,
                        1.3,
                        self.line_height,
                    )
                };
                let start = caret(range.start.max(line.offset).min(end));
                let end_rect = caret(range.end.min(end).max(line.offset));
                fill_rect(
                    Rect::new(
                        start.min.x,
                        line.baseline - self.font_size,
                        (end_rect.min.x - start.min.x).max(2.),
                        self.line_height,
                    ),
                    transform,
                    color,
                    window,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text() -> TextBlock {
        TextBlock {
            id: folio_document::Id::new_v4(),
            text: "café αβ\ntext".into(),
            rect: Rect::new(20., 30., 160., 80.),
            transform: Transform::default(),
            font_family: "Sans".into(),
            font_size: 16.,
            color: folio_document::Color::from_rgb(0x112233),
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        }
    }
    #[::core::prelude::v1::test]
    fn text_layout_reuses_moves_colors_and_underline_but_invalidates_shaping_inputs() {
        let text = text();
        let key = LayoutKey::new(&text);
        let mut painted = text.clone();
        painted.id = folio_document::Id::new_v4();
        painted.transform = Transform::translate(120., 60.);
        painted.rect = Rect::new(90., 50., 160., 500.);
        painted.color = folio_document::Color::from_rgb(0xaabbcc);
        painted.underline = true;
        assert!(key.matches(&painted));
        for change in [
            |t: &mut TextBlock| t.text.push('!'),
            |t: &mut TextBlock| t.font_family = "Serif".into(),
            |t: &mut TextBlock| t.font_size += 1.,
            |t: &mut TextBlock| t.rect.max.x += 1.,
            |t: &mut TextBlock| t.bold = true,
            |t: &mut TextBlock| t.italic = true,
            |t: &mut TextBlock| t.alignment = Alignment::Center,
            |t: &mut TextBlock| t.list = ListStyle::Bullet,
        ] {
            let mut changed = text.clone();
            change(&mut changed);
            assert!(!key.matches(&changed));
        }
        assert!(key.matches_content(&text, &text.text));
        assert!(!key.matches_content(&text, "unsaved inline edit"));
    }
    #[::core::prelude::v1::test]
    fn wrapping_retains_unicode_offsets_whitespace_and_explicit_empty_lines() {
        let text = "café  αβ γδ\n\nlast";
        let lines = folio_document::text_wrap_ranges(text, 7., |t| t.chars().count() as f32);
        assert_eq!(
            lines.iter().map(|r| &text[r.clone()]).collect::<Vec<_>>(),
            ["café  ", "αβ γδ", "", "last"]
        );
        assert!(
            lines
                .iter()
                .all(|r| text.is_char_boundary(r.start) && text.is_char_boundary(r.end))
        );
        let unbreakable = folio_document::text_wrap_ranges("unbreakable", 3., |t| t.len() as f32);
        assert_eq!(unbreakable.len(), 1);
        assert_eq!(unbreakable[0], 0..11);
    }
}
