# Accurate stroke-to-text and handwritten math for Folio

Research date: 2026-10-04. This is an implementation study, not a recognition implementation or a claim of parity with MyScript.

Terminology: online handwriting recognition means recognition from pen trajectories and writing events; it does not require an internet connection. Offline handwriting recognition traditionally means recognition from a static image. Either kind of model can execute locally without network access. In this report, fully offline operation means local execution with no runtime network dependency.

The user confirmed a firm requirement: fully offline, open-source Linux implementation. Commercial and cloud interfaces are studied only as reference behavior and explain why the proprietary SDK is excluded. No application features, settings, recognition models, dependencies, or user documents were changed.

## Decision

Build recognition around a durable relationship between editable ink and its meaning. Evaluate recognizers independently of that layer. The first deliverable should be a stroke replay/evaluation harness and source-linked recognition results for explicitly selected text or math.

MyScript's engine is proprietary and does not satisfy the confirmed requirement. Its publicly qualified native platforms also omit Linux. The documented on-demand C++ API does not make the engine open source or establish that a Linux package is available. Use the published SDK contracts and product behavior to inform an independent implementation.

For an open implementation, there is no verified package in this investigation that provides accurate multilingual text, broad handwritten math, automatic mixed-page interpretation, editable symbol alignment, low CPU latency and unrestricted redistribution together. A credible route combines compact trajectory models, optional image models, a spatial/semantic layer, persistent corrections, and multi-writer training/evaluation. Merely choosing a larger OCR model does not supply those editing capabilities.

The major engineering advantage is that Folio already retains raw pen samples and vector objects. The major unresolved risk is recognition quality on real, unfamiliar handwriting. That risk must be measured before recognition is restored as a default feature.

The concrete first experiments are OnlineHTR as an isolated trajectory-text baseline, TrOCR as the reproducible image-text comparator, Seshat as a native structured-math baseline, and separately measured image-math comparators. MathNote's source architecture is useful, but its tested weights are already a poor default candidate. Any research-only or ambiguously licensed checkpoint remains outside the distributable application. Long-term compact text/math training needs a data-use arrangement compatible with the intended open distribution; the presently available research datasets do not settle that issue.

## What MyScript publicly documents

### The products and SDK have different boundaries

MyScript Notes, formerly Nebo, offers handwriting conversion, pen editing, searchable handwritten content, a personal dictionary, reflowing documents and mathematical objects. MyScript Math adds live equation interpretation, variable-dependent calculations, graphs, scratch editing and LaTeX export. Its current product page also describes solving from photos. These are product capabilities, not evidence that every feature is exposed by the standard SDK. [MyScript Notes](https://www.myscript.com/notes/), [MyScript Math](https://www.myscript.com/math/).

The iink digital-ink recognition interface consumes stroke trajectories, including position/time and optional pressure. It can exploit pen boundaries and order that a bitmap lacks. MyScript's digital-ink-vs-OCR documentation describes that SDK interface as exclusively stroke-based. The Math app's photo feature therefore must not be used to infer that the same SDK endpoint accepts images. Its internal photo pipeline was not established here. [Digital ink versus OCR](https://developer.myscript.com/docs/interactive-ink/4.3/concepts/digital-ink-vs-ocr/).

### Their published algorithmic account

MyScript describes an evolution rather than a single disclosed current model:

- Text: normalization, competing segmentations, neural character hypotheses and statistical language context; modern sequence-to-sequence approaches.
- Math: joint segmentation, symbol interpretation and spatial grammar, with trees/graphs representing two-dimensional relationships.
- Mixed notes: graph neural networks using neighboring strokes to distinguish text from other marks.
- Mixed text/math: an encoder-decoder consuming coordinate sequences and producing characters/LaTeX.
- Training: diverse voluntarily contributed samples and large text corpora.

This account explains why geometry and context both matter. It does not disclose the exact architectures, parameter counts, training sets or model combinations shipping in either app today. In particular, it does not justify claiming that the current text engine is specifically an LSTM, or that the current math engine uses only a hand-written grammar. [MyScript's technical account](https://www.myscript.com/ai/).

### Recognition runs before conversion

Recognition interprets ink in the background. Conversion is a separate explicit operation that replaces it with typeset content. This distinction supports searchable ink and editable handwriting without requiring immediate visual replacement. [Conversion versus recognition](https://developer.myscript.com/doc/interactive-ink/4.3/android/fundamentals/conversion/).

Incremental processing accepts new strokes as writing continues and can revise earlier interpretations. Batch processing remains useful for already collected ink and indexing inactive pages. The implication for Folio is that a pen-up is a scheduling opportunity, not proof that a word or expression is finished. [Batch and incremental processing](https://developer.myscript.com/doc/interactive-ink/4.5/concepts/batch-mode/).

### There are several integration levels

| Interface | Documented purpose | Consequence for Folio |
| --- | --- | --- |
| Text/Math/Shape recognizers | Recognition of transient input; Math Recognizer returns LaTeX | Good first recognition benchmark; a LaTeX string alone does not provide editable symbol alignment |
| Raw Content Recognizer | Automatic block extraction/classification and recognition from unsegmented strokes; JIIX output | Useful mixed-page analysis; inspect exactly which geometry and source ranges it exports |
| OffscreenEditor | Persistent interactivity using the host application's rendering and stroke IDs | Closest architectural fit for GPUI, if available in the selected SDK/platform |
| Editor | Rendering-driven interactivity, conversion and content management | Deeper integration; may overlap Folio's established renderer/history |
| REST | Recognition of already collected stroke input | Convenient controlled batch comparator |
| WebSocket offscreen mode | Stateful incremental remote processing with client-side rendering | Commercial network path for a native client |

The 4.5 recognizer guide documents text/math/layout resource bundles and recommends compact ink ranges for mapping Raw Content recognition output back to input strokes. It also distinguishes classification settings from recognition settings. Requesting text/math recognition does not remove the need to configure the classifier appropriately. [Recognizer capabilities](https://developer.myscript.com/doc/interactive-ink/4.5/overview/recognizers/), [Native recognizer setup](https://developer.myscript.com/doc/interactive-ink/4.5/android/advanced/recognizers/).

The OffscreenEditor documentation also explains why undo history needs special handling: replaying strokes can change their order and recognition. It offers changesets and an optional history manager, and permits forcing selected ink into an intended content type. An integration should synchronize these semantics with Folio's history, rather than maintain two unrelated undo stacks. The detailed page inspected is version 4.1; current availability and signatures need verification against a supplied integration package. [OffscreenEditor](https://developer.myscript.com/doc/interactive-ink/4.1/android/advanced/off-screen-usage/), [Official integration samples](https://github.com/MyScript/interactive-ink-additional-examples-android).

### JIIX reveals useful output contracts

JIIX can expose text words/characters and alternatives, bounding boxes, item IDs, source stroke ranges and mathematical expression nodes. Alternatives depend on the chosen resources; for example, mul_Latn omits character candidates. Its coordinates use millimeters. Partial strokes can reference their full source IDs. Math blocks from an Editor-style model can contain expression structure; Math recognition exported through the standalone Math Recognizer is LaTeX, and Raw Content Recognizer math items also differ from Editor math nodes.

The important distinction is the contract of the selected interface. Do not assume that every JIIX-producing path exports a complete math tree, or that it provides calibrated probabilities: no confidence field was found in the inspected JIIX reference. [JIIX 4.5 reference](https://developer.myscript.com/doc/interactive-ink/4.5/reference/jiix/).

### The latest web capabilities matter

The developer portal identifies SDK 4.5. Its latest changelog describes full math interactivity in WebSocket offscreen mode, including variable management, solving and plot-ready evaluation. It also introduces a multi-Latin resource for supported Latin languages. Older documentation describing narrower web support should not be treated as the current limitation.

This makes the remote offscreen API a more relevant commercial candidate than an old text-only or math-only REST wrapper. It does not establish Linux native support or offline execution. [Current SDK portal](https://developer.myscript.com/), [Latest changelog](https://developer.myscript.com/docs/interactive-ink/latest/overview/changelog/).

## Folio: what is already useful and what must change

The current workspace is Folio 0.1.9, Rust/GPUI, GPL-3.0-or-later. Recognition was removed in 0.1.8. The earlier RECOGNITION_RESEARCH.md and validation artifacts are historical evidence; none of their inference adapters are current application features.

| Existing code | Verified behavior | Proposed use or required change |
| --- | --- | --- |
| crates/input/src/lib.rs | Position, pressure, tilt, timestamp and buttons; timestamp wrap extension | Preserve samples; add explicit recording/import session chronology for recognition |
| crates/ink/src/lib.rs | Raw samples separated from filtered/pressure-dependent display geometry | Recognize raw centerlines or surviving source ranges, not the decorative pen outline |
| crates/document/src/lib.rs | Stable object IDs, transforms, page revisions, legacy groups, source-linked equations/shapes | Add a new versioned semantic model with ranges, alternatives and durable corrections |
| crates/canvas/src/lib.rs | Document/screen transformations and incremental spatial index | Query affected neighborhoods and remove viewport transforms from recognition input |
| crates/app/src/workers.rs | Separate background job/result handling and bounded queues | Add an independent cancellable inference queue/process; current jobs are not recognition jobs |
| crates/storage/src/lib.rs | Incremental SQLite persistence and page headers | Persist semantic metadata/corrections without rewriting the whole page per recognition update |
| crates/math/src/lib.rs | LaTeX validation and pure Rust SVG rendering | Reuse final rendering; recognition, structural interpretation and solving remain separate additions |
| crates/search/src/lib.rs | SQLite FTS5 over derived page text | Add only current recognition text, with result-to-source highlighting and stale-result exclusion |

### Three concrete traps

**Segment erasure:** cut_segment clones the stroke's complete raw sample array into each surviving object and records only its visible fragment path. Sending each fragment's raw array to a recognizer would include erased geometry, possibly repeatedly. Before trajectory recognition, add a mapping from surviving geometry to original raw sample intervals, including interpolated cut endpoints. If that mapping cannot be reconstructed reliably for an old fragment, use its surviving centerline with explicit provenance that original timing is unavailable. Never pretend it is the unchanged sensor sequence.

**Global invalidation:** Command::apply clears page groups for every object change. Those groups are retired compatibility records, so this is consistent with the present app. A fresh semantic layer must invalidate affected content locally; modifying a remote image or recoloring a stroke should not erase all recognized words or user corrections.

**Source retention:** Equation.source_strokes and Page.hidden_sources already support keeping source ink behind a conversion. TextBlock has no corresponding source field. Text conversion needs equivalent provenance or an enclosing conversion record. Otherwise its undo/correction behavior would differ materially from math conversion.

These are code observations, not newly reproduced runtime failures. Relevant locations: input sample construction; InkStroke/display_path; cut_segment; InkGroup/Page; Command::apply; Equation/TextBlock; Workers::submit.

## Proposed architecture

~~~mermaid
flowchart TD
    A[Tablet samples and stroke identities] --> B[Durable ink and edit history]
    B --> C[Immediate GPUI rendering]
    B --> D[Recognition snapshot and surviving source ranges]
    D --> E[Region hypotheses and context]
    E --> F[Text recognizer]
    E --> G[Math recognizer]
    E --> H[Drawing or undecided ink]
    F --> I[Candidates and semantic source links]
    G --> I
    I --> J[Persistent user corrections]
    J --> K[Search and selection]
    J --> L[Explicit conversion]
    L --> M[Typed text or rendered equation]
    J --> N[Optional math evaluation]
~~~

The pen path must be independent of inference. Recognition consumes immutable snapshots, produces proposals, and cannot mutate a document directly. The controller validates each proposal against current content before installing it.

### 1. Capture, coordinates and source lineage

Keep original x/y/time/pressure and pen-down/up boundaries. Preserve taps and dots even when the display path is short. Reject invalid coordinates; do not remove small strokes merely because they look like noise.

Recognize in document coordinates after object transforms, independent of pan, zoom, theme and display scaling. Distinguish an object transform from a viewport transform. A moved word changes placement; a moved single symbol may change an expression's meaning.

Use model-specific normalization on a derived copy. Keep absolute or region-relative y-position and relative glyph size; normalizing each symbol to an identical square alone destroys useful case/script information. Arc-length resampling, Bézier encoding, point deltas and stroke tokens are alternatives to evaluate, not transformations to mix blindly into a checkpoint trained with different preprocessing.

Folio's hardware timestamps are not a durable global ordering across devices, imports or application restarts. Add a writing-session identity and explicit original sequence index. Keep drawing order, recognition order and document z-order separate. Copy/paste and undo must not fabricate original chronology.

Represent source lineage as a root stroke identity plus one or more surviving sample intervals. A semantic character can refer to part of a stroke; a cursive stroke can contribute to several characters; a multi-stroke symbol can refer to several strokes. Whole-stroke-only grouping is insufficient.

### 2. Extract regions without prematurely deciding characters

Start with explicit lasso/rectangle selection and a choice of Text or Math. That eliminates a difficult classification task from the first accuracy experiment.

Next add candidate line/expression neighborhoods based on spatial proximity, temporal information, baseline estimates and existing semantic links. Keep alternate assignments for delayed dots, crossbars and scripts. Expanding an affected region must follow plausible relationships, not just a fixed pixel margin: a long fraction bar or radical can connect distant ink.

Use whole-line or whole-word context for text. Avoid cutting cursive words into isolated geometric characters. For mathematical expressions, preserve the entire two-dimensional structure; text-line cropping can separate a numerator from its denominator.

For automatic mixed pages, use learned stroke/region classification and support an explicit override. A horizontal line could be a minus, fraction bar, underline or diagram edge. Context and a user's chosen content type should influence the interpretation.

### 3. Text recognition and decoding

Evaluate a compact trajectory encoder with CTC as the first online baseline. Train or load a model whose exact feature definition is known. Include coordinate/temporal differences, pen boundaries and enough vertical/scale context; test whether pressure adds accuracy before depending on it.

CTC avoids manually labeled character cuts. Its alignment is not automatically a precise editable character segmentation, especially with late diacritics and reordered strokes. Add and evaluate an alignment layer before presenting character-level selections.

Keep several decoding candidates. A proposed scoring function is:

~~~
score(candidate) =
    recognizer_score
  + alpha * language_context
  + beta * vocabulary_bias
  + gamma * user_correction_constraints
~~~

Tune the weights on held-out data. Retain an open-vocabulary route for names, abbreviations, programming terms and scientific notation. Dictionary correction should rank recognizer-supported alternatives; it should not silently replace arbitrary ink with a more grammatical sentence.

Expose language selection initially. Evaluate mixed English/Spanish or other requested languages explicitly; an English IAM checkpoint does not establish multilingual capability. Use accepted neighboring text as context where the recognizer supports it, while avoiding propagation of unconfirmed errors.

MyScript exposes custom lexicons and character restrictions; its custom-math-grammar documentation specifically refers to the legacy math bundle. Do not assume those controls work identically with math2. [Custom resource construction](https://developer.myscript.com/doc/interactive-ink/4.5/android/advanced/build-custom-resources/), [Custom web recognition](https://developer.myscript.com/doc/interactive-ink/4.4/web/advanced/custom-recognition/).

Google's independent digital-ink guide similarly identifies writing-area scale, preceding text and natural stroke order as accuracy factors. This is useful evidence for capturing context; it is not a Linux dependency recommendation. [ML Kit accuracy guidance](https://developers.google.com/ml-kit/vision/digital-ink-recognition/android).

### 4. Math recognition: preserve both glyph and structure uncertainty

Two approaches should be compared:

**Structured approach:** propose groups of strokes, classify candidate glyphs, predict spatial relations, then jointly search for coherent expression trees. Candidate relations include baseline continuation, superscript, subscript, numerator, denominator, radicand, limits, accents, fences and matrix cells.

**End-to-end approach:** encode trajectories, images or both and decode normalized LaTeX. This avoids separately labeling every symbol relation for the recognition objective. It still needs a distinct source-alignment strategy if individual symbols must remain editable.

Use an expression AST or presentation tree as an application contract when reliable structure is available. Store the source label and raw recognizer output too. Parsing LaTeX into a tree does not recover which pen samples wrote each node, and attention heatmaps are not a validated substitute for source alignment.

Joint reasoning is important. The same mark may be x, multiplication, or an uppercase letter; two horizontal strokes may be an equals sign; a minus-shaped stroke above a baseline may be an accent. Keep plausible segmentations and glyph candidates until structure can resolve them.

Preserve genuinely incomplete expressions while the user writes. A live recognizer may temporarily see an unmatched fence or a fraction without a denominator. Represent partial structure rather than forcing a completed expression or silently adding unseen ink.

Separate validation steps:

1. The recognizer proposes content.
2. Structural validation checks allowable relationships and preserves unresolved nodes.
3. LaTeX parsing/rendering checks whether Folio can display it.
4. The user accepts or corrects.
5. An optional solver evaluates the accepted mathematical meaning.

A successful render proves neither recognition accuracy nor mathematical validity. An equation can be perfectly renderable and transcribed incorrectly.

The SDK's recognized math vocabulary includes many scientific symbols and structural rules, whereas an arithmetic solver covers a narrower task. Product slogans about solving equations should not be used as formal coverage guarantees. [SDK math elements and rules](https://developer.myscript.com/doc/interactive-ink/4.5/overview/math-elements-and-rules/).

### 5. Durable semantic records and corrections

Use a new versioned SemanticRegion rather than silently reviving opaque legacy InkGroup records.

| Record | Essential proposed fields |
| --- | --- |
| Region identity | document/page/region IDs, content kind, reading-order links |
| Source | root stroke IDs, surviving sample intervals, geometry/source hash |
| Interpretation | text or LaTeX, optional math tree, word/symbol geometry |
| Alternatives | candidate labels/trees, raw scores, engine-specific score meaning |
| Alignment | mapping kind, source intervals, whether exact, inferred or unavailable |
| Provenance | engine/checkpoint hash, preprocessing/decoder/config versions, language |
| State | provisional/current/stale, accepted/rejected, user correction version |
| Constraints | corrected text span or pinned math subtree anchored to source lineage |

Do not conflate a model score with an acceptance probability. Calibration belongs to a specific model, preprocessing/decoder and target distribution.

Corrections should be explicit constraints. Correcting one word should survive adding ink elsewhere. Pinning a fraction's denominator should preserve it when extending the expression. If the source ink under a correction is deleted or substantially rewritten, mark the constraint as needing review rather than attaching it to unrelated content.

For a full-region edit from an unaligned image recognizer, initially pin the region as a whole. Offer symbol-level editing only after reliable source alignment exists. This produces a useful first version without promising capabilities the model does not supply.

### 6. Incremental scheduling and stale-result protection

Track region input hashes and local generations. A job token should identify the document, page, region, source hash, recognition configuration and correction version. A global page revision is a conservative fallback, but unrelated edits should not discard useful completed work indefinitely.

On an edit, identify affected source spans, spatial neighbors and semantic parents. Recompute that connected region; do not rerun the whole notebook. A full-word movement can update geometry without changing its transcription. A partial-symbol movement can change structure and needs re-evaluation.

Coalesce repeated changes: keep the newest pending snapshot per region. Cancel obsolete work and reject obsolete results after inference. Cancellation must cover the child process/request, not just hide the result. Recognition queue saturation must never block tablet dispatch.

For an initial CPU prototype, test a 150–300 ms pen-up debounce and a bounded active-region queue. These are tuning starting points, not measured MyScript timings. A batch model invoked after a debounce is still repeated batch recognition; true incremental computation needs reusable features/session state.

Noncausal encoders may revise the whole active line as new context arrives. Bound that working context and measure cost. Do not make a small bidirectional model causal merely to call it streaming without evaluating the accuracy loss.

### 7. Editing behavior that creates trust

Keep handwritten ink visible by default. Show a quiet recognition preview or alternatives on selection. Offer explicit Convert to Text/Equation and Copy as Text/LaTeX. Conversion is one undoable command and retains source ink and accepted interpretation.

Recognition updates should not add a separate undo step for every model hypothesis. User corrections, type overrides and conversions should participate in durable history. Undo/redo restores user intent and source lineage; it should not reorder strokes or rerun the model unnecessarily.

Keep existing geometric scratch/encircle tools, but arbitrate recognition gestures carefully. In math, circles, cross strokes and long bars can be content. Gestures should have configurable scope and use surrounding objects. A false erase during writing is more damaging than an uncertain transcription.

Add handwritten search only after current results can be invalidated reliably. Search hits should carry source-region references for highlighting. Keep stale guesses out of export and accessibility output, and distinguish accepted content from provisional search metadata.

Reflow is a later feature. It needs word/span baselines, spacing and source-preserving transforms. Moving connected cursive components independently can damage handwriting; atomic word/span movement is safer until finer alignment is validated.

### 8. Math evaluation and graphing are downstream features

Represent recognized notation separately from its computational meaning. Preserve the user's exact accepted expression alongside any simplified form and computed result.

If variables and live graphs are added, maintain dependencies between assignments and expressions. Specify variable scope, redefinition rules, angle units, unknown variables and cycles. Invalidate dependent results when a definition changes. Parsing alone must not silently decide whether xy is a single identifier or implicit multiplication.

A general computer algebra system, numeric evaluator, and graph renderer are separate candidates to evaluate later. A solver should accept only the supported subset of an accepted tree and give unsupported/domain errors explicitly. Step-by-step pedagogical explanations are another substantial feature, not a consequence of recognizing LaTeX.

## Recognition candidates and realistic deployment choices

### Text

| Candidate | What it supplies | Main limitation | Proposed role |
| --- | --- | --- | --- |
| OnlineHTR | PyTorch trajectory LSTM/CTC implementation and downloadable English checkpoint | MIT code; checkpoint redistribution not established; LM decoding unfinished | First isolated online baseline |
| TrOCR handwritten | MIT-tagged image-to-text checkpoint trained for handwritten lines | Raster input, no direct ink alignment; multilingual/scientific notes need validation | Reproduce the historical line-recognition comparator |
| Google ML Kit digital ink | On-device text/gesture recognition with downloadable language packs | Documented Android/iOS support; not an open Linux engine | Reference interaction behavior, or future mobile-port comparator |
| ScribeTokens / tokink | Digital-ink tokenization and research experiments | Representation research, not a complete multilingual editor recognizer | Optional tokenizer experiment after an ordinary vector baseline |

OnlineHTR uses dx/dy/dt/new-stroke features with its own resampling scheme. Its published code does not implement every part of Google's production system. Google's Bézier/LSTM research supports evaluating compact trajectory encodings, but is not a downloadable Linux SDK. [OnlineHTR](https://github.com/PellelNitram/OnlineHTR), [Google's online recognition paper](https://arxiv.org/abs/1902.10525), [Google's implementation account](https://www.research.google/blog/rnn-based-handwriting-recognition-in-gboard/).

TrOCR is a line recognizer, so whole-page input requires separate layout handling. Its model card declares MIT. A dataset's terms and a released checkpoint's license are separate questions; investigate checkpoint provenance before redistribution rather than inferring weight rights from code or data alone. [TrOCR checkpoint](https://huggingface.co/microsoft/trocr-base-handwritten).

ML Kit confirms offline recognition on Android/iOS and separate model downloads. There was no documented Linux interface in the inspected overview. [ML Kit digital ink](https://developers.google.com/ml-kit/vision/digital-ink-recognition).

ScribeTokens is a newer research direction using directional/pen-state tokens and BPE. Its recognition results do not establish MyScript-level accuracy or superiority across unrelated benchmarks. The experiment repository had no detected root license in this review; the separate tokink tokenizer declares MIT. Do not infer that tokink's license covers the entire research repository or future checkpoints. [ScribeTokens paper](https://arxiv.org/abs/2603.02805), [Experiment repository](https://github.com/douglasswng/scribe-tokens), [tokink](https://github.com/douglasswng/tokink).

### Math and mixed-content fallback

| Candidate | Actual input/structure | Deployment finding | Proposed role |
| --- | --- | --- | --- |
| MathNote OCR | Strokes; candidate symbol rasterization and learned spatial tree parser | Apache-2.0 repository; approximately 1.2M parameters; CPU sessions and correction pins | Strong architectural reference; current weights failed Folio's prior small screen |
| Seshat | Trajectories with grammar-based C++ math parsing | GPL-3.0; older Linux research system | Native structured baseline, not an assumed modern accuracy leader |
| MathWriting CTC approach | Trajectory-to-LaTeX model | Research architecture; no trained CTC weights found in the official release inspected | Compact training target |
| Uni-MuMER-Qwen3-VL-2B | Image-to-LaTeX | Model card declares Apache-2.0; previous Folio validation had better results but high memory/latency | Optional on-demand comparator/fallback |
| Texo/FormulaNet | Image-to-LaTeX; ONNX deployment | Approximately 20M parameters; AGPL-tagged weights | Small raster challenger; prior tested checkpoint performed poorly |
| GLM-OCR | Image text/formula/document extraction | MIT-tagged model card; no Folio measurements in this study | Additional raster challenger, not a validated handwriting replacement |

MathNote implements caching across stroke edits and pinning corrected subtrees. Its default architecture combines a symbol CNN, subset transformer and full-expression graph refinement. The author's vocabulary note identifies a major generalization risk: approximately 7,800 symbol samples from one writer. A planned broader vocabulary is not evidence of trained multi-writer weights. [MathNote OCR](https://github.com/YonatanNemtsov/mathnote-ocr), [Author's vocabulary/training note](https://raw.githubusercontent.com/YonatanNemtsov/mathnote-ocr/main/docs/vocabulary.md).

Seshat supplies a useful older stroke-based parser and output structure. Its public repository's age and evaluation history do not justify choosing it without building and testing it on current handwriting. [Seshat](https://github.com/falvaro/seshat).

The updated MathWriting paper reports a 35M-parameter CTC Transformer with 5.49% test LaTeX-token CER and about 60% exact expression match. These are different metrics; 5.49% error does not mean 94.51% of equations are correct. Its test comparison also reports different CER/exact-match tradeoffs for image/ink vision-language models. The paper supports compact trajectory recognition as a serious experiment; it does not deliver an interactive editor or source alignment. [MathWriting paper, revised version](https://arxiv.org/html/2404.10690v2).

The official MathWriting release contains data, formatting/tokenization/evaluation examples and a notebook, not a ready-to-install trained CTC recognizer found during this review. [Official MathWriting project](https://github.com/google-research/google-research/tree/master/mathwriting).

The image-based alternatives need the same selected expression and the same canonical ink render for a fair comparison. General document OCR performance is not a handwritten math benchmark. [Uni-MuMER checkpoint](https://huggingface.co/phxember/Uni-MuMER-Qwen3-VL-2B), [FormulaNet](https://huggingface.co/alephpi/FormulaNet), [GLM-OCR](https://huggingface.co/zai-org/GLM-OCR).

For the proposed canonical render, use black monoline surviving centerlines on white, exclude paper guides/highlighters/decorations, apply document object transforms, and retain small dots, descenders, scripts and fraction bars with adequate margins. Match each checkpoint's trained resize/padding convention. Evaluate line-height/stroke-width sensitivity rather than choosing settings from aesthetic screenshots. Keep text line crops and math expression crops separate.

### Why commercial interfaces remain references only

**Native MyScript:** the 4.5 qualified platform table lists Android, iOS, Windows and Web; Linux is absent. An on-demand C++ API is documented. Obtain evidence for Linux x86_64/arm64, target glibc, threading and OffscreenEditor/Raw Content/math2 before choosing an adapter. Use a narrow supported interface and keep Folio's ink format independent of SDK serialization. [Platforms](https://developer.myscript.com/doc/interactive-ink/4.5/overview/platforms/), [Integration levels](https://developer.myscript.com/doc/interactive-ink/4.1/overview/integration-levels/).

**MyScript cloud/on-premises:** the documented web protocols can be used by a native client. REST is useful for isolated experiments; offscreen WebSocket is the closer fit for live editable ink. On-premises hosting is offered by inquiry, but that is not proof that a server package can be bundled into a desktop app or used under ordinary SDK terms. [REST recognizer API](https://developer.myscript.com/doc/interactive-ink/4.5/web/rest/new-api/), [REST/WebSocket modes](https://developer.myscript.com/doc/interactive-ink/4.0/web/overview/http-rest-or-websocket/), [Platform deployment guidance](https://developer.myscript.com/doc/interactive-ink/4.5/overview/platforms/).

**Mathpix strokes:** Mathpix documents an endpoint accepting stroke coordinates directly and returning formatted text/math. It is a useful cloud comparator, not an offline engine or proof of MyScript-style persistent semantic editing. Credentials and actual calls were not used here. [Mathpix stroke API](https://docs.mathpix.com/reference/post-v3-strokes).

MyScript's published pricing describes per-device native licensing, first-use internet activation and a limited offline grace period before activation. Confirm an arrangement for genuinely offline installations; do not promise never-connected deployment from the standard native offering. Cloud pricing and commercial native deployment terms must be obtained from the current account/vendor rather than invented. [SDK pricing and activation](https://developer.myscript.com/pricing).

Folio's GPL distribution also matters. Proprietary linking is a separate distribution issue from technical support; a C++ wrapper or subprocess is not an automatic compatibility guarantee. Establish an appropriate arrangement for the actual combined distribution before shipping. This study does not determine that private experimentation or every possible integration is prohibited. [GNU explanation of GPL-incompatible libraries](https://www.gnu.org/licenses/gpl-faq.en.html#GPLIncompatibleLibs).

## What the existing measurements actually tell us

These are prior local experiments, reviewed during this investigation. No recognizer inference was rerun.

| Prior run | Whole expressions matched | Process memory | Recorded timing |
| --- | --- | --- | --- |
| MathNote default, strokes | 0/12 | 345.3 MiB peak RSS | 0.173 s median per expression |
| Texo transfer ONNX, original render | 2/12 | 243.3 MiB peak RSS | 0.095 s median per expression |
| Texo transfer ONNX, thinner render | 1/12 | 243.6 MiB peak RSS | 0.096 s median per expression |
| Uni-MuMER BF16 CPU | 9/12 | About 4.5 GiB resident snapshot | 85.23 s recorded batch; 6.67 s median per sample |

The thinner/original Texo runs are the same twelve inputs, not independent samples. Input types, preprocessing and decoding differ. Peak RSS and a resident snapshot are not identical memory measurements. Small-sample results are candidate rejection signals, not general accuracy rankings or evidence about the user's handwriting. All runs excluded the Folio UI; reported per-expression numbers should not be read as controlled cold-start latency.

The references and normalization retain symbol case and expression structure while tolerating selected formatting aliases. Exact expression matching still requires a documented normalization policy.

A new audit of the existing Uni-MuMER records found that the three incorrect expressions had raw scores of approximately 0.9623, 0.9513 and 0.9753. A threshold of 0.95 would have accepted all three. Those scores are model-specific conditional scores, not observed correctness probabilities.

The old text experiment reduced CER from 5.05% to 2.45% on eight IAM lines using improved rendering/context/beam decoding, while recorded batch time rose from 54.18 to 133.36 seconds. That is limited historical evidence that preprocessing/decoding can matter substantially; eight lines do not establish production text performance.

Primary local evidence:

- RECOGNITION_RESEARCH.md
- artifacts/research/myscript-alternatives/summary.json
- artifacts/research/myscript-alternatives/mathnote-probe.json
- artifacts/research/myscript-alternatives/texo-original-probe.json
- artifacts/research/myscript-alternatives/texo-probe.json
- artifacts/validation/navigation-recognition/math-metrics.json
- artifacts/validation/navigation-recognition/unimumer-math.json
- DEVELOPMENT.md, historical recognition section
- artifacts/research/stroke-pipeline-2026-10-04/evidence-audit.json

## Training and data strategy for an open implementation

Treat data as a major workstream. Symbol recognition trained on one person's writing and synthetic typeset expressions alone is unlikely to support unfamiliar handwritten lectures.

Start with separate text-line and math-expression tasks. For a compact math model, roughly 30–60M parameters is a reasonable initial experimental range motivated by published compact results, not a guaranteed optimum. Prefer a well-understood vector baseline before adding a new tokenization or dual image/trajectory encoder. Compare model capacity, feature representation and decoding independently.

For future model training, the intended example record should retain:

- Original strokes, their source/session identities and raw timestamps.
- Transcript or normalized mathematical label.
- Writer/session/device grouping for leakage-resistant splits.
- Text language and content kind.
- Optional glyph/word source ranges and mathematical relations.
- Data-use/redistribution rights and annotation provenance.
- Incremental edit sequences where available.

Use writer-disjoint validation and test sets, plus held-out devices and different tasks. Screen copied prompts separately from spontaneous lecture notes. Prevent equivalent synthetic templates, expression images or donor glyphs from leaking across train/test.

Augment point density, small jitter, slant, scale, aspect ratio, modest rotation and writing speed. Apply spatial perturbations to math without changing intended relations. Stroke-order augmentation must model plausible writing variation, especially late marks; arbitrary permutations are a poor substitute. Preserve genuinely small dots rather than augmenting them away.

Use synthetic composition to expand coverage, then evaluate on real multi-writer data. End-to-end LaTeX training supplies content labels, not editable stroke alignment. If alignment is a requirement, add explicit source/relation annotation or a separately validated alignment model.

Keep personal customization local. Start with a user vocabulary and persistent per-region corrections. Later evaluate writer adapters or fine-tuning on intentionally supplied samples. Selecting an alternative should not be described as online neural retraining unless that behavior has actually been implemented and measured.

MathWriting's dataset is CC-BY-NC-SA-4.0; its example code is Apache-2.0. IAM-OnDB is for non-commercial research. These are useful research sources but not unrestricted training-data grants for every intended distribution. A code license, data license and checkpoint license must be tracked separately. [MathWriting license statement](https://raw.githubusercontent.com/google-research/google-research/master/mathwriting/README.md), [IAM-OnDB terms](https://fki.tic.heia-fr.ch/databases/iam-on-line-handwriting-database).

## Evaluation protocol before choosing a default

Create one versioned replay corpus and one adapter protocol. Save every tested checkpoint hash, tokenizer/preprocessing version, decoding settings, runtime/precision, CPU/GPU and source-data policy. Use identical source ink and recognition boundaries where capabilities permit; separately test automatic page segmentation.

An initial useful screen is about 500 text lines and 500 equations across at least 20 writers, including some spontaneous notes, plus 100 mixed regions and at least 30 scripted edit sequences. These are proposed screening sizes, not sufficient certification or promised data availability. Final held-out evaluation must be independently collected/split.

| Capability | Measurements that answer the actual question |
| --- | --- |
| Text content | CER, WER, punctuation/case/diacritics, open-vocabulary error, per-language/per-writer breakdown |
| Math content | Normalized LaTeX-token CER, whole-expression exact match, separately judged presentation/semantic equivalence |
| Math structure | Relation accuracy, script/fraction/fence errors, matrix row/column/cell accuracy |
| Layout/routing | Text/math/drawing classification and region extraction errors, inline math boundaries |
| Editing alignment | Source-range precision/coverage and correct symbol/word hit targets |
| Incremental behavior | End-of-expression latency, hypothesis churn, late-mark repair, correction survival |
| Rejection | Error among accepted results versus acceptance coverage; calibration when available |
| Responsiveness | Warm/cold p50/p95/p99, pen dispatch/paint impact, queue depth and cancellation delay |
| Resource usage | Model load time, process/application peak RSS, idle RSS, restart/unload behavior |

A visually similar render can conceal semantic mistakes; a mathematically equivalent expression can fail literal matching. Report the different metrics separately. Do not convert a CDM/render similarity or token CER into whole-equation accuracy.

Keep two evaluation modes. A personal mode measures the intended user's notes and correction burden. A generalization mode holds out unfamiliar writers. Neither substitutes for the other.

Proposed early product targets are warm active-region p95 below 300 ms for ordinary short text, below 500 ms for supported short equations, and pen dispatch-to-paint p95 within 10% of the recognition-disabled baseline under matched load. A seconds-long engine can still be an explicit conversion fallback. These are acceptance targets to test, not current measured performance.

A target latency without an input-size envelope is meaningless. Record stroke/point/token counts, and define separate limits for a short line, ordinary expression, large matrix and full-page import.

Critical replay cases:

1. Dot an i/j after finishing the word; add accents after neighboring text.
2. Cross a t after writing the next word.
3. Add an exponent/subscript to a previously recognized variable.
4. Write numerator, denominator and bar in different plausible orders.
5. Extend a radical across an existing subexpression.
6. Distinguish x from multiplication and minus from fraction/overbar.
7. Write a matrix and then edit one cell or fence.
8. Mix prose with inline math and place a diagram beside it.
9. Recolor, zoom, pan or change paper without changing recognition.
10. Move/rotate a word as a unit; move only one math symbol.
11. Partially erase a stroke, then recognize and undo/redo.
12. Correct one subtree, continue writing elsewhere, save/reopen.
13. Switch notes/tabs while a recognition job is running.
14. Delete source ink before a delayed result arrives.
15. Copy/paste, duplicate and reload without losing source links.
16. Saturate the worker queue and kill/restart inference while writing.
17. Run offline with the selected runtime/resources already available.

## Implementation sequence and stopping criteria

| Stage | Concrete deliverable | Evidence required before advancing |
| --- | --- | --- |
| A. Replay and source correctness | Model-independent corpus loader, fragment/source mapping, canonical render, metrics | Erased ink stays excluded; transforms/order remain correct; corpus rights/provenance recorded |
| B. Recognition proposals | Explicit selected Text/Math recognition; isolated adapters; measured online/raster comparators | Accuracy/latency/memory results on held-out writers; no fabricated alignment |
| C. Semantic persistence | New region model, source hashes, alternatives, corrections, stale-job rejection | Unrelated edits preserve corrections; save/reopen/undo/copy remain coherent |
| D. Local background recognition | Region extraction, coalescing, optional caches, provisional previews | Late marks repair locally; queue load does not harm pen rendering |
| E. Mixed pages and interaction | Learned routing, inline math, source-aware selection/search and guarded gestures | Layout/routing/alignment evaluation passes separately from transcription |
| F. Optional math workspace | Accepted expression AST, variables, evaluator/graph dependencies | Domain/unsupported errors, cycles, stale computations and units tested |

Stages A and B should precede an automatic recognition UI. For the open route, inadequate held-out accuracy is a signal to improve data/model/decoding or narrow supported scope. It should not be hidden behind a confident UI or a spellchecker.

The semantic/persistence layer can be built while model evaluation proceeds, using deterministic fixture proposals. That separates software correctness from model quality. It does not justify shipping a poorly performing model.

Use a fresh folio-recognition component for future work, with inference isolated from the existing document/render worker. Keep a replaceable engine protocol, explicit capability flags and a bounded queue. Prototype Python only where an existing model requires it; evaluate a narrower native runtime once model operations/precision and deployment needs are known. Do not recreate the previous large always-resident bundle by default.

Load expensive fallback models on demand and release them after idle time. Profile quantization on the same handwriting corpus before adopting it. Lower weight precision reduces weight storage, but process RSS also includes activations, image encoders, caches and runtime allocations; quantization alone is not a promised total-memory reduction.

No schedule or training-compute estimate is credible until corpus availability and a first controlled baseline are established. The app plumbing is comparatively tractable; high-quality multilingual data and robust math structure are the largest research uncertainties.

## Questions that settle the open implementation

- Which text languages and scientific vocabulary must the first release support?
- Is explicit selection/conversion sufficient initially, or is automatic live mixed-page recognition mandatory?
- What CPU, memory and model-download/package-size envelope defines the target Linux devices?
- Which candidate checkpoints have explicit rights covering their actual redistribution, beyond repository code licenses?
- Which data can be used for generalizable training under terms compatible with the intended distribution?
- Can an engine produce reliable source alignment, or should the first interface remain at word/region level?
- Which matrix, accent, calculus and partial-expression cases are required for initial math support?
- How much correction is acceptable on spontaneous notes, measured separately from copied test prompts?

These are design/evaluation questions for the implementation stages, not blockers for this research. The current evidence supports a staged selected-region prototype with offline engines and source-preserving correction before automatic mixed-page recognition.

## Research limits

This study inspected official product/SDK material, primary research papers, author repositories/model cards, Folio's current source, and prior local measurements. Some versioned documentation routes returned errors; conclusions rely on pages whose relevant content was available, and older sources are labeled where material.

The public web demo's math interface was inspected: it exposes conversion and Math/LaTeX/MathML output. A synthetic pointer-event smoke attempt was not faithful enough to serve as an accuracy test and produced a page error; it is excluded from comparisons. The native mobile apps were not installed or instrumented. No proprietary binaries were reverse-engineered.

No new recognizer benchmark was run, no private ink was uploaded, and no contact/email/API credentials were used. Existing small probes are insufficient for a production accuracy claim. The proposed architecture, tuning values and acceptance targets are recommendations to validate.
