//! Windows Ink frames translated into GPUI's rich tablet and control events.
use super::{WindowsWindowInner, current_modifiers, logical_point};
use crate::*;
use ::util::ResultExt;
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::ScreenToClient,
    UI::{
        Input::{KeyboardAndMouse::ReleaseCapture, Pointer::*},
        WindowsAndMessaging::*,
    },
};

pub(super) struct PenState {
    last: TabletEvent,
    button: Option<MouseButton>,
    performance_count: u64,
}

pub(super) fn promoted_pen_mouse() -> bool {
    let extra = unsafe { GetMessageExtraInfo() }.0 as usize;
    extra & 0xffff_ff00 == 0xff51_5700 && extra & 0x80 == 0
}

impl WindowsWindowInner {
    pub(super) fn start_native_drag(&self, hit: u32) {
        let handle = self.get_handle();
        // Schedule the native modal move/resize loop after GPUI releases its
        // current event borrow. Synchronous SendMessage here can re-enter GPUI.
        self.executor
            .spawn(async move {
                unsafe {
                    let mut cursor = POINT::default();
                    if GetCursorPos(&mut cursor).is_ok() {
                        let packed =
                            ((cursor.x as u16 as u32) | ((cursor.y as u16 as u32) << 16)) as isize;
                        ReleaseCapture().log_err();
                        DefWindowProcW(
                            handle,
                            WM_NCLBUTTONDOWN,
                            WPARAM(hit as usize),
                            LPARAM(packed),
                        );
                    }
                }
            })
            .detach();
    }

    fn emit_pen_input(&self, input: PlatformInput) {
        let callback = self.state.borrow_mut().callbacks.input.take();
        if let Some(mut callback) = callback {
            callback(input);
            self.state.borrow_mut().callbacks.input = Some(callback);
        }
    }

    pub(super) fn handle_pen_message(&self, message: u32, wparam: WPARAM) -> Option<isize> {
        let id = (wparam.0 & 0xffff) as u32;
        if message == WM_POINTERCAPTURECHANGED {
            if self.state.borrow().pens.contains_key(&id) {
                self.finish_pen(id, TabletPhase::Cancel);
                return Some(0);
            }
            return None;
        }
        let mut kind = POINTER_INPUT_TYPE::default();
        if unsafe { GetPointerType(id, &mut kind) }.is_err() || kind != PT_PEN {
            return None;
        }
        if message == WM_POINTERLEAVE {
            self.finish_pen(id, TabletPhase::Leave);
            return Some(0);
        }
        let mut latest = POINTER_PEN_INFO::default();
        if unsafe { GetPointerPenInfo(id, &mut latest) }.is_err() {
            self.finish_pen(id, TabletPhase::Cancel);
            return Some(0);
        }
        if latest
            .pointerInfo
            .pointerFlags
            .contains(POINTER_FLAG_CANCELED)
        {
            self.finish_pen(id, TabletPhase::Cancel);
            return Some(0);
        }
        // Read history before the next native message invalidates it. A bounded
        // buffer prevents a stalled UI from creating unbounded allocations.
        let mut samples = vec![latest];
        if message == WM_POINTERUPDATE && latest.pointerInfo.historyCount > 1 {
            let capacity = latest.pointerInfo.historyCount.min(512);
            let mut history = vec![POINTER_PEN_INFO::default(); capacity as usize];
            let mut count = capacity;
            if unsafe { GetPointerPenInfoHistory(id, &mut count, Some(history.as_mut_ptr())) }
                .is_ok()
            {
                history.truncate((count as usize).min(history.len()));
                history.reverse();
                samples = history;
            }
        }
        for sample in samples {
            if sample
                .pointerInfo
                .pointerFlags
                .contains(POINTER_FLAG_CANCELED)
            {
                self.finish_pen(id, TabletPhase::Cancel);
                break;
            }
            let mut location = sample.pointerInfo.ptPixelLocation;
            if !unsafe { ScreenToClient(self.get_handle(), &mut location) }.as_bool() {
                continue;
            }
            let scale = self.state.borrow().scale_factor;
            let flags = sample.pointerInfo.pointerFlags;
            let was_down = self
                .state
                .borrow()
                .pens
                .get(&id)
                .is_some_and(|p| p.button.is_some());
            let in_contact = flags.contains(POINTER_FLAG_INCONTACT);
            let phase = if message == WM_POINTERUP || flags.contains(POINTER_FLAG_UP) {
                TabletPhase::Up
            } else if in_contact && !was_down {
                TabletPhase::Down
            } else if in_contact {
                TabletPhase::Move
            } else {
                TabletPhase::Hover
            };
            if message == WM_POINTERUPDATE
                && sample.pointerInfo.PerformanceCount != 0
                && self
                    .state
                    .borrow()
                    .pens
                    .get(&id)
                    .is_some_and(|p| p.performance_count >= sample.pointerInfo.PerformanceCount)
            {
                continue;
            }
            let position = logical_point(location.x as f32, location.y as f32, scale);
            let event = TabletEvent {
                position,
                pressure: if in_contact {
                    if sample.penMask & PEN_MASK_PRESSURE != 0 {
                        (sample.pressure as f32 / 1024.).clamp(0., 1.)
                    } else {
                        0.6
                    }
                } else {
                    0.
                },
                tilt_x: if sample.penMask & PEN_MASK_TILT_X != 0 {
                    sample.tiltX as f32
                } else {
                    0.
                },
                tilt_y: if sample.penMask & PEN_MASK_TILT_Y != 0 {
                    sample.tiltY as f32
                } else {
                    0.
                },
                timestamp: sample.pointerInfo.dwTime,
                buttons: u32::from(sample.penFlags & PEN_FLAG_BARREL != 0),
                eraser: sample.penFlags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0,
                phase,
                modifiers: current_modifiers(),
            };
            let old_button = self.state.borrow().pens.get(&id).and_then(|p| p.button);
            let button = if in_contact && phase != TabletPhase::Up {
                Some(if sample.penFlags & PEN_FLAG_BARREL != 0 {
                    MouseButton::Right
                } else {
                    MouseButton::Left
                })
            } else {
                None
            };
            self.state.borrow_mut().pens.insert(
                id,
                PenState {
                    last: event.clone(),
                    button,
                    performance_count: sample.pointerInfo.PerformanceCount,
                },
            );
            self.emit_pen_input(PlatformInput::Tablet(event.clone()));
            if let Some(old) = old_button.filter(|old| Some(*old) != button) {
                self.emit_pen_input(PlatformInput::MouseUp(MouseUpEvent {
                    button: old,
                    position,
                    modifiers: event.modifiers,
                    click_count: 1,
                }));
            }
            if let Some(button) = button.filter(|button| Some(*button) != old_button) {
                let click_count = self.state.borrow_mut().click_state.update(
                    button,
                    point(DevicePixels(location.x), DevicePixels(location.y)),
                );
                self.emit_pen_input(PlatformInput::MouseDown(MouseDownEvent {
                    button,
                    position,
                    modifiers: event.modifiers,
                    click_count,
                    first_mouse: false,
                }));
            } else {
                self.emit_pen_input(PlatformInput::MouseMove(MouseMoveEvent {
                    position,
                    pressed_button: button,
                    modifiers: event.modifiers,
                }));
            }
        }
        Some(0)
    }

    fn finish_pen(&self, id: u32, phase: TabletPhase) {
        let state = self.state.borrow_mut().pens.remove(&id);
        if let Some(mut state) = state {
            state.last.phase = phase;
            state.last.pressure = 0.;
            self.emit_pen_input(PlatformInput::Tablet(state.last.clone()));
            if let Some(button) = state.button {
                self.emit_pen_input(PlatformInput::MouseUp(MouseUpEvent {
                    button,
                    position: state.last.position,
                    modifiers: state.last.modifiers,
                    click_count: 1,
                }));
            }
        }
    }

    pub(super) fn cancel_pens(&self) {
        let ids: Vec<_> = self.state.borrow().pens.keys().copied().collect();
        for id in ids {
            self.finish_pen(id, TabletPhase::Cancel);
        }
    }
}
