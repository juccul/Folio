//! Appearance controls; preferences and document content remain independent.
use super::*;
use folio_app::appearance::{Appearance, ThemeToken};
impl NotesView {
    pub(super) fn appearance_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let dark = self.controller.settings.dark;
        let system = self.controller.settings.follow_system_theme;
        let follows = self.controller.settings.appearance.canvas_follows_theme;
        let mut modes = div()
            .flex()
            .gap_1()
            .p_1()
            .rounded(px(7.))
            .bg(rgb(theme.bg))
            .flex_shrink_0();
        for (id, label, value) in [
            ("system-theme", "System", None),
            ("light-theme", "Light", Some(false)),
            ("dark-theme", "Dark", Some(true)),
        ] {
            let selected = if system {
                value.is_none()
            } else {
                value == Some(dark)
            };
            modes = modes.child(
                self.control(
                    id,
                    match value {
                        None => "System appearance",
                        Some(true) => "Dark appearance",
                        Some(false) => "Light appearance",
                    },
                    div().child(label).into_any_element(),
                    selected,
                    cx,
                    move |this, window, _| {
                        this.controller.settings.follow_system_theme = value.is_none();
                        if let Some(dark) = value {
                            this.controller.settings.dark = dark;
                        } else {
                            this.controller.settings.apply_system_theme(matches!(
                                window.appearance(),
                                WindowAppearance::Dark | WindowAppearance::VibrantDark
                            ));
                        }
                        this.controller.store_settings();
                    },
                )
                .px_4()
                .py_1()
                .rounded(px(5.))
                .bg(rgb(if selected { theme.selected } else { theme.bg })),
            );
        }
        let mut body = div()
            .flex()
            .flex_col()
            .child(
                self.settings_row(
                    "Color theme",
                    "System follows your device. Light and dark palettes are saved separately.",
                )
                .child(modes),
            )
            .child(
                self.settings_row(
                    "Paper follows theme",
                    "Match the page to your light or dark workspace.",
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
                self.settings_row(
                    "Paper color",
                    "Use a fixed color for pages without a custom background.",
                )
                .child(
                    self.control(
                        "paper-color",
                        "Custom paper color",
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(rems(1.))
                                    .rounded(px(3.))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(rgb(theme.canvas.paper)),
                            )
                            .child(format!("#{:06X}", theme.canvas.paper))
                            .into_any_element(),
                        false,
                        cx,
                        |this, w, cx| this.modal(Modal::CanvasColor, w, cx),
                    )
                    .flex_shrink_0(),
                ),
            );
        }
        body = body
            .child(
                self.settings_row(
                    "Keep ink readable",
                    "Adapt ink contrast on screen. Export colors stay original.",
                )
                .child(self.appearance_switch(
                    "adapt-ink",
                    "Keep ink readable",
                    self.controller.settings.appearance.adapt_ink,
                    cx,
                    |this| {
                        this.controller.settings.appearance.adapt_ink =
                            !this.controller.settings.appearance.adapt_ink;
                    },
                )),
            )
            .child(
                self.settings_row("Interface corners", "Choose how rounded controls appear.")
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .gap_1()
                            .rounded(px(6.))
                            .bg(rgb(theme.bg))
                            .child(
                                self.control(
                                    "radius-less",
                                    "Decrease corner radius",
                                    div().child("−").into_any_element(),
                                    false,
                                    cx,
                                    |this, _, _| {
                                        this.controller.settings.appearance.radius =
                                            (this.controller.settings.appearance.radius() - 2.)
                                                .max(0.);
                                        this.controller.store_settings();
                                    },
                                )
                                .size(rems(2.))
                                .p_0()
                                .bg(transparent_black()),
                            )
                            .child(
                                div()
                                    .w(rems(3.5))
                                    .text_sm()
                                    .text_align(TextAlign::Center)
                                    .child(format!("{} px", theme.radius as u32)),
                            )
                            .child(
                                self.control(
                                    "radius-more",
                                    "Increase corner radius",
                                    div().child("+").into_any_element(),
                                    false,
                                    cx,
                                    |this, _, _| {
                                        this.controller.settings.appearance.radius =
                                            (this.controller.settings.appearance.radius() + 2.)
                                                .min(20.);
                                        this.controller.store_settings();
                                    },
                                )
                                .size(rems(2.))
                                .p_0()
                                .bg(transparent_black()),
                            ),
                    ),
            )
            .child(
                self.settings_row("Custom palette", "Fine-tune the colors of this theme.")
                    .child(
                        self.button(
                            "customize-theme",
                            if self.theme_colors_open {
                                "Hide colors"
                            } else {
                                "Edit colors…"
                            },
                            self.theme_colors_open,
                            cx,
                            |this, _, _| this.theme_colors_open = !this.theme_colors_open,
                        )
                        .flex_shrink_0(),
                    ),
            );
        if self.theme_colors_open {
            let palette = self.controller.settings.appearance.palette(dark);
            let mut colors = div().flex().flex_col().gap_1().mt_3();
            for token in ThemeToken::ALL {
                let color = palette[&token];
                colors = colors.child(
                    self.control(
                        format!("theme-color-{}", token.key()),
                        format!("Customize {}", token.label()),
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(rems(1.))
                                    .rounded(px(3.))
                                    .bg(rgba(color.0))
                                    .border_1()
                                    .border_color(theme.border),
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
                    .bg(transparent_black()),
                );
            }
            body = body.child(colors);
        }
        body.child(
            div().mt_5().flex().justify_start().child(
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
                .text_color(rgb(theme.muted))
                .bg(transparent_black())
                .px_0(),
            ),
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
        let progress = self.motion.borrow_mut().value(
            id,
            if on { 1. } else { 0. },
            self.controller.settings.reduce_motion,
            Instant::now(),
        );
        self.control(
            id,
            format!("{label}: {}", if on { "On" } else { "Off" }),
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let height = f32::from(bounds.size.height);
                    let inset = height * 0.15;
                    let diameter = height - inset * 2.;
                    window.paint_quad(
                        fill(
                            bounds,
                            rgb(super::theme::mix(theme.selected, theme.accent, progress)),
                        )
                        .corner_radii(px(height * 0.5)),
                    );
                    let knob = Bounds::new(
                        point(
                            bounds.origin.x
                                + px(inset + (f32::from(bounds.size.width) - height) * progress),
                            bounds.origin.y + px(inset),
                        ),
                        size(px(diameter), px(diameter)),
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
                        .corner_radii(px(diameter * 0.5)),
                    );
                },
            )
            .w(rems(2.25))
            .h(rems(1.25))
            .into_any_element(),
            on,
            cx,
            move |this, _, _| {
                action(this);
                this.controller.store_settings();
            },
        )
        .w(rems(2.75))
        .min_h(rems(2.25))
        .p_0()
        .bg(transparent_black())
        .flex_shrink_0()
    }
}
