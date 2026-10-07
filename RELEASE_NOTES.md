# Folio 0.1.0

Folio is a Linux notebook for editable vector handwriting, PDF annotation, rendered LaTeX and step-by-step math. This initial 0.1.0 release distributes **only the x86_64 Flatpak**, with automatic OCR setup and Vulkan Q8 acceleration.

## Install or update

Download `folio-0.1.0-x86_64.flatpak`, then run:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user --reinstall ./folio-0.1.0-x86_64.flatpak
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

## Included in 0.1.0

- Pressure-sensitive vector handwriting, erasing and shape gestures, selection, typed text/images, persistent undo and recovery.
- A document library with folders, favorites, recent notes and trash; browser-style tabs and window controls.
- PDF import, lazy previews and vector/searchable annotated PDF export. Poppler 26.10.0 is included.
- Editable LaTeX and an offline CPU math solver for supported arithmetic, algebra, calculus, graphs, teaching steps and work checking. SymPy 1.14.0 and mpmath 1.3.0 are included.
- OCR review, copying, undoable ink replacement and native LaTeX rendering. Transparent image areas are composited onto white for recognition.
- The persistence, selection, geometry, export and notebook performance fixes documented in `OPTIMIZATION_AND_BUGFIX_REPORT.md`.

Existing Flatpak notes remain in `~/.var/app/io.github.folio.Notes/data/folio`. Import/export uses desktop file chooser portals; no host filesystem permission is granted. The GNOME launcher uses X11/Xwayland; `FOLIO_NATIVE_WAYLAND=1` opts into native Wayland. OCR output should be reviewed, and unsupported mathematics is reported explicitly.

## Verification and source

168 default Rust workspace tests passed after cleanup, along with math unit/protocol checks, private native UI checks, formatting and package validation. Prior recognition checks covered NVIDIA/AMD/CPU OCR and a Flatpak first-use download followed by offline OCR; recognition behavior is unchanged by the version reset. See [release validation](https://github.com/juccul/Folio/blob/v0.1.0/RELEASE_VALIDATION.md).

The SHA-256 checksum for the single Flatpak download is shown below. Matching source, build scripts, locked dependencies, notices and the exact Poppler source are available in the [v0.1.0 source tag](https://github.com/juccul/Folio/tree/v0.1.0). GitHub also provides the tag's source ZIP/tarball. Cargo fetches unmodified registry dependencies from the lockfile; `scripts/package.py` can generate a complete vendored source archive for offline builds. Bundled math packages contain their Python source and notices. Folio is GPL-3.0-or-later; third-party licenses are retained.

```text
dc3b0af7ed2a9e8e95228f94f94b867993c147dd4b9f49bae933815264547249  folio-0.1.0-x86_64.flatpak
```
