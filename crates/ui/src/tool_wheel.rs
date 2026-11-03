//! A radial tool picker: press W (or a tablet-pad button set to `wheel`),
//! then move the pointer or hovering pen onto a tool. Holding W and
//! releasing it over a tool, clicking a tool, or Enter on a focused tool
//! also chooses it; Escape, W or a press elsewhere closes the wheel.
use super::*;

/// Outer radius of the ring of tools, at 1× interface scale.
const RADIUS: f32 = 92.;
/// Diameter of one tool button.
const SLOT: f32 = 52.;
/// Movement within this radius of the center chooses nothing.
const DEAD_ZONE: f32 = 26.;

#[derive(Clone, Copy, Debug)]
pub(super) struct ToolWheel {
    /// Center in window coordinates.
    center: Point<Pixels>,
    /// The tool slot the pointer points at, before it reaches the ring.
    aimed: Option<usize>,
    /// Pointer movement may choose a tool. A wheel moved away from the
    /// pointer, as at a window edge, waits until the pointer reaches its
    /// center so the opening position does not choose a tool.
    armed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WheelTool {
    Pen,
    Eraser,
    Select,
    Shapes,
    Text,
    Pan,
}
impl WheelTool {
    /// Clockwise from the top.
    pub(super) const ALL: [Self; 6] = [
        Self::Pen,
        Self::Eraser,
        Self::Select,
        Self::Pan,
        Self::Text,
        Self::Shapes,
    ];
    fn key(self) -> &'static str {
        match self {
            Self::Pen => "wheel-pen",
            Self::Eraser => "wheel-eraser",
            Self::Select => "wheel-select",
            Self::Shapes => "wheel-shapes",
            Self::Text => "wheel-text",
            Self::Pan => "wheel-pan",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Pen => "Pen",
            Self::Eraser => "Eraser",
            Self::Select => "Select",
            Self::Shapes => "Shapes",
            Self::Text => "Text",
            Self::Pan => "Pan",
        }
    }
    fn tool(self, selection: Tool) -> Tool {
        match self {
            Self::Pen => Tool::Pen,
            Self::Eraser => Tool::Eraser,
            Self::Select => selection,
            Self::Shapes => Tool::Shape,
            Self::Text => Tool::Text,
            Self::Pan => Tool::Hand,
        }
    }
}

/// The slot at `offset` from the wheel's center, outside the dead zone, and
/// the distance in unscaled pixels.
pub(super) fn aim(offset: (f32, f32), scale: f32) -> Option<(usize, f32)> {
    let (x, y) = offset;
    let distance = x.hypot(y);
    if distance < DEAD_ZONE * scale {
        return None;
    }
    let count = WheelTool::ALL.len() as f32;
    // Clockwise angle from the top; each slot owns the sector around it.
    let angle = x.atan2(-y).rem_euclid(std::f32::consts::TAU);
    let slot = (angle / std::f32::consts::TAU * count).round() as usize % WheelTool::ALL.len();
    Some((slot, distance / scale))
}
/// The pointer has reached the ring of tool buttons.
fn reached(distance: f32) -> bool {
    distance >= RADIUS - SLOT
}

/// Where the wheel is drawn for a pointer at `at`: there, unless that would
/// put part of the wheel outside the window.
fn placement(at: Point<Pixels>, window: Size<Pixels>, scale: f32) -> Point<Pixels> {
    let reach = RADIUS * scale + 8.;
    let clamp = |value: Pixels, size: Pixels| {
        let value = f32::from(value);
        let size = f32::from(size);
        px(value.clamp(reach.min(size / 2.), (size - reach).max(size / 2.)))
    };
    point(clamp(at.x, window.width), clamp(at.y, window.height))
}

impl NotesView {
    pub(super) fn toggle_tool_wheel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool_wheel.take().is_some() {
            cx.notify();
            return;
        }
        if self.library_open || self.blocking_overlay() || self.controller.loading_note() {
            return;
        }
        self.finish_inline_text(window, cx);
        self.controller.finish();
        self.dismiss_popovers();
        let scale = self.controller.settings.ui_scale;
        let pointer = window.mouse_position();
        let inside = self.canvas_bounds.is_some_and(|b| b.contains(&pointer));
        let at = if inside {
            pointer
        } else {
            self.canvas_bounds.map_or(pointer, |b| b.center())
        };
        let center = placement(at, window.viewport_size(), scale);
        self.tool_wheel = Some(ToolWheel {
            center,
            aimed: None,
            armed: inside && center == pointer,
        });
        let current = WheelTool::ALL
            .into_iter()
            .find(|slot| slot.tool(self.selection_tool) == self.controller.tool)
            .unwrap_or(WheelTool::Pen);
        self.focus.focus(window);
        self.menu_focus = Some(current.key().into());
        cx.notify();
    }
    /// Choose a tool from the wheel through the toolbar's switching policy.
    fn choose_wheel_tool(&mut self, slot: WheelTool, window: &mut Window, cx: &mut Context<Self>) {
        self.tool_wheel = None;
        let tool = slot.tool(self.selection_tool);
        if !self.controller.read_only()
            || matches!(tool, Tool::Hand | Tool::Lasso | Tool::Rectangle)
        {
            self.select_tool(tool);
        }
        self.focus.focus(window);
        cx.notify();
    }
    /// Track the pointer or hovering pen: reaching a tool chooses it.
    fn aim_tool_wheel(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let scale = self.controller.settings.ui_scale;
        let Some(wheel) = &mut self.tool_wheel else {
            return;
        };
        let offset = (
            f32::from(at.x - wheel.center.x),
            f32::from(at.y - wheel.center.y),
        );
        let aimed = aim(offset, scale);
        if !wheel.armed {
            wheel.armed = aimed.is_none();
            return;
        }
        let previous = wheel.aimed;
        wheel.aimed = aimed.map(|(slot, _)| slot);
        let changed = previous != wheel.aimed;
        if let Some((slot, distance)) = aimed
            && reached(distance)
            && self.wheel_slot_enabled(WheelTool::ALL[slot])
        {
            self.choose_wheel_tool(WheelTool::ALL[slot], window, cx);
        } else if changed {
            cx.notify();
        }
    }
    /// Releasing W chooses the tool it was aimed at; otherwise the wheel
    /// stays open for a click.
    pub(super) fn release_tool_wheel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(slot) = self.tool_wheel.and_then(|wheel| wheel.aimed)
            && self.wheel_slot_enabled(WheelTool::ALL[slot])
        {
            self.choose_wheel_tool(WheelTool::ALL[slot], window, cx);
        }
    }
    fn wheel_slot_enabled(&self, slot: WheelTool) -> bool {
        !self.controller.read_only() || matches!(slot, WheelTool::Select | WheelTool::Pan)
    }
    pub(super) fn tool_wheel_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Some(wheel) = self.tool_wheel else {
            return div().into_any_element();
        };
        let theme = Theme::new(&self.controller.settings);
        let scale = self.controller.settings.ui_scale;
        let (radius, slot_size) = (RADIUS * scale, SLOT * scale);
        let ring = radius - slot_size / 2.;
        let pen_icon = match self.controller.style.tool {
            InkTool::Pencil => Icon::Pencil,
            InkTool::Marker => Icon::Marker,
            InkTool::Highlighter => Icon::Highlighter,
            _ => Icon::Pen,
        };
        let select_icon = if self.selection_tool == Tool::Rectangle {
            Icon::Rectangle
        } else {
            Icon::Lasso
        };
        let mut disc = div()
            .absolute()
            .left(wheel.center.x - px(radius))
            .top(wheel.center.y - px(radius))
            .size(px(2. * radius))
            .rounded_full()
            .glass(theme, theme.popover);
        let aimed = wheel.aimed.map(|slot| WheelTool::ALL[slot]);
        for (index, slot) in WheelTool::ALL.into_iter().enumerate() {
            let angle = index as f32 / WheelTool::ALL.len() as f32 * std::f32::consts::TAU;
            let (x, y) = (radius + ring * angle.sin(), radius - ring * angle.cos());
            let kind = match slot {
                WheelTool::Pen => pen_icon,
                WheelTool::Eraser => Icon::Eraser,
                WheelTool::Select => select_icon,
                WheelTool::Shapes => Icon::Shapes,
                WheelTool::Text => Icon::Text,
                WheelTool::Pan => Icon::Hand,
            };
            let current = slot.tool(self.selection_tool) == self.controller.tool;
            let enabled = self.wheel_slot_enabled(slot);
            let button = self
                .control(
                    slot.key(),
                    slot.label(),
                    icon(kind, theme.ink)
                        .size(px(22. * scale))
                        .into_any_element(),
                    current,
                    cx,
                    move |this, window, cx| {
                        if this.wheel_slot_enabled(slot) {
                            this.choose_wheel_tool(slot, window, cx)
                        }
                    },
                )
                .absolute()
                .left(px(x - slot_size / 2.))
                .top(px(y - slot_size / 2.))
                .size(px(slot_size))
                .p_0()
                .rounded_full()
                .when(!enabled, |button| button.opacity(0.4))
                .when(aimed == Some(slot), |button| button.bg(theme.tint(2.)))
                .hover(move |s| s.bg(theme.tint(2.)));
            disc = disc.child(button);
        }
        let caption = aimed.map_or("Choose a tool", |slot| slot.label());
        disc = disc.child(
            div()
                .absolute()
                .left(px(radius - 40. * scale))
                .top(px(radius - 12. * scale))
                .w(px(80. * scale))
                .h(px(24. * scale))
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(rgb(theme.muted))
                .child(caption),
        );
        let _ = window;
        div()
            .id("tool-wheel")
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.aim_tool_wheel(event.position, window, cx);
            }))
            // A press on a tool chooses it; a press elsewhere closes the
            // wheel. Neither reaches the canvas, so no ink is drawn.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let offset = (
                        f32::from(event.position.x - wheel.center.x),
                        f32::from(event.position.y - wheel.center.y),
                    );
                    match aim(offset, scale) {
                        Some((slot, distance))
                            if reached(distance)
                                && distance <= RADIUS
                                && this.wheel_slot_enabled(WheelTool::ALL[slot]) =>
                        {
                            this.choose_wheel_tool(WheelTool::ALL[slot], window, cx)
                        }
                        _ => {
                            this.tool_wheel = None;
                            cx.notify();
                        }
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.tool_wheel = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(disc)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn pointer_direction_aims_at_the_tool_in_that_direction() {
        // Within the dead zone nothing is aimed at.
        assert_eq!(aim((10., -10.), 1.), None);
        // Straight up is the first tool; the ring is reached further out.
        assert_eq!(aim((0., -30.), 1.), Some((0, 30.)));
        assert!(!reached(30.) && reached(60.));
        // Clockwise: right of the top is the second tool, left the last.
        assert_eq!(aim((60., -35.), 1.).unwrap().0, 1);
        assert_eq!(aim((-60., -35.), 1.).unwrap().0, 5);
        assert_eq!(aim((0., 60.), 1.).unwrap().0, 3);
        // Distances follow the interface scale.
        assert_eq!(aim((0., -30.), 2.), None);
        assert_eq!(WheelTool::ALL[3], WheelTool::Pan);
    }
    #[test]
    fn wheel_stays_inside_the_window() {
        let window = size(px(800.), px(600.));
        let center = placement(point(px(5.), px(590.)), window, 1.);
        assert_eq!(center, point(px(100.), px(500.)));
        assert_eq!(
            placement(point(px(400.), px(300.)), window, 1.),
            point(px(400.), px(300.))
        );
        // A window smaller than the wheel centers it.
        let tiny = size(px(120.), px(120.));
        assert_eq!(
            placement(point(px(0.), px(0.)), tiny, 1.),
            point(px(60.), px(60.))
        );
    }
}
