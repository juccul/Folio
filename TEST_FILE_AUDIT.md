# Test-file audit

Audited 2026-10-07 against source commit `d6e960e7bc910edae13bcde6b33eeacc1670851a`.

The maintained regression suites are mostly useful. There is **no entire owned Rust unit-test file that I recommend deleting**. The strongest cleanup targets are packaged dependency tests, obsolete verification branches, and generated copies/build outputs. This section records the findings before cleanup; the cleanup status is recorded below.

## Scope and evidence

Inspected tracked test/verification scripts, all seven standalone Rust test modules, embedded Rust test cases, example/fixture entry points, references from current source and documentation, vendored test targets, and ignored local test helpers/build outputs.

- 33 owned Rust files define 171 test cases: 170 ordinarily enabled and one explicitly ignored real-model OCR test. These counts include fully qualified Rust test attributes in the UI crate.
- 14 tracked Python files use test/verification names: five unittest suites, eight verification scripts and one Bluetooth diagnostic utility.
- Eight Rust examples provide fixtures, integration validation or benchmarks.
- Ran all five Python unittest suites separately: 58 cases total, 57 passed and one CUDA-only case skipped. No model downloads, physical-device commands or desktop input were run.
- Re-ran the 21 math unit tests and one worker-protocol test with a disposable staged SymPy/mpmath copy that excluded `tests` directories; all passed. Confirmed imports came from that copy.
- Did not rerun the full Rust, compositor or GUI suites for this read-only audit. Static inspection and existing release validation support the retention recommendations; this is not a mutation-testing proof that every assertion is necessary.

## Cleanup candidates

| Location | Finding | Recommendation |
| --- | --- | --- |
| `scripts/packaging_common.py:52` and staged `files/math-solver/site-packages/**/tests/` | Payload copying excludes bytecode but includes **682 SymPy and 36 mpmath test files**, totalling **10,025,378 bytes uncompressed**. Folio's solver does not invoke either library's test runner. Math unit/protocol checks passed without these directories. | Exclude directories named `tests` when staging the runtime. Retain package code, metadata and licenses. This is runtime payload cleanup; retain corresponding dependency source. The compressed Flatpak saving requires a rebuilt bundle to measure. |
| `scripts/verify-x11.py:46` | A small legacy shortcut probe. Its modal-input assertion only checks that the note count stayed the same after typing `p/e/l`; it does not prove input reached the field or that tools stayed unchanged. It has its own copied X11 event machinery, finds windows by title rather than child PID, and depends on a separately launched app. | Migrate its useful Ctrl+N check into the isolated accessibility suite and assert actual field/tool state. Then retire this file. It has some unique coverage, so deletion without migration is not a coverage-neutral change. |
| `scripts/verify-accessibility.py:22` and its `--without-recognition` branches | Historical mode injects retired configuration and asserts OCR controls/threads are absent. OCR is present again, so this mode no longer describes current Folio. | Remove that option and its setup/assertion branches. Keep the rest of the accessibility suite. Keep settings migration tests for old preference fields. |
| `crates/gestures/src/lib.rs:92` | Two tests cover `strike` and `insertion`; repository-wide references show these exported functions have no current production callers. | Retire these functions and their two tests together if the abandoned word-strike/caret features are being removed. The active scratch/encircle tests in `crates/gestures/src/tests.rs` must stay. Deleting only the tests would leave untested dormant code. |
| `scripts/test_math_solver.py:124` | The divergent `integral(-1..1, 1/x)` rejection assertion repeats the assertion in `test_real_definite_integrals_retain_original_domain`. | Optional assertion consolidation. Neither surrounding test method nor the file is useless; both cover additional behavior. |
| `artifacts/validation/{titlebar,tabbar-browser,optimization-pass}/run-smoke.py` | Three byte-for-byte identical 1,362-byte wrappers. They are ignored local outputs rather than maintained source. | Keep a reusable maintained launcher and retain logs/results if wanted; duplicate historical wrappers can be removed. |
| `target/debug/deps/folio_recognition-*` | 13 executable test artifacts belong to a recognition crate absent from the current workspace. | Obsolete generated binaries; clean build outputs rather than removing current OCR tests. |

## Tracked Python inventory

| File | Decision and reason |
| --- | --- |
| `scripts/test_math_solver.py` | Keep: 21 parser, domain, solution, explanation, graph and malicious-input regressions. One repeated assertion can be consolidated. |
| `scripts/test_math_worker_protocol.py` | Keep: real subprocess timeout, malformed requests and recovery. It uses an existing pack; the tested local worker matches current source. Missing runtime setup is a portability problem, not useless coverage. |
| `scripts/test_recognition_preprocessing.py` | Keep: 12 regressions for the still-supported Python OCR backend, including paragraph layout, wrapper preservation, truncation, alpha, INT8 and offline behavior. The native Q8 default does not make compatibility-path tests obsolete. |
| `scripts/test_recognition_encoder.py` | Keep: four equivalence/fallback/parameter-preservation tests for the optional Python/CUDA encoder. CUDA case skipped in this runtime. |
| `scripts/test_tablet_sniff_safety.py` | Keep: 20 tests for rollback, reconnect, placeholder handles and policy preservation in the diagnostic/helper tools. |
| `scripts/test-tablet-sniff.py` | Not a unit-test file: it is a Bluetooth diagnostic utility and is imported by `keep-tablet-active.py`. Do not delete because its filename starts with `test`. |
| `scripts/verify-accessibility.py` | Keep native actions, modal isolation, document management and preference checks; remove only the historical OCR-removal mode. |
| `scripts/verify-appearance-visual.py` | Keep for native visual review across palettes/paper/scaling and finite animation checks. Color-count checks alone do not prove layout correctness; saved images need review. |
| `scripts/verify-math-ui.py` | Keep: actual fields, clipboard, teaching, LaTeX editing, atomic insertion, graphs and compact layout. Covers interactions that controller-only tests cannot establish. |
| `scripts/verify-native-smoke.py` | Keep: isolated real-window launch/event dispatch, bounded wait and process cleanup. The assertions live in the app's smoke path. |
| `scripts/verify-recognition-ui.py` | Keep: real corrected-text/LaTeX clipboard, review, cancel, replacement and undo. The fake worker intentionally isolates UI behavior from model accuracy. |
| `scripts/verify-tablet-protocol.py` | Keep: executes actual vendored child-event registrations against Wayland metadata. It protects against the real tablet registration crash that synthetic pen replay missed. |
| `scripts/verify-titlebar-ui.py` | Keep: window manager behavior, native dragging/resizing, controls, tab overflow/reorder and close/save. Pure geometry tests are not substitutes. |
| `scripts/verify-x11.py` | Consolidate then retire, as described above. |

## Rust test inventory

Algorithm and controller tests sometimes share traces, but they exercise different boundaries: recognition geometry versus settings, document mutations, undo and durable storage. Shared samples alone are not grounds for deletion.

| File | Cases | Coverage / decision |
| --- | ---: | --- |
| `crates/app/src/appearance.rs` | 4 | Preference migration, independent palette overrides, persistence without document mutation. |
| `crates/app/src/gesture_tests.rs` | 8 | Controller integration: erase/select, false positives, temporary tool state, undo and reload. |
| `crates/app/src/lib.rs` | 18 | Editing, document locks, asynchronous loads, clipboard, recovery and durable undo. |
| `crates/app/src/management_tests.rs` | 4 | PDF/document management while other notes are active; exact page order and durable history. |
| `crates/app/src/math_solver.rs` | 10 | Review, cancellation, LaTeX insertion, live dependencies, image/PDF OCR and source preservation. |
| `crates/app/src/optimization_tests.rs` | 8 | Stale previews, coalesced loads, late saves, export snapshots and failed-flush recovery. |
| `crates/app/src/recognition/native.rs` | 9 | Download integrity/resume/cancel, installation locks, GPU selection, image preparation; one opt-in real-model test. |
| `crates/app/src/recognition/tests.rs` | 7 | OCR review and replacement, stale/cancelled results, rendering failures and durable undo. |
| `crates/app/src/settings.rs` | 2 | Malformed preferences and legacy settings migration; legacy fields remain valid migration coverage. |
| `crates/app/src/shape_tests.rs` | 8 | Hold timing, zoom/tremor, preview races, two-stroke joins and raw-source retention. |
| `crates/canvas/src/lib.rs` | 6 | Viewport transforms, spatial-index overflow, cache invalidation and derived-source visibility. |
| `crates/document/src/lib.rs` | 9 | Format migration, validation, transforms, object IDs and command history. |
| `crates/export/src/lib.rs` | 3 | Escaping, path traversal and ink opacity/geometry. |
| `crates/export/src/pdf.rs` | 1 | Duplicate source pages retain independent annotations and links. |
| `crates/gestures/src/lib.rs` | 2 | Two tests cover uncalled strike/caret APIs; conditional retirement candidate described above. |
| `crates/gestures/src/tests.rs` | 8 | Recognizer geometry, rate/rotation invariance, negative traces and detached marks. |
| `crates/ink/src/lib.rs` | 10 | Raw sensor preservation, brush geometry, hit testing, segment cuts and bounded simplification error. |
| `crates/input/src/lib.rs` | 1 | Real 32-bit timestamp wrap boundary; small but valuable test. |
| `crates/math/src/lib.rs` | 3 | Unsafe TeX rejection, retained sources and deep-nesting limits. |
| `crates/math/src/vector.rs` | 2 | Vector glyph holes/transforms and actual formula outline rendering. |
| `crates/pdf/src/lib.rs` | 3 | Rotation/crops, lazy large imports, encryption and password handling. |
| `crates/search/src/lib.rs` | 1 | Unicode prefix handling and safe queries. |
| `crates/shapes/src/tests.rs` | 10 | Primitive fitting, uncertainty, reported regression traces and multi-stroke joins. |
| `crates/storage/src/lib.rs` | 10 | WAL concurrency, snapshot consistency, migrations, disk-full atomicity, recovery and undo-aware asset retention. |
| `crates/ui/src/field.rs` | 3 | Selection reversal and grapheme-safe editing. |
| `crates/ui/src/graph.rs` | 2 | Palette substitution preserves graph geometry, including color collisions. |
| `crates/ui/src/math_panel.rs` | 2 | Guide boundaries and symbolic/prose routing. |
| `crates/ui/src/motion.rs` | 5 | Animation reversal, reduced motion, final-frame scheduling and bounded idle caches. |
| `crates/ui/src/navigation.rs` | 1 | Tab reordering preserves document identities in both directions. |
| `crates/ui/src/painting.rs` | 4 | Paper/raster caches, vertex overflow and retraced arrow geometry. |
| `crates/ui/src/theme.rs` | 4 | Custom paper contrast and source color/alpha preservation. |
| `crates/ui/src/titlebar.rs` | 1 | Tiled edges suppress invalid resize handles. |
| `crates/ui/src/validation.rs` | 2 | Nonfinite values, bounds and malformed form input. |

## Examples, benchmarks and fixtures

Keep `crates/app/examples/{ui_fixture,math_fixture,shape_fixture,recognition_fixture,validate}.rs`. They respectively supply disposable visual documents, native solver inputs, real hold-timing checks, opt-in model/controller checks and real export/PDF/search/reload validation. A fixture generator need not assert every behavior itself to be useful.

Keep `crates/app/examples/{benchmark,benchmark_background}.rs`, `crates/ui/examples/ink_geometry.rs`, and `scripts/benchmark-glm-*.py` as reproducible benchmarks. They are not unit tests or installer assets. Archived benchmark results do not make the maintained runners obsolete.

## Ignored local helpers

The following are historical local validation helpers, not tracked regression suites. Archive/remove these after retaining any desired evidence; do not delete their containing directories wholesale because those also hold models, environments, results or build inputs.

- `artifacts/validation/navigation-recognition/host-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/navigation-recognition/host-release-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/navigation-recognition/bundle-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/navigation-recognition/forwarded-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/navigation-recognition/verify-offline-archive.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/glm-recognition/verify-model.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/encoding-optimization/verify_worker.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/int8-app/verify.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/titlebar/run-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/tabbar-browser/run-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/validation/optimization-pass/run-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/benchmarks/recognition-2026-10-04/verify_preprocessing.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/benchmarks/vulkan-2026-10-07/flatpak-smoke.py` — Keep until replaced by a maintained isolated Flatpak smoke runner.
- `artifacts/release-1.0/verify-native-packages.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/release-1.0/verify-flatpak-smoke.py` — Historical helper; archive/remove after retaining desired evidence.
- `artifacts/release-1.0-refresh/verify-flatpak-smoke.py` — Keep until replaced by a maintained isolated Flatpak smoke runner.

`artifacts/release-1.0/source-check/folio-1.0.0-source/` also contains extracted copies of the source tests. These are release validation material, not independently maintained tests. Dependency tests under Python environments, extracted sources and benchmark dependency folders likewise do not belong to Folio's owned test suite.

At audit time, `artifacts/` used approximately 73 GiB and `target/` approximately 34 GiB on disk. Those totals include much more than tests. `target/debug/deps` alone contained 258 Folio executable build/test outputs with 2,223,165,384 bytes apparent size, including many old hashes. Do not confuse removing a few small source tests with recovering this build/model storage.

## Vendored files

Retain upstream tests and support modules in `vendor/`. The direct test directories contain one GPUI test (1,285 bytes), 31 proc-macro-error2 test/expected-output files (8,313 bytes), and one xattr test (3,228 bytes). They are not ordinary workspace test targets because those packages are excluded. Their small source footprint is not a useful optimization target.

GPUI `src/test.rs`, `src/app/test_context.rs` and `src/platform/test/**` are library test-support modules with conditional exports, not abandoned Folio tests. `src/inspector.rs` is an application inspector, not a test file. License/provenance records under `third_party/` and pinned dependency source are not cleanup targets.

## Suggested cleanup order

1. Exclude dependency `tests` directories from Flatpak staging, rebuild and rerun math/package checks; measure actual compressed savings.
2. Remove the obsolete accessibility OCR-removal mode.
3. Migrate the shortcut probe into the isolated suite, then delete `verify-x11.py`.
4. Retire uncalled strike/caret implementation and its tests together if those features are abandoned.
5. Archive historical local wrappers and clean obsolete build outputs separately from source tests.

No files were deleted during the audit phase. The subsequent authorized cleanup is recorded below.


## Cleanup applied

- Removed the obsolete `--without-recognition` accessibility mode and retired `scripts/verify-x11.py`.
- Moved Ctrl+N/modal typing/Escape/Ctrl+S coverage into the isolated accessibility suite's `--shortcuts` option. It asserts actual copied field text and unchanged document objects rather than only comparing note counts.
- Removed the uncalled strike/caret functions together with their two tests; retained active scratch/encircle coverage and made the test module's document imports explicit.
- Consolidated the repeated divergent-integral assertion while retaining both surrounding mathematical regression cases.
- Excluded upstream dependency `tests` directories from installer payloads; runtime modules, metadata and notices remain.
- Deleted the 14 historical helper files listed above and 13 obsolete recognition-crate executables: 27 generated files, 23,277,361 bytes apparent size. Preserved the two current Flatpak/Vulkan helper files, results, model caches, environments and vendored source.

Validation: all 168 default workspace tests passed, with the one real-model OCR test still explicitly ignored. All 21 math unit tests and the worker timeout/recovery protocol test passed. The isolated native accessibility/shortcut check passed, including actual modal text/clipboard, note creation, page navigation and document preservation. Formatting, Python syntax and source whitespace checks passed. Package rebuild measurements are recorded in `artifacts/test-cleanup/`.

The rebuilt local Flatpak is `artifacts/test-cleanup/dist/folio-1.0.0-x86_64.flatpak`: **16,847,288 bytes**, versus 18,426,648 bytes for the published refresh, a reduction of **1,579,360 bytes (8.57%)**. The 21 math unit cases and one timeout/recovery protocol case also passed using the actual Flatpak Platform runtime and trimmed payload. At the cleanup stage, the GitHub release still contained the previous artifact; that step produced a local replacement for review.

The rebuilt bundle installed successfully in a private Flatpak installation and passed the native editor smoke check. Display, network and GPU permissions remain present.


## Public version reset

The cleaned app and installer are now published as Folio **0.1.0**. Historical build paths and audit counts above retain the versions they actually tested. Current release metadata, installation instructions and corresponding source use `v0.1.0`; the earlier `v1.0.0` release is superseded by this version reset.
