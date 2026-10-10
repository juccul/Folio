use super::*;
fn sample(style: &folio_document::PenStyle) -> Div {
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
    let mut color = rgb(style.color.rgb());
    color.a = style.opacity;
    div()
        .w_full()
        .h(rems(2.))
        .overflow_hidden()
        .rounded_sm()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    window.with_content_mask(Some(ContentMask { bounds }), |w| {
                        super::color_picker::checkerboard(bounds, w);
                        if let Some(path) = &path {
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
                        }
                    });
                },
            )
            .size_full(),
        )
}
impl NotesView {
    pub(super) fn compact_preset_row(
        &self,
        preset: &folio_app::PenPreset,
        active: bool,
        close: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let id = preset.id;
        let hint: SharedString = format!(
            "{:?} · {:.1} px · {}",
            preset.style.tool,
            preset.style.width,
            preset.style.color.hex()
        )
        .into();
        self.control(
            format!("preset-{id}"),
            format!(
                "{} preset {}",
                if close { "Use" } else { "Manage" },
                preset.name
            ),
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .size(rems(1.))
                        .flex_shrink_0()
                        .rounded_full()
                        .border_1()
                        .border_color(theme.border)
                        .bg(rgb(preset.style.color.rgb())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(preset.name.clone()),
                )
                .child(if active {
                    icon(Icon::Check, theme.ink).into_any_element()
                } else {
                    div().size(rems(1.125)).into_any_element()
                })
                .into_any_element(),
            active,
            cx,
            move |this, window, _| {
                if close {
                    this.apply_pen_preset(id);
                    this.dismiss_popovers();
                    this.focus.focus(window);
                } else {
                    this.pen_preset_target = Some(id);
                }
            },
        )
        .w_full()
        .justify_start()
        .min_h_0()
        .py_1()
        .px_2()
        .tooltip(move |_, cx| {
            cx.new(|_| super::workspace::Hint(hint.clone(), theme))
                .into()
        })
    }

    pub(super) fn preset_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let presets = self.controller.settings.pen_presets.clone();
        if self
            .pen_preset_target
            .is_none_or(|id| !presets.iter().any(|preset| preset.id == id))
        {
            self.pen_preset_target = presets
                .iter()
                .find(|preset| preset.style == self.controller.style)
                .or_else(|| presets.first())
                .map(|preset| preset.id);
        }
        let mut panel = div().flex().flex_col().gap_2();
        if presets.is_empty() {
            return panel.child(
                div()
                    .px_2()
                    .py_2()
                    .text_sm()
                    .text_color(rgb(theme.muted))
                    .child("Save a pen you use often to find it here."),
            );
        }
        let mut list = div()
            .id("pen-preset-list")
            .w_full()
            .max_h(rems(7.5))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for preset in &presets {
            list = list.child(self.compact_preset_row(
                preset,
                Some(preset.id) == self.pen_preset_target,
                false,
                cx,
            ));
        }
        panel = panel.child(list);
        if let Some(preset) = presets
            .iter()
            .find(|preset| Some(preset.id) == self.pen_preset_target)
        {
            let id = preset.id;
            panel = panel
                .child(sample(&preset.style))
                .child(
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child(format!(
                            "{:?} · {:.1} px · {}",
                            preset.style.tool,
                            preset.style.width,
                            preset.style.color.hex()
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            self.control(
                                format!("preset-use-{id}"),
                                format!("Use preset {}", preset.name),
                                div().child("Use").into_any_element(),
                                self.controller.style == preset.style,
                                cx,
                                move |this, _, _| this.apply_pen_preset(id),
                            )
                            .flex_1()
                            .min_h_0()
                            .text_xs()
                            .px_2()
                            .py_1(),
                        )
                        .child(
                            self.button(
                                format!("preset-update-{id}"),
                                "Update",
                                false,
                                cx,
                                move |this, _, _| {
                                    if let Err(error) = this.controller.update_preset(id) {
                                        this.controller.error = Some(error);
                                    }
                                },
                            )
                            .flex_1()
                            .min_h_0()
                            .text_xs()
                            .px_2()
                            .py_1(),
                        )
                        .child(
                            self.button(
                                format!("preset-rename-{id}"),
                                "Rename",
                                false,
                                cx,
                                move |this, w, cx| this.modal(Modal::RenamePreset(id), w, cx),
                            )
                            .flex_1()
                            .min_h_0()
                            .text_xs()
                            .px_2()
                            .py_1(),
                        )
                        .child(
                            self.button(
                                format!("preset-delete-{id}"),
                                "Delete",
                                false,
                                cx,
                                move |this, _, _| {
                                    this.controller.delete_preset(id);
                                    this.pen_preset_target = None;
                                },
                            )
                            .flex_1()
                            .min_h_0()
                            .text_xs()
                            .px_2()
                            .py_1(),
                        ),
                )
                .child(
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child("Update uses the current pen."),
                );
        }
        panel
    }
}
