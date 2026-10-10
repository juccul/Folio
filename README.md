# Folio

Offline handwriting and mixed-media notes for Linux and Windows, built in Rust with GPUI. Your notes stay on your device; no account is required.

## Features

- Pressure-sensitive pen input, editable vector ink, shapes and selection tools.
- Paged notebooks or infinite canvases, with text, images and PDF annotation.
- Folders, favorites, templates and search, including reviewed handwriting.
- Offline math solving and optional handwriting recognition.
- Persistent undo, library backups, and PDF, SVG, PNG or editable notebook export.
- Light, dark and automatic system themes.

## Install

Download the [latest release](https://github.com/juccul/Folio/releases/latest) for **Windows 11 x64** or **Linux x86_64**.

**Windows:** run the installer, or extract the entire portable ZIP and launch `bin/folio.exe`.

**Linux:** install the Flatpak and launch Folio from your application menu:

```sh
flatpak install --user https://juccul.github.io/Folio/folio.flatpakref
flatpak run io.github.folio.Notes
```

Updates appear beside the window controls. Click **Update**, then **Restart to update** when ready. See [installation, updates and recovery](UPDATES.md) for existing installations and troubleshooting.

Note taking and math solving work offline. Optional handwriting recognition downloads its model on first use, then runs locally; see [recognition setup](OCR_FIRST_USE.md).

## Build

Linux builds require Rust 1.92+, a C compiler, native desktop libraries and Vulkan support. The [build container](packaging/Containerfile) lists dependencies; see [Windows build instructions](WINDOWS.md) for Windows.

```sh
cargo build --locked --release -p folio
./target/release/folio
```

## Documentation

- [Development status and compatibility](DEVELOPMENT.md)
- [Release notes](RELEASE_NOTES.md)
- [Math solver](MATH_SOLVER_DESIGN.md)

Licensed under [GPL-3.0-or-later](LICENSE). Third-party notices are in [LICENSES.md](LICENSES.md).
