//! Folio extension: rich tablet input.
//!
//! Pen tools report through [`WindowEvent`](crate::event::WindowEvent)s as an ordinary left
//! mouse button (so they operate every control), and additionally publish their pressure, tilt,
//! eraser identity and barrel buttons here. Tablet pads (express keys, touch rings and strips)
//! have no pointer equivalent and are only published here.
//!
//! Backends: Wayland `zwp_tablet_v2` tools and pads, X11 XInput2 valuators, and Windows Ink
//! (`WM_POINTER` pen messages). Other platforms publish nothing.
//!
//! The handler runs on the event loop thread, during event dispatch, before the pointer events
//! emulated for the same report are delivered.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::dpi::PhysicalPosition;
use crate::window::WindowId;

/// The contact phase of a tool report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabletPhase {
    /// In proximity, not touching.
    Hover,
    /// The tip touched the surface.
    Down,
    /// Moved while touching.
    Move,
    /// The tip lifted.
    Up,
    /// The tool left proximity of the window.
    Leave,
    /// The tool or its tablet went away; any stroke in progress should be discarded.
    Cancel,
}

/// The lower barrel button (`BTN_STYLUS`, Windows' barrel button) in [`TabletToolEvent::buttons`].
pub const BUTTON_LOWER: u32 = 1;
/// The upper barrel button (`BTN_STYLUS2`) in [`TabletToolEvent::buttons`].
pub const BUTTON_UPPER: u32 = 2;
/// Any other tool button (`BTN_STYLUS3`, airbrush buttons).
pub const BUTTON_OTHER: u32 = 4;

/// One pen report.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletToolEvent {
    pub window_id: WindowId,
    pub phase: TabletPhase,
    /// In physical pixels from the window's content origin, as `CursorMoved` reports.
    pub position: PhysicalPosition<f64>,
    /// Normalised tip force, 0 to 1. 0 while hovering.
    pub pressure: f32,
    /// Tilt towards the positive x axis, in degrees (-90 to 90). 0 when the device has no tilt.
    pub tilt_x: f32,
    /// Tilt towards the positive y axis, in degrees (-90 to 90).
    pub tilt_y: f32,
    /// Barrel rotation in degrees, 0 to 360, where the device reports it.
    pub twist: Option<f32>,
    /// Whether the device reports pressure at all (a mouse-like puck does not).
    pub has_pressure: bool,
    /// Whether the device reports tilt.
    pub has_tilt: bool,
    /// [`BUTTON_LOWER`], [`BUTTON_UPPER`] and [`BUTTON_OTHER`] bits.
    pub buttons: u32,
    /// The eraser end (or an eraser tool) rather than the tip.
    pub eraser: bool,
    /// The platform's millisecond timestamp. It wraps; consecutive values are comparable.
    pub time: u32,
}

/// A tablet pad control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabletPadEvent {
    /// An express key, numbered from 0 within its pad.
    Button { window_id: WindowId, button: u32, pressed: bool, time: u32 },
    /// A touch ring turned by `delta` degrees (clockwise positive), within one touch.
    Ring { window_id: WindowId, ring: u32, delta: f32, time: u32 },
    /// A touch strip moved by `delta` of its length (towards its end positive), within one touch.
    Strip { window_id: WindowId, strip: u32, delta: f32, time: u32 },
}

/// Anything the tablet backends publish.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabletEvent {
    Tool(TabletToolEvent),
    Pad(TabletPadEvent),
}

type Handler = Box<dyn FnMut(TabletEvent) + Send>;
static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);
static IN_PROXIMITY: AtomicBool = AtomicBool::new(false);
static LAST_TOOL: AtomicU64 = AtomicU64::new(0);
static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Receives every tablet report from now on, replacing an earlier handler.
pub fn set_handler(handler: impl FnMut(TabletEvent) + Send + 'static) {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(handler));
}

/// Whether a pen tool is in proximity of one of the application's windows, so the pointer
/// events being delivered are the pen's emulated ones.
pub fn tool_in_proximity() -> bool {
    IN_PROXIMITY.load(Ordering::Relaxed)
}

/// How long ago the last pen report arrived, if one did.
pub fn since_last_tool_report() -> Option<std::time::Duration> {
    let micros = LAST_TOOL.load(Ordering::Relaxed);
    (micros != 0).then(|| {
        let epoch = *EPOCH.get_or_init(Instant::now);
        epoch.elapsed().saturating_sub(std::time::Duration::from_micros(micros))
    })
}

pub(crate) fn emit(event: TabletEvent) {
    if let TabletEvent::Tool(tool) = &event {
        let epoch = *EPOCH.get_or_init(Instant::now);
        LAST_TOOL.store((epoch.elapsed().as_micros() as u64).max(1), Ordering::Relaxed);
        IN_PROXIMITY
            .store(!matches!(tool.phase, TabletPhase::Leave | TabletPhase::Cancel), Ordering::Relaxed);
    }
    if let Some(handler) = HANDLER.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        handler(event);
    }
}

/// The signed shortest turn from `previous` to `angle`, in degrees.
#[allow(dead_code)]
pub(crate) fn ring_delta(previous: f64, angle: f64) -> f64 {
    (angle - previous + 180.).rem_euclid(360.) - 180.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_wraps_across_zero() {
        assert!((ring_delta(350., 10.) - 20.).abs() < 1e-9);
        assert!((ring_delta(10., 350.) + 20.).abs() < 1e-9);
        assert!((ring_delta(90., 120.) - 30.).abs() < 1e-9);
    }
}
