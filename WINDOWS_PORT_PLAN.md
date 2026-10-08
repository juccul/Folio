# Windows port plan

Initial repository audit: 2026-10-08. The findings below describe the starting point. The port has since been implemented and tested with native Windows builds in **Ransom**; [WINDOWS.md](WINDOWS.md) records the delivered build/package workflow and validation. Physical pen testing remains pending.

## Recommended scope

Keep the Rust workspace, GPUI UI, document format, SQLite storage, vector ink, and existing worker protocols. Port the operating-system boundaries and the vendored GPUI extensions.

Start with Windows 11 x64 using `x86_64-pc-windows-msvc`. Deliver a portable ZIP for testing, followed by a per-user installer. ARM64, Windows 10 certification, Microsoft Store distribution, and legacy WinTab support are follow-up decisions. Windows Ink pen support is part of the initial release; a mouse-only build is an engineering milestone.

The first supported release should provide handwriting, library/editing/search, persistence/recovery, PDF/image import and export, offline math, accessibility, and optional first-use OCR. Core note taking must continue to work without Python, model downloads, or a network connection.

## Findings that determine the work

| Area | Current repository evidence | Required work |
| --- | --- | --- |
| Windowing/rendering | `vendor/gpui/src/platform/windows/` already contains DirectX, DirectWrite, clipboard, dialogs, and window support. | Reuse this backend and verify Folio's custom extensions. |
| Build | Workspace GPUI features explicitly enable Wayland/X11; `.cargo/config.toml` sets `CXX=gcc` globally. GPUI's Windows release build invokes `fxc.exe`. | Select features per target, scope the Linux compiler workaround, and provision MSVC plus the Windows SDK shader compiler. |
| Image transforms | Rust `PolychromeSprite` includes `transformation` and `paint_bounds`; Linux WGSL includes these fields. Windows HLSL omits them and uses untransformed position/clipping logic. | Reconcile CPU/GPU layouts and implement DirectX transform/clipping parity before trusting mixed-media rendering. |
| Pen | Folio's normalized `PenEvent` and GPUI tablet routing already exist. The Windows message handler currently handles mouse messages but has no `WM_POINTER` pen path. | Add a native Windows Ink adapter into the existing tablet pipeline. |
| Window chrome | Folio's title bar calls `start_window_move`/`start_window_resize`; GPUI supplies default no-op implementations, and the Windows window backend does not override these methods. | Implement Windows movement/resizing or use the backend's native hit testing, then validate the tab strip and window controls. |
| Accessibility | `crates/ui` unconditionally depends on `accesskit_unix`; the bridge directly constructs its AT-SPI adapter. | Share the semantic tree and action queue, with target-specific AT-SPI/UI Automation adapters. |
| Data directory and locks | Paths use XDG/HOME; `lock_data_dir` only acquires its lock under `cfg(unix)`. | Use the Windows local application-data directory and a real cross-platform writer lock. |
| Asset/export durability | `copy_asset`, PDF unlocking, and `atomic_write` open directories and call `sync_all`; export replacement also keeps a temporary file handle open through rename. | Centralize platform-aware publication/replacement and verify Windows sharing/flush semantics. |
| PDF | Metadata/password handling is Rust; preview rendering executes `pdftoppm` by name. | Bundle a pinned Windows Poppler toolchain and dependencies, resolved relative to the installation. |
| Math | The SymPy worker protocol is already separated from the UI. Setup writes an absolute Python path and creates a directory symlink. | Bundle a relocatable Python/SymPy/mpmath pack and eliminate the symlink requirement. |
| OCR | Automatic setup explicitly rejects non-Linux targets; runtime name, tar extraction, and server path are Linux-specific. | Add a verified Windows runtime descriptor and extraction/launch path while reusing GGUF models and the local protocol. |
| Validation/release | UI harnesses use X11, D-Bus, and AT-SPI; packaging targets Linux. | Add Windows CI, UI Automation tests, clean-machine packaging checks, and physical pen testing. |

## Milestone 1 — Build and prove the backend

1. Add Windows CI alongside Linux checks. Use a native Windows runner, Rust >=1.92, MSVC C/C++ tools, and the Windows SDK. Locate `fxc.exe` explicitly if needed via `GPUI_FXC_PATH`.
2. Make GPUI features and accessibility dependencies target-specific. Enable a verified Windows manifest/resource path for DPI awareness and app identity. Keep the Linux build working with the locked dependencies.
3. Build both debug and release configurations; the vendored GPUI shader build differs between them. A Linux cross-check alone cannot prove this Windows build path.
4. Run a small GPUI/Folio fixture that draws ink, text, several images, and PDF backgrounds. Port the HLSL image transforms and clipping, checking struct offsets/stride as well as visual output.
5. Fix title-bar movement, resizing, minimize/maximize, system menu, tab dragging, and close-save handling. Include Alt+F4 and native close events.
6. Prove one real Windows Ink pen can deliver pressure/tilt samples into Folio's existing event pipeline before expanding the port.

**Exit:** native debug/release builds launch; a fixture renders correctly at multiple scales; a physical pen supplies rich samples. This resolves the highest architectural uncertainties early.

## Milestone 2 — Make persistence and workers portable

Concentrate shared platform services in `crates/platform` rather than spreading Windows branches through document/editing code:

- Resolve `%LOCALAPPDATA%\Folio` through the Windows known-folder API. Preserve `--data-dir` and `FOLIO_DATA_DIR` overrides. Keep models and mutable state outside the installed application directory.
- Replace the Unix-only application lock with `File::try_lock` or an equivalent portable implementation. Keep the file alive for the full session and preserve the existing second-writer error.
- Define publication helpers for new assets and replacement exports/configuration. Close handles where necessary, flush writable files, publish from a temporary file on the same volume, and handle Windows replacement and sharing errors. Preserve useful failure reporting; do not report a failed save as successful. Specify the actual Windows durability guarantee instead of assuming Unix directory fsync carries over.
- Route asset copy, unlocked PDFs, exports, and OCR pack publication through these helpers. Audit recovery renames, deletion of in-use assets, path-component validation, drive/UNC paths, and reserved Windows names.
- Resolve bundled executables by explicit installation-relative paths. Make command detection account for Windows extensions and remove dependence on the working directory or a globally installed helper.
- Consolidate subprocess launch policy for PDF, math, OCR discovery, and resident OCR. Avoid console flashes and verify cancellation, timeout, shutdown, and child-process cleanup; use a Job Object if needed for process-tree lifetime.
- Exercise paths containing spaces and non-ASCII characters, including user-profile and CLI import paths. Use OS-native arguments where file paths require them.

**Exit:** a Windows build can create, save, reopen, import/export, reject a second writer, and recover notes. Existing Linux data fixtures open without a format migration, and copied-back Windows fixtures remain readable on Linux. Transfer a closed/checkpointed library including assets, rather than copying a live SQLite database alone.

## Milestone 3 — Complete native handwriting and accessibility

### Pen and navigation

Handle pen down/update/up, hover/leave, and capture-loss/cancellation through `WM_POINTER` and `GetPointerPenInfo`/`GetPointerPenInfoHistory`. Translate native samples to the existing GPUI `TabletEvent`, then Folio's `PenEvent`.

Honor validity masks; normalize reported pressure to 0–1 and match Folio's tilt conventions. Preserve tool/device identity, eraser and barrel buttons, and sensor timestamps. Replay coalesced samples oldest first with bounded buffering; Windows returns pen history newest first. Convert screen/device coordinates to GPUI client logical coordinates once, including mixed-DPI and negative monitor origins.

Keep pen interaction working on ordinary controls while suppressing duplicate mouse-promoted canvas strokes. Cancel an active stroke on capture loss, device loss, tab changes, or window deactivation. Test touch/palm behavior separately from mouse and pen, and define the first release's touch-navigation support explicitly. Tablet pad rings/strips and vendor-specific buttons need capability testing; do not promise parity without a Windows mapping.

Use Windows Ink first. Introduce WinTab only if supported target hardware demonstrates a concrete gap; if added, select one input backend per device to avoid duplicate streams.

### Accessibility

Retain the existing semantic tree, stable IDs, focus information, bounds, and queued actions. Add an AccessKit Windows adapter compatible with the pinned `accesskit` version. Connect it to the GPUI HWND and window lifecycle through message handling or a supported subclassing adapter. Preserve UI-thread ownership for actions and updates.

Validate keyboard-only flows, editable text/IME, dialogs, focus restoration, library controls, math review, Narrator, and UI Automation inspection. An internal bring-up build can temporarily omit the adapter, but accessibility remains a release gate.

**Exit:** real pen writing survives fast strokes and interrupted input without duplicates or stuck strokes; keyboard and accessibility workflows operate on Windows.

## Milestone 4 — Bundle PDF, math, and optional OCR

**PDF:** retain the current Rust parser and Poppler preview interface. Build/package pinned Windows `pdftoppm.exe` and its DLLs with notices. Test ordinary/encrypted/rotated/cropped/large PDFs and preview cancellation. A PDF-engine replacement should require evidence that packaging the existing engine is unsuitable.

**Math:** package a compatible Windows Python runtime with the existing pinned SymPy/mpmath versions and worker sources. Use install-relative pack paths; account for embedded Python's module-search configuration. Remove the setup symlink requirement and verify the full package after moving it to a different directory. Preserve the existing JSON worker protocol and offline behavior.

**OCR:** split common model metadata from OS/architecture runtime metadata. Verify a Windows llama.cpp runtime against the currently pinned release and required flags; if no compatible artifact exists, build from a pinned source revision and retain provenance. Add exact runtime sizes/hashes, ZIP extraction with safe paths, `llama-server.exe`/DLL resolution, and platform-derived progress totals. Keep resumable checksummed downloads, cancellation, installation locking, the authenticated loopback endpoint, GPU selection, and CPU fallback. Check firewall behavior without widening the bind address beyond loopback. Reuse the pinned GGUF model/vision assets after validation.

**Exit:** PDF previews and math work on a clean offline Windows machine without a separate Python installation; first-use OCR installs successfully, resumes after interruption, and works offline on CPU and tested GPUs afterward.

## Milestone 5 — Package and certify

1. Produce a portable ZIP containing the executable, required DLLs, math pack, PDF tools, licenses, and support documentation. OCR models remain an optional first-use download.
2. Add a per-user installer with app icon/version resources, Start Menu entry, clean upgrades, and uninstall behavior that preserves notes. Decide code signing before public distribution. File associations and Store/MSIX packaging can follow after the base package is stable.
3. Expand dependency notices beyond the current Linux-only Cargo metadata filter. Include Windows Rust dependencies and the actual PDF/Python/OCR redistribution payloads.
4. Run CI unit tests, formatting, Clippy, debug/release builds, worker integration checks, and package smoke checks on Windows and Linux. Classify existing Linux-specific fixtures rather than silently losing their Windows coverage.
5. Add Windows UI Automation scenarios equivalent to the current native smoke, titlebar, recognition, math, appearance, and accessibility workflows. Reuse the in-app synthetic tablet replay for deterministic regression coverage.
6. Test the release package on a clean Windows VM with no developer tools, system Python, or global Poppler. Cover non-admin install, offline launch, moved portable folders, upgrade, read-only destinations, interrupted worker/setup operations, and uninstall/reinstall.
7. Test physical hardware: an integrated Windows Ink pen device and the user's external tablet, then additional drivers as available. Cover 100/150/200% scaling, mixed-DPI monitors, suspend/resume, USB/Bluetooth reconnect, eraser/barrel buttons, rapid drawing, touch/palm interference, and GPU/device-loss recovery. Measure input-to-display latency on hardware; synthetic replay does not establish it.

**Release gate:** cross-platform note round trips and recovery pass; core features run offline; pen and rendering pass physical testing; accessibility works; no orphan workers remain; the installer/portable package passes clean-machine tests; Linux regressions remain green.

## Delivery order and effort

Suggested reviewable changes:

1. Windows build configuration and CI.
2. DirectX rendering/chrome parity and an initial native pen proof.
3. Portable data paths, locks, durable publication, and subprocess/resource resolution.
4. Complete Windows pen/navigation integration.
5. Windows accessibility integration.
6. Windows PDF and math payloads.
7. Windows OCR setup/runtime support.
8. Portable ZIP, installer, regression harnesses, and release evidence.

Budget approximately **4–8 engineering weeks for one developer** as an initial planning range, not a measured estimate. Re-estimate after Milestone 1: the vendored renderer, tablet drivers, and native dependency packaging dominate uncertainty. Hardware availability and signing/distribution setup may add calendar time. A launchable engineering build should arrive well before the full release gates are met.

The immediate next task is Milestone 1 on a Windows machine/runner with access to an actual pen. Avoid a broad GPUI upgrade during bring-up unless a demonstrated blocker requires it; preserve the existing vendor patches and review their Windows parity explicitly.

## API references

- [Microsoft: pen information and field ranges](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-pointer_pen_info).
- [Microsoft: coalesced pen history and sample order](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getpointerpeninfohistory).
- [Rust: File locking APIs](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock).
- [AccessKit Windows adapters, including WM_GETOBJECT subclassing](https://docs.rs/accesskit_windows/latest/accesskit_windows/).
