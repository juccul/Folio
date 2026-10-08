use super::*;
fn sample(style: &folio_document::PenStyle, theme: Theme) -> Div {
    let mut builder = folio_ink::StrokeBuilder::new(style.clone());
    for i in 0..=24 {
        let t = i as f32 / 24.;
        builder.push(folio_document::StrokePoint::new(
            DocPoint::new(12. + 136. * t, 24. + 8. * (t * std::f32::consts::TAU).sin()),
            0.2 + 0.7 * t,
            i * 16,
        ));
    }
    let mut path = PathBuilder::fill();
    for contour in folio_ink::outline(builder.path()) {
        if let Some(first) = contour.first() {
            path.move_to(point(px(first.x), px(first.y)));
            for p in contour.iter().skip(1) {
                path.line_to(point(px(p.x), px(p.y)));
            }
            path.close();
        }
    }
    let path = path.build().ok();
    let mut color = rgb(theme.canvas.ink(style.color.rgb()));
    color.a = style.opacity;
    div()
        .w_full()
        .h(rems(2.5))
        .overflow_hidden()
        .rounded_sm()
        .bg(rgb(theme.canvas.paper))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    if let Some(path) = &path {
                        window.with_content_mask(Some(ContentMask { bounds }), |w| {
                            w.paint_path(
                                path.clone().transformed([
                                    f32::from(bounds.size.width) / 160.,
                                    0.,
                                    0.,
                                    f32::from(bounds.size.height) / 48.,
                                    f32::from(bounds.left()),
                                    f32::from(bounds.top()),
                                ]),
                                color,
                            )
                        });
                    }
                },
            )
            .size_full(),
        )
}
impl NotesView {
    pub(super) fn preset_controls(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut list = div().flex().flex_col().gap_3();
        for preset in self.controller.settings.pen_presets.clone() {
            let id = preset.id;
            let active = self.controller.style == preset.style;
            let content = div()
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_xs().child(preset.name.clone()))
                .child(sample(&preset.style, theme))
                .child(div().text_xs().text_color(rgb(theme.muted)).child(format!(
                    "{:?} · {:.2} mm · {}",
                    preset.style.tool,
                    preset.style.width * 25.4 / 96.,
                    preset.style.color.hex()
                )));
            list = list
                .child(
                    self.control(
                        format!("preset-{id}"),
                        format!("Use preset {}", preset.name),
                        content.into_any_element(),
                        active,
                        cx,
                        move |this, _, _| {
                            this.controller.apply_preset(id);
                            this.region_selection = None;
                        },
                    )
                    .w_full(),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .child(
                            self.button(
                                format!("preset-update-{id}"),
                                "Update",
                                false,
                                cx,
                                move |this, _, _| {
                                    if let Err(e) = this.controller.update_preset(id) {
                                        this.controller.error = Some(e);
                                    }
                                },
                            )
                            .text_xs()
                            .px_2(),
                        )
                        .child(
                            self.button(
                                format!("preset-rename-{id}"),
                                "Rename",
                                false,
                                cx,
                                move |this, w, cx| this.modal(Modal::RenamePreset(id), w, cx),
                            )
                            .text_xs()
                            .px_2(),
                        )
                        .child(
                            self.button(
                                format!("preset-delete-{id}"),
                                "Delete",
                                false,
                                cx,
                                move |this, _, _| this.controller.delete_preset(id),
                            )
                            .text_xs()
                            .px_2(),
                        ),
                );
        }
        list.child(div().text_xs().text_color(rgb(theme.muted)).child("Update replaces a preset with the current pen. Deleting a preset keeps your current pen settings."))
    }
}
