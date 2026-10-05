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
    fn running(self, now: Instant) -> bool {
        self.from != self.to && now.saturating_duration_since(self.started) < DURATION
    }
}

#[derive(Default)]
pub(super) struct Motion {
    values: HashMap<String, Tween>,
    panels: HashMap<&'static str, String>,
}
impl Motion {
    pub fn value(&mut self, key: &str, target: f32, reduced: bool, now: Instant) -> f32 {
        let tween = self.values.entry(key.into()).or_insert(Tween {
            from: target,
            to: target,
            started: now,
        });
        if reduced {
            tween.from = target;
            tween.to = target;
        } else if tween.to != target {
            *tween = Tween {
                from: tween.sample(now),
                to: target,
                started: now,
            };
        }
        tween.sample(now)
    }
    pub fn hover(&mut self, key: &str, hovered: bool, reduced: bool) {
        // Establish the resting value before starting the first hover transition.
        let key = format!("hover:{key}");
        let now = Instant::now();
        self.values.entry(key.clone()).or_insert(Tween {
            from: 0.,
            to: 0.,
            started: now,
        });
        self.value(&key, if hovered { 1. } else { 0. }, reduced, now);
    }
    pub fn hover_value(&self, key: &str) -> f32 {
        self.values
            .get(&format!("hover:{key}"))
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
        for tween in self.values.values_mut() {
            tween.from = tween.to;
        }
    }
    #[cfg(test)]
    fn running(&self) -> bool {
        self.values.values().any(|t| t.running(Instant::now()))
    }
    pub fn advance(&mut self) -> bool {
        self.advance_at(Instant::now())
    }
    fn advance_at(&mut self, now: Instant) -> bool {
        let mut changed = false;
        for tween in self.values.values_mut() {
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
        if self.values.len() > 512 {
            let now = Instant::now();
            self.values.retain(|_, t| t.running(now));
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
    fn idle_hover_cache_is_bounded() {
        let mut motion = Motion::default();
        for i in 0..600 {
            motion.hover(&i.to_string(), false, false);
        }
        motion.prune();
        assert!(motion.values.len() <= 512);
        assert!(!motion.running());
    }
}
