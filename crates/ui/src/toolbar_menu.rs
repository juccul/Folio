//! Compact editor dropdowns, anchored to their toolbar controls in window coordinates.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolMenu {
    Selection,
    Insert,
    Width,
    Color,
}
/// Ink colors offered before recent and custom colors.
const INK_SWATCHES: [u32; 8] = [
    0x171717, 0x5f6368, 0x1f5fbf, 0x2e7d32, 0xc62828, 0xef6c00, 0x6a1b9a, 0x00838f,
];
impl ToolMenu {
    fn anchor(self) -> &'static str {
        match self {
            Self::Selection => "select-options",
            Self::Insert => "insert-options",
            Self::Width => "width-options",
            Self::Color => "custom-color",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Selection => "Selection tools",
            Self::Insert => "Insert",
            Self::Width => "Stroke width",
            Self::Color => "Ink color",
        }
    }
}

fn width_presets(tool: InkTool) -> [f32; 3] {
    if tool == InkTool::Highlighter {
        [10., 20., 30.]
    } else {
        [1.5, 3., 6.]
    }
}

fn panel_position(x: f32, bottom: f32, viewport: (f32, f32), scale: f32) -> (f32, f32, f32, f32) {
    let margin = 8.;
    let width = (240. * scale).min((viewport.0 - 2. * margin).max(1.));
    let x = x.clamp(margin, (viewport.0 - width - margin).max(margin));
    let y = (bottom + 4.).min((viewport.1 - margin - 1.).max(margin));
    let height = (viewport.1 - y - margin).max(1.);
    (x, y, width, height)
}

impl NotesView {
    pub(super) fn toggle_tool_menu(
        &mut self,
        menu: ToolMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closing = self.tool_menu.is_some_and(|(open, _)| open == menu);
        self.controller.finish();
        self.dismiss_popovers();
        self.document_menu = None;
        if !closing {
            self.tool_menu = Some((menu, window.mouse_position()));
        }
        self.focus.focus(window);
        cx.notify();
    }

    pub(super) fn tool_menu_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Some((kind, at)) = self.tool_menu else {
            return div().into_any_element();
        };
        let theme = Theme::new(&self.controller.settings);
        let anchor = self.accessibility.control_bounds_for_key(kind.anchor());
        let toolbar_bottom = anchor.map_or(f32::from(at.y) + 18., |r| r.y1 as f32);
        let size = window.viewport_size();
        let (x, y, width, height) = panel_position(
            anchor.map_or(f32::from(at.x), |r| r.x0 as f32),
            toolbar_bottom,
            (f32::from(size.width), f32::from(size.height)),
            self.controller.settings.ui_scale,
        );
        // A dialog chosen here turns the dropdown into its page, wider and
        // as tall as the window leaves below it.
        let hosted = self.modal_hosted_in(MenuHost::Tool);
        let (x, width) = if hosted {
            let wide = width.max(300_f32.min(f32::from(size.width) - 16.));
            (x.min((f32::from(size.width) - wide - 8.).max(8.)), wide)
        } else {
            (x, width)
        };
        let panel = div()
            .id("toolbar-dropdown")
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .grows_from(
                anchor.map_or((f32::from(at.x) - x) / width, |r| {
                    ((r.x0 + r.x1) as f32 / 2. - x) / width
                }),
                0.,
            )
            .max_h(px(if hosted { f32::from(size.height) - y - 8. } else { height }))
            .overflow_y_scroll()
            .occlude()
            .rounded(px(theme.corner_md()))
            .glass(theme, theme.popover)
            .flex()
            .flex_col()
            // It springs to the size of the page it turns into and back.
            .el(|e| e.motion(crate::motion::MORPH).animated_changes())
            // The menu is modal: a press outside it, its own button included,
            // only closes it, so it never reaches the page as a stroke.
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, w, cx| {
                if this.modal_hosted_in(MenuHost::Tool) {
                    this.cancel_modal(w, cx);
                }
                this.tool_menu = None;
                this.focus.focus(w);
                cx.notify();
            }));
        if hosted {
            return panel
                .p_1p5()
                .child(self.modal_panel(true, window, cx))
                .into_any_element();
        }
        // Read as the other menus do: a muted heading, then compact rows of
        // an icon and a label, the current choice checked.
        let mut panel = panel.p_1p5().gap(px(1.)).child(
            document_actions::heading(theme, kind.title())
                .flex()
                .items_center()
                .justify_between()
                // L returns to the last chosen selection mode.
                .when(kind == ToolMenu::Selection, |title| {
                    title.child(super::keycap("L", theme))
                }),
        );
        let check = move |on: bool| {
            on.then(|| icon(Icon::Check, theme.ink).size(rems(1.)).into_any_element())
        };
        match kind {
            ToolMenu::Selection => {
                for (id, label, detail, kind, tool) in [
                    ("lasso", "Lasso", "Circle what to select", Icon::Lasso, Tool::Lasso),
                    ("select-rect", "Rectangle", "Drag a box to select", Icon::Rectangle, Tool::Rectangle),
                ] {
                    // The mode the selection button uses is checked, even
                    // while another tool is in use.
                    let chosen = self.selection_tool == tool;
                    panel = panel.child(self.menu_row(
                        id,
                        format!("{label} selection · {detail}").into(),
                        div().child(label),
                        kind,
                        theme.muted,
                        check(chosen),
                        cx,
                        move |this, w, _| {
                            this.focus.focus(w);
                            this.selection_tool = tool;
                            this.region_selection = None;
                            this.controller.set_tool(tool);
                            this.dismiss_popovers();
                        },
                    ));
                }
            }
            ToolMenu::Insert => {
                panel = panel
                    .child(self.menu_row(
                        "shape",
                        "Shapes · S".into(),
                        div().child("Shapes"),
                        Icon::Shapes,
                        theme.muted,
                        Some(super::keycap("S", theme).text_xs().into_any_element()),
                        cx,
                        |this, w, _| {
                            this.focus.focus(w);
                            this.select_tool(Tool::Shape);
                        },
                    ))
                    .child(self.menu_row(
                        "toolbar-image",
                        "Insert image or PDF…".into(),
                        div().child("Image or PDF…"),
                        Icon::Image,
                        theme.muted,
                        None,
                        cx,
                        |this, _, cx| {
                            this.dismiss_popovers();
                            this.import(cx);
                        },
                    ))
                    .child(self.menu_row(
                        "insert-equation",
                        "Insert LaTeX equation…".into(),
                        div().child("LaTeX equation…"),
                        Icon::Math,
                        theme.muted,
                        None,
                        cx,
                        |this, w, cx| {
                            this.modal_in_menu(MenuHost::Tool, Modal::Equation, w, cx);
                        },
                    ));
            }
            ToolMenu::Color => {
                let mut swatches = div().flex().flex_wrap().gap_1().px_1().pb_1();
                let mut colors: Vec<Color> = INK_SWATCHES.map(Color::from_rgb).to_vec();
                for recent in &self.controller.settings.recent_colors {
                    if !colors.contains(recent) {
                        colors.push(*recent);
                    }
                }
                for color in colors {
                    let selected = self.controller.style.color == color;
                    swatches = swatches.child(
                        self.control(
                            format!("ink-swatch-{}", color.rgb()),
                            format!("Ink color {}", color.hex()),
                            // The ring marks the selection; the background
                            // tint alone is shared with hover.
                            div()
                                .size(rems(1.25))
                                .rounded_full()
                                .when(selected, |dot| dot.border_2())
                                .when(!selected, |dot| dot.border_1())
                                .border_color(rgb(if selected { theme.ring } else { theme.muted }))
                                .bg(rgb(color.rgb()))
                                .into_any_element(),
                            selected,
                            cx,
                            move |this, w, cx| {
                                this.controller.set_color(color);
                                this.tool_menu = None;
                                this.focus.focus(w);
                                cx.notify();
                            },
                        )
                        .size(rems(2.25))
                        .min_h_0()
                        .p_1(),
                    );
                }
                panel = panel
                    .child(swatches)
                    .child(document_actions::separator(theme))
                    .child(self.menu_row(
                        "ink-custom-color",
                        "Custom color…".into(),
                        div().child("Custom color…"),
                        Icon::Palette,
                        theme.muted,
                        None,
                        cx,
                        |this, w, cx| {
                            this.modal_in_menu(MenuHost::Tool, Modal::Color, w, cx);
                        },
                    ));
            }
            ToolMenu::Width => {
                let current = self.controller.style.width;
                let presets = width_presets(self.controller.style.tool);
                let thickest = presets.iter().copied().fold(1_f32, f32::max);
                for (i, width) in presets.into_iter().enumerate() {
                    let chosen = (current - width).abs() < 0.1;
                    let label = format!("{width:.1} px");
                    // Each width shows as a stroke of its own weight, in
                    // the icon column.
                    let sample = div()
                        .w(rems(1.125))
                        .h(rems(1.125))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w_full()
                                .h(px((width / thickest * 7.).clamp(1., 7.)))
                                .rounded_full()
                                .bg(rgb(theme.muted)),
                        );
                    panel = panel.child(
                        self.control(
                            format!("width-{i}"),
                            label.clone(),
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(sample)
                                .child(div().flex_1().min_w_0().child(label))
                                .children(check(chosen))
                                .into_any_element(),
                            false,
                            cx,
                            move |this, _, _| {
                                let mut style = this.controller.style.clone();
                                style.width = width;
                                this.controller.set_style(style);
                            },
                        )
                        .w_full()
                        .justify_start()
                        .min_h(rems(2.))
                        .px_2()
                        .py_1(),
                    );
                }
                panel = panel.child(document_actions::separator(theme)).child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_1()
                        .child(self.pen_stepper(pen_menu::PenSetting::Width, false, cx).px_2())
                        .child(
                            div()
                                .text_sm()
                                .child(pen_menu::PenSetting::Width.value(&self.controller.style)),
                        )
                        .child(self.pen_stepper(pen_menu::PenSetting::Width, true, cx).px_2()),
                );
            }
        }
        panel.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn toolbar_popup_stays_inside_narrow_and_scaled_viewports() {
        for (width, height, scale) in [
            (1320., 780., 1.),
            (980., 680., 1.),
            (500., 500., 1.5),
            (220., 300., 2.),
        ] {
            let (x, y, w, h) = panel_position(width - 40., 115., (width, height), scale);
            assert!(x >= 8. && y == 119.);
            assert!(x + w <= width - 8. && y + h <= height - 8.);
            assert!(w > 0. && h > 0.);
        }
    }
}
