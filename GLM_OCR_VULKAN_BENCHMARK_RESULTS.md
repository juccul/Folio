# GLM-OCR Vulkan/GGUF benchmark — 7 October 2026

**Use Vulkan Q8 as the GPU deployment candidate for Folio.** On this laptop, it works on both the NVIDIA RTX 4070 and AMD Radeon 780M without a CUDA inference runtime. NVIDIA Q8 is faster than Folio's optimized CUDA BF16 and CUDA INT8 backends. AMD Q8 is substantially faster than the current CPU backend. Keep PyTorch BF16 available for CPU users who prioritize text latency: the GGUF CPU fallback saves memory and dependencies but is slower for text.

This pass completed **1,600 fresh scored predictions**: 100 handwritten text lines and 100 handwritten equations in each of eight configurations. Four additional Flatpak smoke tests reproduced their native predictions. The application backend and recognition settings were not changed.

## Hardware and protocol

- Ryzen 9 7940HS, approximately 30 GiB system RAM; NVIDIA RTX 4070 Laptop GPU with 8 GiB VRAM; AMD Radeon 780M integrated GPU using RADV. Fedora 44; NVIDIA driver 615.71.09. GPU discovery is retained in the environment manifest.
- Same fixed 100 IAM English lines and 100 MathWriting excerpt equations used in the earlier benchmarks. These are convenience cohorts, not full benchmark test sets. The math images are the existing stroke renderings. Images, ordering, targets, and input hashes were checked.
- Four CPU inference threads, matching Folio's worker. Batch one, greedy decoding, official `Text Recognition:` / `Formula Recognition:` prompts, 512 output-token cap, no output repair, no speculative decoding. Runs are sequential, in fresh processes. One cold request and two warmups precede 100 scored requests per task. There were **zero output-token-cap hits**.
- CUDA uses the existing optimized image-patch projection, BF16 activations, SDPA, Torch 2.9.1+cu128 and Transformers 5.18.0. INT8 uses bitsandbytes 0.50.2 with the existing threshold-6 outlier handling. CPU BF16 uses Torch 2.9.1+cpu. Every baseline was rerun; historical slow Conv3d GPU timings are not used.
- llama.cpp **b11457**, source commit `5ad1c5da0ad7f6176256b823925aad19134f0263`; official Linux Vulkan binary archive verified against its published SHA-256. The server uses local model paths, `--offline`, loopback-only binding, one slot, a fixed 2,048-token context, and disabled prompt caching. Every scored response reports **zero cached prompt tokens**. This context fits all these crops; it is not a full-page memory configuration.
- FP16 and Q8 language/vision files were converted from Folio's exact checkpoint: revision `2e85a62840ccac27daa451df36c736c4636b8628`, weight SHA-256 `a16eb0de98d199293371c560f95f83130d2a2c9612449df16839f08ff9498815`.
- All 179 converted FP16 language tensors match the published FP16 GGUF exactly. All 348 converted Q8 vision tensors match the published Q8 projector. The published Q8 language model differs from the fresh conversion in two embedding tensors, so scored runs consistently use the fresh pinned conversion. Tensor comparisons and export hashes are retained.
- Q8 means **GGUF Q8_0 packed weights**, including block scales and retained higher-precision tensors; computation uses mixed precision. It is a different quantization/backend from bitsandbytes INT8. FP16 controls likewise retain higher-precision normalization tensors.

Latency includes image reading and the complete recognition call. llama.cpp also includes base64/JSON and local HTTP overhead; Transformers includes its image processor, generation and decoding. Model loading, stroke rasterization, Folio's UI, and inserting/rendering the result are excluded. CUDA timing boundaries synchronize the GPU. Clocks, thermals and desktop activity are unlocked, so small timing differences should not be treated as precise hardware rankings.

## Recognition and latency

All times below are **warm median / P95 in milliseconds**. Text CER removes whitespace but preserves case and punctuation; lower is better. Raw CER retaining whitespace is also shown. Math matches use the earlier limited LaTeX formatting normalization, preserving operators and grouping. This is not semantic equivalence or equation-solving accuracy.

| Backend / device | Precision | Text ms | Math ms | Text nonspace CER | Text raw CER | Math format matches |
|---|---|---:|---:|---:|---:|---:|
| PyTorch / CPU | BF16 | 3,519 / 5,680 | 2,295 / 3,296 | 3.32% | 5.55% | 30/100 |
| PyTorch / NVIDIA CUDA | BF16, optimized encoder | 198 / 346 | 262 / 442 | 3.39% | 5.61% | 31/100 |
| PyTorch / NVIDIA CUDA | bitsandbytes INT8, optimized encoder | 491 / 865 | 742 / 1,284 | 3.36% | 5.65% | 31/100 |
| llama.cpp / NVIDIA Vulkan | FP16 | 187 / 340 | 198 / 329 | 3.15% | 5.29% | 30/100 |
| llama.cpp / NVIDIA Vulkan | Q8_0 | **164 / 266** | **162 / 233** | 3.11% | 5.23% | 30/100 |
| llama.cpp / AMD Vulkan | FP16 | 1,029 / 1,574 | 838 / 1,202 | 3.18% | 5.33% | 30/100 |
| llama.cpp / AMD Vulkan | Q8_0 | **770 / 1,170** | **547 / 758** | 3.20% | 5.33% | 30/100 |
| llama.cpp / CPU, no GPU offload | Q8_0 | 4,653 / 7,314 | 2,275 / 3,520 | 3.18% | 5.36% | 31/100 |

NVIDIA Q8 reduces median latency by **17% for text and 38% for math** against optimized CUDA BF16, and by **67% / 78%** against CUDA INT8. AMD Q8 is **4.57× / 4.20×** faster than the current CPU BF16 backend. The GGUF CPU fallback is **32% slower for text**; its math median is approximately unchanged, with a somewhat higher P95.

The cohorts do not establish a general accuracy improvement. Against fresh CUDA BF16, NVIDIA Q8 changes 22 raw text outputs and 26 raw math outputs. Its nonspace CER difference is −0.28 percentage points, with a paired bootstrap 95% interval of **−0.64 to +0.07**. The formula match-rate difference is −1 point, with an interval of **−6 to +3**. It loses three previously matched formulas and gains two. Keep the existing recognition review step; similar aggregate scores do not mean identical answers. Intervals use 10,000 sample resamples with seed 0 and do not address cohort bias or training overlap.

### Preprocessing edge case

The actual server prompt, after expanding its media marker, matches the checkpoint's Transformers template, including the final newline. All text input-token counts match the baseline.

Two math inputs, `01a2f3186b7e33e1` and `01d90fce29b20f64`, are **640×182 pixels**. At the 28-pixel alignment boundary, Python's rounding and C++ `std::round` choose different heights. Transformers produces 151 prompt tokens, while llama.cpp produces 174. Nominal min/max image budgets are matched (16 / 12,288 image tokens), but these two resized images differ. Consequently this comparison measures a deployed recognition pipeline, not arithmetic differences alone. The source images themselves are identical.

## Encoding diagnostics

The server trace brackets image encoding from `encoding mtmd batch` to the beginning of image-embedding decoding. These intervals include encoder setup and transfers and are not isolated GPU-event measurements. Prompt time includes image processing/encoding and language-model prefill. Columns are separate medians and should not be added as an exact breakdown of the median whole call.

| llama.cpp configuration | Text encoding ms | Math encoding ms | Text prompt / generation ms | Math prompt / generation ms |
|---|---:|---:|---:|---:|
| NVIDIA FP16 | 70 | 34 | 97 / 70 | 59 / 125 |
| NVIDIA Q8 | 77 | 41 | 103 / 45 | 68 / 79 |
| AMD FP16 | 622 | 305 | 777 / 219 | 410 / 386 |
| AMD Q8 | 514 | 233 | 614 / 130 | 294 / 219 |
| CPU Q8 | 3,653 | 1,492 | 4,494 / 173 | 1,912 / 289 |

On NVIDIA, Q8's total speedup comes from the language stage even though its vision encoding is slightly slower than FP16. On CPU, image processing/encoding dominates and needs further optimization before the GGUF backend can replace the faster PyTorch text path.

## Storage and memory

The vision encoder is required in addition to the language GGUF. Download estimates use the actual model file sizes plus the published Vulkan archive, rather than the language-model listing alone.

| Component | FP16 | Q8_0 |
|---|---:|---:|
| Language GGUF | 1.786 GB | 0.950 GB |
| Vision/projector GGUF | 0.869 GB | 0.484 GB |
| Both model files | 2.655 GB | **1.435 GB** |
| Complete published Vulkan runtime download | 31.68 MB | 31.68 MB |
| Models + runtime download | 2.686 GB | **1.467 GB** |
| Models + unpacked full runtime | 2.745 GB | **1.525 GB** |

Decimal GB/MB are used here. The unpacked runtime contains all released tools and totals 90.15 MB; a server-only payload could omit some tools. These are inference assets, not rebuilt RPM/DEB/Flatpak sizes. Folio integration, notices, platform/system libraries and graphics drivers are outside this estimate. Model conversion is a preparation step; users of a preconverted pack would not need Python/PyTorch to convert the weights themselves.

| Worker | Peak host RSS, text / math MiB | GPU memory, text / math MiB |
|---|---:|---:|
| PyTorch CPU BF16 | 2,688 / 2,508 | — |
| PyTorch CUDA BF16 | 3,122 / 3,130 | 2,266 / 2,218 **Torch allocated** |
| PyTorch CUDA INT8 | 3,192 / 3,198 | 1,381 / 1,325 **Torch allocated** |
| NVIDIA Vulkan FP16 | 1,879 / 1,880 | 2,568 / 2,315 **NVML process memory** |
| NVIDIA Vulkan Q8 | 1,083 / 1,083 | 1,513 / 1,432 **NVML process memory** |
| AMD Vulkan FP16 | 414 / 306 | System RAM shared with GPU; see below |
| AMD Vulkan Q8 | 413 / 301 | System RAM shared with GPU; see below |
| CPU GGUF Q8 | 2,045 / 1,862 | — |

Host RSS includes loading and file mappings and excludes Folio's UI. NVIDIA NVML samples both compute/graphics process entries every 100 ms and includes driver/context allocations. Torch allocator numbers exclude those allocations, so their GPU-memory columns are **different scopes**, not direct total-VRAM comparisons. End-of-run RSS and Torch reserved peaks are retained in JSON.

AMD's low host RSS does **not** mean the entire model uses only 300–400 MiB. Its integrated GPU allocates system RAM through the graphics driver. Mid-cohort DRM snapshots show approximately **2,381 MiB** of resident GPU buffers for FP16 and **1,508 MiB** for Q8. These are snapshots, not complete-run peaks. Do not add UMA driver residency to host RSS as if they were independently accounted physical-memory totals.

## Startup and Flatpak validation

There is a material first-call setup cost. The initial NVIDIA FP16 pilot needed **35.20 seconds** for its first recognition after **5.91 seconds** to reach server readiness; subsequent identical uncached requests took approximately 0.32 seconds. In the scored NVIDIA Q8 text run, server readiness took **1.13 seconds** and the first recognition **11.42 seconds**. The FP16 math first request took 12.67 seconds, and Q8 math 1.22 seconds. Driver/kernel caches were not purged, so these are observed first calls in new processes, not guaranteed clean-machine startup bounds.

Prepare and warm the GPU worker in the background after installation/download, with progress shown to the user, and retain the resident worker. CUDA baseline `load_seconds` only measures model loading after Python imports; llama.cpp readiness includes starting the server process. Their load fields have different scopes and should not be ranked directly.

Inside the installed Folio Flatpak (`org.freedesktop.Platform` 25.08), the runtime discovered both GPUs. The existing sandbox then ran one public text fixture and one math fixture on **each** GPU, with all four CLI outputs containing the exact corresponding native Q8 prediction. Launch-to-exit times were 20.13 / 2.31 seconds on NVIDIA and 3.03 / 2.03 seconds on AMD. These include startup and are smoke tests, not the warm cohort benchmark.

The checks used temporary read-only access to benchmark assets and a temporary cache directory. They launched the OCR CLI through Flatpak's command override, without opening Folio or changing its notes/settings. They validate runtime/driver compatibility on this machine; the downloadable pack and Folio worker integration have not been implemented by this benchmark.

## Recommendation and limits

1. Integrate **Vulkan Q8_0 for GPU recognition**, sharing one language/vision pack across NVIDIA and AMD. It offers the best measured speed/footprint combination here. Prefer the discrete RTX GPU when both tested GPUs are available; selecting the first enumerated device on this laptop would select the slower AMD iGPU.
2. Keep the existing **CPU BF16 option** for users who prioritize text latency. Offer the GGUF CPU fallback when smaller runtime/dependency requirements matter more; it is functional and saves host RAM, but is not a text speed upgrade in this build.
3. Include first-use preparation/warmup and preserve the review/undo workflow. The measured formula quality still requires review. Benchmarking does not replace testing a user's own writing or full-page selections.

Intel GPUs, other NVIDIA/AMD generations, full-page layout, multilingual text, unusual image sizes, actual stylus trajectories and complete Folio UI latency were not measured. Image-token budgets and context size must be revisited for larger selections. These results also do not benchmark MyScript, Photomath or equation solving.

## Reproduction and evidence

Use the existing Folio benchmark environment and prepared cohorts. On this laptop the scripts map `Vulkan0` to AMD and `Vulkan1` to NVIDIA; inspect `--list-devices` and update the mapping before using a different machine. Completed 100-sample result files are skipped; move a result aside to rerun it. Do not run scored model jobs concurrently.

```sh
python3 scripts/benchmark-glm-vulkan-prepare.py
python3 scripts/benchmark-glm-baselines.py
python3 scripts/benchmark-glm-vulkan-convert.py
python3 scripts/benchmark-glm-vulkan.py --device nvidia --device amd
python3 scripts/benchmark-glm-vulkan.py --device cpu --precision q8
artifacts/benchmarks/recognition-2026-10-04/venv/bin/python scripts/benchmark-glm-vulkan-analyze.py
```

Evidence lives in `artifacts/benchmarks/vulkan-2026-10-07/`:

- [Scored summary and paired comparisons](artifacts/benchmarks/vulkan-2026-10-07/summary.json), [CSV](artifacts/benchmarks/vulkan-2026-10-07/summary.csv), [every prediction/response](artifacts/benchmarks/vulkan-2026-10-07/results/) and [logs](artifacts/benchmarks/vulkan-2026-10-07/logs/).
- [Asset revisions/checksums](artifacts/benchmarks/vulkan-2026-10-07/assets.json), [conversion commands/hashes](artifacts/benchmarks/vulkan-2026-10-07/conversion.json), [tensor provenance](artifacts/benchmarks/vulkan-2026-10-07/tensor-provenance.json), [prompt checks](artifacts/benchmarks/vulkan-2026-10-07/prompt-verification.json), [environment](artifacts/benchmarks/vulkan-2026-10-07/environment.json) and [executed commands](artifacts/benchmarks/vulkan-2026-10-07/jobs.jsonl).
- [Initial NVIDIA pilot](artifacts/benchmarks/vulkan-2026-10-07/pilot-nvidia-f16-text.json), [Flatpak smoke outputs](artifacts/benchmarks/vulkan-2026-10-07/flatpak-smoke.json), [FP16 AMD driver snapshot](artifacts/benchmarks/vulkan-2026-10-07/amd-driver-snapshot.json) and [Q8 AMD driver snapshot](artifacts/benchmarks/vulkan-2026-10-07/amd-q8-driver-snapshot.json).

Primary sources: [llama.cpp b11457](https://github.com/ggml-org/llama.cpp/tree/b11457), [official GGUF distribution](https://huggingface.co/ggml-org/GLM-OCR-GGUF/tree/65a42de1148dbed2297e922b5dbc7d9b70c36578), [GLM-OCR model](https://huggingface.co/zai-org/GLM-OCR), [image alignment implementation](https://github.com/ggml-org/llama.cpp/blob/b11457/tools/mtmd/mtmd-image.cpp).
