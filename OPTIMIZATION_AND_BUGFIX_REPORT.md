# Folio optimization and edge-case audit

2026-10-05. The pass reviews controller mutations, asynchronous jobs, persistence, input fields, tab overflow, OCR review/replacement, math domains and rendering. All executable checks use temporary databases or private display sessions. The normal user application and its notes are not controlled by the tests.

## Fixes

| Area | Failure or edge case | Result |
|---|---|---|
| Database readers | Every tab load/search ran schema initialization and an integrity scan, requesting a write lock during ordinary reads. | Read-only connections validate the initialized schema version without running migrations or a full integrity scan. Startup retains its integrity check. Reads work with an outstanding WAL write transaction. |
| Load consistency | Metadata, pages, objects and undo history could come from different commits. | Document loading uses a read transaction; background loading captures document and history together from one committed snapshot. |
| Save feedback | A delayed acknowledgement overwrote “Unsaved” after autosave was disabled and new edits were made. | “All changes saved” requires no retained unsaved journals or failed notes. |
| Failed close/save | Full flushes did not associate their receipts with notes, leaving a failed close flush outside automatic recovery tracking. | Full flush receipts retain their note IDs and failed writes schedule recovery. Journals clear only after queue acceptance. A forced SQLite failure regression checks recovery after the failure is removed. |
| Export | Switching notes/pages or editing while the file dialog was open could export the newly active document. | Export captures the original note, page and content before opening the dialog, including completion of active ink. |
| Text selection | Shift-arrow reversal expanded the opposite edge rather than shrinking the selection; dragging backwards discarded the anchor. | Mouse and keyboard selection retain a fixed anchor and an independent moving end. Grapheme tests and native clipboard checks cover emoji and combining marks. Reversed IME ranges are normalized. |
| Crowded tabs | A newly opened or selected tab could be outside the scrolled tab strip. | Active tabs scroll into view on activation, reordering and window/UI-scale changes. Manual scrolling is preserved between those changes, and the + picker stays visible. |
| Geometry | Nonuniform scaling and shear produced brush bounds that could exclude visible ink. | Bounds use the transformed brush's separate horizontal and vertical extents. Swept erasing uses the same broad-phase bounds. A regression checks the transformed brush at 360 angles. |
| Malformed documents | Duplicate object IDs across pages, nonfinite font sizes, reversed rectangles, invalid styles/shapes and invalid inverses were accepted. | These fail validation before loading/pasting/rendering. Nonfinite or singular transforms have no inverse. |
| Preferences | Invalid numeric preferences could create oversized or nonfinite UI dimensions. | UI scale, cursor size and pen parameters normalize to supported ranges while retaining other preferences. |
| Math domains | Real-mode definite integrals over undefined subintervals returned complex answers marked verified. Reserved constants could be chosen as differentiation variables and return a misleading zero. | Definite integration retains original real-domain restrictions; isolated singularities still undergo convergence checking. Complex mode remains available. Constants cannot be target variables. Step checks infer a single unknown when the default target does not occur. |
| Page thumbnails | Converted shapes/equations were drawn along with their hidden source strokes. | Thumbnails exclude hidden sources, matching the main canvas's object visibility. |

## Optimization measurements

Release CPU measurements use generated notes, not a user's notebook. `crates/app/examples/benchmark_background.rs` reproduces the background measurements; `crates/app/examples/benchmark.rs` reproduces the ink/storage measurements. JSON output and logs are in `artifacts/validation/optimization-pass`.

| Measurement | Before | After | Scope |
|---|---:|---:|---|
| Page-variable lookup, median | 617.522 µs | 0.100 µs | 1,000 calls on a page with 10,000 ink objects and no linked calculations. |
| Database connection setup, median | 6,527.551 µs | 31.068 µs | 25 full initialization opens versus 25 background read-only opens on the same synthetic workload. |
| Active ink processing, median | 12.894 µs | 12.834 µs | 12,000 synthetic input samples; practically unchanged. |
| Incremental command/index update, median | 1.012 µs | 1.002 µs | 1,000 updates; practically unchanged. |
| Load 10,000 strokes | 279.818 ms | 276.706 ms | One full-load measurement per version; practically unchanged. |
| Initial spatial index | 8.580 ms | 9.731 ms | One measurement per version. Correct transformed bounds add work; this is not evidence of a speedup. |

Page-variable and live-calculation scans now inspect an incrementally maintained set of linked equations instead of revisiting every ink object. Execute, undo/redo and page changes update that set. Persistence checks page membership with a hash set instead of repeated linear searches/string allocation, avoiding quadratic work on notes with many pages. Stroke bounds collect transformed coordinates and maximum radius in one pass.

The gains above concern bookkeeping and database setup. They do not establish whole-app frame latency, OCR inference speed or hardware-to-display latency. OCR model weights, quantization and recognition prompts are unchanged.

## Verification

The complete workspace passes 162 Rust tests, including 14 new edge-case regressions. Both serial and default parallel execution pass. Python validation passes 21 math-engine tests, 12 OCR/preprocessing/loading tests four encoder-equivalence/fallback tests and one real-worker protocol test. The latter sends malformed requests, forces a one-second computation timeout and verifies that the same process subsequently solves a valid problem. Formatting, strict workspace/all-targets Clippy and Python compilation checks pass.

Private native checks cover X11 and Wayland title bars, movement, maximize/restore, minimizing, resizing, tab reorder/overflow, selection reversal, Unicode clipboard input, OCR text/math review and replacement, undo/redo, solver steps, guided navigation, live variables, graph palettes, editable LaTeX and compact layouts. The release passes 68 named native checks (17 X11 title-bar, nine Wayland title-bar, 35 solver/graph/editor and seven OCR checks), plus the pen/navigation smoke replay. Final release results and its SHA-256 are recorded in `artifacts/validation/optimization-pass/verification.json` after verification.

Earlier failure logs (`save-status-before.log`, `flush-before.log`, `geometry-before.log`, `math-before.log`) retain reproduced failures before their fixes. Existing regressions also cover stale/cancelled OCR and solver results, source movement/deletion, off-page results, PDF import/pinning/password paths, disk-full transaction rollback, corrupt-database recovery, asset references retained by undo and durable reopen.

This is a focused correctness and performance audit, not proof that every input is handled. Physical tablet timing, every Linux desktop/distribution and a fresh large OCR-accuracy corpus remain outside these isolated checks. Existing limitations include whole-path tessellation of very long translucent strokes, approximate thumbnail rendering and the solver's supported linear-system/word-template coverage.
