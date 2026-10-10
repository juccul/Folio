//! Native window controls and compositor-driven movement/resizing. The tab
//! strip consumes only background presses; tabs keep their own drag behavior.
use super::*;

impl NotesView {
    pub fn titlebar_smoke_verify_home(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if !self.library_open {
            return Err("The title bar Home button did not open the library".into());
        }
        self.show_editor();
        cx.notify();
        Ok(())
    }

    pub(super) fn window_controls(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let capabilities = window.window_controls();
        let maximized = window.is_maximized();
        let mut controls = div().flex().items_center().gap_1();
        if capabilities.minimize {
            controls = controls.child(
                self.control(
                    "window-minimize",
                    "Minimize window",
                    window_icon(Icon::Minimize, theme.muted).into_any_element(),
                    false,
                    cx,
                    |_, window, _| window.minimize_window(),
                )
                .w(px(34.))
                .h(px(30.))
                .p_0()
                .flex_shrink_0()
                .bg(rgb(theme.chrome)),
            );
        }
        if capabilities.maximize {
            controls = controls.child(
                self.control(
                    "window-maximize",
                    if maximized {
                        "Restore window"
                    } else {
                        "Maximize window"
                    },
                    window_icon(
                        if maximized {
                            Icon::Restore
                        } else {
                            Icon::Maximize
                        },
                        theme.muted,
                    )
                    .into_any_element(),
                    false,
                    cx,
                    |_, window, _| {
                        window.zoom_window();
                        window.refresh();
                    },
                )
                .w(px(34.))
                .h(px(30.))
                .p_0()
                .flex_shrink_0()
                .bg(rgb(theme.chrome)),
            );
        }
        controls.child(
            self.control(
                "window-close",
                "Close window",
                window_icon(Icon::Close, theme.muted).into_any_element(),
                false,
                cx,
                |this, window, cx| {
                    this.finish_inline_text(window, cx);
                    this.store_workspace();
                    match this.controller.flush() {
                        Ok(()) => window.remove_window(),
                        Err(error) => {
                            this.controller.error = Some(error);
                            cx.notify();
                        }
                    }
                },
            )
            .w(px(34.))
            .h(px(30.))
            .p_0()
            .flex_shrink_0()
            .bg(rgb(theme.chrome)),
        )
    }
}

// Window glyphs share a 12px optical footprint and stroke weight. General
// toolbar SVGs have different intrinsic bounds, so equal SVG boxes do not
// produce equally sized minimize/maximize/close marks.
fn window_icon(kind: Icon, color: u32) -> impl IntoElement {
    let lines: &[&[(f32, f32)]] = match kind {
        Icon::Minimize => &[&[(3., 9.), (15., 9.)]],
        Icon::Maximize => &[&[(3., 3.), (15., 3.), (15., 15.), (3., 15.), (3., 3.)]],
        Icon::Restore => &[
            &[(6., 6.), (6., 3.), (15., 3.), (15., 12.), (12., 12.)],
            &[(3., 6.), (12., 6.), (12., 15.), (3., 15.), (3., 6.)],
        ],
        Icon::Close => &[&[(3., 3.), (15., 15.)], &[(15., 3.), (3., 15.)]],
        _ => unreachable!("Not a window control"),
    };
    let paths = lines
        .iter()
        .filter_map(|line| {
            let mut path = PathBuilder::stroke(px(1.4));
            path.move_to(point(px(line[0].0), px(line[0].1)));
            for &(x, y) in &line[1..] {
                path.line_to(point(px(x), px(y)));
            }
            path.build().ok()
        })
        .collect::<Vec<_>>();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for path in &paths {
                window.paint_path(
                    path.clone().transformed([
                        1.,
                        0.,
                        0.,
                        1.,
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y),
                    ]),
                    rgb(color),
                );
            }
        },
    )
    .size(px(18.))
    .flex_shrink_0()
}

pub(super) fn background_press(event: &MouseDownEvent, window: &mut Window, cx: &mut App) {
    if event.click_count == 2 && window.window_controls().maximize {
        window.zoom_window();
        window.refresh();
    } else if event.click_count == 1 {
        window.start_window_move();
    }
    cx.stop_propagation();
}

pub(super) fn frame(content: impl IntoElement, window: &mut Window) -> Div {
    // Opaque chrome needs no invisible compositor/shadow inset. Keep resize
    // hitboxes inside the client bounds, without changing canvas coordinates.
    window.set_client_inset(px(0.));
    let mut frame = div().relative().size_full().child(content);
    if !window.is_maximized()
        && !window.is_fullscreen()
        && let Decorations::Client { tiling } = window.window_decorations()
    {
        let size = window.viewport_size();
        for (edge, bounds) in resize_regions(size, tiling) {
            let cursor = match edge {
                ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
                ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
                ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
            };
            frame = frame.child(
                div()
                    .absolute()
                    .occlude()
                    .left(bounds.origin.x)
                    .top(bounds.origin.y)
                    .w(bounds.size.width)
                    .h(bounds.size.height)
                    .cursor(cursor)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.start_window_resize(edge);
                        cx.stop_propagation();
                    }),
            );
        }
    }
    frame
}

fn resize_regions(size: Size<Pixels>, tiling: Tiling) -> Vec<(ResizeEdge, Bounds<Pixels>)> {
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let (corner, border) = (8., 4.);
    let rect = |x, y, w, h| Bounds::new(point(px(x), px(y)), gpui::size(px(w), px(h)));
    let mut regions = vec![];
    if !tiling.top {
        regions.push((
            ResizeEdge::Top,
            rect(corner, 0., width - corner * 2., border),
        ));
    }
    if !tiling.bottom {
        regions.push((
            ResizeEdge::Bottom,
            rect(corner, height - border, width - corner * 2., border),
        ));
    }
    if !tiling.left {
        regions.push((
            ResizeEdge::Left,
            rect(0., corner, border, height - corner * 2.),
        ));
    }
    if !tiling.right {
        regions.push((
            ResizeEdge::Right,
            rect(width - border, corner, border, height - corner * 2.),
        ));
    }
    for (edge, x, y, available) in [
        (ResizeEdge::TopLeft, 0., 0., !tiling.top && !tiling.left),
        (
            ResizeEdge::TopRight,
            width - corner,
            0.,
            !tiling.top && !tiling.right,
        ),
        (
            ResizeEdge::BottomLeft,
            0.,
            height - corner,
            !tiling.bottom && !tiling.left,
        ),
        (
            ResizeEdge::BottomRight,
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
    #[::core::prelude::v1::test]
    fn tiled_edges_and_corners_have_no_resize_hitboxes() {
        let size = size(px(1000.), px(700.));
        assert_eq!(resize_regions(size, Tiling::default()).len(), 8);
        assert!(resize_regions(size, Tiling::tiled()).is_empty());
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
                ResizeEdge::Right | ResizeEdge::Bottom | ResizeEdge::BottomRight
            ) && bounds.origin.x >= px(8.)
                && bounds.origin.y >= px(8.)
        }));
    }
}
