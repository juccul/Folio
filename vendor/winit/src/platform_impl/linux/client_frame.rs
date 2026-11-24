//! Folio extension: the Linux side of `platform::client_frame`.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::dpi::{LogicalPosition, Position};
use crate::platform::client_frame::{FrameState, Tiling};
use crate::window::ResizeDirection;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// A client-framed window, without keeping it open.
enum Handle {
    #[cfg(wayland_platform)]
    Wayland(std::sync::Weak<std::sync::Mutex<super::wayland::WindowState>>),
    #[cfg(x11_platform)]
    X(std::sync::Weak<super::x11::UnownedWindow>),
}

thread_local! {
    static WINDOWS: RefCell<Vec<Handle>> = const { RefCell::new(Vec::new()) };
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Whether the pointer travelled far enough from `start` to start a deferred window move.
pub(crate) fn past_drag_slop(start: (f64, f64), at: (f64, f64)) -> bool {
    (at.0 - start.0).hypot(at.1 - start.1) > 4.
}

pub(super) fn register(window: &super::Window) {
    let handle = match window {
        #[cfg(wayland_platform)]
        super::Window::Wayland(w) => Handle::Wayland(w.frame_state()),
        #[cfg(x11_platform)]
        super::Window::X(w) => Handle::X(w.unowned()),
    };
    WINDOWS.with(|windows| windows.borrow_mut().push(handle));
}

/// Runs `f` on the newest client-framed window still open, forgetting closed ones.
fn with_window<R>(f: impl FnOnce(&Handle) -> Option<R>) -> Option<R> {
    WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        windows.retain(|handle| match handle {
            #[cfg(wayland_platform)]
            Handle::Wayland(w) => w.strong_count() > 0,
            #[cfg(x11_platform)]
            Handle::X(w) => w.strong_count() > 0,
        });
        windows.last().and_then(f)
    })
}

pub fn state() -> Option<FrameState> {
    with_window(|handle| match handle {
        #[cfg(wayland_platform)]
        Handle::Wayland(w) => {
            let w = w.upgrade()?;
            let state = w.lock().unwrap();
            let configure = state.last_configure.as_ref();
            let flag = |f: fn(&sctk::shell::xdg::window::WindowConfigure) -> bool| {
                configure.is_some_and(f)
            };
            let tiled = flag(|c| c.is_tiled());
            Some(FrameState {
                maximized: flag(|c| c.is_maximized()),
                fullscreen: flag(|c| c.is_fullscreen()),
                tiling: Tiling {
                    top: tiled || flag(|c| c.is_tiled_top()),
                    bottom: tiled || flag(|c| c.is_tiled_bottom()),
                    left: tiled || flag(|c| c.is_tiled_left()),
                    right: tiled || flag(|c| c.is_tiled_right()),
                },
            })
        },
        #[cfg(x11_platform)]
        Handle::X(w) => {
            let w = w.upgrade()?;
            Some(FrameState {
                maximized: w.is_maximized(),
                fullscreen: w.fullscreen().is_some(),
                tiling: Tiling::default(),
            })
        },
    })
}

pub fn drag_resize(direction: ResizeDirection) -> bool {
    with_window(|handle| match handle {
        #[cfg(wayland_platform)]
        Handle::Wayland(w) => w.upgrade()?.lock().unwrap().drag_resize_window(direction).ok(),
        #[cfg(x11_platform)]
        Handle::X(w) => w.upgrade()?.drag_resize_window(direction).ok(),
    })
    .is_some()
}

pub fn show_window_menu(position: LogicalPosition<f64>) {
    with_window(|handle| match handle {
        #[cfg(wayland_platform)]
        Handle::Wayland(w) => {
            let position = LogicalPosition::new(position.x.max(0.) as u32, position.y.max(0.) as u32);
            w.upgrade()?.lock().unwrap().show_window_menu(position);
            Some(())
        },
        #[cfg(x11_platform)]
        Handle::X(w) => {
            w.upgrade()?.show_window_menu(Position::Logical(position));
            Some(())
        },
    });
}
