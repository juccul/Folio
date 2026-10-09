# Offline math solving and guided explanations for Folio

Implemented 2026-10-05. The four delivery stages below now have working offline paths. The original 2026-10-04 research is retained after this implementation record; its proposed extensions are not claims of unlimited solver coverage.

## Implemented stages

| Stage | Available behavior | Scope |
|---|---|---|
| 1 — Selected problems | Native Solve sidebar; editable handwriting/text/image recognition; direct Equation input; exact/decimal arithmetic; simplification/factoring; linear/quadratic methods; rendered steps; hints/details; copy and atomic result/worked insertion | Captured sources and current variables are checked before accepting/inserting; reading preserves ink |
| 2 — Broader teaching | Linear systems with individual row operations; supported real inequalities with sign/interval reasoning; sum/product/power/chain derivatives; nested manual-integration rules and derivative checks | Some non-polynomial or advanced results have labeled summaries; incomplete solving/integration errors are explicit |
| 3 — Live calculations | Explicit page assignments/dependencies; linked results; ink-region recognition after settling; persisted links; explicit adaptive function graphs | Page scope; 32 live results per scan with fair rotation; explicit y=f(x), real samples; no implicit/3D graphs |
| 4 — Smart assistance | Student next-line checking (typed or OCR); image crops and PDF rectangles; reviewed word-problem translation | Deterministic supported templates; no extra local LLM is loaded and no general Photomath parity is claimed |

## Using it

Select a problem and choose **Solve**, or open the document menu's **Math solver**. Review the problem and choose **Solve problem**, or press **Ctrl+Enter**. **Solution**, **Graph** and **Check work** expose the relevant inputs. The operation dropdown (initially **Automatic**) sits beside the primary action; target variable, domain and angle convention are under **Settings**. Empty inputs offer a runnable **Try example**. Systems use semicolons or separate lines. Completing an expression with `=` evaluates it. `?` is an unknown.

The answer and complete steps share a scrolling area beneath the fixed problem editor. **Guide me** starts at step one, with pinned Previous/Next/Hint controls and one current step at a time. **Show all steps** restores the complete trace. **Copy LaTeX** and **Add answer** stay pinned. **Add options** (the adjacent chevron) offers **Answer and steps** and eligible **Live answer**/**Live graph** variants. Adding worked steps and its undo/redo/reopen preserve the original source. **Edit LaTeX** opens a separate focused editor; Apply returns to the answer and Cancel editing discards the draft. Invalid source remains editable with an inline error.

Define `a := 5` and `b := 3*a` with the operation dropdown's **Define variable**, choose **Preview variable**, then **Add variable**. Calculate `a+b=` and choose **Add options → Live answer** to keep the result updated. An explicit `:=` definition also works through the automatic Solve action. Change a definition through its equation editor to recompute downstream results. Definitions apply across the current page, not by spatial reading order. Duplicate names, cycles and unresolved dependencies produce errors. A live result tracks selected typed text only while that text is the calculation's actual expression; typing a different problem creates an independent calculation.

For handwriting, a live result watches a bounded region around its source and recognizes changed strokes after a 700 ms pen-idle delay. Manual definition/expression editing detaches that source region. Graph sampling runs on the CPU worker and inserts a vector graph; range and page variables are persisted.

Open **Check work**, type the next equation or select its handwriting and choose **Use handwritten step**, then **Check next step**. Equation comparisons check solution sets, including lost roots. Expression comparisons retain domain restrictions and can be conditional. Unsupported proofs/comparisons return unknown. No numerical spot checks are presented as proofs.

For a photographed problem, select a single Image and crop it with the existing tool. PDF region entry uses normalized left, top, width and height in [0,1]. These image paths keep the source intact and share the configured OCR worker. Word translations require review and **Solve this equation** before solving. Prose routes automatically to the interpreter; **Translate word problem** in the operation dropdown selects it explicitly.

## Engine and packaging

The Rust controller (`crates/app/src/math_solver.rs`) runs an independent bounded JSON-lines service. Python uses pinned SymPy 1.14.0 and mpmath 1.3.0 with a Folio-owned restricted parser, source spans and explicit domain tracking. There is no Torch import or new neural model in the solver. Internet sockets are disabled, requests are cached, cancellation kills obsolete work, computation defaults to 8 seconds, and process virtual memory is bounded to 768 MiB. The Rust response deadline is 15 seconds. LaTeX parsing and SVG-to-outline conversion run off the UI thread. Sidebar answers and steps use cached GPUI filled paths, including glyph holes and transforms; no PNG is generated for formulas. Graph previews retain their existing raster path and original SVG.

Install with `scripts/math-solver-requirements.txt` and `scripts/setup-math-solver.py`. The prepared workspace pack uses the existing CPU runtime and retains installed license notices. Recognition precision is selected by its own pack; solver installation does not select BF16/INT8. Math pack lookup: `FOLIO_MATH_CONFIG`, data directory, portable directory, then development fallback. Python runtime paths must be updated if moved.

Equation `math_link` metadata uses document format 3, with legacy fields defaulting to absent. Math links originally introduced database schema 4. The current writer upgrades every database transactionally to schema 6, which also supports reviewed handwriting and indexed search-row locators. Older readers reject the upgraded schema rather than dropping dependencies. Copy/paste and note duplication remap sources; a pasted calculation without its handwriting source detaches the watched region.

## Verification

Meaningful Python regressions cover exact arithmetic, source/domain retention, negative inequality signs, quadratic methods and domains, root verification, row operations, derivatives, manual/definite integration, variable graphs and cycles, inverse trig notation, checking lost solutions, reviewed word interpretations, discontinuities, restricted syntax and offline sockets. Controller regressions exercise asynchronous solving/rendering, stale/cancelled/off-page results, unchanged source data, atomic undo/redo/reopen, variable-driven updates, typed independent calculations, live pen-region updates and image-to-solver routing. Mock OCR in controller routing tests isolates integration from recognition accuracy. Native GUI verification and measured worker results are recorded under `artifacts/validation/math-solver`.

The installed worker smoke run used **58.1 MiB RSS/high-water memory**, with **297 ms** for the cold first request. First requests for the sampled arithmetic/algebra/system/inequality/calculus operations took **1.2–12.5 ms**, and `y=1/x` graph preparation took **39.7 ms**. Exact-repeat cache hits were approximately **0.03–0.14 ms**. These measurements exclude OCR, Rust formula rendering, document writes and UI presentation; they are not general complexity/latency bounds. A one-second compute limit interrupted a degree-31 polynomial, and the same process then answered `2+2=` correctly. Results: `artifacts/validation/math-solver/worker-check.json`.

The real CPU BF16 image smoke produced `2 x+3=1 1` from a synthetic printed `2x + 3 = 11`. Adjacent bare numbers are rejected as ambiguous instead of silently becoming a product. Correcting the review to the written equation returns `{4}`. This is integration evidence and an explicit recognition failure, not an image/handwriting accuracy score.

The completed verification run has **139 passing Rust workspace tests**, **18 math-engine regressions**, **12 recognition/preprocessing regressions**, formatting and strict workspace/all-targets Clippy. Native X11/AT-SPI checks pass solving, rendered guides, clipboard/field input, next-step checking, atomic worked insertion/undo/redo, variable definitions, live results, graphs and reviewed word problems. A full typed phrase containing drawing-shortcut letters is round-tripped through the native clipboard. Wheel scrolling and visual formula/graph output were inspected on a temporary Xvfb display. The existing OCR copy/replacement/LaTeX insertion native regression also passes. No user notes or physical input devices were used for these UI checks.

The subsequent solver UX redesign is recorded under `artifacts/validation/math-solver-ux`: **79 UI/controller Rust tests** and **18 math-engine tests**, strict Clippy, and native checks for progressive disclosure, pinned controls, keyboard submission, guided learning, direct selection reuse, reviewed percentages, compact sizing and graph-only range validation. Formula previews have fixed display bounds and sharper rasterization. Field bounds use a separate layout event; validation errors survive dialog movement and are announced through accessibility. The selection editing toolbar is hidden while solving, and exact integers no longer show redundant decimal approximations.

The LaTeX follow-up adds **Edit LaTeX**, a multiline source field, pinned Apply/Revert/Copy controls and native vector formulas. Source edits use the bounded background service without invoking Python. Generation, source and variable checks reject stale edits; invalid rendering retains the last valid report. Dirty/pending source disables result copy/insertion, while Copy LaTeX returns the draft. Applied edits remove verification claims, restrictions, teaching steps and assignment/live metadata. Static insertion preserves the edited source and original SVG, with atomic undo/redo. Re-solving restores the computed report. Validation is recorded under `artifacts/validation/math-solver-latex`: 85 focused Rust tests, strict Clippy, and native copying/editing/invalid-source/static insertion/undo/redo/re-solving/compact-layout checks alongside the existing solver workflows.

The subsequent interaction pass makes operation choices explicit, compacts the pinned problem area, groups Add options, distinguishes previewing/adding variables, provides runnable examples, and isolates editing from result actions. Guided learning has bounded bidirectional navigation, starts with an actual step, and anchors the current step while keeping navigation pinned. Shared disabled controls are visibly dimmed and cannot show press feedback. Busy OCR and answer updates have distinct accessibility announcements. Current verification is under `artifacts/validation/math-solver-intuitive`: 86 focused Rust tests and native solver/editor/guide/example/compact-layout regressions.

Graph previews and inserted graphs use the current page paper, grid, readable foreground, and appearance accent. Theme or custom-paper changes invalidate their display caches; SVG recoloring and rasterization run in bounded background jobs. This also supports previously saved graphs, preserves curve geometry and clipping, and leaves document SVGs and export colors unchanged. Palette and native theme-switch verification is under `artifacts/validation/math-graph-theme`.

## Remaining limits

There is no symbol-to-stroke alignment or per-subexpression handwriting highlighting, implicit/3D graphing, arbitrary word-problem reasoning, general proof checking, or universal teaching trace. Image/handwriting recognition still needs review. Domain restrictions, supported rules and explicit summary/unknown/error states define the current accuracy boundary. General local language-model explanations remain an optional future component; all installed stages work without one.

## Original research and proposed future extensions (2026-10-04)

## Recommended approach

Keep GLM-OCR as the handwriting recognizer. Add a separate CPU symbolic-math worker using SymPy, a parser that preserves the original mathematical structure and restrictions, and explicit rules that produce teaching steps. Use one Solve panel for calculator results and guided explanations. Start with selected handwriting and existing Equation objects, then add live calculations and graphs.

The calculation engine needs no additional neural model or GPU. A language model can be evaluated later for word problems or optional explanations, with a reviewed mathematical interpretation and checked calculations. Mathematical transformations should come from supported rules, with clear treatment of domains and branches.

## Product behavior to borrow

- **MyScript Calculator:** natural handwritten expressions, scientific operations, exact/decimal result formats, degree/radian selection, and solving a single missing value represented by `?`. Its published feature list does not establish a general algebra teaching engine. [Official product page](https://www.myscript.com/calculator/).
- **iPad Math Notes:** expressions completed with an equals sign, named variables reused in later expressions, and graphs. Variable declarations have a defined reading order. Folio needs an equally explicit scope and dependency policy. [Apple guide](https://support.apple.com/guide/ipad/solve-math-with-math-notes-ipadeb38d0f8/ipados).
- **Photomath:** editable recognition, a solution panel, alternative methods, and expandable explanations of individual steps. These are useful interaction patterns; the public pages do not disclose a reproducible implementation of its proprietary solver. [Official walkthrough](https://www.photomath.com/articles/photomath-101-get-math-with-photomath/).

## Proposed selection workflow

1. Select a handwritten problem or an existing rendered equation and choose **Solve**.
2. For handwriting, run the existing math OCR request. Show the recognized, rendered expression for correction. For an Equation object, read its stored LaTeX directly and skip OCR.
3. Parse the complete expression. Show ambiguous interpretations instead of choosing one silently. Distinguish an expression, equation, assignment, inequality, system, derivative or integral.
4. Offer the applicable operation: evaluate, simplify, factor, solve for a chosen variable, differentiate or integrate. Display real/complex domain and degree/radian settings where relevant.
5. Show an answer plus a **Steps** view in a side panel. Each step has rendered math, a short explanation, expandable detail and a highlighted changed subexpression.
6. Provide **Hint**, **Show next step**, **Show all steps**, **Copy result**, **Insert result**, and **Insert worked solution**. Reading solutions preserves the source ink; insertion uses one undoable document command.

Example for `2x + 3 = 11`:

| Expression | Explanation |
|---|---|
| `2x + 3 = 11` | Original equation |
| `2x = 8` | Subtract 3 from both sides |
| `x = 4` | Divide both sides by 2 |
| `2(4) + 3 = 11` | Check the answer in the original equation |

Checking a student's own next handwritten line can later reuse the same expression parser and transformation checks. Valid alternative methods must be accepted even when they differ from the displayed method.

## Backend choices

| Component | Recommendation | Practical limitation |
|---|---|---|
| Symbolic solving | SymPy | Choose APIs by problem class and represent incomplete/unevaluated results honestly. Ordinary answer APIs are not general teaching traces. |
| LaTeX parsing | Restricted grammar and preserved source tree; evaluate Lark as a parser starting point | SymPy's LaTeX parsing is experimental. Preserve structure and domain conditions before CAS normalization. |
| Algebra explanations | Folio-owned, versioned rewrite rules with explanation templates | This is substantive new work; a general solve call does not supply the desired sequence. |
| Calculus explanations | SymPy manual integration rules and ideas from SymPy Gamma's derivative/integral step code | Coverage and result shapes need adapter tests; unknown rules need an explicit fallback. |
| Unit-rich calculator, later | Consider libqalculate if units become a priority | A second engine adds packaging and normalization work. It is not a ready teaching-step backend. |

SymPy is BSD-licensed; retain its bundled notices. Its documented solving APIs support symbolic and numerical strategies, with limitations for general nonlinear equations. [License](https://github.com/sympy/sympy/blob/master/LICENSE), [solving guidance](https://docs.sympy.org/latest/guides/solving/solving-guidance.html).

SymPy's `integral_steps` returns nested rule objects for manual integration. SymPy Gamma provides derivative and integral explanation code worth studying or adapting; its BSD license permits reuse with notices. This does not supply a complete guided algebra solver. [Manual integration API](https://docs.sympy.org/latest/modules/integrals/integrals.html#sympy.integrals.manualintegrate.integral_steps), [derivative step source](https://github.com/sympy/sympy_gamma/blob/master/app/logic/diffsteps.py), [integral step source](https://github.com/sympy/sympy_gamma/blob/master/app/logic/intsteps.py), [Gamma license](https://github.com/sympy/sympy_gamma/blob/master/LICENSE).

Google's Apache-2.0 **mathsteps** demonstrates equation/expression transformations with before/after nodes, change types and nested substeps. It was archived on 2024-08-29, and its package pins mathjs 3.11.2. Study its rule organization rather than make its old Node runtime the foundation of this Rust/Python app. [Repository](https://github.com/google/mathsteps), [package manifest](https://github.com/google/mathsteps/blob/master/package.json), [license](https://github.com/google/mathsteps/blob/master/LICENSE).

Qalculate/libqalculate supplies exact/approximate calculations, variables, units and symbolic operations. It is GPL-2.0-or-later, compatible in principle with this GPL-3.0-or-later application subject to preserving applicable notices. Prefer one backend initially; investigate libqalculate if engineering units dominate requirements. [Project features](https://github.com/Qalculate/libqalculate), [license and releases](https://qalculate.github.io/index.html).

## Parsing and correctness

The representation should preserve both what the user wrote and what the CAS computes. Each mathematical node needs an identifier and source span; handwriting-to-symbol links require additional recognition work because current GLM output is a whole-selection string. Until symbol alignment exists, highlight the corresponding rendered formula, and retain the entire captured ink region as its source.

Use a restricted parser and explicitly construct allowed SymPy objects. Do not pass OCR text through Python `eval` or an unrestricted string-to-expression helper. SymPy documents that some string parsing helpers use `eval`, and that LaTeX parsing has backend-dependent failure/ambiguity behavior. [Parsing documentation](https://docs.sympy.org/latest/modules/parsing.html).

Track original denominators, square-root/logarithm domains, angle conventions, variable assumptions and branch conditions. For example:

- `x/x` simplifies to `1` only where the original expression is defined (`x != 0`).
- For real `x`, `sqrt(x^2)` is `abs(x)`.
- Dividing an equation by an expression requires a nonzero condition or a separate zero branch; it can otherwise discard solutions.
- Squaring can introduce extraneous roots, so check candidates against the original equation and restrictions.
- Numerical agreement at sample points is useful diagnostic evidence, not a proof of equivalence.

Each teaching step should record before/after trees, rule ID, explanation parameters, changed node IDs, assumptions, restrictions and substeps. Verification returns a distinct status for a supported checked transformation, a conditional transformation, or an unsupported/unknown one. A general CAS equivalence query can remain undecided; it is not a universal proof checker. Label incomplete results and numeric approximations accurately.

Preserve unevaluated structure during teaching. Otherwise automatic simplification can skip the fractions, distribution or cancellations that the learner needs to see.

## Integration with current code

- `crates/app/src/recognition.rs` already captures selected strokes, performs asynchronous recognition, shows a review and rejects stale source objects. Reuse those protections for Solve without invoking replacement.
- `crates/document/src/lib.rs` already stores Equation LaTeX and vector rendering. An existing Equation can go straight to the solver.
- Add a separate `app::math_solver` service with bounded JSON-lines requests, cancellation, time limits and process-level resource limits. Avoid making a difficult symbolic problem block the document-render worker or the OCR process. The service should import SymPy without Torch.
- Add controller solve-session state tied to note/page/source objects and request generation. Editing, deleting or moving the relevant source must invalidate the displayed result and prevent stale insertion.
- Add a GPUI Solve panel beside the canvas. Reuse `folio_math` to render formula steps. Cache previews and avoid rendering every expanded step on every frame.
- Initial result insertion can use existing Equation/Text objects and journal commands. Durable live annotations and variable dependencies need a separately reviewed document-schema extension.
- Cache computations by parsed expression, operation, target variable, domain, assumptions, variable bindings and solver/rule version. OCR caching needs its own source revision key.

A proposed request carries a structured expression, operation, target variable, assumptions and generation. A response carries exact/approximate answers, restrictions, method choices, steps, verification status and any unsupported reason. Python representations should not cross the protocol as executable strings.

## Live calculations and graphs

After explicit Solve is reliable, add an opt-in **Live math** mode for marked math regions. A completed equals sign can request evaluation after the pen lifts and the region settles. Coalesce edits, cache unchanged recognition, cancel obsolete work, and update only affected results. Running GLM on every pen frame would undermine writing responsiveness.

For named variables, use explicit assignment actions and a documented scope (start with a page). Maintain dependency edges so changing one assignment invalidates downstream calculations and graphs. Flag duplicate definitions, unresolved variables and cycles. An equation such as `a = 5` must not be silently treated as an assignment while the user intends to solve an equation.

Start graphs with explicit `y = f(x)`, sampled on a CPU worker and drawn as native vector curves. Adaptive sampling, discontinuity handling and finite-value checks prevent lines across poles. Keep graph sources and parameter bindings attached so edits trigger bounded resampling. Implicit and 3D graphs are separate later work.

## Delivery order and acceptance

1. **Selected-problem solving:** handwriting review, direct Equation solving, exact arithmetic, simplification, one-variable linear equations and quadratics, result/steps panel, copying and undoable insertion.
2. **Broader teaching coverage:** polynomial factoring, linear systems, supported inequalities, basic derivatives and manual integrals; multiple methods and hint progression. Differentiate antiderivatives to check supported results, with appropriate domain handling and constants of integration.
3. **Live notebook calculations:** page variables, dependency tracking, result annotations and explicit function graphs.
4. **Broader smart assistance:** check a student's next step, image/PDF problem crops, word-problem interpretation and optional local language-model explanations. Evaluate each separately; do not imply Photomath-wide coverage from the first algebra release.

Golden acceptance problems should cover correct answers and correct steps independently: exact fractions, negative signs, repeated/complex roots, absent/infinite solutions, restrictions, extraneous roots, ambiguous notation, degree/radian modes, supported and unsupported calculus, malformed OCR and compute timeouts. Add controller checks for cancellation, page changes, edits, undo/redo and saved results. Benchmark recognition, parsing, solving, step generation and rendering separately, including first-request startup and warm latency.

## Local feasibility evidence

`artifacts/research/math-solving/probe_solver.py` uses the already installed SymPy 1.14.0 and an isolated Lark 1.2.2 dependency directory. Internet sockets are blocked during the probe; it imports no Torch and loads no OCR model. It does not change the recognition packs or the running application.

Ten trusted-expression symbolic cases passed: exact fractions, linear/quadratic equations, real and complex roots, a rational equation, extraneous-root rejection, real square-root simplification, a chain-rule derivative and a linear system. A manually specified two-step linear trace preserved its solution set, and a nested integration rule produced an antiderivative whose derivative matched the original integrand.

The process peaked at **91.6 MiB RSS**, with **0.434 seconds** for the measured SymPy imports. Fifteen repetitions per simple symbolic case had median compute times between approximately **0 and 5.14 ms**; these timings exclude LaTeX parsing, OCR and UI rendering and do not represent difficult symbolic problems or application latency.

The parser probes confirm important limitations: `f(x)` produces alternative interpretations; `x -` is rejected by Lark; `x/x` becomes `1` and loses its domain restriction. The current OCR runtime's ANTLR 4.9.3 also cannot serve SymPy's ANTLR backend, which requests 4.11. A solver should pin its own parser dependencies and preserve original structure rather than treating the existing OCR runtime as a complete math-solving stack.

Results and timings are in `artifacts/research/math-solving/probe-results.json`. This is a small feasibility study, not an OCR-to-solver benchmark or an implemented teaching engine.
