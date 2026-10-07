# Folio 1.0.0

Folio is a Linux notebook for editable vector handwriting, PDF annotation, rendered LaTeX and step-by-step math. This refreshed 1.0 release distributes **only the x86_64 Flatpak**, with automatic OCR setup and Vulkan Q8 acceleration.

## Install or update

Download `folio-1.0.0-x86_64.flatpak`, then run:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user --reinstall ./folio-1.0.0-x86_64.flatpak
flatpak run io.github.folio.Notes
```

Close an already running Folio window before launching the updated build. Folio is also available from the application menu. The bundle uses the Freedesktop 25.08 runtime, which Flatpak may download during installation.

## OCR setup and Vulkan

- Request **Recognize text**, **Recognize math** or OCR from the math solver. If no usable pack exists, Folio automatically downloads and verifies GLM-OCR **Q8_0** language/vision weights and the llama.cpp Vulkan runtime.
- The first-use download is approximately **1.47 GB**, with approximately **1.56 GB** installed storage. The Flatpak itself contains no OCR weights, CUDA toolkit or Python OCR environment.
- OCR prefers a supported discrete Vulkan GPU, then another available GPU, and falls back to CPU when GPU loading fails. A GPU inference runtime failure gets a CPU retry. NVIDIA and AMD were tested; Intel remains untested.
- Setup shows progress and supports cancellation, resumable downloads and retry. Downloads use pinned revisions and SHA-256 checks before activation.
- Recognition works offline after setup. Notes, handwriting, images and PDFs stay local; only model/runtime assets are fetched from Hugging Face and GitHub. Flatpak has network permission for this setup and GPU permission for acceleration.
- Working existing recognition packs and explicit Python pack overrides remain supported. First GPU use can take several seconds to initialize; subsequent requests reuse the model.

## Included in 1.0

- Pressure-sensitive vector handwriting, erasing and shape gestures, selection, typed text/images, persistent undo and recovery.
- A document library with folders, favorites, recent notes and trash; browser-style tabs and window controls.
- PDF import, lazy previews and vector/searchable annotated PDF export. Poppler 26.10.0 is included.
- Editable LaTeX and an offline CPU math solver for supported arithmetic, algebra, calculus, graphs, teaching steps and work checking. SymPy 1.14.0 and mpmath 1.3.0 are included.
- OCR review, copying, undoable ink replacement and native LaTeX rendering. Transparent image areas are composited onto white for recognition.
- The persistence, selection, geometry, export and notebook performance fixes documented in `OPTIMIZATION_AND_BUGFIX_REPORT.md`.

Existing Flatpak notes remain in `~/.var/app/io.github.folio.Notes/data/folio`. Import/export uses desktop file chooser portals; no host filesystem permission is granted. The GNOME launcher uses X11/Xwayland; `FOLIO_NATIVE_WAYLAND=1` opts into native Wayland. OCR output should be reviewed, and unsupported mathematics is reported explicitly.

## Verification and source

170 Rust workspace tests passed, along with strict Clippy, formatting, private native UI checks, NVIDIA/AMD/CPU OCR and a Flatpak first-use download followed by offline OCR. See [release validation](https://github.com/juccul/Folio/blob/v1.0.0/RELEASE_VALIDATION.md).

The SHA-256 checksum for the single Flatpak download is shown below. Matching source, build scripts, locked dependencies, notices and the exact Poppler source are available in the [v1.0.0 source tag](https://github.com/juccul/Folio/tree/v1.0.0). GitHub also provides the tag's source ZIP/tarball. Cargo fetches unmodified registry dependencies from the lockfile; `scripts/package.py` can generate a complete vendored source archive for offline builds. Bundled math packages contain their Python source and notices. Folio is GPL-3.0-or-later; third-party licenses are retained.

```text
3fe0da14808749428f4c877453059133d5c70e95d55332b65c4b21de252a9531  folio-1.0.0-x86_64.flatpak
```
