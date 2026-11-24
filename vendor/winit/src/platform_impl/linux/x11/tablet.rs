//! Folio extension: pen pressure, tilt and pad buttons from XInput2 valuators.
//!
//! Pens report through the master pointer like a mouse; their slave device carries pressure
//! and tilt valuators, which are published through [`crate::platform::tablet`]. The barrel
//! buttons become [`TabletToolEvent::buttons`] bits instead of middle and right clicks, and
//! a pad's express keys become pad buttons instead of clicks.

use std::ffi::CStr;
use std::slice;

use x11rb::protocol::xproto::ConnectionExt as _;

use super::ffi;
use super::XConnection;
use crate::dpi::PhysicalPosition;
use crate::platform::tablet::{self, TabletEvent, TabletPadEvent, TabletPhase, TabletToolEvent};

/// One valuator: its number and range.
#[derive(Debug, Clone, Copy)]
struct Axis {
    number: i32,
    min: f64,
    max: f64,
}

impl Axis {
    fn pressure(&self, value: f64) -> f32 {
        ((value - self.min) / (self.max - self.min).max(1.)).clamp(0., 1.) as f32
    }

    /// Drivers report tilt in degrees (libinput) or in -64..63 steps of about a degree
    /// (wacom); wider ranges are scaled so their extreme is 64 degrees.
    fn tilt(&self, value: f64) -> f32 {
        let extent = self.min.abs().max(self.max.abs());
        let degrees = if extent > 90. { value / extent * 64. } else { value };
        degrees.clamp(-90., 90.) as f32
    }
}

/// A pen's valuators and contact state.
#[derive(Debug, Clone)]
pub struct TabletAxes {
    pressure: Axis,
    tilt_x: Option<Axis>,
    tilt_y: Option<Axis>,
    values: [f32; 3],
    pub eraser: bool,
    pub down: bool,
    pub buttons: u32,
    /// The window the pen is over, with its last position, while it reports to one.
    pub over: Option<(crate::window::WindowId, PhysicalPosition<f64>)>,
}

/// What a slave device is, for the tablet extension.
#[derive(Debug, Clone, Default)]
pub enum TabletRole {
    #[default]
    None,
    Pen(TabletAxes),
    Pad,
}

impl TabletRole {
    pub fn detect(xconn: &XConnection, info: &ffi::XIDeviceInfo) -> Self {
        if info._use != ffi::XISlavePointer && info._use != ffi::XIFloatingSlave {
            return Self::None;
        }
        let name = unsafe { CStr::from_ptr(info.name) }.to_string_lossy().to_lowercase();
        let classes = unsafe {
            slice::from_raw_parts(
                info.classes as *const *const ffi::XIAnyClassInfo,
                info.num_classes as usize,
            )
        };
        let (mut pressure, mut tilt_x, mut tilt_y) = (None, None, None);
        for &class in classes {
            if unsafe { (*class)._type } != ffi::XIValuatorClass {
                continue;
            }
            let v = unsafe { &*(class as *const ffi::XIValuatorClassInfo) };
            if v.label == 0 {
                continue;
            }
            let label = xconn
                .xcb_connection()
                .get_atom_name(v.label as u32)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| String::from_utf8_lossy(&reply.name).to_lowercase())
                .unwrap_or_default();
            let axis = Axis { number: v.number, min: v.min, max: v.max };
            if label.contains("pressure") {
                pressure = Some(axis);
            } else if label.contains("tilt x") || label.contains("tilt_x") {
                tilt_x = Some(axis);
            } else if label.contains("tilt y") || label.contains("tilt_y") {
                tilt_y = Some(axis);
            }
        }
        match pressure {
            Some(pressure) => Self::Pen(TabletAxes {
                pressure,
                tilt_x,
                tilt_y,
                values: [0.; 3],
                eraser: name.contains("eraser"),
                down: false,
                buttons: 0,
                over: None,
            }),
            None if name.contains(" pad") => Self::Pad,
            None => Self::None,
        }
    }
}

impl TabletAxes {
    /// Reads the valuators an XI2 device event carries.
    ///
    /// # Safety
    /// `mask` and `values` must be the event's valuator mask and values.
    pub unsafe fn read(&mut self, mask: &[u8], mut values: *const f64) {
        let axes = [Some(self.pressure), self.tilt_x, self.tilt_y];
        for bit in 0..(mask.len() * 8) as i32 {
            if mask[(bit >> 3) as usize] & (1 << (bit & 7)) == 0 {
                continue;
            }
            let value = unsafe { *values };
            values = unsafe { values.add(1) };
            for (i, axis) in axes.iter().enumerate() {
                if let Some(axis) = axis.filter(|a| a.number == bit) {
                    self.values[i] = if i == 0 { axis.pressure(value) } else { axis.tilt(value) };
                }
            }
        }
    }

    pub fn emit(
        &mut self,
        window_id: crate::window::WindowId,
        phase: TabletPhase,
        position: PhysicalPosition<f64>,
        time: u32,
    ) {
        self.over = match phase {
            TabletPhase::Leave | TabletPhase::Cancel => None,
            _ => Some((window_id, position)),
        };
        tablet::emit(TabletEvent::Tool(TabletToolEvent {
            window_id,
            phase,
            position,
            pressure: if self.down || phase == TabletPhase::Up { self.values[0] } else { 0. },
            tilt_x: self.values[1],
            tilt_y: self.values[2],
            twist: None,
            has_pressure: true,
            has_tilt: self.tilt_x.is_some() && self.tilt_y.is_some(),
            buttons: self.buttons,
            eraser: self.eraser,
            time,
        }));
    }

    /// Leaves the window the pen was over, lifting it first, as when another device takes
    /// over the pointer or the pointer leaves.
    pub fn leave(&mut self, phase: TabletPhase, time: u32) {
        if let Some((window_id, position)) = self.over {
            if self.down {
                self.emit(window_id, TabletPhase::Up, position, time);
                self.down = false;
            }
            self.buttons = 0;
            self.emit(window_id, phase, position, time);
        }
    }

    /// The tool bit of a barrel button, if `detail` is one.
    pub fn barrel_bit(detail: i32) -> Option<u32> {
        match detail {
            2 => Some(tablet::BUTTON_LOWER),
            3 => Some(tablet::BUTTON_UPPER),
            8..=33 => Some(tablet::BUTTON_OTHER),
            _ => None,
        }
    }
}

/// Publishes a pad's express key. Buttons 4 to 7 are its ring and strip scrolling, which
/// stay wheel events.
pub fn pad_button(window_id: crate::window::WindowId, detail: i32, pressed: bool, time: u32) -> bool {
    let button = match detail {
        1..=3 => detail - 1,
        8.. => detail - 5,
        _ => return false,
    };
    tablet::emit(TabletEvent::Pad(TabletPadEvent::Button {
        window_id,
        button: button as u32,
        pressed,
        time,
    }));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_axes() {
        let pressure = Axis { number: 2, min: 0., max: 65535. };
        assert_eq!(pressure.pressure(65535.), 1.);
        assert_eq!(pressure.pressure(-4.), 0.);
        let wacom = Axis { number: 3, min: -64., max: 63. };
        assert_eq!(wacom.tilt(-40.), -40.);
        let wide = Axis { number: 3, min: -9000., max: 9000. };
        assert_eq!(wide.tilt(4500.), 32.);
    }

    #[test]
    fn reads_masked_valuators() {
        let mut axes = TabletAxes {
            pressure: Axis { number: 2, min: 0., max: 2048. },
            tilt_x: Some(Axis { number: 3, min: -64., max: 63. }),
            tilt_y: Some(Axis { number: 4, min: -64., max: 63. }),
            values: [0.; 3],
            eraser: false,
            down: true,
            buttons: 0,
            over: None,
        };
        // Valuators 0, 2 and 4 are present: x, pressure, tilt y.
        let mask = [0b10101u8];
        let values = [100., 1024., -12.];
        unsafe { axes.read(&mask, values.as_ptr()) };
        assert_eq!(axes.values, [0.5, 0., -12.]);
    }
}
