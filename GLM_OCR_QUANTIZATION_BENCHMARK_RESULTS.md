# GLM-OCR INT8 and 4-bit benchmark — 4 October 2026

**Historical native-encoder timings:** these GPU runs use the original Conv3d projection. The later [encoding optimization](GLM_OCR_ENCODING_OPTIMIZATION_RESULTS.md) measures BF16 GPU text at 214 ms and math at 272 ms; quantized formats require separate optimized-encoder measurements.

I ran **800 new scored predictions**: INT8 and 4-bit NF4, text and math, CPU and GPU, with all 100 examples per configuration. The BF16 comparison reuses the earlier 400 scored predictions on identical ordered cohorts and inputs. No application defaults were changed.

## Protocol

- Same Ryzen 9 7940HS and RTX 4070 Laptop 8 GiB, eight CPU threads, batch one. Transformers 5.18.0; CPU Torch 2.9.1+cpu; GPU Torch 2.9.1+cu128. The extra quantization dependencies live in a separate benchmark overlay, not the app runtime.
- Same pinned GLM checkpoint and input images, official text/formula prompts, original checkpoint processor settings, SDPA, greedy decoding, KV cache, 512-token cap. All inference blocks internet sockets and uses local-only model loading. Input cropping and stroke rasterization are unchanged from the earlier component benchmark.
- Each model/task/device runs in an isolated process, sequentially. One cold call and two warm-up calls precede 100 scored calls. GPU calls synchronize before/after timing. Latency includes local image reading, preprocessing, generation and decoding, excluding model loading, quantization, serialization and pen rendering. Serialization time is recorded separately and subtracted from loading time.
- **INT8:** bitsandbytes 0.50.2 LLM.int8(), threshold 6.0. INT8 linear weights and mixed computation, including higher-precision outliers; the backend casts BF16 linear inputs to FP16 for quantization.
- **4-bit:** bitsandbytes NF4, double quantization enabled, BF16 compute. NF4 is a 4-bit codebook format, **not uniform integer INT4 and not integer-only inference**. Results do not establish performance for GPTQ/AWQ/GGUF/torchao INT4.
- Both modes convert **220 linear modules**, including vision and language layers. Embeddings, the output head, convolutions, normalizations and other non-linear modules remain higher precision. The benchmark verifies quantized layer types and that all parameters reside on the requested device; there is no CPU/disk offloading in GPU runs.
- CPU uses bitsandbytes native operations. Its optional external fused NF4 GEMM kernel had no build matching Torch 2.9; the package-provided native CPU fallback is measured. No kernel downloads occur during inference. The failed optional-kernel probe is preserved under artifacts/research.
- Primary workers quantize the downloaded BF16 file during loading, then serialize it before inference. Separate one-sample reload probes load the saved quantized checkpoint directly and must reproduce its first primary prediction exactly. Reload memory is a deployment diagnostic, not a full-cohort memory bound.

## Handwriting and math accuracy

Text scores are corpus-level cased character error rates. Nonspace CER removes whitespace while preserving case/punctuation; raw CER retains IAM spacing. Formula matches use the existing limited formatting normalization, preserving operators, roots, fractions, case and grouping. GLM commonly adds display-math wrappers, so raw strict string scoring is also retained in JSON but is not the main formula comparison. No semantic rewriting or LLM grading is used.

| Device | Precision | Text raw CER | Text nonspace CER | Math format matches | Text median / P95 | Math median / P95 |
|---|---|---:|---:|---:|---:|---:|
| cpu | BF16 | 5.55% | 3.32% | 30/100 | 2.362 s / 4.359 s | 1.912 s / 3.088 s |
| cpu | INT8 | 5.97% | 3.90% | 30/100 | 3.644 s / 5.786 s | 2.085 s / 3.252 s |
| cpu | 4-bit NF4 | 5.44% | 3.62% | 25/100 | 2.331 s / 3.675 s | 1.629 s / 3.104 s |
| cuda | BF16 | 5.57% | 3.39% | 30/100 | 6.033 s / 9.610 s | 3.165 s / 4.932 s |
| cuda | INT8 | 5.50% | 3.34% | 31/100 | 6.249 s / 9.019 s | 3.654 s / 5.222 s |
| cuda | 4-bit NF4 | 5.38% | 3.57% | 25/100 | 5.977 s / 8.636 s | 3.201 s / 4.762 s |

The previous BF16 run occurred earlier on the same laptop. Clocks, thermals and desktop activity are unlocked; small latency differences may reflect session variability. Primary BF16/quantized image processing and token limits match. The app defaults to four threads and renders selected ink itself; these eight-thread component timings are not complete app latency measurements.

| Device / precision | Changed text outputs vs BF16 | Changed math outputs vs BF16 | Math lost matches | Math gained matches | Text / math token-cap hits |
|---|---:|---:|---:|---:|---:|
| cpu / int8 | 7 | 12 | 1 | 1 | 0 / 0 |
| cpu / nf4 | 66 | 57 | 8 | 3 | 0 / 0 |
| cuda / int8 | 9 | 11 | 1 | 2 | 0 / 0 |
| cuda / nf4 | 66 | 57 | 8 | 3 | 0 / 0 |

Paired accuracy differences are also bootstrapped over samples (10,000 resamples, seed 0). The intervals describe sampling uncertainty within these convenience cohorts; they do not address training overlap or population bias. Positive text differences mean more errors; positive math differences mean more matches.

| Device / precision | Text CER difference, percentage points (95% interval) | Math match-rate difference, percentage points (95% interval) |
|---|---:|---:|
| cpu / int8 | +0.58 (-0.42, +2.26) | +0.00 (-3.00, +3.00) |
| cpu / nf4 | +0.30 (-0.61, +1.32) | -5.00 (-12.00, +1.00) |
| cuda / int8 | -0.05 (-0.18, +0.05) | +1.00 (-2.00, +5.00) |
| cuda / nf4 | +0.19 (-0.76, +1.22) | -5.00 (-12.00, +1.00) |

## Storage and memory

The original BF16 weight file is **2.469 GiB**. The BF16 deployment export below serializes only tensors loaded by the Transformers inference class, as do the quantized exports. This makes their storage comparison consistent; the original published file also has tensors not retained in these inference exports. Quantization metadata and retained BF16 tensors prevent simple 2×/4× total size reductions.

| Saved checkpoint | Weight file GiB | All model/processor files GiB |
|---|---:|---:|
| bf16-cpu | 2.063 | 2.069 |
| int8-cpu | 1.211 | 1.217 |
| int8-cuda | 1.211 | 1.217 |
| nf4-cpu | 0.796 | 0.802 |
| nf4-cuda | 0.796 | 0.802 |

| Primary worker | Peak host RSS MiB | End host RSS MiB | CUDA allocated / reserved peak MiB |
|---|---:|---:|---:|
| bf16-cpu-text | 2691 | 2651 | — |
| bf16-cpu-math | 2523 | 2485 | — |
| int8-cpu-text | 4144 | 4068 | — |
| int8-cpu-math | 3733 | 3674 | — |
| nf4-cpu-text | 3506 | 3508 | — |
| nf4-cpu-math | 3329 | 3328 | — |
| bf16-cuda-text | 3105 | — | 2266 / 2350 |
| bf16-cuda-math | 3111 | 1915 | 2218 / 2318 |
| int8-cuda-text | 3157 | 2147 | 1380 / 1504 |
| int8-cuda-math | 3183 | 2020 | 1324 / 1422 |
| nf4-cuda-text | 3199 | 2099 | 935 / 1014 |
| nf4-cuda-math | 3188 | 1983 | 897 / 958 |

Host RSS includes imports, conversion/loading, allocator retention and inference; it excludes the Folio UI. Primary CUDA peaks include conversion and inference and omit driver/context allocations outside the Torch allocator. End RSS is sampled after scoring. Large startup peaks can obscure steady-state savings.

| Saved-checkpoint reload probe | Peak host RSS MiB | End host RSS MiB | CUDA allocated / reserved peak MiB | Load seconds | Warm first-sample seconds |
|---|---:|---:|---:|---:|---:|
| bf16-cpu-text | 2495 | 2441 | — | 1.876 | 5.448 |
| int8-cpu-text | 1768 | 1663 | — | 3.373 | 7.473 |
| int8-cpu-math | 1613 | 1586 | — | 2.858 | 2.525 |
| nf4-cpu-text | 1714 | 1715 | — | 3.266 | 5.447 |
| nf4-cpu-math | 1632 | 1634 | — | 3.005 | 1.853 |
| bf16-cuda-text | 3088 | 1881 | 2240 / 2328 | 2.691 | 8.272 |
| int8-cuda-text | 2237 | 1984 | 1340 / 1408 | 3.862 | 8.823 |
| int8-cuda-math | 2261 | 1990 | 1280 / 1308 | 3.425 | 3.480 |
| nf4-cuda-text | 1941 | 1942 | 906 / 992 | 3.663 | 8.424 |
| nf4-cuda-math | 1967 | 1969 | 859 / 876 | 3.522 | 3.079 |

## Implementation recommendation

For the current **CPU-first app, keep BF16 as the accuracy default**. INT8 saved 41.3% of equivalent exported weight storage, but its text median was 1.54× BF16 and its observed text CER was higher. It is a footprint option when the storage/RAM saving matters enough to justify the measured CPU latency cost.

**NF4 gives the smallest weight files**, saving 61.4% of equivalent exported weight storage. CPU text latency was close to BF16 (0.99×); CPU math was faster in this run but matched only 25/100 formulas against BF16's 30/100. Select it for a constrained-memory mode only after checking personal handwriting and equations. The small cohorts do not establish a population-level accuracy ranking; paired intervals above show uncertainty.

Direct CPU reload probes show similar overall host RAM for INT8 and NF4, despite NF4 having much smaller weight files. Framework, activation, temporary computation and allocator overhead remain. Loading a prequantized export avoids the much larger primary-worker conversion peak; the short probes cannot establish full-page or long-output memory bounds.

On GPU, quantization substantially lowers Torch VRAM use but does not produce a clear text speedup in this backend. Use the device-specific accuracy and formula results above when choosing a deployment mode. Saved files passed direct reload probes; the app worker/setup still require explicit quantization support before these exports can become app options. **This benchmark leaves the existing BF16 app pack unchanged.**

## Limits and reproduction

The 100 English IAM lines and 100 MathWriting excerpt equations are fixed convenience cohorts, not full public benchmarks or representative personal handwriting. Training overlap is not fully audited. These runs do not evaluate paragraphs, mixed-page routing, multilingual notes, pen trajectories, edits, equation solving or MyScript. Quantization can change correct and incorrect answers independently, so equal aggregate scores do not imply identical predictions.

From the project root, install the pinned quantization packages into `artifacts/benchmarks/recognition-2026-10-04/quantization/deps`, then run `run_glm_quantization.py` with the existing benchmark Python. After primary inference finishes, run `run_glm_quantized_reload.py`, then `analyze_glm_quantization.py`. The runner verifies the original weight SHA-256 and skips completed results. Move a result aside to repeat it. All export checksums and input hashes are retained in the summary.

Evidence: [summary and paired comparisons](artifacts/benchmarks/recognition-2026-10-04/quantization/summary.json), [CSV](artifacts/benchmarks/recognition-2026-10-04/quantization/summary.csv), [per-sample outputs](artifacts/benchmarks/recognition-2026-10-04/quantization/results/), [reload probes](artifacts/benchmarks/recognition-2026-10-04/quantization/reload/), [commands/process outcomes](artifacts/benchmarks/recognition-2026-10-04/quantization/jobs.jsonl), and [earlier BF16 benchmark](GLM_OCR_BENCHMARK_RESULTS.md).

Primary backend reference: [Transformers bitsandbytes documentation](https://huggingface.co/docs/transformers/en/quantization/bitsandbytes), [bitsandbytes CPU implementation](https://github.com/bitsandbytes-foundation/bitsandbytes/blob/0.50.2/bitsandbytes/backends/cpu/ops.py).
