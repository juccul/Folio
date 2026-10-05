# Model selection for offline handwriting and mathematics recognition

Research date: 4 October 2026. Constraint: fully offline inference on Linux, with freely reusable open-source code and model weights. This report refines [the pipeline research](STROKE_RECOGNITION_PIPELINE_RESEARCH.md); it does not add recognition to Folio.

## Recommendation

**Use specialist models in a layered pipeline. A single general vision-language model is currently a poor default for interactive handwriting.** My first implementation experiment would compare HTR-ConvText against TrOCR Small for line text, PP-FormulaNet Plus-S against UniMERNet Tiny for formula crops, and use Uni-MuMER Qwen3.5-2B as an optional accuracy-focused second pass. Preserve original strokes and implement source ownership independently of these image recognizers.

For a system that operates directly on trajectories and supports MyScript-like corrections, use OnlineHTR and MathNote/Seshat as architectural starting points. Their available recognizers are not established substitutes for MyScript accuracy. The direct-stroke route needs substantially more training and evaluation than the raster route.

These are **ranked candidates for evaluation**, not a claim that untested models meet the app's accuracy or latency requirements. Most new candidates have not been run locally. Existing local probes are identified separately below.

| Pipeline component | First candidate | Comparison candidate | Reason for the choice |
|---|---|---|---|
| Text from rendered ink lines | **HTR-ConvText, IAM checkpoint** | **TrOCR Small Handwritten** | CTC encoder with released weights versus a mature, small autoregressive baseline |
| Very small text baseline | **Teklia PyLaia IAM** | HTR-ConvText | Approximately 42.7 MB checkpoint, conventional CTC, weaker published accuracy |
| Compact formula recognition | **PP-FormulaNet Plus-S** | **UniMERNet Tiny** | Published CPU timing for Plus-S; explicit handwritten-expression evidence for UniMERNet |
| Accuracy-focused formula fallback | **Uni-MuMER Qwen3.5-2B** | **Uni-MuMER Qwen3-VL-2B** | Strong released HMER specialization; both have Apache-licensed base models |
| Mixed pages / multilingual fallback | **PaddleOCR-VL-1.6** | **GLM-OCR**; LightOnOCR-2-1B if page import matters | Local document OCR candidates, with handwriting suitability still requiring a dedicated evaluation |
| Direct trajectory text | **OnlineHTR architecture** | ScribeTokens-inspired training | Released implementation and externally advertised weights; no verified production-quality open model |
| Direct trajectory math with editable structure | **MathNote pipeline design**, compare **Seshat** | Train stronger symbol/group/relation heads | Explicit stroke groups and structure; existing MathNote weights performed poorly in the small local screen |
| Text/math/drawing routing | Explicit selection initially; later a small stroke graph classifier | PP-DocLayoutV3 for imported page images | Native ink routing needs different evidence from typeset document layout |

The first comparison is deliberately small. Do not integrate every model listed here into the application.

## What “best” means for this app

The useful objective is **correct transcription with quick, stable correction**, rather than the highest score on an unrelated document benchmark. Evaluate five distinct properties:

1. Writer-independent recognition on real pen input, including cursive text and nested mathematics.
2. Warm and cold CPU latency, memory, cancellation, and sensitivity to input length.
3. Whether alternatives and source evidence can support corrections without replacing unrelated ink.
4. Availability and reuse terms of the exact checkpoint, inference code, tokenizer, and base model.
5. Error behavior on blanks, drawings, partial expressions, delayed dots, erasures, and mixed content.

In recognition papers, **online** means coordinate trajectories, and **offline** often means raster images. Both can run without an internet connection. The application can render its own strokes locally for an image model while retaining the original trajectories for editing.

The initial text comparisons below assume modern English line handwriting. German, Italian, Vietnamese, and multilingual models are discussed where evidence exists. A checkpoint trained on English is not automatically a multilingual model, even when its tokenizer can encode additional characters.

## 1. Text recognition models

### HTR-ConvText: strongest new CTC candidate to test

The authors release a CNN/ViT encoder with a CTC head and a **training-only Textual Context Module**. The module is omitted at inference, so recognition does not require an autoregressive language decoder. Their stated model size is about **65.9M parameters**. Separate IAM, LAM, READ, Vietnamese, and Coptic checkpoint files are present on the public Hub. The model card and README badge declare Apache-2.0, but implementation inspection found a GPL-3.0 repository LICENSE at source revision `9ce7485ff171902bff44d77f4f6ff64d5878d82b`. Treat source and checkpoint licenses separately; retained inference source and provenance are under `third_party/htr-convtext`. [Paper](https://arxiv.org/html/2512.05021v1), [code](https://github.com/DAIR-Group/HTR-ConvText), [released models](https://huggingface.co/DAIR-Group/HTR-ConvText).

| Author-reported test set | CER | WER |
|---|---:|---:|
| IAM, English | 4.0% | 12.9% |
| LAM, Italian | 2.7% | 7.0% |
| READ2016, German | 3.6% | 15.7% |
| HANDS-VNOnDB, Vietnamese | 3.45% | 8.9% |

The model card lists 512×64 input and CTC inference. The Vietnamese dataset name does **not** make this released model a trajectory recognizer: the architecture consumes line images. [Model specifications and scores](https://huggingface.co/DAIR-Group/HTR-ConvText#model-overview).

**Why I would test it first:** CTC fits a bounded background line recognizer and supports beam decoding and forced alignment. The hierarchy is designed to reduce sequence length. These are architecture-based reasons to expect a useful deployment profile, not measured Folio latency.

**Important comparison limit:** its table uses a TrOCR comparator at 7.3% IAM CER. Microsoft separately reports 3.42% for TrOCR Base and 2.89% for Large under its own trained-model protocol. Therefore, “ConvText beats TrOCR” is not a valid general conclusion. Compare the actual released checkpoints on the same Folio inputs.

The IAM checkpoint is **529,444,565 bytes** in the observed Hub manifest. That is the download artifact, not measured resident memory, and should not be treated as a simple parameter-count calculation. A checkpoint can contain more than inference weights. Strip training-only state only after loading and verifying the original format. [Pinned checkpoint directory](https://huggingface.co/DAIR-Group/HTR-ConvText/tree/8ed2ad5390d67f44828c528d9254cc72deeab423/checkpoints).

### TrOCR: best established off-the-shelf text baseline

Microsoft's upstream model zoo reports the following **cased IAM CER**, with pretrained/fine-tuned models released for each size. These are single-line image recognizers, not page layout systems or direct-stroke models. [Microsoft model zoo](https://github.com/microsoft/unilm/blob/master/trocr/README.md).

| Model | Published parameters | Published IAM CER | Observed main weight file |
|---|---:|---:|---:|
| TrOCR Small Handwritten | 62M | 4.22% | 245.9 MB, FP32 `.bin` |
| TrOCR Base Handwritten | 334M | 3.42% | 1,333.4 MB, FP32 safetensors |
| TrOCR Large Handwritten | 558M | 2.89% | 2,229.0 MB, FP32 `.bin` |

The upstream project is MIT. In the model metadata inspected here, Base explicitly has an MIT tag, while Small and Large lack that tag; their cards point to the Microsoft release. Record the upstream license and checkpoint provenance rather than assuming every Hub card includes a license. [Upstream license](https://github.com/microsoft/unilm/blob/master/LICENSE), [Small](https://huggingface.co/microsoft/trocr-small-handwritten), [Base](https://huggingface.co/microsoft/trocr-base-handwritten), [Large](https://huggingface.co/microsoft/trocr-large-handwritten).

**Recommended role:** Small is the practical comparator for interactive CPU text. Base is an accuracy comparator, and Large belongs in an optional resource-heavy profile. An autoregressive decoder can be slow on long lines, and a low CER does not establish stable incremental output.

Keep line cropping, normalization, resize policy, and generation settings fixed during comparison. Evaluate punctuation, identifiers, numbers, and uncommon words separately; a decoder's linguistic prior can improve ordinary prose while damaging technical content.

### HTR-VT: useful research comparator, especially for training

HTR-VT is a **53.5M** CNN/ViT plus CTC model. The paper reports **4.7% IAM CER**, **2.8% LAM**, and **3.9% READ2016**, without the large synthetic pretraining used by some competitors. The official implementation is Apache-2.0 and advertises checkpoints in Google Drive. I verified the code/license and advertised link, but did not download or hash those checkpoint files. [Paper](https://arxiv.org/html/2409.08573v1), [implementation](https://github.com/Intellindust-AI-Lab/HTR-VT), [checkpoint folder](https://drive.google.com/drive/folders/1mGX9Dk7RBY5BKbqHnxwsHtCnX-SuH363?usp=sharing).

It is a good architecture for an application-specific training experiment. HTR-ConvText is the higher-priority released CTC candidate in this survey because its checkpoint manifest is straightforward to inspect and its reported results improve on HTR-VT in the authors' comparison.

### PyLaia: compact, transparent baseline

`Teklia/pylaia-iam` is a modern-English CTC model, MIT-tagged, trained on 6,482 IAM lines. It uses aspect-preserving line images at height 128. Its card reports **8.44% CER / 24.51% WER** without a language model and **7.50% / 20.98%** with its external character 6-gram language model. The Hub includes `weights.ckpt`, alphabet/lexicon files, and a language-model artifact. [Model card](https://huggingface.co/Teklia/pylaia-iam), [toolkit](https://github.com/jpuigcerver/PyLaia).

The checkpoint is **42,671,836 bytes**, excluding other files. This makes it a valuable low-footprint comparison. It is not the accuracy leader in the surveyed text models. Prefer it if a measured resource budget excludes the larger candidates and its actual correction cost is acceptable.

### Page-level and broader text alternatives

| Candidate | Verified evidence | Recommended role |
|---|---|---|
| **DAN, original release** | Source is CeCILL-C; pretrained page models advertised on Zenodo; RIMES and READ page results | Historical page import comparator; not the first model for selected live ink lines. [Source](https://github.com/FactoDeepLearning/DAN), [weights](https://zenodo.org/records/7244382) |
| **Meta-DAN / new DAN family** | Repository explicitly restricts research/academic use and requires permission for commercial use | Exclude from the freely reusable open-source stack. Do not transfer original DAN's license to this newer project. [License statement](https://github.com/FactoDeepLearning/META-DAN#license) |
| **Kraken** | Open toolkit for segmentation/recognition and a model ecosystem; checkpoints have their own scopes and licenses | Useful if historical documents or trainable page/line segmentation becomes a requirement; not one universal model. [Project](https://github.com/mittagessen/kraken) |
| **Thulium** | Apache code and impressive model-zoo claims, but the documented `v1.2.1` release and Tiny weight URL both returned HTTP 404 during this audit | Do not rank its claimed performance as a deployable pretrained option until the artifacts and evaluation can be verified. [Model zoo](https://github.com/thulium-htr/Thulium/blob/main/docs/models/model_zoo.md) |

Generic scene-text recognizers such as PARSeq are not prioritized merely because their code is small: camera scene text and cursive line handwriting are different recognition targets. Similarly, a headline score for a paper without a verified reusable checkpoint does not outrank a usable released model here.

## 2. Direct trajectory text recognition

### OnlineHTR: practical starting point, incomplete production stack

OnlineHTR implements a Google-inspired LSTM/CTC model in PyTorch with MIT code. Its advertised IAM-OnDB model uses interpolated **dx, dy, dt, pen-start** features. The README explicitly lists language-model CTC beam decoding and Bézier preprocessing as unfinished. Weights are advertised on the author's external site; their separate redistribution terms were not established in this audit. [Source and model description](https://github.com/PellelNitram/OnlineHTR), [author's weight page](https://lellep.xyz/blog/online-htr.html).

**Use it to establish a trajectory baseline, not to promise Google or MyScript quality.** Google's production multilingual results came from its own training system and data; this implementation does not release Google's 102-language production weights. [Google research paper](https://arxiv.org/abs/1902.10525).

For Folio, normalize document-space geometry and preserve real stroke boundaries. Do not feed display-smoothed paths or the full raw sequence of an erased fragment. Do not add pressure or tilt to an existing checkpoint's feature vector without retraining. Time features require the same units, resampling, and scaling as training.

CTC forced alignment can estimate character-to-timestep intervals. It does not solve delayed dots, crossing strokes, out-of-order additions, or true source ownership automatically. Corrections must retain the relevant stroke IDs and sample intervals.

### ScribeTokens: promising representation, not a ready replacement

ScribeTokens encodes movement with eight directional unit-step tokens plus two pen-state tokens, followed by BPE. The paper reports **8.27% CER on IAM** and **9.83% on DeepWriting** after its self-supervised pretraining. Those are comparisons among the paper's representations, not proof of superiority to every online recognizer. [Paper](https://arxiv.org/abs/2603.02805).

The separate `tokink` library is MIT. The research repository inspected here has no detected root license, and its documented exported-model folder is not evidence of a downloadable, licensed pretrained recognizer. Use the tokenizer as a research option; do not assume it supplies a production handwriting model. [Tokenizer](https://github.com/douglasswng/tokink), [research repository](https://github.com/douglasswng/scribe-tokens).

**Long-term text recommendation:** train a compact trajectory CTC encoder on data licensed for the intended distribution, compare it with the raster CTC winner, and add context rescoring only after measuring both technical-token errors and prose gains. A trajectory encoder with a raster teacher is an experimental training design, not a released checkpoint identified by this survey.

## 3. Mathematics recognition models

### Uni-MuMER Qwen3.5-2B: first accuracy-focused candidate

The newer Uni-MuMER family fine-tunes Qwen multimodal models for formula transcription using tree, error, and symbol-counting supervision. The Qwen3.5-2B release is Apache-2.0-tagged and uses an Apache-licensed base. It is an **image-to-LaTeX** model, not a raw-stroke model. [Model card](https://huggingface.co/phxember/Uni-MuMER-Qwen3.5-2B), [base license](https://huggingface.co/Qwen/Qwen3.5-2B/blob/main/LICENSE), [training/inference code](https://github.com/BFlameSwift/Uni-MuMER).

The authors' current release table gives these **exact-match expression rates**. These are self-reported checkpoint results, not a local reproduction. [Release evaluation table](https://github.com/BFlameSwift/Uni-MuMER#benchmark-results).

| Test set | Qwen3.5-2B | Qwen3-VL-2B | Qwen3.5-4B |
|---|---:|---:|---:|
| CROHME 2014 | 83.98% | 83.27% | 82.56% |
| CROHME 2016 | 81.17% | 78.55% | 78.20% |
| CROHME 2019 | 80.15% | 79.40% | 75.98% |
| CROHME 2023 test | 69.43% | 70.96% | 66.74% |
| HME100K test | 70.43% | 69.31% | 70.02% |
| MathWriting test | 51.84% | 50.66% | 54.32% |

**Selection:** start with Qwen3.5-2B and retain Qwen3-VL-2B as a serious comparator. The 4B model improves some cases, but there is no uniform benefit that justifies doubling the default footprint.

Avoid selecting by the published **73.09% “Average”** alone. On the model card, that number is the mean of eight rows **including CROHME 2023 validation** and printed Im2LaTeX. It is not a handwriting-only test average. For MathWriting, the same model reports **70.70% CDM expression rate**, which is visual-equivalence-aware and differs from **51.84% string exact match**. Neither is 95.1% correct equations merely because CDM F1 is 0.951. [Checkpoint metrics](https://huggingface.co/phxember/Uni-MuMER-Qwen3.5-2B#benchmark-results).

The observed Qwen3.5-2B BF16 weight file is **4.43 GB**; Qwen3-VL-2B is **4.26 GB**. Runtime requires additional memory for image encoding, state/caches, and generation. Use local GPU acceleration where available; on CPU, evaluate it as an explicit or idle second pass. A previous Qwen3-VL-2B CPU run in this workspace took seconds per short formula, described below.

### Licensing correction: original Qwen2.5-VL-3B variant

The original Uni-MuMER Qwen2.5-VL-3B model card is Apache-tagged, but the upstream Qwen2.5-VL-3B-Instruct **LICENSE grants non-commercial research/evaluation use** and requires a separate license for commercial use. The fine-tune's tag does not establish that this inherited restriction disappeared. Exclude that variant from this strict open-source recommendation unless the upstream rights are clarified. Prefer the newer Apache-based variants. [Actual base-model license](https://huggingface.co/Qwen/Qwen2.5-VL-3B-Instruct/blob/main/LICENSE), [fine-tune card](https://huggingface.co/phxember/Uni-MuMER-Qwen2.5-VL-3B).

This is a concrete artifact mismatch, not a general claim that all Qwen models share the same license. Qwen3-VL-2B-Instruct has an Apache license. [Qwen3-VL license](https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct/blob/main/LICENSE).

### PP-FormulaNet Plus-S: first compact CPU formula candidate

Paddle publishes reusable inference weights and Apache-tagged model cards. The Plus-S variant is designed for smaller, faster English-formula recognition; Plus-M/L support a broader Chinese/complex-formula setting. All consume raster images and output LaTeX. [Plus-S card](https://huggingface.co/PaddlePaddle/PP-FormulaNet_plus-S), [Plus-M card](https://huggingface.co/PaddlePaddle/PP-FormulaNet_plus-M), [formula recognition documentation](https://www.paddleocr.ai/main/en/version3.x/module_usage/formula_recognition.html).

| Variant | Published storage | Published CPU inference | English BLEU | Chinese BLEU |
|---|---:|---:|---:|---:|
| Original S | 224 MB | 254.39 ms | 87.00 | 45.71 |
| Plus-S | 248 MB | 260.99 ms | 88.71 | 53.32 |
| Plus-M | 592 MB | 1,615.80 ms | 91.45 | 89.76 |
| Plus-L | 698 MB | 3,125.58 ms | 92.22 | 90.64 |

These are Paddle's internal formula test set, FP32 CPU with eight threads on an **Intel Xeon Gold 6271C**, high-performance backend, **inference only**. They exclude preprocessing and postprocessing. BLEU is not expression exact-match accuracy, and this internal test is not a demonstrated benchmark of Folio handwriting. [Benchmark definitions and hardware](https://www.paddleocr.ai/main/en/version3.x/pipeline_usage/formula_recognition.html).

**Why Plus-S ranks highly:** it provides the clearest published CPU deployment evidence among the newly considered compact formula models. **What remains unresolved:** handwritten expression accuracy and export/runtime integration on the target machine. Compare Plus-S with UniMERNet Tiny before choosing it.

The observed Hub inference artifacts are about **256.8 MB** for Plus-S and **617.1 MB** for Plus-M, rather than exactly the documentation's rounded sizes. Use exact file manifests for packaging. Disable optional document orientation, dewarping, and page layout when recognizing an already selected vector-ink region.

### UniMERNet Tiny: strongest compact comparator with explicit handwriting evidence

UniMERNet uses a Swin-style image encoder and mBART-style decoder, trained for varied formula images. Its code and Tiny/Small/Base cards are Apache-2.0. The actual publisher namespace for these checkpoints is **`wanderkid`**, linked by the official repository, not the initially guessed `opendatalab/unimernet_*` model paths. [Official repository](https://github.com/opendatalab/UniMERNet), [Tiny](https://huggingface.co/wanderkid/unimernet_tiny), [Small](https://huggingface.co/wanderkid/unimernet_small), [Base](https://huggingface.co/wanderkid/unimernet_base).

| Variant | Published size | Observed weight artifact | UniMER-Test handwritten BLEU / normalized edit distance |
|---|---:|---:|---:|
| Tiny | Approximately 100–107M parameters | 430.1 MB | 0.883 / 0.078 |
| Small | 202M | 810.3 MB | 0.889 / 0.075 |
| Base | 325M | 1,300.8 MB | 0.895 / 0.072 |

The paper itself lists Tiny as 100M in one architecture table and 107M in its comparison table; this report preserves that discrepancy. The handwritten subset has 6,332 examples. Its BLEU/edit-distance results cannot be compared numerically with Uni-MuMER exact-match rates. Published image throughput is not a laptop CPU latency claim. [Paper and tables](https://arxiv.org/html/2404.15254v2).

**Selection:** Tiny is the first candidate; move to Small only if it materially reduces local correction cost. A relatively modest aggregate score gain from Base may not justify the larger memory and autoregressive CPU cost.

### FormulaNet / Texo: keep as a speed baseline

FormulaNet is approximately **20M parameters**, AGPL-3.0-tagged, with ready ONNX encoder/decoder files. This is attractive for an embedded local runtime, but the earlier Folio screen found very poor whole-expression results. Do not choose it from its footprint alone. The released model is `alephpi/FormulaNet`; it is distinct from Paddle's PP-FormulaNet family. [Model and weights](https://huggingface.co/alephpi/FormulaNet), [Texo integration](https://github.com/alephpi/Texo).

AGPL is an open-source license, with different redistribution obligations from Apache/MIT. If this route wins, check the combined application's license and distribution design before packaging it.

### Other specialist math models: useful ideas, limited priority

| Candidate | Evidence / availability | Decision |
|---|---|---|
| **SSAN** | Official code has MIT license; three model families' weights advertised in Drive; symbol spatial distribution auxiliary task | Worth a later compact HMER comparison. Weight hashes, artifact terms, and CPU runtime are not verified here. [Source](https://github.com/Howrunz/SSAN), [license](https://github.com/Howrunz/SSAN/blob/main/LICENSE) |
| **CoMER** | Official raster model and checkpoint committed; coverage attention; no top-level license detected | Do not adopt as a freely reusable component without a license grant. A bundled evaluator's license does not license the model. [Repository](https://github.com/Green-Wood/CoMER) |
| **TAMER** | Raster/tree-aware model; published 61.97% CROHME2019 ExpRate and 69.50% HME100K with fusion; committed checkpoints; no top-level license detected | Research comparator after rights clarification. It does not provide original-stroke ownership merely by using tree supervision. [Repository](https://github.com/qingzhenduyu/TAMER) |
| **PosFormer** | Committed checkpoint; README says academic research only while mentioning BSD; no actual top-level license detected | Exclude pending clarification; position supervision is relevant architectural research. [Repository and license statement](https://github.com/SJTU-DeepVisionLab/PosFormer#license) |
| **NAMER** | Non-autoregressive bottom-up recognition research | Architecturally interesting for latency; no official reusable released checkpoint verified in this audit. [Paper](https://arxiv.org/abs/2407.11380) |
| **TAP** | Direct-trajectory GRU/attention research code and preprocessing; no root license detected | Study spatial/temporal attention design; not a verified freely reusable deployment choice. [Repository](https://github.com/JianshuZhang/TAP) |
| **TexTeller** | Apache code/card, about 298M stored model parameters; released Transformers and ONNX artifacts | Useful optional image-math baseline, but lower priority than specialized handwritten candidates. [Code](https://github.com/OleehyO/TexTeller), [weights](https://huggingface.co/OleehyO/TexTeller) |
| **Pix2tex / LaTeX-OCR** | Accessible image-formula baseline; UniMERNet paper reports poor handwritten-subset results | Do not make it the handwriting default because it works well on rendered formulas. [Paper comparison](https://arxiv.org/html/2404.15254v2), [code](https://github.com/lukas-blecher/LaTeX-OCR) |
| **IMPO, CVPR 2026** | Image-level reward improves HMER training on multiple backbones | Training improvement to monitor; no verified released deployment checkpoint in this survey. [Primary paper](https://openaccess.thecvf.com/content/CVPR2026/html/Liu_From_Pixel_to_Precision_Enhancing_Handwritten_Mathematical_Expression_Recognition_with_CVPR_2026_paper.html) |

Do not compare a CROHME-trained compact model and a large model trained on additional datasets as if their reported scores establish an architecture-only advantage. Both remain relevant deployment candidates, but their training-data exposure must accompany the result.

## 4. Direct trajectory mathematics and stroke alignment

**MathNote** is the closest surveyed open implementation to the proposed editable recognition architecture: group/subset scoring, symbol classification, relations, graph/grammar decoding, incremental session caches, and pinned corrections. Its source is Apache-2.0 and bundles models. Its documented vocabulary training used about 7,800 samples from one writer. The existing local probe scored **0/12 complete expressions**. It is an architectural reference and retraining candidate, not the strongest available recognizer. [Implementation](https://github.com/YonatanNemtsov/mathnote-ocr), [vocabulary/training scope](https://github.com/YonatanNemtsov/mathnote-ocr/blob/main/docs/vocabulary.md).

**Seshat** is an older GPL-3.0 C++ grammar-based trajectory parser. It is useful as a structured baseline and source for understanding group/relation parsing. It has not been built or benchmarked in this research, so no speed or current accuracy is asserted. [Source](https://github.com/falvaro/seshat).

**MathWriting's trajectory Transformer/CTC** is compelling evidence for a learned stroke-to-LaTeX route: the updated paper reports **35M parameters, 5.49% token CER and 60% expression exact match**. Google releases dataset utilities and evaluation support, but I found no corresponding pretrained recognizer weights in that official release. Dataset terms are CC-BY-NC-SA-4.0; the example code has separate Apache terms. This is not a ready, freely reusable pretrained product component. [Updated paper](https://arxiv.org/html/2404.10690v2), [official release](https://github.com/google-research/google-research/tree/master/mathwriting).

A suitable long-term editable-math stack is a learned stroke/subset encoder, group scoring, glyph probabilities, spatial relation classifier, and grammar/AST decoder with pinned user corrections. I would prioritize **training stronger heads within an explicit alignment architecture** over adding more raster/VLM models once the initial recognizer is working. That is an implementation recommendation, not a discovered model with demonstrated MyScript parity.

Important boundaries:

- Tree-aware training does not guarantee that each output symbol maps to the original strokes.
- Attention heatmaps and CTC alignment are inferred evidence, not exact ownership.
- LaTeX can be parsed to an AST, but parsing an output string does not recover missing source alignment.
- OCR should transcribe an equation; a solver is a separate deterministic subsystem and must not rewrite the intended expression to make it mathematically plausible.

## 5. General OCR/VLM fallback models

These all run locally with available implementations, but **document parsing performance is not live digital-ink performance**. Use them for selected difficult regions or imported pages, subject to a separate handwriting evaluation.

| Model | Observed license and main BF16 weight artifact | Evidence and role |
|---|---|---|
| **PaddleOCR-VL-1.6** | Apache-2.0; 1.917 GB | Current released document OCR variant, includes text spotting and formula/table support. First mixed-content candidate to test. [Card](https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6) |
| **PaddleOCR-VL-1.5** | Apache-2.0; 1.917 GB | Retain as a documented control; GLM's report includes an in-house handwriting comparison against it. [Card](https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.5) |
| **GLM-OCR** | MIT model; Apache-2.0 SDK/layout components; 2.651 GB | Task-specific text/formula prompts and eight listed languages. Strong region-recognition comparator. [Card](https://huggingface.co/zai-org/GLM-OCR), [SDK](https://github.com/zai-org/GLM-OCR) |
| **LightOnOCR-2-1B** | Apache-2.0; 2.011 GB | End-to-end multilingual page transcription; prioritize if document import matters. [Card](https://huggingface.co/lightonai/LightOnOCR-2-1B) |
| **GOT-OCR2.0** | Apache-2.0; 1.432 GB | Smaller established broad OCR baseline; lower priority than newer document OCR candidates. [Card](https://huggingface.co/stepfun-ai/GOT-OCR2_0) |
| **DeepSeek-OCR-2** | Apache-2.0; 6.779 GB | Much larger artifact; limited reason to prioritize for a CPU-first handwriting app. [Card](https://huggingface.co/deepseek-ai/DeepSeek-OCR-2) |

GLM-OCR's technical report gives **87.0 versus PaddleOCR-VL-1.5's 87.4** on its in-house handwritten-text benchmark. This is useful evidence to compare both, but the dataset and metric do not establish IAM CER or real pen-note accuracy. Its 94.62 OmniDocBench result is likewise not “94.62% accurate handwriting.” [Technical report](https://arxiv.org/html/2603.10910).

Some published parameter counts and Hub tensor totals differ. GLM advertises 0.9B while the inspected safetensors metadata totals about 1.325B stored tensor elements; the original Uni-MuMER 3B card and serialized tensors also differ. Tied/duplicated weights or packaging can complicate totals. The practical packaging table above uses **actual artifact bytes**, not an assumption that every nominal model size predicts memory usage.

**InkSight** is not prioritized: it converts handwriting images into digital ink. Folio already captures trajectories, so that direction of conversion does not fill the primary recognition gap. [Official project](https://github.com/google-research/inksight).

## 6. Routing, decoding, and runtime choices

**Native ink routing:** start with user-selected Text or Math regions and geometric line grouping. Later train a compact stroke-graph classifier for text, math, drawing, and noise. I found no verified pretrained open classifier that can be assumed to handle Folio's mixed notes. Use spatial neighbors, timing, and geometry, and include drawings in training rather than forcing every region through OCR.

**Image layout:** PP-DocLayoutV3 has Apache-tagged released weights and is used by GLM's document pipeline. It is a candidate for imported pages, not evidence of accurate stroke-level routing on sparse lecture notes. [Layout model](https://huggingface.co/PaddlePaddle/PP-DocLayoutV3).

**Text decoding:** for CTC winners, compare greedy decoding with a compact local character n-gram beam decoder and user vocabulary. `pyctcdecode` is Apache-2.0 and supports KenLM plus hotwords; it is a useful Python reference, not an automatic Rust integration. Check the language model's training-text rights independently. Avoid a general LLM rewriting the recognized line without retaining and showing the alternatives. [Decoder](https://github.com/kensho-technologies/pyctcdecode).

**Initial runtime:** benchmark in a dedicated Python/PyTorch or Paddle worker, bounded and cancellable from the Rust application. Keep GPU and CPU inference outside the GPUI pen/rendering path. No model requires a cloud endpoint once dependencies and all model assets are installed locally.

**Production runtime:** export the winning compact model's numeric encoder/head to ONNX where feasible; retain decoding and source alignment as separately testable logic. TrOCR and formula seq2seq models require correct autoregressive state, cache, preprocessing, tokenizer, and stopping behavior. An encoder ONNX file alone is not a complete recognizer. Paddle models may use local Paddle inference directly before any export effort.

**VLM quantization:** llama.cpp offers local multimodal inference and GGUF/projector support. Verify conversion of the exact fine-tune, architecture support, projector, and prompt before choosing this route. A 4-bit model's theoretical tensor size does not establish RAM use, formula accuracy, or CPU latency. No quantized Uni-MuMER variant was validated in this turn. [Multimodal runtime documentation](https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md).

Offline acceptance must include cold startup with network disabled and locally pinned tokenizer, processor, font/rendering dependencies, and model files. Some upstream examples download demonstration images or models; those examples need local-path equivalents in the application.

## 7. What existing local experiments establish

This turn performed source/checkpoint research and metadata inspection. **It did not download new large models, train models, or run a new inference benchmark.** The earlier workspace probes give limited but relevant evidence:

| Historical local screen | Whole-expression result | Recorded runtime / memory | Interpretation |
|---|---:|---|---|
| MathNote, pinned old revision | 0/12 | Median about 0.173 s; peak about 345 MiB | Current tested heads cannot be assumed to generalize to this writer |
| Texo/FormulaNet, original raster rendering | 2/12 | Median about 0.095 s; peak about 243 MiB | Fast but accuracy inadequate on this small screen |
| Texo/FormulaNet, thinner rendering | 1/12 | Same general CPU inference setting | Rendering matters; this tweak did not solve recognition |
| Uni-MuMER Qwen3-VL-2B, CPU BF16 | 9/12 | Median 6.67 s; p95 11.00 s; reported RSS about 4.5 GiB | Better screened accuracy, unsuitable for the proposed subsecond default in that configuration |

The inputs, preprocessing, model settings, and model revision vary between these probes. The sample count is too small to claim a general accuracy ranking or a universal speed profile. Do not extrapolate Qwen3-VL's timing to Qwen3.5 or to a quantized/GPU runtime.

Three wrong Uni-MuMER outputs in that old screen carried sequence scores above 0.95. A model likelihood is not calibrated correctness probability. The earlier eight-line TrOCR test also shows a useful quality/latency tradeoff from tuning but is not representative model-selection evidence. [Audited historical evidence](artifacts/research/stroke-pipeline-2026-10-04/evidence-audit.json), [math screens](artifacts/research/myscript-alternatives/summary.json).

## 8. Evaluation that will decide the actual winners

### Initial candidate set

Keep the first round to:

1. Text: **HTR-ConvText IAM**, **TrOCR Small**, and **PyLaia IAM** as the compact floor.
2. Math: **PP-FormulaNet Plus-S**, **UniMERNet Tiny**, and **Uni-MuMER Qwen3.5-2B** as the slower accuracy reference.
3. Retain the already tested **Qwen3-VL-2B** and **FormulaNet** results as historical controls, then rerun on identical inputs if used in a fair ranking.
4. Add **GLM-OCR versus PaddleOCR-VL-1.6** only for multilingual/mixed-page fallback evaluation, rather than expanding the first specialist comparison unnecessarily.

### Corpus and metrics

Use a consented, licensed collection of real tablet strokes. For a useful first screen, target at least 200 text lines and 200 formulas across multiple writers; follow with a larger writer-held-out evaluation before claiming general quality. Keep calibration, tuning, and final tests separate. Include delayed dots and accents, rewritten symbols, crossed-out ink, matrices, fractions, radicals, integrals, long expressions, blanks, sketches, and text beside equations.

Render identical source ink for raster candidates, then allow model-specific normalization required by their training. Record both the common source render and the final model input. Keep direct-trajectory inputs from surviving original sample ranges, as described in the previous pipeline report.

Measure:

- **Text:** cased CER, WER, punctuation/numeric/identifier error rates, exact line match, and correction actions.
- **Math:** canonical LaTeX exact match, token CER, structure errors, render-equivalent expression rate/CDM, and invalid/truncated output.
- **Interaction:** edits needed to reach the intended answer, source-alignment correctness, stability after adding/removing strokes, and cancellation/stale-result behavior.
- **Resources:** cold load, first result, warm median/p95 by input length, RSS, disk size, thread count, quantization drift, and optional GPU use.
- **Rejection:** false text/formula output on blank or drawing input; calibrated thresholds and abstention coverage.

Use paired comparisons on the same examples and report uncertainty. Inspect each model's training sets before using public benchmarks as evidence of unseen-data generalization. An exact-match score on previously exposed expressions can otherwise be misleading.

### Decision rule

For the initial live recognizer, the earlier proposed **p95 targets of 300 ms for short text and 500 ms for short math** are engineering goals, not achieved results. Choose the lowest correction-cost model that fits measured target-machine latency and memory. If compact formula recognition misses the quality target, make the slower fallback explicit and preserve the user's ink while it runs.

Advance to direct-stroke training when line/crop recognition and correction storage are working and the evaluation identifies a real limitation that trajectories can resolve. The missing ingredient for MyScript-like behavior is not solely a better checkpoint: it is coordinated grouping, source alignment, stable revisions, grammar, alternatives, and correction handling.

## 9. Pinned artifacts and audit trail

| Recommended artifact | Observed revision | Weight artifact |
|---|---|---|
| `DAIR-Group/HTR-ConvText` | `8ed2ad5390d67f44828c528d9254cc72deeab423` | `checkpoints/iam.pth` |
| `microsoft/trocr-small-handwritten` | `b4648cfa171985a6745f37ddd637e98c0da958ac` | `pytorch_model.bin` |
| `Teklia/pylaia-iam` | `9c22c3e4ae7e8455a7fb7c6784a7c372c47270db` | `weights.ckpt` plus alphabet/model definition |
| `PaddlePaddle/PP-FormulaNet_plus-S` | `3d46f557e3a1752f4bf81202395af3b5ecfadfd2` | `inference.pdiparams` plus inference/config files |
| `wanderkid/unimernet_tiny` | `3f09ac4b1cd583be47ea20a7d7daef839473028a` | `unimernet_tiny.pth` plus model/tokenizer config |
| `phxember/Uni-MuMER-Qwen3.5-2B` | `40a6288292057f1c162b3b0eaccd362036dbd495` | `model.safetensors` plus processor/tokenizer |
| `phxember/Uni-MuMER-Qwen3-VL-2B` | `26d57451c9182b68c60aef78634bfea20c41a866` | `model.safetensors` plus processor/tokenizer |
| `PaddlePaddle/PaddleOCR-VL-1.6` | `c5630abae1d940eafe0697512a0325494b02ab42` | `model.safetensors` plus processor/tokenizer |
| `zai-org/GLM-OCR` | `2e85a62840ccac27daa451df36c736c4636b8628` | `model.safetensors` plus processor/tokenizer |

These are **observed public repository revisions**, not downloaded-weight hashes. The manifests retain file sizes, source URLs, cached-source hashes, and failed requests. GitHub's unauthenticated API hit its rate limit during the extended survey; direct primary README/license fetching was used where available. Some paper pages were retrieved through the Hugging Face papers workflow, with arXiv used as the authoritative paper source. Connector model/paper search returned an unavailable-tool error, so public Hub APIs were used instead.

Evidence lives in [the model-selection artifact directory](artifacts/research/model-selection-2026-10-04/). The collector only reads public metadata/text sources and does not execute downloaded model code. Checkpoint formats, export behavior, and inference quality remain to be tested before integration.
