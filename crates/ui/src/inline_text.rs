use super::{
    NotesView, Theme,
    field::{Field, InlineStyle},
};
use folio_app::Tool;
use folio_document::{Id, Object, Point as DocPoint, Rect, Transform};
use gpui::{prelude::*, *};

pub struct Editor {
    pub note: Id,
    pub page: Id,
    pub id: Id,
    pub field: Entity<Field>,
    _subscriptions: Vec<Subscription>,
}
impl NotesView {
    pub(super) fn begin_inline_text(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Object::Text(text)) = self.controller.page().objects.get(&id).map(|o| o.as_ref())
        else {
            return;
        };
        let field = cx.new(|cx| Field::new(text.text.clone(), true, cx));
        let note = self.controller.active;
        let page = self.controller.page().id;
        let change = cx.observe(&field, move |this, field, cx| {
            // Editing is saved continuously. A stale editor cannot modify a
            // different page/document after navigation or an undo removes it.
            if this.controller.active == note && this.controller.page().id == page {
                let content = field.read(cx).content.clone();
                this.controller.edit_text(id, |text| text.text = content);
                cx.notify();
            }
        });
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
        self.controller.set_tool(Tool::Lasso);
        self.controller.session_mut().selection = std::collections::HashSet::from([id]);
        self.inline_text = Some(Editor {
            note,
            page,
            id,
            field,
            _subscriptions: vec![change, submit, bounds_changed],
        });
        cx.notify();
    }
    pub(super) fn finish_inline_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = &self.inline_text
            && editor.note == self.controller.active
            && editor.page == self.controller.page().id
        {
            let content = editor.field.read(cx).content.clone();
            self.controller
                .edit_text(editor.id, |text| text.text = content);
        }
        self.inline_text = None;
        self.focus.focus(window);
        cx.notify();
    }
    pub(super) fn inline_element(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.inline_text.as_ref()?;
        let selected = self.controller.session().selection.len() == 1
            && self.controller.session().selection.contains(&editor.id);
        let valid = self.controller.active == editor.note
            && self.controller.page().id == editor.page
            && selected
            && !self.library_open
            && self.controller.tool == Tool::Lasso;
        let text = self
            .controller
            .page()
            .objects
            .get(&editor.id)
            .and_then(|o| {
                if let Object::Text(t) = o.as_ref() {
                    Some(t.clone())
                } else {
                    None
                }
            });
        if !valid || text.is_none() {
            self.inline_text = None;
            return None;
        }
        let text = text.unwrap();
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
        field.update(cx, |field, cx| {
            if field.content != text.text {
                field.set_content(text.text.clone(), cx);
            }
            field.theme = theme;
            field.inline = Some(InlineStyle {
                text,
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
        })?;
        let theme = Theme::new(&self.controller.settings);
        let mut row = div()
            .flex()
            .items_center()
            .gap_1()
            .px_4()
            .py_1()
            .bg(rgb(theme.surface))
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
                    format!("{} pt", text.font_size),
                    false,
                    cx,
                    |this, w, cx| this.modal(super::Modal::FontSize, w, cx),
                )
                .text_xs(),
            );
        for (id, label, active, kind) in [
            ("bold", "B", text.bold, 0),
            ("italic", "I", text.italic, 1),
            ("underline", "U", text.underline, 2),
            (
                "align-left",
                "Left",
                text.alignment == folio_document::Alignment::Left,
                3,
            ),
            (
                "align-center",
                "Center",
                text.alignment == folio_document::Alignment::Center,
                4,
            ),
            (
                "align-right",
                "Right",
                text.alignment == folio_document::Alignment::Right,
                5,
            ),
            (
                "bullet",
                "• List",
                text.list == folio_document::ListStyle::Bullet,
                6,
            ),
            (
                "numbered",
                "1. List",
                text.list == folio_document::ListStyle::Numbered,
                7,
            ),
        ] {
            row = row.child(
                self.button(id, label, active, cx, move |this, window, cx| {
                    for id in this.controller.session().selection.clone() {
                        this.controller.edit_text(id, |t| match kind {
                            0 => t.bold = !t.bold,
                            1 => t.italic = !t.italic,
                            2 => t.underline = !t.underline,
                            3 => t.alignment = folio_document::Alignment::Left,
                            4 => t.alignment = folio_document::Alignment::Center,
                            5 => t.alignment = folio_document::Alignment::Right,
                            6 => {
                                t.list = if t.list == folio_document::ListStyle::Bullet {
                                    folio_document::ListStyle::None
                                } else {
                                    folio_document::ListStyle::Bullet
                                }
                            }
                            _ => {
                                t.list = if t.list == folio_document::ListStyle::Numbered {
                                    folio_document::ListStyle::None
                                } else {
                                    folio_document::ListStyle::Numbered
                                }
                            }
                        });
                    }
                    if let Some(editor) = &this.inline_text {
                        editor.field.read(cx).focus.focus(window);
                    }
                })
                .text_xs()
                .px_2()
                .py_1(),
            );
        }
        let swatch = div()
            .size(px(14.))
            .rounded(px(3.))
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
                    .child("Click inside to edit"),
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
