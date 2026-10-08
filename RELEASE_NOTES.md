# Folio 0.1.1

Folio 0.1.1 adds **Windows 11 x64** support with a per-user installer and a portable ZIP, alongside the **Linux x86_64 Flatpak**. It includes the Windows port, current notebook features from main, and the notebook setup, continuous page scrolling, inline text and handwriting fixes in the tested workspace.

## Install or update

Download and run **`folio-0.1.1-windows-x64-setup.exe`**. No administrator rights are needed. Launch Folio from the Start menu. Close an existing Folio window before updating. Notes and downloaded OCR assets remain under `%LOCALAPPDATA%\Folio`; upgrading or uninstalling keeps them.

For portable use, extract **`folio-0.1.1-windows-x64.zip`** and launch `bin/folio.exe`. Keep all sibling directories together. Both downloads bundle the matching Visual C++ runtime, embedded Python, offline SymPy/mpmath math tools and PDF previews; no separate Python installation is required.

On Linux, download **`folio-0.1.1-x86_64.flatpak`** and install it with `flatpak install --user ./folio-0.1.1-x86_64.flatpak`. Launch it from your application menu or run `flatpak run io.github.folio.Notes`. It uses Freedesktop Platform 25.08 and bundles offline math and PDF preview tools. Installation may download the runtime; OCR assets download on first use. Updating keeps existing Flatpak notes.

## Changes

- Native DirectX/DirectWrite rendering, Windows Ink pressure/tilt input, eraser/barrel identity, custom title-bar movement/resizing and UI Automation accessibility.
- Windows application-data paths, a single-writer library lock, durable file replacement, portable worker paths and background-worker cleanup.
- Offline PDF previews and math, including Unicode protocol input, plus checksummed first-use OCR downloads with CPU fallback.
- Configurable notebooks, paper colors, continuous scrolling across finite pages, inline text editing and formatting, and scratch-erase improvements.
- Page bookmarks, duplication and transfers; library content previews and selected cover pages; editable notebook sharing and library backup/restore; reusable page templates; reviewed handwriting search; and an opt-in tablet input inspector.
- Windows and Linux build/test automation and retained dependency notices.

## OCR and pen support

The first OCR request downloads approximately **1.47 GB** of pinned GLM-OCR Q8 model/vision files and the llama.cpp Windows Vulkan runtime. Downloads are resumable and checked with SHA-256. Subsequent recognition works offline. CPU fallback is available when a compatible Vulkan GPU is unavailable. Review recognition results before replacement.

Windows Ink pressure and tilt were verified using native synthetic input in the Windows VM. Physical pen latency, eraser/barrel behavior, palm rejection, mixed-DPI and reconnect tests remain pending. Legacy WinTab-only devices, tablet pad controls and native touchscreen pinch/pan are outside the initial Windows implementation. See [Windows instructions](WINDOWS.md).

## Verification and source

Build and package validation are recorded in [RELEASE_VALIDATION.md](RELEASE_VALIDATION.md). `SHA256SUMS` accompanies the Windows installer, portable ZIP and Linux Flatpak. In PowerShell, use `Get-FileHash .\folio-0.1.1-windows-x64-setup.exe -Algorithm SHA256` and compare with that file.

The [v0.1.1 source tag](https://github.com/juccul/Folio/tree/v0.1.1) contains the matching application source, Cargo.lock, local dependency patches, build/package scripts and notices. GitHub provides source ZIP/tarball downloads. Cargo retrieves registry dependencies at the locked versions. Bundled Python/math distributions include their source or corresponding upstream source references and notices; pinned runtime download URLs/hashes are recorded in the package. Folio is GPL-3.0-or-later; third-party components retain their licenses.
