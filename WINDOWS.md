# Folio on Windows

The Windows port targets Windows 11 x64 with native GPUI/DirectX rendering and Windows Ink input. Physical tablet compatibility and latency must be tested on the devices you use; a VirtualBox session only validates virtual display/input behavior.

## Build

Install Rust 1.92 or newer and Visual Studio Build Tools with **Desktop development with C++** and a Windows SDK. The SDK must include `fxc.exe` for release shaders. From a local checkout in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1 -DebugBuild
powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1
```

The script loads the x64 MSVC environment and finds the SDK shader compiler. `GPUI_FXC_PATH` can override its location. `-Check` checks all workspace targets; `-Test` runs the workspace tests. Before running PDF/math integration tests, prepare the runtime tools below and put the package's `pdf/bin` on PATH and its `math-solver/pack.json` in `FOLIO_MATH_CONFIG`.

## Portable package

Run the packaging script in Python 3.11 or newer on Windows or Linux:

```powershell
python scripts/package-windows.py --binary target/x86_64-pc-windows-msvc/release/folio.exe
```

It creates a ZIP under `artifacts/dist`, containing `bin/folio.exe`, embedded Python, the pinned offline math worker/dependencies, PDF preview tools, and notices. Downloads use the exact SHA-256 values in `packaging/windows/runtime-assets.json`; setup does not need pip. To prepare just the tools before an executable exists, use `--runtime-only --output artifacts/windows-tools`. `--installer` also builds a per-user installer if Inno Setup 6 is available.

Extract the entire ZIP into a writable folder and launch `bin/folio.exe`. Keep the sibling `python`, `math-solver`, and `pdf` directories together. Moving the complete folder preserves worker paths. Packaging on Windows automatically includes the matching x64 Visual C++ redistributable DLLs from the installed build tools. When packaging on Linux, pass those DLLs with repeated `--extra-dll PATH` arguments. The release package includes them beside `folio.exe`.

Notes and downloaded OCR assets live under `%LOCALAPPDATA%\Folio`. `--data-dir PATH` and `FOLIO_DATA_DIR` override this. Launching a second writer for the same directory is rejected. The installer keeps this data when uninstalled.

## Optional OCR

The first recognition request downloads the pinned GLM-OCR Q8 model/vision files and llama.cpp b11457 Windows Vulkan runtime (about 1.47 GB total). Downloads are resumable and checksummed. CPU fallback works when a suitable Vulkan GPU is unavailable; a VirtualBox graphics adapter generally does not provide a usable OCR Vulkan device. Subsequent recognition works offline. Notes, PDFs, and handwriting are processed locally.

## Validation

Use `tools/verify-windows.ps1 -Package PATH` from the extracted package (or `scripts/verify-windows.ps1 -Package PATH` from the checkout) on an interactive Windows desktop to exercise package launch, native synthetic pen replay, durable saving, accessibility actions, PDF rendering, and offline math. It uses throwaway data, never your default notes. Real pen, eraser, barrel-button, palm, mixed-DPI, and reconnect behavior still need hardware validation. Legacy WinTab-only devices, vendor-specific tablet pad controls, and native touchscreen pinch/pan gestures are outside the initial Windows Ink implementation. Trackpad wheel scrolling uses the existing GPUI path.

Opening an existing library migrates its schema transactionally; earlier Folio versions may not be able to open an upgraded library. The installer leaves saved databases untouched until the application opens them. Keep release validation evidence under ignored `artifacts/validation/` and summarize results in the release description.
