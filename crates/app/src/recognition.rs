//! Explicit, offline recognition of selected ink. Suggestions never mutate a document.
use super::*;
mod cache;
mod timing;
use folio_platform::BackgroundCommand;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
};
pub use timing::RecognitionTiming;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecognitionKind {
    Text,
    Math,
}

#[derive(Clone)]
pub struct RecognitionReview {
    pub kind: RecognitionKind,
    pub text: String,
    pub note: Id,
    pub page: Id,
    pub bounds: Rect,
    pub(super) sources: Vec<Arc<Object>>,
    pub(super) pdf_source: Option<PdfBackground>,
}

#[derive(Deserialize)]
struct Pack {
    python: PathBuf,
    worker: PathBuf,
}

#[derive(Serialize)]
struct Request {
    kind: RecognitionKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    strokes: Vec<Vec<[f32; 2]>>,
}

#[derive(Deserialize)]
struct Response {
    text: Option<String>,
    error: Option<String>,
    restart: Option<bool>,
}

#[derive(Clone)]
struct Task {
    submitted: Instant,
    queued: Instant,
    generation: u64,
    pack: PathBuf,
    review: RecognitionReview,
}

struct ResultMessage {
    generation: u64,
    review: RecognitionReview,
    result: Result<String, String>,
    timing: RecognitionTiming,
}

enum Work {
    Recognize(Task),
    Warmup(PathBuf),
    AutoMath(crate::auto_math::AutoMathJob),
}

struct StatusMessage {
    generation: u64,
    text: String,
}

pub(super) struct Service {
    sender: Option<mpsc::SyncSender<Work>>,
    tx: mpsc::Sender<ResultMessage>,
    auto_math_tx: mpsc::Sender<crate::auto_math::AutoMathCompletion>,
    progress: mpsc::Sender<StatusMessage>,
    wakeup: folio_storage::WakeupSignal,
    idle_timeout: Duration,
    results: mpsc::Receiver<ResultMessage>,
    auto_math_results: mpsc::Receiver<crate::auto_math::AutoMathCompletion>,
    status: mpsc::Receiver<StatusMessage>,
    process: Arc<Mutex<Option<Child>>>,
    generation: Arc<AtomicU64>,
    warming: Arc<AtomicBool>,
    last_intent: Option<u64>,
}

struct Client {
    input: ChildStdin,
    output: mpsc::Receiver<Result<String, String>>,
    pack: PathBuf,
}

impl Service {
    pub fn set_wakeup(&self, wakeup: folio_storage::Wakeup) {
        self.wakeup.set(wakeup);
    }
    pub fn new(_startup_pack: PathBuf) -> Self {
        Self::with_idle_timeout(Duration::from_secs(5 * 60))
    }
    fn with_idle_timeout(idle_timeout: Duration) -> Self {
        let (tx, results) = mpsc::channel();
        let (auto_math_tx, auto_math_results) = mpsc::channel();
        let (progress, status) = mpsc::channel();
        Self {
            sender: None,
            tx,
            auto_math_tx,
            progress,
            wakeup: folio_storage::WakeupSignal::default(),
            idle_timeout,
            results,
            auto_math_results,
            status,
            process: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
            warming: Arc::new(AtomicBool::new(false)),
            last_intent: None,
        }
    }
    fn ensure_started(&mut self) -> Option<&mpsc::SyncSender<Work>> {
        if self.sender.is_some() {
            return self.sender.as_ref();
        }
        // A speculative prewarm, auto-math trigger, and manual request can coexist.
        let (sender, jobs) = mpsc::sync_channel::<Work>(4);
        let tx = self.tx.clone();
        let auto_math_tx = self.auto_math_tx.clone();
        let progress = self.progress.clone();
        let wakeup = self.wakeup.clone();
        let idle_timeout = self.idle_timeout;
        let worker_process = self.process.clone();
        let worker_generation = self.generation.clone();
        let worker_warming = self.warming.clone();
        std::thread::Builder::new()
            .name("folio-recognition".into())
            .spawn(move || {
                let mut client: Option<Client> = None;
                let mut stroke_engine: Option<(PathBuf, folio_recognition::Engine)> = None;
                let mut cached = cache::Results::default();
                let mut last_model_use = Instant::now();
                loop {
                    let resident = client.is_some() || stroke_engine.is_some();
                    if resident && last_model_use.elapsed() >= idle_timeout {
                        stop_process(&worker_process);
                        client = None;
                        stroke_engine = None;
                    }
                    let work = if client.is_some() || stroke_engine.is_some() {
                        match jobs
                            .recv_timeout(idle_timeout.saturating_sub(last_model_use.elapsed()))
                        {
                            Ok(work) => work,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match jobs.recv() {
                            Ok(work) => work,
                            Err(_) => break,
                        }
                    };
                    let task = match work {
                        Work::Recognize(task) => task,
                        Work::AutoMath(job) => {
                            if folio_recognition::Engine::pack_ready(&job.pack) {
                                if stroke_engine
                                    .as_ref()
                                    .is_none_or(|(loaded_pack, _)| loaded_pack != &job.pack)
                                {
                                    if let Ok(engine) = folio_recognition::Engine::open(&job.pack) {
                                        stroke_engine = Some((job.pack.clone(), engine));
                                    }
                                }
                                if let Some((_, engine)) = stroke_engine.as_mut() {
                                    last_model_use = Instant::now();
                                    if let Some(done) = job.run(engine) {
                                        let _ = auto_math_tx.send(done);
                                        wakeup.notify();
                                    }
                                }
                            }
                            continue;
                        }
                        Work::Warmup(pack) => {
                            if folio_recognition::Engine::pack_ready(&pack)
                                && stroke_engine
                                    .as_ref()
                                    .is_none_or(|(loaded_pack, _)| loaded_pack != &pack)
                            {
                                if let Ok(mut engine) = folio_recognition::Engine::open(&pack) {
                                    let _ = engine.ensure_math();
                                    let _ = engine.ensure_htr();
                                    stroke_engine = Some((pack, engine));
                                    last_model_use = Instant::now();
                                }
                            }
                            worker_warming.store(false, Ordering::Release);
                            continue;
                        }
                    };
                    if task.generation != worker_generation.load(Ordering::Acquire) {
                        continue;
                    }
                    let mut timing = RecognitionTiming::new(&task);
                    let report_progress = |text: String| {
                        if progress
                            .send(StatusMessage {
                                generation: task.generation,
                                text,
                            })
                            .is_ok()
                        {
                            wakeup.notify();
                        }
                    };
                    let pack_bytes = std::fs::read(&task.pack).unwrap_or_default();
                    let key = cache::Identity::new(&task, &pack_bytes);
                    let cache_hit = key.as_ref().and_then(|key| cached.get(key));
                    let use_stroke = folio_recognition::Engine::pack_ready(&task.pack);
                    let mut restart = false;
                    let result = if let Some(text) = cache_hit {
                        timing.cache_hit = true;
                        Ok(text)
                    } else {
                        (|| {
                            report_progress("Recognizing writing…".into());
                            if use_stroke {
                                if client.take().is_some() {
                                    stop_process(&worker_process);
                                }
                                let prepared_at = Instant::now();
                                let strokes = request_strokes(&task);
                                timing.preparation_ms +=
                                    prepared_at.elapsed().as_secs_f64() * 1000.;
                                if strokes.is_empty() {
                                    return Err("Select handwriting first".into());
                                }
                                if task.generation != worker_generation.load(Ordering::Acquire) {
                                    return Err("Recognition cancelled".into());
                                }
                                if stroke_engine
                                    .as_ref()
                                    .is_none_or(|(loaded_pack, _)| loaded_pack != &task.pack)
                                {
                                    let loaded_at = Instant::now();
                                    let engine = folio_recognition::Engine::open(&task.pack)?;
                                    stroke_engine = Some((task.pack.clone(), engine));
                                    timing.model_load_ms +=
                                        loaded_at.elapsed().as_secs_f64() * 1000.;
                                }
                                let engine = &mut stroke_engine.as_mut().unwrap().1;
                                let load_at = Instant::now();
                                match task.review.kind {
                                    RecognitionKind::Text => {
                                        engine.ensure_htr()?;
                                    }
                                    RecognitionKind::Math => {
                                        engine.ensure_math()?;
                                    }
                                }
                                timing.model_load_ms += load_at.elapsed().as_secs_f64() * 1000.;
                                if task.generation != worker_generation.load(Ordering::Acquire) {
                                    return Err("Recognition cancelled".into());
                                }
                                let inference_at = Instant::now();
                                let text = match task.review.kind {
                                    RecognitionKind::Text => engine.recognize_text(&strokes)?,
                                    RecognitionKind::Math => engine.recognize_math(&strokes)?,
                                };
                                timing.inference_ms +=
                                    inference_at.elapsed().as_secs_f64() * 1000.;
                                last_model_use = Instant::now();
                                if text.trim().is_empty() {
                                    return Err(
                                        "No text recognized; select a clearer line of writing"
                                            .into(),
                                    );
                                }
                                if text.len() > 32_768 {
                                    return Err("Recognized text is too long".into());
                                }
                                return Ok(text);
                            }

                            stroke_engine = None;
                            if !legacy_pack_available(&task.pack) {
                                return Err(format!(
                                    "Offline handwriting recognition models are missing ({})",
                                    task.pack.display()
                                ));
                            }
                            let prepared_at = Instant::now();
                            let request = Request {
                                kind: task.review.kind,
                                strokes: request_strokes(&task),
                            };
                            timing.preparation_ms += prepared_at.elapsed().as_secs_f64() * 1000.;
                            let alive = worker_process
                                .lock()
                                .unwrap()
                                .as_mut()
                                .is_some_and(|p| p.try_wait().ok().flatten().is_none());
                            if !alive || client.as_ref().is_none_or(|c| c.pack != task.pack) {
                                restart = true;
                                let loaded_at = Instant::now();
                                stop_process(&worker_process);
                                client = None;
                                let pack: Pack =
                                    serde_json::from_slice(&std::fs::read(&task.pack).map_err(
                                        |e| format!("Cannot read recognition pack: {e}"),
                                    )?)
                                    .map_err(|e| format!("Invalid recognition pack: {e}"))?;
                                let root = task.pack.parent().unwrap();
                                let absolute =
                                    |p: PathBuf| if p.is_absolute() { p } else { root.join(p) };
                                let mut child = folio_platform::command(absolute(pack.python))
                                    .arg("-u")
                                    .arg(absolute(pack.worker))
                                    .arg("--config")
                                    .arg(&task.pack)
                                    .env("HF_HUB_OFFLINE", "1")
                                    .env("TRANSFORMERS_OFFLINE", "1")
                                    .env("HF_HUB_DISABLE_TELEMETRY", "1")
                                    .env("TOKENIZERS_PARALLELISM", "false")
                                    .stdin(Stdio::piped())
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::inherit())
                                    .spawn_background()
                                    .map_err(|e| {
                                        format!("Cannot start offline recognition: {e}")
                                    })?;
                                let input = child.stdin.take().unwrap();
                                let stdout = child.stdout.take().unwrap();
                                *worker_process.lock().unwrap() = Some(child);
                                let (lines_tx, output) = mpsc::sync_channel(1);
                                std::thread::spawn(move || {
                                    let mut reader = BufReader::new(stdout);
                                    loop {
                                        let mut line = Vec::new();
                                        let result = reader
                                            .by_ref()
                                            .take(1_048_577)
                                            .read_until(b'\n', &mut line);
                                        let value = match result {
                                            Ok(0) => {
                                                Err("Recognition worker exited; try again".into())
                                            }
                                            Ok(_) if line.len() > 1_048_576 => {
                                                Err("Recognition response is too large".into())
                                            }
                                            Ok(_) => {
                                                String::from_utf8(line).map_err(|e| e.to_string())
                                            }
                                            Err(e) => Err(e.to_string()),
                                        };
                                        let failed = value.is_err();
                                        if lines_tx.send(value).is_err() || failed {
                                            break;
                                        }
                                    }
                                });
                                timing.model_load_ms += loaded_at.elapsed().as_secs_f64() * 1000.;
                                client = Some(Client {
                                    input,
                                    output,
                                    pack: task.pack.clone(),
                                });
                            }
                            if task.generation != worker_generation.load(Ordering::Acquire) {
                                stop_process(&worker_process);
                                return Err("Recognition cancelled".into());
                            }
                            restart = true;
                            let inference_at = Instant::now();
                            let c = client.as_mut().unwrap();
                            serde_json::to_writer(&mut c.input, &request)
                                .map_err(|e| e.to_string())?;
                            c.input
                                .write_all(b"\n")
                                .and_then(|_| c.input.flush())
                                .map_err(|e| e.to_string())?;
                            let line = c
                                .output
                                .recv_timeout(Duration::from_secs(180))
                                .map_err(|_| {
                                    "Recognition timed out or stopped; try a smaller selection"
                                        .to_string()
                                })??;
                            let response: Response = serde_json::from_str(&line)
                                .map_err(|e| format!("Invalid recognition response: {e}"))?;
                            timing.inference_ms += inference_at.elapsed().as_secs_f64() * 1000.;
                            last_model_use = Instant::now();
                            restart = false;
                            if let Some(error) = response.error {
                                restart = response.restart.unwrap_or(true);
                                return Err(error);
                            }
                            let text = response.text.unwrap_or_default();
                            if text.trim().is_empty() {
                                return Err(
                                    "No text recognized; select a clearer line of writing".into()
                                );
                            }
                            if text.len() > 32_768 {
                                return Err("Recognized text is too long".into());
                            }
                            Ok(text)
                        })()
                        .map(|text| normalize_digit_spacing(text, &task.review.sources))
                    };
                    if restart {
                        stop_process(&worker_process);
                        client = None;
                    }
                    if let (Some(key), Ok(text)) = (key, &result)
                        && !timing.cache_hit
                        && task.generation == worker_generation.load(Ordering::Acquire)
                    {
                        cached.insert(key, text.clone());
                    }
                    timing.trace(if result.is_ok() {
                        "worker-complete"
                    } else {
                        "worker-error"
                    });
                    if tx
                        .send(ResultMessage {
                            generation: task.generation,
                            review: task.review,
                            result,
                            timing,
                        })
                        .is_err()
                    {
                        break;
                    }
                    wakeup.notify();
                }
                stop_process(&worker_process);
            })
            .expect("recognition worker thread");
        self.sender = Some(sender);
        self.sender.as_ref()
    }
    fn submit(&mut self, mut task: Task) -> Result<(), String> {
        task.queued = Instant::now();
        self.ensure_started()
            .ok_or("Recognition worker stopped")?
            .try_send(Work::Recognize(task))
            .map_err(|_| "Recognition worker is busy; try again".to_string())?;
        Ok(())
    }
    pub(crate) fn submit_auto_math(&mut self, job: crate::auto_math::AutoMathJob) {
        if let Some(sender) = self.ensure_started() {
            let _ = sender.try_send(Work::AutoMath(job));
        }
    }
    pub(crate) fn try_recv_auto_math(&self) -> Option<crate::auto_math::AutoMathCompletion> {
        self.auto_math_results.try_recv().ok()
    }
    fn prewarm(&mut self, pack: PathBuf, intent: u64) {
        if self.last_intent == Some(intent) || self.warming.load(Ordering::Acquire) {
            return;
        }
        if !folio_recognition::Engine::pack_ready(&pack) {
            self.last_intent = Some(intent);
            return;
        }
        if self
            .warming
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        if self
            .ensure_started()
            .is_some_and(|sender| sender.try_send(Work::Warmup(pack)).is_ok())
        {
            self.last_intent = Some(intent);
        } else {
            self.warming.store(false, Ordering::Release);
        }
    }
    fn cancel(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
        stop_process(&self.process);
    }
}

/// Repair a numeric handwriting line only when its ink can be aligned to every digit.
/// Wide gaps remain separators; uncertain layouts retain the model's suggestion.
fn normalize_digit_spacing(text: String, sources: &[Arc<Object>]) -> String {
    if !text.contains(' ')
        || !text.bytes().all(|b| b.is_ascii_digit() || b == b' ')
        || sources.is_empty()
    {
        return text;
    }
    let digit_count = text.bytes().filter(u8::is_ascii_digit).count();
    if digit_count < 2 {
        return text;
    }
    let mut bounds = Vec::with_capacity(sources.len());
    for source in sources {
        let Object::Stroke(stroke) = source.as_ref() else {
            return text;
        };
        if stroke.display_path().is_empty() {
            continue;
        }
        let mut valid = true;
        let bounds_for_stroke = Rect::from_points(stroke.display_path().iter().map(|p| {
            let point = stroke.transform.apply(p.position);
            valid &= point.x.is_finite() && point.y.is_finite();
            point
        }));
        if !valid {
            return text;
        }
        bounds.push(bounds_for_stroke);
    }
    bounds.sort_unstable_by(|a, b| a.min.x.total_cmp(&b.min.x));
    let mut digits: Vec<Rect> = Vec::with_capacity(bounds.len());
    for bounds in bounds {
        // A digit may use several strokes, such as the crossbar of a handwritten 4.
        if let Some(previous) = digits.last_mut()
            && bounds.min.x <= previous.max.x
        {
            previous.max.x = previous.max.x.max(bounds.max.x);
            previous.min.y = previous.min.y.min(bounds.min.y);
            previous.max.y = previous.max.y.max(bounds.max.y);
        } else {
            digits.push(bounds);
        }
    }
    if digits.len() != digit_count {
        return text;
    }
    let top = digits.iter().map(|b| b.min.y).fold(f32::INFINITY, f32::min);
    let bottom = digits
        .iter()
        .map(|b| b.max.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let height = bottom - top;
    let shared_top = digits
        .iter()
        .map(|b| b.min.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let shared_bottom = digits.iter().map(|b| b.max.y).fold(f32::INFINITY, f32::min);
    let shortest = digits
        .iter()
        .map(|b| b.height())
        .fold(f32::INFINITY, f32::min);
    if !height.is_finite()
        || height <= 0.
        || shortest < height * 0.4
        || shared_bottom - shared_top < shortest * 0.5
    {
        return text;
    }
    let mut result = String::with_capacity(text.len());
    let mut seen = 0;
    for character in text.chars() {
        if character.is_ascii_digit() {
            seen += 1;
        } else if seen > 0
            && seen < digit_count
            && digits[seen].min.x - digits[seen - 1].max.x <= height * 0.35
        {
            continue;
        }
        result.push(character);
    }
    result
}

fn request_strokes(task: &Task) -> Vec<Vec<[f32; 2]>> {
    task.review
        .sources
        .iter()
        .filter_map(|object| {
            let Object::Stroke(stroke) = object.as_ref() else {
                return None;
            };
            Some(
                stroke
                    .display_path()
                    .iter()
                    .map(|point| {
                        let point = stroke.transform.apply(point.position);
                        [point.x, point.y]
                    })
                    .collect(),
            )
        })
        .collect()
}

fn legacy_pack_available(path: &std::path::Path) -> bool {
    let Some(config) = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return false;
    };
    let root = path.parent().unwrap_or(std::path::Path::new("."));
    let resolve = |key: &str| config[key].as_str().map(|value| root.join(value));
    resolve("python").is_some_and(|p| p.is_file())
        && resolve("worker").is_some_and(|p| p.is_file())
}

fn pack_usable(path: &std::path::Path) -> bool {
    folio_recognition::Engine::pack_ready(path) || legacy_pack_available(path)
}

fn stop_process(process: &Mutex<Option<Child>>) {
    let mut guard = process.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.cancel(u64::MAX);
        self.sender.take();
    }
}

impl Controller {
    /// Preload the bundled offline stroke models on hover over a recognition button.
    pub fn prepare_recognition(&mut self) {
        use std::hash::{Hash, Hasher};
        if self.recognition_pending
            || self.interaction.is_some()
            || self.session().selection.len() > 4096
        {
            return;
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.active.hash(&mut hash);
        self.page().id.hash(&mut hash);
        let mut count = 0;
        let mut points = 0usize;
        for object in self.selected_objects().take(4097) {
            let Object::Stroke(stroke) = object.as_ref() else {
                continue;
            };
            points = points.saturating_add(stroke.display_path().len());
            if points > 250_000 {
                return;
            }
            (object.id(), Arc::as_ptr(object) as usize).hash(&mut hash);
            count += 1;
        }
        if count == 0 || count > 4096 {
            return;
        }
        self.recognition_service
            .prewarm(self.recognition_pack(), hash.finish());
    }
    pub fn recognition_frame_pending(&self) -> Option<u64> {
        self.recognition_timing
            .as_ref()
            .filter(|timing| timing.review_ms.is_some() && timing.frame_ms.is_none())
            .map(|timing| timing.generation)
    }
    pub fn recognition_frame_completed(&mut self, request_id: u64) {
        if request_id != self.recognition_generation {
            return;
        }
        if let Some(timing) = self.recognition_timing.as_mut()
            && timing.generation == request_id
            && timing.frame_ms.is_none()
        {
            timing.frame_ms = Some(timing.submitted.elapsed().as_secs_f64() * 1000.);
            timing.trace("frame-complete");
        }
    }
    pub fn recognition_ready(&self) -> bool {
        pack_usable(&self.recognition_pack())
    }
    pub(crate) fn recognition_pack(&self) -> PathBuf {
        pack_path(&self.data_dir)
    }
}

pub(super) fn pack_path(data_dir: &std::path::Path) -> PathBuf {
    if let Some(path) = std::env::var_os("FOLIO_RECOGNITION_CONFIG") {
        return path.into();
    }
    let local = data_dir.join("recognition/pack.json");
    if pack_usable(&local) {
        return local;
    }
    if let Ok(exe) = std::env::current_exe() {
        // Linux tarball / Flatpak / distro layout: <prefix>/bin/folio -> <prefix>/recognition/pack.json
        if let Some(root) = exe.parent().and_then(|p| p.parent()) {
            let pack = root.join("recognition/pack.json");
            if pack_usable(&pack) {
                return pack;
            }
        }
        // Windows portable / installer layout: <prefix>/folio.exe -> <prefix>/recognition/pack.json
        if let Some(dir) = exe.parent() {
            let pack = dir.join("recognition/pack.json");
            if pack_usable(&pack) {
                return pack;
            }
        }
    }
    // Workspace checkout fallback for `cargo run` / `cargo test`.
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../recognition/pack.json");
    if pack_usable(&workspace) {
        return workspace;
    }
    local
}

impl Controller {
    pub fn can_recognize_selection(&self) -> bool {
        let hidden = self.hidden_selection();
        self.session().selection.iter().any(|id| {
            !hidden.contains(id)
                && self
                    .page()
                    .objects
                    .get(id)
                    .is_some_and(|o| matches!(o.as_ref(), Object::Stroke(_)))
        })
    }
    fn recognition_snapshot(&self, kind: RecognitionKind) -> Result<RecognitionReview, String> {
        let hidden = self.hidden_selection();
        let sources: Vec<_> = self
            .selected_objects()
            .filter(|o| !hidden.contains(&o.id()) && matches!(o.as_ref(), Object::Stroke(_)))
            .cloned()
            .collect();
        let bounds = sources
            .iter()
            .map(|o| o.bounds())
            .reduce(Rect::union)
            .ok_or("Select handwriting first")?;
        let count = sources
            .iter()
            .map(|o| match o.as_ref() {
                Object::Stroke(s) => s.display_path().len(),
                _ => 0,
            })
            .sum::<usize>();
        if sources.len() > 4096 || count > 250_000 {
            return Err("Select a smaller passage of writing".into());
        }
        Ok(RecognitionReview {
            kind,
            text: String::new(),
            note: self.active,
            page: self.page().id,
            bounds,
            pdf_source: None,
            sources,
        })
    }
    pub fn recognize_selection(&mut self, kind: RecognitionKind) -> Result<(), String> {
        let submitted = Instant::now();
        self.recognition_for_index = false;
        self.finish();
        if self.recognition_pending {
            return Err("Recognition is already running".into());
        }
        let pack = self.recognition_pack();
        let review = self.recognition_snapshot(kind)?;
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_timing = None;
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service.submit(Task {
            submitted,
            queued: Instant::now(),
            generation: self.recognition_generation,
            pack,
            review,
        })?;
        self.recognition_review = None;
        self.recognition_pending = true;
        self.recognition_status = "Recognizing writing…".into();
        Ok(())
    }
    pub fn cancel_recognition(&mut self) {
        self.recognition_service.last_intent = None;
        self.recognition_for_index = false;
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_timing = None;
        self.recognition_service.cancel(self.recognition_generation);
        self.recognition_pending = false;
        self.recognition_status.clear();
        self.recognition_replacing = false;
        self.recognition_review = None;
    }
    pub(super) fn recognize_math_sources(
        &mut self,
        sources: Vec<Arc<Object>>,
        bounds: Rect,
    ) -> Result<(), String> {
        let submitted = Instant::now();
        if sources.len() > 4096
            || sources
                .iter()
                .map(|o| match o.as_ref() {
                    Object::Stroke(s) => s.display_path().len(),
                    _ => 0,
                })
                .sum::<usize>()
                > 250_000
        {
            return Err("Live region is too large".into());
        }
        if self.recognition_pending {
            return Err("Recognition is already running".into());
        }
        let review = RecognitionReview {
            kind: RecognitionKind::Math,
            text: String::new(),
            note: self.active,
            page: self.page().id,
            bounds,
            pdf_source: None,
            sources,
        };
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_timing = None;
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service.submit(Task {
            submitted,
            queued: Instant::now(),
            generation: self.recognition_generation,
            pack: self.recognition_pack(),
            review,
        })?;
        self.recognition_for_index = false;
        self.recognition_pending = true;
        self.recognition_status = "Recognizing writing…".into();
        self.recognition_review = None;
        Ok(())
    }
    pub(super) fn recognition_is_current(&self, review: &RecognitionReview) -> bool {
        let hidden = self.page().hidden_sources();
        review.note == self.active
            && review.page == self.page().id
            && review
                .pdf_source
                .as_ref()
                .is_none_or(|pdf| self.page().properties.pdf.as_ref() == Some(pdf))
            && review.sources.iter().all(|old| {
                !hidden.contains(&old.id())
                    && self
                        .page()
                        .objects
                        .get(&old.id())
                        .is_some_and(|current| current == old)
            })
    }
    pub(super) fn poll_recognition(&mut self) -> bool {
        let mut changed = false;
        while let Ok(message) = self.recognition_service.status.try_recv() {
            if self.recognition_pending && message.generation == self.recognition_generation {
                self.recognition_status = message.text;
                changed = true;
            }
        }
        while let Ok(mut message) = self.recognition_service.results.try_recv() {
            if message.generation != self.recognition_generation {
                continue;
            }
            changed = true;
            self.recognition_pending = false;
            if !self.recognition_is_current(&message.review) {
                message.timing.trace("stale-result");
                self.error = Some("Writing changed or another page opened; select the writing and recognize again".into());
                continue;
            }
            message.timing.review_ms =
                Some(message.timing.submitted.elapsed().as_secs_f64() * 1000.);
            message.timing.trace("controller-complete");
            self.recognition_timing = Some(message.timing);
            match message.result {
                Ok(text) => {
                    self.recognition_review = Some(RecognitionReview {
                        text,
                        ..message.review
                    })
                }
                Err(error) => {
                    self.recognition_for_index = false;
                    self.error = Some(error);
                }
            }
        }
        changed
    }
    /// Text replaces immediately; math renders first, then commits one history entry.
    pub fn replace_recognized_writing(&mut self, text: String) -> Result<(), String> {
        self.finish();
        if self.recognition_pending {
            return Err("Recognition or equation rendering is already running".into());
        }
        let review = self
            .recognition_review
            .as_ref()
            .ok_or("No recognition result to replace")?
            .clone();
        if !self.recognition_is_current(&review) {
            return Err("The source writing changed; recognize it again before replacing".into());
        }
        if review
            .sources
            .iter()
            .any(|o| !matches!(o.as_ref(), Object::Stroke(_)))
            || review.sources.is_empty()
        {
            return Err("Replacement is only supported for handwritten ink".into());
        }
        if text.trim().is_empty() || text.len() > 32_768 {
            return Err("Enter 1–32768 bytes of text".into());
        }
        if review.kind == RecognitionKind::Math {
            let text = text.trim().to_string();
            folio_math::validate_latex(&text).map_err(|error| error.to_string())?;
            let review = RecognitionReview { text, ..review };
            self.workers.submit(Job::RecognizedEquation {
                generation: self.recognition_generation,
                review,
            })?;
            self.busy += 1;
            self.recognition_pending = true;
            self.recognition_status = "Rendering equation…".into();
            self.recognition_replacing = true;
            self.recognition_review = None;
            return Ok(());
        }
        let bounds = review.bounds;
        let color = review
            .sources
            .iter()
            .find_map(|o| match o.as_ref() {
                Object::Stroke(s) => Some(s.style.color),
                _ => None,
            })
            .unwrap_or(self.style.color);
        let lines = text.lines().count().max(1) as f32;
        let font_size = (bounds.height() / (lines * 1.4)).clamp(12., 32.);
        let width = bounds.width().max(160.).max(
            text.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f32 * font_size * 0.6,
        );
        let object = Object::Text(TextBlock {
            id: Id::new_v4(),
            text,
            rect: Rect::new(
                bounds.min.x,
                bounds.min.y,
                width,
                (lines * font_size * 1.4).max(bounds.height()),
            ),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size,
            color,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        });
        self.commit_recognized_replacement(&review, object);
        Ok(())
    }

    pub(super) fn finish_recognized_equation(
        &mut self,
        generation: u64,
        review: RecognitionReview,
        result: Result<Equation, String>,
    ) {
        if generation != self.recognition_generation {
            return;
        }
        self.recognition_pending = false;
        self.recognition_status.clear();
        self.recognition_replacing = false;
        if !self.recognition_is_current(&review) {
            self.error = Some(
                "Writing changed or another page opened; recognize again before replacing".into(),
            );
            return;
        }
        match result {
            Ok(equation) => self.commit_recognized_replacement(&review, Object::Equation(equation)),
            Err(error) => {
                self.error = Some(error);
                self.recognition_review = Some(review);
            }
        }
    }

    fn commit_recognized_replacement(&mut self, review: &RecognitionReview, object: Object) {
        let ids = review.sources.iter().map(|object| object.id()).collect();
        let mut changes = self.object_changes(&ids, |_| None);
        let index = changes
            .iter()
            .filter_map(|change| match change {
                Change::Object { index, .. } => Some(*index),
                _ => None,
            })
            .min()
            .expect("Current recognition has source writing");
        let id = object.id();
        changes.push(Change::Object {
            page: self.page().id,
            id,
            before: None,
            after: Some(Arc::new(object)),
            index,
        });
        self.commit(
            if review.kind == RecognitionKind::Math {
                "Replace writing with equation"
            } else {
                "Replace writing with text"
            },
            changes,
        );
        self.session_mut().selection = HashSet::from([id]);
        self.recognition_review = None;
    }
}

#[cfg(test)]
mod tests;
