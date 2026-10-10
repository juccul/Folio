//! Compact editor dropdowns, anchored to their toolbar controls in window coordinates.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolMenu {
    Selection,
    Insert,
    Width,
}
impl ToolMenu {
    fn anchor(self) -> &'static str {
        match self {
            Self::Selection => "select-options",
            Self::Insert => "insert-options",
            Self::Width => "width-options",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Selection => "Selection tools",
            Self::Insert => "Insert",
            Self::Width => "Stroke width",
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
        let mut panel = div()
            .id("toolbar-dropdown")
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .max_h(px(height))
            .overflow_y_scroll()
            .occlude()
            .p_2()
            .rounded(px(theme.radius))
            .border_1()
            .border_color(theme.border)
            .bg(rgb(theme.popover))
            .shadow_sm()
            .flex()
            .flex_col()
            .gap_1()
            .on_mouse_down_out(cx.listener(move |this, event: &MouseDownEvent, w, cx| {
                // Let the anchor perform its own toggle; otherwise capture closes
                // the menu before the subsequent click and immediately reopens it.
                let own_anchor = this.accessibility.control_bounds_for_key(kind.anchor());
                let in_anchor = own_anchor.is_some_and(|r| {
                    let x = f32::from(event.position.x) as f64;
                    let y = f32::from(event.position.y) as f64;
                    x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1
                });
                if in_anchor {
                    return;
                }
                this.tool_menu = None;
                if f32::from(event.position.y) >= toolbar_bottom {
                    // Closing a dropdown on the page must not also begin a stroke.
                    cx.stop_propagation();
                }
                this.focus.focus(w);
                cx.notify();
            }))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(kind.title()),
            );
        match kind {
            ToolMenu::Selection => {
                for (id, label, tool) in [
                    ("lasso", "Lasso · L", Tool::Lasso),
                    ("select-rect", "Rectangular selection", Tool::Rectangle),
                ] {
                    panel = panel.child(
                        self.button(
                            id,
                            label,
                            self.controller.tool == tool,
                            cx,
                            move |this, w, _| {
                                this.focus.focus(w);
                                this.selection_tool = tool;
                                this.region_selection = None;
                                this.controller.set_tool(tool);
                                this.dismiss_popovers();
                            },
                        )
                        .w_full()
                        .justify_start(),
                    );
                }
            }
            ToolMenu::Insert => {
                panel = panel
                    .child(
                        self.control(
                            "shape",
                            "Shapes · S",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(Icon::Shapes, theme.muted))
                                .child("Shapes · S")
                                .into_any_element(),
                            self.controller.tool == Tool::Shape,
                            cx,
                            |this, w, _| {
                                this.focus.focus(w);
                                this.region_selection = None;
                                this.controller.set_tool(Tool::Shape);
                                this.dismiss_popovers();
                            },
                        )
                        .w_full()
                        .justify_start(),
                    )
                    .child(
                        self.control(
                            "toolbar-image",
                            "Insert image or PDF",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(Icon::Image, theme.muted))
                                .child("Insert image or PDF")
                                .into_any_element(),
                            false,
                            cx,
                            |this, _, cx| {
                                this.dismiss_popovers();
                                this.import(cx);
                            },
                        )
                        .w_full()
                        .justify_start(),
                    )
                    .child(
                        self.control(
                            "insert-equation",
                            "Insert LaTeX equation",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(icon(Icon::Math, theme.muted))
                                .child("Insert LaTeX equation")
                                .into_any_element(),
                            false,
                            cx,
                            |this, w, cx| {
                                this.modal(Modal::Equation, w, cx);
                            },
                        )
                        .w_full()
                        .justify_start(),
                    );
            }
            ToolMenu::Width => {
                for (i, width) in width_presets(self.controller.style.tool)
                    .into_iter()
                    .enumerate()
                {
                    panel = panel.child(
                        self.button(
                            format!("width-{i}"),
                            format!("{width:.1} px"),
                            (self.controller.style.width - width).abs() < 0.1,
                            cx,
                            move |this, _, _| {
                                let mut style = this.controller.style.clone();
                                style.width = width;
                                this.controller.set_style(style);
                            },
                        )
                        .w_full()
                        .justify_start(),
                    );
                }
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_2()
                        .child(
                            self.button("minus-0", "−", false, cx, |this, _, _| {
                                adjust_pen(this, 0, -1.)
                            })
                            .px_2(),
                        )
                        .child(
                            div()
                                .text_sm()
                                .child(format!("{:.1} px", self.controller.style.width)),
                        )
                        .child(
                            self.button("plus-0", "+", false, cx, |this, _, _| {
                                adjust_pen(this, 0, 1.)
                            })
                            .px_2(),
                        ),
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
