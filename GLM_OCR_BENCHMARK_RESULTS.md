# GLM-OCR local benchmark — 4 October 2026

**Historical native-encoder timings:** the later [encoding optimization](GLM_OCR_ENCODING_OPTIMIZATION_RESULTS.md) reduces GPU text median to 214 ms and math to 272 ms on the same cohorts. The timings below use the original Conv3d projection.

GLM-OCR was tested on the same **100 IAM handwritten lines and 100 MathWriting handwritten equations** as the earlier eight-model benchmark, on both CPU and GPU: **400 scored predictions**. Every prediction, target, token count, processed image size and latency is retained. Downloads use the internet; inference blocks internet sockets and runs with Hugging Face offline mode.

**Recommendation:** keep ConvText for fast text previews and Qwen3-VL for math. GLM-OCR reduces CPU nonspace character error from 5.07% to 3.32%, but takes 14.5 times as long per line in this comparison. It is an optional final-text accuracy candidate. Its formula score is 30/100 after formatting normalization, versus 47/100 for Qwen, so the smaller math footprint comes with lower measured recognition quality.

## Setup and comparability

- Hardware: Ryzen 9 7940HS, approximately 30 GiB RAM; RTX 4070 Laptop GPU, 8 GiB VRAM.
- Checkpoint: `zai-org/GLM-OCR`, pinned revision `2e85a62840ccac27daa451df36c736c4636b8628`; verified weight SHA-256 `a16eb0de98d199293371c560f95f83130d2a2c9612449df16839f08ff9498815`.
- Transformers 5.18.0; GPU Torch 2.9.1+cu128; CPU Torch 2.9.1+cpu. CPU uses the application's CPU-only environment; the earlier CPU baselines ran in a CUDA-enabled Torch build. Both execute on CPU with eight threads, but this is a runtime-build difference.
- Batch one, BF16, SDPA attention, deterministic greedy decoding, KV cache enabled, 512 new-token cap. No quantization, speculative decoding/MTP, external language model or output repair. The official server's MTP throughput claims do not apply to this ordinary Transformers generation path.
- Official task prompts: `Text Recognition:` and `Formula Recognition:`. The checkpoint processor's original image settings are preserved; there is no hand-tuned input resizing or prompt search. GLM and the earlier models use their own processors, not identical internal pixel/token budgets.
- No layout model is required: each input is already a selected line or expression crop. This is base-model recognition, rather than the full PP-DocLayout-V3 document pipeline.
- Each model/task/device runs in an isolated process, sequentially. One cold call and two warm-up calls precede the 100 scored calls. Latency includes local image loading, preprocessing, generation and decoding; it excludes loading the model and rendering pen strokes. GPU calls synchronize before and after timing.

## Handwritten text

CER is corpus-level summed Levenshtein edits divided by summed reference characters. Raw CER retains IAM's tokenized punctuation spacing. Nonspace CER removes whitespace while preserving case and punctuation; it does not evaluate word boundaries.

| Model | Raw CER | Nonspace CER | CPU median / P95 | GPU median / P95 |
|---|---:|---:|---:|---:|
| ConvText | 7.43% | 5.07% | 0.163 s / 0.264 s | 0.010 s / 0.014 s |
| TrOCR Base, beam 5 | 4.77% | 4.93% | Not measured | 0.157 s / 0.249 s |
| GLM-OCR CPU | 5.55% | 3.32% | 2.362 s / 4.359 s | — |
| GLM-OCR GPU | 5.57% | 3.39% | — | 6.033 s / 9.610 s |

GLM CPU/GPU raw strings differ on **6/100** lines. CPU word/punctuation token error is 10.13%, versus 15.23% for ConvText. The character and word diagnostics are distinct metrics.

## Handwritten mathematics

Strict exact match removes whitespace only. The supplementary format score also strips whole-expression math delimiters and sizing/style commands, equates `dfrac`/`tfrac` with `frac`, and normalizes braces around a single alphanumeric subscript/superscript. It preserves operators, case, fractions, roots and grouping. Neither score is semantic equivalence or the papers' rendered-equivalence metric.

| Model | Strict matches | Format-normalized matches | CPU median / P95 | GPU median / P95 |
|---|---:|---:|---:|---:|
| Uni-MuMER Qwen3-VL-2B | 47/100 | 47/100 | 3.643 s / 6.721 s* | 2.822 s / 4.168 s |
| GLM-OCR CPU | 0/100 | 30/100 | 1.912 s / 3.088 s | — |
| GLM-OCR GPU | 0/100 | 30/100 | — | 3.165 s / 4.932 s |

GLM places display-math delimiters around all 100/100 GPU predictions. Raw strict scoring counts those delimiter differences as errors; the formatting-normalized score is the useful comparison here.

*The earlier Qwen CPU run has only the first 20 equations. On that same 20-example intersection, Qwen CPU median is **3.643 s** and GLM CPU median is **1.935 s**. Their format-normalized match counts are **8/20** and **5/20**, respectively. Comparing the unpaired full-cohort/20-example medians as speedups would be misleading.

On the full GPU cohort, strict scoring gives 47 equations correct only in Qwen and 0 only in GLM. Under format scoring, 20 are correct only in Qwen, 3 only in GLM, 27 in both, and 50 in neither. This paired comparison does not implement an answer chooser or ensemble.

GLM CPU/GPU raw strings differ on **2/100** equations. Token-cap hits: GPU text 0, CPU text 0, GPU math 0, CPU math 0. Capped outputs remain in the score.

## GPU phase diagnostics

| First cohort sample | Preprocessing | Vision encoder | Language model total | Whole call |
|---|---:|---:|---:|---:|
| text | 0.007 s | 8.869 s | 0.261 s | 9.173 s |
| math | 0.005 s | 2.905 s | 0.199 s | 3.138 s |

These separate one-sample probes synchronize module hooks and can add overhead; they are excluded from the primary latency table. Both reproduce the saved prediction exactly. The measured GPU cost is dominated by the vision encoder. This identifies where this backend spends time and does not establish performance for a different kernel, resolution, quantization or serving framework.


## Footprint

Verified BF16 weights: **2.651 GB / 2.469 GiB**. This is the checkpoint, not the Python runtime or a complete application installation.

| GLM worker | Peak host RSS | Peak CUDA allocated / reserved |
|---|---:|---:|
| CPU text | 2691 MiB | — |
| CPU math | 2523 MiB | — |
| GPU text | 3105 MiB | 2266 / 2350 MiB |
| GPU math | 3111 MiB | 2218 / 2318 MiB |

Host RSS includes Python, imports and checkpoint loading; it excludes the app UI. CUDA values cover the Torch allocator and exclude driver/context allocations. These are individual workers, not simultaneous residency. The CPU figures come from the CPU-only runtime and should not be compared directly with the older CUDA-enabled workers' host RSS. The app's previously measured Qwen CPU footprint is approximately 4.5 GiB and ConvText approximately 630 MiB loaded / 830 MiB loading peak, from a different small four-thread fixture run.

## Limits and reproduction

These convenience cohorts are not full IAM/MathWriting benchmark reproductions or a representative sample of your personal writing. Dataset exposure and training overlap are not fully audited. English lines and isolated formulas do not measure full-page layout, multilingual notes, live trajectories, late strokes, editing gestures or equation solving. Laptop clocks and thermals are unlocked. MyScript was not benchmarked.

To rerun, acquire the pinned files through `hf download`, then run `run_glm.py` from the existing benchmark environment. It verifies the checkpoint hash and skips complete outputs. Move a result aside to repeat its measurement. Run `analyze.py`, `paired_analysis.py` and `analyze_glm.py` after completion.

Evidence: [checkpoint/input manifest](artifacts/benchmarks/recognition-2026-10-04/glm-checkpoint-manifest.json), [GLM summary and paired comparisons](artifacts/benchmarks/recognition-2026-10-04/glm-summary.json), [raw predictions](artifacts/benchmarks/recognition-2026-10-04/results/), [logs](artifacts/benchmarks/recognition-2026-10-04/logs/), and [process outcomes and commands](artifacts/benchmarks/recognition-2026-10-04/jobs.jsonl). The app's recognition defaults were not changed.

Primary documentation: [GLM-OCR model card](https://huggingface.co/zai-org/GLM-OCR), [technical report](https://arxiv.org/html/2603.10910), [Transformers usage](https://huggingface.co/docs/transformers/model_doc/glm_ocr).
