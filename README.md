# Folio

Offline handwriting and mixed-media notes for Linux and Windows, built in Rust with GPUI. Raw tablet samples and editable vector ink are the document's source of truth.

Folio 0.1.2 includes native pen/pad input, vector editing, search over titles/tags/typed text, mixed-media/PDF notes, persistent undo, recovery and accessibility interfaces. Optional offline recognition converts selected handwriting into editable text or LaTeX. Normal note taking needs no model or Python runtime. Physical pen latency and desktop/device compatibility still require broader testing. See [DEVELOPMENT.md](DEVELOPMENT.md) for the exact status.

Windows 11 x64 build, portable package, per-user installer, and validation instructions are in [WINDOWS.md](WINDOWS.md).

## Releases

Download [Folio 0.1.2](https://github.com/juccul/Folio/releases/tag/v0.1.2) for **Linux x86_64 and Windows 11 x64**. Sign in to GitHub with access to this private repository.

| Format | Download | Install |
| --- | --- | --- |
| Windows installer | [folio-0.1.2-windows-x64-setup.exe](https://github.com/juccul/Folio/releases/download/v0.1.2/folio-0.1.2-windows-x64-setup.exe) | Run the installer; no administrator rights needed. |
| Windows portable ZIP | [folio-0.1.2-windows-x64.zip](https://github.com/juccul/Folio/releases/download/v0.1.2/folio-0.1.2-windows-x64.zip) | Extract the complete folder and run `bin/folio.exe`. |
| Linux Flatpak | [folio-0.1.2-x86_64.flatpak](https://github.com/juccul/Folio/releases/download/v0.1.2/folio-0.1.2-x86_64.flatpak) | `flatpak install --user ./folio-0.1.2-x86_64.flatpak` |

On Windows, launch **Folio** from the Start menu. Notes are stored in `%LOCALAPPDATA%\Folio`; updates and uninstall preserve them. The installer bundles offline math, PDF tools, Python and the matching Visual C++ runtime. See [Windows instructions](WINDOWS.md).

On Linux, launch **Folio** from your application menu, or run `flatpak run io.github.folio.Notes`. The Flatpak uses the Freedesktop 25.08 runtime. If Flathub is not configured, add it first with `flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo`. Installation may download the runtime; note taking and math solving then work offline.

The Flatpak includes the **offline CPU math solver**, pinned SymPy/mpmath dependencies and PDF preview tools. GLM-OCR Q8 weights and the Vulkan inference runtime download automatically on the first OCR request if no usable model pack exists; see [recognition setup](#install-optional-offline-recognition). Use the Flatpak file picker for importing and exporting files.

The [0.1.2 release notes](RELEASE_NOTES.md) describe the Linux and Windows downloads and matching source. `SHA256SUMS` accompanies the release assets.

The interface opens to a minimal document library with folders, favorites, recent notes, and grid/list views. Open a document for a compact writing toolbar and a collapsible page strip; the tab strip includes a + picker for opening an existing document or creating one. Drag tabs to reorder them. Right-click a library card or use its … menu to rename, duplicate, favorite, move, tag, trash or restore that document. Folio uses embedded Tabler Outline icons and a matching custom marker with native GPUI rendering.

The tab strip is Folio's title bar, with the + picker immediately after the tabs and minimize, maximize/restore and close buttons at the far right. Drag empty space to move the window; double-click it to maximize or restore. Window edges and corners resize it. Dragging a tab reorders documents. The title bar remains available in the library and above dialogs, and closing saves pending note changes.

**Settings → Appearance** provides neutral Light/Dark themes, separate custom colors for each mode, corner radius and reset. Paper follows the theme by default; turn off **Paper follows appearance** to keep white paper or choose a fixed paper color. **Keep ink readable** adjusts low-contrast handwriting and text only on screen and can be disabled. Original document colors, PDF/image backgrounds and exports stay intact.

## Tablet stability notice

The current release includes the fix for a Wayland tablet-pad announcement abort found in an early development build. The portable launcher temporarily uses X11/Xwayland on GNOME (`FOLIO_NATIVE_WAYLAND=1` opts into Wayland). Real Bluetooth Intuos testing also exposed a GNOME/Mutter 50.5 proximity-out crash with Folio closed; this app hotfix does not resolve that system crash. The user subsequently reported normal tablet operation after a restart. See [DEVELOPMENT.md](DEVELOPMENT.md) for the incident and exact verification limits.

For the specific Intuos BT S in this incident, two short sniff-disable interventions (60 and about 82 seconds) maintained operation; the user reported failures after restoration and on a subsequent unguarded reconnection. [The foreground compatibility helper](scripts/keep-tablet-active.py) maintains that per-tablet setting while running, with restoration when stopped. It requires sudo for the native HCI interface and installs no service or permanent setting. One-hour stability and GNOME crash prevention remain unverified; this is an optional diagnostic workaround, not a requirement for ordinary Folio use. The incident report records its command and limits.

Leave the helper off while the tablet works normally after the restart. The affected session's results do not establish a need for permanent Bluetooth changes.

The interface includes subtle panel transitions, animated settings switches and a **Reduce motion** preference. Settings and dialogs isolate keyboard focus; dismissing menus preserves your ink selection. Document loading cannot send pen input to the previous tab.

Large-page edits update object indexes incrementally. The canvas reuses visible ink and paper geometry between frames, and image preparation runs off the UI thread with bounded queues. Stale preview results cannot overwrite newer edits.

## Build and launch

Rust 1.92 or newer, a Linux desktop session, a C compiler, and Vulkan support are required. GPUI uses Vulkan on Linux; a working software Vulkan driver can also run it. Wayland is preferred, with X11/Xwayland fallback.

Install the native build/runtime packages for your distribution:

```sh
# Fedora
sudo dnf install cargo rust gcc gcc-c++ pkgconf-pkg-config \
  libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel \
  fontconfig-devel freetype-devel wayland-devel vulkan-loader mesa-vulkan-drivers

# Debian / Ubuntu
sudo apt install cargo rustc build-essential pkg-config \
  libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libfontconfig1-dev libfreetype6-dev libwayland-dev libvulkan1 mesa-vulkan-drivers
```

Distribution Rust packages must meet the minimum version; use a current Rust toolchain if they do not. The local linker adapter also supports installed versioned runtime libraries on minimal Fedora/Debian systems. `CXX=gcc` in the Cargo configuration builds FreeType's C sources on systems without g++; an explicit environment override takes precedence.

```sh
cargo build --locked --release -p folio
./target/release/folio
```

For X11:

```sh
WAYLAND_DISPLAY= ./target/release/folio
```

An initial dependency fetch needs internet access. The application requires no account and sends no documents to cloud services. First-use OCR setup downloads verified model/runtime assets; recognition then works offline.

Data goes to `$XDG_DATA_HOME/folio` or `~/.local/share/folio`. Override it with `--data-dir PATH` or `FOLIO_DATA_DIR`. One application writer is allowed per data directory. PDFs/images passed on the command line each create their own document. Home imports also create new documents; editor imports add to the document where you started the import, even if you change tabs while it runs. Desktop files and an SVG icon are in [packaging](packaging).

## Writing and editing

Lasso or rectangle-select handwriting and choose **Recognize text** or **Recognize math**. Review and correct the result, then choose **Copy text** to copy the entire corrected result while keeping the ink, or **Replace writing** to insert it in its place. Text recognition inserts an editable text block; math recognition renders the corrected LaTeX as an editable equation. Equation rendering happens in the background and keeps the ink until it succeeds. Invalid LaTeX returns to review for correction. Replacement is one undoable action, including after save/reopen. Copy text copies the LaTeX source for formulas. Cancel or Escape keeps the writing. Recognition runs on a separate worker, and results cannot replace ink that has changed or been deleted. Mixed selections recognize only visible ink; typed text, images and fitted shapes are left intact.

Both recognition actions use one shared **GLM-OCR** model. Text recognition renders the entire selection as one image, preserving paragraph layout; math uses the formula prompt and removes outer display-math wrappers before review. Select a line, short paragraph or individual expression. In the local CPU benchmark, GLM had 3.32% nonspace character error on 100 English handwriting lines and matched 30/100 handwritten formulas after formatting normalization. Its text accuracy improved over ConvText, while math accuracy was lower than Qwen's earlier 47/100 result. Results still need review. No correction service, account or cloud call is involved. See [GLM-OCR benchmarks](GLM_OCR_BENCHMARK_RESULTS.md).

Automatic first-use recognition uses Q8_0 weights with Vulkan GPU acceleration and CPU fallback. Existing Python BF16/INT8 packs remain supported. A separate [INT8 and 4-bit NF4 benchmark](GLM_OCR_QUANTIZATION_BENCHMARK_RESULTS.md) compares offline CPU/GPU accuracy, latency, storage and memory; its benchmark exports do not change the default recognition pack.

CUDA recognition uses an optimized image-patch projection. In the same-cohort [encoding benchmark](GLM_OCR_ENCODING_OPTIMIZATION_RESULTS.md), GPU text median fell from 6.03 seconds to 214 ms and math from 3.17 seconds to 272 ms; text character error stayed at 3.39%, with 31/100 formula matches versus 30/100 before. Model weights and image resolution are preserved. The earlier quantization GPU timings used the original encoder.

### Offline math solver and guides

Select handwriting, text, an equation or a cropped image and choose **Solve**, or open **Math solver** from the document menu. Handwriting/images use the current GLM-OCR pack; review and edit the recognized expression before solving. Existing equations skip OCR. Review the **Problem** and choose **Solve problem**, or press **Ctrl+Enter**. The **Automatic** dropdown beside that button offers arithmetic, simplification, factoring, derivatives, integrals, variable definitions and word translation. **Settings** contains the target variable, real/complex numbers and angle units. Use one equation per line for systems. Empty inputs offer **Try example** to run a sample problem.

The answer and full steps appear together. **Guide me** opens the first step; pinned **Previous step**, **Next step** and **Hint** controls help you move through one step at a time. **Show all steps** returns to the complete solution. **Copy LaTeX** and **Add answer** stay visible while you scroll. The chevron beside **Add answer** opens **Add options**: **Answer and steps** adds the worked solution as one undoable action; **Live answer** keeps a calculation updated.

Answers and step formulas use native vector outlines from the bundled LaTeX renderer. **Edit LaTeX** opens a focused answer editor with **Apply changes**, **Copy LaTeX** and **Cancel editing**. Apply returns to the updated answer; Cancel discards the draft. Invalid source stays in the editor and preserves the last valid formula. Applied edits are labelled as unverified, omit the original teaching steps and add as static editable equations; solve again to restore the computed answer and steps. Graph previews still use raster images while retaining vector SVG for insertion/export.

**Define variable**, in the operation dropdown, accepts `a := 5` or `b := 3*a`. **Preview variable** checks it; **Add variable** stores the definition on the current page. Definitions apply independently of their position; duplicate definitions, undefined dependencies and cycles produce errors. **Live answer** links a calculation to its expression and page variables. Editing a definition updates dependent results and graphs. Double-click a linked result to edit its underlying expression. Linked handwriting regions recognize again after the pen settles. This is opt-in for each added result. The **Graph** tab supports explicit `y = f(x)`; enter the x range, choose **Plot graph**, then **Add graph** or **Live graph**. Discontinuities appear as gaps.

Open **Check work**, enter the original problem and proposed next line, then choose **Check next step**. **Use handwritten step** reads a selected line from the page. Lost or added solutions are reported; undecidable comparisons stay unknown. **Solve problem** routes prose to supported English percentage, number, sum/difference, rectangle-area and speed/time templates. Review the translation and choose **Solve this equation**; **Translate word problem** in the operation dropdown also opens this flow explicitly. More general wording must be entered as math. Select one image and choose **Crop…**, then drag a rectangle over the image. For a PDF, choose **Read options** (the chevron beside **Use selection**) → **Read PDF region**, then drag a rectangle on the page. Escape cancels either selection. Repeated image crops compose with the previous crop; Reset crop restores the original image.

The math engine is a separate CPU SymPy worker; it needs no neural weights or GPU. This workspace's installed pack is `artifacts/math-solver/pack.json`, linked from `target/math-solver`. To prepare it in an existing compatible Python runtime:

```sh
/path/to/python -m pip install -r scripts/math-solver-requirements.txt
/path/to/python scripts/setup-math-solver.py --output /path/to/folio-data/math-solver
```

`FOLIO_MATH_CONFIG` overrides the data-directory or portable math pack. Installation may fetch dependencies; runtime calculation is offline and never downloads them. Teaching traces cover supported rules; other solvable problems can have explicitly labeled summary steps. Unsupported or incomplete results are reported. Multi-letter names are single variables; write `x*y` for a product. `log` is base 10 and `ln` is natural logarithm. Formula rendering falls back to source text when a step cannot be rendered.

Saved math links use document format 3. Earlier notes remain readable. The current database uses schema 6; older Folio builds refuse upgraded databases. See [implementation and coverage](MATH_SOLVER_DESIGN.md).

### OCR setup and offline recognition

Request **Recognize text**, **Recognize math**, or **Solve** on handwriting, an image, or a PDF region. If no usable recognition pack is installed, Folio automatically downloads GLM-OCR **Q8_0** and the llama.cpp Vulkan runtime: **1.47 GB** downloaded, about **1.56 GB** installed. Files come directly from Hugging Face and GitHub at pinned revisions, and each SHA-256 is checked before installation. The status bar and math panel show setup progress. **Cancel** or **Esc** stops the request; request OCR again to resume an interrupted download. On a connection or checksum error, the same OCR action retries setup. Notes remain editable during setup.

OCR prefers a supported discrete Vulkan GPU, then another available GPU, and falls back to CPU if no GPU can load the model. Both language and vision weights use Q8_0. No CUDA toolkit, PyTorch or Python OCR runtime is needed. First GPU use can take several seconds to initialize; subsequent requests reuse the resident model. Supported automatic downloads currently target **x86_64 Linux**; Intel Vulkan has not been tested. GPU drivers must already support Vulkan. The downloaded Ubuntu runtime requires glibc 2.34 or later; Folio's 0.1.0 release packages require 2.35 or later.

After setup, recognition works offline. Only model/runtime assets are downloaded; handwriting, images and PDFs stay local. The native recognizer runs with offline loading and an authenticated loopback endpoint that bypasses HTTP proxies. Flatpak enables network access for asset setup and keeps its existing GPU access. Downloaded files live under `recognition/glm-ocr-q8-b11457` in the Folio data directory (Flatpak: `~/.var/app/io.github.folio.Notes/data/folio`). Model and runtime notices are retained there. `last-runtime.log` contains runtime diagnostics.

Working Python recognition packs remain supported. `FOLIO_RECOGNITION_CONFIG` selects an explicit pack and reports errors instead of silently replacing a missing/broken explicit Python pack. Otherwise a data-directory pack takes precedence over a usable portable pack beside the executable's `bin` directory. To choose CPU or a particular GPU in a native pack, set `device` in `recognition/pack.json` to `cpu`, `auto`, or a device ID such as `Vulkan1`, then restart Folio. To switch an existing Python installation to automatic Q8, move its `pack.json` aside and restart; keep the old pack to restore it.

The following manual Python setup is optional for reproducing the earlier BF16/CUDA benchmarks.

To reproduce setup on another Linux machine, install Python 3.12, create a virtual environment, and install the CPU runtime while connected (or use an offline wheelhouse):

```sh
python3.12 -m venv /path/to/recognition-runtime
/path/to/recognition-runtime/bin/pip install torch==2.9.1 torchvision==0.24.1 --index-url https://download.pytorch.org/whl/cpu
/path/to/recognition-runtime/bin/pip install -r scripts/recognition-requirements.txt
```

Obtain the pinned [GLM-OCR checkpoint](https://huggingface.co/zai-org/GLM-OCR/tree/2e85a62840ccac27daa451df36c736c4636b8628), including `README.md`, `chat_template.jinja`, `config.json`, `generation_config.json`, `model.safetensors`, `preprocessor_config.json`, `processor_config.json`, `tokenizer.json` and `tokenizer_config.json`. Assemble the pack from those local files:

```sh
/path/to/recognition-runtime/bin/python scripts/setup-recognition.py \
  --output /path/to/folio-data/recognition \
  --ocr-model /path/to/glm-ocr --copy-model
```

Setup verifies every supplied model/config/tokenizer file against pinned SHA-256 hashes and retains the upstream model card and license declaration. GLM weights are about 2.65 GB (2.47 GiB), shared between text and math. Without `--copy-model`, the pack references the supplied model directory. `pack.json` stores an absolute Python path; recreate the runtime or update that path when moving the pack. Rerun setup to migrate an older ConvText/Qwen pack to GLM. An explicitly selected Python pack reports missing files in the UI. Its runtime disables Hub networking and internet sockets and uses built-in model classes. The resident process keeps the same GLM model loaded when switching between text and math. CPU is the default; optional `--device cuda` needs separately installed compatible CUDA wheels/drivers. Four CPU threads are used by default.

This workspace also has a ready CUDA pack at `artifacts/recognition-cuda/pack.json`. Launch the optimized GPU configuration with:

```sh
FOLIO_RECOGNITION_CONFIG="$PWD/artifacts/recognition-cuda/pack.json" target/release/folio
```

To try INT8, close Folio and launch the prepared CUDA INT8 pack:

```sh
FOLIO_RECOGNITION_CONFIG="$PWD/artifacts/recognition-cuda-int8/pack.json" target/release/folio
```

INT8 uses bitsandbytes LLM.int8 with threshold 6.0 for linear weights; other layers and image inputs retain BF16. The optimized patch projection stays enabled. It quantizes the verified original checkpoint during model loading, so disk files remain the original BF16 weights and initial startup includes quantization. The worker checks that INT8 layers exist and that all parameters remain on the chosen device. To create another INT8 pack, install `scripts/recognition-int8-requirements.txt` in the inference runtime and pass `--quantization int8` to setup. Returning to the previous CUDA pack restores BF16.

Setup includes `recognition_encoder.py` beside the worker. CUDA uses `encoder_projection: "linear_cuda"`; `--encoder-projection native` restores the original convolution for comparison. CPU uses its original operator. Restart Folio after changing its recognition pack or worker.

Choose **New document** in the library, press **Ctrl+N**, or use **+ → Create new document**. The **New notebook** dialog lets you name the document and choose pages or an infinite canvas, blank/lines/grid/dots, page size and orientation, and a theme-following, preset, or custom paper color. The preview updates as you choose. Cancel leaves your library unchanged; added pages inherit the canvas settings. Then draw with a pen, or use the constant-pressure mouse fallback. Wayland tablet-v2 frames retain position, pressure, tilt, buttons, eraser identity, hover and hardware timestamps. X11 uses XInput2 pressure/tilt valuators where exposed by the device driver.

Open the pen settings beside the quick widths and colors for presets: ballpoint, fountain, pencil, marker or highlighter. Width, opacity, stabilization and pressure response are editable. Choose whole-object or segment erasing in Settings. Each segment fragment retains the original raw stroke, and the erase is one undoable command. Lasso or rectangle-select objects, then drag, transform, duplicate, recolor or refine them. Conversion to a fitted shape keeps its original stroke available for undo and recovery.

Paged documents scroll as a continuous vertical stack, with the current-page indicator following the page in view. Click or write on a visible page to edit it; page thumbnails and previous/next controls jump to that page. Infinite canvases pan freely. Two-finger scrolling pans; Ctrl+scroll zooms around the pointer. Native Wayland pinch/rotation gestures navigate separately from pen input. Tablet-pad buttons have configurable actions, rings zoom and strips pan. The Pan tool and middle mouse drag also pan. Pages can use blank, ruled, grid or dotted paper, custom sizes, or an infinite canvas. The navigation row adds pages; the footer and thumbnail strip navigate them. The document menu handles tags, trash/restore, notebooks, page settings and recovery snapshots.

Hold the normal pen at an endpoint for 550 ms to preview a confident geometric fit; small endpoint tremors are tolerated at any zoom. Lifting also checks the held stroke if the preview timer missed it. Supported shapes include lines, arrows, rectangles/squares, circles/ellipses, triangles/diamonds, open circular arcs and angular paths such as an L. The Shape tool snaps on lift without a hold. Draw a two-stroke arrow with the same pen preset within five seconds and hold the end of the second stroke to combine it. Original source ink is retained and conversion is undoable. Low-confidence ink stays freehand. In Settings, enable **Scratch to erase** or **Circle and hold to select ink** to use smart gestures. Scribble back and forth across existing ink with three or more broad passes (or repeated overlapping loops), in any direction, then lift to erase. A rough circle/oval around content can have a small gap or overlap at the end; hold the endpoint for 550 ms to temporarily activate selection for moving, resizing and rotating. An outside tap clears the selection without making a dot; an outside drag returns to your pen and starts writing immediately. Choosing Lasso explicitly keeps that tool active. Explicit Shape mode and highlighter marks keep their normal behavior. Scratch erase and encircle selection are configurable and off by default. Stroke smoothing/refinement and manual space insertion are geometric and reversible; they do not synthesize handwriting.

| Shortcut | Action |
| --- | --- |
| P / E / L / H / T / S | Pen / eraser / lasso / pan / text / shapes |
| Ctrl+Z / Ctrl+Shift+Z | Undo / redo |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste objects, text or images |
| Ctrl+A / Delete / Escape | Select all / delete / cancel |
| Ctrl+S / Ctrl+F | Save / search |
| Ctrl+N / Ctrl+Shift+N | New note / new page |
| Ctrl+Shift+L / Ctrl+Shift+P | Library / page thumbnails |
| Ctrl+PageUp / Ctrl+PageDown | Previous / next page |
| Ctrl+0 / Ctrl+= / Ctrl+- | Fit page / zoom in / zoom out |
| Ctrl+O / Ctrl+Shift+E | Import / export PDF |
| Ctrl+, | Settings |

Click a text box to select it, then click inside again to edit on the page. The formatting bar below the drawing tools offers font, size, color, bold, italic, underline, alignment and lists. Text wraps as you resize its frame, while glyphs and font size keep their proportions. Editing saves continuously; Ctrl+Enter, Escape or Done leaves editing mode. The inline editor supports IME input, clipboard operations, caret movement and text selection. Selected images expose normalized crop controls. Drag selection corners to resize or use the rotation handle. Tab/Shift+Tab navigate notes and controls; Enter/Space activates them. Drop or import PNG/JPEG/WebP images. PDF backgrounds retain their original file plus separate vector annotations. Export PDF for all pages, SVG/PNG for the current page, or typed text and LaTeX source as plain text.

## PDF tools and typed equations

PDF metadata/import/export use Rust. Requested-page previews require `pdftoppm` from Poppler (`poppler-utils` on Fedora/Debian). Import retains source page order, crop boxes and rotation, replaces an unused blank first page, and supports password-protected PDFs. Rendering is lazy. Annotated PDF export preserves original PDF objects and adds vector ink and searchable subset-font text. Original files remain in assets.

Clean equations can be inserted by entering LaTeX in the document menu. A bundled open font and pure Rust renderer generate SVG offline; existing equation objects remain editable. Selected handwriting can be recognized as editable text or LaTeX through the optional offline OCR pack described above. Spellchecking and a graphical model/device configuration editor are not available. Shape snapping, scratch erase and encircle selection remain geometric pen features.

Search indexes titles, tags, typed text, LaTeX equation source and explicitly reviewed handwriting annotations with SQLite FTS5. Selecting a result opens its page and highlights matching objects. On opening an older database, Folio upgrades it to schema 6 with an indexed, stable page-to-search-row mapping. Databases predating schema 3 also rebuild the derived index to exclude old OCR text. Original stroke data, previous group metadata and undo history remain readable. The migration is transactional; older builds refuse the upgraded database. Incremental saves no longer scan unrelated library pages or rewrite unchanged search text. See the [performance audit](PERFORMANCE_AUDIT.md) for measurements and validation.

## Save, backup and recovery

SQLite WAL transactions save each completed command on a storage worker. Object payloads are updated individually; ordinary strokes do not rewrite the entire notebook. An active long stroke is checkpointed about once per second when autosave is enabled. Cancel removes the draft; normal completion replaces it atomically. Drafts are recovered on load.

Periodic database snapshots are kept next to `notes.sqlite3` (three automatic generations). The document menu can request one immediately. A save error is visible and prevents ordinary window closure from silently discarding changes. Ctrl+S explicitly saves with autosave disabled; a normal close saves every opened session.

Back up the **whole data directory**, including `assets/`. On corruption, the native recovery window can copy a healthy checkpoint or salvage readable records into a separate directory. The original database/WAL/SHM and assets stay untouched. You can also run `folio --data-dir PATH --recover`. The recovery report lists issues. Document menu → Quarantine unused assets moves unreferenced files into `orphaned-assets` after considering undo and checkpoints; it does not delete them. Undo/redo history survives reopening.

## Verification and packaging

Install the rustfmt and Clippy components for development checks (`rustfmt` and `clippy` packages on Fedora, or `rustup component add rustfmt clippy` with rustup).

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run --locked -p folio-app --example validate -- artifacts/validation
./target/release/folio --data-dir /tmp/folio-smoke --smoke-test
WAYLAND_DISPLAY= ./target/release/folio --data-dir /tmp/folio-x11-smoke --smoke-test
python3 scripts/dependency-notices.py
python3 scripts/package.py
python3 scripts/package-distros.py --math-python /path/to/math-runtime/bin/python
python3 scripts/package-flatpak.py --math-python /path/to/math-runtime/bin/python
cargo run --release --locked -p folio-app --example benchmark -- 10000
dbus-run-session -- python3 scripts/verify-accessibility.py
```

The native smoke test opens a real GPUI window, dispatches synthetic rich tablet frames at 8 ms intervals, validates pressure/tilt retention, exercises a stylus-operated selection button without drawing through its overlay, edits/pages, and flushes storage. This is not a physical tablet or end-to-end latency measurement. The headless validation generates real exports, searches Unicode text, imports a two-page PDF, and reloads the database.

On a private Xvfb/D-Bus session, add `--virtual-display --shortcuts` to `scripts/verify-accessibility.py` to check Ctrl+N, actual modal typing through the native clipboard, Escape and Ctrl+S. This option requires GTK 3 Python introspection bindings and targets only the launched test app's window; use a disposable display, never your desktop session.

The package script produces a Linux binary archive with notices and a corresponding source archive; it does not install or publish anything. The binary still depends on the system's Linux libraries, graphics driver, fonts and optional tools. Original source is GPL-3.0-or-later. Dependency and font notices are in [LICENSES.md](LICENSES.md).

To record physical input locally, start with `FOLIO_PEN_RECORD=/new/path.jsonl`; the file must not already exist. `FOLIO_PROFILE_INK=1` reports dispatch-to-CPU-paint metrics on exit. These exclude GPU presentation and display scanout. Recording is optional and bounded.

The 0.1.0 release publishes only the x86_64 Flatpak, with automatic OCR downloads and Vulkan Q8 acceleration. The native packaging scripts remain available for local builds. The release uses a Debian bookworm build (glibc 2.35 or newer). Build it with `packaging/Containerfile`, then pass `--binary artifacts/debian-target/release/folio` to the packaging scripts. RPM and DEB bundle the math worker with portable relative paths; `--math-python` must have the pinned math requirements installed. Flatpak requires the installed Freedesktop SDK/Platform 25.08 and verifies its pinned Poppler source before building. It grants display and GPU access, uses file chooser portals, and grants network access for first-use OCR downloads, and grants no host filesystem permission.

For the complete release source archive, run `scripts/package.py --no-build --binary artifacts/debian-target/release/folio`. The separate desktop tar archive excludes math and OCR runtimes; the release installers include math. No package copies development databases, model caches or virtual environments.

Installer math payloads retain the SymPy/mpmath runtime and distribution notices while excluding their upstream `tests` directories.

A cross-distribution build image is provided in `packaging/Containerfile`; it builds against Debian bookworm rather than this machine’s newer glibc. Package dependency metadata must match the chosen binary. Native COSMIC/KDE and physical tablet certification require their own sessions/devices.

## Organizing pages

Open the page strip with **Ctrl+Shift+P**. Drag a thumbnail onto another to reorder pages. **Duplicate page** creates editable content with new object identities, including remapped shape and live-math source links. **Name bookmark** labels a page; an empty name removes the bookmark. These changes support undo/redo and survive reopening.

**Move to notebook** lets you choose another document. Folio saves the destination copy before removing the source; moving the only page leaves a blank page. Undo in the source restores a copy, while the destination has its own undo history. If a save fails, the source is retained; an interrupted move may leave a destination copy. Restore trashed notebooks before moving pages.

## Library previews

The document grid shows the first page's content with a page count and last-edited label. In the document menu, **Use current page as cover** chooses a different page. If that page is deleted, the first page is used. Library rows are virtualized; visible covers load asynchronously from a consistent database snapshot without opening full editor sessions. The cover cache retains at most 128 pages and queues at most eight reads at a time.

## Learning the tools

Open Help and choose **Open starter notebook** to create an editable four-page guide to pen tools, held shapes, selection, recognition, solving and organization. Opening Help alone creates nothing and existing notes remain intact. Crop and PDF-region tools accept a rectangle drawn with the mouse or pen and reject stale selections after navigation or source edits.

## Portable notebooks and library backups

**Export → Editable notebook · with assets** writes a `.folio` archive containing editable vector objects, raw ink, page settings, bookmarks and referenced images/PDFs. Import it from the library to create a notebook, or from an editor to append pages. Imported identities and asset filenames are remapped, so importing the same file twice cannot overwrite another notebook. PDF/SVG/PNG exports remain available. Notebook archives contain current content; full undo history is included in library backups.

In **Settings → Backup and restore**, **Back up library…** writes a `.foliobackup` containing a consistent SQLite snapshot, all assets and quarantined assets. It preserves folders, preferences and durable undo history. Downloadable OCR/math runtimes and old automatic snapshots are excluded. **Restore backup to a new library…** asks for a backup file and a parent folder, then validates checksums and database integrity in a separate directory. **Open restored library** saves the current library before opening the restored one. The original remains intact.

Archives have a versioned SHA-256 manifest, reject duplicate paths, traversal, symlinks and special files, and allow at most 16,384 files / 2 GiB of uncompressed content. Notebook JSON is limited to 128 MiB. Export publishes the archive only after it is complete. Keep an external copy of library backups.

## Reusable page templates

Choose **Document menu → Save page as template…** to save a named snapshot of the current page, including paper, dimensions, headings, handwriting, images and PDF backgrounds. **Add page from template…** in the document menu or page strip opens a preview picker with Add page, Rename and Remove controls. Adding a template creates a separate editable page as one undoable action; later edits do not alter the saved template. Templates and their assets are included in library backups. Remove hides a template from the picker; asset quarantine handles its unused files without permanently deleting them.

## Search handwriting without replacing it

Select a line or short paragraph and choose **Index handwriting…**, or use **Document menu → Index page handwriting…** for a modest page. Review and correct the OCR text, then choose **Keep ink and index**. The normal text-recognition review also offers **Keep ink and make searchable**. Original vector strokes and raw samples remain intact; no background OCR starts merely because you write or search. The optional local OCR model uses the existing first-use setup.

**Ctrl+F** finds indexed handwriting with Unicode/accent-aware prefix queries. Opening a result highlights its source region. Moving, rotating or restyling the indexed strokes keeps the text and updates the region. Changing their geometry, deleting/replacing them, converting them to shapes/equations, or writing over the indexed region invalidates the annotation. Undo restores the writing and index together. **Clear page handwriting index** is also undoable. Page duplication, templates and notebook archives remap index source identities.

Dense pages may exceed the OCR selection/token limit; index smaller paragraphs instead. Indexing uses document format 4. The current database uses schema 6; older builds refuse upgraded databases. OCR annotations are reviewed text, not a guarantee of recognition accuracy.

## Check tablet input

Open **Help → Start input check** to inspect pressure, tilt, eraser and tablet-pad delivery while writing on a test page. Stop/reset the check and save a local JSON report. The timing report measures delivery to CPU canvas paint; physical pen-to-screen latency requires the [camera procedure and validation matrix](INPUT_VALIDATION.md). Physical desktop/tablet trials remain pending.
