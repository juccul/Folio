//! A radial tool picker: press W, the pen's lower barrel button or a
//! tablet-pad button set to `wheel`, then move the pointer or hovering pen
//! onto a tool. Releasing W or the barrel button, or lifting the pen, while
//! aiming at a tool also chooses it, as do clicking or tapping a tool and
//! Enter on a focused tool. Escape, W, the barrel button or a press
//! elsewhere closes the wheel.
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
    /// Pointer movement may choose a tool. A wheel moved away from the
    /// pointer, as at a window edge, waits until the pointer reaches its
    /// center so the opening position does not choose a tool.
    armed: bool,
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
    let slot = (pointing(offset) / TAU * count).round() as usize % WheelTool::ALL.len();
    Some((slot, distance / scale))
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
/// The ring segment around `angle` between two radii, separated from its
/// neighbors by an even `gap`.
fn segment_path(
    center: Point<Pixels>,
    (inner, outer): (f32, f32),
    angle: f32,
    gap: f32,
) -> Option<Path<Pixels>> {
    const STEPS: usize = 12;
    let half = PI / WheelTool::ALL.len() as f32;
    let span = |radius: f32| half - (gap / 2. / radius).min(1.).asin();
    let mut b = PathBuilder::fill();
    let outer_span = span(outer);
    b.move_to(polar(center, outer, angle - outer_span));
    for step in 1..=STEPS {
        let t = step as f32 / STEPS as f32;
        b.line_to(polar(center, outer, angle + outer_span * (2. * t - 1.)));
    }
    let inner_span = span(inner);
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
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
            armed: inside && center == pointer,
            keyboard: false,
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
        let previous = (wheel.aimed, wheel.pointing);
        wheel.aimed = aimed.map(|(slot, _)| slot);
        wheel.pointing = aimed.map(|_| pointing(offset));
        if wheel.aimed.is_some() {
            wheel.keyboard = false;
        }
        let changed = previous != (wheel.aimed, wheel.pointing);
        if let Some((slot, distance)) = aimed
            && reached(distance)
            && self.wheel_slot_enabled(WheelTool::ALL[slot])
        {
            self.choose_wheel_tool(WheelTool::ALL[slot], window, cx);
        } else if changed {
            cx.notify();
        }
    }
    /// Releasing W or the barrel button chooses the tool it was aimed at;
    /// otherwise the wheel stays open for a click.
    pub(super) fn release_tool_wheel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(slot) = self.tool_wheel.and_then(|wheel| wheel.aimed)
            && self.wheel_slot_enabled(WheelTool::ALL[slot])
        {
            self.choose_wheel_tool(WheelTool::ALL[slot], window, cx);
        }
    }
    /// The pen's barrel button opens the wheel at the pen and, like W,
    /// chooses the aimed tool on release; lifting the pen tip while aiming
    /// also chooses. Returns whether the wheel used this tablet frame.
    pub(super) fn pen_tool_wheel(
        &mut self,
        event: &TabletEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let gone = matches!(event.phase, TabletPhase::Leave | TabletPhase::Cancel);
        let previous =
            std::mem::replace(&mut self.pen_buttons, if gone { 0 } else { event.buttons });
        if gone {
            return false;
        }
        match barrel(previous, event.buttons) {
            Some(Barrel::Pressed) => self.toggle_tool_wheel(event.position, window, cx),
            Some(Barrel::Released) => self.release_tool_wheel(window, cx),
            None if event.phase == TabletPhase::Up && self.tool_wheel.is_some() => {
                self.release_tool_wheel(window, cx)
            }
            None => return false,
        }
        true
    }
    /// Smoke-test input through window dispatch: the hovering pen presses
    /// its lower barrel button; then (`aim`) points at Pan and releases it.
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
        // Inside the ring, so only releasing the button chooses.
        let pan = point(at.x, at.y + px(32. * self.controller.settings.ui_scale));
        Ok(if aim {
            let mut events = [hover(at, 1), hover(pan, 1), hover(pan, 0)].concat();
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
            Err("Releasing the barrel button while aiming did not choose Pan".into())
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
        let current = WheelTool::ALL
            .into_iter()
            .find(|slot| slot.tool(self.selection_tool) == self.controller.tool);
        let hovers = WheelTool::ALL.map(|slot| self.motion.borrow_mut().hover_value(slot.key()));
        // The pointer's aim, else the keyboard focus, else a hovered tool.
        let highlighted = wheel
            .aimed
            .map(|slot| WheelTool::ALL[slot])
            .or_else(|| {
                wheel.keyboard.then(|| {
                    WheelTool::ALL
                        .into_iter()
                        .find(|slot| self.accessibility.is_focused(slot.key(), window))
                })?
            })
            .or_else(|| {
                (0..WheelTool::ALL.len())
                    .filter(|&i| hovers[i] > 0.5)
                    .max_by(|&a, &b| hovers[a].total_cmp(&hovers[b]))
                    .map(|i| WheelTool::ALL[i])
            });
        let accent = |alpha: f32| rgba((theme.accent << 8) | (alpha.clamp(0., 1.) * 255.) as u32);
        let mut segments = Vec::new();
        let mut buttons = Vec::new();
        for (index, slot) in WheelTool::ALL.into_iter().enumerate() {
            let angle = index as f32 / WheelTool::ALL.len() as f32 * TAU;
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
            let enabled = self.wheel_slot_enabled(WheelTool::ALL[slot]);
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
                    let angle = index as f32 / WheelTool::ALL.len() as f32 * TAU;
                    for color in [segment.base, segment.highlight] {
                        if color.a > 0.
                            && let Some(path) = segment_path(center, (inner, outer), angle, gap)
                        {
                            window.paint_path(path, color);
                        }
                    }
                    if let Some(color) = segment.mark
                        && let Some(path) =
                            segment_path(center, (outer - 4. * size, outer), angle, gap)
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
        let disc = div()
            .absolute()
            .left(wheel.center.x - px(radius))
            .top(wheel.center.y - px(radius))
            .size(px(2. * radius))
            .rounded_full()
            .glass_dialog(theme, theme.popover)
            .opacity(appear)
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
