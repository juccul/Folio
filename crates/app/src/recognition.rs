//! Explicit, offline recognition of selected ink. Suggestions never mutate a document.
use super::*;
use folio_platform::BackgroundCommand;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

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
}
struct TemporaryImage(Option<PathBuf>);
impl Drop for TemporaryImage {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}
struct Task {
    generation: u64,
    pack: PathBuf,
    review: RecognitionReview,
    image: Option<ImageSource>,
    auto_setup: bool,
}
enum ImageSource {
    Object(Arc<Object>, PathBuf),
    Pdf(PdfBackground, PathBuf, Rect),
}
struct ResultMessage {
    generation: u64,
    review: RecognitionReview,
    result: Result<String, String>,
}

struct StatusMessage {
    generation: u64,
    text: String,
}
pub(super) struct Service {
    sender: Option<mpsc::SyncSender<Task>>,
    results: mpsc::Receiver<ResultMessage>,
    status: mpsc::Receiver<StatusMessage>,
    process: Arc<Mutex<Option<Child>>>,
    generation: Arc<AtomicU64>,
}
struct Client {
    input: ChildStdin,
    output: mpsc::Receiver<Result<String, String>>,
    pack: PathBuf,
}
impl Service {
    pub fn new() -> Self {
        let (sender, jobs) = mpsc::sync_channel::<Task>(1);
        let (tx, results) = mpsc::channel();
        let (progress, status) = mpsc::channel();
        let process: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let generation = Arc::new(AtomicU64::new(0));
        let worker_process = process.clone();
        let worker_generation = generation.clone();
        std::thread::Builder::new()
            .name("folio-recognition".into())
            .spawn(move || {
                let mut client: Option<Client> = None;
                let mut native_client: Option<native::Native> = None;
                while let Ok(task) = jobs.recv() {
                    if task.generation != worker_generation.load(Ordering::Acquire) {
                        continue;
                    }
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
                    let native_pack = std::fs::read(&task.pack)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                        .is_some_and(|config| config["backend"] == "llama-vulkan");
                    let use_native =
                        native_pack || (task.auto_setup && !legacy_pack_available(&task.pack));
                    let result = (|| {
                        if use_native {
                            if client.is_some() {
                                stop_process(&worker_process);
                                client = None;
                            }
                            let alive = worker_process
                                .lock()
                                .unwrap()
                                .as_mut()
                                .is_some_and(|p| matches!(p.try_wait(), Ok(None)));
                            if native_client.as_ref().is_none_or(|c| c.pack != task.pack) || !alive
                            {
                                stop_process(&worker_process);
                                native_client = Some(native::Native::load(&task.pack, &context)?);
                            }
                            let (request, temporary_image) = prepare_request(&task)?;
                            let mut result = native_client
                                .as_ref()
                                .unwrap()
                                .recognize(&request, &context);
                            if result
                                .as_ref()
                                .is_err_and(|error| error.starts_with("OCR runtime unavailable:"))
                                && native_client.as_ref().unwrap().is_gpu()
                            {
                                eprintln!(
                                    "Folio OCR GPU failed: {}; retrying on CPU",
                                    result.as_ref().unwrap_err()
                                );
                                report_progress("GPU OCR failed; retrying on CPU…".into());
                                stop_process(&worker_process);
                                native_client =
                                    Some(native::Native::load_cpu(&task.pack, &context)?);
                                result = native_client
                                    .as_ref()
                                    .unwrap()
                                    .recognize(&request, &context);
                            }
                            drop(temporary_image);
                            return result;
                        }
                        if native_client.take().is_some() {
                            stop_process(&worker_process);
                        }
                        report_progress("Recognizing writing…".into());
                        let alive = worker_process
                            .lock()
                            .unwrap()
                            .as_mut()
                            .is_some_and(|p| p.try_wait().ok().flatten().is_none());
                        if !alive || client.as_ref().is_none_or(|c| c.pack != task.pack) {
                            stop_process(&worker_process);
                            client = None;
                            let pack: Pack = serde_json::from_slice(
                                &std::fs::read(&task.pack)
                                    .map_err(|e| format!("Cannot read recognition pack: {e}"))?,
                            )
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
                                .map_err(|e| format!("Cannot start offline recognition: {e}"))?;
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
                                        Ok(0) => Err("Recognition worker exited; try again".into()),
                                        Ok(_) if line.len() > 1_048_576 => {
                                            Err("Recognition response is too large".into())
                                        }
                                        Ok(_) => String::from_utf8(line).map_err(|e| e.to_string()),
                                        Err(e) => Err(e.to_string()),
                                    };
                                    let failed = value.is_err();
                                    if lines_tx.send(value).is_err() || failed {
                                        break;
                                    }
                                }
                            });
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
                        let (request, temporary_image) = prepare_request(&task)?;
                        let c = client.as_mut().unwrap();
                        serde_json::to_writer(&mut c.input, &request).map_err(|e| e.to_string())?;
                        c.input
                            .write_all(b"\n")
                            .and_then(|_| c.input.flush())
                            .map_err(|e| e.to_string())?;
                        let line_result =
                            c.output
                                .recv_timeout(Duration::from_secs(180))
                                .map_err(|_| {
                                    "Recognition timed out or stopped; try a smaller selection"
                                        .to_string()
                                });
                        drop(temporary_image);
                        let line = line_result??;
                        let response: Response = serde_json::from_str(&line)
                            .map_err(|e| format!("Invalid recognition response: {e}"))?;
                        if let Some(error) = response.error {
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
                    })();
                    if result.is_err() {
                        stop_process(&worker_process);
                        client = None;
                        native_client = None;
                    }
                    if tx
                        .send(ResultMessage {
                            generation: task.generation,
                            review: task.review,
                            result,
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
            results,
            status,
            process,
            generation,
        }
    }
    fn cancel(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
        stop_process(&self.process);
    }
}
fn prepare_request(task: &Task) -> Result<(Request, TemporaryImage), String> {
    let strokes = task
        .review
        .sources
        .iter()
        .filter_map(|o| {
            let Object::Stroke(s) = o.as_ref() else {
                return None;
            };
            Some(
                s.display_path()
                    .iter()
                    .map(|p| {
                        let p = s.transform.apply(p.position);
                        [p.x, p.y]
                    })
                    .collect(),
            )
        })
        .collect();
    let mut temporary_image = TemporaryImage(None);
    let image_path = if let Some(image) = &task.image {
        let path = std::env::temp_dir().join(format!("folio-ocr-{}.png", Id::new_v4()));
        temporary_image.0 = Some(path.clone());
        match image {
            ImageSource::Object(object, assets) => {
                let (_, pixels) = folio_export::raster_object_limited(object, assets, 1_400_000)
                    .map_err(|e| e.to_string())?;
                pixels.save_png(&path).map_err(|e| e.to_string())?;
            }
            ImageSource::Pdf(background, assets, bounds) => {
                folio_pdf::render_preview(background, assets).map_err(|e| e.to_string())?;
                let source = folio_export::asset_path(
                    assets,
                    background
                        .preview_asset
                        .as_deref()
                        .ok_or("PDF preview missing")?,
                )
                .map_err(|e| e.to_string())?;
                let image = image::open(source).map_err(|e| e.to_string())?;
                let (width, height) = (image.width(), image.height());
                let x = (bounds.min.x * width as f32).floor() as u32;
                let y = (bounds.min.y * height as f32).floor() as u32;
                let w = (bounds.width() * width as f32).ceil() as u32;
                let h = (bounds.height() * height as f32).ceil() as u32;
                image
                    .crop_imm(x, y, w.min(width - x).max(1), h.min(height - y).max(1))
                    .resize(1400, 1000, image::imageops::FilterType::Lanczos3)
                    .save(&path)
                    .map_err(|e| e.to_string())?;
            }
        }
        Some(path)
    } else {
        None
    };
    Ok((
        Request {
            kind: task.review.kind,
            strokes,
            image_path,
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
        self.cancel(u64::MAX);
        self.sender.take();
    }
}
impl Controller {
    fn recognition_pack(&self) -> PathBuf {
        if let Some(path) = std::env::var_os("FOLIO_RECOGNITION_CONFIG") {
            return path.into();
        }
        let local = self.data_dir.join("recognition/pack.json");
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
    pub fn can_recognize_selection(&self) -> bool {
        let hidden = self.page().hidden_sources();
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
        let hidden = self.page().hidden_sources();
        let sources: Vec<_> = self
            .page()
            .ordered_objects()
            .filter(|o| {
                self.session().selection.contains(&o.id())
                    && !hidden.contains(&o.id())
                    && matches!(o.as_ref(), Object::Stroke(_))
            })
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
        self.recognition_for_index = false;
        self.finish();
        if self.recognition_pending {
            return Err("Recognition is already running".into());
        }
        let pack = self.recognition_pack();
        let review = self.recognition_snapshot(kind)?;
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service
            .sender
            .as_ref()
            .unwrap()
            .try_send(Task {
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack,
                review,
                image: None,
            })
            .map_err(|_| "Recognition worker is busy; try again".to_string())?;
        self.recognition_review = None;
        self.recognition_pending = true;
        self.recognition_status = "Preparing recognition…".into();
        Ok(())
    }
    pub fn cancel_recognition(&mut self) {
        self.recognition_for_index = false;
        self.recognition_generation = self.recognition_generation.wrapping_add(1);
        self.recognition_service.cancel(self.recognition_generation);
        self.recognition_pending = false;
        self.recognition_status.clear();
        self.recognition_replacing = false;
        self.recognition_review = None;
    }
    pub fn recognize_math_image(&mut self, kind: RecognitionKind) -> Result<(), String> {
        if self.recognition_pending {
            return Err("Recognition is already running".into());
        }
        let sources: Vec<_> = self
            .page()
            .ordered_objects()
            .filter(|o| self.session().selection.contains(&o.id()))
            .cloned()
            .collect();
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
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service
            .sender
            .as_ref()
            .unwrap()
            .try_send(Task {
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack: self.recognition_pack(),
                review,
                image: Some(ImageSource::Object(sources[0].clone(), self.assets.clone())),
            })
            .map_err(|_| "Recognition worker is busy")?;
        self.recognition_review = None;
        self.recognition_pending = true;
        self.recognition_status = "Preparing recognition…".into();
        Ok(())
    }
    pub fn recognize_pdf_math_region(
        &mut self,
        fractions: Rect,
        kind: RecognitionKind,
    ) -> Result<(), String> {
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
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service
            .sender
            .as_ref()
            .unwrap()
            .try_send(Task {
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack: self.recognition_pack(),
                review,
                image: Some(ImageSource::Pdf(background, self.assets.clone(), fractions)),
            })
            .map_err(|_| "Recognition worker is busy")?;
        self.recognition_review = None;
        self.recognition_pending = true;
        self.recognition_status = "Preparing recognition…".into();
        Ok(())
    }
    pub(super) fn recognize_math_sources(
        &mut self,
        sources: Vec<Arc<Object>>,
        bounds: Rect,
    ) -> Result<(), String> {
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
        self.recognition_service
            .generation
            .store(self.recognition_generation, Ordering::Release);
        self.recognition_service
            .sender
            .as_ref()
            .unwrap()
            .try_send(Task {
                generation: self.recognition_generation,
                auto_setup: std::env::var_os("FOLIO_RECOGNITION_CONFIG").is_none(),
                pack: self.recognition_pack(),
                review,
                image: None,
            })
            .map_err(|_| "Recognition is busy")?;
        self.recognition_pending = true;
        self.recognition_status = "Preparing recognition…".into();
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
        while let Ok(message) = self.recognition_service.results.try_recv() {
            if message.generation != self.recognition_generation {
                continue;
            }
            changed = true;
            self.recognition_pending = false;
            if !self.recognition_is_current(&message.review) {
                self.error = Some("Writing changed or another page opened; select the writing and recognize again".into());
                continue;
            }
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
            self.recognition_status = "Preparing recognition…".into();
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
