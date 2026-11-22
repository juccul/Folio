//! Native window controls and compositor-driven movement/resizing. The tab
//! strip consumes only background presses; tabs keep their own drag behavior.
use super::*;

impl NotesView {
    /// Closing would leave edits unsaved because autosave is off.
    pub(super) fn needs_close_prompt(&self) -> bool {
        !self.controller.settings.autosave && self.controller.has_unsaved_documents()
    }
    /// Close the window, saving document edits or discarding those that
    /// autosave-off deferred. Folders and preferences are always kept.
    pub(super) fn close_window(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_inline_text(cx);
        self.store_workspace();
        let result = if save {
            self.controller.flush()
        } else {
            self.controller.close_discarding_documents()
        };
        match result {
            Ok(()) => {
                self.closing = true;
                window.remove_window();
            }
            Err(error) => {
                self.modal = None;
                self.controller.error = Some(error);
                cx.notify();
            }
        }
    }
    /// The window's controls: on Linux, where Folio draws the frame, its own
    /// minimize, maximize/restore and close buttons; elsewhere room for the
    /// caption buttons Rune draws over the titlebar band (Windows, macOS).
    pub(super) fn window_controls(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        if !client_frame() {
            return div().w(px(self.chrome_trailing)).flex_shrink_0();
        }
        let theme = Theme::new(&self.controller.settings);
        let maximized = window.is_maximized();
        let button = |control: Stateful<Div>| {
            control
                .w(px(34.))
                .h(px(30.))
                .min_h(px(30.))
                .p_0()
                .flex_shrink_0()
        };
        div()
            .flex()
            .items_center()
            .gap_1()
            .pr(px(4.))
            .child(button(self.control(
                "window-minimize",
                "Minimize window",
                window_icon(Icon::WindowMinimize, theme.muted).into_any_element(),
                false,
                cx,
                |_, window, _| window.minimize_window(),
            )))
            .child(button(
                self.control(
                    "window-maximize",
                    if maximized {
                        "Restore window"
                    } else {
                        "Maximize window"
                    },
                    window_icon(
                        if maximized {
                            Icon::WindowRestore
                        } else {
                            Icon::WindowMaximize
                        },
                        theme.muted,
                    )
                    .into_any_element(),
                    false,
                    cx,
                    |_, window, _| window.zoom_window(),
                ),
            ))
            .child(
                button(self.control(
                    "window-close",
                    "Close window",
                    window_icon(Icon::WindowClose, theme.muted).into_any_element(),
                    false,
                    cx,
                    |this, window, cx| {
                        this.commit_inline_text(cx);
                        if this.needs_close_prompt() {
                            this.modal(super::Modal::UnsavedChanges, window, cx);
                        } else {
                            this.close_window(true, window, cx);
                        }
                    },
                ))
                .hover(|s| s.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff))),
            )
    }
}

/// Whether Folio draws the window's frame: on Linux, unless
/// `FOLIO_SYSTEM_FRAME=1` keeps the system's.
pub(crate) fn client_frame() -> bool {
    winit::platform::client_frame::enabled()
}

/// Asks the platform layer to leave the frame to Folio, before the window
/// opens. Linux only.
pub(crate) fn request_client_frame() {
    if cfg!(target_os = "linux") && std::env::var_os("FOLIO_SYSTEM_FRAME").is_none() {
        winit::platform::client_frame::set_enabled(true);
    }
}

/// Whether the window is maximized, from the platform layer where Folio
/// draws the frame.
pub(crate) fn window_maximized() -> bool {
    winit::platform::client_frame::state().is_some_and(|s| s.maximized)
}

// Window glyphs share a 12px optical footprint and stroke weight.
fn window_icon(kind: Icon, color: u32) -> impl IntoElement {
    icon(kind, color).size(px(18.))
}

/// Empty titlebar chrome: a press moves the window, a double press zooms it,
/// and a secondary click opens the system window menu where Folio draws the
/// frame.
pub(super) fn drag_region() -> Div {
    div().id("titlebar-drag-region").flex_1().h_full().el(|e| {
        let e = e.window_drag();
        if !client_frame() {
            return e;
        }
        e.on_secondary_press(
            |_: &mut AppState, press: rune::PointerPress, _: &mut rune::Tx| {
                winit::platform::client_frame::show_window_menu(winit::dpi::LogicalPosition::new(
                    press.x as f64,
                    press.y as f64,
                ));
            },
        )
    })
}

/// The window's content with, where Folio draws the frame, edge and corner
/// regions that resize the window. Maximized, full-screen and tiled edges
/// have none.
pub(super) fn frame(content: impl IntoElement, window: &mut Window) -> Div {
    let mut frame = div().relative().size_full().child(content);
    let Some(state) = winit::platform::client_frame::state() else {
        return frame;
    };
    if state.maximized || state.fullscreen {
        return frame;
    }
    for (edge, bounds) in resize_regions(window.viewport_size(), state.tiling) {
        let cursor = match edge {
            ResizeDirection::North | ResizeDirection::South => rune::Cursor::ResizeRow,
            // Rune has no diagonal resize cursors; corners show the column one.
            _ => rune::Cursor::ResizeColumn,
        };
        frame = frame.child(
            div()
                .id(format!("window-resize-{edge:?}"))
                .absolute()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .el(move |e| {
                    e.cursor(cursor).drag_threshold(0.).on_drag(
                        move |_: &mut AppState, drag: &mut rune::Drag, _: &mut rune::Tx| {
                            if drag.phase == rune::Phase::Down {
                                winit::platform::client_frame::drag_resize(edge);
                            }
                        },
                    )
                }),
        );
    }
    frame
}

use winit::window::ResizeDirection;

fn resize_regions(
    size: Size<Pixels>,
    tiling: winit::platform::client_frame::Tiling,
) -> Vec<(ResizeDirection, Bounds<Pixels>)> {
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let (corner, border) = (8., 4.);
    let rect = |x, y, w, h| Bounds::new(point(px(x), px(y)), super::size(px(w), px(h)));
    let mut regions = vec![];
    if !tiling.top {
        regions.push((
            ResizeDirection::North,
            rect(corner, 0., width - corner * 2., border),
        ));
    }
    if !tiling.bottom {
        regions.push((
            ResizeDirection::South,
            rect(corner, height - border, width - corner * 2., border),
        ));
    }
    if !tiling.left {
        regions.push((
            ResizeDirection::West,
            rect(0., corner, border, height - corner * 2.),
        ));
    }
    if !tiling.right {
        regions.push((
            ResizeDirection::East,
            rect(width - border, corner, border, height - corner * 2.),
        ));
    }
    for (edge, x, y, available) in [
        (
            ResizeDirection::NorthWest,
            0.,
            0.,
            !tiling.top && !tiling.left,
        ),
        (
            ResizeDirection::NorthEast,
            width - corner,
            0.,
            !tiling.top && !tiling.right,
        ),
        (
            ResizeDirection::SouthWest,
            0.,
            height - corner,
            !tiling.bottom && !tiling.left,
        ),
        (
            ResizeDirection::SouthEast,
            width - corner,
            height - corner,
            !tiling.bottom && !tiling.right,
        ),
    ] {
        if available {
            regions.push((edge, rect(x, y, corner, corner)));
        }
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::platform::client_frame::Tiling;

    #[test]
    fn tiled_edges_and_corners_have_no_resize_regions() {
        let size = size(px(1000.), px(700.));
        assert_eq!(resize_regions(size, Tiling::default()).len(), 8);
        let tiled = Tiling {
            top: true,
            bottom: true,
            left: true,
            right: true,
        };
        assert!(resize_regions(size, tiled).is_empty());
        let regions = resize_regions(
            size,
            Tiling {
                top: true,
                left: true,
                ..Default::default()
            },
        );
        assert_eq!(regions.len(), 3);
        assert!(regions.iter().all(|(edge, bounds)| {
            matches!(
                edge,
                ResizeDirection::East | ResizeDirection::South | ResizeDirection::SouthEast
            ) && bounds.origin.x >= px(8.)
                && bounds.origin.y >= px(8.)
        }));
    }
}
