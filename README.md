# Folio

Offline handwriting and mixed-media notes for Linux and Windows, built in Rust with the [Rune](https://github.com/anomalyco/rune) UI toolkit. Your notes stay on your device; no account is required.

## Features

- Pressure-sensitive pen input, editable vector ink, shapes and selection tools.
- Paged notebooks or infinite canvases, with text, images and PDF annotation.
- Folders, favorites, templates and search, including reviewed handwriting.
- Offline handwriting and math recognition from your strokes, offline math solving and inline auto math.
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

Everything works offline. The stroke recognition models ship with Folio and run locally on the CPU; nothing is downloaded or sent anywhere. A short tour of the pen's features opens on first launch and stays available in Help.

## Build

Linux builds require Rust 1.92+, a C compiler, native desktop libraries (`libxkbcommon`, `libwayland-client`) and Vulkan support. The [build container](packaging/Containerfile) lists dependencies; see [Windows build instructions](WINDOWS.md) for Windows.

```sh
cargo build --locked --release -p folio
./target/release/folio
```

## Documentation

- [Development status and compatibility](DEVELOPMENT.md)
- [Release notes](RELEASE_NOTES.md)
- [Math solver](DEVELOPMENT.md#offline-solving-and-live-math)

Licensed under [GPL-3.0-or-later](LICENSE). Third-party notices are in [LICENSES.md](LICENSES.md).
