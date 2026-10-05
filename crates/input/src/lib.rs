//! Normalized rich tablet frames, independent of any UI toolkit.
use folio_document::{Point, StrokePoint};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Tablet,
    Mouse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Hover,
    Down,
    Move,
    Up,
    Leave,
    Cancel,
}
#[derive(Clone, Copy, Debug)]
pub struct PenEvent {
    pub device: Device,
    pub tool: Tool,
    pub phase: Phase,
    pub position: Point,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub buttons: u32,
    pub timestamp: u64,
}
impl PenEvent {
    pub fn sample(self, position: Point) -> StrokePoint {
        StrokePoint {
            x: position.x,
            y: position.y,
            pressure: self.pressure.clamp(0., 1.),
            tilt_x: self.tilt_x,
            tilt_y: self.tilt_y,
            timestamp: self.timestamp,
            buttons: self.buttons,
        }
    }
}
/// Wayland timestamps wrap at u32 milliseconds. Extend monotonically without
/// inventing timing from render frames, including near the 49-day wrap boundary.
#[derive(Default)]
pub struct TimestampExtender {
    last: Option<u32>,
    epoch: u64,
}
impl TimestampExtender {
    pub fn extend(&mut self, time: u32) -> u64 {
        if let Some(last) = self.last
            && time < last
            && last - time > u32::MAX / 2
        {
            self.epoch += 1u64 << 32
        }
        self.last = Some(time);
        self.epoch + time as u64
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps() {
        let mut t = TimestampExtender::default();
        assert_eq!(t.extend(u32::MAX - 1), u32::MAX as u64 - 1);
        assert_eq!(t.extend(2), (1u64 << 32) + 2);
    }
}
