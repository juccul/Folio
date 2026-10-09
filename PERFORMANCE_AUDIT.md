# Performance and optimization audit

This pass audits the native Rust/GPUI application across ink/input/canvas geometry, rendering and accessibility, controller bookkeeping, persistence/search/recovery, OCR preparation, math rendering, and document/PDF/export paths. Three agents worked on separate areas, followed by combined review and regression checks. Baseline source is commit `6c031e6`; measurements use generated fixtures and temporary databases.

## Changes

| Area | Bottleneck | Implementation |
| --- | --- | --- |
| Incremental persistence | FTS deletion scanned unrelated library rows on each save | Schema 6 adds a stable page-to-FTS-row locator, indexed page/draft ownership, cached statements, and skips identical search content. |
| Deferred history | Every edit cloned the entire undo history while autosave was disabled or saving failed | Track unsaved history with a flag, skip unused persistence deltas for deferred edits, and materialize a replacement only when queuing the catch-up save. Preserve durable undo/redo and error recovery. |
| Selection edits | Transforming one object visited every object on its page | Resolve selected IDs through cached order positions, sort only the selection, and fall back safely if callers change the active page directly. |
| Library covers | Every repaint cloned the loaded page and searched/cloned metadata | Reuse an `Arc<Page>` snapshot by page ID/revision; invalidate for page replacement even when ID/revision are unchanged. Thumbnail workers take snapshots only on cache misses. |
| Fragment restyling | Every fragment sample linearly searched the original trace | Balanced exact 2D nearest-neighbor lookup for larger fragments; retain the small-fragment linear path and deterministic tie handling. |
| Erasing | Misses cloned full paths; resampling built an intermediate vector; sparse segments could be missed | Bounds rejection and streaming cuts; selective cloning; continuous swept intersection with linearly varying brush radius, including tapered-stroke regressions. |
| Spatial queries | Objects spanning many cells repeated bounds checks and hashing | Deduplicate candidates before bounds checks, reuse caller result sets, and switch to a direct scan for heavily overlapping queries. |
| Long documents | Page hit/nearest lookup was linear, and rendering visited all pages twice | Binary page lookup; build one stack per paint and restrict page painting to the visible candidate range. |
| Text rendering | Style/transform-only changes rebuilt layout; field rendering repeatedly shaped text | Cache layouts by geometry, text and font inputs, share inline text via `Arc`, and apply color/underline/transform during paint. |
| Live translucent ink | Unchanged repaints rebuilt the complete compound stroke path | Cache by builder identity, point count and final pressure sample. Keep the compound fill to preserve translucent overlap appearance. |
| UI bookkeeping | Every color lookup allocated a complete palette; hover keys allocated repeatedly; unchanged accessibility trees were dispatched | Static defaults and direct token lookup, borrowed hover keys, and equality-based accessibility update suppression. Window focus/bounds updates remain active. |
| Folder hierarchy | Building each child list rescanned the entire library | Sort once and use an adjacency map; cover deep hierarchies and malformed/orphan/self-parent nodes. |
| OCR preparation | Multiple bounds scans, a reference-vector allocation and an extra image-pixel copy | Validate and accumulate bounds in one pass; take the rendered pixel buffer directly. |
| Search/math | Repeated query normalization, linear annotation membership and repeated formula rendering | Reusable text matcher, membership sets when worthwhile, shared stroke comparisons, and per-report formula/vector reuse. |
| Export/PDF/recovery | Full intermediate text/JSON buffers, repeated SVG parsing, page clones and quadratic reconciliation | Stream text/XML/portable JSON, parse thumbnail SVG once, borrow export pages, restrict selected-page template work, and reconcile recovery order with indexed membership. |

The pen-input router already has constant-space, constant-work sample dispatch and was retained. Worker concurrency limits, durable WAL `synchronous=FULL` commits, raw ink, undo semantics, SVG/PDF output, and review-before-OCR-conversion behavior remain in place.

## Measurements

The release controller and geometry benchmarks use Rust 1.98.1, optimized builds with thin LTO and one codegen unit. Storage/UI microbenchmarks use the test profile. Results are CPU measurements on Fedora 44, AMD Ryzen 9 7940HS (8 cores/16 threads) on this machine, not application frame rate, GPU performance, OCR model speed, or physical tablet latency. Very small timings and different runs vary with scheduling and allocator/cache state.

| CPU workload | Baseline | After | Approximate improvement |
| --- | ---: | ---: | ---: |
| Save with 10,000 unrelated pages (mean) | 16.335 ms | 0.138 ms | 118× |
| Save with no unrelated pages (mean) | 0.230 ms | 0.137 ms | 1.7× |
| Restyle 2,000-point fragment over 10,000-point trace | 337.66 ms | 1.33 ms | 253× |
| Dense multicell spatial query | 1.482 ms | 0.125 ms | 12× |
| Eraser miss on 10,000-point path | 220.81 µs | 12.89 µs | 17× |
| Nearest page among 10,000 pages, prebuilt stack | 23.41 µs | 0.12 µs | 195× |
| 1,000 cached thumbnail requests, 2,000 objects | 246.43 ms | 0.209 ms | 1,179× |
| 100 unchanged translucent repaints, 12,000 points | 6,860.49 ms | 271.42 ms | 25× |

Controller timings below are the median of three runs, each measuring 200 operations.

| Controller fixture / operation | Baseline p50 | After p50 | Baseline p95 | After p95 |
| --- | ---: | ---: | ---: | ---: |
| 10,000 objects: Loaded cover cache hit | 152.337 µs | 0.040 µs | 206.258 µs | 0.050 µs |
| 10,000 objects: Move one selected object, autosave off | 1,061.760 µs | 1.213 µs | 1,943.280 µs | 1.443 µs |
| 10,000 objects: Insert short text, autosave off | 948.105 µs | 1.382 µs | 1,408.423 µs | 1.734 µs |
| 100,000 objects: Loaded cover cache hit | 6,324.071 µs | 0.040 µs | 7,703.389 µs | 0.041 µs |
| 100,000 objects: Move one selected object, autosave off | 41,246.757 µs | 1.333 µs | 43,748.680 µs | 1.983 µs |
| 100,000 objects: Insert short text, autosave off | 40,694.126 µs | 2.064 µs | 43,721.238 µs | 2.675 µs |

The controller fixture retains its initial bulk insertion as a large undo-history entry and disables autosave, specifically stressing deferred history and page-size-dependent bookkeeping. It is not a typical per-stroke history profile or an autosave-enabled input benchmark. Cover timings measure warmed snapshot reuse and exclude thumbnail rasterization; sub-microsecond results are sensitive to timer granularity, so no speedup ratio is assigned.

The broader existing 10,000-stroke benchmark showed largely unchanged ordinary paths in the matched run: active ink push/chunk p50 12.493 → 12.464 µs; ordinary spatial query 12.013 → 12.413 µs; incremental command/index 0.932 → 0.962 µs; initial index 7.810 → 8.010 ms; initial save 1,007.410 → 982.610 ms; load 275.206 → 283.480 ms. Full database-open p50 was 6.322 → 6.202 ms and background-reader open 30.758 → 30.858 µs. These single-run differences are small and noisy; no improvement is claimed for them. The large spatial-query gain above applies to dense multicell geometry, rather than every query.

Storage fixture: 100 warmed saves of a one-page document, unchanged searchable text, WAL/FULL commits, with either zero or 10,000 unrelated pages. The baseline uses the same fixture against the starting source. UI fixtures compare the prior eager/rebuild path with the new cached path inside the same test binary; they exclude raster/GPU presentation. Geometry benchmarks warm up three times and run each kernel for at least 250 ms; the expensive baseline fragment case completed only one timed iteration.

Raw results and validation logs are under `artifacts/validation/performance-pass/` (ignored local artifacts). Controller runs retain each repetition and the median summary. Benchmark source is included in the workspace so measurements can be repeated.

## Validation

- **288 Rust tests passed**, zero failures; four ignored tests comprise the real-model OCR integration and three manual performance tests. All three manual performance tests were run separately and passed.
- Strict workspace/all-targets **Clippy** with `-D warnings`, workspace **formatting**, and `git diff --check` passed.
- Python math solver (21 tests), recognition preprocessing (12), CPU encoder (3 passed, CUDA test skipped), and math-worker protocol (1) passed.
- Headless integration validated vector exports, Unicode text, FTS navigation, two-page PDF import and durable reload.
- Native pen/save smoke, accessibility editing/navigation/appearance/shortcuts, and notebook workflow checks passed on the final debug build. Light/dark editor captures were visually inspected.
- Final **release native smoke and complete six-case layout suite passed**, including 0.8/1.0/1.6 interface scales, compact/normal windows, fresh onboarding, library scrolling, actual mouse/keyboard favorites, settings and editing workflows. Evidence: `release-native-verification.json` and `release-native-layout/results.json`.
- Final optimized desktop and benchmark example builds passed (`cargo build --locked --release`).

New regression coverage includes durable deferred-history catch-up, sparse selection ordering, direct page switches, cover invalidation after same-revision replacement/edit/undo/delete, search migration and rollback, stable FTS locators across `VACUUM`, rename/trash/restore/delete isolation, Unicode matching, streaming export equivalence, eraser oracle comparisons and taper/sparse-hit cases, nearest-neighbor ties, adaptive spatial queries, rotated/zoomed visible-page ranges, text layout keys, stationary-pressure cache invalidation, and repeated formula reuse.

Native verification uses a private Xvfb display, software Vulkan, private D-Bus sessions and copied throwaway fixtures. It does not alter the user's notes, desktop, tablet configuration or accessibility settings. Legacy verification scripts were updated to the current native control roles/labels and to scroll pointer targets into visible content; assertions still verify the behavior and disabled state of controls.

## Compatibility

Every writer open now upgrades the database to schema **6**. The stable locator deliberately does not depend on `pages.rowid`, which can change during `VACUUM`. Versions 3–5 preserve valid existing FTS row IDs; versions below 3 first rebuild legacy derived OCR search content. Migration failures roll back the locator/version change. Document format remains 4; original documents, objects and durable history are retained. Earlier Folio builds reject schema 6. Tests and benchmarks upgrade only temporary databases; the user's actual database has not been opened by this pass.

## Remaining costs and limits

- With autosave enabled, building a persistence delta still copies the affected page's order/header and scans that page for searchable text. This is page-size dependent even though unrelated-library FTS work is removed. Incremental cached page text would need careful invalidation for undo, page replacements and annotations.
- New translucent sensor samples still rebuild the compound path. The cache improves unchanged repaints; splitting overlapping translucent fragments would change opacity without a different rendering strategy.
- A page-stack build still visits all document pages once per paint. Visible-page painting and hit lookup improve, but a persistent stack would require complete size/order/layout invalidation.
- The existing 16 ms UI polling cadence remains. Reducing it safely needs event-driven worker/input/motion/accessibility wakeups and physical-device responsiveness measurements.
- Integrity checking and first-time migration/opening remain proportional to database size. The pass preserves integrity checks rather than weakening them for startup numbers.
- PDF template source copying, raster image uploads, initial layout/thumbnail generation, final export buffers, and actual OCR/model inference remain material costs. PDF forms/links and portable export semantics take priority over speculative changes.
- Nearest-neighbor indexing has an extra construction cost and can degrade on pathological repeated coordinates; small fragments keep the linear implementation.
- No physical tablet, real screen-reader session, Windows runtime, hardware GPU/compositor, or CUDA inference was measured. The real-model OCR integration test remains opt-in. Small/noisy geometry changes are not claimed as meaningful wins.

## Reproduction

Run from the repository root, with the normal native build prerequisites and the existing recognition runtime for the Python checks:

```bash
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release -p folio --bin folio
cargo run --locked --release -p folio-app --example benchmark_controller -- 10000
cargo run --locked --release -p folio-app --example benchmark_controller -- 100000
cargo run --locked --release -p folio-canvas --example performance
cargo test --locked -p folio-storage incremental_save_scaling -- --ignored --nocapture
cargo test --locked -p folio-ui benchmark_ -- --ignored --nocapture
cargo run --locked -p folio-app --example validate -- /tmp/folio-performance-validation
artifacts/recognition-v2/runtime/bin/python scripts/test_math_solver.py
artifacts/recognition-v2/runtime/bin/python scripts/test_recognition_preprocessing.py
artifacts/recognition-v2/runtime/bin/python scripts/test_recognition_encoder.py
python3 scripts/test_math_worker_protocol.py
```

The available local Rust 1.99 toolchain supplied formatting and strict Clippy; tests and release measurements used system Rust 1.98.1. Native scripts require an isolated display/D-Bus session; the local `run-native.py` artifact records the exact runner used. Compare the supplied benchmarks against the starting revision with identical compiler/profile, generated input and machine conditions; do not treat the microbenchmarks as end-to-end interaction latency.
