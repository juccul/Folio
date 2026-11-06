use super::{
    NotesView, Theme,
    field::{Field, InlineStyle},
    icons::{Icon, icon},
};
use folio_app::Tool;
use folio_document::{Id, Object, Point as DocPoint, Rect, Transform};
use gpui::{prelude::*, *};
use std::sync::Arc;

pub struct Editor {
    pub note: Id,
    pub page: Id,
    pub id: Id,
    pub field: Entity<Field>,
    /// The box was created with the Text tool, which returns afterwards so
    /// further boxes can be placed.
    return_to_text: bool,
    _subscriptions: Vec<Subscription>,
}
/// A whole-box text style the formatting bar and shortcuts change.
#[derive(Clone, Copy)]
pub(super) enum TextFormat {
    Bold,
    Italic,
    Underline,
    Align(folio_document::Alignment),
    List(folio_document::ListStyle),
    Size(f32),
}
impl NotesView {
    pub(super) fn begin_inline_text(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.controller.read_only() {
            return;
        }
        // Keep an open editor for this box; finish any other box first so
        // its text is committed.
        if let Some(editor) = &self.inline_text {
            if editor.id == id {
                editor.field.read(cx).focus.focus(window);
                return;
            }
            self.commit_inline_text(cx);
        }
        let Some(Object::Text(text)) = self.controller.page().objects.get(&id).map(|o| o.as_ref())
        else {
            return;
        };
        let field = cx.new(|cx| {
            let mut field = Field::new(text.text.clone(), true, cx);
            // Text boxes on the canvas are not dialog fields in the Tab order.
            field.focus = field.focus.clone().tab_stop(false);
            field
        });
        let note = self.controller.active;
        let page = self.controller.page().id;
        // The editor paints its own text; the document receives it as one
        // undoable edit when editing ends.
        let change = cx.observe(&field, |_, _, cx| cx.notify());
        let submit = cx.subscribe_in(
            &field,
            window,
            |this, _, _: &super::field::Submitted, window, cx| this.finish_inline_text(window, cx),
        );
        let bounds_changed = cx
            .subscribe(&field, |_, _, _: &super::field::FieldBoundsChanged, cx| {
                cx.notify()
            });
        field.read(cx).focus.focus(window);
        let return_to_text = self.controller.tool == Tool::Text;
        // Editing selects the box; rectangular selection stays chosen.
        if self.controller.tool != Tool::Rectangle {
            self.controller.set_tool(Tool::Lasso);
        }
        self.controller.session_mut().selection = std::collections::HashSet::from([id]);
        self.inline_text = Some(Editor {
            note,
            page,
            id,
            field,
            return_to_text,
            _subscriptions: vec![change, submit, bounds_changed],
        });
        cx.notify();
    }
    pub(super) fn finish_inline_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inline_text.is_none() {
            return;
        }
        self.commit_inline_text(cx);
        self.focus.focus(window);
        cx.notify();
    }
    /// Commit and close the inline editor, wherever its box now is.
    pub(super) fn commit_inline_text(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.inline_text.take() {
            let content = editor.field.read(cx).content.clone();
            self.controller
                .finish_text_edit(editor.note, editor.page, editor.id, content);
            // Return to the Text tool unless another tool was chosen meanwhile.
            if editor.return_to_text
                && self.controller.tool == Tool::Lasso
                && !self.controller.read_only()
            {
                self.controller.session_mut().selection.clear();
                self.controller.set_tool(Tool::Text);
            }
        }
    }
    /// Change the style of the selected text boxes, keeping the caret in the
    /// box being edited.
    pub(super) fn format_text(
        &mut self,
        format: TextFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use folio_document::ListStyle;
        for id in self.controller.session().selection.clone() {
            self.controller.edit_text(id, |t| match format {
                TextFormat::Bold => t.bold = !t.bold,
                TextFormat::Italic => t.italic = !t.italic,
                TextFormat::Underline => t.underline = !t.underline,
                TextFormat::Align(alignment) => t.alignment = alignment,
                TextFormat::Size(size) => t.font_size = size,
                TextFormat::List(list) => {
                    t.list = if t.list == list {
                        ListStyle::None
                    } else {
                        list
                    }
                }
            });
        }
        if let Some(editor) = &self.inline_text {
            editor.field.read(cx).focus.focus(window);
        }
        cx.notify();
    }
    /// Start editing the only selected text box, as Enter does.
    pub(super) fn edit_selected_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = &self.controller.session().selection;
        let id = selection.iter().next().copied().filter(|id| {
            selection.len() == 1
                && matches!(
                    self.controller.page().objects.get(id).map(|o| o.as_ref()),
                    Some(Object::Text(_))
                )
        });
        if let Some(id) = id {
            self.begin_inline_text(id, window, cx);
        }
    }
    pub(super) fn inline_element(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.controller.read_only() {
            self.commit_inline_text(cx);
            return None;
        }
        let editor = self.inline_text.as_ref()?;
        let selected = self.controller.session().selection.len() == 1
            && self.controller.session().selection.contains(&editor.id);
        let valid = self.controller.active == editor.note
            && self.controller.page().id == editor.page
            && selected
            && !self.library_open
            && matches!(self.controller.tool, Tool::Lasso | Tool::Rectangle);
        let source = self.controller.page().objects.get(&editor.id).cloned();
        let text = source.as_ref().and_then(|source| {
            let Object::Text(text) = source.as_ref() else {
                return None;
            };
            let cached = editor
                .field
                .read(cx)
                .inline
                .as_ref()
                .filter(|style| Arc::ptr_eq(&style.source, source));
            Some(cached.map_or_else(|| Arc::new(text.clone()), |style| style.text.clone()))
        });
        if !valid || text.is_none() {
            self.commit_inline_text(cx);
            return None;
        }
        let text = text.unwrap();
        let source = source.unwrap();
        let bounds = self.canvas_bounds?;
        let viewport = self.controller.session().viewport;
        let local = viewport
            .transform()
            .compose(text.transform)
            .compose(Transform::translate(text.rect.min.x, text.rect.min.y));
        let rect = Rect::from_points(
            [
                DocPoint::new(0., 0.),
                DocPoint::new(text.rect.width(), 0.),
                DocPoint::new(text.rect.width(), text.rect.height()),
                DocPoint::new(0., text.rect.height()),
            ]
            .map(|p| local.apply(p)),
        );
        let theme = Theme::new(&self.controller.settings);
        let color = theme
            .canvas_for_page(&self.controller.page().properties)
            .ink(text.color.rgb());
        let transform =
            Transform::translate(f32::from(bounds.origin.x), f32::from(bounds.origin.y))
                .compose(local);
        let page_world =
            Transform::translate(f32::from(bounds.origin.x), f32::from(bounds.origin.y))
                .compose(viewport.transform());
        let handles = self
            .controller
            .selection_bounds()
            .map(|r| {
                vec![
                    r.min,
                    DocPoint::new(r.max.x, r.min.y),
                    r.max,
                    DocPoint::new(r.min.x, r.max.y),
                    DocPoint::new(r.center().x, r.min.y - 24. / viewport.zoom),
                ]
            })
            .unwrap_or_default()
            .into_iter()
            .map(|p| page_world.apply(p))
            .collect();
        let field = editor.field.clone();
        field.update(cx, |field, _| {
            field.theme = theme;
            field.inline = Some(InlineStyle {
                text,
                source,
                transform,
                mask: bounds,
                color,
                handles,
            });
            field.height = Some(rect.height().max(28.));
        });
        Some(
            div()
                .absolute()
                .left(px(rect.min.x))
                .top(px(rect.min.y))
                .w(px(rect.width().max(16.)))
                .h(px(rect.height().max(28.)))
                .child(field)
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                    this.scroll(event, cx);
                    cx.stop_propagation();
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        let hit = this.inline_text.as_ref().is_some_and(|e| {
                            e.field.read(cx).inline.as_ref().is_some_and(|s| {
                                s.handles.iter().any(|p| {
                                    p.distance(DocPoint::new(
                                        f32::from(event.position.x),
                                        f32::from(event.position.y),
                                    )) < 10.
                                })
                            })
                        });
                        if hit {
                            this.finish_inline_text(window, cx);
                            this.mouse(
                                event.position,
                                folio_input::Phase::Down,
                                MouseButton::Left,
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        }
                    }),
                )
                .into_any_element(),
        )
    }
    pub(super) fn text_formatting_bar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let text = self.controller.session().selection.iter().find_map(|id| {
            match self.controller.page().objects.get(id)?.as_ref() {
                Object::Text(t) => Some(t.clone()),
                _ => None,
            }
        });
        // The font menu belongs to the selected text box.
        let Some(text) = text else {
            self.font_menu = None;
            return None;
        };
        use folio_document::{Alignment, ListStyle};
        let theme = Theme::new(&self.controller.settings);
        // The selection also holds ink or other objects.
        let mixed = self.controller.session().selection.iter().any(|id| {
            !matches!(
                self.controller.page().objects.get(id).map(|o| o.as_ref()),
                Some(Object::Text(_))
            )
        });
        let divider = || {
            div()
                .w(px(1.))
                .h(rems(1.25))
                .mx_1p5()
                .flex_shrink_0()
                .bg(theme.border)
        };
        let group = || div().flex().items_center().gap_0p5().flex_shrink_0();

        // Font family, shown in its own face, and a size stepper.
        let font = self.font_button(&text.font_family, cx);
        let smaller = FONT_SIZES
            .iter()
            .rev()
            .copied()
            .find(|&size| size < text.font_size - 0.01);
        let larger = FONT_SIZES
            .iter()
            .copied()
            .find(|&size| size > text.font_size + 0.01);
        let step = |this: &Self,
                    id: &'static str,
                    label: &'static str,
                    kind: Icon,
                    to: Option<f32>,
                    cx: &mut Context<Self>| {
            this.format_button(
                id,
                label,
                label.into(),
                kind,
                false,
                to.is_some(),
                cx,
                move |this, w, cx| {
                    if let Some(size) = to {
                        this.format_text(TextFormat::Size(size), w, cx);
                    }
                },
            )
        };
        let size_label = format!("{}", (text.font_size * 10.).round() / 10.);
        let size = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .rounded(px(theme.radius))
            .bg(theme.raised())
            .border_1()
            .border_color(theme.border)
            .child(step(
                self,
                "font-size-down",
                "Smaller text",
                Icon::Minimize,
                smaller,
                cx,
            ))
            .child(
                self.control(
                    "font-size",
                    format!("Font size {size_label} px"),
                    div()
                        .min_w(rems(1.75))
                        .flex()
                        .justify_center()
                        .child(size_label)
                        .into_any_element(),
                    false,
                    cx,
                    |this, w, cx| this.modal(super::Modal::FontSize, w, cx),
                )
                .h(rems(1.875))
                .min_h_0()
                .px_1()
                .text_xs()
                .with_hint("Font size", theme, self.hints()),
            )
            .child(step(
                self,
                "font-size-up",
                "Larger text",
                Icon::Plus,
                larger,
                cx,
            ));

        let mut row = div()
            .flex()
            .items_center()
            .px_3()
            .py_1()
            .gap_1()
            .min_w_0()
            .overflow_hidden()
            .border_b_1()
            .border_color(theme.border)
            // Names what the row applies to: the selected text box.
            .child(
                div()
                    .flex_shrink_0()
                    .pr_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(rems(0.8125))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.secondary()))
                    .child(if mixed { "Text in selection" } else { "Text" }),
            )
            .child(font)
            .child(size)
            .child(divider());

        let toggles = |this: &Self,
                       items: &[(
            &'static str,
            &'static str,
            &'static str,
            Icon,
            bool,
            TextFormat,
        )],
                       cx: &mut Context<Self>| {
            let mut g = group();
            for &(id, label, hint, kind, active, format) in items {
                g = g.child(this.format_button(
                    id,
                    label,
                    hint.into(),
                    kind,
                    active,
                    true,
                    cx,
                    move |this, w, cx| this.format_text(format, w, cx),
                ));
            }
            g
        };
        row = row
            .child(toggles(
                self,
                &[
                    (
                        "bold",
                        "Bold",
                        "Bold · Ctrl+B",
                        Icon::Bold,
                        text.bold,
                        TextFormat::Bold,
                    ),
                    (
                        "italic",
                        "Italic",
                        "Italic · Ctrl+I",
                        Icon::Italic,
                        text.italic,
                        TextFormat::Italic,
                    ),
                    (
                        "underline",
                        "Underline",
                        "Underline · Ctrl+U",
                        Icon::Underline,
                        text.underline,
                        TextFormat::Underline,
                    ),
                ],
                cx,
            ))
            .child(divider())
            .child(toggles(
                self,
                &[
                    (
                        "align-left",
                        "Align left",
                        "Align left",
                        Icon::AlignLeft,
                        text.alignment == Alignment::Left,
                        TextFormat::Align(Alignment::Left),
                    ),
                    (
                        "align-center",
                        "Align center",
                        "Align center",
                        Icon::AlignCenter,
                        text.alignment == Alignment::Center,
                        TextFormat::Align(Alignment::Center),
                    ),
                    (
                        "align-right",
                        "Align right",
                        "Align right",
                        Icon::AlignRight,
                        text.alignment == Alignment::Right,
                        TextFormat::Align(Alignment::Right),
                    ),
                ],
                cx,
            ))
            .child(divider())
            .child(toggles(
                self,
                &[
                    (
                        "bullet",
                        "Bulleted list",
                        "Bulleted list",
                        Icon::List,
                        text.list == ListStyle::Bullet,
                        TextFormat::List(ListStyle::Bullet),
                    ),
                    (
                        "numbered",
                        "Numbered list",
                        "Numbered list",
                        Icon::ListNumbers,
                        text.list == ListStyle::Numbered,
                        TextFormat::List(ListStyle::Numbered),
                    ),
                ],
                cx,
            ))
            .child(divider());

        // Text color: the current color as a ringed swatch.
        let swatch = div()
            .size(rems(1.))
            .flex_shrink_0()
            .rounded_full()
            .border(px(1.5))
            .border_color(rgba((theme.ink << 8) | 0x80))
            .bg(rgb(theme.canvas.ink(text.color.rgb())));
        row = row.child(
            self.control(
                "text-color",
                "Text color",
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(swatch)
                    .child(icon(Icon::Down, theme.muted).size(rems(0.875)))
                    .into_any_element(),
                false,
                cx,
                |this, w, cx| this.modal(super::Modal::TextColor, w, cx),
            )
            .h(rems(1.875))
            .min_h_0()
            .px_1p5()
            .rounded(px(theme.radius))
            .with_hint("Text color", theme, self.hints()),
        );
        row = if self.inline_text.is_some() {
            row.child(
                self.control(
                    "finish-text",
                    "Done",
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(icon(Icon::Check, theme.primary_foreground).size(rems(1.)))
                        .child("Done")
                        .into_any_element(),
                    false,
                    cx,
                    |this, w, cx| this.finish_inline_text(w, cx),
                )
                .h(rems(1.875))
                .min_h_0()
                .px_3()
                .flex_shrink_0()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .rounded(px(theme.radius))
                .bg(rgb(theme.accent))
                .text_color(rgb(theme.primary_foreground))
                .with_hint("Finish editing · Esc", theme, self.hints()),
            )
        } else {
            let id = text.id;
            row.child(
                // The Enter shortcut lives in the button's hint.
                div()
                    .w(px(1.))
                    .h(rems(1.25))
                    .mx_1p5()
                    .flex_shrink_0()
                    .bg(theme.border),
            )
            .child(
                self.control(
                    "edit-text",
                    "Edit text",
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(icon(Icon::Text, theme.ink).size(rems(1.)))
                        .child("Edit text")
                        .into_any_element(),
                    false,
                    cx,
                    move |this, w, cx| this.begin_inline_text(id, w, cx),
                )
                .h(rems(1.875))
                .min_h_0()
                .px_2p5()
                .flex_shrink_0()
                .text_xs()
                .rounded(px(theme.radius))
                .border_1()
                .border_color(theme.border)
                .with_hint("Edit text · Enter", theme, self.hints()),
            )
        };
        Some(row.into_any_element())
    }
    /// An icon button of the formatting bar. `active` styles mark an
    /// option that is on.
    #[allow(clippy::too_many_arguments)]
    fn format_button(
        &self,
        id: &'static str,
        label: &'static str,
        hint: SharedString,
        kind: Icon,
        active: bool,
        enabled: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let hover = self.motion.borrow_mut().hover_value(id);
        let color = if !enabled {
            theme.muted
        } else if active {
            theme.accent
        } else {
            theme.ink
        };
        let background = if active {
            rgba((theme.accent << 8) | if theme.dark { 0x38 } else { 0x24 })
        } else {
            theme.tint(if enabled { hover } else { 0. })
        };
        self.control(
            id,
            label,
            icon(kind, color).size(rems(1.125)).into_any_element(),
            active,
            cx,
            action,
        )
        .size(rems(1.875))
        .min_h_0()
        .flex_shrink_0()
        .p_0()
        .rounded(px(theme.radius))
        .bg(background)
        .opacity(if enabled { 1. } else { 0.5 })
        .with_hint(hint, theme, self.hints())
    }
    fn hints(&self) -> bool {
        !self.blocking_overlay() && self.tool_menu.is_none()
    }
}

/// Sizes the formatting bar's stepper moves between, in canvas pixels.
const FONT_SIZES: &[f32] = &[
    6., 8., 9., 10., 11., 12., 14., 16., 18., 20., 24., 28., 32., 36., 40., 48., 56., 64., 72.,
    96., 128., 180.,
];

trait WithHint {
    fn with_hint(self, hint: impl Into<SharedString>, theme: Theme, show: bool) -> Self;
}
impl WithHint for Stateful<Div> {
    fn with_hint(self, hint: impl Into<SharedString>, theme: Theme, show: bool) -> Self {
        let hint = hint.into();
        self.when(show, |button| {
            button.tooltip(move |_, cx| {
                cx.new(|_| super::workspace::Hint(hint.clone(), theme))
                    .into()
            })
        })
    }
}
