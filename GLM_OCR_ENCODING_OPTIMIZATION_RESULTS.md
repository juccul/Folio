# GLM-OCR encoding optimization — 4 October 2026

**Implemented and installed a CUDA BF16 image-patch projection optimization.** On the same 100 handwriting lines and 100 equations, GPU text median fell from 6.033 seconds to 0.214 seconds, and math from 3.165 seconds to 0.272 seconds. Text nonspace CER remains 3.39%; formula format matches are 31/100 against 30/100 before.

## Confirmed bottleneck and implementation

CUDA-event module timing localized the slowdown to `model.visual.patch_embed.proj`, a Conv3d with kernel and stride `(2, 14, 14)`. Each input is already one complete RGB patch of exactly that shape, producing one output position. Its convolution is mathematically a dense projection of the flattened patch. In a fresh two-repeat probe on the first handwriting image, native patch projection took 8.645 and 8.698 seconds; the matrix projection took approximately 0.2–0.3 milliseconds. Full calls fell from 9.020/9.069 seconds to 0.376/0.366 seconds and reproduced the prediction. Both downsample-only probes retained the slowdown; the other vision blocks were each approximately 3.5 ms.

The adapter uses `torch.nn.functional.linear(pixels.flatten(1), weight.flatten(1), bias)` and restores the original output shape. It retains all original weight objects, bias, BF16 precision, input pixels, patch ordering, encoder blocks and decoder. It adds no model, dependency, compilation, cache of user images, downscaling or cloud service. It changes only the two-dimensional calculation of this full-kernel dot product.

The adapter is instance-local and validates the expected Conv3d geometry. CPU, other dtypes and other input geometries call the original operator. It does not alter state-dict keys or saved weights. The app defaults to `encoder_projection: "linear_cuda"`; `"native"` or setup `--encoder-projection native` restores the original path. Geometry not matching the validated GLM model is rejected by setup/load integration. The current CPU app pack is retained, and a ready CUDA pack is installed at `artifacts/recognition-cuda/pack.json`.

Earlier attention/synchronization explanations were hypotheses. The projection experiment establishes the expensive operation and an effective replacement, without claiming a specific internal cuDNN algorithm defect. The CUPTI operator trace did not complete, so these diagnostics use CUDA events and module hooks instead.

## Same-cohort GPU results

| Task | Native median / P95 | Optimized median / P95 | Median speedup | Accuracy before → after | Changed raw outputs |
|---|---:|---:|---:|---:|---:|
| text | 6.033 / 9.610 s | 0.214 / 0.365 s | 28.3× | 3.39% → 3.39% nonspace CER | 2 |
| math | 3.165 / 4.932 s | 0.272 / 0.490 s | 11.6× | 30/100 → 31/100 format matches | 5 |

Different kernels can accumulate BF16 products differently. Two text predictions and five math predictions changed. No previously correct normalized text line or formula became incorrect in these cohorts; one previously incorrect formula became correct. These small differences do not establish an accuracy improvement or guarantee identical predictions on other handwriting. Raw text CER, unchanged-input assertions, every output and paired sample decisions are retained in JSON. Token-cap hits remain zero.

Protocol: same Ryzen 9 7940HS / RTX 4070 Laptop 8 GiB, pinned model, Transformers 5.18.0, Torch 2.9.1+cu128, SDPA, official processor/prompts, BF16, batch one, eight CPU threads, greedy decoding, KV cache and 512 new-token cap. Inference is offline. One cold plus two warm-up calls precede 100 scored calls per task. Timings include image reading, processing, generation and decoding, excluding model loading; CUDA synchronizes at timing boundaries. The full native cohort is reused from the earlier benchmark, while the fresh first-image ablations independently reproduce the large slowdown and speedup. Thermals/clocks are unlocked. This is a component benchmark; the app uses four threads and its selected-ink renderer and text token cap.

Torch peak GPU allocation remains about 2266 MiB for text and 2218 MiB for math. The optimization accelerates computation and does not shrink the model.

## CPU and installed-worker validation

On CPU, the original first-image patch projection took 7–8 ms; the linear version took about 3 ms, while whole calls remained approximately 4.0–4.4 seconds. The CPU path therefore retains the original operator. This targeted optimization provides its material benefit on CUDA.

Four projection tests pass with the CUDA runtime: float64 equivalence for biased/unbiased and noncontiguous patches, unchanged parameters/state dictionary and CPU behavior, rejection of unsupported geometry, and CUDA BF16 equivalence plus fallback for larger input shapes. The CPU-only runtime passes three and skips its CUDA-only test. All eight existing worker preprocessing/offline regressions pass.

The installed four-thread worker passed actual JSON-line requests for text → math → text and a two-line paragraph. It returned `hello` consistently and preserved `hello\n\nhello`, using one resident process. Math recognition remains a suggestion requiring review. UI/controller code was not changed.

## Use and reproduce

Use the ready CUDA pack with a CUDA-capable runtime:

```sh
FOLIO_RECOGNITION_CONFIG="$PWD/artifacts/recognition-cuda/pack.json" target/release/folio
```

Recreate it with `artifacts/benchmarks/recognition-2026-10-04/venv/bin/python scripts/setup-recognition.py --output artifacts/recognition-cuda --device cuda`. No download is performed. Restart a resident app worker to pick up updated Python code.

The reproduction script `probe_glm_vision.py` tests original, linear patch, linear downsample and both operators; `--device cpu` runs CPU probes. To repeat a full cohort, run `benchmark.py --model glm-ocr --kind text --device cuda --encoder-projection linear_cuda --threads 8 --limit 100 --out ...`, then repeat with `--kind math`. `analyze_glm_encoding.py` validates cohort/input equality and creates this report.

Evidence: [summary, hashes, paired results and probes](artifacts/benchmarks/recognition-2026-10-04/diagnostics/encoding-optimization/summary.json), [installed worker verification](artifacts/validation/encoding-optimization/worker-integration.json), [adapter](scripts/recognition_encoder.py), [original BF16 benchmark](GLM_OCR_BENCHMARK_RESULTS.md). Previous quantization results used the original Conv3d; their GPU latencies are historical and should not be used as optimized-encoder estimates.

Operator definitions: [PyTorch Conv3d](https://docs.pytorch.org/docs/2.9/generated/torch.nn.Conv3d.html), [PyTorch linear projection](https://docs.pytorch.org/docs/2.9/generated/torch.nn.functional.linear.html).
