//! Appearance controls; preferences and document content remain independent.
use super::*;
use folio_app::appearance::{Appearance, ThemeToken};
impl NotesView {
    pub(super) fn appearance_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let dark = self.controller.settings.dark;
        let follows = self.controller.settings.appearance.canvas_follows_theme;
        let adapt = self.controller.settings.appearance.adapt_ink;
        let mut body = div().flex().flex_col().gap_4().child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Appearance"),
        );
        let mut modes = div()
            .flex()
            .gap_1()
            .p_1()
            .rounded(px(theme.radius))
            .bg(rgb(theme.selected));
        for (id, label, value) in [
            ("light-theme", "Light", false),
            ("dark-theme", "Dark", true),
        ] {
            modes = modes.child(
                self.control(
                    id,
                    if value {
                        "Dark appearance"
                    } else {
                        "Light appearance"
                    },
                    div().child(label).into_any_element(),
                    dark == value,
                    cx,
                    move |this, _, _| {
                        this.controller.settings.dark = value;
                        this.controller.store_settings();
                    },
                )
                .flex_1()
                .when(dark == value, |s| s.bg(rgb(theme.surface)))
                .when(dark != value, |s| s.bg(rgb(theme.selected))),
            );
        }
        // Buttons use the visible short label while retaining their accessible name.
        body = body.child(modes).child(
            div()
                .text_xs()
                .text_color(rgb(theme.muted))
                .child("Light and dark colors are saved independently."),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child("Paper follows appearance")
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.muted))
                                .child("Use themed paper, or keep a fixed paper color."),
                        ),
                )
                .child(self.appearance_switch(
                    "canvas-theme",
                    "Paper follows appearance",
                    follows,
                    cx,
                    |this| {
                        this.controller.settings.appearance.canvas_follows_theme =
                            !this.controller.settings.appearance.canvas_follows_theme;
                    },
                )),
        );
        if !follows {
            body = body.child(
                self.control(
                    "paper-color",
                    "Custom paper color",
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .size(px(20.))
                                .rounded(px(4.))
                                .border_1()
                                .border_color(theme.border)
                                .bg(rgb(theme.canvas.paper)),
                        )
                        .child("Paper color")
                        .child(div().flex_1())
                        .child(format!("#{:06x}", theme.canvas.paper))
                        .into_any_element(),
                    false,
                    cx,
                    |this, w, cx| this.modal(Modal::CanvasColor, w, cx),
                )
                .w_full()
                .justify_start(),
            );
        }
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child("Keep ink readable")
                        .child(div().text_xs().text_color(rgb(theme.muted)).child(
                            "Adjust low-contrast ink on screen. Exports keep original colors.",
                        )),
                )
                .child(self.appearance_switch(
                    "adapt-ink",
                    "Keep ink readable",
                    adapt,
                    cx,
                    |this| {
                        this.controller.settings.appearance.adapt_ink =
                            !this.controller.settings.appearance.adapt_ink;
                    },
                )),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child("Corner radius")
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(self.button("radius-less", "−", false, cx, |this, _, _| {
                            this.controller.settings.appearance.radius =
                                (this.controller.settings.appearance.radius() - 2.).max(0.);
                            this.controller.store_settings();
                        }))
                        .child(
                            div()
                                .w(px(44.))
                                .text_sm()
                                .text_color(rgb(theme.muted))
                                .child(format!("{} px", theme.radius as u32)),
                        )
                        .child(self.button("radius-more", "+", false, cx, |this, _, _| {
                            this.controller.settings.appearance.radius =
                                (this.controller.settings.appearance.radius() + 2.).min(20.);
                            this.controller.store_settings();
                        })),
                ),
        );
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(self.button(
                    "customize-theme",
                    "Customize colors",
                    self.theme_colors_open,
                    cx,
                    |this, _, _| this.theme_colors_open = !this.theme_colors_open,
                ))
                .child(div().flex_1())
                .child(
                    self.button(
                        "reset-appearance",
                        "Reset appearance",
                        false,
                        cx,
                        |this, _, _| {
                            this.controller.settings.appearance = Appearance::default();
                            this.controller.store_settings();
                        },
                    )
                    .text_xs()
                    .text_color(rgb(theme.muted)),
                ),
        );
        if self.theme_colors_open {
            let palette = self.controller.settings.appearance.palette(dark);
            let mut colors = div().flex().flex_col().gap_1();
            for token in ThemeToken::ALL {
                let color = palette[&token];
                colors = colors.child(
                    self.control(
                        format!("theme-color-{}", token.key()),
                        format!("Customize {}", token.label()),
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .w_full()
                            .child(
                                div()
                                    .size(px(20.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(rgba(color.0)),
                            )
                            .child(token.label())
                            .child(div().flex_1())
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.muted))
                                    .child(color.hex()),
                            )
                            .into_any_element(),
                        false,
                        cx,
                        move |this, w, cx| this.modal(Modal::ThemeColor { dark, token }, w, cx),
                    )
                    .w_full()
                    .justify_start()
                    .py_2(),
                );
            }
            body = body
                .child(div().text_xs().text_color(rgb(theme.muted)).child(
                    "Mix a color interactively or enter a hex value. Reset restores the supplied palette.",
                ))
                .child(colors);
        }
        body.child(div().h(px(1.)).bg(theme.border).mt_2()).child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Writing & gestures"),
        )
    }
    pub(super) fn appearance_switch(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        self.control(
            id,
            format!("{label}: {}", if on { "On" } else { "Off" }),
            {
                let progress = self.motion.borrow_mut().value(
                    id,
                    if on { 1. } else { 0. },
                    self.controller.settings.reduce_motion,
                    Instant::now(),
                );
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        window.paint_quad(
                            fill(
                                bounds,
                                rgb(super::theme::mix(theme.selected, theme.accent, progress)),
                            )
                            .corner_radii(px(9.)),
                        );
                        let knob = Bounds::new(
                            point(
                                bounds.origin.x + px(3. + 14. * progress),
                                bounds.origin.y + px(3.),
                            ),
                            size(px(12.), px(12.)),
                        );
                        window.paint_quad(
                            fill(
                                knob,
                                rgb(super::theme::mix(
                                    theme.muted,
                                    theme.primary_foreground,
                                    progress,
                                )),
                            )
                            .corner_radii(px(6.)),
                        );
                    },
                )
                .w(px(32.))
                .h(px(18.))
                .into_any_element()
            },
            on,
            cx,
            move |this, _, _| {
                action(this);
                this.controller.store_settings();
            },
        )
        .px_1()
        .py_2()
        .flex_shrink_0()
    }
}
