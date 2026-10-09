//! Finite presentation tweens. No document geometry or input events are delayed.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const DURATION: Duration = Duration::from_millis(160);

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    started: Instant,
}
impl Tween {
    fn sample(self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / DURATION.as_secs_f32())
        .clamp(0., 1.);
        self.from + (self.to - self.from) * (1. - (1. - t).powi(3))
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
            };
        }
        self.sample(now)
    }
    fn running(self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.started) < DURATION
    }
}

#[derive(Default)]
pub(super) struct Motion {
    values: HashMap<String, Tween>,
    hovers: HashMap<String, Tween>,
    panels: HashMap<&'static str, String>,
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
            },
        );
        target
    }
    pub fn hover(&mut self, key: &str, hovered: bool, reduced: bool) {
        let now = Instant::now();
        // Keep hover keys separate so drawing each control can use a borrowed
        // lookup, without allocating a prefixed String every frame.
        if !self.hovers.contains_key(key) {
            self.hovers.insert(
                key.into(),
                Tween {
                    from: 0.,
                    to: 0.,
                    started: now,
                },
            );
        }
        self.hovers
            .get_mut(key)
            .unwrap()
            .retarget(if hovered { 1. } else { 0. }, reduced, now);
    }
    pub fn hover_value(&self, key: &str) -> f32 {
        self.hovers
            .get(key)
            .map_or(0., |t| t.sample(Instant::now()))
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
                Tween {
                    from: 0.35,
                    to: 1.,
                    started: now,
                },
            );
        }
        self.value(key, 1., reduced, now)
    }
    pub fn settle(&mut self) {
        for tween in self.values.values_mut().chain(self.hovers.values_mut()) {
            tween.from = tween.to;
        }
    }
    #[cfg(test)]
    fn running(&self) -> bool {
        self.values
            .values()
            .chain(self.hovers.values())
            .any(|t| t.running(Instant::now()))
    }
    pub fn advance(&mut self) -> bool {
        self.advance_at(Instant::now())
    }
    fn advance_at(&mut self, now: Instant) -> bool {
        let mut changed = false;
        for tween in self.values.values_mut().chain(self.hovers.values_mut()) {
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
            self.hovers.retain(|_, t| t.running(now));
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
        assert_eq!(motion.panel("dialog", Some("open".into()), true), 1.);
        assert!(!motion.running());
    }
    #[test]
    fn completion_requests_one_last_frame_then_stops() {
        let now = Instant::now();
        let mut motion = Motion::default();
        motion.value("switch", 0., false, now);
        motion.value("switch", 1., false, now);
        assert!(motion.advance_at(now + DURATION));
        assert!(!motion.advance_at(now + DURATION));
        assert_eq!(motion.value("switch", 1., false, now + DURATION), 1.);
    }
    #[test]
    fn redraws_do_not_restart_panel_animation_but_reopening_does() {
        let mut motion = Motion::default();
        assert!(motion.panel("dialog", Some("open".into()), false) < 1.);
        motion.values.get_mut("dialog").unwrap().started -= DURATION;
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
