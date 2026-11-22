//! Optional iPad Math Notes-style inline auto math solver.
//!
//! When enabled (`Settings::auto_math_ink`), drawing an equals sign (`=`) after a
//! handwritten numeric expression (e.g. `4+4=`, `\sqrt{3}=`) silently recognizes the
//! expression on the background stroke-recognition thread, evaluates numeric answers in
//! pure Rust, and inserts authentic handwritten ink strokes right after `=`.
pub mod detect;
pub mod eval;
pub mod glyphs;
pub mod stats;

use crate::Controller;
use detect::{
    EqualsCandidate, PageEquationCandidate, detect_equals_equation, detect_page_equations,
    right_side_is_clear,
};
use eval::{
    evaluate_trailing_equals, evaluate_trailing_equals_with_equations, needs_page_variables,
};
use folio_document::*;
use glyphs::{StrokeReveal, synthesize_handwritten_answer};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

/// Record of a previously auto-solved equation on a page, used to isolate closely stacked rows
/// while automatically freeing LHS strokes if the user erases that equation's `=`, and to power
/// hover popups showing the detected LaTeX for each auto-completed answer.
pub(crate) struct SolvedAutoMathRow {
    pub note: Id,
    pub page: Id,
    pub eq_ids: (Id, Id),
    pub lhs_ids: Vec<Id>,
    pub answer_ids: Vec<Id>,
    pub detected_latex: String,
    pub vector: Option<Arc<folio_math::VectorFormula>>,
    pub context_latex: Option<String>,
    pub context_vector: Option<Arc<folio_math::VectorFormula>>,
}

/// Detected LaTeX metadata shown in a hover popup over an auto-completed handwritten answer.
#[derive(Clone)]
pub struct AutoMathHoverInfo {
    /// Live bounding box of the auto-completed answer strokes in document coordinates.
    pub anchor_bounds: Rect,
    /// Detected equation LaTeX (`lhs = answer`) that produced the answer.
    pub detected_latex: String,
    /// Pre-typeset vector outlines for `detected_latex`.
    pub vector: Option<Arc<folio_math::VectorFormula>>,
    /// Optional page context definitions (e.g. `y = 3,\; x = y^2 + 4`) that contributed to the answer.
    pub context_latex: Option<String>,
    /// Pre-typeset vector outlines for `context_latex`.
    pub context_vector: Option<Arc<folio_math::VectorFormula>>,
}

/// Hold briefly at golden-yellow after the final digit finishes before fading to pen color.
const FADE_HOLD_MS: u32 = 110;
/// Duration of the yellow -> pen-color fade once writing finishes.
const FADE_DURATION_MS: u32 = 460;

/// Active write-on + color-fade presentation state for one auto-solved handwritten answer.
#[derive(Clone, Debug)]
pub struct AutoMathAnimation {
    pub note: Id,
    pub page: Id,
    pub started: Instant,
    pub write_end_ms: u32,
    pub fade_start_ms: u32,
    pub fade_duration_ms: u32,
    pub strokes: Vec<StrokeReveal>,
}

/// Sampled presentation state of one animating stroke at a specific instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoMathStrokeState {
    /// Arc-length reveal fraction in `[0.0, 1.0]` (`0.0` = hidden, `1.0` = fully drawn).
    pub progress: f32,
    /// Blend factor toward golden-yellow in `[0.0, 1.0]` (`1.0` = yellow, `0.0` = pen color).
    pub yellow_mix: f32,
}

impl AutoMathAnimation {
    pub fn total_duration_ms(&self) -> u32 {
        self.fade_start_ms.saturating_add(self.fade_duration_ms)
    }

    pub fn is_finished(&self, now: Instant) -> bool {
        let elapsed_ms = now.saturating_duration_since(self.started).as_secs_f32() * 1000.0;
        elapsed_ms >= self.total_duration_ms() as f32
    }

    /// Sample reveal progress and yellow-tint mix for all strokes in this animation at `now`.
    pub fn sample_at(&self, now: Instant, out: &mut HashMap<Id, AutoMathStrokeState>) {
        let elapsed_ms = now.saturating_duration_since(self.started).as_secs_f32() * 1000.0;
        let yellow_mix = if elapsed_ms <= self.fade_start_ms as f32 {
            1.0
        } else if self.fade_duration_ms == 0 {
            0.0
        } else {
            let t = ((elapsed_ms - self.fade_start_ms as f32) / self.fade_duration_ms as f32)
                .clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t);
            (1.0 - eased).clamp(0.0, 1.0)
        };
        for reveal in &self.strokes {
            let progress = if elapsed_ms <= reveal.start_ms as f32 {
                0.0
            } else {
                let dur = (reveal.duration_ms.max(1)) as f32;
                let u = ((elapsed_ms - reveal.start_ms as f32) / dur).clamp(0.0, 1.0);
                // Natural handwriting kinematics: gentle ease-in/out without sluggishness.
                let smooth = u * u * (3.0 - 2.0 * u);
                (0.25 * u + 0.75 * smooth).clamp(0.0, 1.0)
            };
            out.insert(
                reveal.id,
                AutoMathStrokeState {
                    progress,
                    yellow_mix,
                },
            );
        }
    }
}

/// Background job submitted to the resident stroke-recognition engine.
pub(crate) struct AutoMathJob {
    pub pack: PathBuf,
    pub note: Id,
    pub page: Id,
    pub candidate: EqualsCandidate,
    pub variables: BTreeMap<String, String>,
    /// Compact equations with ink on both sides of `=` elsewhere on the page (e.g. `x = 3`,
    /// `2x + 4 = 10`), recognized lazily only when the triggered expression uses a variable.
    pub page_equations: Vec<PageEquationCandidate>,
}

/// Completed background result delivered back to the controller.
pub(crate) struct AutoMathCompletion {
    pub note: Id,
    pub page: Id,
    pub candidate: EqualsCandidate,
    pub lhs_latex: String,
    pub answer: String,
    pub detected_latex: String,
    pub vector: Option<Arc<folio_math::VectorFormula>>,
    pub context_latex: Option<String>,
    pub context_vector: Option<Arc<folio_math::VectorFormula>>,
}

fn format_answer_latex(answer: &str) -> String {
    if let Some((mant, exp)) = answer.split_once('e')
        && !exp.is_empty()
        && exp.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b'+')
    {
        return format!("{mant} \\times 10^{{{exp}}}");
    }
    answer.to_string()
}

impl AutoMathJob {
    /// Run pure-Rust stroke recognition + numeric/algebra evaluation on the worker thread.
    pub(crate) fn run(
        self,
        engine: &mut folio_recognition::Engine,
    ) -> Option<AutoMathCompletion> {
        let recognized = engine.recognize_math(&self.candidate.strokes).ok()?;
        let mut used_context: Vec<String> = Vec::new();
        let (lhs_latex, answer) = if needs_page_variables(&recognized)
            && !self.page_equations.is_empty()
        {
            let mut recognized_eqs = Vec::with_capacity(self.page_equations.len());
            for eq in &self.page_equations {
                if let Ok(latex) = engine.recognize_math(&eq.strokes) {
                    recognized_eqs.push(latex);
                }
            }
            let direct = evaluate_trailing_equals(&recognized, &self.variables);
            let with_eqs =
                evaluate_trailing_equals_with_equations(&recognized, &self.variables, &recognized_eqs)?;
            // Only display page context when page equations actually contributed to the result.
            if direct.as_ref().map(|(_, a)| a) != Some(&with_eqs.1) {
                for eq in &recognized_eqs {
                    if !eq.trim().is_empty() {
                        used_context.push(eq.trim().to_string());
                    }
                }
            }
            with_eqs
        } else {
            evaluate_trailing_equals(&recognized, &self.variables)?
        };
        let detected_latex = format!("{lhs_latex} = {}", format_answer_latex(&answer));
        let vector = crate::math_solver::typeset(&detected_latex).ok();
        let context_latex = (!used_context.is_empty()).then(|| used_context.join(",\\; "));
        let context_vector = context_latex
            .as_deref()
            .and_then(|ctx| crate::math_solver::typeset(ctx).ok());
        Some(AutoMathCompletion {
            note: self.note,
            page: self.page,
            candidate: self.candidate,
            lhs_latex,
            answer,
            detected_latex,
            vector,
            context_latex,
            context_vector,
        })
    }
}

fn canonical_pair(a: Id, b: Id) -> (Id, Id) {
    if a <= b { (a, b) } else { (b, a) }
}

impl Controller {
    /// Return the detected LaTeX metadata when the pointer hovers over an auto-completed answer
    /// on the current page. Bounds are computed live so moving/erasing strokes stays accurate.
    pub fn hovered_auto_math_answer(&self) -> Option<AutoMathHoverInfo> {
        let cursor = self.cursor?;
        let page = self.page();
        let hidden = page.hidden_sources();
        for row in self.auto_math_solved_rows.iter().rev() {
            if row.note != self.active || row.page != page.id {
                continue;
            }
            let mut bounds: Option<Rect> = None;
            for &id in &row.answer_ids {
                if hidden.contains(&id) {
                    continue;
                }
                let Some(obj) = page.objects.get(&id) else {
                    continue;
                };
                let b = obj.bounds();
                bounds = Some(match bounds {
                    Some(acc) => acc.union(b),
                    None => b,
                });
            }
            let Some(anchor_bounds) = bounds else {
                continue;
            };
            let pad = (anchor_bounds.height() * 0.25).clamp(8.0, 16.0);
            if anchor_bounds.expand(pad).contains(cursor) {
                return Some(AutoMathHoverInfo {
                    anchor_bounds,
                    detected_latex: row.detected_latex.clone(),
                    vector: row.vector.clone(),
                    context_latex: row.context_latex.clone(),
                    context_vector: row.context_vector.clone(),
                });
            }
        }
        None
    }

    /// Collect stroke IDs belonging to intact previously solved equations on the current page.
    /// If the user erases an equation's `=`, its LHS strokes automatically become eligible again.
    fn active_consumed_lhs_ids(&self) -> HashSet<Id> {
        let page = self.page();
        let hidden = page.hidden_sources();
        let mut ids = HashSet::new();
        for row in &self.auto_math_solved_rows {
            if row.note == self.active
                && row.page == page.id
                && page.objects.contains_key(&row.eq_ids.0)
                && page.objects.contains_key(&row.eq_ids.1)
                && !hidden.contains(&row.eq_ids.0)
                && !hidden.contains(&row.eq_ids.1)
            {
                ids.insert(row.eq_ids.0);
                ids.insert(row.eq_ids.1);
                ids.extend(row.lhs_ids.iter().copied());
            }
        }
        ids
    }

    /// Called immediately after committing a freehand pen stroke (non-highlighter, non-shape).
    /// Performs an O(1) local geometric `=` check; only submits background work when a real
    /// equals sign is completed at the end of a handwritten expression.
    pub(crate) fn maybe_trigger_auto_math(&mut self, stroke: &InkStroke) {
        if !self.settings.auto_math_ink
            || self.tool != crate::Tool::Pen
            || stroke.style.tool == InkTool::Highlighter
            || self.session().document.metadata.trashed
        {
            return;
        }
        let pack = self.recognition_pack();
        if !folio_recognition::Engine::pack_ready(&pack) {
            return;
        }
        let consumed_lhs = self.active_consumed_lhs_ids();
        let Some(candidate) = detect_equals_equation(
            self.session(),
            stroke,
            &self.auto_math_generated_ids,
            &consumed_lhs,
        ) else {
            return;
        };
        let pair = canonical_pair(candidate.eq_ids.0, candidate.eq_ids.1);
        if self.auto_math_solved_eqs.contains(&pair) {
            return;
        }
        let variables = self.math_variables().unwrap_or_default();
        let page_equations = detect_page_equations(
            self.session(),
            candidate.eq_ids,
            &self.auto_math_generated_ids,
        );
        let job = AutoMathJob {
            pack,
            note: self.active,
            page: self.page().id,
            candidate,
            variables,
            page_equations,
        };
        self.recognition_service.submit_auto_math(job);
    }

    /// Sample active auto-math stroke reveal & color-fade states for `page_id` at `now`.
    pub fn auto_math_animation_states(
        &self,
        page_id: Id,
        now: Instant,
    ) -> HashMap<Id, AutoMathStrokeState> {
        let mut states = HashMap::new();
        if self.settings.reduce_motion {
            return states;
        }
        for anim in &self.auto_math_animations {
            if anim.note == self.active && anim.page == page_id && !anim.is_finished(now) {
                anim.sample_at(now, &mut states);
            }
        }
        states
    }

    /// Apply any completed inline auto-math solutions safely on the controller thread,
    /// and advance/prune active write-on animations.
    pub(crate) fn poll_auto_math(&mut self) -> bool {
        let mut changed = false;
        let now = Instant::now();
        if !self.auto_math_animations.is_empty() {
            let active_note = self.active;
            let active_page = self.page().id;
            let reduce_motion = self.settings.reduce_motion;
            let had_any = !self.auto_math_animations.is_empty();
            self.auto_math_animations.retain(|anim| {
                if reduce_motion || anim.is_finished(now) {
                    return false;
                }
                if anim.note == active_note && anim.page == active_page {
                    let page = self.sessions[&active_note].page();
                    anim.strokes.iter().any(|s| page.objects.contains_key(&s.id))
                } else {
                    true
                }
            });
            // Keep repainting every frame while animating, plus one final frame when settling.
            if had_any {
                changed = true;
            }
        }
        while let Some(done) = self.recognition_service.try_recv_auto_math() {
            if !self.settings.auto_math_ink
                || done.note != self.active
                || done.page != self.page().id
                || self.session().document.metadata.trashed
            {
                continue;
            }
            // Verify both `=` bars are still present and haven't already been solved.
            let pair = canonical_pair(done.candidate.eq_ids.0, done.candidate.eq_ids.1);
            if self.auto_math_solved_eqs.contains(&pair) {
                continue;
            }
            let page = self.page();
            if !page.objects.contains_key(&done.candidate.eq_ids.0)
                || !page.objects.contains_key(&done.candidate.eq_ids.1)
            {
                continue;
            }
            // Re-verify the space to the right of `=` is still empty so we never overwrite user ink.
            if !right_side_is_clear(
                self.session(),
                done.candidate.eq_ids,
                done.candidate.eq_bounds,
                done.candidate.style.glyph_height,
            ) {
                continue;
            }
            let page_bounds = (!page.properties.infinite)
                .then_some((page.properties.width, page.properties.height));
            let synthesized = synthesize_handwritten_answer(
                &done.answer,
                done.candidate.answer_x,
                &done.candidate.style,
                done.candidate.seed,
                page_bounds,
            );
            if synthesized.strokes.is_empty() {
                continue;
            }
            let base_index = page.order.len();
            let page_id = page.id;
            let mut answer_ids = Vec::with_capacity(synthesized.strokes.len());
            let mut changes = Vec::with_capacity(synthesized.strokes.len());
            for (offset, stroke) in synthesized.strokes.into_iter().enumerate() {
                let id = stroke.id;
                answer_ids.push(id);
                self.auto_math_generated_ids.insert(id);
                changes.push(Change::Object {
                    page: page_id,
                    id,
                    before: None,
                    after: Some(Arc::new(Object::Stroke(stroke))),
                    index: base_index + offset,
                });
            }
            self.auto_math_solved_eqs.insert(pair);
            self.auto_math_solved_rows.push(SolvedAutoMathRow {
                note: self.active,
                page: page_id,
                eq_ids: done.candidate.eq_ids,
                lhs_ids: done.candidate.lhs_ids,
                answer_ids,
                detected_latex: done.detected_latex,
                vector: done.vector,
                context_latex: done.context_latex,
                context_vector: done.context_vector,
            });
            // Keep tracking sets bounded in long sessions without discarding recent rows.
            if self.auto_math_solved_eqs.len() > 256 {
                self.auto_math_solved_eqs.clear();
                self.auto_math_solved_eqs.insert(pair);
            }
            if self.auto_math_generated_ids.len() > 2048 {
                self.auto_math_generated_ids.clear();
            }
            if self.auto_math_solved_rows.len() > 128 {
                let excess = self.auto_math_solved_rows.len() - 128;
                self.auto_math_solved_rows.drain(0..excess);
            }
            self.commit("Auto-solve math", changes);
            if !self.settings.reduce_motion && !synthesized.reveals.is_empty() {
                self.auto_math_animations.push(AutoMathAnimation {
                    note: self.active,
                    page: page_id,
                    started: Instant::now(),
                    write_end_ms: synthesized.write_end_ms,
                    fade_start_ms: synthesized.write_end_ms.saturating_add(FADE_HOLD_MS),
                    fade_duration_ms: FADE_DURATION_MS,
                    strokes: synthesized.reveals,
                });
            }
            self.status = format!("Auto-solved: {} = {}", done.lhs_latex, done.answer);
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Controller;
    use folio_ink::StrokeBuilder;
    use std::time::{Duration, Instant};

    fn make_stroke(pts: &[(f32, f32)], style: &PenStyle) -> InkStroke {
        let mut b = StrokeBuilder::new(style.clone());
        for (i, &(x, y)) in pts.iter().enumerate() {
            b.push(StrokePoint::new(Point::new(x, y), 0.6, (i as u64 + 1) * 8));
        }
        b.finish().expect("valid stroke")
    }

    fn commit_raw(app: &mut Controller, strokes: Vec<InkStroke>) {
        let page = app.page().id;
        let base = app.page().order.len();
        let changes = strokes
            .into_iter()
            .enumerate()
            .map(|(i, s)| Change::Object {
                page,
                id: s.id,
                before: None,
                after: Some(Arc::new(Object::Stroke(s))),
                index: base + i,
            })
            .collect();
        app.commit("Draw test strokes", changes);
    }

    #[test]
    fn detects_equals_and_synthesizes_handwritten_answer_strokes() {
        let root = std::env::temp_dir().join(format!("folio-auto-math-test-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let style = PenStyle::default();

        // Draw "4 + 4" using clean strokes, then draw "=" via the last two horizontal bars.
        let strokes = vec![
            // "4": down-right L + vertical stem
            make_stroke(&[(100., 100.), (96., 128.), (124., 128.)], &style),
            make_stroke(&[(116., 100.), (114., 150.)], &style),
            // "+": vertical + horizontal
            make_stroke(&[(150., 112.), (150., 140.)], &style),
            make_stroke(&[(136., 126.), (164., 126.)], &style),
            // "4"
            make_stroke(&[(184., 100.), (180., 128.), (208., 128.)], &style),
            make_stroke(&[(200., 100.), (198., 150.)], &style),
            // "=" top bar
            make_stroke(&[(228., 118.), (254., 118.)], &style),
        ];
        commit_raw(&mut app, strokes);
        let eq_bot = make_stroke(&[(228., 134.), (256., 134.)], &style);
        commit_raw(&mut app, vec![eq_bot.clone()]);

        let cand = detect_equals_equation(
            app.session(),
            &eq_bot,
            &app.auto_math_generated_ids,
            &app.active_consumed_lhs_ids(),
        )
        .expect("should detect '=' after 4+4");
        assert_eq!(cand.strokes.len(), 8);
        assert!(cand.answer_x > cand.eq_bounds.max.x);

        // Synthesize handwritten "1.7321" and verify strokes + sequential animation timings.
        let ans = synthesize_handwritten_answer(
            "1.7321",
            cand.answer_x,
            &cand.style,
            cand.seed,
            Some((800., 1000.)),
        );
        assert!(ans.strokes.len() >= 6, "multi-character answer should emit handwritten strokes");
        assert_eq!(ans.strokes.len(), ans.reveals.len());
        for s in &ans.strokes {
            assert!(s.bounds().min.x >= cand.eq_bounds.max.x);
            assert_eq!(s.style.color, style.color);
        }
        // Verify staggered overlapping cascade: each digit starts shortly after the previous one
        // starts (without waiting for the previous digit to finish).
        for pair in ans.reveals.windows(2) {
            assert!(
                pair[1].start_ms > pair[0].start_ms,
                "consecutive digits should start with a slight stagger"
            );
            assert!(
                pair[1].start_ms < pair[0].start_ms + pair[0].duration_ms,
                "next digit should start before the previous digit finishes"
            );
        }
        let t0 = Instant::now();
        let anim = AutoMathAnimation {
            note: app.active,
            page: app.page().id,
            started: t0,
            write_end_ms: ans.write_end_ms,
            fade_start_ms: ans.write_end_ms + FADE_HOLD_MS,
            fade_duration_ms: FADE_DURATION_MS,
            strokes: ans.reveals.clone(),
        };
        let mut mid = HashMap::new();
        anim.sample_at(t0 + Duration::from_millis(40), &mut mid);
        assert!(mid[&ans.reveals[0].id].progress > 0.0);
        assert_eq!(mid[&ans.reveals[0].id].yellow_mix, 1.0);
        assert_eq!(mid[&ans.reveals.last().unwrap().id].progress, 0.0);

        let mut fading = HashMap::new();
        anim.sample_at(
            t0 + Duration::from_millis((anim.fade_start_ms + anim.fade_duration_ms / 2) as u64),
            &mut fading,
        );
        assert_eq!(fading[&ans.reveals.last().unwrap().id].progress, 1.0);
        assert!(
            (0.2..0.8).contains(&fading[&ans.reveals[0].id].yellow_mix),
            "ink should smoothly fade from yellow to pen color after writing finishes"
        );
        assert!(anim.is_finished(t0 + Duration::from_millis(anim.total_duration_ms() as u64 + 5)));

        // End-to-end check with bundled stroke model if available.
        if app.recognition_ready() {
            let before_count = app.page().order.len();
            app.maybe_trigger_auto_math(&eq_bot);
            let start = Instant::now();
            while app.page().order.len() == before_count && start.elapsed() < Duration::from_secs(10) {
                app.tick();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                app.page().order.len() > before_count,
                "expected auto-solved handwritten answer strokes to be inserted for 4+4="
            );
            assert!(
                app.status.contains("= 8"),
                "expected status to confirm 4+4 = 8, got: {}",
                app.status
            );

            // Second independent equation on a lower row: "\sqrt{3} =" -> must not merge row 1!
            let row2 = vec![
                // radical sign
                make_stroke(&[(95., 268.), (106., 284.), (120., 238.), (168., 238.)], &style),
                // "3" under radical
                make_stroke(
                    &[
                        (126., 250.),
                        (148., 246.),
                        (148., 262.),
                        (132., 266.),
                        (150., 270.),
                        (150., 284.),
                        (124., 284.),
                    ],
                    &style,
                ),
                // "=" top bar
                make_stroke(&[(186., 260.), (214., 260.)], &style),
            ];
            commit_raw(&mut app, row2);
            let eq2_bot = make_stroke(&[(186., 274.), (216., 274.)], &style);
            commit_raw(&mut app, vec![eq2_bot.clone()]);
            let row2_before = app.page().order.len();
            app.maybe_trigger_auto_math(&eq2_bot);
            let start2 = Instant::now();
            while app.page().order.len() == row2_before && start2.elapsed() < Duration::from_secs(10) {
                app.tick();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                app.page().order.len() > row2_before,
                "expected auto-solved handwritten answer strokes for sqrt(3)="
            );
            assert!(
                app.status.contains("1.7321"),
                "expected status to confirm sqrt(3) = 1.7321, got: {}",
                app.status
            );

            if let Ok(out) = std::env::var("FOLIO_AUTO_MATH_PREVIEW") {
                let _ = folio_export::png(app.page(), &app.assets, std::path::Path::new(&out), 2.0);
            }

            // Hovering over the auto-completed answer strokes exposes the detected LaTeX + vector.
            let answer_center = app
                .page()
                .order
                .iter()
                .skip(row2_before)
                .filter_map(|id| app.page().objects.get(id))
                .map(|o| o.bounds())
                .reduce(|a, b| a.union(b))
                .expect("auto-solved answer bounds")
                .center();
            app.cursor = Some(answer_center);
            let hover = app
                .hovered_auto_math_answer()
                .expect("hovering over auto-completed answer should return detected LaTeX info");
            assert!(
                hover.detected_latex.contains("1.7321"),
                "expected hover LaTeX to contain evaluated answer, got: {}",
                hover.detected_latex
            );
            assert!(
                hover.vector.is_some(),
                "expected hover LaTeX to be pre-typeset as a vector formula"
            );
            app.cursor = Some(Point::new(0., 0.));
            assert!(app.hovered_auto_math_answer().is_none());

            // Single undo removes only the second auto-inserted answer while keeping '=' intact,
            // and immediately clears hover info for the undone answer.
            app.cursor = Some(answer_center);
            app.undo();
            assert_eq!(app.page().order.len(), row2_before);
            assert!(app.hovered_auto_math_answer().is_none());
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn respects_disabled_setting_and_right_side_guard() {
        let root = std::env::temp_dir().join(format!("folio-auto-math-guard-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let style = PenStyle::default();

        // Draw "4+4=8" where something already exists right of '='.
        let strokes = vec![
            make_stroke(&[(100., 100.), (100., 140.)], &style),
            make_stroke(&[(130., 114.), (156., 114.)], &style),
            // Existing stroke immediately right of '='
            make_stroke(&[(175., 100.), (175., 142.)], &style),
        ];
        commit_raw(&mut app, strokes);
        let eq_bot = make_stroke(&[(130., 128.), (156., 128.)], &style);
        commit_raw(&mut app, vec![eq_bot.clone()]);

        assert!(
            detect_equals_equation(
                app.session(),
                &eq_bot,
                &app.auto_math_generated_ids,
                &app.active_consumed_lhs_ids(),
            )
            .is_none(),
            "must not trigger when content already exists to the right of '='"
        );

        app.settings.auto_math_ink = false;
        let before = app.page().order.len();
        app.maybe_trigger_auto_math(&eq_bot);
        assert_eq!(app.page().order.len(), before);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn isolates_tightly_stacked_rows_and_preserves_fractions() {
        let root = std::env::temp_dir().join(format!("folio-auto-math-stack-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let style = PenStyle::default();
        let empty = HashSet::new();

        // Tightly stacked equations (~0.6 * char_h gap, char_h ~ 48px, row pitch ~ 76px):
        // Row 1 (y: 100..150): √3 = 1.7321
        let row1 = vec![
            make_stroke(&[(95., 130.), (106., 148.), (120., 102.), (168., 102.)], &style),
            make_stroke(
                &[(126., 114.), (148., 112.), (148., 128.), (132., 132.), (150., 136.), (150., 148.), (124., 148.)],
                &style,
            ),
            make_stroke(&[(186., 120.), (214., 120.)], &style),
            make_stroke(&[(186., 134.), (216., 134.)], &style),
            // Existing answer ink to the right of Row 1's '='
            make_stroke(&[(240., 102.), (238., 148.)], &style),
        ];
        commit_raw(&mut app, row1);

        // Row 2 (y: 176..220): 2 + 2 = 4 (only 26px below Row 1!)
        let row2 = vec![
            make_stroke(&[(100., 184.), (124., 182.), (126., 196.), (98., 218.), (128., 218.)], &style),
            make_stroke(&[(148., 184.), (146., 218.)], &style),
            make_stroke(&[(134., 200.), (162., 200.)], &style),
            make_stroke(&[(174., 184.), (198., 182.), (200., 196.), (172., 218.), (202., 218.)], &style),
            make_stroke(&[(214., 192.), (238., 198.)], &style), // slightly slanted top bar
            make_stroke(&[(214., 210.), (240., 210.)], &style),
            // Existing answer ink ("4") on Row 2 directly above Row 3's right side
            make_stroke(&[(256., 184.), (254., 204.), (274., 204.)], &style),
            make_stroke(&[(270., 184.), (268., 218.)], &style),
        ];
        commit_raw(&mut app, row2);

        // Row 3 (y: 248..296): √3 = (only 28px below Row 2, with a slanted top '=' bar)
        let row3_lhs = vec![
            make_stroke(&[(95., 278.), (106., 296.), (120., 250.), (168., 250.)], &style),
            make_stroke(
                &[(126., 262.), (148., 260.), (148., 276.), (132., 280.), (150., 284.), (150., 296.), (124., 296.)],
                &style,
            ),
            make_stroke(&[(186., 266.), (210., 274.)], &style), // naturally slanted (~18°)
        ];
        let row3_expected_lhs: HashSet<Id> = row3_lhs[..2].iter().map(|s| s.id).collect();
        commit_raw(&mut app, row3_lhs);
        let row3_eq_bot = make_stroke(&[(188., 288.), (218., 290.)], &style);
        commit_raw(&mut app, vec![row3_eq_bot.clone()]);

        // Pure geometric isolation: even with empty `consumed_lhs`, Row 3 must ONLY gather its own
        // √ and 3 + two '=' bars (4 strokes total), never pulling Row 2 or Row 1.
        let cand = detect_equals_equation(app.session(), &row3_eq_bot, &empty, &empty)
            .expect("Row 3 tightly below Row 2 should still be detected");
        assert_eq!(cand.strokes.len(), 4, "Row 3 must not pull strokes from Row 2 or Row 1");
        assert_eq!(
            cand.lhs_ids.iter().copied().collect::<HashSet<_>>(),
            row3_expected_lhs
        );

        // Row 4 (y: 326..370): 5 - 2 = tightly below Row 3 — '-' must NOT act as a fraction bar
        // and pull Row 3's '3' into Row 4.
        let row4_lhs = vec![
            make_stroke(&[(110., 328.), (96., 328.), (96., 348.), (114., 350.), (114., 368.), (94., 368.)], &style),
            make_stroke(&[(126., 348.), (156., 348.)], &style), // minus bar directly under Row 3's '3'
            make_stroke(&[(168., 330.), (192., 330.), (192., 346.), (166., 368.), (196., 368.)], &style),
            make_stroke(&[(210., 342.), (236., 342.)], &style),
        ];
        commit_raw(&mut app, row4_lhs);
        let row4_eq_bot = make_stroke(&[(210., 356.), (238., 356.)], &style);
        commit_raw(&mut app, vec![row4_eq_bot.clone()]);
        let cand4 = detect_equals_equation(app.session(), &row4_eq_bot, &empty, &empty)
            .expect("Row 4 with minus sign should be detected");
        assert_eq!(
            cand4.strokes.len(),
            5,
            "minus sign must not pull strokes from the row above as a false fraction numerator"
        );

        // Stacked fraction "\frac{1}{2} =" far to the right must still gather numerator + bar + denominator + '='.
        let frac = vec![
            make_stroke(&[(520., 460.), (520., 494.)], &style), // numerator "1"
            make_stroke(&[(500., 508.), (546., 508.)], &style), // fraction bar
            make_stroke(&[(510., 522.), (534., 522.), (534., 536.), (508., 556.), (538., 556.)], &style), // denominator "2"
            make_stroke(&[(564., 500.), (592., 500.)], &style),
        ];
        commit_raw(&mut app, frac);
        let frac_eq_bot = make_stroke(&[(564., 516.), (592., 516.)], &style);
        commit_raw(&mut app, vec![frac_eq_bot.clone()]);
        let frac_cand = detect_equals_equation(app.session(), &frac_eq_bot, &empty, &empty)
            .expect("stacked fraction should be detected");
        assert_eq!(
            frac_cand.strokes.len(),
            5,
            "stacked fraction should include numerator, bar, denominator, and both '=' bars"
        );

        // Nested 3-level fraction `\frac{4}{\frac{4}{4}} =` where '=' aligns with the middle '4'.
        let nested = vec![
            make_stroke(&[(720., 430.), (720., 466.)], &style), // top 4
            make_stroke(&[(700., 478.), (746., 478.)], &style), // upper bar
            make_stroke(&[(720., 492.), (720., 528.)], &style), // middle 4 (aligned with '=')
            make_stroke(&[(700., 540.), (746., 540.)], &style), // lower bar
            make_stroke(&[(720., 552.), (720., 588.)], &style), // bottom 4
            make_stroke(&[(762., 502.), (788., 508.)], &style), // opposing slant top '=' bar
        ];
        commit_raw(&mut app, nested);
        let nested_eq_bot = make_stroke(&[(762., 522.), (788., 516.)], &style); // opposing slant bottom '=' bar
        commit_raw(&mut app, vec![nested_eq_bot.clone()]);
        let nested_cand = detect_equals_equation(app.session(), &nested_eq_bot, &empty, &empty)
            .expect("nested fraction with opposing-slant '=' should be detected");
        assert_eq!(
            nested_cand.strokes.len(),
            7,
            "nested fraction should include all 3 levels, both bars, and both '=' bars"
        );

        // Long bowed/arched fraction bar (outer numerator terms sit lower than the bar's center peak).
        let bowed = vec![
            make_stroke(&[(100., 680.), (100., 716.)], &style), // left outer term (below bar peak y=706!)
            make_stroke(&[(220., 666.), (220., 700.)], &style), // middle term above arch
            make_stroke(&[(340., 680.), (340., 716.)], &style), // right outer term
            make_stroke(&[(86., 728.), (220., 706.), (354., 728.)], &style), // arched fraction bar
            make_stroke(&[(220., 734.), (220., 768.)], &style), // denominator
            make_stroke(&[(372., 718.), (398., 718.)], &style),
        ];
        commit_raw(&mut app, bowed);
        let bowed_eq_bot = make_stroke(&[(372., 732.), (398., 732.)], &style);
        commit_raw(&mut app, vec![bowed_eq_bot.clone()]);
        let bowed_cand = detect_equals_equation(app.session(), &bowed_eq_bot, &empty, &empty)
            .expect("long bowed fraction should be detected");
        assert_eq!(
            bowed_cand.strokes.len(),
            7,
            "bowed fraction bar should keep outer numerator terms via local curve y"
        );

        // Multi-digit exponent `100^{10} =`: both exponent digits must attach.
        let exp_strokes = vec![
            make_stroke(&[(520., 680.), (520., 726.)], &style), // 1
            make_stroke(&[(538., 682.), (538., 724.), (562., 724.), (562., 682.)], &style), // 0
            make_stroke(&[(574., 682.), (574., 724.), (598., 724.), (598., 682.)], &style), // 0
            make_stroke(&[(608., 650.), (608., 676.)], &style), // exponent '1'
            make_stroke(&[(618., 650.), (618., 676.), (636., 676.), (636., 650.)], &style), // exponent '0'
            make_stroke(&[(648., 696.), (674., 696.)], &style),
        ];
        commit_raw(&mut app, exp_strokes);
        let exp_eq_bot = make_stroke(&[(648., 712.), (674., 712.)], &style);
        commit_raw(&mut app, vec![exp_eq_bot.clone()]);
        let exp_cand = detect_equals_equation(app.session(), &exp_eq_bot, &empty, &empty)
            .expect("multi-digit exponent equation should be detected");
        assert_eq!(
            exp_cand.strokes.len(),
            7,
            "both digits of exponent '10' in 100^10= must be gathered"
        );

        // Verify detect_page_equations finds existing two-sided equations (Row 1 and Row 2)
        // and algebraic glyph synthesis emits strokes for `x^2+2x+1`.
        let page_eqs = detect_page_equations(app.session(), exp_cand.eq_ids, &empty);
        assert!(
            page_eqs.len() >= 2,
            "expected two-sided equations on page to be discovered for variable lookup"
        );
        let alg_ans = synthesize_handwritten_answer(
            "x^2+2x+1",
            exp_cand.answer_x,
            &exp_cand.style,
            exp_cand.seed,
            None,
        );
        assert!(
            alg_ans.strokes.len() >= 7,
            "algebraic answer x^2+2x+1 should synthesize handwritten letter & superscript strokes"
        );

        // Multi-stroke cap bars (`T` in `Tan(\frac{\pi}{4}) =` and `\pi` inside fraction numerator)
        // plus standalone `\pi =` with pen-lift gap, compact `=` spacing, and `x = y^2 + 4` in
        // `detect_page_equations`.
        let tan_strokes = vec![
            // T vertical stem + high overhanging top bar
            make_stroke(&[(42., 954.), (38., 993.)], &style),
            make_stroke(&[(26., 952.), (79., 954.)], &style),
            // a
            make_stroke(
                &[(78., 972.), (60., 974.), (52., 988.), (64., 992.), (78., 980.), (84., 990.)],
                &style,
            ),
            // n
            make_stroke(&[(96., 972.), (96., 991.), (104., 972.), (114., 974.), (116., 991.)], &style),
            // (
            make_stroke(&[(154., 946.), (138., 968.), (136., 988.), (152., 994.)], &style),
            // \pi: left leg, right leg, high top bar above fraction bar
            make_stroke(&[(180., 955.), (179., 972.)], &style),
            make_stroke(&[(195., 955.), (194., 970.)], &style),
            make_stroke(&[(176., 952.), (206., 952.)], &style),
            // fraction bar
            make_stroke(&[(163., 983.), (200., 983.)], &style),
            // denominator "4"
            make_stroke(&[(170., 992.), (166., 1012.), (186., 1008.)], &style),
            make_stroke(&[(180., 996.), (178., 1020.)], &style),
            // )
            make_stroke(&[(208., 964.), (218., 984.), (206., 1004.)], &style),
            // '=' top bar (slanted down-right, shorter than bottom bar)
            make_stroke(&[(240., 968.), (255., 973.)], &style),
        ];
        commit_raw(&mut app, tan_strokes);
        let tan_eq_bot = make_stroke(&[(236., 988.), (264., 989.)], &style);
        commit_raw(&mut app, vec![tan_eq_bot.clone()]);
        let tan_cand = detect_equals_equation(app.session(), &tan_eq_bot, &empty, &empty)
            .expect("Tan(pi/4)= should be detected");
        assert_eq!(
            tan_cand.strokes.len(),
            14,
            "Tan(pi/4)= must retain both T's top bar and pi's top bar above the fraction bar"
        );

        // Standalone `\pi =` where pi's top bar sits above `row_core_y0` with a small pen-lift gap,
        // and compact widely-spaced `=` bars (`dy / avg_w ~ 2.0`).
        let pi_strokes = vec![
            make_stroke(&[(437., 964.), (440., 996.)], &style),
            make_stroke(&[(452., 963.), (454., 987.)], &style),
            make_stroke(&[(431., 954.), (472., 952.)], &style), // pi top bar above legs
            make_stroke(&[(486., 966.), (497., 968.)], &style), // compact top '=' bar (w=11)
        ];
        commit_raw(&mut app, pi_strokes);
        let pi_eq_bot = make_stroke(&[(486., 988.), (498., 990.)], &style); // dy=22 (~2.0 * avg_w)
        commit_raw(&mut app, vec![pi_eq_bot.clone()]);
        let pi_cand = detect_equals_equation(app.session(), &pi_eq_bot, &empty, &empty)
            .expect("pi= with high top bar and compact '=' should be detected");
        assert_eq!(
            pi_cand.strokes.len(),
            5,
            "pi= must retain pi's horizontal top bar above both legs"
        );

        // Page equation `x = y^2 + 4` where superscript `2` sits above the row core must still
        // include `^2` in `detect_page_equations`.
        let eq_row_strokes = vec![
            make_stroke(&[(680., 1160.), (710., 1192.)], &style), // x stroke 1
            make_stroke(&[(710., 1160.), (680., 1192.)], &style), // x stroke 2
            make_stroke(&[(736., 1169.), (754., 1169.)], &style), // = top
            make_stroke(&[(735., 1187.), (755., 1187.)], &style), // = bot
            make_stroke(&[(782., 1158.), (794., 1178.)], &style), // y stroke 1
            make_stroke(&[(804., 1158.), (784., 1196.)], &style), // y stroke 2
            make_stroke(&[(814., 1134.), (834., 1134.), (814., 1154.), (836., 1154.)], &style), // high ^2
            make_stroke(&[(856., 1162.), (856., 1196.)], &style), // + vert
            make_stroke(&[(844., 1179.), (870., 1179.)], &style), // + horiz
            make_stroke(&[(886., 1162.), (884., 1182.), (906., 1182.)], &style), // 4 L
            make_stroke(&[(900., 1162.), (898., 1196.)], &style), // 4 stem
        ];
        commit_raw(&mut app, eq_row_strokes);
        let page_eqs_with_sup = detect_page_equations(app.session(), pi_cand.eq_ids, &empty);
        assert!(
            page_eqs_with_sup.iter().any(|eq| eq.strokes.len() == 11),
            "detect_page_equations must retain high superscript '2' in x = y^2 + 4"
        );

        // Statistics canvas patterns:
        // 1) `X ~ B(10, 0.3)` tilde declaration discovered by `detect_page_equations`.
        let tilde_row = vec![
            make_stroke(&[(100., 1360.), (124., 1394.)], &style), // X1
            make_stroke(&[(124., 1360.), (100., 1394.)], &style), // X2
            make_stroke(&[(142., 1378.), (150., 1373.), (158., 1382.), (166., 1377.)], &style), // ~
            make_stroke(&[(184., 1358.), (184., 1396.)], &style), // B stem
            make_stroke(&[(184., 1358.), (202., 1366.), (184., 1377.), (204., 1386.), (184., 1396.)], &style), // B loops
            make_stroke(&[(216., 1358.), (210., 1377.), (216., 1396.)], &style), // (
            make_stroke(&[(228., 1360.), (228., 1394.)], &style), // 1
            make_stroke(&[(240., 1362.), (256., 1362.), (256., 1392.), (240., 1392.), (240., 1362.)], &style), // 0
            make_stroke(&[(266., 1390.), (264., 1398.)], &style), // ,
            make_stroke(&[(278., 1362.), (294., 1362.), (294., 1392.), (278., 1392.), (278., 1362.)], &style), // 0
            make_stroke(&[(302., 1390.), (304., 1392.)], &style), // .
            make_stroke(&[(312., 1362.), (328., 1362.), (316., 1376.), (328., 1388.), (312., 1392.)], &style), // 3
            make_stroke(&[(336., 1358.), (342., 1377.), (336., 1396.)], &style), // )
        ];
        commit_raw(&mut app, tilde_row);
        let page_eqs_with_tilde = detect_page_equations(app.session(), pi_cand.eq_ids, &empty);
        assert!(
            page_eqs_with_tilde.iter().any(|eq| eq.strokes.len() == 13),
            "detect_page_equations must discover `X ~ B(10, 0.3)` tilde declarations"
        );

        // 2) `P(X = 3) =` with an inner `=` inside parentheses must gather the whole expression.
        let prob_eq = vec![
            make_stroke(&[(100., 1460.), (100., 1498.)], &style), // P stem
            make_stroke(&[(100., 1460.), (118., 1468.), (100., 1478.)], &style), // P loop
            make_stroke(&[(132., 1456.), (124., 1479.), (132., 1502.)], &style), // (
            make_stroke(&[(144., 1464.), (164., 1494.)], &style), // X1
            make_stroke(&[(164., 1464.), (144., 1494.)], &style), // X2
            make_stroke(&[(178., 1474.), (198., 1474.)], &style), // inner = top
            make_stroke(&[(178., 1486.), (198., 1486.)], &style), // inner = bot
            make_stroke(&[(212., 1464.), (230., 1464.), (216., 1478.), (230., 1492.), (212., 1494.)], &style), // 3
            make_stroke(&[(240., 1456.), (248., 1479.), (240., 1502.)], &style), // )
            make_stroke(&[(266., 1472.), (290., 1472.)], &style), // outer = top
        ];
        commit_raw(&mut app, prob_eq);
        let prob_eq_bot = make_stroke(&[(266., 1486.), (290., 1486.)], &style);
        commit_raw(&mut app, vec![prob_eq_bot.clone()]);
        let prob_cand = detect_equals_equation(app.session(), &prob_eq_bot, &empty, &empty)
            .expect("P(X = 3) = should be detected");
        assert_eq!(
            prob_cand.strokes.len(),
            11,
            "P(X = 3) = must retain the inner '=' and all strokes inside P(...)"
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
