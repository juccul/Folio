//! Explicit local diagnostic recording. Recording is opt-in and bounded; a slow
//! disk drops diagnostic samples rather than interfering with pen input.
use super::*;
use std::{
    io::{BufWriter, Write},
    sync::mpsc,
    thread,
};
pub struct Diagnostics {
    enabled: bool,
    sender: Option<mpsc::SyncSender<String>>,
    join: Option<thread::JoinHandle<()>>,
    pending: Option<Instant>,
    samples: Vec<f64>,
    dropped: u64,
    pub check: Option<InputCheck>,
}
impl Diagnostics {
    pub fn new() -> Self {
        let path = std::env::var_os("FOLIO_PEN_RECORD");
        let (sender, join) = if let Some(path) = path {
            let (tx, rx) = mpsc::sync_channel::<String>(1024);
            let join = thread::spawn(move || {
                let file = match std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(path)
                {
                    Ok(file) => file,
                    Err(e) => {
                        eprintln!("Cannot create pen diagnostics: {e}");
                        return;
                    }
                };
                let mut out = BufWriter::new(file);
                while let Ok(line) = rx.recv() {
                    if writeln!(out, "{line}").is_err() {
                        break;
                    }
                }
                let _ = out.flush();
            });
            (Some(tx), Some(join))
        } else {
            (None, None)
        };
        Self {
            enabled: sender.is_some() || std::env::var_os("FOLIO_PROFILE_INK").is_some(),
            sender,
            join,
            pending: None,
            samples: Vec::new(),
            dropped: 0,
            check: None,
        }
    }
    pub fn pen(&mut self, event: &TabletEvent) {
        if let Some(check) = &mut self.check {
            check.pen(event);
        }
        if !self.enabled && !self.check.as_ref().is_some_and(|check| check.running) {
            return;
        }
        self.pending = Some(Instant::now());
        if let Some(sender) = &self.sender {
            let sample = serde_json::json!({"type":"tablet","phase":format!("{:?}",event.phase),"x":f32::from(event.position.x),"y":f32::from(event.position.y),"pressure":event.pressure,"tilt_x":event.tilt_x,"tilt_y":event.tilt_y,"timestamp":event.timestamp,"buttons":event.buttons,"eraser":event.eraser});
            if sender.try_send(sample.to_string()).is_err() {
                self.dropped += 1;
            }
        }
    }
    pub fn painted(&mut self) {
        if let Some(pending) = self.pending.take() {
            let elapsed = pending.elapsed().as_secs_f64() * 1000.;
            if self.enabled && self.samples.len() < MAX_SAMPLES {
                self.samples.push(elapsed);
            }
            if let Some(check) = &mut self.check
                && check.running
                && check.paint_ms.len() < MAX_SAMPLES
            {
                check.paint_ms.push(elapsed);
            }
        }
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        if !self.samples.is_empty() {
            let mut result = percentiles(&self.samples);
            result["type"] = serde_json::json!("dispatch_to_cpu_paint");
            result["dropped_recordings"] = serde_json::json!(self.dropped);
            result["excludes"] =
                serde_json::json!("tablet transport, GPU present, compositor and display scanout");
            eprintln!("FOLIO_INPUT_METRICS: {result}");
            if let Some(tx) = self.sender.take() {
                let _ = tx.send(result.to_string());
            }
        }
        self.sender.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

const MAX_SAMPLES: usize = 100_000;
/// Bounded aggregate inspection, active only after a user starts a check.
/// Reports delivery to CPU paint, never pen-to-screen latency.
pub struct InputCheck {
    pub running: bool,
    start: Instant,
    elapsed: Option<Duration>,
    phases: [u64; 6],
    contact: bool,
    pressure: Option<(f32, f32)>,
    tilt_max: (f32, f32),
    buttons: u32,
    eraser: u64,
    invalid: u64,
    last_move: Option<u32>,
    intervals_ms: Vec<f64>,
    paint_ms: Vec<f64>,
    pad_events: u64,
    pad_buttons: Vec<u32>,
    rings: u64,
    strips: u64,
}
fn percentiles(samples: &[f64]) -> serde_json::Value {
    if samples.is_empty() {
        return serde_json::Value::Null;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |p: f64| {
        let rank = (sorted.len() - 1) as f64 * p;
        let lower = rank.floor() as usize;
        let upper = rank.ceil() as usize;
        sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64)
    };
    serde_json::json!({"samples":sorted.len(),"p50_ms":at(0.5),"p95_ms":at(0.95),"p99_ms":at(0.99),"max_ms":sorted[sorted.len()-1]})
}
impl InputCheck {
    pub fn new() -> Self {
        Self {
            running: true,
            start: Instant::now(),
            elapsed: None,
            phases: [0; 6],
            contact: false,
            pressure: None,
            tilt_max: (0., 0.),
            buttons: 0,
            eraser: 0,
            invalid: 0,
            last_move: None,
            intervals_ms: Vec::new(),
            paint_ms: Vec::new(),
            pad_events: 0,
            pad_buttons: Vec::new(),
            rings: 0,
            strips: 0,
        }
    }
    pub fn stop(&mut self) {
        self.elapsed = Some(self.start.elapsed());
        self.running = false;
    }
    fn pen(&mut self, event: &TabletEvent) {
        if !self.running {
            return;
        }
        let phase = match event.phase {
            TabletPhase::Hover => 0,
            TabletPhase::Down => 1,
            TabletPhase::Move => 2,
            TabletPhase::Up => 3,
            TabletPhase::Leave => 4,
            TabletPhase::Cancel => 5,
        };
        self.phases[phase] += 1;
        if !event.pressure.is_finite()
            || !(0.0..=1.0).contains(&event.pressure)
            || !event.tilt_x.is_finite()
            || !event.tilt_y.is_finite()
            || !f32::from(event.position.x).is_finite()
            || !f32::from(event.position.y).is_finite()
        {
            self.invalid += 1;
        }
        if event.phase == TabletPhase::Down {
            self.contact = true;
            self.last_move = None;
        }
        if self.contact && event.pressure.is_finite() {
            self.pressure = Some(match self.pressure {
                Some((min, max)) => (min.min(event.pressure), max.max(event.pressure)),
                None => (event.pressure, event.pressure),
            });
        }
        if event.phase == TabletPhase::Move && self.contact {
            if let Some(last) = self.last_move {
                let interval = event.timestamp.wrapping_sub(last);
                // Ignore equal timestamps and discontinuities (device restart or reversal).
                if interval > 0 && interval < 60_000 && self.intervals_ms.len() < MAX_SAMPLES {
                    self.intervals_ms.push(interval as f64);
                }
            }
            self.last_move = Some(event.timestamp);
        }
        if matches!(
            event.phase,
            TabletPhase::Up | TabletPhase::Leave | TabletPhase::Cancel
        ) {
            self.contact = false;
            self.last_move = None;
        }
        if event.tilt_x.is_finite() {
            self.tilt_max.0 = self.tilt_max.0.max(event.tilt_x.abs());
        }
        if event.tilt_y.is_finite() {
            self.tilt_max.1 = self.tilt_max.1.max(event.tilt_y.abs());
        }
        self.buttons |= event.buttons;
        self.eraser += u64::from(event.eraser);
    }
    pub fn pad(&mut self, event: &TabletPadEvent) {
        if !self.running {
            return;
        }
        self.pad_events += 1;
        if let Some(button) = event.button {
            if !self.pad_buttons.contains(&button) && self.pad_buttons.len() < 128 {
                self.pad_buttons.push(button);
            }
        } else if event.strip {
            self.strips += 1;
        } else {
            self.rings += 1;
        }
    }
    pub fn label(&self) -> String {
        let pressure = self
            .pressure
            .map(|(min, max)| format!("{min:.2}–{max:.2}"))
            .unwrap_or_else(|| "waiting for contact".into());
        format!(
            "{} · {} frames · pressure {} · tilt {:.0}°/{:.0}° · eraser {} · pad {}",
            if self.running {
                "Input check"
            } else {
                "Check stopped"
            },
            self.phases.iter().sum::<u64>(),
            pressure,
            self.tilt_max.0,
            self.tilt_max.1,
            self.eraser,
            self.pad_events
        )
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"folio_input_check", "schema_version":1,
            "folio_version":env!("CARGO_PKG_VERSION"), "os":std::env::consts::OS,
            "session_type": if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|s| !s.is_empty()) {"wayland"} else {"x11"},
            "running":self.running, "elapsed_s":self.elapsed.unwrap_or_else(||self.start.elapsed()).as_secs_f64(),
            "tablet":{"hover":self.phases[0],"down":self.phases[1],"move":self.phases[2],"up":self.phases[3],"leave":self.phases[4],"cancel":self.phases[5],
                "contact_pressure_range":self.pressure,"max_abs_tilt_degrees":self.tilt_max,"observed_button_mask":self.buttons,"eraser_frames":self.eraser,"invalid_frames":self.invalid},
            "pad":{"events":self.pad_events,"buttons":self.pad_buttons,"ring_frames":self.rings,"strip_frames":self.strips},
            "contact_move_intervals":percentiles(&self.intervals_ms),
            "dispatch_to_cpu_paint":percentiles(&self.paint_ms),
            "sample_limit":MAX_SAMPLES,
            "timing_limits":"latest delivered tablet event to CPU canvas paint; excludes tablet transport, GPU present, compositor and display scanout; capped samples; not pen-to-screen latency",
            "hardware_validation":"requires physical trial; no pass/fail certification inferred"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    fn event(phase: TabletPhase, timestamp: u32, pressure: f32) -> TabletEvent {
        TabletEvent {
            position: point(px(10.), px(20.)),
            pressure,
            tilt_x: 12.,
            tilt_y: -8.,
            timestamp,
            buttons: 2,
            eraser: true,
            phase,
            modifiers: Modifiers::default(),
        }
    }
    #[test]
    fn check_tracks_contact_wrap_pad_and_freezes_after_stop() {
        let mut check = InputCheck::new();
        check.pen(&event(TabletPhase::Hover, 0, 1.));
        check.pen(&event(TabletPhase::Down, u32::MAX - 8, 0.2));
        check.pen(&event(TabletPhase::Move, u32::MAX - 4, 0.8));
        check.pen(&event(TabletPhase::Move, 3, 0.6));
        check.pen(&event(TabletPhase::Up, 5, 0.));
        check.pad(&TabletPadEvent {
            button: Some(3),
            pressed: true,
            delta: 0.,
            strip: false,
            timestamp: 6,
        });
        check.paint_ms = vec![1., 2., 3.];
        check.stop();
        let frozen = check.report();
        check.pen(&event(TabletPhase::Down, 20, 0.5));
        assert_eq!(frozen, check.report());
        assert_eq!(frozen["tablet"]["contact_pressure_range"][0], 0.);
        assert!(
            (frozen["tablet"]["contact_pressure_range"][1]
                .as_f64()
                .unwrap()
                - 0.8)
                .abs()
                < 1e-6
        );
        assert_eq!(frozen["contact_move_intervals"]["p50_ms"], 8.);
        assert_eq!(frozen["dispatch_to_cpu_paint"]["p95_ms"], 2.9);
        assert_eq!(frozen["pad"]["buttons"], serde_json::json!([3]));
    }
    #[test]
    fn check_reports_absent_device_and_bad_data_without_panics() {
        let mut check = InputCheck::new();
        assert!(check.report()["dispatch_to_cpu_paint"].is_null());
        assert!(check.report()["tablet"]["contact_pressure_range"].is_null());
        check.pen(&event(TabletPhase::Down, 1, f32::NAN));
        check.pen(&event(TabletPhase::Move, 2, 1.2));
        assert_eq!(check.report()["tablet"]["invalid_frames"], 2);
    }
}
