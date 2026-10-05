# Local handwriting recognition benchmark — 4 October 2026

The subsequent [GLM-OCR benchmark](GLM_OCR_BENCHMARK_RESULTS.md) tests a ninth model on these same text and math cohorts. The eight-model measurements and recommendations below describe the original run; the addendum records the newer comparison.

I ran **eight released models**, with **18 primary model/device/decoder configurations**, on this Linux laptop. The primary runs contain **1,640 scored predictions** over the same 100 English handwritten lines and 100 handwritten equations. A further **112 scored predictions** test printed controls and stroke width. All recognition inference runs locally with internet connections blocked in the worker.

**Recommendation:** start with HTR-ConvText for a responsive text preview. TrOCR Base with beam-five decoding is the text accuracy candidate on GPU. For math, Qwen3-VL-2B is the more practical of the two large candidates in this environment: both matched 47/100 equations, and Qwen3-VL was slightly faster. The compact stock math models are not accurate enough on this stroke-rendered cohort to recommend for automatic conversion.

Hardware: **AMD Ryzen 9 7940HS, 8 cores / 16 threads, about 30 GiB RAM; NVIDIA RTX 4070 Laptop GPU, 8 GiB VRAM**. The workers use eight CPU threads and batch size one. Compact Torch models use FP32; the two large math models use BF16. Measured Torch version: 2.9.1+cu128. The available Paddle CPU runtime is 3.2.2.

The text set is the first 100 public `Teklia/IAM-line` test rows. The math set is all 100 human test examples in the **MathWriting 2024 excerpt**, not the full MathWriting test set. Math traces are rendered as black 1.75 px strokes on white with fourfold antialiasing. The recognizers receive the finished raster, so timing and stroke order are not model inputs. Text input is the original line image, rather than an online pen stream. These are fixed convenience cohorts, not representative samples of your own handwriting or full published benchmark reproductions.

All latency figures below are warmed, single-image **image-to-output** measurements, including image reading, preprocessing, inference and decoding. They exclude model loading and stroke rasterization. GPU timing synchronizes before and after the call. Each configuration runs in a separate process; measured contenders do not run inference simultaneously. P95 describes the set of different expression/line lengths, rather than repeated timing on one example.

For text, **raw CER** retains IAM's tokenized punctuation spacing; **nonspace CER** ignores whitespace but preserves case, punctuation and all other characters. The latter helps compare the differing output spacing conventions, but does not grade word boundaries. Both are corpus-level character error rates, calculated as total edits divided by total reference characters. These supplementary nonspace results must not be compared directly with published IAM CER.

| Text model / decoder | Raw CER | Nonspace CER | CPU median / P95 | GPU median / P95 |
|---|---:|---:|---:|---:|
| HTR-ConvText IAM / CTC greedy | 7.43% | 5.07% | 163 ms / 264 ms | 10 ms / 14 ms |
| PyLaia IAM / CTC greedy | 7.90% | 8.88% | 33 ms / 48 ms | 16 ms / 24 ms |
| TrOCR Small / beam 1 | 6.45% | 6.75% | 276 ms / 440 ms | 45 ms / 76 ms |
| TrOCR Small / beam 5 | 6.18% | 6.40% | 290 ms / 431 ms | 56 ms / 97 ms |
| TrOCR Base / beam 1 | 5.08% | 5.26% | 1.37 s / 2.01 s | 95 ms / 155 ms |
| TrOCR Base / beam 5 | 4.77% | 4.93% | Not measured | 157 ms / 249 ms |

All rows above have 100 scored lines. TrOCR Base beam-five was tested on GPU only. The CPU/GPU greedy predictions agree exactly for ConvText, PyLaia and TrOCR Base. TrOCR Small differs on one line, with GPU nonspace CER 6.77% versus CPU 6.75%. Beam-five Small agrees across devices on the aggregate score.

ConvText has the best observed character-error/CPU-latency tradeoff, but **word-level quality is a separate consideration**. A cased diagnostic that tokenizes words and punctuation separately gives error rates of 15.23% for ConvText, 13.80% for TrOCR Small beam-five, and 11.65% for TrOCR Base beam-five. TrOCR can therefore be useful as a final natural-language pass, despite its slower inference. The 4.93% versus 5.07% nonspace CER gap between Base beam-five and ConvText is small and does not establish a broad accuracy advantage across writers.

For math, **strict match** removes whitespace only. Supplementary format normalization strips delimiter sizing/style, whole-expression math delimiters, and braces around a single alphanumeric subscript or superscript, and equates `dfrac`/`tfrac` with `frac`. It retains case, operators, fraction order, roots and grouping. The match count did not change under that limited normalization on these primary runs. This is string agreement, not semantic equivalence or the papers' rendered-equivalence CDM metric.

| Math model | Strict matches, full 100 | CPU median / P95 | GPU median / P95 |
|---|---:|---:|---:|
| PP-FormulaNet Plus-S | 3/100 | 408 ms / 599 ms | Not measured |
| UniMERNet Tiny | 6/100 | 976 ms / 2.06 s | 146 ms / 329 ms |
| Uni-MuMER Qwen3-VL-2B | 47/100 | 3.64 s* / 6.72 s* | 2.82 s / 4.17 s |
| Uni-MuMER Qwen3.5-2B | 47/100 | 3.89 s* / 6.99 s* | 3.02 s / 4.47 s |

\* The large-model CPU probes use the **same first 20 equations**, whereas their GPU accuracy and latency rows use all 100. Do not derive CPU/GPU speedups from these unpaired table medians. On the matched 20 examples, CPU and GPU have identical match counts: 8/20 for Qwen3-VL and 6/20 for Qwen3.5. The CPU/GPU Qwen3.5 predictions differ on one incorrect example. The compact model CPU and GPU predictions agree on all 100 equations.

The two large models each get 47 equations right, with five examples correct only in each model. An **oracle chooser** with access to the answers would reach 52/100 across their outputs; that is an upper bound, not a tested deployable ensemble. Using two models together does not solve the remaining accuracy gap.

The single-sample Qwen3-VL GPU phase probe measured 2.401 seconds in generation and 0.003 seconds in preprocessing. The slow call is therefore not explained by the raster/image preprocessing adapter. This probe is diagnostic, rather than a general decomposition for all expression lengths.

The following checks investigate the compact math failure without replacing the main cohort. Printed controls are eight fixed synthetic formulas; the stroke-width comparison uses the same first 20 test traces, with identical geometry and labels.

| Math model | Printed controls | Thin handwriting, same first 20 | 3.5 px handwriting, same first 20 |
|---|---:|---:|---:|
| PP-FormulaNet Plus-S | 6/8 | 1/20 | 1/20 |
| UniMERNet Tiny | 4/8 | 1/20 | 1/20 |
| Uni-MuMER Qwen3-VL-2B | 7/8 | 8/20 | 8/20 |
| Uni-MuMER Qwen3.5-2B | 6/8 | 6/20 | 7/20 |

The checkpoints produce correct simple printed formulas, while doubling stroke width did not rescue the compact handwritten results. The controls are small and do not measure printed-document accuracy or prove that a model cannot improve with other preprocessing or fine-tuning.

I also checked the minimal formula transform against upstream Paddle preprocessing on every one of the 100 equation images at both 384×384 and 192×672. The largest normalized pixel difference was **4.77×10⁻⁷**. Applying Paddle's additional text cleanup to the saved predictions changes no strict or format-normalized match decisions. ConvText, PyLaia and UniMERNet inference parameters are loaded strictly; compatibility adaptations are recorded in the benchmark README.

Separate stroke rasterization costs **24.2 ms median / 40.7 ms P95** over 100 examples. That measurement includes fourfold drawing, Lanczos downsampling and PNG encoding to an in-memory buffer, and excludes parsing INKML. A direct in-memory image pipeline can avoid the PNG encoding/decoding work; it has not been benchmarked here.

Memory below is **peak worker RSS**, including imports and checkpoint loading. CUDA columns are Torch allocator peaks; they omit driver/context allocations outside that allocator. These are isolated workers, not simultaneous residency measurements for a complete application.

| Model / selected decoder | CPU peak RSS | GPU peak RSS | CUDA allocated / reserved peak |
|---|---:|---:|---:|
| HTR-ConvText IAM | 1428 MiB | 1472 MiB | 347 / 370 MiB |
| PyLaia IAM | 939 MiB | 1283 MiB | 110 / 216 MiB |
| TrOCR Small / beam 5 | 1086 MiB | 1731 MiB | 294 / 314 MiB |
| TrOCR Base / beam 1 | 2119 MiB | 2200 MiB | 1357 / 1426 MiB |
| PP-FormulaNet Plus-S | 1274 MiB | — | — |
| UniMERNet Tiny | 1671 MiB | 1693 MiB | 459 / 506 MiB |
| Uni-MuMER Qwen3-VL-2B | 5011 MiB | 5089 MiB | 4131 / 4172 MiB |
| Uni-MuMER Qwen3.5-2B | 5261 MiB | 5371 MiB | 4288 / 4406 MiB |

All measured GPU workers fit the laptop's 8 GiB GPU individually. PyLaia has about 5.33 million runtime parameters, ConvText 65.92 million, TrOCR Small 61.60 million, TrOCR Base 333.92 million, UniMERNet Tiny 107.40 million, Qwen3-VL 2.128 billion and Qwen3.5 2.213 billion. Model-file size and worker RSS are different quantities.

Model loading and cold first inference are recorded separately in the JSON summaries. Loading is timed after core Torch imports, so it is not total process startup time. A persistent worker is the appropriate application design; repeatedly spawning and loading a model would add seconds before each recognition result. The released ConvText checkpoint contains both normal and EMA training weights; a deployment should retain only the validated EMA inference state.

Several checkpoint configs disable decoder caching as a training setting. The final TrOCR and Uni-MuMER benchmarks explicitly enable KV caching. For TrOCR Base CPU greedy, this changed median time from 2.232 seconds in the retained uncached diagnostic to 1.372 seconds in the final run, with unchanged scored predictions. Qwen3.5 GPU uses FLA 0.5.2 gated-delta kernels, with the upstream reference causal convolution because the optional compiled causal-conv1d package is unavailable. CPU uses the upstream Torch recurrence. Paddle uses CPU inference with oneDNN disabled. These timings do not establish the fastest achievable ONNX, quantized, TensorRT, OpenVINO or other deployment backend.

The practical next implementation choice is **ConvText for quick text previews**, retaining TrOCR Base beam-five as a higher-cost final text candidate. **Qwen3-VL-2B is the current math experiment candidate**, with original ink retained and results presented as editable suggestions. A real handwriting evaluation set from the intended user, followed by adaptation and confidence calibration, is needed before promising high math accuracy. The raw word/punctuation diagnostic means TrOCR Small also deserves a final-text A/B when prose quality matters more than the lowest character error.

This is a recognition-component benchmark. It does not test text/math routing, full-page layout, online text trajectories, late strokes, editing gestures, stroke-to-symbol alignment, app frame times, multilingual notes, equation solving or a CAS. **MyScript itself was not benchmarked**, so these measurements do not establish a numerical quality gap against MyScript. Public test exposure and exact training/test split overlap have not been fully audited. Adjacent IAM lines may come from the same writer. The 100-example sets and unlocked laptop clocks/thermals limit the generality of small ranking differences.

The candidates were selected under the open-source/offline constraint documented in the earlier model research. The NVIDIA CUDA stack used for GPU measurements includes separately licensed proprietary components. If the constraint covers every runtime/driver component, an open-driver GPU backend or an appropriately built CPU runtime needs separate validation. CPU timings here are CPU execution in the measured CUDA-enabled Torch environment. MathWriting evaluation data is CC-BY-NC-SA-4.0; these test assets are separate from application code/model licensing and are not an unrestricted product training corpus.

The complete reproducible evidence is in [the benchmark directory](artifacts/benchmarks/recognition-2026-10-04/README.md): [JSON summary](artifacts/benchmarks/recognition-2026-10-04/summary.json), [CSV summary](artifacts/benchmarks/recognition-2026-10-04/summary.csv), [per-sample predictions](artifacts/benchmarks/recognition-2026-10-04/results/), [paired comparisons](artifacts/benchmarks/recognition-2026-10-04/paired-comparisons.json), [preprocessing checks](artifacts/benchmarks/recognition-2026-10-04/preprocessing-verification.json), [dataset/input hashes](artifacts/benchmarks/recognition-2026-10-04/dataset-manifest.json), [checkpoint hashes](artifacts/benchmarks/recognition-2026-10-04/checkpoint-inventory.json), [environment](artifacts/benchmarks/recognition-2026-10-04/environment.json), and [commands/process outcomes](artifacts/benchmarks/recognition-2026-10-04/jobs.jsonl). The application recognition code was not changed by this benchmark work.

Primary checkpoint/source references: [HTR-ConvText](https://huggingface.co/DAIR-Group/HTR-ConvText), [PyLaia IAM](https://huggingface.co/Teklia/pylaia-iam), [TrOCR Small](https://huggingface.co/microsoft/trocr-small-handwritten), [TrOCR Base](https://huggingface.co/microsoft/trocr-base-handwritten), [PP-FormulaNet Plus-S](https://huggingface.co/PaddlePaddle/PP-FormulaNet_plus-S), [UniMERNet Tiny](https://huggingface.co/wanderkid/unimernet_tiny), [Uni-MuMER Qwen3-VL-2B](https://huggingface.co/phxember/Uni-MuMER-Qwen3-VL-2B), [Uni-MuMER Qwen3.5-2B](https://huggingface.co/phxember/Uni-MuMER-Qwen3.5-2B).
