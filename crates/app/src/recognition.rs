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
    #[serde(skip_serializing_if = "Option::is_none")]
    image_path: Option<PathBuf>,
}
#[derive(Deserialize)]
struct Response {
    text: Option<String>,
    error: Option<String>,
    restart: Option<bool>,
}
struct TemporaryImage(Option<PathBuf>);
impl Drop for TemporaryImage {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}
#[derive(Clone)]
struct Task {
    submitted: Instant,
    queued: Instant,
    generation: u64,
    pack: PathBuf,
    review: RecognitionReview,
    image: Option<ImageSource>,
    auto_setup: bool,
}
#[derive(Clone)]
enum ImageSource {
    Object(Arc<Object>, PathBuf),
    Pdf(PdfBackground, PathBuf, Rect),
}
struct ResultMessage {
    generation: u64,
    review: RecognitionReview,
    result: Result<String, String>,
    timing: RecognitionTiming,
}

enum Work {
    Recognize(Task),
    Warmup(PathBuf, u64),
}
struct StatusMessage {
    generation: u64,
    text: String,
}
pub(super) struct Service {
    sender: Option<mpsc::SyncSender<Work>>,
    deferred: Option<Task>,
    last_task: Option<Task>,
    results: mpsc::Receiver<ResultMessage>,
    status: mpsc::Receiver<StatusMessage>,
    process: Arc<Mutex<Option<Child>>>,
    generation: Arc<AtomicU64>,
    startup_process: Arc<Mutex<Option<Child>>>,
    shutdown: Arc<AtomicU64>,
    warm_cancel: Arc<AtomicU64>,
    warming: Arc<AtomicBool>,
    last_intent: Option<u64>,
}
struct Client {
    input: ChildStdin,
    output: mpsc::Receiver<Result<String, String>>,
    pack: PathBuf,
}
impl Service {
    pub fn new(startup_pack: PathBuf) -> Self {
        Self::with_idle_timeout(startup_pack, Duration::from_secs(5 * 60))
    }
    fn with_idle_timeout(startup_pack: PathBuf, idle_timeout: Duration) -> Self {
        Self::with_preparation(startup_pack, idle_timeout, native::Cache::default())
    }
    fn with_preparation(
        startup_pack: PathBuf,
        idle_timeout: Duration,
        mut preparation: native::Cache,
    ) -> Self {
        // A speculative load and one real request can coexist; real requests are never
        // rejected just because selection-triggered preparation occupied the queue.
        let (sender, jobs) = mpsc::sync_channel::<Work>(2);
        let (tx, results) = mpsc::channel();
        let (progress, status) = mpsc::channel();
        let process: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let generation = Arc::new(AtomicU64::new(0));
        let startup_process = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicU64::new(0));
        let warm_cancel = Arc::new(AtomicU64::new(0));
        let warming = Arc::new(AtomicBool::new(false));
        let worker_process = process.clone();
        let worker_generation = generation.clone();
        let worker_startup_process = startup_process.clone();
        let worker_shutdown = shutdown.clone();
        let worker_warm_cancel = warm_cancel.clone();
        let worker_warming = warming.clone();
        std::thread::Builder::new()
            .name("folio-recognition".into())
            .spawn(move || {
                let mut client: Option<Client> = None;
                let mut native_client: Option<native::Native> = None;
                let mut cached = cache::Results::default();
                let mut last_model_use = Instant::now();
                let startup_context = native::Context {
                    generation: 0,
                    current: &worker_shutdown,
                    process: &worker_startup_process,
                    progress: &|_| {},
                };
                if let Err(error) = preparation.prepare_existing(&startup_pack, &startup_context) {
                    eprintln!("Folio OCR startup preparation: {error}");
                }
                stop_process(&worker_startup_process);
                loop {
                    let resident = client.is_some() || native_client.is_some();
                    if resident && last_model_use.elapsed() >= idle_timeout {
                        stop_process(&worker_process);
                        client = None;
                        native_client = None;
                    }
                    let work = if client.is_some() || native_client.is_some() {
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
                        Work::Warmup(pack, epoch) => {
                            let context = native::Context {
                                generation: epoch,
                                current: &worker_warm_cancel,
                                process: &worker_process,
                                progress: &|_| {},
                            };
                            let alive = worker_process
                                .lock()
                                .unwrap()
                                .as_mut()
                                .is_some_and(|process| matches!(process.try_wait(), Ok(None)));
                            // A normal request does not cancel useful loading. Only Cancel/Drop
                            // advances this independent epoch. Prewarming never installs assets.
                            if context.check().is_ok()
                                && (native_client
                                    .as_ref()
                                    .is_none_or(|client| client.pack != pack)
                                    || !alive)
                            {
                                stop_process(&worker_process);
                                client = None;
                                if preparation.prepare_existing(&pack, &context).is_err()
                                    || !preparation.is_prepared(&pack)
                                {
                                    worker_warming.store(false, Ordering::Release);
                                    continue;
                                }
                                match native::Native::load(&pack, &mut preparation, &context) {
                                    Ok(loaded) => {
                                        native_client = Some(loaded);
                                        last_model_use = Instant::now();
                                    }
                                    Err(_) => {
                                        stop_process(&worker_process);
                                        native_client = None;
                                    }
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
                        let _ = progress.send(StatusMessage {
                            generation: task.generation,
                            text,
                        });
                    };
                    let context = native::Context {
                        generation: task.generation,
                        current: &worker_generation,
                        process: &worker_process,
                        progress: &report_progress,
                    };
                    let pack_bytes = std::fs::read(&task.pack).unwrap_or_default();
                    let key = cache::Identity::new(&task, &pack_bytes);
                    let cache_hit = key.as_ref().and_then(|key| cached.get(key));
                    let native_pack = serde_json::from_slice::<serde_json::Value>(&pack_bytes)
                        .ok()
                        .is_some_and(|config| config["backend"] == "llama-vulkan");
                    let use_native =
                        native_pack || (task.auto_setup && !legacy_pack_available(&task.pack));
                    let mut restart = false;
                    let result = if let Some(text) = cache_hit {
                        timing.cache_hit = true;
                        Ok(text)
                    } else {
                        (|| {
                            if use_native {
                                let prepared_at = Instant::now();
                                let input = native::Input::prepare(&task)?;
                                timing.preparation_ms +=
                                    prepared_at.elapsed().as_secs_f64() * 1000.;
                                context.check()?;
                                if client.take().is_some() {
                                    stop_process(&worker_process);
                                }
                                let alive =
                                    worker_process.lock().unwrap().as_mut().is_some_and(
                                        |process| matches!(process.try_wait(), Ok(None)),
                                    );
                                if native_client
                                    .as_ref()
                                    .is_none_or(|client| client.pack != task.pack)
                                    || !alive
                                {
                                    restart = true;
                                    let loaded_at = Instant::now();
                                    stop_process(&worker_process);
                                    native_client = Some(native::Native::load(
                                        &task.pack,
                                        &mut preparation,
                                        &context,
                                    )?);
                                    timing.model_load_ms +=
                                        loaded_at.elapsed().as_secs_f64() * 1000.;
                                }
                                let loaded_before_inference = timing.model_load_ms;
                                let inference_at = Instant::now();
                                let mut result = native_client
                                    .as_ref()
                                    .unwrap()
                                    .recognize_prepared(&input, &context, Some(&mut timing));
                                while result.as_ref().is_err_and(|error| error.restart)
                                    && native_client.as_ref().unwrap().is_gpu()
                                    && context.check().is_ok()
                                {
                                    report_progress(
                                        "GPU OCR failed; reducing GPU memory use…".into(),
                                    );
                                    restart = true;
                                    let loaded_at = Instant::now();
                                    stop_process(&worker_process);
                                    native_client = Some(
                                        native_client
                                            .as_ref()
                                            .unwrap()
                                            .load_fallback(&mut preparation, &context)?,
                                    );
                                    timing.model_load_ms +=
                                        loaded_at.elapsed().as_secs_f64() * 1000.;
                                    result = native_client.as_ref().unwrap().recognize_prepared(
                                        &input,
                                        &context,
                                        Some(&mut timing),
                                    );
                                }
                                // This wall span includes fallback startup; subtract it from inference.
                                timing.inference_ms += (inference_at.elapsed().as_secs_f64()
                                    * 1000.
                                    - (timing.model_load_ms - loaded_before_inference))
                                    .max(0.);
                                last_model_use = Instant::now();
                                restart = result.as_ref().is_err_and(|error| error.restart);
                                return result.map_err(|error| error.message);
                            }
                            if native_client.take().is_some() {
                                stop_process(&worker_process);
                            }
                            report_progress("Recognizing writing…".into());
                            let prepared_at = Instant::now();
                            let (request, temporary_image) = prepare_request(&task)?;
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
                            let line_result = c
                                .output
                                .recv_timeout(Duration::from_secs(180))
                                .map_err(|_| {
                                    "Recognition timed out or stopped; try a smaller selection"
                                        .to_string()
                                });
                            drop(temporary_image);
                            let line = line_result??;
                            let response: Response = serde_json::from_str(&line)
                                .map_err(|e| format!("Invalid recognition response: {e}"))?;
                            timing.inference_ms += inference_at.elapsed().as_secs_f64() * 1000.;
                            last_model_use = Instant::now();
                            restart = false;
                            if let Some(error) = response.error {
                                // Older external Python packs do not classify failures.
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
                        native_client = None;
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
                }
                stop_process(&worker_process);
            })
            .expect("recognition worker thread");
        Self {
            sender: Some(sender),
            deferred: None,
            last_task: None,
            results,
            status,
            process,
            generation,
            startup_process,
            shutdown,
            warm_cancel,
            warming,
            last_intent: None,
        }
    }
    fn submit(&mut self, mut task: Task, download_allowed: bool) -> Result<(), String> {
        let native = std::fs::read(&task.pack)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .is_some_and(|v| v["backend"] == "llama-vulkan");
        if (task.auto_setup || native)
            && !download_allowed
            && !legacy_pack_available(&task.pack)
            && !native::installed(&task.pack)
        {
            self.deferred = Some(task);
            return Ok(());
        }
        task.queued = Instant::now();
        self.sender
            .as_ref()
            .ok_or("Recognition worker stopped")?
            .try_send(Work::Recognize(task.clone()))
            .map_err(|_| "Recognition worker is busy; try again".to_string())?;
        self.last_task = Some(task);
        Ok(())
    }
    fn prewarm(&mut self, pack: PathBuf, intent: u64) {
        if self.last_intent == Some(intent) || self.warming.load(Ordering::Acquire) {
            return;
        }
        // Only an already installed native pack is eligible. No first-use choice is bypassed.
        if !std::fs::read(&pack)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|config| config["backend"] == "llama-vulkan")
        {
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
        let work = Work::Warmup(pack, self.warm_cancel.load(Ordering::Acquire));
        if self
            .sender
            .as_ref()
            .is_some_and(|sender| sender.try_send(work).is_ok())
        {
            self.last_intent = Some(intent);
        } else {
            self.warming.store(false, Ordering::Release);
        }
    }
    fn cancel(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
        self.warm_cancel.fetch_add(1, Ordering::AcqRel);
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
    // A tall 1 and shorter rounded digits need not share half the whole line's
    // height. Compare their common band with the shortest digit instead, while
    // excluding tiny marks that might be punctuation mistaken for a digit.
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
fn prepare_image(source: &ImageSource) -> Result<image::DynamicImage, String> {
    match source {
        ImageSource::Object(object, assets) => {
            let (_, pixels) = folio_export::raster_object_limited(object, assets, 1_400_000)
                .map_err(|error| error.to_string())?;
            let image = image::RgbaImage::from_raw(
                pixels.width(),
                pixels.height(),
                pixels.take_demultiplied(),
            )
            .ok_or("Invalid recognition image")?;
            Ok(image::DynamicImage::ImageRgba8(image))
        }
        ImageSource::Pdf(background, assets, bounds) => {
            folio_pdf::render_preview(background, assets).map_err(|error| error.to_string())?;
            let source = folio_export::asset_path(
                assets,
                background
                    .preview_asset
                    .as_deref()
                    .ok_or("PDF preview missing")?,
            )
            .map_err(|error| error.to_string())?;
            let image = image::open(source).map_err(|error| error.to_string())?;
            let (width, height) = (image.width(), image.height());
            let x = ((bounds.min.x * width as f32).floor() as u32).min(width.saturating_sub(1));
            let y = ((bounds.min.y * height as f32).floor() as u32).min(height.saturating_sub(1));
            let w = ((bounds.width() * width as f32).ceil() as u32)
                .min(width - x)
                .max(1);
            let h = ((bounds.height() * height as f32).ceil() as u32)
                .min(height - y)
                .max(1);
            Ok(image
                .crop_imm(x, y, w, h)
                .resize(1400, 1000, image::imageops::FilterType::Lanczos3))
        }
    }
}
fn prepare_request(task: &Task) -> Result<(Request, TemporaryImage), String> {
    let mut temporary_image = TemporaryImage(None);
    if let Some(source) = &task.image {
        let path = std::env::temp_dir().join(format!("folio-ocr-{}.png", Id::new_v4()));
        temporary_image.0 = Some(path.clone());
        prepare_image(source)?
            .save(&path)
            .map_err(|error| error.to_string())?;
    }
    Ok((
        Request {
            kind: task.review.kind,
            strokes: request_strokes(task),
            image_path: temporary_image.0.clone(),
        },
        temporary_image,
    ))
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
        && resolve("ocr_model").is_none_or(|p| p.join("model.safetensors").is_file())
}
fn stop_process(process: &Mutex<Option<Child>>) {
    if let Some(mut child) = process.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown.store(1, Ordering::Release);
        stop_process(&self.startup_process);
        self.cancel(u64::MAX);
        self.sender.take();
    }
}
impl Controller {
    /// Prepare an installed native model on explicit OCR-control hover.
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
            if let Object::Stroke(stroke) = object.as_ref() {
                points = points.saturating_add(stroke.display_path().len());
                if points > 250_000 {
                    return;
                }
            }
            if !matches!(object.as_ref(), Object::Stroke(_) | Object::Image(_)) {
                continue;
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
        let pack = self.recognition_pack();
        legacy_pack_available(&pack) || native::installed(&pack)
    }
    pub fn recognition_setup_needed(&self) -> bool {
        self.recognition_service.deferred.is_some()
    }
    pub fn accept_recognition_setup(&mut self) -> Result<(), String> {
        let Some(mut task) = self.recognition_service.deferred.take() else {
            return Ok(());
        };
        if !self.recognition_is_current(&task.review) {
            self.cancel_recognition();
            return Err("The source changed. Select it and request recognition again.".into());
        }
        self.settings.ocr_download_allowed = true;
        self.store_settings();
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_timing = None;
        task.generation = self.recognition_generation;
        task.submitted = Instant::now();
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service.submit(task, true)?;
        self.recognition_pending = true;
        self.recognition_status = "Preparing OCR download…".into();
        Ok(())
    }
    pub fn pause_recognition_setup(&mut self) {
        let task = self.recognition_service.last_task.take();
        let for_index = self.recognition_for_index;
        self.cancel_recognition();
        self.recognition_for_index = for_index;
        self.recognition_service.deferred = task;
        self.recognition_status = "OCR download paused. Partial downloads are retained.".into();
    }
    fn recognition_pack(&self) -> PathBuf {
        pack_path(&self.data_dir)
    }
}

pub(super) fn pack_path(data_dir: &std::path::Path) -> PathBuf {
    if let Some(path) = std::env::var_os("FOLIO_RECOGNITION_CONFIG") {
        return path.into();
    }
    let local = data_dir.join("recognition/pack.json");
    if local.is_file() {
        return local;
    }
    if let Some(root) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent()?.parent().map(PathBuf::from))
    {
        let pack = root.join("recognition/pack.json");
        if pack.is_file()
            && (legacy_pack_available(&pack)
                || std::fs::read(&pack)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .is_some_and(|config| config["backend"] == "llama-vulkan"))
        {
            return pack;
        }
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
        self.recognition_service.submit(
            Task {
                submitted,
                queued: Instant::now(),
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack,
                review,
                image: None,
            },
            self.settings.ocr_download_allowed,
        )?;
        self.recognition_review = None;
        self.recognition_pending = true;
        self.recognition_status = if self.recognition_setup_needed() {
            "OCR setup needs your download choice"
        } else {
            "Preparing recognition…"
        }
        .into();
        Ok(())
    }
    pub fn cancel_recognition(&mut self) {
        self.recognition_service.last_intent = None;
        self.recognition_service.deferred = None;
        self.recognition_service.last_task = None;
        self.recognition_for_index = false;
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_timing = None;
        self.recognition_service.cancel(self.recognition_generation);
        self.recognition_pending = false;
        self.recognition_status.clear();
        self.recognition_replacing = false;
        self.recognition_review = None;
    }
    pub fn recognize_math_image(&mut self, kind: RecognitionKind) -> Result<(), String> {
        let submitted = Instant::now();
        if self.recognition_pending {
            return Err("Recognition is already running".into());
        }
        let sources: Vec<_> = self.selected_objects().cloned().collect();
        if sources.len() != 1 || !matches!(sources[0].as_ref(), Object::Image(_)) {
            return Err("Select one image; use its crop tool to isolate a problem".into());
        }
        let review = RecognitionReview {
            kind,
            text: String::new(),
            note: self.active,
            page: self.page().id,
            bounds: sources[0].bounds(),
            sources: sources.clone(),
            pdf_source: None,
        };
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_timing = None;
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service.submit(
            Task {
                submitted,
                queued: Instant::now(),
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack: self.recognition_pack(),
                review,
                image: Some(ImageSource::Object(sources[0].clone(), self.assets.clone())),
            },
            self.settings.ocr_download_allowed,
        )?;
        self.recognition_review = None;
        self.recognition_pending = true;
        self.recognition_status = if self.recognition_setup_needed() {
            "OCR setup needs your download choice"
        } else {
            "Preparing recognition…"
        }
        .into();
        Ok(())
    }
    pub fn recognize_pdf_math_region(
        &mut self,
        fractions: Rect,
        kind: RecognitionKind,
    ) -> Result<(), String> {
        let submitted = Instant::now();
        if self.recognition_pending {
            return Err("Recognition is already running".into());
        }
        if ![
            fractions.min.x,
            fractions.min.y,
            fractions.max.x,
            fractions.max.y,
        ]
        .iter()
        .all(|v| v.is_finite())
            || fractions.min.x < 0.
            || fractions.min.y < 0.
            || fractions.max.x > 1.
            || fractions.max.y > 1.
            || fractions.width() <= 0.
            || fractions.height() <= 0.
        {
            return Err(
                "Choose left, top, width and height inside the page, as fractions from 0 to 1"
                    .into(),
            );
        }
        let background = self
            .page()
            .properties
            .pdf
            .clone()
            .ok_or("Open a PDF page first")?;
        let properties = &self.page().properties;
        let bounds = Rect::new(
            fractions.min.x * properties.width,
            fractions.min.y * properties.height,
            fractions.width() * properties.width,
            fractions.height() * properties.height,
        );
        let review = RecognitionReview {
            kind,
            text: String::new(),
            note: self.active,
            page: self.page().id,
            bounds,
            sources: Vec::new(),
            pdf_source: Some(background.clone()),
        };
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_timing = None;
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service.submit(
            Task {
                submitted,
                queued: Instant::now(),
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack: self.recognition_pack(),
                review,
                image: Some(ImageSource::Pdf(background, self.assets.clone(), fractions)),
            },
            self.settings.ocr_download_allowed,
        )?;
        self.recognition_review = None;
        self.recognition_pending = true;
        self.recognition_status = if self.recognition_setup_needed() {
            "OCR setup needs your download choice"
        } else {
            "Preparing recognition…"
        }
        .into();
        Ok(())
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
        self.recognition_service.submit(
            Task {
                submitted,
                queued: Instant::now(),
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack: self.recognition_pack(),
                review,
                image: None,
            },
            self.settings.ocr_download_allowed,
        )?;
        self.recognition_pending = true;
        self.recognition_status = if self.recognition_setup_needed() {
            "OCR setup needs your download choice"
        } else {
            "Preparing recognition…"
        }
        .into();
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
            self.recognition_service.last_task = None;
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
                Err(error) => self.error = Some(error),
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
            return Err(
                "Image and PDF recognition can be solved or copied; replacement is for handwriting"
                    .into(),
            );
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
            self.recognition_status = if self.recognition_setup_needed() {
                "OCR setup needs your download choice"
            } else {
                "Preparing recognition…"
            }
            .into();
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
        // Leave room for corrected output; existing text wrapping/edit tools remain usable.
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

mod native;
#[cfg(test)]
mod tests;
