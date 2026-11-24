---
name: software-launch-video
description: End-to-end engineering, scripting, storyboarding, Remotion/code-rendered motion production, audio mastering, and quality verification for high-converting software product launch videos, SaaS explainers, feature reveals, and demos. Use whenever asked to design, write, animate, code, render, package, or audit a software launch video, teaser, homepage hero loop, Product Hunt video, YC or accelerator demo, developer tool reveal, or multi-ratio social launch kit.
---

# Software Launch Video

Produce software product launch videos, scripts, storyboards, code-rendered motion compositions (Remotion/React/SVG/HTML), and rendered MP4 kits that convert technical and business buyers. Every output proves real working software fast, communicates 100% on mute, avoids generic dark-mode template clones, and passes deterministic technical and visual QC before delivery.

## Agent Operating Principles

- **Inspect the product first:** When working inside a software repository, read the codebase (`README.md`, design tokens, CSS/Tailwind variables, typography, real UI components, routes, and screenshots) before designing a single frame. Pull authentic brand colors, fonts, copy, and domain data directly from the repo instead of inventing generic placeholder SaaS screens.
- **Self-contained execution:** All strategic frameworks, visual/motion specs, audio mastering recipes, safe-zone pixel margins, and verification scripts are embedded in this file.
- **Verify with scripts, not guesses:** Materialize `scripts/script_budget.py` and `scripts/verify_render.py` from the Verification Scripts section at the bottom of this skill if they do not already exist in the workspace, and run them on every spoken script and rendered video file before handing deliverables to the user.

---

## End-to-End Execution Pipeline

Execute these six stages in order. If the user requests only one stage (such as a script, Remotion build, or audit), still lock Stage 1 and run Stage 6 verification.

### Stage 1: Lock Positioning, Audience, and Single Workflow

Establish four inputs (extract from the repo/prompt first; ask only if genuinely ambiguous):

1. **Primary audience:** Developers, Enterprise buyers, PLG end-users, or Investors/Accelerators. Never optimize one 60-second cut for all four.
2. **Surface & funnel job:** Out-of-market awareness launch film (the 95%) vs. in-market product demo (the 5%) vs. silent homepage hero loop.
3. **One complete workflow:** Select one causal end-to-end loop to prove on screen:
   `Input (blank state or real trigger) -> Product Behavior (visible mechanism) -> Output (concrete artifact with realistic domain data)`.
   Never build a fragmented feature tour across settings and sidebars.
4. **Output artifact:** Strategy + Script + Storyboard, Remotion/React code project + rendered MP4 kit, Screen-Capture shot list, or Audit of an existing asset.

Write **Rule Zero** at the top of the plan before drafting scene 1:

- **Positioning sentence:** "This video helps [specific buyer] understand [specific product value] so they can [exact next action]."
- **Colleague-repeatable sentence:** The one sentence a viewer should repeat to a teammate after one muted watch.
- **Single visual proof:** The exact on-screen moment that proves that sentence without narration.

### Stage 2: Select Archetype, Runtime, and Narrative Arc

Use the Narrative & Audience Specifications below to choose format by the largest psychological barrier rather than aesthetic taste:

- **Default flagship:** Hybrid Cut (Founder + Simplified UI) or Simplified UI (SUI) Explainer (60-90s) for horizontal SaaS, APIs, infrastructure, and AI workflows.
- **Live Screen Capture (30-90s):** When raw live output is the proof (dev tools, CLI, AI code/media generators, App Store previews).
- **Founder-Led Camera Film (60-120s):** For trust-heavy categories (fintech, healthtech, legal) or YC application founder videos (strictly 60s, camera only, zero music/B-roll).
- **Pattern Breaker / Cultural Stunt:** Only when the brand can own the joke without repelling its target buyer.

Match runtime to job:

- **15s:** Paid social hook or pre-launch teaser.
- **30-45s:** Single-feature drop or tightly cropped UI teaser.
- **60-90s:** Flagship launch film or homepage hero (avoid >90s on landing pages).
- **2-4 min:** Deep product-page walkthrough or technical architecture demo (or 3+ min substantive founder demo on LinkedIn).

Select one narrative arc:

- **Problem -> Product -> Proof -> CTA** (default B2B SaaS flagship; skip macro market stats).
- **Immediate Show / Blank-State Reveal** (dev tools, AI, visual apps; lead with output or blank state on frame 1).
- **Before -> After -> Bridge** (workflow consolidation tools).
- **Beats, Not Cuts** (continuous founder shipping updates answering the viewer's next unspoken question).

### Stage 3: Engineer Script and Verify Word Budget

Structure every cut around the **Nose, Body, and Tail** retention curve:

1. **Nose (0.0-3.0s):** Trigger a visual or verbal pattern interrupt within 0.5-1.5s and lock buyer context by 3.0s. Cut all cold logo stings, greetings ("Hi everyone"), and company history.
2. **Body:** Deliver one clear visual progression every 3-4 seconds. Eliminate dead title cards and lingering static screens.
3. **Tail:** Delete wrap-up cues ("In summary", "That's about it", fading music resolves) that trigger exits before the CTA. End on a brief brand mark, one URL, and one action.

Reserve **17% of runtime** for visual holds and UI comprehension (`Max Words = (Total Seconds * 0.83 * WPM) / 60`). Validate every spoken script with `scripts/script_budget.py`:

```bash
python3 scripts/script_budget.py --duration 60 --pace launch script.txt
```

Fix any word-count overrun, long sentence (>22 words average), corporate filler, or wrap-up cue before moving to visual production.

### Stage 4: Build Visuals, Motion Physics, and Anti-Sameness Craft

Enforce the Visual Craft & Remotion Engineering rules below:

- **Mobile-first UI legibility:** Never shrink a dense desktop dashboard into a mobile player. Strip peripheral chrome, abstract background rows into muted placeholders, elevate one focal card, and use realistic domain data.
- **Motion discipline:** Hold wide ~1.0s before zooming (~1.6x); dim background layers rather than blurring them into mush; move an enlarged cursor along eased arced paths (20-30 frames at 30 fps) with an ~8-frame hover pause and 0.92x press scale; use snappy ease-out curves (`cubic-bezier(0.7, 0, 0.2, 1)`) and 0.1-0.2s reading-order staggers; animate hard-masked 2-5 word kinetic headlines with >=0.3s per word reading time and strictly <3 flashes/sec. Never use a 50/50 split screen between face and UI on mobile.
- **Code-rendered video (Remotion / web motion):** Externalize all copy, metrics, and locales into a single typed config file; use deterministic frame math (`useCurrentFrame`, `spring`, `interpolate`), never CSS transitions or `setTimeout`; isolate volatile product screens into swappable 3-5s scene components; define parameterized compositions for `16:9` (`1920x1080`), `9:16` (`1080x1920`), and `1:1` (`1080x1080`) with a dev toggle for mobile safe-zone overlays.
- **Escape template sameness:** Reject generic dark-mode floating-UI clones with swelling synths. Differentiate through real brand world-building, self-referential product proof (showing controls alongside live output), dramatized human workflow pain, or calm technical restraint.

### Stage 5: Engineer Silent-First Audio, Captions, and Credibility

Enforce the Audio Engineering, Credibility, and Safe-Zone specs below:

- **100% Mute Comprehension:** Ensure kinetic headlines or burned-in captions carry the full story on mute and sit strictly inside the universal `1080x1920` safe core (`top >= 250px`, `bottom >= 560px`, `left >= 60px`, `right >= 180px`, ideally inside `Y = 260..1280px`).
- **VO vs. No-VO:** Use No-VO (music + crisp UI foley + kinetic type) only when UI motion is visually self-explanatory and primary distribution is muted social feeds. Use calm, conversational human VO for multi-step B2B workflows, infrastructure, and YouTube.
- **Audio mastering:** Sync tactile UI clicks to exact contact frames (lead transition whooshes by 10-20 ms); duck music ~6 LU (18-20 dB) beneath VO; normalize masters via FFmpeg to **-14 LUFS / -1.0 dBTP** (YouTube/Reels/TikTok) or **-16 LUFS / -1.5 dBTP** (LinkedIn).
- **Radical honesty & licensing:** Explicitly label any speed-up, trimmed latency, sample dataset, or pre-GA prototype on screen (or mark `1.0x` real time on frame 1). Keep generative AI behind the glass (captions, dubbing, storyboards, Remotion code) and keep founders, customer quotes, and live software proof genuinely human. Verify paid-ad sync clearance and exclude non-commercial AI audio (never use free-tier ElevenLabs/Suno or Udio).

### Stage 6: Run Automated Verification and 10-Point Pre-Flight Audit

When producing rendered video files, run `scripts/verify_render.py` to audit codec, dimensions, loudness, file size, and extract key frames (with 9:16 safe-zone overlays when vertical):

```bash
python3 scripts/verify_render.py output/launch-16x9.mp4 --target youtube --extract-frames qc_frames/16x9
python3 scripts/verify_render.py output/launch-9x16.mp4 --target vertical --extract-frames qc_frames/9x16
python3 scripts/verify_render.py output/hero-loop.mp4 --target hero --extract-frames qc_frames/hero
```

If you have vision capabilities, open the extracted `0.5s`, `1.5s`, `3.0s`, midpoint, and safe-zone overlay PNGs and confirm zero text clipping, zero illegible micro-UI, and an unmistakable frame-1-to-3s hook.

Pass all 10 gates before finalizing:

1. **One-Sentence Test:** Outsider can repeat what the product does and who it serves after one muted watch.
2. **3-Second Mobile Hook:** Product, tension, or concrete proof appears inside 3s with zero logo sting.
3. **Full Mute & Safe-Zone Pass:** Captions/kinetic type legible at arm's length and clear of all 9:16 UI chrome.
4. **Single-Workflow Real Data:** One complete `Input -> Mechanism -> Output` loop using realistic domain data.
5. **Word-Count Budget:** `script_budget.py` passes with zero filler or wrap-up tail clichés.
6. **Skepticism Audit:** Every speed-up, mock dataset, or prototype explicitly labeled (or verified `1.0x`).
7. **Audio & Licensing:** Loudness targets verified by `verify_render.py`, 6 LU ducking, UI foley, commercial paid-ad licenses cleared.
8. **Anti-Sameness Check:** Carries unmistakable product/brand DNA rather than a generic SaaS template.
9. **Multi-Ratio & Web Vitals Kit:** Native `16:9`, `9:16`, `1:1`/`4:5`, `15s` teaser, and `<2 MB` silent hero loop (`<50 KB` poster, no raw YouTube iframe, primary CTA above fold).
10. **First-Hour Distribution Plan:** Founder-account native uploads, X external links in replies, UTM paths, and live 2-hour comment response documented.

## Gotchas

- **Do not build a feature tour:** Six screens in 60 seconds means zero retention. Cut to one complete workflow.
- **Do not open with a logo sting or company bio:** Cold logos lose 40% to 60% of social viewers before the product appears.
- **Do not hide latency or stage unshipped features:** Frame-by-frame technical teardowns cause durable trust damage.
- **Do not use visible AI avatars or synthetic testimonials:** Audiences penalize detectable AI on human trust surfaces and fake testimonials violate advertising disclosure rules.
- **Do not use CSS transitions or wall-clock timers in Remotion:** Always drive animation from `useCurrentFrame()` so headless renders never drop or desync frames.
- **Do not drop raw YouTube iframes or sound-on autoplay onto landing pages:** Both damage Core Web Vitals and depress conversion.

---

## Reference 1: Narrative Architecture, Retention Physics, and Audience Calibration

### The 95-5 Strategic Split

Never force one video to serve out-of-market discovery and deep in-market technical evaluation simultaneously:

- **Launch Film (targets the ~95% out-of-market audience):** Answers "Why does this category matter and why should I remember this product?" Optimizes for recall, emotional framing, and shareability across social feeds and launch day.
- **Product Demo (targets the ~5% in-market evaluators):** Answers "How does this actually work in my workflow?" Lives on product pages, changelogs, docs, and sales follow-ups.
- **Interactive Click-Through Tour (mid/late funnel):** Pair the 60-90s top-of-funnel launch video with a 3-5 chapter interactive tour (30-45s per chapter) on pricing and evaluation pages so buyers self-qualify specific workflows without waiting for sales.

Capture both the launch film and clean modular demo footage from one shared production plan whenever possible.

### Five Software Launch Video Archetypes

| Archetype | Barrier Solved | Typical Runtime | Best Software Fit | Core Trade-Off |
| :--- | :--- | :--- | :--- | :--- |
| **Simplified UI (SUI) / Animated Explainer** | Understanding ("How does it work?") | 45-75s | APIs, infra, data pipelines, AI agents, dense horizontal SaaS | Highest mobile clarity and longest shelf life; risks generic template look without strong brand DNA |
| **Live Product Demo / Screen Capture** | Clarity ("Does it actually work?") | 30-90s (2-4m deep walkthrough) | Developer tools, CLI, AI generators, visual apps, App Store previews | Maximum credibility and lowest cost; breaks when UI changes and fails on mobile without tight zooms |
| **Founder-Led Camera Film** | Credibility ("Can I trust this team?") | 60-120s | Trust-heavy domains (fintech, healthtech, legal AI) or major pivots | Fastest trust builder if candid; drops rapidly if scripted, stiff, or slow to open |
| **Hybrid Cut (Founder + Animated UI)** | Credibility + Understanding | 60-90s | Default Seed-Series B public launch on X and LinkedIn | Combines human trust with UI clarity; demands tight pacing between camera and UI |
| **Pattern Breaker / Cultural Stunt** | Attention ("Why stop scrolling?") | 60-120s | Crowded horizontal categories competing for organic reach | Highest reach ceiling; the stunt defines brand perception and can inflate low-retention signups |

### Empirical Format Retention Reality (Embedded Business Websites)

- Direct human formats (Talking Head ~37.5% completion, Customer Testimonials) and Product Demos outperform cinematic Founder/Brand Stories and Live-Action B-Roll with voiceover (~13.4% completion).
- Brand stories lose ~32% of viewers in the first 10% and half by 27 seconds because of slow atmospheric intros and logo stings.
- Testimonials lose ~41% in the first 10% while visitors check peer fit, then retain strongly across the remaining 90% once relevance is confirmed.

### Retention Physics and the Engagement-Conversion Paradox

- **Sub-2-second scroll filter:** Mobile viewers decide whether to fixate within ~1.3s. Split the opening into a **0.5-1.5s visual/verbal pattern interrupt** plus a **1.5-3.0s context lock** naming who this is for.
- **Logarithmic completion decay:** Every time an embedded website video doubles in length, completion drops by roughly 4.5-4.7 percentage points.
- **The Paradox:** Shorter videos maximize total completion (~50% under 1 min), whereas longer videos filter for higher-intent buyers whose conversion rate among surviving viewers climbs sharply. Never optimize solely for completion rate on high-intent product demos, and never run >90s on a cold homepage fold.

#### Runtime Selection Table

| Runtime Band | Primary Launch Job | Notes |
| :--- | :--- | :--- |
| **5-15 sec** | Paid social ads & curiosity teasers | Pure scroll-stop + single visual hook |
| **30-45 sec** | Single-feature drop, fundraise clip, UI teaser | Tightly cropped real software intrigue |
| **60-90 sec** | Flagship launch hero & homepage fold | Completes one workflow before landing-page completion falls below 30% past 90s |
| **2-4 min** | Product-page walkthrough & complex dev releases | Balances ~46% engagement with rising buyer intent; include YouTube chapters |
| **3+ min (LinkedIn exception)** | Substantive founder deep dives on personal LinkedIn profiles | LinkedIn dwell-time weighting rewards substantive 3+ min founder walkthroughs over thin 30-60s clips |

### Narrative Frameworks

| Framework | Beat Allocation (60-90s Cut) | Best Fit | Pitfall to Avoid |
| :--- | :--- | :--- | :--- |
| **Immediate Show / Blank-State Reveal** | Frame-1 output or sharp claim (0-5s) -> Live workflow from blank state (5-50s) -> Proof + CTA (50-60s) | Dev tools, AI apps, visual tools, solution-aware buyers | Moving so fast non-experts miss what input drove the output |
| **Problem -> Product -> Proof -> CTA** | Felt daily pain (5-10s) -> 2-3 core UI beats (30-50s) -> One concrete proof point (5-15s) -> Single CTA + URL (3-5s) | Default B2B SaaS flagship and homepage hero | Opening with macro industry statistics instead of concrete workflow pain |
| **Before -> After -> Bridge** | Chaotic Before (15-20s) -> Concrete After artifact (15-20s) -> Product as the Bridge (25-30s) + CTA | Consolidation tools, ops platforms, executive buyers | Making the "After" abstract instead of grounded in real UI output |
| **Problem -> Agitation -> Solution (PAS)** | Specific pain (10-15s) -> Quantified cost (15-20s) -> Mechanism + proof (25-35s) + CTA | Paid direct-response ads and problem-aware B2B ops | Spending >30% on scripted drama before showing software |
| **Beats, Not Cuts** | 10-20s candid setup -> 40-50s continuous demo where each beat answers the viewer's next unspoken question | Weekly founder shipping updates and prosumer demos | Using frantic jump cuts to mask a flat workflow |

### Script Word-Count Pacing Table

Formula with 17% visual-hold reserve: `Words = (Total Seconds * 0.83 * WPM) / 60`.

| Total Runtime | Visual Reserve (~17%) | Speech Window | Calm B2B (135 WPM) | Crisp Launch (150 WPM) | Fast Social (165 WPM) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 15 sec | 2.6 sec | 12.4 sec | 28 words | 31 words | 34 words |
| 30 sec | 5.1 sec | 24.9 sec | 56 words | 62 words | 68 words |
| 45 sec | 7.7 sec | 37.4 sec | 84 words | 93 words | 103 words |
| 60 sec | 10.2 sec | 49.8 sec | 112 words (~90 UI-led) | 124 words | 137 words |
| 90 sec | 15.3 sec | 74.7 sec | 168 words | 187 words | 205 words |
| 120 sec | 20.4 sec | 99.6 sec | 224 words | 249 words | 274 words |

Spoken-copy rules: 15-20 words per sentence, active voice, present tense, contractions, direct second-person "you". Eliminate filler adjectives ("seamless", "revolutionary", "empowering", "AI-powered workflow").

### Audience Calibration Matrix

| Audience | What Builds Trust | Instant Rejection Triggers | Execution Rules |
| :--- | :--- | :--- | :--- |
| **Developers / Technical Builders** | Real code, CLI output, API payloads, architecture diagrams, engineers presenting their own work, honest scope limits | Buzzwords, marketing hype, hidden latency, pop-ups, LLM-written Show HN posts | Open with real code or artifact; add YouTube chapter timestamps; hand-write all community posts |
| **Enterprise Buying Committees** | Unscripted peer customer proof, clear security/governance posture, concrete ROI workflow, forwardable clips | Consumer stunts that weaken executive credibility, vague vision reels without workflow mechanics | Pair 60s executive overview with role-specific demos or interactive tours for internal champions |
| **PLG / Prosumer End-Users** | Immediate visual gratification, dramatic time-to-value collapse on a familiar tedious task | Slow setup, abstract positioning, friction between watching and trying | Show workflow transformation within seconds and link directly to low-friction self-serve signup |
| **YC Application Founder Video** | Candid, clear founder communication and genuine co-founder chemistry | Screen recordings, B-roll, background music, logo stings, reading a script, >60s runtime | Strict <=60s; equity-holding founders only speaking to camera from bullet points on unlisted YouTube (put demo in separate demo slot) |
| **a16z speedrun / Demo-First Investors** | Shipping velocity, runnable live software, one unfair insight, weekly growth slope | Pitch decks without working software, slow theatrical intros | Lead immediately with a 60-90s live screen capture + clickable demo URL |

---

## Reference 2: Visual Craft, Simplified UI, and Remotion Engineering

### Rendering Mode Decision Matrix

| Mode | Mechanism | Best Fit | Trade-Off |
| :--- | :--- | :--- | :--- |
| **Polished Screen Capture** | Live app recording (or deterministic Playwright capture) with smooth zooms and clean data | Dev tools, AI generators where raw output is proof, App Store previews | Highest trust and lowest cost; breaks on UI changes and fails on mobile without tight zooms |
| **Simplified UI (SUI)** | Rebuild only narrative-critical UI as clean layered vectors/HTML | Dense dashboards, APIs, backend infra, horizontal SaaS hero films | Maximum mobile legibility and long shelf life; risks template sameness without brand DNA |
| **Code-Rendered UI (Remotion)** | Frame-by-frame React/CSS rendering driven by typed config data | Engineering-led teams, multilingual rollouts, frequent pricing/changelog updates | Zero dropped frames, instant data/locale re-renders, Git-versioned; still requires deliberate art direction |

### Simplified UI (SUI) 4-Step Construction Blueprint

1. **Strip peripheral chrome:** Remove browser bars, OS docks, secondary sidebars, notification badges, and unused toolbar icons.
2. **Abstract background density:** Replace non-focal table rows or surrounding cards with muted skeleton placeholders receding behind the focal surface.
3. **Elevate one focal card:** Isolate the active widget, editor, or dialog with generous whitespace, crisp contrast, soft depth shadows, and an enlarged primary action button.
4. **Use realistic domain data:** Pull authentic names, schemas, code snippets, or metrics from the product's actual domain. Never ship "Lorem ipsum" or "Test 123".

### The 9 Mechanical Rules of High-Craft Software Motion

1. **Zoom with orientation holds:** Hold wide for ~1.0s (30 frames at 30 fps) so viewers orient spatially, zoom smoothly (~1.6x) onto a crisp vector/high-DPI region, hold long enough to read, then ease out. Never zoom into blurry raster captures or zoom on every beat.
2. **Focus pull and depth dimming:** When context matters on dense screens, dim background opacity (e.g., 0.35-0.45) or apply very light blur while scaling the focal card slightly up (~1.03-1.06x).
3. **Restrained parallax layers:** Separate hero reveals into 3-4 Z-planes (app window, active panel, floating result card, cursor) with gentle tilt and soft shadows. Never warp typography with steep 3D perspective.
4. **Humanized cursor choreography:** Never move a cursor in a straight constant-speed line. Animate a slightly enlarged cursor (1.2-1.4x) along a gentle quadratic Bezier arc over 20-30 frames (at 30 fps), pause ~8 frames over the target, trigger a press scale-down (`0.92x` for 4-6 frames), and fire the UI state change on release. Omit the cursor when depicting autonomous background agents.
5. **Product-native motion physics and staggering:** Stagger UI reveals top-to-bottom and left-to-right at 0.1-0.2s (3-6 frames) offsets. Use crisp ease-out/snap curves such as `cubic-bezier(0.7, 0, 0.2, 1)` or critically damped springs (`damping: 200`, `stiffness: 180` for clean UI; `damping: 18`, `stiffness: 220` for snappy tactile buttons). Reserve linear easing solely for progress bars or continuous mechanical line draws. Pace roughly one primary UI action every 3-4 seconds.
6. **Hard-masked kinetic typography:** Slide concise 2-5 word headlines out from behind `overflow: hidden` hard masks in the brand typeface rather than slow opacity fades. Enforce **>=0.3s per word** reading floor (allow two comfortable reads on key statements) and stay strictly **<3 flashes per second** (WCAG seizure safety).
7. **Continuous camera drifts and match cuts:** Replace abrupt jump cuts and generic slide wipes with subtle continuous camera drift and match cuts where a clicked button or card morphs directly into the next view. Never use a 50/50 split screen between presenter face and UI on mobile; use a compact corner inset (~15-20% frame width) when UI legibility matters.
8. **Focused data proof and Bento hierarchy:** Animate one chart series at a time and land on one clearly labeled metric (place headline proof early). When summarizing capabilities near the close, use an asymmetrical **Bento Grid** of 6-9 tiles with one idea per tile and tile area communicating importance.
9. **End-loaded logo reveal:** Keep the logo off the opening 3 seconds unless the brand already has mass recognition. Place a crisp, brief logo lockup at the end followed directly by the URL and single CTA.

### Production Remotion Architecture Pattern

When building a launch video in Remotion (React), organize the codebase for deterministic rendering, instant copy/locale updates, and multi-ratio exports:

```text
video/
├── src/
│   ├── Root.tsx                  # Registers 16:9, 9:16, 1:1, and 15s teaser compositions
│   ├── config/
│   │   ├── brand.ts              # Colors, fonts, radii, shadows extracted from product repo
│   │   └── content.ts            # Externalized script beats, headlines, metrics, CTA, locale strings
│   ├── components/
│   │   ├── MaskedHeadline.tsx    # Hard-masked kinetic typography (overflow: hidden)
│   │   ├── HumanCursor.tsx       # Quadratic Bezier arc + 8-frame hover pause + click press
│   │   ├── SafeZoneOverlay.tsx   # Dev-only 9:16 obstruction guide (off in prod renders)
│   │   └── BentoSummary.tsx      # 6-9 tile asymmetrical closing summary
│   ├── scenes/
│   │   ├── Scene01Hook.tsx       # 0-3s pattern interrupt + context lock
│   │   ├── Scene02Input.tsx      # Blank state or real trigger (3-5s swappable module)
│   │   ├── Scene03Mechanism.tsx  # Core product transformation
│   │   ├── Scene04Output.tsx     # Real domain artifact + proof metric
│   │   └── Scene05EndCard.tsx    # Brief logo lockup + URL + single CTA
│   └── LaunchVideo.tsx           # Master <TransitionSeries> + frame-locked <Audio> tracks
└── public/
    └── audio/                    # Licensed BGM, VO stems, tactile click/whoosh SFX
```

#### Non-Negotiable Remotion Implementation Rules

- **Zero wall-clock animation:** Drive every transform, opacity, cursor position, and typing reveal strictly from `useCurrentFrame()` + `interpolate()` / `spring()`. Never use CSS `transition`, `@keyframes`, or `setTimeout`.
- **Single-source multi-ratio layouts:** Pass a `format: '16:9' | '9:16' | '1:1'` prop from `Root.tsx` so components adapt font scale, padding, and safe-zone margins dynamically instead of duplicating scenes.
- **Exact preset dimensions (30 or 60 fps):**
  - `Launch16x9`: `1920 x 1080`
  - `Launch9x16`: `1080 x 1920` (anchor primary UI and captions inside `Y = 260..1280px`, `X = 60..900px`)
  - `Launch1x1`: `1080 x 1080`
  - `HeroLoop16x9`: `1280 x 720` or `1920 x 1080`, 15-30s seamless loop, muted, compressed `<2 MB`.
- **Frame-locked UI foley:** Place `<Sequence from={clickFrame}><Audio src={staticFile('audio/click.wav')} volume={0.6} /></Sequence>` on the exact button-press frame; lead transition whooshes by 1 frame (at 60 fps) or 10-20 ms before visual cuts.
- **Isolate volatile UI:** Keep live-product mockups in standalone 3-5s scene modules so post-launch UI changes only require editing one file and re-rendering.

### Escaping Template Sameness

Avoid generic dark-mode floating-UI clones with purple glows and swelling synth beds. Commit to one explicit differentiation lever:

1. **Brand world-building:** Pull distinctive illustration, warm editorial palettes, tactile typography, or mascot character directly from the product's brand system.
2. **Self-referential product proof:** Keep the product's live controls or code visible in frame right beside the real-time output they generate.
3. **Dramatized human workflow pain:** Anchor the opening in a concrete, memorable human scenario of the manual chore your product eliminates before cutting to clean UI.
4. **Deliberate calm restraint:** In noisy hype categories (especially AI), quiet, artifact-first technical pacing signals real engineering confidence.

---

## Reference 3: Audio Engineering, Commercial Licensing, and Demo Credibility

### Silent-First and VO Decision Rules

- **Silent-first baseline:** Roughly 85% of LinkedIn and landing-page video plays occur on mute. Burn verified captions or kinetic headlines inside mobile safe zones and review the full cut muted before export.
- **When to go No-VO (Music + UI Foley + Kinetic Type):** Choose No-VO when the interface transformation is self-explanatory visually, primary distribution is muted social feeds, the audience is technical or multilingual, and you want calm product-first authority.
- **When to use Voiceover:** Choose calm, conversational human VO (never booming commercial announcer delivery) for multi-step B2B workflows, abstract infrastructure, or YouTube/product-page walkthroughs.

### Audio Mixing and Loudness Targets

| Parameter | Target Specification | Notes |
| :--- | :--- | :--- |
| **YouTube / Reels / TikTok Master** | `-14 LUFS` integrated; `-1.0 dBTP` true peak; `48 kHz` AAC | Mastering louder buys zero volume; platforms attenuate and leave dynamics crushed |
| **LinkedIn Native Master** | `-16 LUFS` integrated; `-1.5 dBTP` true peak; `48 kHz` AAC | QC final mix on smartphone speakers at low volume |
| **Dialogue Peak Range** | `-12` to `-6 dBFS` | Record on USB dynamic or lavalier mic with pop filter; never raw laptop mics |
| **Music Ducking Under VO** | `~6 LU` gap (`18-20 dB` below VO) | Sidechain attack `80-150 ms`, release `300-500 ms`, plus gentle midrange EQ carve around vocal clarity band (`1-4 kHz`) |
| **UI Foley & Transition SFX** | Click on exact contact frame; whoosh `10-20 ms` ahead of visual cut | Subtle tactile clicks, toggles, and confirmation chimes make pixels feel physical |

### Ready-to-Run FFmpeg Mastering Commands

#### 1. Sidechain-Duck Music Under Voiceover

```bash
ffmpeg -i vo.wav -i bgm.wav -filter_complex \
  "[1:a]volume=-12dB[bgm_base]; \
   [bgm_base][0:a]sidechaincompress=threshold=0.02:ratio=6:attack=100:release=400[bgm_ducked]; \
   [0:a][bgm_ducked]amix=inputs=2:duration=first:dropout_transition=2[out]" \
  -map "[out]" -ar 48000 -c:a pcm_s24le mix_premaster.wav
```

#### 2. EBU R128 Loudness Normalization (YouTube / Reels / TikTok: -14 LUFS)

For LinkedIn, replace `I=-14:TP=-1.0` with `I=-16:TP=-1.5`.

```bash
# Pass 1: Measure loudness stats
ffmpeg -i input.mp4 -af "loudnorm=I=-14:TP=-1.0:LRA=11:print_format=json" -f null -

# Pass 2: Apply exact linear normalization (plug measured_I, measured_TP, measured_LRA, measured_thresh, offset from Pass 1)
ffmpeg -i input.mp4 -c:v copy \
  -af "loudnorm=I=-14:TP=-1.0:LRA=11:measured_I=-18.2:measured_TP=-2.1:measured_LRA=6.4:measured_thresh=-28.5:offset=0.3:linear=true" \
  -ar 48000 -c:a aac -b:a 192k -movflags +faststart master_14lufs.mp4
```

#### 3. Silent Homepage Hero Loop (<2 MB H.264) + Poster (<50 KB)

```bash
# Compress silent hero loop (adjust -crf 24..28 or scale to 1280:-2 to stay strictly under 2 MB)
ffmpeg -i hero_master.mp4 -an -vf "scale=1280:-2:flags=lanczos" \
  -c:v libx264 -profile:v high -level 4.1 -pix_fmt yuv420p -crf 26 -preset slow \
  -movflags +faststart hero_loop_web.mp4

# Extract crisp poster frame from the best visual moment (e.g., 1.5s) under 50 KB
ffmpeg -ss 00:00:01.5 -i hero_master.mp4 -vframes 1 -vf "scale=1280:-2" -q:v 5 hero_poster.jpg
```

### Commercial Licensing and AI Media Guardrails

- **Paid-ad sync clearance:** Verify that every stock music, SFX, and font license explicitly covers paid social advertising, not only organic posts.
- **AI audio commercial rights:**
  - **ElevenLabs:** Free tier grants NO commercial rights. Require an active paid tier (Starter/Creator+) for commercial launches; obtain explicit consent for cloned human voices.
  - **Suno:** Free tier grants NO commercial rights. Commercial rights apply only to tracks downloaded under an active paid plan (Pro/Premier).
  - **Udio:** Do NOT use for commercial launch videos (post-Nov 2025 terms bar downloads and commercial exploitation).
- **Behind-the-glass vs. visible AI rule:** Empirical studies show measurable trust and CTR penalties when audiences detect or see disclosure for synthetic presenters or AI creative on high-craft brands. Use AI freely **behind the glass** (transcription editing, multilingual subtitles/dubbing, storyboarding, Remotion code generation) and keep **visible trust surfaces** (founders on camera, customer quotes, live product proof) genuinely human. Never fabricate synthetic customer testimonials.

### Radical Demonstration Honesty & Pre-GA Shot Classification

Frame-by-frame scrutiny by technical buyers destroys trust faster than visual imperfection. Enforce these guardrails:

- **Pre-emptive labeling:** If the entire demo runs in real time with no cuts, state `1.0x real speed` visibly on frame 1. Whenever you speed up a load state, trim LLM latency, use sample data, or show an early prototype, label it explicitly on screen (e.g., `4x speed`, `Latency trimmed`, `Sample dataset`).
- **Pre-GA shot triage:** Before shooting or coding a video for software still in active development, classify every planned visual shot:
  - **Stable:** Approved, shipping UI safe for hero capture or SUI rebuild.
  - **Replaceable:** Fluid UI isolated into standalone 3-5s clips that swap cleanly without altering VO timing.
  - **Unconfirmed:** Speculative or unshipped capabilities. Omit completely.

---

## Reference 4: Distribution Specs, Mobile Safe Zones, Web Vitals, and Measurement

### Multi-Format Launch Asset Kit

Generate a coordinated kit from one source project instead of a single 16:9 export:

1. **Flagship Hero Cut (`16:9`, 60-90s):** `1920x1080` H.264 MP4 for YouTube, native X, Product Hunt embed, and website modal.
2. **Vertical Social Cut (`9:16`, 30-60s):** `1080x1920` H.264 MP4 for TikTok, Instagram Reels, YouTube Shorts, and LinkedIn vertical feed with all UI and captions inside the safe core.
3. **Square / Portrait Feed Cut (`1:1` `1080x1080` or `4:5` `1080x1350`, 30-60s):** High-real-estate feed cut for LinkedIn and Instagram feeds.
4. **Teaser / Paid Hook Cut (15s):** Curiosity-first clip for pre-launch momentum or retargeting.
5. **Silent Homepage Hero Loop (15-30s):** `<2 MB` H.264 MP4 (muted, `-an`, `+faststart`) or Lottie/Rive vector loop + `<50 KB` poster image.

### Mobile 9:16 Safe-Zone Geometry (`1080 x 1920 px`)

Platform UI overlays (handles, captions, audio tickers, right-hand engagement buttons, top nav) cover 55% to 67% of a vertical smartphone frame. Keep all headlines, captions, CTAs, and critical product UI strictly inside these margins:

| Platform | Top Margin | Bottom Margin | Left Margin | Right Margin | Usable Safe Area |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Instagram Reels** | `250 px` | `480 px` | `60 px` | `150 px` | ~44.9% |
| **TikTok** | `200 px` | `540 px` | `60 px` | `180 px` | ~37.7% |
| **YouTube Shorts** | `220 px` | `480 px` | `60 px` | `170 px` | ~42.8% |
| **Universal Cross-App Core** | `250 px` | `560 px` | `60 px` | `180 px` | ~32.9% (safest single render) |

Practical layout rule: anchor focal UI and burned-in captions inside **`X = 60..900 px`, `Y = 260..1280 px`**. Reserve the bottom third and right 180 px rail strictly for ambient background bleed.

### Platform-Native Specifications and Rollout Playbook

| Channel | Technical Specs | Algorithmic & Launch Mechanics |
| :--- | :--- | :--- |
| **X (Twitter)** | Native H.264 MP4; `1920x1080` or `1080x1920`; `<=512 MB` | Upload natively (external links sharply suppress reach). Publish from the founder's personal account, place signup and Product Hunt links in Replies 1 and 2, quote-post from the brand account, rally 15-30 genuine supporters in the first 60 minutes, and answer comments live for 2 hours. Schedule a second "available now" wave days later. |
| **LinkedIn** | Native H.264 MP4, AAC, 30 fps; `9:16` for vertical feed, `16:9` for desktop UI shares, `1:1`/`4:5` safe middle; `<200 MB` target | Native uploads outperform YouTube links on reach and watch time. Post from personal founder/exec profiles. First-hour comments and dwell time drive distribution; substantive 3+ min founder demos often out-reach thin 30-60s clips. |
| **Product Hunt** | Public/unlisted YouTube URL + >=2 gallery images (`1270x760`) + square thumbnail (`600x600`, `<1 MB`) | Launch at 12:01 AM PT; first 3-4 hours shape ranking. Never solicit upvotes directly. Avoid unreadable micro-text in the square thumbnail. Skip weak videos that displace crisp screenshots. Capture onboarding immediately so upvotes turn into active users. |
| **YouTube & Shorts** | `16:9` (`1080p`/`4K`) long-form; `9:16` Shorts; `48 kHz` AAC; WebVTT captions + chapter timestamps | Fulfill the thumbnail promise within the first 3s to lift completion. Treat YouTube as a high-intent conversion asset and evergreen SEO companion to docs and changelogs. |
| **Reels & TikTok** | `1080x1920` H.264 MP4; hook within `1.3-3.0s`; burned-in safe-core captions | Optimize for sends/shares per reach and low early exits. Pair visual proof with explicit ICP callouts inside the opening 2 seconds. |

### Landing-Page Conversion Physics and Core Web Vitals

Video lifts landing-page conversion only when all six conditions hold simultaneously:

1. **Visually demonstrable value:** Use video to answer "How does this actually work?", never to replace scannable pricing or technical specs.
2. **Fold runtime cap:** Keep hero fold video <=60s (<=120s lower on page).
3. **Silent by default or click-to-play:** Never autoplay with sound. Use click-to-play + custom thumbnail for narrated explainers; reserve muted autoplay strictly for 15-30s ambient UI loops.
4. **Skippable conversion path:** Pair the player with a clear headline, scannable value bullets, and a two-tier CTA hierarchy (primary action like "Start Free Trial" beside secondary "Watch Demo") visible above the fold on mobile.
5. **Zero Core Web Vitals regression (<2s load / <2.5s LCP):** Compress hero H.264 MP4s to **`<2 MB`**, serve a **`<50 KB`** poster image, lazy-load below-the-fold videos, and never embed raw YouTube iframes (use `lite-youtube-embed` or replace UI loops with Lottie, Rive, or animated SVG).
6. **Two-tier mid-funnel handoff:** Pair the hero launch video with a 3-5 chapter interactive click-through tour (30-45s per chapter) on product/pricing pages.

### Post-Ship Modular Maintenance Triage

When the product UI ships changes after launch, keep homepage video aligned with live signup reality:

- **Patch (hours):** Re-capture or swap one isolated 3-5s shot when button placement/styling changed while workflow logic stayed intact.
- **Section Re-Record (1-2 days):** Re-render one altered workflow segment while preserving intro, outro, VO arc, and music.
- **Full Rebuild:** Trigger only when the design system, primary workflow, or target ICP fundamentally changes.

Propagate updates across paid social cuts, sales decks, and onboarding emails.

### Three-Tier Measurement and Diagnostic Benchmarks

Never compare raw view counts across platforms (view definitions differ across TikTok, LinkedIn/X, and YouTube). Track three connected tiers:

#### Tier 1: Attention Diagnostics

| Platform | Healthy 3s Hold Rate (Pass) | Weak Hook Threshold (Fix Immediately) |
| :--- | :--- | :--- |
| **TikTok** | `>70%` | `<50%` |
| **Instagram Reels** | `>65%` | `<45%` |
| **YouTube Shorts** | `>60%` | `<40%` |
| **LinkedIn** | `>55%` | `<35%` |

Retention diagnostics: Nose cliffs = slow intro or weak hook; Body decay = static pacing; Tail cliffs = premature wrap-up cues before CTA; rewatch spikes = strongest clips to repurpose for paid ads. Homepage click-to-play benchmark: ~24%.

#### Tier 2: Acquisition & Conversion Attribution

- Assign dedicated channel paths/UTMs (`/x`, `/li`, `/yt`, `/ph`) to separate high-impression feeds from high-converting channels.
- Keep gated demo forms at `<=5` fields (conversion drops sharply past 5 fields).
- Run single-variable landing-page A/B tests for 2-4 full business weeks to 95% confidence.

#### Tier 3: Pipeline & Product Retention

- Sync video watch thresholds into CRM deal records to measure multi-stakeholder account views and influenced pipeline.
- For PLG launches, optimize for **Day-5 active user retention**, not Day-1 signup spikes.

---

## Verification Scripts

Save these two scripts into `scripts/` alongside this `SKILL.md` (or write them to a temp directory during agent execution) to enforce deterministic Stage 3 and Stage 6 quality gates.

### `scripts/script_budget.py`

```python
#!/usr/bin/env python3
"""Validate spoken launch video scripts against runtime word budgets and pacing anti-patterns.

Usage:
    python3 scripts/script_budget.py --duration 60 --pace launch script.txt
    cat script.txt | python3 scripts/script_budget.py --duration 45 --pace enterprise --json
"""

import argparse
import json
import re
import sys
from pathlib import Path

PACE_WPM = {
    "enterprise": 135,
    "launch": 150,
    "social": 165,
}

VISUAL_RESERVE_RATIO = 0.17

BANNED_FILLER = [
    "seamless",
    "seamlessly",
    "empowering",
    "empower",
    "unlock",
    "revolutionize",
    "revolutionary",
    "game-changer",
    "game changer",
    "cutting-edge",
    "next-generation",
    "synergy",
    "supercharge",
    "effortless",
    "effortlessly",
    "reimagine",
]

TAIL_CLICHES = [
    "in summary",
    "to summarize",
    "to wrap up",
    "to wrap things up",
    "that's about it",
    "thats about it",
    "in conclusion",
    "thanks for watching",
    "hi everyone",
    "hello everyone",
    "welcome back",
]


def strip_stage_directions(text: str) -> str:
    # Remove bracketed stage directions like [0:00 - Hook] or (cut to UI)
    cleaned = re.sub(r"\[[^\]]*\]", " ", text)
    cleaned = re.sub(r"\([^)]*\)", " ", cleaned)
    # Remove markdown headings and timestamp prefixes like 0:05 -
    lines = []
    for line in cleaned.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        stripped = re.sub(r"^\d{1,2}:\d{2}(?:\.\d+)?\s*[-:]?\s*", "", stripped)
        lines.append(stripped)
    return " ".join(lines)


def analyze_script(raw_text: str, duration_sec: float, wpm: int) -> dict:
    spoken = strip_stage_directions(raw_text)
    words = re.findall(r"[A-Za-z0-9']+", spoken)
    word_count = len(words)

    speech_window_sec = duration_sec * (1.0 - VISUAL_RESERVE_RATIO)
    max_words = int(round((speech_window_sec * wpm) / 60.0))
    est_speech_sec = (word_count / wpm) * 60.0 if wpm > 0 else 0.0
    visual_hold_left_sec = duration_sec - est_speech_sec

    sentences = [s.strip() for s in re.split(r"[.!?]+", spoken) if s.strip()]
    sentence_lengths = [len(re.findall(r"[A-Za-z0-9']+", s)) for s in sentences]
    avg_sentence_len = round(sum(sentence_lengths) / len(sentence_lengths), 1) if sentence_lengths else 0.0
    long_sentences = [
        {"sentence": s, "words": n}
        for s, n in zip(sentences, sentence_lengths)
        if n > 24
    ]

    lower_spoken = spoken.lower()
    filler_hits = sorted({term for term in BANNED_FILLER if re.search(r"\b" + re.escape(term) + r"\b", lower_spoken)})
    cliche_hits = sorted({term for term in TAIL_CLICHES if re.search(r"\b" + re.escape(term) + r"\b", lower_spoken)})

    errors = []
    warnings = []

    if word_count == 0:
        errors.append("No spoken words detected after stripping stage directions.")
    if word_count > max_words:
        errors.append(
            f"Over word budget: {word_count} words exceeds max {max_words} words for {duration_sec:.0f}s at {wpm} WPM "
            f"(with 17% visual-hold reserve). Trim at least {word_count - max_words} words."
        )
    if avg_sentence_len > 22.0:
        warnings.append(f"Average sentence length is {avg_sentence_len} words (target 15-20 for spoken cadence).")
    if long_sentences:
        warnings.append(f"{len(long_sentences)} sentence(s) exceed 24 words; split for cleaner vocal rhythm.")
    if filler_hits:
        warnings.append(f"Corporate filler detected: {', '.join(filler_hits)}. Replace with concrete product mechanics.")
    if cliche_hits:
        errors.append(f"Intro/tail retention-killer cliche detected: {', '.join(cliche_hits)}. Remove immediately.")

    return {
        "status": "PASS" if not errors else "FAIL",
        "duration_sec": duration_sec,
        "wpm": wpm,
        "speech_window_sec": round(speech_window_sec, 1),
        "word_count": word_count,
        "max_words": max_words,
        "headroom_words": max_words - word_count,
        "estimated_speech_sec": round(est_speech_sec, 1),
        "remaining_visual_hold_sec": round(visual_hold_left_sec, 1),
        "sentence_count": len(sentences),
        "avg_sentence_words": avg_sentence_len,
        "long_sentences": long_sentences,
        "filler_hits": filler_hits,
        "cliche_hits": cliche_hits,
        "errors": errors,
        "warnings": warnings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate a software launch video script budget and cadence.")
    parser.add_argument("script_file", nargs="?", help="Path to script text/markdown file (or read from stdin).")
    parser.add_argument("--duration", type=float, required=True, help="Target total runtime in seconds (e.g., 60).")
    parser.add_argument("--pace", choices=PACE_WPM.keys(), default="launch", help="Speaking pace preset.")
    parser.add_argument("--wpm", type=int, help="Optional custom WPM override.")
    parser.add_argument("--json", action="store_true", help="Output machine-readable JSON.")
    args = parser.parse_args()

    if args.script_file:
        text = Path(args.script_file).read_text(encoding="utf-8")
    else:
        text = sys.stdin.read()

    wpm = args.wpm if args.wpm else PACE_WPM[args.pace]
    report = analyze_script(text, args.duration, wpm)

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"=== Script Budget Audit: {report['status']} ===")
        print(
            f"Words: {report['word_count']} / {report['max_words']} max "
            f"({report['duration_sec']:.0f}s @ {report['wpm']} WPM, 17% visual hold)"
        )
        print(
            f"Est. Speech: {report['estimated_speech_sec']}s | "
            f"Visual Hold Left: {report['remaining_visual_hold_sec']}s | "
            f"Avg Sentence: {report['avg_sentence_words']} words"
        )
        for err in report["errors"]:
            print(f"[FAIL] {err}")
        for warn in report["warnings"]:
            print(f"[WARN] {warn}")
        if not report["errors"] and not report["warnings"]:
            print("All pacing, word-budget, and copy checks passed cleanly.")

    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
```

### `scripts/verify_render.py`

```python
#!/usr/bin/env python3
"""Verify rendered software launch video files against technical, loudness, and safe-zone gates.

Requires: ffprobe and ffmpeg in PATH. Optional: Pillow (PIL) for drawing 9:16 safe-zone overlays.

Usage:
    python3 scripts/verify_render.py output/launch-16x9.mp4 --target youtube --extract-frames qc/16x9
    python3 scripts/verify_render.py output/launch-9x16.mp4 --target vertical --extract-frames qc/9x16
    python3 scripts/verify_render.py output/hero-loop.mp4 --target hero
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

TARGETS = {
    "youtube": {
        "aspect": (16, 9),
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": None,
        "require_silent": False,
    },
    "x": {
        "aspect": None,
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": 512.0,
        "require_silent": False,
    },
    "linkedin": {
        "aspect": None,
        "lufs": -16.0,
        "lufs_tol": 1.5,
        "true_peak_max": -1.3,
        "max_mb": 200.0,
        "require_silent": False,
    },
    "vertical": {
        "aspect": (9, 16),
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": 200.0,
        "require_silent": False,
    },
    "square": {
        "aspect": (1, 1),
        "lufs": -14.0,
        "lufs_tol": 1.5,
        "true_peak_max": -0.8,
        "max_mb": 200.0,
        "require_silent": False,
    },
    "hero": {
        "aspect": (16, 9),
        "lufs": None,
        "lufs_tol": None,
        "true_peak_max": None,
        "max_mb": 2.0,
        "require_silent": True,
    },
}


def run_cmd(cmd: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True)


def probe_video(path: Path) -> dict:
    res = run_cmd([
        "ffprobe", "-v", "error",
        "-show_format", "-show_streams",
        "-print_format", "json", str(path),
    ])
    if res.returncode != 0:
        raise RuntimeError(f"ffprobe failed: {res.stderr.strip()}")
    return json.loads(res.stdout)


def measure_loudness(path: Path, target_lufs: float, target_tp: float) -> dict | None:
    res = run_cmd([
        "ffmpeg", "-hide_banner", "-nostats", "-i", str(path),
        "-af", f"loudnorm=I={target_lufs}:TP={target_tp}:LRA=11:print_format=json",
        "-f", "null", "-",
    ])
    match = re.search(r'\{[\s\S]*?"input_i"[\s\S]*?\}', res.stderr)
    if not match:
        return None
    data = json.loads(match.group(0))
    return {
        "input_i": float(data["input_i"]),
        "input_tp": float(data["input_tp"]),
        "input_lra": float(data["input_lra"]),
    }


def draw_safe_zone_overlay(image_path: Path, out_path: Path) -> bool:
    try:
        from PIL import Image, ImageDraw
    except ImportError:
        return False

    img = Image.open(image_path).convert("RGBA")
    w, h = img.size
    # Scale universal 1080x1920 cross-app safe-zone margins proportionally
    top = int(round(h * (250 / 1920)))
    bottom = int(round(h * (560 / 1920)))
    left = int(round(w * (60 / 1080)))
    right = int(round(w * (180 / 1080)))

    overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    mask_color = (220, 38, 38, 95)

    # Top, bottom, left, right platform UI obstruction zones
    draw.rectangle([0, 0, w, top], fill=mask_color)
    draw.rectangle([0, h - bottom, w, h], fill=mask_color)
    draw.rectangle([0, top, left, h - bottom], fill=mask_color)
    draw.rectangle([w - right, top, w, h - bottom], fill=mask_color)
    # Green border around safe core
    draw.rectangle([left, top, w - right, h - bottom], outline=(34, 197, 94, 230), width=4)

    combined = Image.alpha_composite(img, overlay).convert("RGB")
    combined.save(out_path)
    return True


def extract_qc_frames(path: Path, duration: float, out_dir: Path, is_vertical: bool) -> list[str]:
    out_dir.mkdir(parents=True, exist_ok=True)
    timestamps = [
        ("01_hook_0p5s", min(0.5, max(0.0, duration - 0.1))),
        ("02_hook_1p5s", min(1.5, max(0.0, duration - 0.1))),
        ("03_lock_3p0s", min(3.0, max(0.0, duration - 0.1))),
        ("04_body_25pct", duration * 0.25),
        ("05_body_50pct", duration * 0.50),
        ("06_body_75pct", duration * 0.75),
        ("07_tail_end", max(0.0, duration - 1.0)),
    ]
    extracted = []
    for label, ts in timestamps:
        frame_path = out_dir / f"{label}.png"
        res = run_cmd([
            "ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
            "-ss", f"{ts:.3f}", "-i", str(path), "-vframes", "1", str(frame_path),
        ])
        if res.returncode == 0 and frame_path.exists():
            extracted.append(str(frame_path))
            if is_vertical:
                overlay_path = out_dir / f"{label}_safezone.png"
                if draw_safe_zone_overlay(frame_path, overlay_path):
                    extracted.append(str(overlay_path))
    return extracted


def parse_fps(rate_str: str) -> float:
    if "/" in rate_str:
        num, den = rate_str.split("/")
        return round(float(num) / float(den), 2) if float(den) else 0.0
    return round(float(rate_str), 2)


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify a rendered software launch video against quality gates.")
    parser.add_argument("video", help="Path to rendered video file.")
    parser.add_argument("--target", choices=TARGETS.keys(), default="youtube", help="Distribution target preset.")
    parser.add_argument("--extract-frames", help="Optional directory to extract QC inspection frames.")
    parser.add_argument("--json", action="store_true", help="Output machine-readable JSON.")
    args = parser.parse_args()

    if not shutil.which("ffprobe") or not shutil.which("ffmpeg"):
        print("Error: ffmpeg and ffprobe must be installed and in PATH.", file=sys.stderr)
        return 2

    video_path = Path(args.video)
    if not video_path.exists():
        print(f"Error: File not found: {video_path}", file=sys.stderr)
        return 2

    cfg = TARGETS[args.target]
    probe = probe_video(video_path)
    streams = probe.get("streams", [])
    v_stream = next((s for s in streams if s.get("codec_type") == "video"), None)
    a_stream = next((s for s in streams if s.get("codec_type") == "audio"), None)

    if not v_stream:
        print("Error: No video stream found.", file=sys.stderr)
        return 2

    width = int(v_stream["width"])
    height = int(v_stream["height"])
    codec = v_stream.get("codec_name", "")
    pix_fmt = v_stream.get("pix_fmt", "")
    fps = parse_fps(v_stream.get("r_frame_rate", "0/1"))
    duration = float(probe.get("format", {}).get("duration", 0.0))
    size_mb = round(video_path.stat().st_size / (1024 * 1024), 2)

    errors = []
    warnings = []

    if codec != "h264":
        warnings.append(f"Video codec is '{codec}'; prefer 'h264' (yuv420p) for universal social and browser compatibility.")
    if pix_fmt and pix_fmt != "yuv420p":
        warnings.append(f"Pixel format is '{pix_fmt}'; prefer 'yuv420p' so Safari and mobile players render cleanly.")

    if cfg["aspect"]:
        exp_w, exp_h = cfg["aspect"]
        actual_ratio = width / height
        expected_ratio = exp_w / exp_h
        if abs(actual_ratio - expected_ratio) > 0.03:
            errors.append(f"Aspect ratio mismatch for {args.target}: got {width}x{height}, expected {exp_w}:{exp_h}.")

    if cfg["max_mb"] is not None and size_mb > cfg["max_mb"]:
        errors.append(f"File size {size_mb} MB exceeds {args.target} limit of {cfg['max_mb']} MB.")

    if args.target == "hero" and duration > 35.0:
        warnings.append(f"Hero loop is {duration:.1f}s; keep silent homepage hero loops to 15-30s.")

    loudness = None
    if cfg["require_silent"]:
        if a_stream is not None:
            errors.append("Homepage hero loop contains an audio stream. Strip audio (-an) so playback is strictly silent.")
    else:
        if a_stream is None:
            warnings.append("No audio stream detected. Confirm this cut is intentionally silent.")
        else:
            sample_rate = int(a_stream.get("sample_rate", 0))
            if sample_rate != 48000:
                warnings.append(f"Audio sample rate is {sample_rate} Hz; prefer 48000 Hz AAC.")
            loudness = measure_loudness(video_path, cfg["lufs"], -1.0 if cfg["lufs"] == -14.0 else -1.5)
            if loudness:
                if abs(loudness["input_i"] - cfg["lufs"]) > cfg["lufs_tol"]:
                    errors.append(
                        f"Integrated loudness {loudness['input_i']:.1f} LUFS is outside "
                        f"{cfg['lufs']:.1f} +/- {cfg['lufs_tol']:.1f} LUFS for {args.target}."
                    )
                if loudness["input_tp"] > cfg["true_peak_max"]:
                    errors.append(
                        f"True peak {loudness['input_tp']:.1f} dBTP exceeds ceiling {cfg['true_peak_max']:.1f} dBTP."
                    )

    extracted_frames = []
    if args.extract_frames:
        is_vertical = height > width
        extracted_frames = extract_qc_frames(video_path, duration, Path(args.extract_frames), is_vertical)

    report = {
        "status": "PASS" if not errors else "FAIL",
        "file": str(video_path),
        "target": args.target,
        "width": width,
        "height": height,
        "codec": codec,
        "pix_fmt": pix_fmt,
        "fps": fps,
        "duration_sec": round(duration, 2),
        "size_mb": size_mb,
        "has_audio": a_stream is not None,
        "loudness": loudness,
        "extracted_frames": extracted_frames,
        "errors": errors,
        "warnings": warnings,
    }

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"=== Render Verification ({args.target}): {report['status']} ===")
        print(
            f"Video: {width}x{height} @ {fps} fps | {codec}/{pix_fmt} | "
            f"{duration:.2f}s | {size_mb} MB | Audio: {'yes' if a_stream else 'no'}"
        )
        if loudness:
            print(
                f"Loudness: {loudness['input_i']:.1f} LUFS | "
                f"True Peak: {loudness['input_tp']:.1f} dBTP | LRA: {loudness['input_lra']:.1f} LU"
            )
        if extracted_frames:
            print(f"Extracted {len(extracted_frames)} QC frame(s) to {args.extract_frames}")
        for err in errors:
            print(f"[FAIL] {err}")
        for warn in warnings:
            print(f"[WARN] {warn}")
        if not errors and not warnings:
            print("All technical and loudness gates passed.")

    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
```