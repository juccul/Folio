use super::*;
use folio_document::{Point, Rect};
/// Keep the action row close to the selected bounds and inside the canvas.
pub(super) fn toolbar_origin(selection: Rect, canvas: (f32, f32), toolbar: (f32, f32)) -> Point {
    let margin = 12.;
    let width = toolbar.0.min((canvas.0 - margin * 2.).max(0.));
    let height = toolbar.1.min((canvas.1 - margin * 2.).max(0.));
    let above = selection.min.y - height - margin;
    let y = if above >= margin {
        above
    } else {
        selection.max.y + margin
    };
    Point::new(
        selection
            .min
            .x
            .clamp(margin, (canvas.0 - width - margin).max(margin)),
        y.clamp(margin, (canvas.1 - height - margin).max(margin)),
    )
}

struct SelectionHint {
    text: SharedString,
    theme: Theme,
}
impl Render for SelectionHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(6.))
            .bg(rgb(self.theme.popover))
            .border_1()
            .border_color(self.theme.border)
            .text_xs()
            .text_color(rgb(self.theme.ink))
            .child(self.text.clone())
    }
}
impl NotesView {
    fn selection_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        active: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let id = id.into();
        let label = label.into();
        let theme = Theme::new(&self.controller.settings);
        let kind = match id.as_ref() {
            "solve-selection" => Icon::Calculator,
            "recognize-math" => Icon::Math,
            "index-handwriting" => Icon::Search,
            "recognize-text" | "review-recognition" => Icon::Text,
            "cancel-recognition" => Icon::Close,
            "edit-equation" => Icon::Pencil,
            "crop-image" => Icon::Crop,
            "crop-image-coordinates" => Icon::Sliders,
            _ => Icon::Undo,
        };
        self.control(
            id,
            label.clone(),
            icon(kind, theme.ink).size(rems(1.125)).into_any_element(),
            active,
            cx,
            action,
        )
        .px_2()
        .py_1()
        .min_h(rems(2.25))
        .min_w(rems(2.25))
        .rounded(px(8.))
        .bg(transparent_black())
        .hover(move |s| s.bg(rgb(theme.selected)))
        .tooltip(move |_, cx| {
            cx.new(|_| SelectionHint {
                text: label.clone(),
                theme,
            })
            .into()
        })
    }

    pub(super) fn selection_toolbar_origin(&self) -> DocPoint {
        let canvas_size = self
            .canvas_bounds
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
            .unwrap_or((1000., 700.));
        let bounds = self.controller.selection_bounds().unwrap_or_default();
        let viewport = self.controller.session().viewport;
        let bounds = folio_document::Rect::from_points([
            viewport.to_screen(bounds.min),
            viewport.to_screen(bounds.max),
        ]);
        selection::toolbar_origin(
            bounds,
            canvas_size,
            (
                f32::from(self.selection_toolbar_size.width),
                f32::from(self.selection_toolbar_size.height),
            ),
        )
    }
    pub(super) fn selection_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let objects: Vec<_> = self
            .controller
            .session()
            .selection
            .iter()
            .filter_map(|id| self.controller.page().objects.get(id))
            .cloned()
            .collect();
        let can_solve = objects.iter().any(|o| {
            matches!(
                o.as_ref(),
                folio_document::Object::Stroke(_)
                    | folio_document::Object::Text(_)
                    | folio_document::Object::Equation(_)
                    | folio_document::Object::Image(_)
            )
        });
        let canvas_size = self
            .canvas_bounds
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
            .unwrap_or((1000., 700.));
        let origin = self.selection_toolbar_origin();
        let entity = cx.entity().downgrade();
        let dimensions = canvas(
            move |bounds, window, cx| {
                let _ = entity.update(cx, |view, cx| {
                    if view.selection_toolbar_size != bounds.size {
                        view.selection_toolbar_size = bounds.size;
                        cx.on_next_frame(window, |_, _, cx| cx.notify());
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let mut row = div()
            .occlude()
            .absolute()
            .left(px(origin.x))
            .top(px(origin.y))
            .max_w(px((canvas_size.0 - 24.).max(40.)))
            .flex_wrap()
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .rounded(px(12.))
            .bg(rgb(theme.popover))
            .border_1()
            .border_color(theme.border)
            .shadow_md();
        let mut editing = div().flex().items_center().gap_1();
        for (id, label, kind, shortcut, action) in [
            ("selection-cut", "Cut", Icon::Cut, "Ctrl+X", 1),
            ("selection-copy", "Copy", Icon::Copy, "Ctrl+C", 2),
            ("delete-selection", "Delete", Icon::Trash, "Delete", 3),
        ] {
            editing = editing.child(
                self.control(
                    id,
                    label,
                    icon(
                        kind,
                        if action == 3 {
                            theme.destructive
                        } else {
                            theme.ink
                        },
                    )
                    .size(rems(1.125))
                    .into_any_element(),
                    false,
                    cx,
                    move |this, _, cx| match action {
                        1 => this.copy(true, cx),
                        2 => this.copy(false, cx),
                        _ => this.controller.delete_selection(),
                    },
                )
                .px_2()
                .py_1()
                .min_h(rems(2.25))
                .min_w(rems(2.25))
                .rounded(px(8.))
                .bg(transparent_black())
                .hover(move |s| s.bg(rgb(theme.selected)))
                .tooltip(move |_, cx| {
                    cx.new(|_| SelectionHint {
                        text: format!("{label} · {shortcut}").into(),
                        theme,
                    })
                    .into()
                }),
            );
        }
        row = row
            .child(editing)
            .child(div().w(px(1.)).h(rems(1.25)).mx_1().bg(theme.border));
        for id in self.controller.session().selection.clone() {
            if let Some(folio_document::Object::Equation(e)) =
                self.controller.page().objects.get(&id).map(|o| o.as_ref())
                && let Some(error) = e
                    .math_link
                    .as_ref()
                    .and_then(|link| link.last_error.clone())
            {
                row = row
                    .child(
                        div()
                            .text_xs()
                            .max_w(px(240.))
                            .child(format!("Out of date: {error}")),
                    )
                    .child(self.selection_button(
                        format!("retry-linked-math-{id}"),
                        "Retry calculation",
                        false,
                        cx,
                        move |this, _, _| {
                            if let Err(error) = this.controller.retry_linked_math(id) {
                                this.controller.status = error;
                            }
                        },
                    ));
            }
        }
        if can_solve {
            row = row.child(self.selection_button(
                "solve-selection",
                "Solve",
                false,
                cx,
                |this, window, cx| this.start_math(window, cx),
            ));
        }
        if self.controller.recognition_pending {
            row = row
                .child(div().text_xs().px_2().max_w(px(280.)).truncate().child(
                    if self.controller.recognition_replacing {
                        "Rendering equation…".to_string()
                    } else {
                        self.controller.recognition_status.clone()
                    },
                ))
                .child(self.selection_button(
                    "cancel-recognition",
                    "Cancel recognition",
                    false,
                    cx,
                    |this, _, _| this.controller.cancel_recognition(),
                ));
        } else if self.controller.can_recognize_selection() {
            row = row.child(self.selection_button(
                "index-handwriting",
                "Index handwriting…",
                false,
                cx,
                |this, _, _| {
                    if let Err(e) = this.controller.index_selected_handwriting() {
                        this.controller.error = Some(e);
                    }
                },
            ));
            for (id, label, kind) in [
                ("recognize-text", "Recognize text", RecognitionKind::Text),
                ("recognize-math", "Recognize math", RecognitionKind::Math),
            ] {
                row = row.child(
                    self.selection_button(id, label, false, cx, move |this, _, _| {
                        if let Err(error) = this.controller.recognize_selection(kind) {
                            this.controller.error = Some(error);
                        }
                    }),
                );
            }
        }
        if self.controller.recognition_review.is_some() {
            row = row.child(self.selection_button(
                "review-recognition",
                "Review text",
                false,
                cx,
                |this, window, cx| this.modal(Modal::Recognition, window, cx),
            ));
        }
        if objects.len() == 1
            && let Some(id) = self
                .controller
                .session()
                .selection
                .iter()
                .find(|id| {
                    matches!(
                        self.controller.page().objects.get(id).map(|o| o.as_ref()),
                        Some(folio_document::Object::Equation(_))
                    )
                })
                .copied()
        {
            row = row.child(self.selection_button(
                "edit-equation",
                "Edit equation",
                false,
                cx,
                move |this, window, cx| this.modal(Modal::EditEquation(id), window, cx),
            ));
        }
        if objects.len() == 1 && matches!(objects[0].as_ref(), folio_document::Object::Image(_)) {
            row = row
                .child(
                    self.selection_button("crop-image", "Crop…", false, cx, |this, w, cx| {
                        this.start_image_crop(w, cx)
                    }),
                )
                .child(self.selection_button(
                    "crop-image-coordinates",
                    "Crop with numbers…",
                    false,
                    cx,
                    |this, w, cx| this.modal(Modal::Crop, w, cx),
                ))
                .child(self.selection_button(
                    "uncrop-image",
                    "Reset crop",
                    false,
                    cx,
                    |this, _, _| this.controller.crop_selection(None),
                ));
        }
        row.child(dimensions)
            .id("selection-actions")
            .flex_wrap()
            .max_h(px((canvas_size.1 - 24.).max(28.)))
            .overflow_y_scroll()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn actions_follow_selection_without_leaving_small_or_edge_viewports() {
        assert_eq!(
            toolbar_origin(Rect::new(300., 300., 100., 80.), (1000., 700.), (500., 60.)),
            Point::new(300., 228.)
        );
        assert_eq!(
            toolbar_origin(Rect::new(900., 0., 100., 20.), (1000., 700.), (500., 60.)),
            Point::new(488., 32.)
        );
        assert_eq!(
            toolbar_origin(Rect::new(-300., 1000., 50., 20.), (200., 100.), (500., 60.)),
            Point::new(12., 28.)
        );
    }
}
