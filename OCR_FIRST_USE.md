# First-use GLM-OCR Q8 setup

Folio now sets up recognition when an OCR request has no usable pack. Handwriting, image crops, PDF regions and solver/live-math OCR share the same resident backend. A valid existing Python pack remains supported; an explicit `FOLIO_RECOGNITION_CONFIG` Python override reports missing files rather than being replaced.

The native backend installs GLM-OCR's language and vision weights in Q8_0, plus an unmodified llama.cpp Vulkan runtime. It prefers a discrete GPU, tries other eligible Vulkan GPUs, and falls back to CPU when GPU startup fails. A GPU inference connection/server failure gets one CPU retry. Output truncation and invalid input remain errors for review. Both image encoding and language inference run on the chosen GPU.

## Installation and privacy

- Downloads total **1,466,516,991 bytes**, about **1.47 GB**; installed storage is approximately **1.56 GB**, including the retained runtime archive.
- Native setup supports **x86_64 Linux**. NVIDIA and AMD were tested; Intel was not. Vulkan-capable drivers are required for GPU acceleration. No CUDA toolkit or Python OCR environment is downloaded.
- Assets come directly from Hugging Face and GitHub over HTTPS. Exact revision, file sizes and SHA-256 digests are embedded in `crates/app/src/recognition/native.rs`.
- Downloads use bounded range requests. Cancellation retains `.partial` files; repeating the OCR action resumes them. Network errors explain how to retry. Checksum failures discard the corrupt partial. Only verified files become active.
- A file lock serializes setup across Folio data directories sharing a pack. Extraction uses a separate staging directory and safe archive paths. The pack marker is published last. Every cold native load verifies the assets again; warm requests reuse the model.
- Images stay local. The inference endpoint binds to loopback with a random API key, bypasses HTTP proxies, disables the server UI and uses offline model loading. The Flatpak enables network access to fetch assets; later recognition needs no Internet.
- Download, verification, loading and recognition stages appear in the status bar, selection controls, math panel and accessibility status. Cancel/Esc invalidates pending results and stops the model process.
- Native ink rendering uses white-background, black, rounded strokes with 4× supersampling. Images composite transparency onto white before encoding PNG, including transparent areas created by rotation or cropping.
- Review, copying, replacement, LaTeX rendering, stale-source checks and durable undo behavior are retained. Recognition does not modify the source before replacement.
- Model attribution/card, runtime MIT license and runtime third-party notices are retained in the pack. Small notice files are included in application packages.

## Pinned assets

Model repository: `ggml-org/GLM-OCR-GGUF`, revision `65a42de1148dbed2297e922b5dbc7d9b70c36578`. These are the public upstream Q8 files. The earlier large benchmark used Folio's local conversion; its language embedding quantization differs slightly, as recorded in the benchmark report.

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| `GLM-OCR-Q8_0.gguf` | 950,433,408 | `45bc244a6446aff850521dc41f18bc8d7105ad5f0c2c8c28af04e7cc4f4d50b1` |
| `mmproj-GLM-OCR-Q8_0.gguf` | 484,403,648 | `9c4b58e33e316ed142eb5dcb41abec3844d3e6e5dc361ffb782c3fa9d175141f` |
| `llama-b11457-bin-ubuntu-vulkan-x64.tar.gz` | 31,679,935 | `cb528b7f75e466f5113685d8aca7a9966a5dac3192f2e12bd4f96d7505fbea39` |

## Validation

- Workspace: **170 tests passed**, including eight native setup/processing tests and the existing recognition/solver/undo tests. The opt-in real OCR test is excluded from the normal suite.
- Strict Clippy, formatting, whitespace and packaging-script syntax checks passed.
- Actual runtime download: all 31.68 MB fetched through the production range downloader, SHA-256 verified, then installed. The two model files were seeded from the existing verified benchmark cache to avoid downloading another 1.43 GB.
- Real text and math recognition passed on NVIDIA RTX 4070, AMD Radeon 780M, CPU, and CPU fallback with a nonexistent GPU ID. Repeated warm recognition produced the same output. These smoke checks establish execution, not a new accuracy benchmark.
- Controller fixtures began with no pack marker and verified automatic setup, text/math review, conversion to native text/LaTeX, undo, redo and reopening. Only disposable synthetic/public fixtures were used.
- Flatpak Platform 25.08: production downloader fetched and verified the runtime, automatic selection used `Vulkan1` (NVIDIA), and a subsequent math fixture completed with network unshared. Both language and vision used the same Vulkan device. Only disposable bound fixture directories were accessed.
- Distribution build: Debian bookworm release compiled successfully with maximum glibc requirement 2.35. The updated local Flatpak bundle is under `artifacts/ocr-q8-release/dist`; its model/runtime assets are downloaded at first use, not bundled.
- Native UI: clipboard copy, edited text replacement, math cancellation, equation rendering, undo/redo and event-dispatch smoke replay passed on a private X11 display.

Evidence is under `artifacts/validation/ocr-first-use` and `artifacts/validation/ocr-native-*.log`. Existing installed applications and personal notes were not used for these checks. See README for paths, device overrides and migration from a Python pack.
