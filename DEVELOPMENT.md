# Development status — Folio 0.1.2

Updated 2026-10-11. Folio is a runnable Rust/GPUI Linux and Windows desktop application. Production certification still requires physical tablets and additional desktop/distribution sessions; synthetic events and CPU benchmarks do not establish hardware-to-display latency.

Version 0.1.2 adds bounded settled-ink caching on Windows, swept-area scribble erase with broader gesture recognition and letter-fragment completion, and a latched smart-shape preview that tolerates small endpoint movements. Windows setup/portable and signed Flatpak packages share the same release source. See `WINDOWS.md` for Windows Ink support and validation limits.

## Continuous integration

The `Linux and Windows` workflow tests every branch push, pull request and tag,
and can be started manually with `workflow_dispatch`.
New pushes cancel superseded runs on the same branch,
and pull request updates cancel superseded runs for that pull request. Release
tag builds and manually started builds are not cancelled by branch updates.

Rust tests run with two test threads in CI to limit contention between each
controller's storage and rendering workers. Asynchronous worker behavior remains
covered. To use the same limit locally, run
`cargo test --locked --workspace -- --test-threads=2` on Linux or
`./scripts/build-windows.ps1 -Test -TestThreads 2` on Windows. Omitting
`-TestThreads` preserves Cargo's default parallelism for local Windows builds.
Updater, packaging, math protocol and Rust checks have separate CI steps so the
failing check is visible directly in the run summary.

## Scratch erase — current source

Recognition separates a geometric scrub candidate from the decision to erase. Compact retracing accepts three broad passes with an ink hit; advancing up/down scrubs accept six broad legs and require three legs to cross existing non-highlighter ink. Rounded corners, unequal amplitudes, diagonal travel and slow gestures are supported. Both footprint/travel axes and their perpendiculars are inspected. Bounded arc-length resampling preserves the peaks of long gestures without depending on device report rate.

Deletion uses actual transformed ink against the gesture path and small convex regions between adjacent passes. It fills the gaps between zigzag legs without erasing the whole bounding box. Short legs and exit tails retain their direct crossing coverage. Disconnected dots/crossbars are compared in the word's orientation and attached nonrecursively; rotated long word boxes cannot turn adjacent letters into apparent tiny marks. Typed content, images and standalone equations remain excluded. Held encircle selection takes priority, opt-in defaults stay unchanged, and erase/undo/redo is one operation.

Regressions include synthetic approximations of the three reported scribbles, rounded/unequal passes, fragments inside swept gaps, footprint corners and neighboring lines/words, long/slow gestures, sparse/dense reports, mouse/tablet events at multiple zooms, rotated detached marks and undo/redo/reload. These are geometry/controller checks, not physical pen recordings. A repetitive wave deliberately drawn across existing writing remains inherently ambiguous with a scrub; the overlap gate protects empty-space writing and incidental contacts, not every possible handwriting intention.

Two additional fixtures trace the white gesture and visible blue ink directly from reported screenshots: large rounded scrubbing across multiple lines and unequal rounded scrubbing across a single line. Raster centerlines are simplified within 0.8 pixels and visible ink is split at junctions. These verify triggering against screenshot geometry; a screenshot cannot recover pen lifts, timestamps or occluded ink, so they do not establish complete original-letter deletion.

For local testing, close Folio normally and run `cargo run --locked -p folio`, or `cargo run --locked --release -p folio` for the optimized profile. Each command builds and runs its corresponding executable. `target/debug/folio` and `target/release/folio` are separate builds; rebuilding one does not update the other or replace code already running in an open window.

## Compact editor toolbar

The editor uses one 44 px row below the unchanged 40 px document tabs at 1× UI scale. Page thumbnails/search and document actions/settings sit in separate 36 px outlined groups; editing tools use the base background with individual selection highlights. Rename remains available in Document actions. Below 1100 logical pixels, stroke width and color controls are available through Pen settings; eraser modes and sizes use the same popover. The tool area can scroll horizontally at larger UI scales without expanding the header. Read-only documents keep page navigation and document actions without editing controls.

## Optimization and edge-case audit

The 2026-10-05 audit fixes reader/write contention and inconsistent loads, export-dialog targets, save acknowledgements and failed-flush recovery, selection reversal, tab overflow, malformed geometry/preferences and definite-integral domains. It adds indexed linked-math bookkeeping and faster database reader setup. Validation is under `artifacts/validation/optimization-pass`.

## Application title bar

Folio requests client decorations and uses the document tab strip as its title bar. The + picker follows the last tab; tabs scroll when space runs out, while the picker stays visible and minimize, maximize/restore and close stay at the far right. Empty-space presses start compositor-driven movement; double-click toggles maximize and right-click opens the native window menu. Tabs consume their own presses and retain drag reordering. Edge/corner hitboxes resize floating windows and are omitted on maximized/fullscreen or tiled edges. The library shows the same title bar without changing the open-tab list. Dialogs occupy the area below it, and window controls remain accessible during overlays and document loading. Close explicitly flushes pending note changes and reports failure instead of removing the window.

The vendored X11 backend now allows opaque client frames without a compositor or GTK frame-extents support; transparent frames retain the existing server-decoration fallback. Verification is recorded under `artifacts/validation/titlebar`, including private Mutter/Xwayland window movement, tab reordering, controls and resize, native Wayland controls, and existing pen/navigation and solver regressions. No physical desktop or tablet is controlled by these tests.

## Offline solving and live math

The independent CPU SymPy service supports selected-problem solving, guided steps, systems/inequalities/calculus, page variables, linked results, explicit function graphs, next-step checking and reviewed word-problem templates. Computation, LaTeX rendering and PDF/image preparation run outside the UI thread. Cancellation, captured source objects, variable snapshots and page/PDF identity prevent stale results from being inserted. Live handwriting regions wait for pen-idle input.

The native panel renders vector formulas and keeps editing, copying and insertion separate. Worked insertion is one durable command; edited results insert as static Equation objects. Keyboard shortcuts do not switch drawing tools while a math field is focused. See [MATH_SOLVER_DESIGN.md](MATH_SOLVER_DESIGN.md) for usage, engine limits and packaging.

## Explicit offline recognition, copy and replacement

Text and math recognition share a resident GLM-OCR backend. The default native Q8 setup downloads verified model and Vulkan runtime assets after an explicit first-use choice, then processes images locally with GPU selection and CPU fallback. Existing configured Python packs remain supported. See [OCR_FIRST_USE.md](OCR_FIRST_USE.md) for setup, pinned assets, privacy and validation limits.

Results open an editable review. Copy leaves the document intact. Replace writing removes only captured strokes and inserts editable text or a rendered equation in one journaled command. Math validates and renders before changing the page; failure restores the corrected review and retains the ink. Note/page/source identity is checked before accepting and replacing, and generations reject cancelled responses. Original sensor data stays in durable undo history.

Keeping ink and indexing the reviewed text makes handwriting searchable without replacing its strokes. Editing or deleting a source invalidates its reviewed index; undo restores it. There is no automatic text detector.

## Rendering and persistence performance

Ordinary edits update only changed spatial-index entries and the source references of derived shapes/equations. Reference counts keep an original stroke hidden when several derived objects share it, including after undo/redo. Command application no longer scans every page to restore drawing order; appended ink keeps its existing order, while insertion/deletion restoration remains exact. Session drawing-order ranks update incrementally for appended objects and are rebuilt for removals or middle insertions.

The renderer caches the ordered visible object set until the page revision, spatial-index generation or viewport changes. Completed path-cache maintenance runs when that set changes rather than on every pen frame. Ruled, grid and dotted paper share one cached compound path per viewport/theme instead of rebuilding many small paths on each redraw. Text/image/equation pixel copies and theme adaptation run on GPUI background tasks, with at most two conversions in flight. Rendering rejects stale source objects, pixels and theme results. The controller trims preview/session caches every 250 ms instead of scanning them every UI tick. Worker results are bounded to eight items rather than 64, limiting queued decoded-image accumulation; this is not a bound on total application RAM.

Windows additionally caches consecutive settled ink in ordered batches of up to 32 strokes. Two background raster jobs run at most; cached images are limited to 16 million pixels per visible page and one million pixels per batch. Device-pixel density is preserved, and oversized batches use vectors. Selection, erasing, object changes, themes and display density invalidate affected batches immediately; stale jobs cannot replace current ink. Dropped pages/images release their atlas entries. Live ink and exports remain vector-based. The isolated `cargo run --release -p folio-ui --example ink_layers` fixture exercises 1,024 strokes, selection and theme invalidation without opening installed update caches. `cargo test --release -p folio-ui benchmark_dense_saved_ink_scene_preparation -- --ignored --nocapture` measures CPU scene geometry only, not GPU frame time or physical input latency.

Scratch erase accepts sustained advancing up/down scrubs across existing writing, without requiring horizontal return passes. It still requires broad reversals with deliberate turnarounds, rejects smooth waves and ordinary W/M handwriting, targets actual ink crossings and remains one undoable operation.

Preview completions/errors for edited, deleted or evicted objects are discarded. Offscreen-page results cannot insert their bounds into the active page's index. Higher-resolution previews are retained when a smaller render budget would already suffice. Repeated document-load requests are coalesced, and a failed queue submission no longer leaves a tab indefinitely pending. A failed held-pen shape fit runs once until the pen moves substantially, preventing repeated stationary fitting; endpoint tremor tolerance and continued drawing/shape recognition remain intact. Saturated grid-cell counts use checked arithmetic; extreme imported coordinates and non-finite query bounds cannot overflow the index or cause huge grid allocation.

Validation and measured CPU results for this pass are recorded in `artifacts/validation/optimization`. The benchmark measures command application plus spatial-index maintenance separately from drawing, persistence and presentation; its reduction must not be read as an equivalent improvement in physical pen latency. Full-document save/load still scale with document size. No external dependency, document format, recognition component or system input configuration was added.

## Tablet incident — 2026-10-03

Latest outcome: the user reports that the tablet works normally after a restart. No permanent compatibility service or Bluetooth configuration change was installed. Leave the optional helper off while normal operation continues. The short sniff-disable trials describe the affected session; they do not establish that future sessions need a workaround. The restart's scope and subsequent stable duration were not captured, so the exact cause and lasting resolution remain unconfirmed.

Real Intuos BT S testing exposed an abort missed by synthetic smoke replay: the Wayland pad group child event was registered as opcode 1 instead of 0. The hotfix uses generated protocol opcode constants for all tablet child registrations. `scripts/verify-tablet-protocol.py` executes those registrations against the protocol's child-event metadata using a private socket pair, without connecting to GNOME or input devices. It fails on the earliest development build and passes all six child events with the fix. Those early development packages were withdrawn; their synthetic input checks did not establish real-device stability.

The user's GNOME/Mutter 50.5 also crashed twice after Folio had exited. An October 1 core has the identical fault offset (`libmutter-18.so.0 + 0xa9e61`), while this project's files were created on October 3; the same compositor fault had occurred before Folio. Both cores resolve to `meta_wayland_tablet_update_sprite` writing through a null current tablet during proximity-out, after Bluetooth tablet reconnects. System input/GNOME package checksums are intact, their installs predate this work, and no Folio service, system driver, udev/Xorg rule or global accessibility setting was installed/changed. Root cause of the Bluetooth proximity sequence remains unresolved; fixing Folio's child-event registration does not fix this compositor failure or exclude an indirect trigger in the affected session. No system package, driver, pairing, GNOME extension or desktop configuration was reset by the agent.

The incident-era X11 launcher workaround is retired. The current launcher prefers native Wayland; `FOLIO_FORCE_X11=1` selects X11 for Folio's process only. Backend selection does not prevent GNOME from processing Bluetooth proximity events while Folio is closed. Native Wayland hotplug remains unverified on physical hardware.

Live diagnosis established Bluetooth link timeouts with Folio closed. USB runtime state was active at the observed drops and suspended afterward; GNOME stayed alive in the later sessions. An HCI trace showed one link becoming silent around sniff-mode entry and timing out, while a subsequent active link delivered 2,243 reports over 41.4 seconds. Sniff mode is now the primary hypothesis, with the exact controller/remote-firmware/RF cause unresolved.

The corrected per-connection experiment **completed on physical hardware**: handle 11 changed from policy 0x000f to 0x000b (only sniff disabled), survived 60 seconds, delivered 2,995 reports, and restored 0x000f with successful controller readback. The user confirmed the tablet disconnected by itself after the command finished. The post-restoration disconnect occurred outside the HCI trace, so its exact reason/time are not captured.

The user then ran the foreground helper on handle 12. Its log confirms policy 0x000f → 0x000b at 19:04:01, followed by restoration to 0x000f when interrupted at 19:05:22 (81.896 seconds). The user reported normal tablet operation throughout, failure immediately after stopping, and failure on reconnecting without the helper. Two short interventions now support a repeatable temporary workaround. The helper was configured for one hour but interrupted after about 82 seconds; one-hour stability, the exact faulty component and GNOME crash prevention remain unverified. These observations do not establish how the session became unstable or exclude an earlier indirect Folio trigger.

`scripts/keep-tablet-active.py` provides an optional foreground helper scoped to this Intuos address. It keeps the sniff bit disabled on that device's connections for a bounded session (default one hour), restores the current link's original sniff bit on normal/interrupted exit, and preserves other policy flags. It never connects/scans/resets, changes adapter defaults, installs services or edits persistent settings. It logs policy metadata only. Native policy changes and interruption cleanup were verified in the hardware trials; long-session helper behavior remains unverified. A root-owned runtime lock prevents duplicate helpers. Linux requires sudo for the HCI policy interface.

Twenty simulated safety tests cover pending/reserved handles, device identity, rollback, interruption, link loss/reconnect, policy drift, other-bit preservation and duration expiry; native ABI was independently checked with a C harness against upstream headers. The first experiment's pending-handle I/O error was a diagnostic bug and was fixed before the successful trial. Read-only utilities remain available for raw tablet events, BlueZ disconnect signals and private HCI traces. HCI traces contain pairing-key material and remain local/private. No Bluetooth reset, pairing/driver change or persistent compatibility service has been applied. See [incident report](artifacts/incident/REPORT.md) for exact evidence and limits.

## Architecture

The workspace separates `apps/desktop` and `app`, `ui`, `input`, `ink`, `canvas`, `document`, `storage`, `gestures`, `shapes`, `math`, `pdf`, `search`, `export`, and `platform` crates. Rust 1.92+ is required by the subset-font PDF backend. Ink, geometry and structured documents have no GPUI dependency. The UI forwards operations to a controller; document mutation and LaTeX rendering are outside the presentation layer. Explicit optional recognition lives in `app::recognition`, with a resident native Q8 backend and support for existing Python packs; no automatic text detector is present.

Native Linux GPUI patches provide Wayland tablet-v2 frames, tablet pads and pointer pinch gestures, and XInput2 pressure/tilt. Six pen styles share an incremental pressure/velocity filter and vector outlines. Raw and processed vectors are shared immutable arrays so transformations and undo do not copy sensor buffers. Raw samples survive smoothing, transforms, segment erasing, shape/equation conversion and refinement. Each erased fragment keeps original sensor samples plus an independent visible path.

Completed paths are cached; opaque active ink is divided into 256-point chunks with a bounded mutable tail. Translucent active ink uses one compound fill for consistent overlap opacity. A uniform-grid spatial index updates changed objects and derived-source reference counts and culls offscreen content. Visible objects are cached in document order until their revision, index generation or viewport changes. Paper geometry is also cached. Images, text and equations use bounded worker-generated previews; GPUI transforms those raster sprites on the GPU, keeping them aligned with vector ink during pan, zoom, resize and rotation.

Message-passing workers separate persistence, document/rendering processing and search/load. Each expensive processing queue holds at most 16 jobs; the result channel holds eight items. GPUI pixel preparation has a separate two-task bound. Search generations coalesce obsolete search jobs. Persistence uses a 64-item channel; saturation or disk errors retain the live document and bounded command history for a full retry rather than blocking pen input or accumulating failed deltas indefinitely.

## Completed functionality

- Native Wayland and X11 windows; pressure, tilt, eraser identity, buttons, hover and timestamps; cancellation on device removal. Native Wayland tablet-pad buttons are configurable, rings zoom and strips pan. Native touchpad pinch, rotation, translation and cancellation are separate from pen input. Pen proximity suppresses pointer emulation on the canvas.
- Ballpoint, fountain, pencil, marker and highlighter; whole-object and segment erasers; saved presets, pressure curve, stabilization, opacity, recent/custom colors, configurable cursor size.
- Fixed/infinite paper, custom sizes, blank/ruled/grid/dot backgrounds, pages, fit, zoom, pan and rotation.
- Lasso/rectangle selection, move, corner drag resize, rotation handle, duplicate/cut/copy/paste/delete, restyling and command undo/redo. History survives reopening.
- Notes, tags, favorites, recents and trash; nested notebook/folder creation, rename, safe parent changes with cycle prevention and empty-folder deletion. Note entries and controls participate in keyboard focus traversal.
- Conservative scratch, encircle and hold-to-shape gestures. Destructive smart gestures require opt-in. Deterministic fitting covers lines, arrows, rectangles, squares, circles, ellipses, triangles, diamonds, open circular arcs and angular polylines.
- FTS5 search/navigation/highlights over titles, tags, typed text, LaTeX equation source and explicitly reviewed handwriting; no automatic text detector.
- Wrapped Unicode text blocks, styling/alignment/lists; images with crop controls and transform handles; editable LaTeX equation objects preserving source handwriting.
- Lazy PDF import without a 500-page cap, requested-page Poppler previews, password dialogs, encrypted original retention and unlocked local working copies. Original assets and annotation objects remain separate.
- SVG/PNG/plain-text export; searchable subset-font vector PDF annotation layers over original PDF objects. Original vectors, page text, links, existing attachment/name trees and form dictionaries remain present; duplicated source pages get independent overlays and annotation references.
- Non-destructive smoothing/refinement and manual geometric insertion/removal of space.
- Goodnotes-inspired library/editor layout, document tabs, cached page thumbnails, Tabler Outline and a matching custom marker, light/dark themes, UI scaling, standard shortcuts, keyboard-accessible controls and an AccessKit AT-SPI bridge for document text, controls, status and text/password dialogs.
- Native corruption-recovery window and `--recover`, conservative record salvage, snapshots, atomic asset writes, orphan quarantine, transaction-safe disk-full behavior, desktop/AppStream metadata and source/license-aware Linux packaging.

## Persistence and recovery

SQLite uses WAL, `synchronous=FULL`, foreign keys, stable UUIDs, per-object rows and atomic document/search/journal transactions. Schema version 6 adds an indexed, stable page-to-FTS-row mapping independent of SQLite page rowids, plus note/page-position and draft indexes. Every writer open upgrades older databases transactionally; versions 3–5 retain their existing valid FTS rows, while older databases first rebuild legacy OCR search content. Earlier builds reject schema 6. Search updates use rowid lookups and skip unchanged content. The current document format is 4. Format 3 introduced optional math links alongside the version 2 cut-path and character-group records; earlier document formats remain readable. Editing upgrades the format, and clipboard writes use V2 while accepting V1. Each history holds at most 512 commands; full catch-up saves replace its journal consistently with the current live state.

Normal edits write changed objects and affected page headers/FTS rows. Metadata-only edits update searchable titles/tags without rebuilding every page. Active ink drafts are saved approximately once per second. Completion clears the draft in the stroke transaction. Periodic SQLite snapshots retain three generations. Assets must be backed up with the database.

Recovery never overwrites or renames the original data. It prefers a healthy checkpoint, otherwise salvages independently readable records into a new sibling directory, copies assets, and writes `RECOVERY.json` with issues. Asset maintenance considers current documents, undo history and retained snapshots and moves unused files to `orphaned-assets`; it does not permanently delete them. Damaged snapshots stop maintenance rather than guessing references. Save errors remain visible and retry from the retained live document.

## Dependency and packaging decisions

GPUI 0.2.2 is pinned/vendored; native input and GPU sprite extensions are recorded in [vendor/README.md](vendor/README.md). Geometry algorithms are original implementations informed by perfect-freehand, Xournal++ and Rnote; no algorithm source was copied. Krilla/krilla-svg provide maintained subset-font PDF output, lopdf preserves original objects/decrypts PDFs, Poppler renders only requested background previews, and latex-rust renders equation outlines locally. [LICENSES.md](LICENSES.md) records the locked Rust graph and retained notices.

`scripts/package.py` produces a desktop archive and complete Cargo-vendored corresponding-source archive with notices. Models and Python inference runtimes are excluded. `scripts/package-distros.py` creates RPM and Debian packages without installing them. The launcher prefers native Wayland when available; `FOLIO_FORCE_X11=1` selects X11 for that process only.

## Validation and measurements

Run `cargo test --locked --workspace -- --test-threads=2`, `cargo fmt --all -- --check`, and `python3 scripts/check-documentation.py`. Workspace tests cover document compatibility, raw-data preservation, ink/gesture geometry, undo/redo/reopen, storage recovery and rollback, search, recognition review, solver cancellation and stale results, and UI cache/field behavior. Native smoke replay and accessibility checks use private desktop sessions and throwaway documents; synthetic checks do not establish physical tablet reliability.

Reproduce CPU measurements with `cargo run --release --locked -p folio-app --example benchmark -- 10000` (also use `50000`). Command application, spatial updates, initial save and load are separate measurements; they exclude GPU presentation, compositor and physical-device latency. `FOLIO_PROFILE_INK=1` reports dispatch-to-CPU-paint timings. `FOLIO_PEN_RECORD=/new/path.jsonl` records rich pen frames asynchronously; a bounded queue drops diagnostics instead of delaying ink. Keep generated evidence under ignored `artifacts/validation/`.

## Remaining validation and technical limits

- A physical Intuos BT S was used for incident and OS transport/policy testing. Folio's physical pen rendering and measured 120 Hz hardware-to-display latency remain unverified. Palm rejection, HiDPI tablet calibration, pad layouts and hardware button mappings need further device trials. The app suppresses pointer emulation during pen proximity; the compositor remains responsible for rejecting native touchscreen palm contacts.
- Native work is verified on this Fedora GNOME Wayland/Xwayland session, with builds/tests also run in Debian bookworm. COSMIC, KDE, other distributions, screen-reader workflows and HiDPI sessions require their own runs. Packaging and AT-SPI interfaces do not substitute for certification.
- Original PDF objects are preserved, but digital signatures become invalid when an annotation export changes the document. Complex multi-source named destinations, conflicting form resource/name dictionaries and unusual PDFs need broader corpus testing. The original retained files remain intact. Preview resolution is bounded; vector originals are retained for PDF export.
- Pixel previews use a 128 MiB controller cache and adapt to the visible working set; GPU/GPUI image copies and visible path geometry have additional costs. Extremely large visible geometry still requires GPU profiling. Dirty note sessions are retained instead of evicted when saves fail or autosave is disabled.

Priorities are physical pen trials, larger ink workloads and PDF corpus evaluation, then cross-desktop/distribution and assistive-technology certification. No account, network service, proprietary SDK or sync backend is required.
