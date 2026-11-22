# Folio 0.2.3

- **Built-in offline handwriting and math recognition**: replaces the 1.47 GB GLM-OCR / `llama-server` download with bundled pure-Rust stroke-sequence models (`folio-recognition`, ~38 MiB total) that run directly on CPU with zero downloads, zero external inference servers, and zero Vulkan/GPU requirements.
  - **Math v2** (18.58M-parameter RoPE Conformer Encoder + KV-cached Decoder): trained on Google MathWriting with 10 kinematic/spatial stroke features and stroke-order shuffling augmentation (`28.3 ms` median on CPU; `66.5%` exact match on full held-out MathWriting test; `62.5%` EM / `6.83%` CER vs GLM-OCR `46.5%` EM / `15.3%` CER on 200-sample head-to-head).
  - **Handwriting v2** (16.63M-parameter 8-layer RoPE Conformer-CTC + Bigram Lexicon Beam Search): two-pass scale-invariant multi-line vector clustering and delayed-stroke reordering (`i`-dots and `t`-crosses) directly from raw ink strokes (`56.8 ms` median on CPU; `2.25%` CER / `85.5%` exact line match vs GLM-OCR `16.0%` CER / `49.7%` EM on 200 unseen-writer samples; `0.82%` raw greedy CER on unseen BRUSH stylus writers).
  - Removes the first-use 1.47 GB OCR download dialog, `llama-server` process management, and raster image / PDF crop OCR paths.
- **Smoother writing and previews**: pixel-lattice-snapped ink batch rasters, prefix-layer stand-in caching while appending strokes, and stand-in page thumbnails prevent ink shifting, ink flickering, and page-preview flashing while writing.
- **Memory, storage and rendering optimizations**: compact bit-exact binary serialization for ink strokes, lower RAM usage across thumbnails, fonts, image/atlas caches, and Wayland/GPUI frame buffers, plus zero-Hz idle sleeping when no animations or background work are active.
- **Reliability and bug fixes**: fixes across document storage, history/undo, PDF/image handling, selection, math solver workflows, and UI stability.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak and desktop archive, checksums and complete corresponding source are published together with the bundled `recognition/` models. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.
