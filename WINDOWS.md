# Folio on Windows

The Windows port targets Windows 11 x64. Its interface and canvas draw through Rune's GPU renderer on Direct3D 12, and pens use Windows Ink input. Physical tablet compatibility and latency must be tested on the devices you use; a VirtualBox session only validates virtual display/input behavior.

## Build

Install Rust 1.92 or newer and Visual Studio Build Tools with **Desktop development with C++** and a Windows SDK. From a local checkout in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1 -DebugBuild
powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1
```

The script loads the x64 MSVC environment. `-Check` checks all workspace targets; `-Test` runs the workspace tests. Before running PDF/math integration tests, prepare the runtime tools below and put the package's `pdf/bin` on PATH and its `math-solver/pack.json` in `FOLIO_MATH_CONFIG`.

## Portable package

Run the packaging script in Python 3.11 or newer on Windows or Linux:

```powershell
python scripts/package-windows.py --binary target/x86_64-pc-windows-msvc/release/folio.exe
```

It creates a ZIP under `artifacts/dist`, containing `bin/folio.exe`, bundled offline stroke-recognition models (`recognition/`), embedded Python, the pinned offline math worker/dependencies, PDF preview tools, and notices. Downloads use the exact SHA-256 values in `packaging/windows/runtime-assets.json`; setup does not need pip. To prepare just the tools before an executable exists, use `--runtime-only --output artifacts/windows-tools`. `--installer` also builds a per-user installer if Inno Setup 6 is available.

Extract the entire ZIP into a writable folder and launch `bin/folio.exe`. Keep the sibling `recognition`, `python`, `math-solver`, and `pdf` directories together. Moving the complete folder preserves worker paths. Packaging on Windows automatically includes the matching x64 Visual C++ redistributable DLLs from the installed build tools. When packaging on Linux, pass those DLLs with repeated `--extra-dll PATH` arguments. The release package includes them beside `folio.exe`.

Notes and preferences live under `%LOCALAPPDATA%\Folio`. `--data-dir PATH` and `FOLIO_DATA_DIR` override this. Launching Folio again for the same directory, including from a shortcut or the taskbar, activates the running window (restoring it if minimized) through a per-library local named pipe instead of opening a second instance. The installer keeps this data when uninstalled.

## Offline handwriting and math recognition

Handwriting and math recognition models (`recognition/math_v2.frec`, `recognition/htr_v2.frec`, and `recognition/htr_lm.json.gz`, ~38 MiB total) are bundled directly in the package and execute on CPU in pure Rust via `folio-recognition`. No model download, external inference server, or Vulkan GPU driver is required. Ink strokes are recognized directly as stroke sequences; raster image and PDF crop OCR are not used. Notes and handwriting stay local.

## Validation

Release 0.2.6 is validated natively on Windows by the `Linux and Windows` workflow (GitHub `windows-2025`, MSVC): `cargo check` of every workspace target, the whole workspace test suite (storage, documents, recognition, math, export, updater and the Rune UI), the updater-recovery and packaging tests, the offline runtime check (`verify-windows-runtime.py`: embedded Python, math worker and PDF tools) and the release build with installer and portable packages. Folio's Windows-specific paths beyond those tests (Windows Ink history and HIMETRIC mapping, the single-instance pipe, the update helper) were reviewed in code for this release. An interactive session on the disposable VirtualBox VM was not run for 0.2.6; the checks below remain the way to exercise launch, pen replay and UI Automation on a desktop. They do not establish physical pen/display latency.

Use `tools/verify-windows.ps1 -Package PATH` from the extracted package (or `scripts/verify-windows.ps1 -Package PATH` from the checkout) on an interactive Windows desktop to exercise package launch (window and library creation, then a clean close), native synthetic pen replay, durable saving, accessibility actions, PDF rendering, and offline math. It uses throwaway data, never your default notes. Real pen, eraser, barrel-button, palm, mixed-DPI, and reconnect behavior still need hardware validation. Legacy WinTab-only devices, vendor-specific tablet pad controls, and native touchscreen pinch/pan gestures are outside the initial Windows Ink implementation. Trackpad scrolling and pinch arrive as Rune wheel and magnify events.

For titlebar regressions, run `powershell -ExecutionPolicy Bypass -File scripts/verify-titlebar-windows.ps1 -Package PATH -ConfirmDisposableVm` on an idle VirtualBox Windows desktop. It checks Home, Update and Maximize/Restore with real mouse input across windowed/maximized states, button centers/edges and quick/normal presses, plus blank-titlebar dragging and double-click maximize/restore. The `--smoke-titlebar` fixture requires a fresh explicit data directory and simulates update progress without opening the installed updater cache or downloading/installing anything. Evidence goes under `artifacts/validation/` by default; keep the desktop free of other input during the run.

Opening an existing library migrates its schema transactionally; earlier Folio versions may not be able to open an upgraded library. The installer leaves saved databases untouched until the application opens them. Keep release validation evidence under ignored `artifacts/validation/` and summarize results in the release description.
