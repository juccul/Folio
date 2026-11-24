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
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(theme.corner_md()))
            .border_1()
            .border_color(theme.glass_edge())
            .bg(theme.raised())
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
                            this.controller.set_theme(dark);
                        } else {
                            this.controller.apply_system_theme(window.dark);
                        }
                        this.controller.store_settings();
                    },
                )
                // Equal segments, so the choice moves between even slots.
                .w(rems(4.5))
                .px_2()
                .py_1()
                .min_h(rems(1.875))
                .bg(theme.tint(if selected { 1. } else { 0. })),
            );
        }
        let mut theme_rows = vec![
            self.settings_row(
                "Color theme",
                "System follows your device. Each theme keeps its own colors.",
            )
            .child(modes),
            self.settings_row("Interface corners", "Choose how rounded controls appear.")
                .child(self.settings_stepper(
                    ("radius-less", "Decrease corner radius"),
                    ("radius-more", "Increase corner radius"),
                    format!("{} px", theme.radius as u32),
                    theme.radius as f64,
                    cx,
                    |this, sign| {
                        let appearance = &mut this.controller.settings.appearance;
                        appearance.radius = (appearance.radius() + sign * 2.).clamp(0., 20.);
                    },
                )),
            self.settings_row(
                "Glass opacity",
                "How solid menus, dialogs, sidebars and bars appear.",
            )
            .child(self.settings_stepper(
                ("glass-opacity-less", "Decrease glass opacity"),
                ("glass-opacity-more", "Increase glass opacity"),
                format!("{}%", (theme.glass_opacity * 100.).round() as u32),
                (theme.glass_opacity * 100.).round() as f64,
                cx,
                |this, sign| {
                    let appearance = &mut this.controller.settings.appearance;
                    // Round to whole steps so repeated changes stay exact.
                    let next = ((appearance.glass_opacity() * 20.).round() + sign) / 20.;
                    appearance.glass_opacity = next.clamp(0.3, 1.);
                },
            )),
            self.settings_row(
                "Glass blur",
                "How strongly content behind glass is blurred.",
            )
            .child(self.settings_stepper(
                ("glass-blur-less", "Decrease glass blur"),
                ("glass-blur-more", "Increase glass blur"),
                if theme.glass_blur == 0. {
                    "Off".into()
                } else {
                    format!("{} px", theme.glass_blur as u32)
                },
                theme.glass_blur as f64,
                cx,
                |this, sign| {
                    let appearance = &mut this.controller.settings.appearance;
                    appearance.glass_blur = (appearance.glass_blur() + sign * 4.).clamp(0., 40.);
                },
            )),
            self.settings_row("Custom palette", "Fine-tune the colors of this theme.")
                .child(self.settings_action(
                    "customize-theme",
                    if self.theme_colors_open {
                        "Hide colors"
                    } else {
                        "Edit colors…"
                    },
                    cx,
                    |this, _, _| this.theme_colors_open = !this.theme_colors_open,
                )),
        ];
        if self.theme_colors_open {
            let palette = self.controller.settings.appearance.palette(dark);
            let mut colors = div().flex().flex_col().gap(px(1.)).px_2().pb_2();
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
                                    .rounded(px(theme.corner_sm()))
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
                    .min_h(rems(2.))
                    .py_1()
                    .bg(transparent_black()),
                );
            }
            // Part of the palette row, without a divider between them.
            let last = theme_rows.pop().unwrap();
            theme_rows.push(div().flex().flex_col().child(last).child(colors));
        }
        let mut paper_rows = vec![
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
        ];
        if !follows {
            paper_rows.push(
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
                                    .rounded(px(theme.corner_sm()))
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
                    .flex_shrink_0()
                    .px_3()
                    .py_1()
                    .border_color(theme.border),
                ),
            );
        }
        paper_rows.push(
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
        );
        // Theme and paper come first; surface styling follows.
        let palette = theme_rows.pop().unwrap();
        let styling = theme_rows.split_off(1);
        theme_rows.push(palette);
        div()
            .flex()
            .flex_col()
            .child(self.settings_group(Some("Theme"), theme_rows))
            .child(self.settings_group(Some("Paper and ink"), paper_rows))
            .child(self.settings_group(Some("Interface styling"), styling))
            .child(div().flex().child(self.settings_action(
                "reset-appearance",
                "Reset appearance",
                cx,
                |this, _, _| {
                    this.controller.settings.appearance = Appearance::default();
                    this.controller.store_settings();
                },
            )))
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
            switch_track(theme, id, on).into_any_element(),
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
        // No box around a switch: its track lights up instead.
        .hover(|s| s.bg(transparent_black()))
        .active(|s| s.bg(transparent_black()))
        .flex_shrink_0()
    }
}
/// The track and knob of an on/off switch. The knob stretches elastically as it
/// springs across while the track's colour and border cross-fade.
pub(super) fn switch_track(theme: Theme, key: &str, on: bool) -> impl IntoElement {
    let track_color = if on {
        theme.accent
    } else {
        super::theme::mix(theme.surface, theme.ink, if theme.dark { 0.28 } else { 0.2 })
    };
    let border_color = if on {
        super::theme::mix(theme.accent, 0xffffff, 0.16)
    } else {
        super::theme::mix(theme.surface, theme.ink, if theme.dark { 0.36 } else { 0.26 })
    };
    div()
        .id(format!("{key}-track"))
        .w(rems(2.25))
        .h(rems(1.25))
        .p(px(2.))
        .flex()
        .items_center()
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .border_color(rgb(border_color))
        // Off, the track is a clear mid tone, never a near-invisible wash.
        .bg(rgb(track_color))
        .hover(move |s| s.bg(theme.tint(1.)))
        .el(|e| e.motion(crate::motion::GLIDE).animated_changes())
        .child(
            div()
                .id(format!("{key}-knob"))
                .size(rems(0.875))
                .ml(rems(if on { 1. } else { 0. }))
                .rounded_full()
                .bg(rgb(if on { theme.primary_foreground } else { 0xffffff }))
                .shadow_sm()
                .el(|e| {
                    e.motion(crate::motion::POP)
                        .lead(crate::motion::SWITCH_LEAD, crate::motion::SWITCH_TRAIL)
                        .animated_changes()
                }),
        )
}
