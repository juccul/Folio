//! Finite presentation tweens. No document geometry or input events are delayed.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Springs for Rune-driven motion. `POP` overshoots a touch, for small
/// things that flip or appear; `GLIDE` carries a highlight between controls
/// with a whisper of bounce; `SETTLE` reflows lists and grids.
pub const POP: rune::Spring = rune::Spring::new(0.32, 0.28);
pub const GLIDE: rune::Spring = rune::Spring::new(0.34, 0.14);
pub const SETTLE: rune::Spring = rune::Spring::new(0.42, 0.08);
/// A menu springing to the size of the page it turns into, and back: quick,
/// with a little snap at the end.
pub const MORPH: rune::Spring = rune::Spring::new(0.3, 0.16);
/// Content replacing other content in place arrives quickly, so the gap
/// between the two stays short.
pub const SWAP: rune::Spring = rune::Spring::new(0.24, 0.);
/// Subtle fade for hover highlights entering and leaving across the UI.
pub const HOVER_ENTER: rune::Curve = rune::Curve::tween(0.1, rune::Easing::EASE_OUT);
pub const HOVER_EXIT: rune::Curve = rune::Curve::tween(0.16, rune::Easing::EASE_OUT);
/// Elastic leading and trailing edges for switch knobs: the leading edge runs
/// ahead so the knob stretches smoothly into a pill mid-flight, then bounces
/// gently back into a circle at rest.
pub const SWITCH_LEAD: rune::Spring = rune::Spring::new(0.24, 0.24);
pub const SWITCH_TRAIL: rune::Spring = rune::Spring::new(0.36, 0.18);

const DURATION: Duration = Duration::from_millis(180);
/// Panels settle more slowly, decelerating into place.
const PANEL_DURATION: Duration = Duration::from_millis(240);
/// Hover highlights follow the pointer closely.
const HOVER_DURATION: Duration = Duration::from_millis(120);

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
}
impl Tween {
    fn sample(self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0., 1.);
        // Panels use a quintic ease-out: a quick start that glides to rest.
        let eased = if self.duration == PANEL_DURATION {
            1. - (1. - t).powi(5)
        } else {
            1. - (1. - t).powi(3)
        };
        self.from + (self.to - self.from) * eased
    }
    fn retarget(&mut self, target: f32, reduced: bool, now: Instant) -> f32 {
        if reduced {
            self.from = target;
            self.to = target;
        } else if self.to != target {
            *self = Self {
                from: self.sample(now),
                to: target,
                started: now,
                duration: self.duration,
            };
        }
        self.sample(now)
    }
    fn running(self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.started) < self.duration
    }
}

/// A control's hover tween and the frame that last drew the control.
struct Hover {
    tween: Tween,
    drawn: u64,
}

#[derive(Default)]
pub(super) struct Motion {
    values: HashMap<String, Tween>,
    hovers: HashMap<String, Hover>,
    panels: HashMap<&'static str, String>,
    frame: u64,
}
impl Motion {
    pub fn value(&mut self, key: &str, target: f32, reduced: bool, now: Instant) -> f32 {
        if let Some(tween) = self.values.get_mut(key) {
            return tween.retarget(target, reduced, now);
        }
        self.values.insert(
            key.into(),
            Tween {
                from: target,
                to: target,
                started: now,
                duration: DURATION,
            },
        );
        target
    }
    pub fn hover(&mut self, key: &str, hovered: bool, reduced: bool) {
        let now = Instant::now();
        // Keep hover keys separate so drawing each control can use a borrowed
        // lookup, without allocating a prefixed String every frame.
        let target = if hovered { 1. } else { 0. };
        if let Some(entry) = self.hovers.get_mut(key) {
            entry.tween.retarget(target, reduced, now);
        } else {
            let mut tween = Tween {
                from: 0.,
                to: 0.,
                started: now,
                duration: HOVER_DURATION,
            };
            tween.retarget(target, reduced, now);
            self.hovers.insert(
                key.into(),
                Hover {
                    tween,
                    drawn: self.frame,
                },
            );
        }
    }
    /// The hover amount of a control being drawn in the current frame.
    pub fn hover_value(&mut self, key: &str) -> f32 {
        let frame = self.frame;
        self.hovers.get_mut(key).map_or(0., |h| {
            h.drawn = frame;
            h.tween.sample(Instant::now())
        })
    }
    /// Start a frame, forgetting the hover of controls the previous frame did
    /// not draw. GPUI drops a removed element's hover state without reporting
    /// that the pointer left, so a control shown again (a swatch in a reopened
    /// menu) would otherwise keep the hover tint it had when it disappeared.
    pub fn begin_frame(&mut self) {
        let previous = self.frame;
        self.frame += 1;
        self.hovers.retain(|_, h| h.drawn == previous);
    }
    pub fn panel(&mut self, key: &'static str, signature: Option<String>, reduced: bool) -> f32 {
        let now = Instant::now();
        let Some(signature) = signature else {
            self.panels.remove(key);
            self.values.remove(key);
            return 1.;
        };
        if self.panels.get(key) != Some(&signature) {
            self.panels.insert(key, signature);
            self.values.insert(
                key.into(),
                // Panels fade in from transparent as they slide into place.
                // Switching library views only dips, so the grid never blinks out.
                Tween {
                    from: 0.,
                    to: 1.,
                    started: now,
                    duration: PANEL_DURATION,
                },
            );
        }
        self.value(key, 1., reduced, now)
    }
    pub fn settle(&mut self) {
        for tween in self
            .values
            .values_mut()
            .chain(self.hovers.values_mut().map(|h| &mut h.tween))
        {
            tween.from = tween.to;
        }
    }
    #[cfg(test)]
    fn running(&self) -> bool {
        self.values
            .values()
            .chain(self.hovers.values().map(|h| &h.tween))
            .any(|t| t.running(Instant::now()))
    }
    pub fn advance(&mut self) -> bool {
        self.advance_at(Instant::now())
    }
    pub fn needs_frame(&self) -> bool {
        self.values
            .values()
            .chain(self.hovers.values().map(|h| &h.tween))
            .any(|t| t.from != t.to)
    }
    fn advance_at(&mut self, now: Instant) -> bool {
        let mut changed = false;
        for tween in self
            .values
            .values_mut()
            .chain(self.hovers.values_mut().map(|h| &mut h.tween))
        {
            if tween.from != tween.to {
                changed = true;
                if !tween.running(now) {
                    // Paint the exact endpoint once before becoming idle.
                    tween.from = tween.to;
                }
            }
        }
        changed
    }
    pub fn prune(&mut self) {
        if self.values.len() + self.hovers.len() > 512 {
            let now = Instant::now();
            self.values.retain(|_, t| t.running(now));
            self.hovers.retain(|_, h| h.tween.running(now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversals_are_continuous_and_finish() {
        let start = Instant::now();
        let mut motion = Motion::default();
        assert_eq!(motion.value("switch", 0., false, start), 0.);
        assert_eq!(motion.value("switch", 1., false, start), 0.);
        let halfway = start + DURATION / 2;
        let before = motion.value("switch", 1., false, halfway);
        assert!(before > 0.5 && before < 1.);
        assert_eq!(motion.value("switch", 0., false, halfway), before);
        assert_eq!(motion.value("switch", 0., false, halfway + DURATION), 0.);
    }
    #[test]
    fn reduced_motion_settles_without_frames() {
        let now = Instant::now();
        let mut motion = Motion::default();
        motion.value("switch", 0., false, now);
        assert_eq!(motion.value("switch", 1., true, now), 1.);
        assert!(!motion.running());
        assert!(!motion.needs_frame());
        assert_eq!(motion.panel("dialog", Some("open".into()), true), 1.);
        assert!(!motion.running());
    }
    #[test]
    fn completion_requests_one_last_frame_then_stops() {
        let now = Instant::now();
        let mut motion = Motion::default();
        motion.value("switch", 0., false, now);
        motion.value("switch", 1., false, now);
        assert!(motion.needs_frame());
        motion.values.get_mut("switch").unwrap().started = now - DURATION;
        assert!(!motion.running());
        assert!(
            motion.needs_frame(),
            "Expired tweens still need their endpoint painted"
        );
        assert!(motion.advance_at(now + DURATION));
        assert!(!motion.needs_frame());
        assert!(!motion.advance_at(now + DURATION));
        assert_eq!(motion.value("switch", 1., false, now + DURATION), 1.);
    }
    #[test]
    fn redraws_do_not_restart_panel_animation_but_reopening_does() {
        let mut motion = Motion::default();
        assert!(motion.panel("dialog", Some("open".into()), false) < 1.);
        motion.values.get_mut("dialog").unwrap().started -= PANEL_DURATION;
        assert_eq!(motion.panel("dialog", Some("open".into()), false), 1.);
        assert!(!motion.running());
        motion.panel("dialog", None, false);
        assert!(motion.panel("dialog", Some("open".into()), false) < 1.);
    }
    #[test]
    fn hover_and_value_with_same_control_key_advance_independently() {
        let mut motion = Motion::default();
        let now = Instant::now();
        motion.value("control", 0., false, now);
        motion.value("control", 1., false, now);
        motion.hover("control", true, true);
        assert_eq!(motion.hover_value("control"), 1.);
        assert_eq!(motion.value("control", 1., false, now), 0.);
        motion.settle();
        assert_eq!(motion.value("control", 1., false, now), 1.);
        assert!(!motion.running());
        // A drawn control keeps its hover across frames. One that disappears
        // while hovered (a swatch in a menu its click closes) gets no leave
        // event, so it must not reappear with the old tint.
        motion.hover("swatch", true, true);
        motion.begin_frame();
        assert_eq!(motion.hover_value("control"), 1.);
        motion.begin_frame();
        assert_eq!(motion.hover_value("control"), 1.);
        motion.begin_frame();
        assert_eq!(motion.hover_value("swatch"), 0.);
    }
    #[test]
    fn idle_hover_cache_is_bounded() {
        let mut motion = Motion::default();
        for i in 0..600 {
            motion.hover(&i.to_string(), false, false);
        }
        motion.prune();
        assert!(motion.values.len() + motion.hovers.len() <= 512);
        assert!(!motion.running());
    }
}
