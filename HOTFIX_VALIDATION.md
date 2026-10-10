# Windows controls and Linux resizing

The starting checkout was fast-forwarded to `origin/main` at `c5e8e49`.

## Changes

- `codex/windows-linux-fixes`: Home and update-notice pointer isolation plus
  Linux rendering changes.
- `codex/backport-windows-0.1.3`: based on `v0.1.3`; includes the pointer fix and
  the existing 0.1.4 Windows preparation-progress fix (`4683bf8`).
- `codex/backport-windows-0.1.4`: based on `v0.1.4`; includes the pointer fix.
  Preparation progress was already present. Neither backport contains the Linux
  rendering changes.

See [WINDOWS_HOTFIX.md](WINDOWS_HOTFIX.md) for the pointer failure and release
rebuild requirements.

On X11, ConfigureNotify updates logical bounds immediately but defers the
geometry query, GPU wait and swapchain recreation until presentation. Multiple
intermediate sizes therefore share one drawable update per frame. Move-only
events no longer query/reconfigure the drawable or call the resize callback.
The window manager's synchronization counter is acknowledged only after drawing
and completing the GPU work for the resized frame.

On Wayland, throttled interactive configures retain their size and serial
together. The next frame applies the newest pending configure; a subsequent
configure supersedes it. The previous code dropped the size while throttling and
could leave the last requested size unapplied until another event arrived.

## Linux native regression

`scripts/verify-x11-resize.py` runs only on an explicitly owned virtual display
with a disposable library. It hides the window to suspend rendering, sends a
window-manager synchronization request and resizes the window. The old binary
failed because it acknowledged the resize while rendering remained suspended.
The fixed binary kept the old counter value until remapping and rendering, then
acknowledged the new value. It also handled a 201-event resize burst, settled at
1180 by 740, and remained responsive to a native save shortcut. The final client
capture was inspected and showed the correctly sized page and title bar.

The expanded native smoke replay passed Home and update-notice pointer presses,
pressure/tilt, drawing, undo/redo, pages, menus, tab navigation/reordering and
durable saving. The harness now allows 90 seconds for debug builds on a software
Vulkan virtual display; its old 30-second allowance expired during the replay.
Title bar pointer checks run after the drawing fixture so opening the library
does not disturb its setup. Notice checks wait for rendered controls instead of
assuming a fixed 200-millisecond layout delay.

## Source checks and limits

The focused command is `cargo test -p folio-update -p folio-ui -p folio --locked`.
Current source and the 0.1.4 backport passed 73 tests each, with two existing
optional tests ignored. The 0.1.3 backport passed 71, with the same two optional
tests ignored. Python Windows
updater/package checks passed: 14 on current source and 0.1.4, 10 on 0.1.3.
Formatting and patch whitespace checks passed.
Both backport application builds and their native X11 smoke replays passed.

Strict workspace Clippy stopped at the pre-existing `clippy::let_and_return`
diagnostic in `crates/storage/src/recovery.rs:218` under Rust 1.92. A focused run
allowing that diagnostic also encountered the existing
`clippy::collapsible_else_if` in `apps/desktop/build_linux.rs:11`. Those unrelated sources
were left unchanged.
A normal focused Clippy run completed with existing warnings in those files,
`crates/app/src/handwriting_search.rs`, `crates/app/src/lib.rs` and the vendored
GPUI surface code; none pointed to a hotfix change.

This workspace has Linux and software Vulkan. Native Windows installation,
download/restart and physical compositor flicker/latency are not established by
these checks. Wayland changes compile with the normal application build but
still need a native compositor resize run. Published tags, release assets and
signed manifests were not changed.
