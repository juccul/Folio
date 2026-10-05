# Local compatibility and native Linux patches

These directories retain full upstream sources and notices.

## GPUI 0.2.2 — Apache-2.0

- `interactive.rs`/`window.rs`: rich `TabletEvent`, `TabletPadEvent` and `NavigationGesture` platform events, atomic sensor fields, input routing and native synthetic replay entry points.
- `platform/linux/wayland/client/tablet.rs`: tablet-v2 pressure/tilt/contact/proximity/tool buttons, eraser identity, removal cancellation; pad enter/leave, buttons and frame-delimited ring/strip deltas with ring wrap handling. Pointer-compatible events also operate GPUI controls; the application suppresses canvas duplicates.
- `platform/linux/wayland/client/gestures.rs`: native pointer-gestures pinch/rotation/translation and cancellation, separate from tablet input.
- `platform/linux/x11/client.rs`/`event.rs`: XInput2 pressure/tilt valuator discovery and normalization, eraser identity, button/contact/hover frames, hierarchy cancellation, correct 16.16 coordinates and allocation-free valuator lookup.
- `path_builder.rs`: use 32-bit tessellation indices so large compound ink paths do not disappear above 65,535 vertices.
- `scene.rs`: transform cached vector paths without tessellating again; affine polychrome sprites with transformed culling bounds.
- `window.rs`/`platform/blade/shaders.wgsl`: GPU affine image painting and transformed clipping. PDF/images/text stay aligned with ink during rotation and drag transforms. This renderer extension targets Linux's Blade backend.
- `elements/img.rs`: expose cached asynchronous raster loading to the canvas.
- `taffy.rs`: f32 casts for the resolved Taffy layout version.

Tablet child-event registration uses generated Wayland opcode constants. The isolated protocol regression in `scripts/verify-tablet-protocol.py` checks the actual registration macros against protocol metadata; it catches the 0.1.0 pad-group opcode error without touching the desktop.

No privileged evdev access or handwriting network transport is introduced. Native APIs and synthetic replay still need physical tablet/HiDPI/palm trials. Upstreaming the input/renderer changes and replacing the vendor fork with a maintained release remain priorities.

## xattr 0.2.3 — MIT or Apache-2.0

Use Linux/Android `ENODATA` instead of libc's removed `ENOATTR` alias.

## proc-macro-error2 2.0.1 — MIT or Apache-2.0

Make the already publicly re-exported `proc_macro` crate public to fix Rust's private-reexport warning.

All original notices remain in these directories and `third_party/licenses`. Cargo.lock identifies exact versions.

Reusable native controls refine existing hover styles rather than asserting that only one hover builder exists. This fixes debug-build panics when themed document tabs override the base control background. Existing hover properties are retained unless explicitly overridden.
