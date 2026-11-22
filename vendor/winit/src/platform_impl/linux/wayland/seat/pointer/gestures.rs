//! Folio extension: touchpad pinch and rotation through `zwp_pointer_gestures_v1`, as the
//! `PinchGesture` and `RotationGesture` window events other platforms report.

use std::sync::Mutex;

use sctk::compositor::SurfaceData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_pointer::WlPointer;
use sctk::reexports::client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
};

use crate::event::{TouchPhase, WindowEvent};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId, WindowId};

/// The pointer gestures global.
pub struct PointerGesturesState {
    manager: ZwpPointerGesturesV1,
}

impl PointerGesturesState {
    pub fn new(globals: &GlobalList, queue_handle: &QueueHandle<WinitState>) -> Result<Self, BindError> {
        Ok(Self { manager: globals.bind(queue_handle, 1..=3, ())? })
    }

    pub fn pinch(&self, pointer: &WlPointer, queue_handle: &QueueHandle<WinitState>) -> ZwpPointerGesturePinchV1 {
        self.manager.get_pinch_gesture(pointer, queue_handle, PinchData::default())
    }
}

/// The window a pinch began over and its scale at the last update.
#[derive(Default)]
pub struct PinchData(Mutex<Option<(WindowId, f64)>>);

impl Dispatch<ZwpPointerGesturePinchV1, PinchData> for WinitState {
    fn event(
        state: &mut Self,
        _: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        data: &PinchData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let device_id = crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));
        let mut pinch = data.0.lock().unwrap();
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { surface, .. } => {
                let parent = surface
                    .data::<SurfaceData>()
                    .and_then(|d| d.parent_surface().cloned())
                    .unwrap_or(surface);
                let window_id = wayland::make_wid(&parent);
                if !state.windows.borrow().contains_key(&window_id) {
                    *pinch = None;
                    return;
                }
                *pinch = Some((window_id, 1.));
                let phase = TouchPhase::Started;
                let sink = &mut state.events_sink;
                sink.push_window_event(WindowEvent::PinchGesture { device_id, delta: 0., phase }, window_id);
                sink.push_window_event(WindowEvent::RotationGesture { device_id, delta: 0., phase }, window_id);
            },
            zwp_pointer_gesture_pinch_v1::Event::Update { scale, rotation, .. } => {
                let Some((window_id, last)) = pinch.as_mut() else { return };
                let delta = if *last > 0. && scale > 0. { scale / *last - 1. } else { 0. };
                *last = scale;
                let (window_id, phase) = (*window_id, TouchPhase::Moved);
                let sink = &mut state.events_sink;
                if delta != 0. {
                    sink.push_window_event(WindowEvent::PinchGesture { device_id, delta, phase }, window_id);
                }
                if rotation != 0. {
                    // Wayland turns clockwise positive; winit counterclockwise.
                    let delta = -rotation as f32;
                    sink.push_window_event(WindowEvent::RotationGesture { device_id, delta, phase }, window_id);
                }
            },
            zwp_pointer_gesture_pinch_v1::Event::End { cancelled, .. } => {
                let Some((window_id, _)) = pinch.take() else { return };
                let phase = if cancelled != 0 { TouchPhase::Cancelled } else { TouchPhase::Ended };
                let sink = &mut state.events_sink;
                sink.push_window_event(WindowEvent::PinchGesture { device_id, delta: 0., phase }, window_id);
                sink.push_window_event(WindowEvent::RotationGesture { device_id, delta: 0., phase }, window_id);
            },
            _ => {},
        }
    }
}

delegate_noop!(WinitState: ignore ZwpPointerGesturesV1);
