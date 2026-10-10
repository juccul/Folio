//! Focused pen choices with separate tuning and preset-management views.
use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Section {
    #[default]
    Pen,
    Settings,
    Presets,
}

pub(super) fn quick_presets(
    presets: &[folio_app::PenPreset],
    style: &folio_document::PenStyle,
) -> Vec<folio_app::PenPreset> {
    let mut visible: Vec<_> = presets.iter().take(3).cloned().collect();
    if let Some(active) = presets.iter().find(|preset| &preset.style == style)
        && !visible.iter().any(|preset| preset.id == active.id)
    {
        visible.pop();
        visible.push(active.clone());
    }
    visible
}

impl NotesView {
    pub(super) fn apply_pen_preset(&mut self, id: Id) {
        if self.controller.style.tool != InkTool::Highlighter
            && self
                .controller
                .settings
                .pen_presets
                .iter()
                .any(|preset| preset.id == id && preset.style.tool == InkTool::Highlighter)
        {
            self.writing_style = Some(self.controller.style.clone());
        }
        self.controller.apply_preset(id);
        self.pen_preset_target = Some(id);
        self.region_selection = None;
    }

    pub(super) fn pen_menu(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut panel = div().flex().flex_col().gap_1();
        if self.pen_menu_section != Section::Pen {
            panel = panel.child(
                self.control(
                    "pen-menu-back",
                    "Back to pens",
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(Icon::Back, theme.muted))
                        .child("Pen")
                        .into_any_element(),
                    false,
                    cx,
                    |this, _, _| this.pen_menu_section = Section::Pen,
                )
                .w_full()
                .justify_start()
                .min_h_0()
                .py_1(),
            );
        }
        match self.pen_menu_section {
            Section::Pen => {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child("Pen type"),
                );
                for (id, label, tool, kind) in [
                    ("ballpoint", "Ballpoint", InkTool::Ballpoint, Icon::Pen),
                    ("fountain", "Fountain", InkTool::Fountain, Icon::Pen),
                    ("pencil", "Pencil", InkTool::Pencil, Icon::Pencil),
                    ("marker", "Marker", InkTool::Marker, Icon::Marker),
                    (
                        "highlighter",
                        "Highlighter",
                        InkTool::Highlighter,
                        Icon::Highlighter,
                    ),
                ] {
                    let active = self.controller.style.tool == tool;
                    panel = panel.child(
                        self.control(
                            format!("pen-type-{id}"),
                            label,
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(kind, theme.muted))
                                .child(div().flex_1().child(label))
                                .child(if active {
                                    icon(Icon::Check, theme.ink).into_any_element()
                                } else {
                                    div().size(rems(1.125)).into_any_element()
                                })
                                .into_any_element(),
                            active,
                            cx,
                            move |this, window, _| {
                                if tool == InkTool::Highlighter
                                    && this.controller.style.tool != InkTool::Highlighter
                                {
                                    this.writing_style = Some(this.controller.style.clone());
                                }
                                this.controller.set_ink_tool(tool);
                                this.dismiss_popovers();
                                this.focus.focus(window);
                            },
                        )
                        .w_full()
                        .justify_start()
                        .min_h_0()
                        .py_1()
                        .px_2(),
                    );
                }
                let presets = quick_presets(
                    &self.controller.settings.pen_presets,
                    &self.controller.style,
                );
                if !presets.is_empty() {
                    panel = panel.child(
                        div()
                            .px_2()
                            .pt_3()
                            .pb_1()
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .child("Saved presets"),
                    );
                    for preset in presets {
                        let active = preset.style == self.controller.style;
                        panel = panel.child(self.compact_preset_row(&preset, active, true, cx));
                    }
                }
                panel = panel.child(div().h(px(1.)).my_2().bg(theme.border));
                for (id, label, section, kind) in [
                    (
                        "pen-menu-settings",
                        "Pen settings…",
                        Section::Settings,
                        Icon::Settings,
                    ),
                    (
                        "pen-menu-presets",
                        "Manage presets…",
                        Section::Presets,
                        Icon::More,
                    ),
                ] {
                    panel = panel.child(
                        self.control(
                            id,
                            label,
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(kind, theme.muted))
                                .child(div().flex_1().child(label))
                                .child(icon(Icon::Forward, theme.muted))
                                .into_any_element(),
                            false,
                            cx,
                            move |this, _, _| this.pen_menu_section = section,
                        )
                        .w_full()
                        .justify_start()
                        .min_h_0()
                        .py_1()
                        .px_2(),
                    );
                }
            }
            Section::Settings => {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_2()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Pen settings"),
                );
                for (label, kind, value) in [
                    ("Width", 0, format!("{:.1} px", self.controller.style.width)),
                    (
                        "Opacity",
                        1,
                        format!("{:.0}%", self.controller.style.opacity * 100.),
                    ),
                    (
                        "Stabilization",
                        2,
                        format!("{:.0}%", self.controller.style.stabilization * 100.),
                    ),
                    (
                        "Pressure response",
                        3,
                        format!("{:.1}", self.controller.style.pressure_gamma),
                    ),
                ] {
                    panel = panel.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_2()
                            .text_sm()
                            .child(label)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        self.button(
                                            format!("minus-{kind}"),
                                            "−",
                                            false,
                                            cx,
                                            move |this, _, _| adjust_pen(this, kind, -1.),
                                        )
                                        .min_h_0()
                                        .px_2()
                                        .py_1(),
                                    )
                                    .child(div().min_w(rems(3.)).text_center().child(value))
                                    .child(
                                        self.button(
                                            format!("plus-{kind}"),
                                            "+",
                                            false,
                                            cx,
                                            move |this, _, _| adjust_pen(this, kind, 1.),
                                        )
                                        .min_h_0()
                                        .px_2()
                                        .py_1(),
                                    ),
                            ),
                    );
                }
                panel = panel.child(
                    self.control(
                        "pen-custom-color",
                        "Custom color…",
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div().size(rems(1.)).rounded_full().bg(rgb(self
                                    .controller
                                    .style
                                    .color
                                    .rgb())),
                            )
                            .child("Custom color…")
                            .into_any_element(),
                        false,
                        cx,
                        |this, w, cx| this.modal(Modal::Color, w, cx),
                    )
                    .w_full()
                    .justify_start(),
                );
                if !self.controller.settings.recent_colors.is_empty() {
                    let mut recent = div().flex().flex_wrap().gap_1();
                    for color in self.controller.settings.recent_colors.clone() {
                        recent = recent.child(
                            self.control(
                                format!("recent-{}", color.rgb()),
                                format!("Use recent ink color {}", color.hex()),
                                div()
                                    .size(rems(1.))
                                    .rounded_full()
                                    .bg(rgb(color.rgb()))
                                    .into_any_element(),
                                self.controller.style.color == color,
                                cx,
                                move |this, _, _| this.controller.set_color(color),
                            )
                            .size(rems(2.))
                            .min_h_0()
                            .p_1(),
                        );
                    }
                    panel = panel
                        .child(
                            div()
                                .px_2()
                                .pt_2()
                                .text_xs()
                                .text_color(rgb(theme.muted))
                                .child("Recent colors"),
                        )
                        .child(recent);
                }
            }
            Section::Presets => {
                panel = panel.child(
                    div()
                        .px_2()
                        .py_2()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Saved presets"),
                );
                panel = panel.child(self.preset_controls(cx)).child(
                    self.button(
                        "save-preset",
                        "Save current pen as preset…",
                        false,
                        cx,
                        |this, w, cx| this.modal(Modal::SavePreset, w, cx),
                    )
                    .w_full()
                    .justify_start(),
                );
            }
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn quick_choices_are_bounded_and_include_the_active_preset_without_reordering_saved_data() {
        let presets: Vec<_> = (0..12)
            .map(|i| folio_app::PenPreset {
                id: Id::new_v4(),
                name: format!("Preset {i}"),
                style: folio_document::PenStyle {
                    width: i as f32 + 1.,
                    ..Default::default()
                },
            })
            .collect();
        let original_ids: Vec<_> = presets.iter().map(|p| p.id).collect();
        let visible = quick_presets(&presets, &presets[10].style);
        assert_eq!(visible.len(), 3);
        assert!(visible.iter().any(|p| p.id == presets[10].id));
        assert_eq!(
            presets.iter().map(|p| p.id).collect::<Vec<_>>(),
            original_ids
        );
        assert_eq!(quick_presets(&presets[..2], &presets[1].style).len(), 2);
        assert!(quick_presets(&[], &presets[0].style).is_empty());
    }
}
