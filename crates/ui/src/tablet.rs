//! Pen and tablet-pad input from Folio's winit tablet extension.
//!
//! The platform layer publishes each pen report (pressure, tilt, eraser, barrel buttons) and
//! each pad control while it emulates the mouse for ordinary controls. Reports queue here,
//! wake the window, and reach the view at the start of the next frame, every sample kept.
//! [`folio_input::ForceAngle`] turns their force and angle into what ink uses, and the canvas
//! ignores the emulated mouse events while a pen is in range.
use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use winit::platform::tablet as native;

/// Reports waiting for the next frame. Bounded so a window that stops drawing cannot grow it
/// without limit; the oldest hover reports go first.
static QUEUE: Mutex<VecDeque<native::TabletEvent>> = Mutex::new(VecDeque::new());
const QUEUE_LIMIT: usize = 4096;

/// Starts receiving tablet reports, waking the window with `waker` for each.
pub fn install(waker: rune::Waker) {
    native::set_handler(move |event| {
        let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        if queue.len() >= QUEUE_LIMIT {
            let hover = queue.iter().position(|e| {
                matches!(e, native::TabletEvent::Tool(t) if t.phase == native::TabletPhase::Hover)
            });
            queue.remove(hover.unwrap_or(0));
        }
        queue.push_back(event);
        drop(queue);
        waker();
    });
}

/// Takes the reports that arrived since the last call.
pub fn drain() -> Vec<native::TabletEvent> {
    QUEUE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect()
}

/// Whether the pointer events being delivered now are a pen's emulated ones: a pen is in
/// range, or reported moments ago.
pub fn pen_active() -> bool {
    native::tool_in_proximity()
        || native::since_last_tool_report().is_some_and(|t| t < Duration::from_millis(200))
}

/// A pen report in view points, its force and tilt adapted.
pub fn tool_event(event: &native::TabletToolEvent, scale: f32) -> TabletEvent {
    let adapter = folio_input::ForceAngle::default();
    let phase = match event.phase {
        native::TabletPhase::Hover => TabletPhase::Hover,
        native::TabletPhase::Down => TabletPhase::Down,
        native::TabletPhase::Move => TabletPhase::Move,
        native::TabletPhase::Up => TabletPhase::Up,
        native::TabletPhase::Leave => TabletPhase::Leave,
        native::TabletPhase::Cancel => TabletPhase::Cancel,
    };
    let raw = folio_input::RawPen {
        pressure: event.pressure,
        tilt_x: event.tilt_x,
        tilt_y: event.tilt_y,
        has_pressure: event.has_pressure,
        has_tilt: event.has_tilt,
        contact: matches!(
            phase,
            TabletPhase::Down | TabletPhase::Move | TabletPhase::Up
        ),
    };
    let (tilt_x, tilt_y) = adapter.tilt(raw);
    let scale = scale.max(0.01) as f64;
    TabletEvent {
        phase,
        position: point(
            px((event.position.x / scale) as f32),
            px((event.position.y / scale) as f32),
        ),
        pressure: adapter.force(raw),
        tilt_x,
        tilt_y,
        buttons: event.buttons,
        timestamp: event.time,
        eraser: event.eraser,
    }
}

/// A pad report: ring movement in degrees, strip movement in fractions of its length.
pub fn pad_event(event: &native::TabletPadEvent) -> TabletPadEvent {
    match *event {
        native::TabletPadEvent::Button {
            button,
            pressed,
            time,
            ..
        } => TabletPadEvent {
            button: Some(button),
            pressed,
            strip: false,
            delta: 0.,
            timestamp: time,
        },
        native::TabletPadEvent::Ring { delta, time, .. } => TabletPadEvent {
            button: None,
            pressed: false,
            strip: false,
            delta,
            timestamp: time,
        },
        native::TabletPadEvent::Strip { delta, time, .. } => TabletPadEvent {
            button: None,
            pressed: false,
            strip: true,
            delta,
            timestamp: time,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(phase: native::TabletPhase, pressure: f32) -> native::TabletToolEvent {
        native::TabletToolEvent {
            window_id: winit::window::WindowId::dummy(),
            phase,
            position: winit::dpi::PhysicalPosition::new(300., 150.),
            pressure,
            tilt_x: 20.,
            tilt_y: -10.,
            twist: None,
            has_pressure: true,
            has_tilt: true,
            buttons: native::BUTTON_LOWER,
            eraser: false,
            time: 42,
        }
    }

    #[test]
    fn reports_become_view_points_with_adapted_force() {
        let down = tool_event(&report(native::TabletPhase::Down, 0.5), 1.5);
        assert_eq!(down.phase, TabletPhase::Down);
        assert_eq!(down.position, point(px(200.), px(100.)));
        assert!(down.pressure > 0.4 && down.pressure < 0.6);
        assert_eq!((down.tilt_x, down.tilt_y), (20., -10.));
        assert_eq!(down.buttons, native::BUTTON_LOWER);
        // A hovering pen exerts no force whatever the device last reported.
        let hover = tool_event(&report(native::TabletPhase::Hover, 0.5), 1.);
        assert_eq!(hover.pressure, 0.);
    }
}
