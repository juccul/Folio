# Local compatibility and native platform patches

These directories retain full upstream sources and notices.

## winit 0.30.13 (Rune's fork, revision dd672fc) — Apache-2.0

The workspace patches `winit` to `vendor/winit`, Rune's patched winit with Folio's tablet extension on top. Only the files below differ from Rune's copy:

- `src/platform/tablet.rs`: the public `winit::platform::tablet` side channel (pen reports, pad buttons/rings/strips, proximity state) and ring wrap arithmetic.
- `src/platform_impl/linux/wayland/seat/tablet.rs` with `seat/mod.rs`, `state.rs` and `window/state.rs`: `zwp_tablet_v2` tools (pressure, tilt, rotation, eraser, barrel buttons, removal cancellation), pointer emulation for the window's controls, pen moves and clicks on client-side decorations, `wp_cursor_shape_v1` tool cursors that follow the window cursor, and pads with frame-delimited ring/strip deltas.
- `src/platform_impl/linux/wayland/seat/pointer/gestures.rs`: `zwp_pointer_gestures_v1` pinch and rotation as `PinchGesture`/`RotationGesture` window events.
- `src/platform_impl/linux/x11/tablet.rs` with `event_processor.rs` and `mod.rs`: XInput2 pressure/tilt valuator discovery by label, normalization, tip/barrel/pad button routing and proximity loss when another device moves the pointer.
- `src/platform/client_frame.rs` with `platform_impl/linux/client_frame.rs`, `linux/mod.rs`, `x11/window.rs`, `x11/atoms.rs`, `wayland/window/state.rs` and `wayland/seat/pointer/mod.rs`: frameless decorated windows on Linux that stay minimizable for the toolkit, edge resizing and window-menu requests (`_GTK_SHOW_WINDOW_MENU` on X11), maximized/full-screen/tiled state, and title-bar moves deferred until the pointer travels 4 points.
- `src/platform_impl/windows/pen.rs` with `event_loop.rs`: Windows Ink pen history with sub-pixel himetric positions, pressure, tilt, twist, eraser and barrel identity, cancellation, and mouse emulation instead of touch events.

Run the extension's unit tests from `vendor/winit` with Rune's `sctk-adwaita` patch: `cargo test --lib tablet --config 'patch.crates-io.sctk-adwaita.git="https://github.com/anomalyco/rune"' --config 'patch.crates-io.sctk-adwaita.rev="dd672fc8257a1bfb55d2e8595f585fc411e709df"'`.

## rune-image 0.0.1 (Rune, revision dd672fc) — MIT

The workspace patches Rune's `rune-image` to `vendor/rune-image`, an unchanged copy except:

- `Cargo.toml`: the dependency versions Rune's workspace supplied, written out, and no workspace lints.
- `src/pixels.rs`: fitted placement no longer clamps a drawn image to at least one point inside a frame narrower or shorter than a point (an image in a menu that grows from or shrinks to nothing), which panicked because `f32::clamp`'s lower bound passed its upper one.

All original notices remain in these directories and `third_party/licenses`. Cargo.lock identifies exact versions.

Reusable native controls refine existing hover styles rather than asserting that only one hover builder exists. This fixes debug-build panics when themed document tabs override the base control background. Existing hover properties are retained unless explicitly overridden.
