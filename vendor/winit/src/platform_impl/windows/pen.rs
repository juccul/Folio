//! Folio extension: Windows Ink pens.
//!
//! winit reports pens as touches, which a touch-first UI pans with and long-presses into
//! context menus, and drops their hover, tilt, eraser and barrel button. Pens instead publish
//! every history sample through [`crate::platform::tablet`] and emulate the mouse (cursor
//! movement and the left button) for the window's controls.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;

use windows_sys::Win32::Foundation::{HANDLE, HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::Input::Pointer::{
    POINTER_FLAG_CANCELED, POINTER_FLAG_INCONTACT, POINTER_FLAG_UP, POINTER_INFO,
    POINTER_PEN_INFO,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetMessageExtraInfo, PEN_FLAG_BARREL, PEN_FLAG_ERASER, PEN_FLAG_INVERTED, PEN_MASK_PRESSURE,
    PEN_MASK_ROTATION, PEN_MASK_TILT_X, PEN_MASK_TILT_Y, PT_PEN, WM_POINTERCAPTURECHANGED,
    WM_POINTERLEAVE, WM_POINTERUP,
};

use crate::dpi::PhysicalPosition;
use crate::event::{ElementState, MouseButton, WindowEvent};
use crate::platform::tablet::{self, TabletEvent, TabletPhase, TabletToolEvent};
use crate::platform_impl::platform::{WindowId, DEVICE_ID};

type GetPointerType = unsafe extern "system" fn(pointer_id: u32, kind: *mut i32) -> i32;
type GetPointerPenInfoHistory =
    unsafe extern "system" fn(pointer_id: u32, entries: *mut u32, info: *mut POINTER_PEN_INFO) -> i32;
type GetPointerPenInfo = unsafe extern "system" fn(pointer_id: u32, info: *mut POINTER_PEN_INFO) -> i32;
type GetPointerDeviceRects =
    unsafe extern "system" fn(device: HANDLE, device_rect: *mut RECT, display_rect: *mut RECT) -> i32;

struct Api {
    pointer_type: GetPointerType,
    pen_info: GetPointerPenInfo,
    pen_history: GetPointerPenInfoHistory,
    device_rects: GetPointerDeviceRects,
}

fn api() -> Option<&'static Api> {
    static API: std::sync::OnceLock<Option<Api>> = std::sync::OnceLock::new();
    API.get_or_init(|| {
        Some(Api {
            pointer_type: get_function!("user32.dll", GetPointerType)?,
            pen_info: get_function!("user32.dll", GetPointerPenInfo)?,
            pen_history: get_function!("user32.dll", GetPointerPenInfoHistory)?,
            device_rects: get_function!("user32.dll", GetPointerDeviceRects)?,
        })
    })
    .as_ref()
}

#[derive(Default)]
struct PenState {
    down: bool,
    performance_count: u64,
    position: PhysicalPosition<f64>,
}

thread_local! {
    static PENS: RefCell<HashMap<(HWND, u32), PenState>> = RefCell::new(HashMap::new());
}

/// Handles a pointer message if it comes from a pen. Returns whether it did.
pub(crate) fn handle(
    window: HWND,
    message: u32,
    pointer_id: u32,
    send: &mut dyn FnMut(WindowEvent),
) -> bool {
    let Some(api) = api() else { return false };
    let key = (window, pointer_id);
    if message == WM_POINTERCAPTURECHANGED {
        if PENS.with(|p| p.borrow().contains_key(&key)) {
            finish(window, pointer_id, TabletPhase::Cancel, send);
            return true;
        }
        return false;
    }
    let mut kind = 0;
    if unsafe { (api.pointer_type)(pointer_id, &mut kind) } == 0 || kind != PT_PEN {
        return false;
    }
    if message == WM_POINTERLEAVE {
        finish(window, pointer_id, TabletPhase::Leave, send);
        return true;
    }
    let mut latest: POINTER_PEN_INFO = unsafe { mem::zeroed() };
    if unsafe { (api.pen_info)(pointer_id, &mut latest) } == 0
        || latest.pointerInfo.pointerFlags & POINTER_FLAG_CANCELED != 0
    {
        finish(window, pointer_id, TabletPhase::Cancel, send);
        return true;
    }
    // Every sample since the last message, oldest first, bounded so a stalled loop cannot
    // allocate without limit.
    let mut samples = vec![latest];
    if latest.pointerInfo.historyCount > 1 {
        let mut count = latest.pointerInfo.historyCount.min(512);
        let mut history = vec![unsafe { mem::zeroed::<POINTER_PEN_INFO>() }; count as usize];
        if unsafe { (api.pen_history)(pointer_id, &mut count, history.as_mut_ptr()) } != 0 {
            history.truncate(count as usize);
            if !history.is_empty() {
                history.reverse();
                samples = history;
            }
        }
    }
    let last = samples.len() - 1;
    for (index, sample) in samples.into_iter().enumerate() {
        let info = &sample.pointerInfo;
        if info.pointerFlags & POINTER_FLAG_CANCELED != 0 {
            finish(window, pointer_id, TabletPhase::Cancel, send);
            break;
        }
        let Some(position) = client_position(api, window, info) else { continue };
        let (was_down, stale) = PENS.with(|p| {
            let pens = p.borrow();
            let pen = pens.get(&key);
            (
                pen.is_some_and(|p| p.down),
                info.PerformanceCount != 0
                    && pen.is_some_and(|p| p.performance_count >= info.PerformanceCount),
            )
        });
        if stale {
            continue;
        }
        let in_contact = info.pointerFlags & POINTER_FLAG_INCONTACT != 0;
        // A lift's message carries the samples before it too: only the newest one (or one
        // flagged as the lift) ends the stroke, so a fast lift keeps its stroke's tail.
        let lifted = (index == last && message == WM_POINTERUP) || info.pointerFlags & POINTER_FLAG_UP != 0;
        let phase = if lifted {
            if !was_down {
                continue;
            }
            TabletPhase::Up
        } else if in_contact && !was_down {
            TabletPhase::Down
        } else if in_contact {
            TabletPhase::Move
        } else {
            TabletPhase::Hover
        };
        PENS.with(|p| {
            let mut pens = p.borrow_mut();
            let pen = pens.entry(key).or_default();
            pen.down = matches!(phase, TabletPhase::Down | TabletPhase::Move);
            pen.performance_count = pen.performance_count.max(info.PerformanceCount);
            pen.position = position;
        });
        let mask = sample.penMask;
        tablet::emit(TabletEvent::Tool(TabletToolEvent {
            window_id: crate::window::WindowId(WindowId(window)),
            phase,
            position,
            pressure: if in_contact || phase == TabletPhase::Up {
                if mask & PEN_MASK_PRESSURE != 0 {
                    (sample.pressure as f32 / 1024.).clamp(0., 1.)
                } else {
                    0.5
                }
            } else {
                0.
            },
            tilt_x: if mask & PEN_MASK_TILT_X != 0 { sample.tiltX as f32 } else { 0. },
            tilt_y: if mask & PEN_MASK_TILT_Y != 0 { sample.tiltY as f32 } else { 0. },
            twist: (mask & PEN_MASK_ROTATION != 0).then_some(sample.rotation as f32),
            has_pressure: mask & PEN_MASK_PRESSURE != 0,
            has_tilt: mask & (PEN_MASK_TILT_X | PEN_MASK_TILT_Y) != 0,
            buttons: if sample.penFlags & PEN_FLAG_BARREL != 0 { tablet::BUTTON_LOWER } else { 0 },
            eraser: sample.penFlags & (PEN_FLAG_ERASER | PEN_FLAG_INVERTED) != 0,
            time: info.dwTime,
        }));
        emulate(window, phase, position, send);
    }
    true
}

fn finish(window: HWND, pointer_id: u32, phase: TabletPhase, send: &mut dyn FnMut(WindowEvent)) {
    let Some(pen) = PENS.with(|p| p.borrow_mut().remove(&(window, pointer_id))) else { return };
    let report = |phase| {
        tablet::emit(TabletEvent::Tool(TabletToolEvent {
            window_id: crate::window::WindowId(WindowId(window)),
            phase,
            position: pen.position,
            pressure: 0.,
            tilt_x: 0.,
            tilt_y: 0.,
            twist: None,
            has_pressure: false,
            has_tilt: false,
            buttons: 0,
            eraser: false,
            time: 0,
        }))
    };
    if pen.down && phase == TabletPhase::Leave {
        report(TabletPhase::Up);
        emulate(window, TabletPhase::Up, pen.position, send);
    }
    report(phase);
    emulate(window, phase, pen.position, send);
}

/// The mouse events a control sees for a pen report.
fn emulate(
    _window: HWND,
    phase: TabletPhase,
    position: PhysicalPosition<f64>,
    send: &mut dyn FnMut(WindowEvent),
) {
    // These events come from a pen message, not from mouse messages the system promoted from
    // a pen, which a host filters out by this thread's message extra info.
    unsafe { SetMessageExtraInfo(0) };
    let device_id = DEVICE_ID;
    match phase {
        TabletPhase::Hover | TabletPhase::Move => {
            send(WindowEvent::CursorMoved { device_id, position })
        },
        TabletPhase::Down | TabletPhase::Up => {
            send(WindowEvent::CursorMoved { device_id, position });
            let state =
                if phase == TabletPhase::Down { ElementState::Pressed } else { ElementState::Released };
            send(WindowEvent::MouseInput { device_id, state, button: MouseButton::Left });
        },
        TabletPhase::Leave | TabletPhase::Cancel => send(WindowEvent::CursorLeft { device_id }),
    }
}

/// The sample's position in physical client pixels, at the digitizer's sub-pixel precision.
fn client_position(api: &Api, window: HWND, info: &POINTER_INFO) -> Option<PhysicalPosition<f64>> {
    let mut device: RECT = unsafe { mem::zeroed() };
    let mut display: RECT = unsafe { mem::zeroed() };
    let (x, y) = if unsafe { (api.device_rects)(info.sourceDevice, &mut device, &mut display) } != 0
        && device.right > device.left
        && device.bottom > device.top
    {
        let rx = (display.right - display.left) as f64 / (device.right - device.left) as f64;
        let ry = (display.bottom - display.top) as f64 / (device.bottom - device.top) as f64;
        (
            display.left as f64 + (info.ptHimetricLocation.x - device.left) as f64 * rx,
            display.top as f64 + (info.ptHimetricLocation.y - device.top) as f64 * ry,
        )
    } else {
        (info.ptPixelLocation.x as f64, info.ptPixelLocation.y as f64)
    };
    let mut origin = POINT { x: 0, y: 0 };
    if unsafe { ScreenToClient(window, &mut origin) } == 0 {
        return None;
    }
    Some(PhysicalPosition::new(x + origin.x as f64, y + origin.y as f64))
}
