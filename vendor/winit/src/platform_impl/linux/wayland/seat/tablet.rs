//! Folio extension: `zwp_tablet_v2` tools and pads.
//!
//! Binding the tablet manager stops the compositor from emulating a pointer for pens, so each
//! tool frame is published through [`crate::platform::tablet`] and then emulated here as cursor
//! movement and a left button, for the window's ordinary controls. Pads publish their express
//! keys and frame-delimited ring and strip movement.

use std::time::Duration;

use ahash::AHashMap;

use sctk::compositor::SurfaceData;
use sctk::reexports::client::backend::ObjectId;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_noop, event_created_child, Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use sctk::reexports::csd_frame::FrameClick;
use sctk::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};
use sctk::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1;
use sctk::reexports::protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2::ZwpTabletManagerV2,
    zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2},
    zwp_tablet_pad_ring_v2::{self, ZwpTabletPadRingV2},
    zwp_tablet_pad_strip_v2::{self, ZwpTabletPadStripV2},
    zwp_tablet_pad_v2::{self, ZwpTabletPadV2},
    zwp_tablet_seat_v2::{self, ZwpTabletSeatV2},
    zwp_tablet_tool_v2::{self, ZwpTabletToolV2},
    zwp_tablet_v2::{self, ZwpTabletV2},
};

use crate::dpi::{LogicalPosition, PhysicalPosition};
use crate::event::{ElementState, MouseButton, WindowEvent};
use crate::platform::tablet::{self, TabletEvent, TabletPadEvent, TabletPhase, TabletToolEvent};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId, WindowId};
use crate::window::CursorIcon;

/// Tablet globals and the tools and pads they announced.
#[derive(Default)]
pub struct TabletState {
    manager: Option<ZwpTabletManagerV2>,
    cursor_shape: Option<WpCursorShapeManagerV1>,
    tablet_seats: AHashMap<ObjectId, ZwpTabletSeatV2>,
    tools: AHashMap<ObjectId, ToolState>,
    pads: AHashMap<ObjectId, PadState>,
    pad_groups: AHashMap<ObjectId, ObjectId>,
    pad_controls: AHashMap<ObjectId, PadControl>,
}

impl TabletState {
    pub fn new(globals: &GlobalList, queue_handle: &QueueHandle<WinitState>) -> Self {
        if std::env::var_os("WINIT_WAYLAND_NO_TABLET").is_some() {
            return Self::default();
        }
        Self {
            manager: globals.bind(queue_handle, 1..=1, ()).ok(),
            cursor_shape: globals.bind(queue_handle, 1..=1, ()).ok(),
            ..Self::default()
        }
    }

    /// Asks for the tablets of `seat`.
    pub fn add_seat(&mut self, seat: &WlSeat, queue_handle: &QueueHandle<WinitState>) {
        if let Some(manager) = &self.manager {
            self.tablet_seats
                .entry(seat.id())
                .or_insert_with(|| manager.get_tablet_seat(seat, queue_handle, seat.clone()));
        }
    }

    pub fn remove_seat(&mut self, seat: &WlSeat) {
        if let Some(tablet_seat) = self.tablet_seats.remove(&seat.id()) {
            tablet_seat.destroy();
        }
    }
}

/// A tool in proximity of a window, for that window's cursor changes.
#[derive(Debug, Clone)]
pub struct TabletCursor {
    tool: ZwpTabletToolV2,
    serial: u32,
    shape: Option<WpCursorShapeDeviceV1>,
}

impl TabletCursor {
    pub fn serial(&self) -> u32 {
        self.serial
    }

    /// Shows `icon` for the tool, or hides its cursor.
    pub fn apply(&self, icon: Option<CursorIcon>) {
        match (icon, &self.shape) {
            (Some(icon), Some(shape)) => shape.set_shape(self.serial, cursor_shape(icon)),
            (None, _) => self.tool.set_cursor(self.serial, None, 0, 0),
            // Without the cursor shape protocol the compositor keeps its own tool cursor.
            (Some(_), None) => {},
        }
    }
}

#[derive(Default)]
struct ToolState {
    seat: Option<WlSeat>,
    window: Option<WindowId>,
    /// The decoration surface the tool is over, rather than the window's content.
    frame_surface: Option<WlSurface>,
    serial: u32,
    down_serial: u32,
    position: (f64, f64),
    pressure: f32,
    tilt: (f32, f32),
    twist: Option<f32>,
    has_pressure: bool,
    has_tilt: bool,
    buttons: u32,
    eraser: bool,
    down: bool,
    pending: Vec<TabletPhase>,
    leaving: bool,
    shape: Option<WpCursorShapeDeviceV1>,
}

#[derive(Default)]
struct PadState {
    window: Option<WindowId>,
    rings: u32,
    strips: u32,
}

struct PadControl {
    pad: ObjectId,
    index: u32,
    strip: bool,
    last: Option<f64>,
    pending: Option<f64>,
}

impl WinitState {
    fn tablet_window(&self, surface: &WlSurface) -> Option<(WindowId, Option<WlSurface>)> {
        let parent = surface.data::<SurfaceData>()?.parent_surface().unwrap_or(surface).clone();
        let window_id = wayland::make_wid(&parent);
        self.windows.borrow().contains_key(&window_id).then(|| {
            let frame = (parent != *surface).then(|| surface.clone());
            (window_id, frame)
        })
    }

    fn tool_frame(&mut self, id: &ObjectId, time: u32) {
        let device_id = crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));
        let Some(t) = self.tablet.tools.get_mut(id) else { return };
        let Some(window_id) = t.window else {
            t.pending.clear();
            return;
        };
        let phases = if t.pending.is_empty() {
            vec![if t.down { TabletPhase::Move } else { TabletPhase::Hover }]
        } else {
            std::mem::take(&mut t.pending)
        };
        let windows = self.windows.borrow();
        let Some(window) = windows.get(&window_id) else { return };
        let mut window = window.lock().unwrap();
        let scale = window.scale_factor();
        let position: PhysicalPosition<f64> =
            LogicalPosition::new(t.position.0, t.position.1).to_physical(scale);

        // Over client-side decorations: move, resize and click them as a pointer would.
        if let Some(surface) = t.frame_surface.clone() {
            let seat = t.seat.clone();
            for phase in phases {
                let Some(seat) = &seat else { break };
                match phase {
                    TabletPhase::Hover | TabletPhase::Move => {
                        window.frame_point_moved(
                            seat,
                            &surface,
                            Duration::from_millis(time as u64),
                            t.position.0,
                            t.position.1,
                        );
                    },
                    TabletPhase::Down | TabletPhase::Up => {
                        window.frame_click(
                            FrameClick::Normal,
                            phase == TabletPhase::Down,
                            seat,
                            t.down_serial,
                            Duration::from_millis(time as u64),
                            window_id,
                            &mut self.window_compositor_updates,
                        );
                    },
                    TabletPhase::Leave | TabletPhase::Cancel => window.frame_point_left(),
                }
            }
            if t.leaving {
                window.tablet_cursor_left(&t.serial);
                *t = ToolState { seat: t.seat.take(), eraser: t.eraser, shape: t.shape.take(), ..ToolState::default() };
            }
            return;
        }

        for phase in phases {
            tablet::emit(TabletEvent::Tool(TabletToolEvent {
                window_id: crate::window::WindowId(window_id),
                phase,
                position,
                pressure: if t.down || phase == TabletPhase::Up { t.pressure } else { 0. },
                tilt_x: t.tilt.0,
                tilt_y: t.tilt.1,
                twist: t.twist,
                has_pressure: t.has_pressure,
                has_tilt: t.has_tilt,
                buttons: t.buttons,
                eraser: t.eraser,
                time,
            }));
            let sink = &mut self.events_sink;
            match phase {
                TabletPhase::Hover | TabletPhase::Move => {
                    sink.push_window_event(WindowEvent::CursorMoved { device_id, position }, window_id);
                },
                TabletPhase::Down | TabletPhase::Up => {
                    sink.push_window_event(WindowEvent::CursorMoved { device_id, position }, window_id);
                    let state = if phase == TabletPhase::Down {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    };
                    sink.push_window_event(
                        WindowEvent::MouseInput { device_id, state, button: MouseButton::Left },
                        window_id,
                    );
                },
                TabletPhase::Leave | TabletPhase::Cancel => {
                    sink.push_window_event(WindowEvent::CursorLeft { device_id }, window_id);
                },
            }
        }
        if t.leaving {
            window.tablet_cursor_left(&t.serial);
            t.window = None;
            t.down = false;
            t.buttons = 0;
            t.leaving = false;
        }
    }
}

impl Dispatch<ZwpTabletSeatV2, WlSeat> for WinitState {
    fn event(
        state: &mut Self,
        _: &ZwpTabletSeatV2,
        event: zwp_tablet_seat_v2::Event,
        seat: &WlSeat,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_seat_v2::Event::ToolAdded { id } = event {
            state.tablet.tools.insert(
                id.id(),
                ToolState { seat: Some(seat.clone()), ..ToolState::default() },
            );
        }
    }

    event_created_child!(WinitState, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);
}

impl Dispatch<ZwpTabletV2, ()> for WinitState {
    fn event(
        _: &mut Self,
        tablet: &ZwpTabletV2,
        event: zwp_tablet_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_tablet_v2::Event::Removed) {
            tablet.destroy();
        }
    }
}

impl Dispatch<ZwpTabletToolV2, ()> for WinitState {
    fn event(
        state: &mut Self,
        tool: &ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        use zwp_tablet_tool_v2::Event;
        let id = tool.id();
        let target = match &event {
            Event::ProximityIn { surface, .. } => state.tablet_window(surface),
            _ => None,
        };
        let cursor_shape = state.tablet.cursor_shape.clone();
        let t = state.tablet.tools.entry(id.clone()).or_default();
        match event {
            Event::Type { tool_type } => {
                t.eraser = matches!(tool_type, WEnum::Value(zwp_tablet_tool_v2::Type::Eraser));
            },
            Event::Capability { capability } => match capability {
                WEnum::Value(zwp_tablet_tool_v2::Capability::Pressure) => t.has_pressure = true,
                WEnum::Value(zwp_tablet_tool_v2::Capability::Tilt) => t.has_tilt = true,
                _ => {},
            },
            Event::ProximityIn { serial, .. } => {
                let Some((window_id, frame)) = target else { return };
                t.window = Some(window_id);
                t.frame_surface = frame;
                t.serial = serial;
                t.leaving = false;
                t.pressure = 0.;
                t.buttons = 0;
                t.down = false;
                t.pending.clear();
                if t.shape.is_none() {
                    t.shape = cursor_shape.map(|m| m.get_tablet_tool_v2(tool, queue_handle, ()));
                }
                // The compositor forgets the tool's cursor when it leaves.
                let cursor = TabletCursor { tool: tool.clone(), serial, shape: t.shape.clone() };
                if t.frame_surface.is_some() {
                    cursor.apply(Some(CursorIcon::Default));
                } else if let Some(window) = state.windows.borrow().get(&window_id) {
                    window.lock().unwrap().tablet_cursor_entered(cursor);
                }
            },
            Event::ProximityOut => {
                t.leaving = true;
                if t.down {
                    t.pending.push(TabletPhase::Up);
                }
                t.pending.push(TabletPhase::Leave);
            },
            Event::Down { serial } => {
                t.down = true;
                t.down_serial = serial;
                t.pending.push(TabletPhase::Down);
            },
            Event::Up => {
                t.down = false;
                t.pending.push(TabletPhase::Up);
            },
            Event::Motion { x, y } => t.position = (x, y),
            Event::Pressure { pressure } => t.pressure = pressure as f32 / 65535.,
            Event::Tilt { tilt_x, tilt_y } => t.tilt = (tilt_x as f32, tilt_y as f32),
            Event::Rotation { degrees } => t.twist = Some((degrees as f32).rem_euclid(360.)),
            Event::Button { button, state: button_state, .. } => {
                let bit = match button {
                    0x14b => tablet::BUTTON_LOWER,
                    0x14c => tablet::BUTTON_UPPER,
                    _ => tablet::BUTTON_OTHER,
                };
                if matches!(button_state, WEnum::Value(zwp_tablet_tool_v2::ButtonState::Pressed)) {
                    t.buttons |= bit;
                } else {
                    t.buttons &= !bit;
                }
            },
            Event::Frame { time } => state.tool_frame(&id, time),
            Event::Removed => {
                if let Some(t) = state.tablet.tools.remove(&id) {
                    if let (Some(window_id), None) = (t.window, &t.frame_surface) {
                        if let Some(window) = state.windows.borrow().get(&window_id) {
                            window.lock().unwrap().tablet_cursor_left(&t.serial);
                        }
                        let device_id = crate::event::DeviceId(
                            crate::platform_impl::DeviceId::Wayland(DeviceId),
                        );
                        let scale = state
                            .windows
                            .borrow()
                            .get(&window_id)
                            .map(|w| w.lock().unwrap().scale_factor())
                            .unwrap_or(1.);
                        tablet::emit(TabletEvent::Tool(TabletToolEvent {
                            window_id: crate::window::WindowId(window_id),
                            phase: TabletPhase::Cancel,
                            position: LogicalPosition::new(t.position.0, t.position.1)
                                .to_physical(scale),
                            pressure: 0.,
                            tilt_x: t.tilt.0,
                            tilt_y: t.tilt.1,
                            twist: t.twist,
                            has_pressure: t.has_pressure,
                            has_tilt: t.has_tilt,
                            buttons: 0,
                            eraser: t.eraser,
                            time: 0,
                        }));
                        state
                            .events_sink
                            .push_window_event(WindowEvent::CursorLeft { device_id }, window_id);
                    }
                    if let Some(shape) = t.shape {
                        shape.destroy();
                    }
                }
                tool.destroy();
            },
            _ => {},
        }
    }
}

impl Dispatch<ZwpTabletPadV2, ()> for WinitState {
    fn event(
        state: &mut Self,
        pad: &ZwpTabletPadV2,
        event: zwp_tablet_pad_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_tablet_pad_v2::Event;
        let id = pad.id();
        match event {
            Event::Enter { surface, .. } => {
                let window = state.tablet_window(&surface).map(|(window, _)| window);
                state.tablet.pads.entry(id).or_default().window = window;
            },
            Event::Leave { .. } => {
                if let Some(p) = state.tablet.pads.get_mut(&id) {
                    p.window = None;
                }
                for c in state.tablet.pad_controls.values_mut().filter(|c| c.pad == id) {
                    c.last = None;
                    c.pending = None;
                }
            },
            Event::Group { pad_group } => {
                state.tablet.pad_groups.insert(pad_group.id(), id.clone());
                state.tablet.pads.entry(id).or_default();
            },
            Event::Button { time, button, state: button_state } => {
                let window = state.tablet.pads.get(&id).and_then(|p| p.window);
                if let Some(window_id) = window {
                    tablet::emit(TabletEvent::Pad(TabletPadEvent::Button {
                        window_id: crate::window::WindowId(window_id),
                        button,
                        pressed: matches!(
                            button_state,
                            WEnum::Value(zwp_tablet_pad_v2::ButtonState::Pressed)
                        ),
                        time,
                    }));
                }
            },
            Event::Removed => {
                state.tablet.pads.remove(&id);
                state.tablet.pad_controls.retain(|_, c| c.pad != id);
                state.tablet.pad_groups.retain(|_, p| *p != id);
                pad.destroy();
            },
            _ => {},
        }
    }

    event_created_child!(WinitState, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadGroupV2, ()> for WinitState {
    fn event(
        state: &mut Self,
        group: &ZwpTabletPadGroupV2,
        event: zwp_tablet_pad_group_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(pad) = state.tablet.pad_groups.get(&group.id()).cloned() else { return };
        let Some(p) = state.tablet.pads.get_mut(&pad) else { return };
        let (id, strip, index) = match event {
            zwp_tablet_pad_group_v2::Event::Ring { ring } => {
                p.rings += 1;
                (ring.id(), false, p.rings - 1)
            },
            zwp_tablet_pad_group_v2::Event::Strip { strip } => {
                p.strips += 1;
                (strip.id(), true, p.strips - 1)
            },
            _ => return,
        };
        state.tablet.pad_controls.insert(id, PadControl { pad, index, strip, last: None, pending: None });
    }

    event_created_child!(WinitState, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
    ]);
}

impl WinitState {
    /// Publishes the movement since the control's last frame within one touch.
    fn pad_control_frame(&mut self, id: &ObjectId, time: u32) {
        let Some(control) = self.tablet.pad_controls.get_mut(id) else { return };
        let Some(value) = control.pending.take() else { return };
        let Some(previous) = control.last.replace(value) else { return };
        let (strip, index) = (control.strip, control.index);
        let Some(window_id) = self.tablet.pads.get(&control.pad).and_then(|p| p.window) else {
            return;
        };
        let window_id = crate::window::WindowId(window_id);
        let event = if strip {
            TabletPadEvent::Strip { window_id, strip: index, delta: (value - previous) as f32, time }
        } else {
            TabletPadEvent::Ring {
                window_id,
                ring: index,
                delta: tablet::ring_delta(previous, value) as f32,
                time,
            }
        };
        tablet::emit(TabletEvent::Pad(event));
    }

    fn pad_control_stop(&mut self, id: &ObjectId) {
        if let Some(c) = self.tablet.pad_controls.get_mut(id) {
            c.last = None;
            c.pending = None;
        }
    }
}

impl Dispatch<ZwpTabletPadRingV2, ()> for WinitState {
    fn event(
        state: &mut Self,
        ring: &ZwpTabletPadRingV2,
        event: zwp_tablet_pad_ring_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_tablet_pad_ring_v2::Event;
        match event {
            Event::Angle { degrees } => {
                if let Some(c) = state.tablet.pad_controls.get_mut(&ring.id()) {
                    c.pending = Some(degrees);
                }
            },
            Event::Stop => state.pad_control_stop(&ring.id()),
            Event::Frame { time } => state.pad_control_frame(&ring.id(), time),
            _ => {},
        }
    }
}

impl Dispatch<ZwpTabletPadStripV2, ()> for WinitState {
    fn event(
        state: &mut Self,
        strip: &ZwpTabletPadStripV2,
        event: zwp_tablet_pad_strip_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_tablet_pad_strip_v2::Event;
        match event {
            Event::Position { position } => {
                if let Some(c) = state.tablet.pad_controls.get_mut(&strip.id()) {
                    c.pending = Some(position as f64 / 65535.);
                }
            },
            Event::Stop => state.pad_control_stop(&strip.id()),
            Event::Frame { time } => state.pad_control_frame(&strip.id(), time),
            _ => {},
        }
    }
}

delegate_noop!(WinitState: ignore ZwpTabletManagerV2);
delegate_noop!(WinitState: ignore WpCursorShapeManagerV1);
delegate_noop!(WinitState: ignore WpCursorShapeDeviceV1);

/// The `wp_cursor_shape_v1` shape for a cursor icon.
fn cursor_shape(icon: CursorIcon) -> Shape {
    match icon {
        CursorIcon::Default => Shape::Default,
        CursorIcon::ContextMenu => Shape::ContextMenu,
        CursorIcon::Help => Shape::Help,
        CursorIcon::Pointer => Shape::Pointer,
        CursorIcon::Progress => Shape::Progress,
        CursorIcon::Wait => Shape::Wait,
        CursorIcon::Cell => Shape::Cell,
        CursorIcon::Crosshair => Shape::Crosshair,
        CursorIcon::Text => Shape::Text,
        CursorIcon::VerticalText => Shape::VerticalText,
        CursorIcon::Alias => Shape::Alias,
        CursorIcon::Copy => Shape::Copy,
        CursorIcon::Move => Shape::Move,
        CursorIcon::NoDrop => Shape::NoDrop,
        CursorIcon::NotAllowed => Shape::NotAllowed,
        CursorIcon::Grab => Shape::Grab,
        CursorIcon::Grabbing => Shape::Grabbing,
        CursorIcon::EResize => Shape::EResize,
        CursorIcon::NResize => Shape::NResize,
        CursorIcon::NeResize => Shape::NeResize,
        CursorIcon::NwResize => Shape::NwResize,
        CursorIcon::SResize => Shape::SResize,
        CursorIcon::SeResize => Shape::SeResize,
        CursorIcon::SwResize => Shape::SwResize,
        CursorIcon::WResize => Shape::WResize,
        CursorIcon::EwResize => Shape::EwResize,
        CursorIcon::NsResize => Shape::NsResize,
        CursorIcon::NeswResize => Shape::NeswResize,
        CursorIcon::NwseResize => Shape::NwseResize,
        CursorIcon::ColResize => Shape::ColResize,
        CursorIcon::RowResize => Shape::RowResize,
        CursorIcon::AllScroll => Shape::AllScroll,
        CursorIcon::ZoomIn => Shape::ZoomIn,
        CursorIcon::ZoomOut => Shape::ZoomOut,
        _ => Shape::Default,
    }
}
