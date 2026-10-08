# Folio 0.1.1 release validation

Validated on 8 October 2026. This release publishes a Linux x86_64 Flatpak, Windows 11 x64 installer and portable ZIP. The Linux and Windows executables are built from the same immutable v0.1.1 application source tag.

- **Native build:** Built the optimized executable in the Windows 11 VirtualBox VM **Ransom**, using the x64 MSVC toolchain and the Windows SDK shader compiler. The embedded product/file version is 0.1.1. The package manifest records the executable, Cargo.lock and app-local Visual C++ runtime hashes.
- **Linux Flatpak:** Built with Rust 1.98.1 against Debian Bookworm; the maximum required glibc symbol version is 2.35. Installed the actual 0.1.1 single-file bundle into a separate private Flatpak installation using Freedesktop Platform 25.08. Version, matching optimized binary hash, AppStream metadata, pinned SymPy 1.14.0/mpmath 1.3.0, Unicode requests, malformed-request recovery and actual PDF rasterization passed. The installed native editor replay passed pressure/tilt, toolbar interactions, undo/redo, page creation, card menus, duplication, tab picking/dragging, crop outline/undo and durable saving. The package grants display, IPC, GPU and first-use download network access, with no host filesystem permission. Models and personal notes are excluded.
- **Regression checks:** All 216 Rust workspace tests passed on Windows and Linux. One opt-in real-model benchmark remains ignored. These checks include notebook sharing, backup/restore, library management, handwriting search, templates, bookmarks, text editing, storage migrations and Windows portable file replacement. Python math protocol and input-analysis tests, Rust formatting and packaging-script syntax checks passed.
- **Installed package:** The actual Inno Setup installer installed Folio per user without elevation. Offline SymPy/mpmath, Unicode protocol input, malformed-request recovery and PDF previews passed from an unrelated working directory. The installed executable passed the native editor smoke replay, window movement/resizing, UI Automation actions, close/save and Windows Ink replay. Pen verification found exactly one stroke with 25 pressure/tilt samples and no duplicate mouse ink. The executable loaded its bundled Visual C++ runtime.
- **Upgrade and uninstall:** Installed 0.1.0 into an isolated directory, then upgraded it with the 0.1.1 installer. Used disposable notes saved by the previously validated 0.1.0 package. The installer left the database unchanged; launching 0.1.1 reopened it while preserving note IDs/titles, page IDs/order and every saved object exactly. Uninstalling removed the executable while retaining the saved databases. The final 0.1.1 package remains installed in Ransom at `%LOCALAPPDATA%\Programs\Folio`.
- **Corresponding source:** The release tag contains the matching application source, locked dependencies, local GPUI patches, build/package scripts, notices and the unmodified Poppler 26.09.0 source archive matching the Windows PDF runtime and Poppler 26.10.0 source matching the Flatpak runtime. Runtime asset URLs and SHA-256 values are pinned in `packaging/windows/runtime-assets.json`. The source snapshot used for the native build was compared with the release checkout; Rust sources, Cargo manifests and Cargo.lock match exactly.

## Scope

The earlier Windows port validation also exercised real CPU text/math OCR, native replacement, undo/redo/reopening, image rotation and PDF import. Those checks establish functionality and were not repeated as recognition-accuracy benchmarks for this release. First-use OCR still downloads about 1.47 GB of pinned model/runtime assets; the installer itself includes offline PDF and math tools.

Ransom exposes a VirtualBox USB tablet pointer. No physical pen/tablet passthrough is configured. Physical pen latency, eraser/barrel buttons, palm rejection, mixed-DPI and reconnect behavior require hardware validation. Synthetic pen replay does not certify those behaviors.

Build/test logs and disposable fixtures are retained locally under `artifacts/release-0.1.1`, excluded from Git and the release downloads. Validation uses synthetic notes rather than personal notes.

## Release downloads

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| `folio-0.1.1-windows-x64-setup.exe` | 49,463,152 | `1ec6f44ee65027ab0c2bc76f6b31a4303c3a672610958fa52c547b891061e0b1` |
| `folio-0.1.1-windows-x64.zip` | 79,357,948 | `ab386f1e7027f4e228d1607758c5097698841827177430d58289c546cece3c79` |

| `folio-0.1.1-x86_64.flatpak` | 17,236,128 | `a93ac6992df62104c8ca49d3be13152ff1e5ff30049fa4c31222b9330026f5e5` |

The same checksums accompany the downloads in `SHA256SUMS`.
