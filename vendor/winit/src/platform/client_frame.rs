//! Folio extension: windows that draw their own frame on Linux.
//!
//! [`set_enabled`] makes decorated windows created afterwards frameless (Wayland asks for
//! client-side decorations and skips winit's drawn frame; X11 sets no-decoration hints) while
//! they otherwise stay ordinary windows: they minimize, maximize and report as decorated to
//! the toolkit above. The application then draws its title bar, starts edge resizing with
//! [`drag_resize`], opens the system window menu with [`show_window_menu`] and reads
//! [`state`] to leave resize edges off maximized, full-screen and tiled windows.
//!
//! Requests apply to the most recently created such window that is still open, and must be
//! made on the event loop thread, during the button press that starts them.

use crate::dpi::LogicalPosition;
use crate::window::ResizeDirection;

/// Which window edges touch a screen edge or another window, so they do not resize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tiling {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// What a client-framed window's frame should leave out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameState {
    pub maximized: bool,
    pub fullscreen: bool,
    pub tiling: Tiling,
}

/// Whether decorated windows created from now on draw their own frame. Linux only; elsewhere
/// it records the choice and changes nothing.
pub fn set_enabled(enabled: bool) {
    imp::set_enabled(enabled)
}

/// Whether windows draw their own frame: [`set_enabled`] on Linux.
pub fn enabled() -> bool {
    imp::enabled()
}

/// The frame state of the client-framed window, if one is open.
pub fn state() -> Option<FrameState> {
    imp::state()
}

/// Starts a compositor-driven resize from `direction`'s edge. Returns whether a window took it.
pub fn drag_resize(direction: ResizeDirection) -> bool {
    imp::drag_resize(direction)
}

/// Opens the system window menu at `position`, in logical points from the window's content
/// origin, where the platform has one (Wayland; X11 window managers that support
/// `_GTK_SHOW_WINDOW_MENU`).
pub fn show_window_menu(position: LogicalPosition<f64>) {
    imp::show_window_menu(position)
}

#[cfg(any(x11_platform, wayland_platform))]
use crate::platform_impl::client_frame as imp;

#[cfg(not(any(x11_platform, wayland_platform)))]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    static ENABLED: AtomicBool = AtomicBool::new(false);
    pub fn set_enabled(enabled: bool) {
        ENABLED.store(enabled, Ordering::Relaxed)
    }
    pub fn enabled() -> bool {
        false
    }
    pub fn state() -> Option<FrameState> {
        None
    }
    pub fn drag_resize(_: ResizeDirection) -> bool {
        false
    }
    pub fn show_window_menu(_: LogicalPosition<f64>) {}
}
