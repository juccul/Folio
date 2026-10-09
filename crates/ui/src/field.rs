//! Reusable GPUI text input with UTF-16 IME ranges and grapheme-safe editing.
use gpui::{prelude::*, *};
use std::{
    ops::Range,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;
actions!(
    field,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Home,
        End,
        SelectLeft,
        SelectRight,
        Up,
        Down,
        SelectUp,
        SelectDown,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Enter,
        Submit,
        FieldUndo,
        FieldRedo,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight
    ]
);
pub struct Submitted;
impl EventEmitter<Submitted> for Field {}
pub struct FieldBoundsChanged;
impl EventEmitter<FieldBoundsChanged> for Field {}
#[derive(Clone, Debug, PartialEq)]
struct Draft {
    content: String,
    selection: Range<usize>,
    anchor: usize,
}
#[derive(Default)]
struct EditHistory {
    undo: Vec<Draft>,
    redo: Vec<Draft>,
    group: Option<(u8, usize, Instant)>,
}
impl EditHistory {
    fn record(&mut self, before: Draft, kind: u8, head: usize, now: Instant) {
        let grouped = kind != 0
            && before.selection.is_empty()
            && self.group.is_some_and(|(previous, cursor, time)| {
                previous == kind
                    && cursor == before.selection.end
                    && now.duration_since(time) < Duration::from_millis(750)
            });
        if !grouped {
            self.undo.push(before);
            if self.undo.len() > 64 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.group = Some((kind, head, now));
    }
    fn travel(&mut self, current: Draft, redo: bool) -> Option<Draft> {
        self.group = None;
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let draft = from.pop()?;
        to.push(current);
        Some(draft)
    }
}
fn word_boundary(text: &str, head: usize, forward: bool) -> usize {
    if forward {
        text.unicode_word_indices()
            .find(|(start, word)| start + word.len() > head)
            .map_or(text.len(), |(start, word)| start + word.len())
    } else {
        text.unicode_word_indices()
            .take_while(|(start, _)| *start < head)
            .last()
            .map_or(0, |(start, _)| start)
    }
}
#[derive(Clone)]
pub struct InlineStyle {
    pub text: std::sync::Arc<folio_document::TextBlock>,
    pub source: std::sync::Arc<folio_document::Object>,
    pub transform: folio_document::Transform,
    pub mask: Bounds<Pixels>,
    pub color: u32,
    pub handles: Vec<folio_document::Point>,
}
#[derive(Clone)]
struct FieldLayoutKey {
    content: String,
    font: Font,
    font_size: f32,
    width: f32,
    color: Hsla,
    multiline: bool,
    secret: bool,
}
impl FieldLayoutKey {
    fn matches(
        &self,
        content: &str,
        font: &Font,
        size_width: (f32, f32),
        color: Hsla,
        modes: (bool, bool),
    ) -> bool {
        (self.font_size, self.width) == size_width
            && (self.multiline, self.secret) == modes
            && self.color == color
            && self.font == *font
            && self.content == content
    }
}
pub struct Field {
    pub content: String,
    pub inline: Option<InlineStyle>,
    inline_layout: Option<super::text_render::Layout>,
    inline_layout_key: Option<super::text_render::LayoutKey>,
    pub focus: FocusHandle,
    pub multiline: bool,
    pub height: Option<f32>,
    line_height: f32,
    pub secret: bool,
    pub theme: super::Theme,
    selection: Range<usize>,
    selection_anchor: usize,
    marked: Option<Range<usize>>,
    layouts: std::sync::Arc<Vec<(usize, ShapedLine)>>,
    layout_key: Option<FieldLayoutKey>,
    pub(super) bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    history: EditHistory,
    scroll: Point<Pixels>,
    follow_caret: bool,
    scroll_max: Point<Pixels>,
}
impl Field {
    pub fn new(content: String, multiline: bool, cx: &mut Context<Self>) -> Self {
        let end = content.len();
        Self {
            content,
            inline: None,
            inline_layout: None,
            inline_layout_key: None,
            focus: cx.focus_handle(),
            multiline,
            height: None,
            line_height: 26.,
            secret: false,
            theme: super::Theme::new(&folio_app::Settings::default()),
            selection: end..end,
            selection_anchor: end,
            marked: None,
            layouts: std::sync::Arc::new(vec![]),
            layout_key: None,
            bounds: None,
            selecting: false,
            history: EditHistory::default(),
            scroll: point(px(0.), px(0.)),
            scroll_max: point(px(0.), px(0.)),
            follow_caret: true,
        }
    }
    pub fn set_content(&mut self, content: String, cx: &mut Context<Self>) {
        self.content = content;
        let end = self.content.len();
        self.select(end, end);
        self.marked = None;
        self.history = EditHistory::default();
        cx.notify();
    }
    pub fn set_accessible_content(&mut self, content: String, cx: &mut Context<Self>) {
        self.replace(0..self.content.len(), &content, cx);
    }
    pub fn bindings(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("ctrl-z", FieldUndo, Some("FolioField")),
            KeyBinding::new("ctrl-shift-z", FieldRedo, Some("FolioField")),
            KeyBinding::new("ctrl-y", FieldRedo, Some("FolioField")),
            KeyBinding::new("ctrl-left", WordLeft, Some("FolioField")),
            KeyBinding::new("ctrl-right", WordRight, Some("FolioField")),
            KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("FolioField")),
            KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("FolioField")),
            KeyBinding::new("backspace", Backspace, Some("FolioField")),
            KeyBinding::new("delete", Delete, Some("FolioField")),
            KeyBinding::new("left", Left, Some("FolioField")),
            KeyBinding::new("right", Right, Some("FolioField")),
            KeyBinding::new("home", Home, Some("FolioField")),
            KeyBinding::new("end", End, Some("FolioField")),
            KeyBinding::new("shift-left", SelectLeft, Some("FolioField")),
            KeyBinding::new("shift-right", SelectRight, Some("FolioField")),
            KeyBinding::new("up", Up, Some("FolioField")),
            KeyBinding::new("down", Down, Some("FolioField")),
            KeyBinding::new("shift-up", SelectUp, Some("FolioField")),
            KeyBinding::new("shift-down", SelectDown, Some("FolioField")),
            KeyBinding::new("ctrl-a", SelectAll, Some("FolioField")),
            KeyBinding::new("ctrl-c", Copy, Some("FolioField")),
            KeyBinding::new("ctrl-x", Cut, Some("FolioField")),
            KeyBinding::new("ctrl-v", Paste, Some("FolioField")),
            KeyBinding::new("enter", Enter, Some("FolioField")),
            KeyBinding::new("ctrl-enter", Submit, Some("FolioField")),
        ]);
    }
    fn select(&mut self, anchor: usize, head: usize) {
        self.history.group = None;
        self.follow_caret = true;
        select_range(
            &mut self.selection,
            &mut self.selection_anchor,
            anchor,
            head,
        );
    }
    fn move_cursor(&mut self, forward: bool, extend: bool) {
        self.history.group = None;
        self.follow_caret = true;
        move_selection(
            &self.content,
            &mut self.selection,
            &mut self.selection_anchor,
            forward,
            extend,
        );
    }
    fn move_vertical(&mut self, down: bool, extend: bool) {
        let head = if self.selection_anchor == self.selection.start {
            self.selection.end
        } else {
            self.selection.start
        };
        let next = if let Some(layout) = &self.inline_layout {
            let caret = layout.caret(head);
            layout.index_at(folio_document::Point::new(
                caret.min.x,
                (caret.min.y
                    + if down {
                        layout.line_height
                    } else {
                        -layout.line_height
                    })
                .max(0.),
            ))
        } else {
            let row = self
                .layouts
                .iter()
                .rposition(|(offset, _)| *offset <= head)
                .unwrap_or(0);
            let target = if down {
                (row + 1).min(self.layouts.len().saturating_sub(1))
            } else {
                row.saturating_sub(1)
            };
            match (self.layouts.get(row), self.layouts.get(target)) {
                (Some((offset, line)), Some((next, target))) => {
                    next + target.closest_index_for_x(
                        line.x_for_index(head.saturating_sub(*offset).min(line.len())),
                    )
                }
                _ => head,
            }
        };
        self.select(if extend { self.selection_anchor } else { next }, next);
    }
    fn previous(&self) -> usize {
        self.content[..self.selection.start]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
    fn next(&self) -> usize {
        self.content[self.selection.end..]
            .graphemes(true)
            .next()
            .map(|s| self.selection.end + s.len())
            .unwrap_or(self.content.len())
    }
    fn utf8(&self, u16_offset: usize) -> usize {
        let mut count = 0;
        for (i, c) in self.content.char_indices() {
            if count >= u16_offset {
                return i;
            }
            count += c.len_utf16();
        }
        self.content.len()
    }
    fn utf16(&self, byte: usize) -> usize {
        self.content[..byte.min(self.content.len())]
            .encode_utf16()
            .count()
    }
    fn range(&self, r: Range<usize>) -> Range<usize> {
        let start = self.utf8(r.start);
        let end = self.utf8(r.end);
        start.min(end)..start.max(end)
    }
    fn index_at(&self, p: Point<Pixels>) -> usize {
        if let Some(style) = &self.inline
            && let Some(layout) = &self.inline_layout
        {
            let p = style
                .transform
                .inverse()
                .unwrap_or_default()
                .apply(folio_document::Point::new(f32::from(p.x), f32::from(p.y)));
            return layout.index_at(p).min(self.content.len());
        }
        let Some(bounds) = self.bounds else {
            return self.content.len();
        };
        let row = (f32::from(p.y - bounds.top() + self.scroll.y) / self.line_height)
            .floor()
            .max(0.) as usize;
        let Some((offset, line)) = self
            .layouts
            .get(row.min(self.layouts.len().saturating_sub(1)))
        else {
            return 0;
        };
        let index = line.closest_index_for_x(p.x - bounds.left() + self.scroll.x);
        if self.secret {
            self.content[*offset..]
                .char_indices()
                .nth(index)
                .map(|(i, _)| offset + i)
                .unwrap_or(self.content.len())
        } else {
            offset + index
        }
    }
    fn replace(&mut self, r: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let text = if self.multiline {
            text.to_string()
        } else {
            text.replace(['\n', '\r'], " ")
        };
        let before = self.draft();
        let kind = if self.marked.is_some() {
            3
        } else if r.is_empty() && !text.contains(char::is_whitespace) && text.chars().count() == 1 {
            1
        } else if text.is_empty() && !r.is_empty() && self.selection.is_empty() {
            2
        } else {
            0
        };
        self.content.replace_range(r.clone(), &text);
        let end = r.start + text.len();
        select_range(&mut self.selection, &mut self.selection_anchor, end, end);
        self.history.record(before, kind, end, Instant::now());
        self.marked = None;
        self.follow_caret = true;
        cx.notify();
    }
    fn draft(&self) -> Draft {
        Draft {
            content: self.content.clone(),
            selection: self.selection.clone(),
            anchor: self.selection_anchor,
        }
    }
    fn travel(&mut self, redo: bool, cx: &mut Context<Self>) {
        if let Some(draft) = self.history.travel(self.draft(), redo) {
            self.content = draft.content;
            self.selection = draft.selection;
            self.selection_anchor = draft.anchor;
            self.marked = None;
            self.follow_caret = true;
            cx.notify();
        }
    }
    fn move_word(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) {
        let head = if self.selection_anchor == self.selection.start {
            self.selection.end
        } else {
            self.selection.start
        };
        let next = word_boundary(&self.content, head, forward);
        self.select(if extend { self.selection_anchor } else { next }, next);
        cx.notify();
    }
}
impl Focusable for Field {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl EntityInputHandler for Field {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.range(r);
        *actual = Some(self.utf16(r.start)..self.utf16(r.end));
        Some(self.content[r].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16(self.selection.start)..self.utf16(self.selection.end),
            reversed: self.selection_anchor != self.selection.start,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .clone()
            .map(|r| self.utf16(r.start)..self.utf16(r.end))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = r
            .map(|r| self.range(r))
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        self.replace(r, text, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let r = r
            .map(|r| self.range(r))
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        let start = r.start;
        self.replace(r, text, cx);
        self.marked = Some(start..start + text.len());
        if let Some(sel) = selection {
            let byte = |n| {
                text.char_indices()
                    .scan(0, |count, (i, c)| {
                        let before = *count;
                        *count += c.len_utf16();
                        Some((i, before))
                    })
                    .find(|(_, count)| *count >= n)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())
            };
            self.select(start + byte(sel.start), start + byte(sel.end));
        }
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let r = self.range(r);
        if let Some(style) = &self.inline
            && let Some(layout) = &self.inline_layout
        {
            let rect = layout.caret(r.start);
            let rect = folio_document::Rect::from_points(
                [
                    rect.min,
                    folio_document::Point::new(rect.max.x, rect.min.y),
                    rect.max,
                    folio_document::Point::new(rect.min.x, rect.max.y),
                ]
                .map(|p| style.transform.apply(p)),
            );
            return Some(Bounds::new(
                point(px(rect.min.x), px(rect.min.y)),
                size(px(rect.width()), px(rect.height())),
            ));
        }
        let (row, (offset, line)) = self
            .layouts
            .iter()
            .enumerate()
            .rev()
            .find(|(_, (offset, _))| *offset <= r.start)?;
        Some(Bounds::new(
            point(
                bounds.left() - self.scroll.x
                    + line.x_for_index(if self.secret {
                        self.content[*offset..r.start].chars().count()
                    } else {
                        r.start - offset
                    }),
                bounds.top() + px(row as f32 * self.line_height) - self.scroll.y,
            ),
            size(px(2.), px(self.line_height)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.utf16(self.index_at(p)))
    }
}
struct FieldElement {
    field: Entity<Field>,
    multiline: bool,
    height: Option<f32>,
}
impl IntoElement for FieldElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = std::sync::Arc<Vec<(usize, ShapedLine)>>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(self
            .height
            .unwrap_or(if self.multiline { 208. } else { 28. })
            * f32::from(window.rem_size())
            / 16.)
        .into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let field = self.field.read(cx);
        if let Some(style) = &field.inline {
            if field
                .inline_layout_key
                .as_ref()
                .is_none_or(|key| !key.matches_content(&style.text, &field.content))
            {
                let mut text = style.text.as_ref().clone();
                text.text.clone_from(&field.content);
                let layout = super::text_render::layout(&text, style.color, window);
                let key = super::text_render::LayoutKey::new(&text);
                self.field.update(cx, |field, _| {
                    field.inline_layout = Some(layout);
                    field.inline_layout_key = Some(key);
                });
            }
            return std::sync::Arc::new(vec![]);
        }
        let font_size = f32::from(window.rem_size());
        let line_height = font_size * 26. / 16.;
        self.field
            .update(cx, |field, _| field.line_height = line_height);
        let field = self.field.read(cx);
        let style = window.text_style();
        let font = style.font();
        let width = f32::from(bounds.size.width);
        let cached = field.layout_key.as_ref().is_some_and(|key| {
            key.matches(
                &field.content,
                &font,
                (font_size, width),
                style.color,
                (field.multiline, field.secret),
            )
        });
        let shape = |text: &str| {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color: style.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(text.to_owned().into(), px(font_size), &[run], None)
        };
        let lines = if cached {
            field.layouts.clone()
        } else {
            let ranges = if field.multiline && !field.secret {
                folio_document::text_wrap_ranges(
                    &field.content,
                    f32::from(bounds.size.width),
                    |text| f32::from(shape(text).width),
                )
            } else {
                std::iter::once(0..field.content.len()).collect()
            };
            let lines: Vec<_> = ranges
                .into_iter()
                .map(|range| {
                    let text = &field.content[range.clone()];
                    let displayed = if field.secret {
                        "*".repeat(text.chars().count())
                    } else {
                        text.to_owned()
                    };
                    (range.start, shape(&displayed))
                })
                .collect();
            std::sync::Arc::new(lines)
        };
        let key = (!cached).then(|| FieldLayoutKey {
            content: field.content.clone(),
            font,
            font_size,
            width,
            color: style.color,
            multiline: field.multiline,
            secret: field.secret,
        });
        self.field.update(cx, |field, _| {
            if let Some(key) = key {
                field.layout_key = Some(key);
                field.layouts = lines.clone();
            }
            let head = if field.selection_anchor == field.selection.start {
                field.selection.end
            } else {
                field.selection.start
            };
            let row = lines
                .iter()
                .rposition(|(offset, _)| *offset <= head)
                .unwrap_or(0);
            let x = lines.get(row).map_or(0., |(offset, line)| {
                f32::from(line.x_for_index(if field.secret {
                    field.content[*offset..head].chars().count()
                } else {
                    head.saturating_sub(*offset).min(line.len())
                }))
            });
            let max_width = lines
                .iter()
                .map(|(_, line)| f32::from(line.width))
                .fold(0., f32::max)
                + 4.;
            field.scroll_max = point(
                px((max_width - f32::from(bounds.size.width)).max(0.)),
                px((lines.len() as f32 * line_height - f32::from(bounds.size.height)).max(0.)),
            );
            if field.follow_caret {
                field.scroll.x = px(caret_scroll(
                    f32::from(field.scroll.x),
                    x,
                    4.,
                    f32::from(bounds.size.width),
                    f32::from(field.scroll_max.x),
                ));
                field.scroll.y = px(caret_scroll(
                    f32::from(field.scroll.y),
                    row as f32 * line_height,
                    line_height,
                    f32::from(bounds.size.height),
                    f32::from(field.scroll_max.y),
                ));
                field.follow_caret = false;
            } else {
                field.scroll.x = field.scroll.x.clamp(px(0.), field.scroll_max.x);
                field.scroll.y = field.scroll.y.clamp(px(0.), field.scroll_max.y);
            }
        });
        lines
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        lines: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let field = self.field.read(cx);
        if let Some(style) = &field.inline
            && let Some(layout) = &field.inline_layout
        {
            let focus = field.focus.clone();
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.field.clone()),
                cx,
            );
            window.with_content_mask(Some(ContentMask { bounds: style.mask }), |window| {
                layout.paint_selection(
                    field.selection.clone(),
                    style.transform,
                    rgba((field.theme.accent << 8) | 0x35),
                    window,
                );
                layout.paint(style.transform, style.color, style.text.underline, window);
                if focus.is_focused(window) && field.selection.is_empty() {
                    super::text_render::fill_rect(
                        layout.caret(field.selection.start),
                        style.transform,
                        rgb(field.theme.accent),
                        window,
                    );
                }
            });
            self.field.update(cx, |field, cx| {
                if field.bounds != Some(bounds) {
                    field.bounds = Some(bounds);
                    cx.emit(FieldBoundsChanged);
                }
            });
            return;
        }
        let selection = if field.secret {
            field.content[..field.selection.start].chars().count()
                ..field.content[..field.selection.end].chars().count()
        } else {
            field.selection.clone()
        };
        let theme = field.theme;
        let line_height = field.line_height;
        let scroll = field.scroll;
        let caret_row = lines
            .iter()
            .rposition(|(offset, _)| *offset <= selection.start)
            .unwrap_or(0);
        let focused = field.focus.is_focused(window);
        let focus = field.focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.field.clone()),
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (row, (offset, line)) in lines.iter().enumerate() {
                let y = bounds.top() + px(row as f32 * line_height) - scroll.y;
                let end = offset + line.len();
                if selection.start <= end && selection.end >= *offset && !selection.is_empty() {
                    let start = selection.start.saturating_sub(*offset).min(line.len());
                    let end = selection.end.saturating_sub(*offset).min(line.len());
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.left() - scroll.x + line.x_for_index(start), y),
                            size(
                                (line.x_for_index(end) - line.x_for_index(start)).max(px(2.)),
                                px(line_height),
                            ),
                        ),
                        rgba((theme.accent << 8) | 0x30),
                    ));
                }
                let _ = line.paint(
                    point(bounds.left() - scroll.x, y),
                    px(line_height),
                    window,
                    cx,
                );
                if focused
                    && selection.is_empty()
                    && row == caret_row
                    && selection.start >= *offset
                    && selection.start <= end
                {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                bounds.left() - scroll.x
                                    + line.x_for_index(selection.start - offset),
                                y,
                            ),
                            size(px(1.5), px(22.)),
                        ),
                        rgb(theme.ink),
                    ));
                }
            }
        });
        self.field.update(cx, |field, cx| {
            if field.bounds != Some(bounds) {
                field.bounds = Some(bounds);
                // Publish the bounds after a newly revealed field is painted so
                // its accessibility node can be focused on the next frame.
                cx.emit(FieldBoundsChanged);
            }
        });
    }
}
impl Render for Field {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id("text-field")
            .key_context("FolioField")
            .track_focus(&self.focus)
            .w_full()
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.ink))
            .border_1()
            .border_color(theme.input)
            .rounded(px(theme.radius))
            .focus(move |s| s.border_color(rgb(theme.ring)))
            .p_3()
            .when(self.inline.is_none(), |s| s.overflow_hidden())
            .when(self.inline.is_some(), |s| {
                s.bg(transparent_black())
                    .border_0()
                    .rounded(px(0.))
                    .p_0()
                    .occlude()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if this.inline.as_ref().is_some_and(|style| {
                        style.handles.iter().any(|p| {
                            p.distance(folio_document::Point::new(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                            )) < 10.
                        })
                    }) {
                        return;
                    }
                    this.focus.focus(window);
                    let i = this.index_at(event.position);
                    this.select(i, i);
                    this.selecting = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                if this.inline.is_some() {
                    return;
                }
                let delta = event.delta.pixel_delta(px(this.line_height));
                this.scroll.x = (this.scroll.x - delta.x).clamp(px(0.), this.scroll_max.x);
                this.scroll.y = (this.scroll.y - delta.y).clamp(px(0.), this.scroll_max.y);
                this.follow_caret = false;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.selecting {
                    let i = this.index_at(event.position);
                    this.select(this.selection_anchor, i);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &Backspace, _, cx| {
                let r = if this.selection.is_empty() {
                    this.previous()..this.selection.end
                } else {
                    this.selection.clone()
                };
                this.replace(r, "", cx)
            }))
            .on_action(cx.listener(|this, _: &FieldUndo, _, cx| this.travel(false, cx)))
            .on_action(cx.listener(|this, _: &FieldRedo, _, cx| this.travel(true, cx)))
            .on_action(cx.listener(|this, _: &WordLeft, _, cx| this.move_word(false, false, cx)))
            .on_action(cx.listener(|this, _: &WordRight, _, cx| this.move_word(true, false, cx)))
            .on_action(
                cx.listener(|this, _: &SelectWordLeft, _, cx| this.move_word(false, true, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectWordRight, _, cx| this.move_word(true, true, cx)),
            )
            .on_action(cx.listener(|this, _: &Delete, _, cx| {
                let r = if this.selection.is_empty() {
                    this.selection.start..this.next()
                } else {
                    this.selection.clone()
                };
                this.replace(r, "", cx)
            }))
            .on_action(cx.listener(|this, _: &Left, _, cx| {
                this.move_cursor(false, false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Right, _, cx| {
                this.move_cursor(true, false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Up, _, cx| {
                this.move_vertical(false, false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Down, _, cx| {
                this.move_vertical(true, false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| {
                this.move_vertical(false, true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| {
                this.move_vertical(true, true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Home, _, cx| {
                let start = this
                    .inline_layout
                    .as_ref()
                    .and_then(|layout| {
                        layout
                            .lines
                            .iter()
                            .rev()
                            .find(|line| line.offset <= this.selection.end)
                    })
                    .map_or(0, |line| line.offset);
                this.select(start, start);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &End, _, cx| {
                let end = this
                    .inline_layout
                    .as_ref()
                    .and_then(|layout| {
                        layout
                            .lines
                            .iter()
                            .rev()
                            .find(|line| line.offset <= this.selection.end)
                    })
                    .map_or(this.content.len(), |line| line.offset + line.shaped.len());
                this.select(end, end);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| {
                this.move_cursor(false, true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.move_cursor(true, true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.select(0, this.content.len());
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Copy, _, cx| {
                if this.secret {
                    return;
                }
                if !this.selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        this.content[this.selection.clone()].into(),
                    ))
                }
            }))
            .on_action(cx.listener(|this, _: &Cut, _, cx| {
                if this.secret {
                    let r = this.selection.clone();
                    this.replace(r, "", cx);
                    return;
                }
                if !this.selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        this.content[this.selection.clone()].into(),
                    ));
                    this.replace(this.selection.clone(), "", cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    this.replace(this.selection.clone(), &text, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Enter, _, cx| {
                if this.multiline {
                    this.replace(this.selection.clone(), "\n", cx)
                } else {
                    cx.emit(Submitted)
                }
            }))
            .on_action(cx.listener(|_, _: &Submit, _, cx| cx.emit(Submitted)))
            .child(FieldElement {
                field: cx.entity(),
                multiline: self.multiline,
                height: self.height,
            })
    }
}
fn caret_scroll(current: f32, caret: f32, extent: f32, viewport: f32, maximum: f32) -> f32 {
    let next = if caret < current {
        caret
    } else if caret + extent > current + viewport {
        caret + extent - viewport
    } else {
        current
    };
    next.clamp(0., maximum)
}
fn select_range(range: &mut Range<usize>, anchor: &mut usize, start: usize, head: usize) {
    *anchor = start;
    *range = start.min(head)..start.max(head);
}
fn move_selection(
    content: &str,
    range: &mut Range<usize>,
    anchor: &mut usize,
    forward: bool,
    extend: bool,
) {
    let head = if range.start == *anchor {
        range.end
    } else {
        range.start
    };
    let next = if !extend && range.start != range.end {
        if forward { range.end } else { range.start }
    } else if forward {
        content[head..]
            .graphemes(true)
            .next()
            .map_or(content.len(), |grapheme| head + grapheme.len())
    } else {
        content[..head]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    };
    select_range(range, anchor, if extend { *anchor } else { next }, next);
}

#[cfg(test)]
mod tests {
    use super::{Draft, EditHistory, caret_scroll, word_boundary};
    #[test]
    fn field_layout_cache_invalidates_wrapping_font_color_and_display_modes() {
        let font = gpui::font("Sans");
        let color: gpui::Hsla = gpui::rgb(0x112233).into();
        let key = super::FieldLayoutKey {
            content: "café\ntext".into(),
            font: font.clone(),
            font_size: 16.,
            width: 200.,
            color,
            multiline: true,
            secret: false,
        };
        assert!(key.matches("café\ntext", &font, (16., 200.), color, (true, false)));
        assert!(!key.matches("edited", &font, (16., 200.), color, (true, false)));
        assert!(!key.matches(
            &key.content,
            &gpui::font("Serif"),
            (16., 200.),
            color,
            (true, false)
        ));
        assert!(!key.matches(&key.content, &font, (17., 200.), color, (true, false)));
        assert!(!key.matches(&key.content, &font, (16., 201.), color, (true, false)));
        assert!(!key.matches(
            &key.content,
            &font,
            (16., 200.),
            gpui::rgb(0xffffff).into(),
            (true, false)
        ));
        assert!(!key.matches(&key.content, &font, (16., 200.), color, (false, false)));
        assert!(!key.matches(&key.content, &font, (16., 200.), color, (true, true)));
    }
    #[test]
    fn scrolling_keeps_caret_visible_and_clamps_after_deletion() {
        assert_eq!(caret_scroll(0., 260., 26., 208., 78.), 78.);
        assert_eq!(caret_scroll(78., 0., 26., 208., 78.), 0.);
        assert_eq!(caret_scroll(80., 60., 4., 30., 0.), 0.);
        assert_eq!(caret_scroll(40., 50., 4., 30., 100.), 40.);
    }
    use std::time::{Duration, Instant};
    fn draft(text: &str) -> Draft {
        Draft {
            content: text.into(),
            selection: text.len()..text.len(),
            anchor: text.len(),
        }
    }
    #[test]
    fn draft_history_groups_typing_and_preserves_replacements_and_redo() {
        let mut history = EditHistory::default();
        let now = Instant::now();
        history.record(draft(""), 1, 1, now);
        history.record(draft("a"), 1, 2, now + Duration::from_millis(50));
        assert_eq!(history.travel(draft("ab"), false), Some(draft("")));
        assert_eq!(history.travel(draft(""), true), Some(draft("ab")));
        history.record(draft("ab"), 0, 7, now + Duration::from_secs(1));
        assert_eq!(history.travel(draft("pasted!"), false), Some(draft("ab")));
        history.record(draft("ab"), 1, 3, now + Duration::from_secs(2));
        assert!(history.redo.is_empty());
    }
    #[test]
    fn word_navigation_handles_unicode_and_punctuation() {
        assert_eq!(word_boundary("one café, two", 4, true), 9);
        assert_eq!(word_boundary("one café, two", 11, false), 4);
        assert_eq!(word_boundary("one café, two", 0, false), 0);
    }
    use super::{move_selection, select_range};
    #[::core::prelude::v1::test]
    fn reversing_shift_selection_shrinks_then_crosses_its_anchor() {
        let text = "abcdef";
        let (mut range, mut anchor) = (3..3, 3);
        for _ in 0..2 {
            move_selection(text, &mut range, &mut anchor, false, true);
        }
        assert_eq!(range, 1..3);
        move_selection(text, &mut range, &mut anchor, true, true);
        assert_eq!(range, 2..3);
        for _ in 0..2 {
            move_selection(text, &mut range, &mut anchor, true, true);
        }
        assert_eq!(range, 3..4);
        assert_eq!(anchor, 3);
        move_selection(text, &mut range, &mut anchor, false, false);
        assert_eq!(range, 3..3);
        move_selection(text, &mut range, &mut anchor, true, false);
        assert_eq!(range, 4..4);
    }
    #[::core::prelude::v1::test]
    fn mouse_selection_keeps_anchor_when_dragging_backwards_and_crossing() {
        let (mut range, mut anchor) = (4..4, 4);
        for (head, expected) in [(2, 2..4), (1, 1..4), (3, 3..4), (5, 4..5)] {
            select_range(&mut range, &mut anchor, 4, head);
            assert_eq!(range, expected);
            assert_eq!(anchor, 4);
        }
    }
    #[::core::prelude::v1::test]
    fn cursor_moves_by_grapheme_without_splitting_emoji_or_combining_marks() {
        let text = "a👩‍💻e\u{301}";
        let (mut range, mut anchor) = (text.len()..text.len(), text.len());
        move_selection(text, &mut range, &mut anchor, false, true);
        assert_eq!(&text[range.clone()], "e\u{301}");
        move_selection(text, &mut range, &mut anchor, false, true);
        assert_eq!(&text[range.clone()], "👩‍💻e\u{301}");
        move_selection(text, &mut range, &mut anchor, true, true);
        assert_eq!(&text[range.clone()], "e\u{301}");
        move_selection(text, &mut range, &mut anchor, true, true);
        assert!(range.is_empty());
    }
}
