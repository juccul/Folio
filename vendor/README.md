# Local compatibility and native platform patches

These directories retain full upstream sources and notices.

## GPUI 0.2.2 — Apache-2.0

- `interactive.rs`/`window.rs`: rich `TabletEvent`, `TabletPadEvent` and `NavigationGesture` platform events, atomic sensor fields, input routing and native synthetic replay entry points.
- `platform/linux/wayland/client/tablet.rs`: tablet-v2 pressure/tilt/contact/proximity/tool buttons, eraser identity, removal cancellation; pad enter/leave, buttons and frame-delimited ring/strip deltas with ring wrap handling. Pointer-compatible events also operate GPUI controls; the application suppresses canvas duplicates.
- `platform/linux/wayland/client/gestures.rs`: native pointer-gestures pinch/rotation/translation and cancellation, separate from tablet input.
- `platform/linux/x11/client.rs`/`event.rs`: XInput2 pressure/tilt valuator discovery and normalization, eraser identity, button/contact/hover frames, hierarchy cancellation, correct 16.16 coordinates and allocation-free valuator lookup.
- `path_builder.rs`: use 32-bit tessellation indices so large compound ink paths do not disappear above 65,535 vertices.
- `scene.rs`: transform cached vector paths without tessellating again; affine polychrome sprites with transformed culling bounds.
- `paint_effect.rs`: `effect()` applies a fade, offset and scale to everything an element paints and keeps a `PaintSnapshot` tree of it (nested effects preserved); `snapshot()` repaints one later with new effects, skipping sprites whose atlas texture was released (`PlatformAtlas::contains_texture`). Used for enter/exit transitions.
- `window.rs`/`platform/blade/shaders.wgsl`: GPU affine image painting and transformed clipping. PDF/images/text stay aligned with ink during rotation and drag transforms. This renderer extension targets Linux's Blade backend.
- `elements/img.rs`: expose cached asynchronous raster loading to the canvas, upload large decoded single-frame rasters through ephemeral BGRA buffers (retaining CPU bytes for tiny `<= 64 KiB` SVG icons), convert RGB/Luma directly to BGRA without an intermediate RGBA buffer, and serialize large image decodes to avoid peak `VmHWM` spikes.
- `assets.rs`/`window.rs`: add `RenderImage::from_ephemeral_bgra` and `RenderImage::from_ephemeral_shared_bgra` so single-frame still images release their CPU pixel buffers on first GPU atlas upload without cloning shared buffers (falling back to retained `Shared` storage on Windows so D3D11 device-lost recovery can re-upload).
- `platform.rs`/`window.rs`/`platform/linux/wayland/window.rs`: schedule Wayland `wl_surface.frame` callbacks on demand via `PlatformWindow::frame_notifier` instead of unconditionally re-arming at monitor refresh rate (including `appearance_changed` refreshes, `Paint`/`Focus` invalidation, and `set_hovered` state tracking), letting idle windows sleep at 0 Hz; apply initial `WindowParams.titlebar.title` via `xdg_toplevel.set_title` before first commit.
- `platform/linux/dispatcher.rs`: cap the background worker thread pool to `2..=4` threads instead of one per hardware thread.
- `text_system.rs`/`platform/linux/text_system.rs`: match font faces by `fontdb` metadata and memory-map only the winning face instead of eagerly mapping every face in a family, clear font ID caches on `add_fonts`, and avoid removing non-Latin/symbol faces from `fontdb`.
- `platform/blade/blade_renderer.rs`/`platform/linux/wayland/window.rs`: during an interactive Wayland resize the swapchain keeps spare capacity in 64-pixel steps and `wp_viewport` crops it, so most resize steps do not recreate the swapchain; the exact size returns when the resize ends.
- `platform/blade/blade_context.rs`/`platform/linux/wayland/client.rs`: on a multi-GPU Linux machine, prefer the Wayland compositor's `zwp_linux_dmabuf_v1` `main_device` GPU before falling back to firmware `boot_vga` unless `ZED_DEVICE_ID`, `DRI_PRIME`, `MESA_VK_DEVICE_SELECT`, `__NV_PRIME_RENDER_OFFLOAD`, `VK_ICD_FILENAMES`, or `VK_DRIVER_FILES` chooses one; fall back to the default device if it is unusable; stop the Wayland event loop cleanly on fatal compositor protocol errors instead of spinning at 100% CPU.
- `taffy.rs`: f32 casts for the resolved Taffy layout version.
- `platform.rs`/`window.rs`/`platform/linux/{wayland,x11}/window.rs`: `activate_window_with_token` activates with a launcher's xdg-activation token (Wayland) or startup-ID timestamp (X11) for single-instance handoff.
- `platform/windows/pen.rs`/`events.rs`/`window.rs`: Windows Ink pointer history with pressure, tilt, eraser/barrel identity, cancellation, DPI conversion, duplicate mouse filtering, and compatible control events; deferred native title-bar movement and edge resizing.
- `platform/windows/shaders.hlsl`: affine polychrome image transforms and local rounded clipping, matching the shared scene layout.
- `platform/windows/window.rs`: an application-owned `WM_GETOBJECT` handler for the AccessKit Windows accessibility adapter.
- `platform/windows/destination_list.rs`: bounded UTF-16 length discovery without an ICU import dependency. The application owns its Windows manifest and resources.

Tablet child-event registration uses generated Wayland opcode constants. The isolated protocol regression in `scripts/verify-tablet-protocol.py` checks the actual registration macros against protocol metadata; it catches the 0.1.0 pad-group opcode error without touching the desktop.

No privileged evdev access or handwriting network transport is introduced. Native APIs and synthetic replay still need physical tablet/HiDPI/palm trials. Upstreaming the input/renderer changes and replacing the vendor fork with a maintained release remain priorities.

## xattr 0.2.3 — MIT or Apache-2.0

Use Linux/Android `ENODATA` instead of libc's removed `ENOATTR` alias.

## proc-macro-error2 2.0.1 — MIT or Apache-2.0

Make the already publicly re-exported `proc_macro` crate public to fix Rust's private-reexport warning.

All original notices remain in these directories and `third_party/licenses`. Cargo.lock identifies exact versions.

Reusable native controls refine existing hover styles rather than asserting that only one hover builder exists. This fixes debug-build panics when themed document tabs override the base control background. Existing hover properties are retained unless explicitly overridden.
