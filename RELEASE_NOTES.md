# Folio 1.0.0

Folio is an offline Linux notebook for editable vector handwriting, mixed-media notes, PDF annotation, rendered LaTeX and step-by-step math.

## Downloads and installation

These packages are for **x86_64 / amd64 Linux**:

| Package | Installation |
| --- | --- |
| `folio-1.0.0-1.x86_64.rpm` | `sudo dnf install ./folio-1.0.0-1.x86_64.rpm` |
| `folio_1.0.0_amd64.deb` | `sudo apt install ./folio_1.0.0_amd64.deb` |
| `folio-1.0.0-x86_64.flatpak` | `flatpak install --user ./folio-1.0.0-x86_64.flatpak` |

Launch Folio from your application menu. Native packages also provide `folio`; Flatpak uses `flatpak run io.github.folio.Notes`.

Native packages need glibc 2.35+, Python 3.10+, a Vulkan driver and a desktop session. RPM/DEB declare their runtime dependencies, including Poppler for PDF previews. Flatpak needs the Freedesktop 25.08 runtime; if necessary, configure Flathub with `flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo`. The initial runtime installation may use internet. Folio itself works offline, with no account or cloud service.

## Included in 1.0

- Pressure-sensitive handwriting, editable vector ink, erasing, shape gestures, selection, text and image insertion, persistent undo and recovery.
- A document library with folders, favorites, recent notes and trash; browser-style document tabs and custom window controls.
- PDF import, lazy page previews and vector/searchable annotated PDF export.
- Rendered, editable LaTeX and an offline CPU math solver with step-by-step rules, algebra, calculus, graphs and work checking. The packages bundle SymPy 1.14.0 and mpmath 1.3.0; Flatpak also includes Poppler 26.10.0 for PDF previews.
- Optional GLM-OCR handwriting-to-text and handwriting-to-LaTeX, including review, copying and undoable ink replacement. **OCR model weights and the neural inference runtime are not bundled.** Install a compatible recognition pack separately using the README. A Flatpak recognition runtime must work inside its sandbox.

## Reliability and performance

This release includes the optimization and edge-case fixes recorded in `OPTIMIZATION_AND_BUGFIX_REPORT.md`: cheaper background database reads and linked-math lookup, consistent document/history snapshots, safer save acknowledgements and exports during navigation, Unicode selection fixes, tab overflow handling, stronger geometry/document validation and improved mathematical domain checking.

## Data and compatibility

Existing notes remain readable. Native and Flatpak use separate default data directories; back up existing data before migrating it. Flatpak uses the desktop file chooser portal for import/export and has no network permission by default. The GNOME launcher uses X11/Xwayland for the previously documented tablet compatibility issue; `FOLIO_NATIVE_WAYLAND=1` opts into native Wayland.

Validation uses automated Rust/Python checks, isolated native UI sessions and package smoke tests. Broad physical-tablet and distribution certification remains incomplete. OCR output needs review, and unsupported mathematics is reported explicitly.

## Checksums and source

`SHA256SUMS` covers the released packages and `folio-1.0.0-source.tar.gz`. Check it with `sha256sum --check SHA256SUMS` after downloading all listed assets.

The source archive includes the locked vendored Rust dependency graph for offline builds and the exact Poppler source used by Flatpak. Bundled math packages include their Python source and license notices in each installer. Original Folio source is GPL-3.0-or-later; dependency notices accompany every package.
