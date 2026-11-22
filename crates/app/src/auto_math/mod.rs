//! Optional iPad Math Notes-style inline auto math solver.
//!
//! When enabled (`Settings::auto_math_ink`), drawing an equals sign (`=`) after a
//! handwritten numeric expression (e.g. `4+4=`, `\sqrt{3}=`) silently recognizes the
//! expression on the background stroke-recognition thread, evaluates numeric answers in
//! pure Rust, and inserts authentic handwritten ink strokes right after `=`.
pub mod detect;
pub mod eval;
pub mod glyphs;

use crate::Controller;
use detect::{EqualsCandidate, detect_equals_equation, right_side_is_clear};
use eval::evaluate_trailing_equals;
use folio_document::*;
use glyphs::{StrokeReveal, synthesize_handwritten_answer};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

/// Record of a previously auto-solved equation on a page, used to isolate closely stacked rows
/// while automatically freeing LHS strokes if the user erases that equation's `=`.
pub(crate) struct SolvedAutoMathRow {
    pub page: Id,
    pub eq_ids: (Id, Id),
    pub lhs_ids: Vec<Id>,
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
}

/// Completed background result delivered back to the controller.
pub(crate) struct AutoMathCompletion {
    pub note: Id,
    pub page: Id,
    pub candidate: EqualsCandidate,
    pub lhs_latex: String,
    pub answer: String,
}

impl AutoMathJob {
    /// Run pure-Rust stroke recognition + numeric evaluation on the worker thread.
    pub(crate) fn run(
        self,
        engine: &mut folio_recognition::Engine,
    ) -> Option<AutoMathCompletion> {
        let recognized = engine.recognize_math(&self.candidate.strokes).ok()?;
        let (lhs_latex, answer) = evaluate_trailing_equals(&recognized, &self.variables)?;
        Some(AutoMathCompletion {
            note: self.note,
            page: self.page,
            candidate: self.candidate,
            lhs_latex,
            answer,
        })
    }
}

fn canonical_pair(a: Id, b: Id) -> (Id, Id) {
    if a <= b { (a, b) } else { (b, a) }
}

impl Controller {
    /// Collect stroke IDs belonging to intact previously solved equations on the current page.
    /// If the user erases an equation's `=`, its LHS strokes automatically become eligible again.
    fn active_consumed_lhs_ids(&self) -> HashSet<Id> {
        let page = self.page();
        let hidden = page.hidden_sources();
        let mut ids = HashSet::new();
        for row in &self.auto_math_solved_rows {
            if row.page == page.id
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
        let job = AutoMathJob {
            pack,
            note: self.active,
            page: self.page().id,
            candidate,
            variables,
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
            let mut changes = Vec::with_capacity(synthesized.strokes.len());
            for (offset, stroke) in synthesized.strokes.into_iter().enumerate() {
                let id = stroke.id;
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
                page: page_id,
                eq_ids: done.candidate.eq_ids,
                lhs_ids: done.candidate.lhs_ids,
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

            // Single undo removes only the second auto-inserted answer while keeping '=' intact.
            app.undo();
            assert_eq!(app.page().order.len(), row2_before);
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

        let _ = std::fs::remove_dir_all(root);
    }
}
