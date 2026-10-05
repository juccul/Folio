# Offline interactive handwriting recognition for Folio

Research date: 2026-10-04. Historical research: handwriting OCR/text detection/math recognition were subsequently removed in Folio 0.1.8 at the user’s request. Recommendations below are future possibilities, not current application features. Scope: Linux, offline operation, redistributable FOSS dependencies, handwriting text and mathematics, and recognition that remains attached to editable vector ink.

## Recommendation

Build a persistent semantic ink and correction layer, with separate compact text and math recognizers behind replaceable worker interfaces. Keep the existing better-tested math recognizer as an explicit conversion fallback until a smaller model passes broader evaluation. No verified ready-to-ship FOSS package found in this investigation supplies MyScript Notes/Math quality, interactive editing, unrestricted redistribution and native Linux integration together.

For trajectory text recognition, evaluate the OnlineHTR architecture first, but do not bundle its downloadable checkpoint until its redistribution rights are established. For interactive math, study MathNote OCR's expression-tree/session API, but do not use its current checkpoint as Folio's default: it failed the local screen. Seshat provides an older Linux-native stroke-math baseline. GLM-OCR is a reasonable smaller image-based challenger to evaluate next, not a validated replacement.

The current roughly 4.5 GiB math-worker footprint is a consequence of the chosen 2.1-billion-parameter model and runtime, not a fundamental requirement of handwriting recognition. Idle worker termination and load-on-demand should precede shipping another large always-resident model. Neither idle unloading nor quantization was implemented during this research.

## What produces the MyScript experience

MyScript's public [JIIX format](https://developer.myscript.com/doc/interactive-ink/4.1/reference/jiix/) exposes semantic text/math structures, word and character alternatives, geometry, and references back to source ink. This supports a stronger design than storing one OCR string for an entire screenshot.

For Folio, retain stroke IDs through character/word/line/paragraph groups and symbol/expression nodes. Store recognition provenance, alternatives, document revision and user corrections. A new dot, crossbar or superscript should invalidate a local region; previously accepted corrections should survive. Selection, movement, insertion and reflow should use these relationships while preserving original strokes. Recognition remains asynchronous and independent of the pen/render path. These are proposed extensions, not a claim that Folio already implements the complete interaction model.

MyScript's recognition SDK itself is proprietary: open sample applications do not make the underlying engine FOSS. Its [published qualified platforms](https://developer.myscript.com/doc/interactive-ink/4.4/overview/platforms/) omit Linux; [C++ integration is available on request](https://developer.myscript.com/doc/interactive-ink/4.1/overview/integration-levels/). The [SDK licensing terms](https://github.com/MyScript/interactive-ink-licenses-android) therefore exclude it under Folio's current constraints. This is not a claim that a commercial Linux agreement is impossible.

## Candidates and limitations

| Candidate | Actual input | Licensing findings | Recommendation |
| --- | --- | --- | --- |
| OnlineHTR | Text pen trajectories | MIT code; separate checkpoint redistribution rights not verified | First trajectory-text prototype to investigate |
| MathNote OCR | Stroke groups; raster symbol classification; spatial expression parsing | Apache-2.0 repository with bundled checkpoints | Interactive architecture reference; current accuracy insufficient |
| Seshat | Math trajectories, InkML/SCGINK | GPL-3.0 repository | Secondary native C++ baseline; older research implementation |
| Texo / FormulaNet | Equation images | AGPL-3.0 code and model card | Compact ONNX candidate; evaluated checkpoint failed screening |
| GLM-OCR | Images: text, formulas, tables | Apache-2.0 code; MIT model weights | Next smaller raster challenger; untested here |
| MathWriting CTC research model | Math trajectories | Apache example code; CC-BY-NC-SA-4.0 dataset; no published trained CTC checkpoint found | Long-term architecture evidence, not an installable model |

### Text trajectories: OnlineHTR

[OnlineHTR](https://github.com/PellelNitram/OnlineHTR) implements an LSTM/CTC approach inspired by Google's online handwriting paper. It derives coordinate/time deltas and stroke-boundary features, rather than recognizing a page image. The supplied model is an English IAM-OnDB research checkpoint; support for other languages would require suitable data and models. Language-model beam decoding is listed as unfinished work. It is a research implementation, not a complete maintained Linux handwriting SDK.

The [code license is MIT](https://raw.githubusercontent.com/PellelNitram/OnlineHTR/main/LICENSE.md). A freely downloadable checkpoint is not sufficient evidence of an explicit weight redistribution grant. IAM's [official download terms](https://fki.tic.heia-fr.ch/databases/download-the-iam-on-line-handwriting-database) also specify non-commercial research use. Dataset terms and checkpoint licenses are separate questions; this report does not assert that a dataset restriction automatically determines the resulting weights' license. No text trajectory inference was measured during this investigation.

### Interactive math: MathNote OCR

[MathNote OCR](https://github.com/YonatanNemtsov/mathnote-ocr) has useful incremental add/remove sessions, source stroke references, candidate alternatives and pinned corrected subtrees. Its symbol CNN and small spatial parsing models are approximately 1.2 million parameters combined. It consumes strokes but rasterizes candidate symbols for classification; it is not a purely temporal trajectory model. Matrix/case recognition requires marking a grid region. Python/PyTorch CPU execution works on Linux.

The author's [vocabulary and training notes](https://raw.githubusercontent.com/YonatanNemtsov/mathnote-ocr/main/docs/vocabulary.md) identify the main limitation: the current 125-class symbol model was trained on approximately 7,800 samples from one writer and struggles with other writers. A proposed 164-class multi-writer study does not establish that improved weights already exist. Retraining and multi-writer validation are necessary before considering this as a default recognizer.

### Other math choices

[Seshat](https://github.com/falvaro/seshat) accepts actual pen trajectories and outputs mathematical structure/LaTeX. Its C++ grammar-based parser and symbol models were tested on Linux, but its competition evidence and tooling are old. It was not built or measured here, so no performance or accuracy comparison is claimed.

[Texo](https://github.com/alephpi/Texo) offers a roughly 20-million-parameter image formula recognizer and ONNX deployment. Its [FormulaNet weights](https://huggingface.co/alephpi/FormulaNet) declare AGPL-3.0; this allows redistribution with the applicable source/license obligations. The author's [paper](https://arxiv.org/html/2602.17189v1) reports handwriting CDM results, a rendered-formula similarity metric, not a percentage of wholly correct equations. The specific transfer ONNX checkpoint screened below did poorly on these samples; that does not establish the performance of every Texo checkpoint or decoding configuration.

[GLM-OCR](https://huggingface.co/zai-org/GLM-OCR) is a 0.9-billion-parameter image OCR model with MIT weights and [Apache-2.0 code](https://github.com/zai-org/GLM-OCR). It covers text/formulas/tables but does not consume trajectories or maintain editing semantics. Its deployment stack differs from Folio's pinned runtime. CPU latency, memory and handwriting accuracy must be measured before adoption; advertised document OCR scores do not prove handwriting quality.

Google's [MathWriting paper](https://openreview.net/pdf?id=bxwWikAXSy) reports a 35-million-parameter trajectory CTC Transformer with 5.49% LaTeX-token character error on its test split. This demonstrates that useful recognition need not require billions of parameters; it is not an accuracy guarantee for Folio. The [released project](https://github.com/google-research/google-research/tree/master/mathwriting) supplies data and examples, not a trained downloadable CTC recognizer. Its non-commercial dataset license requires separate review and excludes treating the dataset as unrestricted FOSS training material.

Public research repositories are not automatically redistributable: TAP and the original CoMER repository did not show an explicit root license in this review. A licensed wrapper/fork does not by itself settle upstream model rights. Google ML Kit's offline digital ink feature is useful evidence of the interaction approach, but its [Android/iOS SDK](https://developers.google.com/ml-kit/vision/digital-ink-recognition) is not an open Linux recognition engine.

## Local exploratory screening

Two isolated CPU probes reused Folio's existing Python runtime without installing packages or changing application code/settings. Input was the existing set of 12 MathWriting research expressions, not the user's handwriting. Files remain outside release archives. Results must not be presented as a representative handwriting benchmark.

| Run | Correct whole expressions / 12 | Recognizer process memory | Time |
| --- | --- | --- | --- |
| MathNote default, raw strokes | 0 | 345.3 MiB peak RSS | 0.173 s median per expression |
| Texo transfer ONNX, original images | 2 | 243.3 MiB peak RSS | 0.095 s median per expression |
| Texo transfer ONNX, thinner images | 1 | 243.6 MiB peak RSS | 0.096 s median per expression |
| Existing Uni-MuMER, previous validation | 9 | Approximately 4.5 GiB resident RSS | 85 s for the 12-expression batch |

The two Texo image sets represent the same equations and writers, not 24 independent samples. Normalization ignores whitespace, single-token braces and selected LaTeX formatting aliases; it retains case, symbols and expression structure. MathNote used its default configuration; Texo used greedy FP32 ONNX decoding with cached keys/values and a Pillow implementation of the author's evaluation preprocessing. These differ from the existing Uni-MuMER pipeline. Measurements exclude the Folio UI, use two CPU threads and distinguish peak RSS from the previous resident-memory snapshot. Per-expression times exclude imports/model loading, so this is not a controlled cold-start comparison. Low latency alone did not deliver usable accuracy.

Exact sources and artifacts:

- MathNote revision `bef25ced6f6f9313ac82b25a984c3c8a580bcdd3`; selected source, license and bundled weights in `artifacts/research/myscript-alternatives/mathnote-ocr/`, with `research-source.json`.
- Texo FormulaNet revision `b2668efe5112082846fde4d446b9bfaab3989533`; metadata, source notices and ONNX files in `artifacts/research/myscript-alternatives/texo/`, with `provenance.json`. The three ONNX graphs were checked against their upstream LFS SHA-256 values.
- Probe programs: `probe_mathnote.py`, `probe_texo.py`; individual results: `mathnote-probe.json`, `texo-probe.json`, `texo-original-probe.json`; reproducible normalization/results: `summarize.py`, `summary.json`, all in the research directory.
- Prior comparator: `artifacts/validation/navigation-recognition/math-metrics.json` and `unimumer-math.json`. No comparator inference was rerun during this research.

## Implementation priorities

1. Make large recognizers load on demand and exit after configurable idle time. Persist results and keep pen input independent. Benchmark any quantized replacement against the existing precision before changing defaults.
2. Extend the current semantic groups with durable symbol/word alignment, recognition alternatives and correction constraints. Make edits invalidate only affected groups and retain accepted corrections through recomputation.
3. Validate a trajectory-text prototype and resolve weight licensing. Prototype a structured math session without adopting the failing MathNote checkpoint as a default. Measure GLM-OCR as a smaller image fallback challenger.
4. Establish a multi-writer, multi-language evaluation corpus with clear use rights: text CER/WER, whole-equation correctness and rendered CDM, late dot/crossbar/script edits, calibrated rejection/correction behavior, CPU p50/p95 latency and warm/idle memory. Start with a larger screening set, then evaluate separately held-out writers before shipping.
5. If compact public checkpoints remain inadequate, train or fine-tune compact trajectory/symbol models on appropriately licensed multi-writer data. This is real model/data work, not a dependency swap. User documents remain local; collection or uploads would require an explicit separate instruction.

This research added documentation and isolated probe artifacts only. Folio's default models, installed runtime, pen handling, Bluetooth configuration and running desktop were not changed.
