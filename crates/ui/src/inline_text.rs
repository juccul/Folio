use super::{
    NotesView, Theme,
    field::{Field, InlineStyle},
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
                Object::Text(t) => Some(t),
                _ => None,
            }
        })?;
        let theme = Theme::new(&self.controller.settings);
        let mut row = div()
            .flex()
            .items_center()
            .gap_1()
            .px_4()
            .py_1()
            .border_b_1()
            .border_color(theme.border)
            .child(
                self.button(
                    "font",
                    text.font_family.clone(),
                    false,
                    cx,
                    |this, w, cx| this.modal(super::Modal::Font, w, cx),
                )
                .text_xs(),
            )
            .child(
                self.button(
                    "font-size",
                    format!("{} px", text.font_size),
                    false,
                    cx,
                    |this, w, cx| this.modal(super::Modal::FontSize, w, cx),
                )
                .text_xs(),
            );
        use folio_document::{Alignment, ListStyle};
        for (id, label, active, format) in [
            ("bold", "B", text.bold, TextFormat::Bold),
            ("italic", "I", text.italic, TextFormat::Italic),
            ("underline", "U", text.underline, TextFormat::Underline),
            (
                "align-left",
                "Left",
                text.alignment == Alignment::Left,
                TextFormat::Align(Alignment::Left),
            ),
            (
                "align-center",
                "Center",
                text.alignment == Alignment::Center,
                TextFormat::Align(Alignment::Center),
            ),
            (
                "align-right",
                "Right",
                text.alignment == Alignment::Right,
                TextFormat::Align(Alignment::Right),
            ),
            (
                "bullet",
                "• List",
                text.list == ListStyle::Bullet,
                TextFormat::List(ListStyle::Bullet),
            ),
            (
                "numbered",
                "1. List",
                text.list == ListStyle::Numbered,
                TextFormat::List(ListStyle::Numbered),
            ),
        ] {
            row = row.child(
                self.button(id, label, active, cx, move |this, window, cx| {
                    this.format_text(format, window, cx)
                })
                .text_xs()
                .px_2()
                .py_1(),
            );
        }
        let swatch = div()
            .size(px(14.))
            .rounded(px(theme.corner_sm()))
            .bg(rgb(text.color.rgb()));
        row = row.child(
            self.control(
                "text-color",
                "Text color",
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(swatch)
                    .child("Color")
                    .into_any_element(),
                false,
                cx,
                |this, w, cx| this.modal(super::Modal::TextColor, w, cx),
            )
            .text_xs(),
        );
        row = row.child(div().flex_1());
        row = if self.inline_text.is_some() {
            row.child(
                self.button("finish-text", "Done", false, cx, |this, w, cx| {
                    this.finish_inline_text(w, cx)
                })
                .text_xs(),
            )
        } else {
            let id = text.id;
            row.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Click inside or press Enter to edit"),
            )
            .child(
                self.button("edit-text", "Edit text", false, cx, move |this, w, cx| {
                    this.begin_inline_text(id, w, cx)
                })
                .text_xs(),
            )
        };
        Some(row.into_any_element())
    }
}
