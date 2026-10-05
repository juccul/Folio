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
        }
    }
    pub fn pen(&mut self, event: &TabletEvent) {
        if !self.enabled {
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
        if let Some(pending) = self.pending.take()
            && self.samples.len() < 100_000
        {
            self.samples.push(pending.elapsed().as_secs_f64() * 1000.);
        }
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        if !self.samples.is_empty() {
            self.samples.sort_by(f64::total_cmp);
            let percentile = |p: f64| self.samples[((self.samples.len() - 1) as f64 * p) as usize];
            let result = serde_json::json!({"type":"dispatch_to_cpu_paint","samples":self.samples.len(),"p50_ms":percentile(0.5),"p95_ms":percentile(0.95),"p99_ms":percentile(0.99),"dropped_recordings":self.dropped,"excludes":"tablet transport, GPU present, compositor and display scanout"});
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
