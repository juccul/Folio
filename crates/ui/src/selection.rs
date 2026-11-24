use super::*;
use folio_document::{Point, Rect};
const ROTATION_HANDLE_CLEARANCE: f32 = 28.;
/// Keep the action row close to the selected bounds and inside the canvas.
pub(super) fn toolbar_origin(selection: Rect, canvas: (f32, f32), toolbar: (f32, f32)) -> Point {
    let margin = 12.;
    let width = toolbar.0.min((canvas.0 - margin * 2.).max(0.));
    let height = toolbar.1.min((canvas.1 - margin * 2.).max(0.));
    // Leave the rotation handle, drawn 24 px above the bounds, uncovered.
    let above = selection.min.y - height - margin - ROTATION_HANDLE_CLEARANCE;
    // Below, it clears the outline and handles drawn around the bounds.
    let y = if above >= margin {
        above
    } else {
        selection.max.y + margin + 12.
    };
    // Centred under the selection, clamped to the canvas.
    Point::new(
        (selection.center().x - width / 2.).clamp(margin, (canvas.0 - width - margin).max(margin)),
        y.clamp(margin, (canvas.1 - height - margin).max(margin)),
    )
}

impl NotesView {
    pub(super) fn selection_toolbar_origin(&self) -> DocPoint {
        let canvas_size = self
            .canvas_bounds
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
            .unwrap_or((1000., 700.));
        // Follow the drag preview so the row tracks the selection every frame.
        let r = self.controller.live_selection_bounds().unwrap_or_default();
        let viewport = self.controller.session().viewport;
        let bounds = folio_document::Rect::from_points(
            [
                r.min,
                Point::new(r.max.x, r.min.y),
                r.max,
                Point::new(r.min.x, r.max.y),
            ]
            .map(|p| viewport.to_screen(p)),
        );
        selection::toolbar_origin(
            bounds,
            canvas_size,
            (
                f32::from(self.selection_toolbar_size.width),
                f32::from(self.selection_toolbar_size.height),
            ),
        )
    }
    pub(super) fn selection_toolbar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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
            )
        });
        let canvas_size = self
            .canvas_bounds
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
            .unwrap_or((1000., 700.));
        let origin = self.selection_toolbar_origin();
        let dimensions = div().absolute().inset_0().el(|e| {
            e.on_layout(|s: &mut AppState, r: [f32; 4]| {
                let view = s.view.get_mut();
                let size = size(px(r[2]), px(r[3]));
                if view.selection_toolbar_size != size {
                    view.selection_toolbar_size = size;
                }
            })
        });
        // A small menu: cut, copy and delete side by side on top, then the
        // other actions as rows. A recognized result's review takes its
        // place, as a menu turns into the page of the action chosen in it.
        let hosted = self.modal_hosted_in(MenuHost::Selection);
        // The box springs to the size of what it shows (the review page
        // and back); where it sits follows the selection at once.
        // As wide as its rows need; the review page takes a set width.
        let menu = div()
            .when(hosted, |menu| menu.w(px(340_f32.min((canvas_size.0 - 24.).max(40.)))))
            .max_w(px((canvas_size.0 - 24.).max(40.)))
            .max_h(px((canvas_size.1 - 24.).max(28.)))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(1.))
            .p_1p5()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .shadow_md()
            .el(|e| e.motion(crate::motion::MORPH).animated_changes());
        let place = move |menu: Div, dimensions: Div, cx: &mut Context<Self>| {
            div()
                .id("selection-actions")
                .occlude()
                .absolute()
                .left(px(origin.x))
                .top(px(origin.y))
                .when(hosted, |root| {
                    root.on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, w, cx| {
                        this.cancel_modal(w, cx)
                    }))
                })
                .child(menu)
                .child(dimensions)
        };
        if hosted {
            let page = self.modal_panel(true, window, cx);
            return place(menu.child(page), dimensions, cx);
        }
        // Cut, copy and delete share the top row evenly.
        let mut editing = div().w_full().flex().items_center().gap_0p5();
        for (id, label, kind, shortcut, action) in [
            ("selection-cut", "Cut", Icon::Cut, "Ctrl+X", 1),
            ("selection-copy", "Copy", Icon::Copy, "Ctrl+C", 2),
            ("delete-selection", "Delete", Icon::Trash, "Delete", 3),
        ] {
            editing = editing.child(
                self.control(
                    id,
                    label,
                    icon(kind, if action == 3 { theme.destructive } else { theme.ink })
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
                .flex_1()
                .h(rems(2.))
                .min_h_0()
                .min_w(rems(2.))
                .p_0()
                .rounded(px(theme.radius))
                .bg(transparent_black())
                .hover(move |s| s.bg(theme.tint(1.)))
                .tooltip(format!("{label} · {shortcut}")),
            );
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        for id in self.controller.session().selection.clone() {
            if let Some(folio_document::Object::Equation(e)) =
                self.controller.page().objects.get(&id).map(|o| o.as_ref())
                && let Some(error) = e
                    .math_link
                    .as_ref()
                    .and_then(|link| link.last_error.clone())
            {
                rows.push(
                    document_actions::hint(theme, format!("Out of date: {error}"))
                        .px_2()
                        .py_1()
                        .flex_shrink()
                        .into_any_element(),
                );
                rows.push(self.selection_row(
                    format!("retry-linked-math-{id}"),
                    "Retry calculation",
                    Icon::Undo,
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
            rows.push(self.selection_row("solve-selection", "Solve", Icon::Calculator, cx, |this, window, cx| {
                this.start_math(window, cx)
            }));
        }
        if self.controller.recognition_pending {
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .min_h(rems(2.))
                    .px_2()
                    .child(working_dots(theme.muted))
                    .child(div().flex_1().min_w_0().text_xs().truncate().child(
                        if self.controller.recognition_replacing {
                            "Rendering equation…".to_string()
                        } else {
                            self.controller.recognition_status.clone()
                        },
                    ))
                    .into_any_element(),
            );
            rows.push(self.selection_row("cancel-recognition", "Cancel recognition", Icon::Close, cx, |this, _, _| {
                this.controller.cancel_recognition()
            }));
        } else if self.controller.can_recognize_selection() {
            for (id, label, kind, glyph) in [
                ("recognize-text", "Convert to text", RecognitionKind::Text, Icon::TextScan),
                ("recognize-math", "Convert to math", RecognitionKind::Math, Icon::Math),
            ] {
                rows.push(self.selection_row(id, label, glyph, cx, move |this, _, _| {
                    if let Err(error) = this.controller.recognize_selection(kind) {
                        this.controller.error = Some(error);
                    }
                }));
            }
        }
        if self.controller.recognition_review.is_some() {
            rows.push(self.selection_row("review-recognition", "Review text", Icon::Text, cx, |this, window, cx| {
                this.modal_in_menu(MenuHost::Selection, Modal::Recognition, window, cx)
            }));
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
            rows.push(self.selection_row("edit-equation", "Edit equation", Icon::Pencil, cx, move |this, window, cx| {
                this.modal(Modal::EditEquation(id), window, cx)
            }));
        }
        if objects.len() == 1 && matches!(objects[0].as_ref(), folio_document::Object::Image(_)) {
            rows.push(self.selection_row("crop-image", "Crop…", Icon::Crop, cx, |this, w, cx| {
                this.start_image_crop(w, cx)
            }));
            rows.push(self.selection_row("crop-image-coordinates", "Crop with numbers…", Icon::Sliders, cx, |this, w, cx| {
                this.modal(Modal::Crop, w, cx)
            }));
            rows.push(self.selection_row("uncrop-image", "Reset crop", Icon::Undo, cx, |this, _, _| {
                this.controller.crop_selection(None)
            }));
        }
        let has_rows = !rows.is_empty();
        let menu = menu
            .child(editing)
            .when(has_rows, |menu| menu.child(document_actions::separator(theme)))
            .children(rows);
        place(menu, dimensions, cx)
    }
    /// One action of the selection menu: an icon and its name.
    fn selection_row(
        &self,
        id: impl Into<SharedString>,
        label: &'static str,
        kind: Icon,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let theme = Theme::new(&self.controller.settings);
        self.menu_row(id, label.into(), div().child(label), kind, theme.muted, None, cx, action)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn actions_follow_selection_without_leaving_small_or_edge_viewports() {
        assert_eq!(
            toolbar_origin(Rect::new(300., 300., 100., 80.), (1000., 700.), (500., 60.)),
            // Centred under the selection.
            Point::new(100., 200.)
        );
        // The row ends above the rotation handle (24 px plus its radius).
        assert!(200. + 60. < 300. - 24. - 4.);
        assert_eq!(
            toolbar_origin(Rect::new(900., 0., 100., 20.), (1000., 700.), (500., 60.)),
            // Below the selection, clear of its outline and handles.
            Point::new(488., 44.)
        );
        assert_eq!(
            toolbar_origin(Rect::new(-300., 1000., 50., 20.), (200., 100.), (500., 60.)),
            Point::new(12., 28.)
        );
    }
}
