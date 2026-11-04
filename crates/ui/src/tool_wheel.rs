//! A radial tool picker: press W, the pen's lower barrel button or a
//! tablet-pad button set to `wheel`. Pointing at a tool highlights it;
//! clicking or tapping it, or Enter on a focused tool, chooses it. Escape,
//! W, the barrel button or a press elsewhere closes the wheel.
use super::*;
use std::f32::consts::{PI, TAU};

/// Outer radius of the wheel, at 1× interface scale.
const RADIUS: f32 = 100.;
/// The ring of tool segments lies between these radii.
const RING_OUTER: f32 = 94.;
const RING_INNER: f32 = 40.;
/// Radius of the hub that names the aimed tool.
const HUB: f32 = 33.;
/// Width of the clear gap between neighboring segments.
const GAP: f32 = 4.;
/// Movement within this radius of the center chooses nothing.
const DEAD_ZONE: f32 = 26.;
/// The `TabletEvent::buttons` bit of the pen's lower barrel button
/// (BTN_STYLUS on Wayland, button 2 on X11). The upper button erases.
const WHEEL_PEN_BUTTON: u32 = 1;

#[derive(Clone, Copy, Debug)]
pub(super) struct ToolWheel {
    /// Center in window coordinates.
    center: Point<Pixels>,
    /// The tool slot the pointer points at, before it reaches the ring.
    aimed: Option<usize>,
    /// The pointer's clockwise angle from the top while it aims.
    pointing: Option<f32>,
    /// Arrow keys or Tab moved focus, so the focused tool is highlighted.
    keyboard: bool,
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
    /// Every tool the wheel can show, in the default order.
    pub(super) const ALL: [Self; 6] = [
        Self::Pen,
        Self::Eraser,
        Self::Select,
        Self::Pan,
        Self::Text,
        Self::Shapes,
    ];
    /// The name saved in [`folio_app::Settings::wheel_tools`].
    pub(super) fn slug(self) -> &'static str {
        match self {
            Self::Pen => "pen",
            Self::Eraser => "eraser",
            Self::Select => "select",
            Self::Shapes => "shapes",
            Self::Text => "text",
            Self::Pan => "pan",
        }
    }
    /// The saved tools, falling back to the defaults if fewer than two are
    /// known.
    pub(super) fn from_slugs(slugs: &[String]) -> Vec<Self> {
        let mut tools: Vec<Self> = Vec::new();
        for slug in slugs {
            if let Some(tool) = Self::ALL.into_iter().find(|tool| tool.slug() == slug)
                && !tools.contains(&tool)
            {
                tools.push(tool);
            }
        }
        if tools.len() < folio_app::MIN_WHEEL_TOOLS {
            return Self::from_slugs(&folio_app::Settings::default().wheel_tools);
        }
        tools
    }
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
    pub(super) fn label(self) -> &'static str {
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

/// The slot of `count` at `offset` from the wheel's center, outside the dead
/// zone, and the distance in unscaled pixels.
pub(super) fn aim(offset: (f32, f32), scale: f32, count: usize) -> Option<(usize, f32)> {
    let (x, y) = offset;
    let distance = x.hypot(y);
    if distance < DEAD_ZONE * scale || count == 0 {
        return None;
    }
    let slot = (pointing(offset) / TAU * count as f32).round() as usize % count;
    Some((slot, distance / scale))
}
/// Clockwise angle from the top of slot `index` of `count`.
fn slot_angle(index: usize, count: usize) -> f32 {
    index as f32 / count as f32 * TAU
}
/// Clockwise angle of `offset` from the top; each slot owns the sector
/// around its angle.
fn pointing((x, y): (f32, f32)) -> f32 {
    x.atan2(-y).rem_euclid(TAU)
}
/// The pointer has reached the ring of tool segments.
fn reached(distance: f32) -> bool {
    distance >= RING_INNER
}

/// A change of the wheel's barrel button between two tablet frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Barrel {
    Pressed,
    Released,
}
pub(super) fn barrel(previous: u32, buttons: u32) -> Option<Barrel> {
    match (
        previous & WHEEL_PEN_BUTTON != 0,
        buttons & WHEEL_PEN_BUTTON != 0,
    ) {
        (false, true) => Some(Barrel::Pressed),
        (true, false) => Some(Barrel::Released),
        _ => None,
    }
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

/// The point `radius` from `center` at clockwise `angle` from the top.
fn polar(center: Point<Pixels>, radius: f32, angle: f32) -> Point<Pixels> {
    point(
        center.x + px(radius * angle.sin()),
        center.y - px(radius * angle.cos()),
    )
}
/// The ring segment around `angle`, one of `count`, between two radii and
/// separated from its neighbors by an even `gap`.
fn segment_path(
    center: Point<Pixels>,
    (inner, outer): (f32, f32),
    angle: f32,
    gap: f32,
    count: usize,
) -> Option<Path<Pixels>> {
    let half = PI / count as f32;
    // Wider segments need more steps to stay round.
    let steps = (24 / count).max(12);
    let span = |radius: f32| half - (gap / 2. / radius).min(1.).asin();
    let mut b = PathBuilder::fill();
    let outer_span = span(outer);
    b.move_to(polar(center, outer, angle - outer_span));
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        b.line_to(polar(center, outer, angle + outer_span * (2. * t - 1.)));
    }
    let inner_span = span(inner);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        b.line_to(polar(center, inner, angle + inner_span * (1. - 2. * t)));
    }
    b.close();
    b.build().ok()
}

/// How one segment is painted.
struct Segment {
    base: Rgba,
    /// The aim or hover highlight drawn over the base.
    highlight: Rgba,
    /// The active tool's mark along the outer edge.
    mark: Option<Rgba>,
}

impl NotesView {
    /// The wheel's tools, clockwise from the top.
    pub(super) fn wheel_tools(&self) -> Vec<WheelTool> {
        WheelTool::from_slugs(&self.controller.settings.wheel_tools)
    }
    pub(super) fn toggle_tool_wheel(
        &mut self,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            pointing: None,
            keyboard: false,
        });
        let tools = self.wheel_tools();
        let current = tools
            .iter()
            .copied()
            .find(|slot| slot.tool(self.selection_tool) == self.controller.tool)
            .unwrap_or(tools[0]);
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
    /// Track the pointer or hovering pen to highlight the tool it points
    /// at; only a press chooses.
    fn aim_tool_wheel(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let scale = self.controller.settings.ui_scale;
        let count = self.wheel_tools().len();
        let Some(wheel) = &mut self.tool_wheel else {
            return;
        };
        let offset = (
            f32::from(at.x - wheel.center.x),
            f32::from(at.y - wheel.center.y),
        );
        let aimed = aim(offset, scale, count);
        let previous = (wheel.aimed, wheel.pointing);
        wheel.aimed = aimed.map(|(slot, _)| slot);
        wheel.pointing = aimed.map(|_| pointing(offset));
        if wheel.aimed.is_some() {
            wheel.keyboard = false;
        }
        if previous != (wheel.aimed, wheel.pointing) {
            cx.notify();
        }
    }
    /// The pen's lower barrel button opens and closes the wheel at the pen,
    /// unless that is turned off in Settings. Returns whether the wheel used
    /// this tablet frame.
    pub(super) fn pen_tool_wheel(
        &mut self,
        event: &TabletEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let gone = matches!(event.phase, TabletPhase::Leave | TabletPhase::Cancel);
        let previous =
            std::mem::replace(&mut self.pen_buttons, if gone { 0 } else { event.buttons });
        if gone || !self.controller.settings.barrel_opens_wheel {
            return false;
        }
        match barrel(previous, event.buttons) {
            Some(Barrel::Pressed) => self.toggle_tool_wheel(event.position, window, cx),
            Some(Barrel::Released) => {}
            None => return false,
        }
        true
    }
    /// Smoke-test input through window dispatch: the hovering pen presses
    /// its lower barrel button; then (`aim`) hovers over Eraser, which must
    /// not choose it, and taps Pan.
    pub fn wheel_smoke_events(&self, aim: bool) -> Result<Vec<PlatformInput>, String> {
        let at = if aim {
            self.tool_wheel
                .ok_or("The pen's barrel button did not open the tool wheel")?
                .center
        } else {
            self.canvas_bounds.ok_or("Canvas not laid out")?.center()
        };
        let tablet = |position, buttons, phase| {
            PlatformInput::Tablet(TabletEvent {
                position,
                pressure: 0.,
                tilt_x: 0.,
                tilt_y: 0.,
                timestamp: 2000,
                buttons,
                eraser: false,
                phase,
                modifiers: Default::default(),
            })
        };
        let hover = |position, buttons| {
            [
                tablet(position, buttons, TabletPhase::Hover),
                PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: Default::default(),
                }),
            ]
        };
        let tools = self.wheel_tools();
        let ring = (RING_INNER + RING_OUTER) / 2. * self.controller.settings.ui_scale;
        let on_ring = |tool: WheelTool| {
            tools
                .iter()
                .position(|&slot| slot == tool)
                .map(|index| polar(at, ring, slot_angle(index, tools.len())))
                .ok_or(format!("{} is not on the tool wheel", tool.label()))
        };
        let pan = on_ring(WheelTool::Pan)?;
        let eraser = on_ring(WheelTool::Eraser)?;
        let mouse = |down: bool| {
            if down {
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: pan,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                })
            } else {
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: pan,
                    modifiers: Default::default(),
                    click_count: 1,
                })
            }
        };
        Ok(if aim {
            let mut events = [hover(at, 1), hover(at, 0), hover(eraser, 0), hover(pan, 0)].concat();
            events.extend([
                tablet(pan, 0, TabletPhase::Down),
                mouse(true),
                tablet(pan, 0, TabletPhase::Up),
                mouse(false),
            ]);
            // Leaving proximity lets the mouse drive the canvas again.
            events.push(tablet(pan, 0, TabletPhase::Leave));
            events
        } else {
            [hover(at, 0), hover(at, 1)].concat()
        })
    }
    pub fn wheel_smoke_verify(&mut self, tool: Tool, cx: &mut Context<Self>) -> Result<(), String> {
        let chosen = self.tool_wheel.is_none() && self.controller.tool == Tool::Hand;
        self.tool_wheel = None;
        self.controller.set_tool(tool);
        cx.notify();
        if chosen {
            Ok(())
        } else {
            Err("Tapping Pan on the tool wheel did not choose it alone".into())
        }
    }
    fn wheel_slot_enabled(&self, slot: WheelTool) -> bool {
        !self.controller.read_only() || matches!(slot, WheelTool::Select | WheelTool::Pan)
    }
    /// The wheel, faded and grown into place by `appear` from 0 to 1.
    pub(super) fn tool_wheel_overlay(
        &mut self,
        appear: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Some(wheel) = self.tool_wheel else {
            return div().into_any_element();
        };
        let theme = Theme::new(&self.controller.settings);
        let scale = self.controller.settings.ui_scale;
        let size = scale * (0.92 + 0.08 * appear);
        let radius = RADIUS * size;
        let (inner, outer) = (RING_INNER * size, RING_OUTER * size);
        let ring = (inner + outer) / 2.;
        let slot_size = outer - inner - 2. * GAP * size;
        let hub = HUB * size;
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
        let tools = self.wheel_tools();
        let count = tools.len();
        let current = tools
            .iter()
            .copied()
            .find(|slot| slot.tool(self.selection_tool) == self.controller.tool);
        let hovers: Vec<f32> = tools
            .iter()
            .map(|slot| self.motion.borrow_mut().hover_value(slot.key()))
            .collect();
        // The pointer's aim, else the keyboard focus, else a hovered tool.
        let highlighted = wheel
            .aimed
            .and_then(|slot| tools.get(slot).copied())
            .or_else(|| {
                wheel.keyboard.then(|| {
                    tools
                        .iter()
                        .copied()
                        .find(|slot| self.accessibility.is_focused(slot.key(), window))
                })?
            })
            .or_else(|| {
                (0..count)
                    .filter(|&i| hovers[i] > 0.5)
                    .max_by(|&a, &b| hovers[a].total_cmp(&hovers[b]))
                    .map(|i| tools[i])
            });
        let accent = |alpha: f32| rgba((theme.accent << 8) | (alpha.clamp(0., 1.) * 255.) as u32);
        let mut segments = Vec::new();
        let mut buttons = Vec::new();
        for (index, slot) in tools.iter().copied().enumerate() {
            let angle = slot_angle(index, count);
            let enabled = self.wheel_slot_enabled(slot);
            let active = current == Some(slot);
            let lit = if highlighted == Some(slot) {
                1.
            } else {
                hovers[index]
            };
            segments.push(Segment {
                base: match (enabled, active) {
                    (true, true) => accent(0.2),
                    (true, false) => theme.tint(0.75),
                    (false, _) => theme.tint(0.25),
                },
                highlight: if enabled {
                    accent(lit)
                } else {
                    theme.tint(lit)
                },
                mark: active.then(|| {
                    if enabled && lit > 0.5 {
                        rgb(theme.primary_foreground)
                    } else {
                        rgb(theme.accent)
                    }
                }),
            });
            let kind = match slot {
                WheelTool::Pen => pen_icon,
                WheelTool::Eraser => Icon::Eraser,
                WheelTool::Select => select_icon,
                WheelTool::Shapes => Icon::Shapes,
                WheelTool::Text => Icon::Text,
                WheelTool::Pan => Icon::Hand,
            };
            let color = if !enabled {
                theme.muted
            } else if lit > 0.5 {
                theme.primary_foreground
            } else if active {
                theme.accent
            } else {
                theme.ink
            };
            let center = (radius + ring * angle.sin(), radius - ring * angle.cos());
            buttons.push(
                self.control(
                    slot.key(),
                    slot.label(),
                    icon(kind, color)
                        .size(px(22. * size))
                        .when(!enabled, |icon| icon.opacity(0.5))
                        .into_any_element(),
                    active,
                    cx,
                    move |this, window, cx| {
                        if this.wheel_slot_enabled(slot) {
                            this.choose_wheel_tool(slot, window, cx)
                        }
                    },
                )
                .absolute()
                .left(px(center.0 - slot_size / 2.))
                .top(px(center.1 - slot_size / 2.))
                .size(px(slot_size))
                .p_0()
                .rounded_full()
                // The painted segment shows hover, aim, focus and the
                // active tool.
                .bg(transparent_black())
                .border_0()
                .hover(|s| s)
                .focus(|s| s)
                .active(|s| s.opacity(0.85)),
            );
        }
        let read_only = self.controller.read_only();
        let (name, note) = match highlighted.or(current) {
            Some(slot) if !self.wheel_slot_enabled(slot) => (slot.label(), Some("Read only")),
            Some(slot) if highlighted.is_some() => (slot.label(), None),
            Some(slot) => (
                slot.label(),
                Some(if read_only { "Read only" } else { "Current" }),
            ),
            None => ("Tools", read_only.then_some("Read only")),
        };
        let pointer = wheel.pointing.zip(wheel.aimed).map(|(angle, slot)| {
            let enabled = tools
                .get(slot)
                .is_some_and(|&slot| self.wheel_slot_enabled(slot));
            (
                angle,
                if enabled {
                    rgb(theme.accent)
                } else {
                    rgb(theme.muted)
                },
            )
        });
        let gap = GAP * size;
        let painted = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let center = bounds.center();
                for (index, segment) in segments.iter().enumerate() {
                    let angle = slot_angle(index, count);
                    for color in [segment.base, segment.highlight] {
                        if color.a > 0.
                            && let Some(path) =
                                segment_path(center, (inner, outer), angle, gap, count)
                        {
                            window.paint_path(path, color);
                        }
                    }
                    if let Some(color) = segment.mark
                        && let Some(path) =
                            segment_path(center, (outer - 4. * size, outer), angle, gap, count)
                    {
                        window.paint_path(path, color);
                    }
                }
                // A notch on the hub points from the center toward the aim.
                if let Some((angle, color)) = pointer {
                    let mut b = PathBuilder::fill();
                    let spread = 0.2;
                    b.move_to(polar(center, hub + 7. * size, angle));
                    b.line_to(polar(center, hub + 1. * size, angle - spread));
                    b.line_to(polar(center, hub + 1. * size, angle + spread));
                    b.close();
                    if let Ok(path) = b.build() {
                        window.paint_path(path, color);
                    }
                }
            },
        )
        .absolute()
        .inset_0();
        let label = div()
            .absolute()
            .left(px(radius - hub))
            .top(px(radius - hub))
            .size(px(2. * hub))
            .rounded_full()
            .bg(theme.card())
            .border_1()
            .border_color(theme.glass_edge())
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.ink))
                    .child(name),
            )
            .children(note.map(|note| {
                div()
                    .text_xs()
                    .line_height(rems(0.875))
                    .text_color(rgb(theme.muted))
                    .child(note)
            }));
        // The glass disc's border would offset absolutely placed children,
        // so the disc is a sibling layer and everything shares one center.
        let disc = div()
            .absolute()
            .left(wheel.center.x - px(radius))
            .top(wheel.center.y - px(radius))
            .size(px(2. * radius))
            .opacity(appear)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .glass_dialog(theme, theme.popover),
            )
            .child(painted)
            .children(buttons)
            .child(label);
        div()
            .id("tool-wheel")
            .absolute()
            .inset_0()
            .occlude()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if let Some(wheel) = &mut this.tool_wheel
                    && matches!(
                        event.keystroke.key.as_str(),
                        "left" | "right" | "up" | "down" | "tab"
                    )
                {
                    wheel.keyboard = true;
                    wheel.aimed = None;
                    wheel.pointing = None;
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.aim_tool_wheel(event.position, cx);
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
                    let tools = this.wheel_tools();
                    match aim(offset, scale, tools.len()) {
                        Some((slot, distance))
                            if reached(distance)
                                && distance <= RADIUS
                                && this.wheel_slot_enabled(tools[slot]) =>
                        {
                            this.choose_wheel_tool(tools[slot], window, cx)
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
        assert_eq!(aim((10., -10.), 1., 4), None);
        // Straight up is the first tool; the ring is reached further out.
        assert_eq!(aim((0., -30.), 1., 4), Some((0, 30.)));
        assert!(!reached(30.) && reached(60.));
        // Clockwise sectors split evenly among however many tools there are.
        let slot = |offset, count| aim(offset, 1., count).unwrap().0;
        let right_up = (60., -35.);
        let left_up = (-60., -35.);
        assert_eq!([3, 4, 6].map(|count| slot(right_up, count)), [0, 1, 1]);
        assert_eq!([3, 4, 6].map(|count| slot(left_up, count)), [0, 3, 5]);
        assert_eq!([3, 4, 6].map(|count| slot((50., 30.), count)), [1, 1, 2]);
        assert_eq!([3, 4, 6].map(|count| slot((-60., 20.), count)), [2, 3, 4]);
        // Each tool's angle lies inside its own sector.
        for count in 2..=6 {
            for index in 0..count {
                let point = polar(point(px(0.), px(0.)), 60., slot_angle(index, count));
                let offset = (f32::from(point.x), f32::from(point.y));
                assert_eq!(slot(offset, count), index);
            }
        }
        // Distances follow the interface scale.
        assert_eq!(aim((0., -30.), 2., 4), None);
    }
    #[test]
    fn saved_wheel_tools_keep_their_order_and_default_to_four() {
        let slugs = |names: &[&str]| names.iter().map(|&name| name.into()).collect::<Vec<_>>();
        let defaults = WheelTool::from_slugs(&folio_app::Settings::default().wheel_tools);
        assert_eq!(
            defaults,
            [
                WheelTool::Pen,
                WheelTool::Eraser,
                WheelTool::Select,
                WheelTool::Pan
            ]
        );
        assert_eq!(
            WheelTool::from_slugs(&slugs(&["text", "typo", "pan", "text"])),
            [WheelTool::Text, WheelTool::Pan]
        );
        assert_eq!(WheelTool::from_slugs(&slugs(&["shapes"])), defaults);
        // Every wheel tool has a saved name the app crate knows.
        let names: Vec<_> = WheelTool::ALL.map(WheelTool::slug).into();
        assert_eq!(names, folio_app::WHEEL_TOOLS);
    }
    #[test]
    fn lower_barrel_button_opens_and_releases_the_wheel() {
        // Bit 1 is the lower barrel button; bit 2, the upper one, erases.
        assert_eq!(barrel(0, 1), Some(Barrel::Pressed));
        assert_eq!(barrel(2, 3), Some(Barrel::Pressed));
        assert_eq!(barrel(1, 1), None);
        assert_eq!(barrel(3, 2), Some(Barrel::Released));
        assert_eq!(barrel(0, 2), None);
        assert_eq!(barrel(2, 0), None);
    }
    #[test]
    fn wheel_stays_inside_the_window() {
        let window = size(px(800.), px(600.));
        let center = placement(point(px(5.), px(590.)), window, 1.);
        assert_eq!(center, point(px(108.), px(492.)));
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
