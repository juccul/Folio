//! Independent offline math process, source validation and durable calculations.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command as ProcessCommand, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MathRequest {
    pub expression: String,
    pub operation: String,
    pub variable: String,
    pub domain: String,
    pub angle: String,
    pub method: String,
    pub next_line: String,
    pub variables: BTreeMap<String, String>,
    pub x_min: f64,
    pub x_max: f64,
}
impl Default for MathRequest {
    fn default() -> Self {
        Self {
            expression: String::new(),
            operation: "auto".into(),
            variable: "x".into(),
            domain: "real".into(),
            angle: "radians".into(),
            method: String::new(),
            next_line: String::new(),
            variables: BTreeMap::new(),
            x_min: -10.,
            x_max: 10.,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MathStep {
    pub rule: String,
    pub before: String,
    pub after: String,
    pub explanation: String,
    pub details: String,
    pub status: String,
    pub children: Vec<MathStep>,
    #[serde(skip)]
    pub preview: Option<PathBuf>,
    #[serde(skip)]
    pub rendered_svg: Option<String>,
    #[serde(skip)]
    pub vector: Option<Arc<folio_math::VectorFormula>>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MathGraph {
    pub svg: String,
    pub x_min: f64,
    pub x_max: f64,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MathReport {
    pub answer: String,
    pub answer_latex: String,
    pub approximate: String,
    pub input_latex: String,
    pub title: String,
    pub message: String,
    pub status: String,
    pub restrictions: Vec<String>,
    pub methods: Vec<String>,
    pub steps: Vec<MathStep>,
    pub graph: Option<MathGraph>,
    pub interpretation: Option<String>,
    pub assignment: Option<String>,
    pub assignment_value: Option<String>,
    #[serde(skip)]
    pub preview: Option<PathBuf>,
    #[serde(skip)]
    pub rendered_svg: Option<String>,
    #[serde(skip)]
    pub vector: Option<Arc<folio_math::VectorFormula>>,
}
#[derive(Clone)]
struct Sources {
    note: Id,
    page: Id,
    objects: Vec<Arc<Object>>,
    bounds: Rect,
    pdf_source: Option<PdfBackground>,
}
fn math_source_text(sources: &Sources) -> String {
    sources
        .objects
        .iter()
        .filter_map(|object| match object.as_ref() {
            Object::Equation(e) => Some(
                e.math_link
                    .as_ref()
                    .map_or_else(|| e.latex.clone(), |link| link.expression.clone()),
            ),
            Object::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub struct MathSession {
    pub request: MathRequest,
    pub report: Option<MathReport>,
    pub pending: bool,
    pub error: Option<String>,
    pub revealed: usize,
    pub show_details: bool,
    pub hint: bool,
    pub revision: u64,
    sources: Sources,
}
#[derive(Clone)]
enum Purpose {
    Panel(Sources),
    Edit {
        sources: Sources,
        latex: String,
    },
    Live {
        note: Id,
        page: Id,
        before: Arc<Object>,
        signature: String,
        link: Box<MathLink>,
        sources: Vec<Arc<Object>>,
        ink_version: Option<String>,
    },
}
struct Task {
    generation: u64,
    pack: PathBuf,
    request: MathRequest,
    purpose: Purpose,
}
struct Completion {
    generation: u64,
    request: MathRequest,
    purpose: Purpose,
    result: Result<MathReport, String>,
}
struct ProcessClient {
    input: ChildStdin,
    output: mpsc::Receiver<Result<String, String>>,
    pack: PathBuf,
}
pub(super) struct Service {
    sender: Option<mpsc::SyncSender<Task>>,
    results: mpsc::Receiver<Completion>,
    process: Arc<Mutex<Option<Child>>>,
    generation: Arc<AtomicU64>,
}
fn stop(process: &Mutex<Option<Child>>) {
    if let Some(mut child) = process.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
fn formula(latex: &str) -> Result<(String, Arc<folio_math::VectorFormula>), String> {
    let svg = folio_math::render(latex).map_err(|e| e.to_string())?;
    let vector = folio_math::VectorFormula::from_svg(&svg).map_err(|e| e.to_string())?;
    Ok((svg, Arc::new(vector)))
}
fn prepare_report(report: &mut MathReport, root: &std::path::Path) {
    if let Some(graph) = &report.graph {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        graph.svg.hash(&mut hash);
        let path = root.join(format!("{:016x}.png", hash.finish()));
        let saved = path.is_file()
            || (|| {
                std::fs::create_dir_all(root).ok()?;
                folio_export::raster_svg(&graph.svg, 2.)
                    .ok()?
                    .save_png(&path)
                    .ok()
            })()
            .is_some();
        report.preview = saved.then_some(path);
        report.rendered_svg = Some(graph.svg.clone());
    } else if let Ok((svg, vector)) = formula(&report.answer_latex) {
        report.rendered_svg = Some(svg);
        report.vector = Some(vector);
    }
    prepare_steps(&mut report.steps, &mut 96);
}
fn prepare_steps(steps: &mut [MathStep], remaining: &mut usize) {
    for step in steps.iter_mut().take(48) {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;
        if let Ok((svg, vector)) = formula(&step.after) {
            step.rendered_svg = Some(svg);
            step.vector = Some(vector);
        }
        prepare_steps(&mut step.children, remaining);
    }
}
impl Service {
    pub fn new() -> Self {
        let (sender, jobs) = mpsc::sync_channel::<Task>(32);
        let (tx, results) = mpsc::sync_channel(8);
        let process = Arc::new(Mutex::new(None));
        let generation = Arc::new(AtomicU64::new(0));
        let previews = std::env::temp_dir().join(format!("folio-math-{}", Id::new_v4()));
        let worker_process = process.clone();
        let worker_generation = generation.clone();
        let worker_previews = previews.clone();
        std::thread::Builder::new()
            .name("folio-math-solver".into())
            .spawn(move || {
                let mut client: Option<ProcessClient> = None;
                while let Ok(task) = jobs.recv() {
                    if task.generation != worker_generation.load(Ordering::Acquire) {
                        continue;
                    }
                    let result =
                        (|| {
                            if let Purpose::Edit { latex, .. } = &task.purpose {
                                let (svg, vector) = formula(latex)?;
                                return Ok(MathReport {
                                    answer: latex.clone(),
                                    answer_latex: latex.clone(),
                                    title: "Edited result".into(),
                                    message: "LaTeX edited manually; this answer has not been verified by the solver.".into(),
                                    status: "edited".into(),
                                    rendered_svg: Some(svg),
                                    vector: Some(vector),
                                    ..Default::default()
                                });
                            }
                            let alive =
                                worker_process.lock().unwrap().as_mut().is_some_and(
                                    |p: &mut Child| p.try_wait().ok().flatten().is_none(),
                                );
                            if !alive || client.as_ref().is_none_or(|c| c.pack != task.pack) {
                                stop(&worker_process);
                                client = None;
                                #[derive(Deserialize)]
                                struct Pack {
                                    python: PathBuf,
                                    worker: PathBuf,
                                }
                                let pack: Pack =
                                    serde_json::from_slice(&std::fs::read(&task.pack).map_err(
                                        |e| format!("Offline math pack is missing: {e}"),
                                    )?)
                                    .map_err(|e| format!("Invalid math pack: {e}"))?;
                                let root = task.pack.parent().ok_or("Invalid math pack path")?;
                                let absolute = |path: PathBuf| {
                                    if path.is_absolute() {
                                        path
                                    } else {
                                        root.join(path)
                                    }
                                };
                                let mut child = ProcessCommand::new(absolute(pack.python))
                                    .arg("-u")
                                    .arg(absolute(pack.worker))
                                    .arg("--config")
                                    .arg(&task.pack)
                                    .stdin(Stdio::piped())
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::inherit())
                                    .spawn()
                                    .map_err(|e| format!("Cannot start offline math: {e}"))?;
                                let input = child.stdin.take().unwrap();
                                let stdout = child.stdout.take().unwrap();
                                *worker_process.lock().unwrap() = Some(child);
                                let (lines_tx, output) = mpsc::sync_channel(1);
                                std::thread::spawn(move || {
                                    let mut reader = BufReader::new(stdout);
                                    loop {
                                        let mut line = Vec::new();
                                        let value = match reader
                                            .by_ref()
                                            .take(4_194_305)
                                            .read_until(b'\n', &mut line)
                                        {
                                            Ok(0) => Err("Math worker stopped; try again".into()),
                                            Ok(_) if line.len() > 4_194_304 => {
                                                Err("Math response is too large".into())
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
                                client = Some(ProcessClient {
                                    input,
                                    output,
                                    pack: task.pack.clone(),
                                });
                            }
                            if task.generation != worker_generation.load(Ordering::Acquire) {
                                return Err("Math solving cancelled".into());
                            }
                            let c = client.as_mut().unwrap();
                            serde_json::to_writer(&mut c.input, &task.request)
                                .map_err(|e| e.to_string())?;
                            c.input
                                .write_all(b"\n")
                                .and_then(|_| c.input.flush())
                                .map_err(|e| e.to_string())?;
                            let line = c.output.recv_timeout(Duration::from_secs(15)).map_err(
                                |_| "Math solver timed out; try a smaller problem".to_string(),
                            )??;
                            let value: serde_json::Value =
                                serde_json::from_str(&line).map_err(|e| e.to_string())?;
                            if let Some(error) = value.get("error").and_then(|e| e.as_str()) {
                                return Err(error.to_string());
                            }
                            let mut report: MathReport =
                                serde_json::from_value(value).map_err(|e| e.to_string())?;
                            prepare_report(&mut report, &worker_previews);
                            Ok(report)
                        })();
                    if result.is_err() {
                        stop(&worker_process);
                        client = None;
                    }
                    if tx
                        .send(Completion {
                            generation: task.generation,
                            request: task.request,
                            purpose: task.purpose,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                stop(&worker_process);
                let _ = std::fs::remove_dir_all(worker_previews);
            })
            .expect("start math solver");
        Self {
            sender: Some(sender),
            results,
            process,
            generation,
        }
    }
    fn cancel(&self, generation: u64) {
        self.generation.store(generation, Ordering::Release);
        stop(&self.process);
    }
    fn submit(&self, task: Task) -> Result<(), String> {
        self.sender
            .as_ref()
            .ok_or("Math solver stopped")?
            .try_send(task)
            .map_err(|_| "Math solver queue is full; try again".into())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.cancel(u64::MAX);
        self.sender.take();
        // The worker removes these after it exits, avoiding a preview-write race.
    }
}
impl Controller {
    fn math_pack(&self) -> PathBuf {
        if let Some(path) = std::env::var_os("FOLIO_MATH_CONFIG") {
            return path.into();
        }
        let local = self.data_dir.join("math-solver/pack.json");
        if local.is_file() {
            return local;
        }
        if let Some(root) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent()?.parent().map(PathBuf::from))
        {
            let portable = root.join("math-solver/pack.json");
            if portable.is_file() {
                return portable;
            }
        }
        if cfg!(debug_assertions) {
            let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../artifacts/math-solver/pack.json");
            if development.is_file() {
                return development;
            }
        }
        local
    }
    fn math_sources(&self) -> Sources {
        let objects: Vec<_> = self
            .page()
            .ordered_objects()
            .filter(|o| self.session().selection.contains(&o.id()))
            .cloned()
            .collect();
        let bounds = objects
            .iter()
            .map(|o| o.bounds())
            .reduce(Rect::union)
            .unwrap_or(Rect::new(100., 100., 240., 70.));
        Sources {
            note: self.active,
            page: self.page().id,
            objects,
            bounds,
            pdf_source: None,
        }
    }
    fn math_sources_current(&self, sources: &Sources) -> bool {
        sources.note == self.active
            && sources.page == self.page().id
            && sources
                .pdf_source
                .as_ref()
                .is_none_or(|pdf| self.page().properties.pdf.as_ref() == Some(pdf))
            && sources
                .objects
                .iter()
                .all(|old| self.page().objects.get(&old.id()) == Some(old))
    }
    pub fn open_math_solver(&mut self) -> Result<(), String> {
        self.finish();
        self.cancel_math_solver();
        let sources = self.math_sources();
        let text = math_source_text(&sources);
        let mut request = MathRequest {
            expression: text,
            variables: self.math_variables()?,
            ..Default::default()
        };
        if let Some(Object::Equation(e)) = sources.objects.first().map(|o| o.as_ref())
            && let Some(link) = &e.math_link
        {
            request.operation = if link.operation == "assign" {
                "auto".into()
            } else {
                link.operation.clone()
            };
            request.variable = link.variable.clone();
            request.domain = link.domain.clone();
            request.angle = link.angle.clone();
            request.x_min = link.x_min;
            request.x_max = link.x_max;
        }
        self.math_session = Some(MathSession {
            request,
            report: None,
            pending: false,
            error: None,
            revealed: 0,
            show_details: false,
            hint: false,
            revision: self.math_generation,
            sources,
        });
        if self
            .math_session
            .as_ref()
            .unwrap()
            .request
            .expression
            .is_empty()
            && self.can_recognize_selection()
        {
            self.math_after_ocr = true;
            self.recognize_selection(RecognitionKind::Math)?;
        } else if self
            .math_session
            .as_ref()
            .unwrap()
            .request
            .expression
            .is_empty()
            && self.page().ordered_objects().any(|o| {
                self.session().selection.contains(&o.id()) && matches!(o.as_ref(), Object::Image(_))
            })
        {
            self.math_after_ocr = true;
            self.recognize_math_image(RecognitionKind::Math)?;
        }
        Ok(())
    }
    pub fn use_math_review(&mut self, text: String) -> Result<(), String> {
        let review = self
            .recognition_review
            .as_ref()
            .ok_or("No math recognition to solve")?
            .clone();
        if !self.recognition_is_current(&review) {
            return Err("Source writing changed; recognize again".into());
        }
        self.cancel_math_solver();
        self.math_session = Some(MathSession {
            request: MathRequest {
                expression: text,
                variables: self.math_variables()?,
                ..Default::default()
            },
            report: None,
            pending: false,
            error: None,
            revealed: 0,
            show_details: false,
            hint: false,
            revision: self.math_generation,
            sources: Sources {
                note: review.note,
                page: review.page,
                objects: review.sources,
                bounds: review.bounds,
                pdf_source: review.pdf_source.clone(),
            },
        });
        self.recognition_review = None;
        Ok(())
    }
    pub fn run_math_solver(&mut self, mut request: MathRequest) -> Result<(), String> {
        let session = self
            .math_session
            .as_ref()
            .ok_or("Open the math panel first")?;
        if !self.math_sources_current(&session.sources) {
            return Err("Source changed; select it and reopen Solve".into());
        }
        if request.expression.trim().is_empty() {
            return Err("Enter a problem or select writing to recognize".into());
        }
        if request.expression.len() > 8192 || request.next_line.len() > 8192 {
            return Err("Select a smaller problem".into());
        }
        let sources = session.sources.clone();
        self.math_generation = self.math_generation.wrapping_add(1);
        if self.math_session.as_ref().is_some_and(|s| s.pending)
            || !self.math_live_pending.is_empty()
        {
            self.math_service.cancel(self.math_generation);
        } else {
            self.math_service
                .generation
                .store(self.math_generation, Ordering::Release);
        }
        self.math_live_pending.clear();
        request.variables = self.math_variables()?;
        self.math_service.submit(Task {
            generation: self.math_generation,
            pack: self.math_pack(),
            request: request.clone(),
            purpose: Purpose::Panel(sources),
        })?;
        let session = self.math_session.as_mut().unwrap();
        session.request = request;
        session.pending = true;
        session.report = None;
        session.error = None;
        session.hint = false;
        session.revealed = 0;
        Ok(())
    }
    /// Typeset a manually edited result independently of the Python solver.
    /// Keep the last valid report until the worker accepts the new LaTeX.
    pub fn edit_math_result(&mut self, latex: String) -> Result<(), String> {
        let session = self.math_session.as_ref().ok_or("No math result")?;
        let report = session.report.as_ref().ok_or("Solve a problem first")?;
        if report.graph.is_some()
            || report.interpretation.is_some()
            || report.answer_latex.is_empty()
        {
            return Err("This result has no editable formula".into());
        }
        if !self.math_sources_current(&session.sources)
            || self.math_variables()? != session.request.variables
        {
            return Err("Source or page variables changed; solve again before editing".into());
        }
        folio_math::validate_latex(&latex).map_err(|e| e.to_string())?;
        let sources = session.sources.clone();
        let request = session.request.clone();
        self.cancel_math_solver();
        self.math_service.submit(Task {
            generation: self.math_generation,
            pack: self.math_pack(),
            request,
            purpose: Purpose::Edit { sources, latex },
        })?;
        let session = self.math_session.as_mut().unwrap();
        session.pending = true;
        session.error = None;
        Ok(())
    }
    pub fn cancel_math_solver(&mut self) {
        if self.math_after_ocr || self.math_live_ocr.is_some() {
            self.cancel_recognition();
        }
        self.math_generation = self.math_generation.wrapping_add(1);
        self.math_service.cancel(self.math_generation);
        self.math_live_pending.clear();
        self.math_after_ocr = false;
        self.math_live_ocr = None;
        if let Some(session) = &mut self.math_session {
            session.pending = false;
        }
    }
    pub fn close_math_solver(&mut self) {
        self.cancel_math_solver();
        self.math_session = None;
    }
    pub fn read_math_selection(
        &mut self,
        kind: RecognitionKind,
        next_line: bool,
    ) -> Result<(), String> {
        self.finish();
        if self.math_session.is_none() {
            self.open_math_solver()?;
        }
        self.cancel_math_solver();
        if let Some(session) = &mut self.math_session {
            session.report = None;
            session.error = None;
            session.request.method.clear();
        }
        let sources = self.math_sources();
        let text = math_source_text(&sources);
        if !text.trim().is_empty() {
            let session = self.math_session.as_mut().unwrap();
            if next_line {
                session.request.next_line = text;
            } else {
                session.request.expression = text;
                session.sources = sources;
            }
            session.revision += 1;
            return Ok(());
        }
        if self.can_recognize_selection() {
            self.recognize_selection(kind)?;
        } else {
            self.recognize_math_image(kind)?;
        }
        self.math_after_ocr = true;
        self.math_ocr_next = next_line;
        Ok(())
    }
    pub fn read_pdf_math(&mut self, fractions: Rect) -> Result<(), String> {
        if self.math_session.is_none() {
            self.open_math_solver()?;
        }
        self.recognize_pdf_math_region(fractions, RecognitionKind::Math)?;
        self.math_after_ocr = true;
        self.math_ocr_next = false;
        Ok(())
    }
    pub fn math_variables(&self) -> Result<BTreeMap<String, String>, String> {
        let mut variables = BTreeMap::new();
        for object in self.page().ordered_objects() {
            if let Object::Equation(e) = object.as_ref()
                && let Some(link) = &e.math_link
                && link.operation == "assign"
            {
                let (name, value) = link
                    .expression
                    .split_once(":=")
                    .ok_or("Invalid stored variable assignment")?;
                let name = name.trim().to_string();
                if variables
                    .insert(name.clone(), value.trim().to_string())
                    .is_some()
                {
                    return Err(format!(
                        "Variable {name} is defined twice on this page; edit or remove a definition"
                    ));
                }
            }
        }
        Ok(variables)
    }
    fn make_math_object(
        &self,
        request: &MathRequest,
        report: &MathReport,
        sources: &Sources,
        live: bool,
    ) -> Result<Equation, String> {
        let svg = report
            .rendered_svg
            .clone()
            .ok_or("This result cannot be inserted as a rendered formula")?;
        let graph = report.graph.is_some();
        let mut equation = Equation {
            id: Id::new_v4(),
            latex: report.answer_latex.clone(),
            rendered_svg: Some(svg),
            rect: Rect::new(
                sources.bounds.min.x,
                sources.bounds.max.y + 16.,
                if graph {
                    420.
                } else {
                    sources.bounds.width().max(180.)
                },
                if graph { 250. } else { 64. },
            ),
            source_strokes: Vec::new(),
            transform: Transform::default(),
            math_link: None,
        };
        if live && report.status == "edited" {
            return Err("Edited LaTeX is a static result; use Insert result".into());
        }
        if live && request.operation == "check" {
            return Err("A checked step is a static result; use Insert result".into());
        }
        let source_text = sources
            .objects
            .iter()
            .filter_map(|o| match o.as_ref() {
                Object::Text(t) => Some(t.text.clone()),
                Object::Equation(e) => Some(
                    e.math_link
                        .as_ref()
                        .map_or_else(|| e.latex.clone(), |l| l.expression.clone()),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let has_ink = sources
            .objects
            .iter()
            .any(|o| matches!(o.as_ref(), Object::Stroke(_)));
        let attach = has_ink || source_text == request.expression;
        if live || graph || report.assignment.is_some() {
            equation.math_link = Some(MathLink {
                expression: report.assignment.as_ref().map_or_else(
                    || request.expression.clone(),
                    |name| {
                        format!(
                            "{name}:={}",
                            report.assignment_value.as_deref().unwrap_or("0")
                        )
                    },
                ),
                operation: if report.assignment.is_some() {
                    "assign".into()
                } else {
                    request.operation.clone()
                },
                variable: request.variable.clone(),
                domain: request.domain.clone(),
                angle: request.angle.clone(),
                method: request.method.clone(),
                x_min: request.x_min,
                x_max: request.x_max,
                live: live && report.assignment.is_none(),
                sources: if attach {
                    sources.objects.iter().map(|o| o.id()).collect()
                } else {
                    Vec::new()
                },
                ink_region: (live
                    && sources
                        .objects
                        .iter()
                        .any(|o| matches!(o.as_ref(), Object::Stroke(_))))
                .then_some(Rect::new(
                    sources.bounds.min.x - 12.,
                    sources.bounds.min.y - 12.,
                    sources.bounds.width().max(360.) + 24.,
                    sources.bounds.height().max(64.) + 24.,
                )),
            });
        }
        Ok(equation)
    }
    pub fn insert_math_result(&mut self, worked: bool, live: bool) -> Result<(), String> {
        self.finish();
        let session = self.math_session.as_ref().ok_or("No math result")?;
        if !self.math_sources_current(&session.sources) {
            return Err("The source changed; solve it again before inserting".into());
        }
        if self.math_variables()? != session.request.variables {
            return Err("Page variables changed; solve again before inserting".into());
        }
        if session.pending {
            return Err("Wait for the result preview before inserting".into());
        }
        let report = session
            .report
            .as_ref()
            .ok_or("Solve the problem before inserting")?;
        if matches!(report.status.as_str(), "incorrect" | "unknown" | "review") {
            return Err("Review or correct this result before inserting".into());
        }
        let mut equation =
            self.make_math_object(&session.request, report, &session.sources, live)?;
        let worked_height: f32 = if worked {
            report
                .steps
                .iter()
                .take(48)
                .map(|s| 40. + if s.rendered_svg.is_some() { 60. } else { 0. })
                .sum()
        } else {
            0.
        };
        let width = equation.rect.width().max(if worked { 440. } else { 0. });
        let height = equation.rect.height() + if worked { 16. + worked_height } else { 0. };
        for _ in 0..128 {
            let reserved = Rect::new(equation.rect.min.x, equation.rect.min.y, width, height);
            let next = self
                .session()
                .index
                .query(reserved)
                .iter()
                .filter_map(|id| self.page().objects.get(id))
                .map(|o| o.bounds())
                .filter(|bounds| bounds.intersects(reserved))
                .map(|bounds| bounds.max.y + 16.)
                .reduce(f32::max);
            let Some(y) = next else {
                break;
            };
            equation.rect = Rect::new(
                equation.rect.min.x,
                y,
                equation.rect.width(),
                equation.rect.height(),
            );
        }
        let result_bounds = equation.rect;
        if let Some(region) = equation.math_link.as_ref().and_then(|l| l.ink_region) {
            let ink: Vec<_> = self
                .page()
                .ordered_objects()
                .filter(|o| {
                    matches!(o.as_ref(), Object::Stroke(_))
                        && region.contains(o.bounds().min)
                        && region.contains(o.bounds().max)
                })
                .cloned()
                .collect();
            self.math_ink_signatures
                .insert(equation.id, ink_signature(&ink));
        }
        let mut objects = vec![Object::Equation(equation)];
        if worked {
            let position = session.sources.bounds;
            let mut y = result_bounds.max.y + 16.;
            for (index, step) in report.steps.iter().take(48).enumerate() {
                objects.push(Object::Text(TextBlock {
                    id: Id::new_v4(),
                    text: format!("{}. {}", index + 1, step.explanation),
                    rect: Rect::new(position.min.x, y, 440., 36.),
                    transform: Transform::default(),
                    font_family: "sans-serif".into(),
                    font_size: 14.,
                    color: self.style.color,
                    bold: false,
                    italic: false,
                    underline: false,
                    alignment: Alignment::Left,
                    list: ListStyle::None,
                }));
                y += 40.;
                if let Some(svg) = &step.rendered_svg {
                    objects.push(Object::Equation(Equation {
                        id: Id::new_v4(),
                        latex: step.after.clone(),
                        rendered_svg: Some(svg.clone()),
                        rect: Rect::new(position.min.x, y, 300., 48.),
                        source_strokes: Vec::new(),
                        transform: Transform::default(),
                        math_link: None,
                    }));
                    y += 60.;
                }
            }
        }
        let index = self.page().order.len();
        let ids = objects.iter().map(Object::id).collect();
        let changes = objects
            .into_iter()
            .enumerate()
            .map(|(offset, object)| Change::Object {
                page: self.page().id,
                id: object.id(),
                before: None,
                after: Some(Arc::new(object)),
                index: index + offset,
            })
            .collect();
        self.commit(
            if worked {
                "Insert worked solution"
            } else if live {
                "Insert live calculation"
            } else {
                "Insert math result"
            },
            changes,
        );
        self.session_mut().selection = ids;
        self.math_live_signatures.clear();
        Ok(())
    }
    pub fn update_math_expression(&mut self, id: Id, expression: String) -> Result<(), String> {
        self.update_math_expression_inner(id, expression, false)
    }
    fn update_math_expression_inner(
        &mut self,
        id: Id,
        expression: String,
        retain_sources: bool,
    ) -> Result<(), String> {
        let object = self
            .page()
            .objects
            .get(&id)
            .ok_or("Calculation is missing")?
            .clone();
        let Object::Equation(e) = object.as_ref() else {
            return Err("Select a calculation".into());
        };
        let mut link = e.math_link.clone().ok_or("Not a linked calculation")?;
        link.expression = expression;
        if retain_sources && let Some(region) = link.ink_region {
            link.sources = self
                .math_region_sources(region)
                .iter()
                .map(|o| o.id())
                .collect();
        }
        if !retain_sources {
            link.sources.clear();
            link.ink_region = None;
        }
        let request = request_from_link(&link, self.math_variables()?);
        let signature = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        self.math_service.submit(Task {
            generation: self.math_generation,
            pack: self.math_pack(),
            request,
            purpose: Purpose::Live {
                note: self.active,
                page: self.page().id,
                before: object,
                signature,
                sources: link
                    .sources
                    .iter()
                    .filter_map(|id| self.page().objects.get(id).cloned())
                    .collect(),
                ink_version: link
                    .ink_region
                    .map(|r| ink_signature(&self.math_region_sources(r))),
                link: Box::new(link),
            },
        })?;
        self.math_live_pending.insert(id);
        Ok(())
    }
    fn math_region_sources(&self, region: Rect) -> Vec<Arc<Object>> {
        let mut sources: Vec<_> = self
            .session()
            .index
            .query(region)
            .into_iter()
            .filter_map(|id| self.page().objects.get(&id))
            .filter(|o| {
                matches!(o.as_ref(), Object::Stroke(_))
                    && region.contains(o.bounds().min)
                    && region.contains(o.bounds().max)
            })
            .take(4097)
            .cloned()
            .collect();
        sources.sort_unstable_by_key(|o| self.session().order_positions[&o.id()]);
        sources
    }
    pub(super) fn poll_math_solver(&mut self) -> bool {
        let mut changed = false;
        if self.interaction.is_some() {
            self.last_math_ink = Instant::now();
        }
        if self.math_live_ocr.is_some() && !self.recognition_pending {
            let (id, signature) = self.math_live_ocr.take().unwrap();
            let current = self
                .page()
                .objects
                .get(&id)
                .and_then(|o| match o.as_ref() {
                    Object::Equation(e) => e.math_link.as_ref()?.ink_region,
                    _ => None,
                })
                .is_some_and(|region| {
                    ink_signature(&self.math_region_sources(region)) == signature
                });
            self.math_ink_signatures.insert(id, signature);
            if let Some(review) = self.recognition_review.take()
                && current
                && self.recognition_is_current(&review)
                && let Err(error) = self.update_math_expression_inner(id, review.text, true)
            {
                self.status = format!("Live math: {error}");
            }
            changed = true;
        }
        if self.math_after_ocr && !self.recognition_pending {
            self.math_after_ocr = false;
            if let Some(review) = self.recognition_review.take()
                && let Some(session) = &mut self.math_session
            {
                if self.math_ocr_next {
                    session.request.next_line = review.text;
                } else {
                    session.request.expression = review.text;
                    session.sources.pdf_source = review.pdf_source;
                    session.sources.bounds = review.bounds;
                    session.sources.objects = review.sources;
                }
                session.revision += 1;
            }
            changed = true;
        }
        while let Ok(completion) = self.math_service.results.try_recv() {
            if completion.generation != self.math_generation {
                continue;
            }
            changed = true;
            match completion.purpose {
                Purpose::Panel(sources) | Purpose::Edit { sources, .. } => {
                    let current = self.math_sources_current(&sources)
                        && self.math_variables().ok().as_ref()
                            == Some(&completion.request.variables);
                    if let Some(session) = &mut self.math_session {
                        session.pending = false;
                        session.revision += 1;
                        if !current {
                            session.error = Some(
                                "The source changed while solving; select it and reopen Solve"
                                    .into(),
                            );
                            continue;
                        }
                        match completion.result {
                            Ok(report) => {
                                session.report = Some(report);
                                session.revealed = usize::MAX;
                            }
                            Err(error) => session.error = Some(error),
                        }
                    }
                }
                Purpose::Live {
                    note,
                    page,
                    before,
                    signature,
                    link,
                    sources,
                    ink_version,
                } => {
                    self.math_live_pending.remove(&before.id());
                    let current = self
                        .sessions
                        .get(&note)
                        .and_then(|s| s.document.page(page))
                        .and_then(|p| p.objects.get(&before.id()))
                        == Some(&before);
                    if !current
                        || note != self.active
                        || page != self.page().id
                        || sources
                            .iter()
                            .any(|o| self.page().objects.get(&o.id()) != Some(o))
                    {
                        continue;
                    }
                    if self.math_variables().ok().as_ref() != Some(&completion.request.variables) {
                        continue;
                    }
                    if let Some(region) = link.ink_region
                        && ink_version.as_deref()
                            != Some(ink_signature(&self.math_region_sources(region)).as_str())
                    {
                        continue;
                    }
                    if link.sources.len() == 1
                        && let Some(source) = self.page().objects.get(&link.sources[0])
                    {
                        let text = match source.as_ref() {
                            Object::Equation(e) if e.math_link.is_none() => Some(e.latex.as_str()),
                            Object::Text(t) => Some(t.text.as_str()),
                            _ => None,
                        };
                        if text.is_some_and(|text| text != completion.request.expression) {
                            continue;
                        }
                    }
                    let completed_id = before.id();
                    match completion.result {
                        Ok(report) => {
                            let Object::Equation(old) = before.as_ref() else {
                                continue;
                            };
                            if let Some(svg) = report.rendered_svg {
                                let mut after = old.clone();
                                after.latex = report.answer_latex;
                                after.rendered_svg = Some(svg);
                                after.math_link = Some(*link);
                                let index = self.sessions[&note]
                                    .document
                                    .page(page)
                                    .unwrap()
                                    .order
                                    .iter()
                                    .position(|id| *id == before.id())
                                    .unwrap();
                                self.commit_to(
                                    note,
                                    "Update linked math",
                                    vec![Change::Object {
                                        page,
                                        id: before.id(),
                                        before: Some(before),
                                        after: Some(Arc::new(Object::Equation(after))),
                                        index,
                                    }],
                                );
                            }
                            self.math_live_signatures.insert(completed_id, signature);
                        }
                        Err(error) => {
                            self.math_live_signatures.insert(before.id(), signature);
                            self.status = format!("Live math: {error}");
                        }
                    }
                }
            }
        }
        if self.last_math_scan.elapsed() >= Duration::from_millis(600)
            && self.interaction.is_none()
            && !self.math_session.as_ref().is_some_and(|s| s.pending)
        {
            self.last_math_scan = Instant::now();
            let variables = match self.math_variables() {
                Ok(v) => v,
                Err(e) => {
                    self.status = e;
                    return changed;
                }
            };
            let mut candidates: Vec<_> = self
                .page()
                .ordered_objects()
                .filter_map(|o| {
                    if let Object::Equation(e) = o.as_ref()
                        && let Some(link) = &e.math_link
                        && link.live
                    {
                        Some((o.clone(), link.clone()))
                    } else {
                        None
                    }
                })
                .skip(self.math_scan_cursor)
                .take(32)
                .collect();
            if candidates.is_empty() {
                self.math_scan_cursor = 0;
                candidates = self
                    .page()
                    .ordered_objects()
                    .filter_map(|o| match o.as_ref() {
                        Object::Equation(e) if e.math_link.as_ref().is_some_and(|l| l.live) => {
                            Some((o.clone(), e.math_link.clone().unwrap()))
                        }
                        _ => None,
                    })
                    .take(32)
                    .collect();
            }
            self.math_scan_cursor += candidates.len();
            for (before, mut link) in candidates {
                if self.math_live_pending.contains(&before.id()) {
                    continue;
                }
                if let Some(region) = link.ink_region {
                    let sources = self.math_region_sources(region);
                    // Fingerprint immutable object versions without serializing raw pen samples.
                    let signature = ink_signature(&sources);
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        self.math_ink_signatures.entry(before.id())
                    {
                        entry.insert(signature.clone());
                    } else if self.math_ink_signatures.get(&before.id()) != Some(&signature)
                        && !sources.is_empty()
                        && !self.recognition_pending
                        && self.math_live_ocr.is_none()
                        && self.last_math_ink.elapsed() >= Duration::from_millis(700)
                    {
                        if self.recognize_math_sources(sources, region).is_ok() {
                            self.math_live_ocr = Some((before.id(), signature));
                        }
                        continue;
                    }
                }
                if link.sources.len() == 1
                    && let Some(source) = self.page().objects.get(&link.sources[0])
                {
                    match source.as_ref() {
                        Object::Equation(e) if e.math_link.is_none() => {
                            link.expression = e.latex.clone()
                        }
                        Object::Text(t) => link.expression = t.text.clone(),
                        _ => {}
                    }
                }
                if link
                    .sources
                    .iter()
                    .any(|id| !self.page().objects.contains_key(id))
                {
                    self.status =
                        "Live math source was removed; edit the calculation to reattach it".into();
                    continue;
                }
                let request = request_from_link(&link, variables.clone());
                let signature = serde_json::to_string(&request).unwrap_or_default();
                if self.math_live_signatures.get(&before.id()) == Some(&signature) {
                    continue;
                }
                let id = before.id();
                let purpose = Purpose::Live {
                    note: self.active,
                    page: self.page().id,
                    before,
                    signature,
                    sources: link
                        .sources
                        .iter()
                        .filter_map(|id| self.page().objects.get(id).cloned())
                        .collect(),
                    ink_version: link
                        .ink_region
                        .map(|r| ink_signature(&self.math_region_sources(r))),
                    link: Box::new(link),
                };
                if self
                    .math_service
                    .submit(Task {
                        generation: self.math_generation,
                        pack: self.math_pack(),
                        request,
                        purpose,
                    })
                    .is_ok()
                {
                    self.math_live_pending.insert(id);
                }
            }
        }
        changed
    }
}
fn ink_signature(sources: &[Arc<Object>]) -> String {
    let mut versions: Vec<_> = sources
        .iter()
        .map(|o| (o.id(), Arc::as_ptr(o) as usize))
        .collect();
    versions.sort_unstable();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    versions.hash(&mut hash);
    format!("{:016x}", hash.finish())
}
fn request_from_link(link: &MathLink, variables: BTreeMap<String, String>) -> MathRequest {
    MathRequest {
        expression: link.expression.clone(),
        operation: link.operation.clone(),
        variable: link.variable.clone(),
        domain: link.domain.clone(),
        angle: link.angle.clone(),
        method: link.method.clone(),
        x_min: link.x_min,
        x_max: link.x_max,
        variables,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Controller, PathBuf) {
        let root = std::env::temp_dir().join(format!("folio-solver-test-{}", Id::new_v4()));
        (Controller::open(root.clone()).unwrap(), root)
    }
    fn wait(app: &mut Controller) {
        let start = Instant::now();
        while app.math_session.as_ref().is_some_and(|s| s.pending)
            || !app.math_live_pending.is_empty()
            || app.recognition_pending
            || app.math_live_ocr.is_some()
        {
            app.tick();
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "Math service did not finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn run(app: &mut Controller, expression: &str, operation: &str) {
        app.open_math_solver().unwrap();
        app.run_math_solver(MathRequest {
            expression: expression.into(),
            operation: operation.into(),
            ..Default::default()
        })
        .unwrap();
        wait(app);
        assert!(
            app.math_session.as_ref().unwrap().error.is_none(),
            "{:?}",
            app.math_session.as_ref().unwrap().error
        );
    }
    fn clean(mut app: Controller, root: PathBuf) {
        app.close_math_solver();
        app.flush().unwrap();
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn solution_keeps_sources_and_worked_insertion_is_one_durable_command() {
        let (mut app, root) = fixture();
        app.add_text("2x+3=11".into(), Point::new(60., 80.));
        app.select_all();
        let originals = app.page().objects.clone();
        run(&mut app, "2x+3=11", "auto");
        let report = app.math_session.as_ref().unwrap().report.as_ref().unwrap();
        assert_eq!(report.answer, "{4}");
        assert!(report.preview.is_none());
        assert!(!report.vector.as_ref().unwrap().paths.is_empty());
        assert!(
            report
                .steps
                .iter()
                .all(|step| step.preview.is_none() && step.vector.is_some())
        );
        assert_eq!(app.page().objects, originals);
        app.insert_math_result(true, false).unwrap();
        assert!(app.page().objects.len() > 2);
        assert!(
            app.page()
                .objects
                .values()
                .any(|o| matches!(o.as_ref(),Object::Equation(e) if e.rendered_svg.is_some()))
        );
        app.undo();
        assert_eq!(app.page().objects, originals);
        app.redo();
        let inserted = app.page().objects.clone();
        app.flush().unwrap();
        drop(app);
        let mut app = Controller::open(root.clone()).unwrap();
        assert_eq!(app.page().objects, inserted);
        app.undo();
        assert_eq!(app.page().objects, originals);
        clean(app, root);
    }
    #[test]
    fn edited_latex_is_native_static_undoable_and_invalid_edits_preserve_it() {
        let (mut app, root) = fixture();
        run(&mut app, "2x+3=11", "auto");
        let original = app
            .math_session
            .as_ref()
            .unwrap()
            .report
            .as_ref()
            .unwrap()
            .answer_latex
            .clone();
        let latex = r"x = \frac{8}{2}";
        app.edit_math_result(latex.into()).unwrap();
        assert!(app.insert_math_result(false, false).is_err());
        wait(&mut app);
        let report = app.math_session.as_ref().unwrap().report.as_ref().unwrap();
        assert_eq!(report.answer_latex, latex);
        assert_eq!(report.status, "edited");
        assert!(report.vector.is_some() && report.preview.is_none());
        assert!(report.steps.is_empty() && report.assignment.is_none());
        assert!(app.insert_math_result(false, true).is_err());
        let before = app.page().objects.clone();
        app.insert_math_result(false, false).unwrap();
        assert!(app.page().objects.values().any(|o| matches!(o.as_ref(), Object::Equation(e) if e.latex == latex && e.math_link.is_none() && e.rendered_svg.as_ref().is_some_and(|s| s.contains("<path")))));
        app.undo();
        assert_eq!(app.page().objects, before);
        app.redo();
        let inserted = app.page().objects.clone();
        app.edit_math_result(r"\frac{".into()).unwrap();
        wait(&mut app);
        let session = app.math_session.as_ref().unwrap();
        assert!(session.error.is_some());
        assert_eq!(session.report.as_ref().unwrap().answer_latex, latex);
        assert_eq!(app.page().objects, inserted);
        assert!(app.edit_math_result(r"\input{file}".into()).is_err());
        app.edit_math_result(r"x = 100".into()).unwrap();
        app.cancel_math_solver();
        for _ in 0..10 {
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            app.math_session
                .as_ref()
                .unwrap()
                .report
                .as_ref()
                .unwrap()
                .answer_latex,
            latex
        );
        app.run_math_solver(MathRequest {
            expression: "2x+3=11".into(),
            ..Default::default()
        })
        .unwrap();
        wait(&mut app);
        let report = app.math_session.as_ref().unwrap().report.as_ref().unwrap();
        assert_eq!(report.answer_latex, original);
        assert_ne!(report.status, "edited");
        assert!(!report.steps.is_empty());
        clean(app, root);
    }
    #[test]
    fn edited_source_and_cancelled_completion_cannot_insert() {
        let (mut app, root) = fixture();
        app.add_text("x+1=3".into(), Point::new(20., 20.));
        app.select_all();
        app.open_math_solver().unwrap();
        let request = app.math_session.as_ref().unwrap().request.clone();
        app.run_math_solver(request.clone()).unwrap();
        app.transform_selection(Transform::translate(10., 0.), "Move");
        wait(&mut app);
        assert!(app.math_session.as_ref().unwrap().report.is_none());
        assert!(app.insert_math_result(false, false).is_err());
        app.open_math_solver().unwrap();
        app.run_math_solver(request).unwrap();
        app.cancel_math_solver();
        for _ in 0..30 {
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(app.math_session.as_ref().unwrap().report.is_none());
        clean(app, root);
    }
    #[test]
    fn offpage_solution_is_discarded() {
        let (mut app, root) = fixture();
        app.open_math_solver().unwrap();
        app.run_math_solver(MathRequest {
            expression: "x^2=4".into(),
            ..Default::default()
        })
        .unwrap();
        app.add_page();
        wait(&mut app);
        assert!(app.math_session.as_ref().unwrap().report.is_none());
        clean(app, root);
    }
    #[test]
    fn live_variable_dependency_recomputes_after_edit_and_reopen() {
        let (mut app, root) = fixture();
        run(&mut app, "a:=5", "assign");
        app.insert_math_result(false, false).unwrap();
        let assignment = *app.session().selection.iter().next().unwrap();
        app.session_mut().selection.clear();
        run(&mut app, "a*2=", "auto");
        assert_eq!(
            app.math_session
                .as_ref()
                .unwrap()
                .report
                .as_ref()
                .unwrap()
                .answer,
            "10"
        );
        app.insert_math_result(false, true).unwrap();
        let result = *app.session().selection.iter().next().unwrap();
        app.close_math_solver();
        app.update_math_expression(assignment, "a:=7".into())
            .unwrap();
        wait(&mut app);
        app.last_math_scan = Instant::now() - Duration::from_secs(1);
        app.tick();
        wait(&mut app);
        assert_eq!(app.page().objects[&result].searchable_text(), "14");
        app.flush().unwrap();
        drop(app);
        let app = Controller::open(root.clone()).unwrap();
        assert_eq!(app.math_variables().unwrap()["a"], "7");
        assert!(
            matches!(app.page().objects[&result].as_ref(),Object::Equation(e) if e.math_link.as_ref().unwrap().live)
        );
        clean(app, root);
    }
    #[test]
    fn graph_and_next_step_check_use_the_same_panel_service() {
        let (mut app, root) = fixture();
        run(&mut app, "y=1/x", "graph");
        assert!(
            app.math_session
                .as_ref()
                .unwrap()
                .report
                .as_ref()
                .unwrap()
                .graph
                .is_some()
        );
        app.insert_math_result(false, true).unwrap();
        assert!(app.page().objects.values().any(|o| matches!(o.as_ref(),Object::Equation(e) if e.math_link.as_ref().is_some_and(|l| l.operation=="graph"))));
        app.session_mut().selection.clear();
        app.open_math_solver().unwrap();
        app.run_math_solver(MathRequest {
            expression: "x*(x-1)=0".into(),
            operation: "check".into(),
            next_line: "x-1=0".into(),
            ..Default::default()
        })
        .unwrap();
        wait(&mut app);
        assert_eq!(
            app.math_session
                .as_ref()
                .unwrap()
                .report
                .as_ref()
                .unwrap()
                .status,
            "incorrect"
        );
        assert!(app.insert_math_result(false, false).is_err());
        clean(app, root);
    }
    #[test]
    fn edited_panel_problem_does_not_follow_unrelated_selected_text() {
        let (mut app, root) = fixture();
        app.add_text("2x+3=11".into(), Point::new(20., 20.));
        app.select_all();
        run(&mut app, "8+2=", "auto");
        app.insert_math_result(false, true).unwrap();
        let result = *app.session().selection.iter().next().unwrap();
        assert!(
            matches!(app.page().objects[&result].as_ref(),Object::Equation(e) if e.math_link.as_ref().unwrap().sources.is_empty())
        );
        app.close_math_solver();
        app.last_math_scan = Instant::now() - Duration::from_secs(1);
        app.tick();
        wait(&mut app);
        assert_eq!(app.page().objects[&result].searchable_text(), "10");
        app.open_math_solver().unwrap();
        app.run_math_solver(MathRequest {
            expression: "4+3=".into(),
            ..Default::default()
        })
        .unwrap();
        wait(&mut app);
        app.insert_math_result(false, false).unwrap();
        let another = *app.session().selection.iter().next().unwrap();
        assert!(
            !app.page().objects[&result]
                .bounds()
                .intersects(app.page().objects[&another].bounds())
        );
        app.session_mut().selection = app.page().objects.keys().copied().collect();
        let copy = app.copy_objects();
        app.paste_objects(copy);
        app.session().document.validate().unwrap();
        clean(app, root);
    }
    fn mock_ocr(app: &Controller, root: &std::path::Path) {
        let pack: serde_json::Value =
            serde_json::from_slice(&std::fs::read(app.math_pack()).unwrap()).unwrap();
        let dir = root.join("recognition");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("worker.py"),
            r#"import sys,json,os
for line in sys.stdin:
    request=json.loads(line)
    assert request['kind']=='math'
    if 'image_path' in request:
        assert os.path.isfile(request['image_path'])
        assert 'strokes' not in request
        text='2x+3=11'
    else: text=str(len(request['strokes']))+'+2='
    print(json.dumps({'text':text}),flush=True)
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("pack.json"),
            serde_json::to_vec(&serde_json::json!({"python":pack["python"],"worker":"worker.py"}))
                .unwrap(),
        )
        .unwrap();
    }
    fn draw_ink(app: &mut Controller, x: f32) -> Id {
        app.set_tool(Tool::Pen);
        for (i, phase) in [Phase::Down, Phase::Move, Phase::Up]
            .into_iter()
            .enumerate()
        {
            app.pointer(PenEvent {
                device: folio_input::Device::Tablet,
                tool: PenTool::Pen,
                phase,
                position: app
                    .session()
                    .viewport
                    .to_screen(Point::new(x + i as f32 * 10., 50. + i as f32 * 12.)),
                pressure: 0.7,
                tilt_x: 0.,
                tilt_y: 0.,
                buttons: 0,
                timestamp: i as u64 * 16,
            });
        }
        *app.page().order.last().unwrap()
    }
    #[test]
    fn live_handwriting_ocr_recomputes_after_pen_lifts_and_preserves_raw_ink() {
        let (mut app, root) = fixture();
        mock_ocr(&app, &root);
        let first = draw_ink(&mut app, 20.);
        let original = app.page().objects[&first].clone();
        app.session_mut().selection = HashSet::from([first]);
        app.open_math_solver().unwrap();
        wait(&mut app);
        assert_eq!(
            app.math_session.as_ref().unwrap().request.expression,
            "1+2="
        );
        app.run_math_solver(app.math_session.as_ref().unwrap().request.clone())
            .unwrap();
        wait(&mut app);
        app.insert_math_result(false, true).unwrap();
        let result = *app.session().selection.iter().next().unwrap();
        app.close_math_solver();
        draw_ink(&mut app, 80.);
        app.last_math_scan = Instant::now() - Duration::from_secs(1);
        app.last_math_ink = Instant::now() - Duration::from_secs(2);
        app.tick();
        wait(&mut app);
        assert_eq!(app.page().objects[&result].searchable_text(), "4");
        assert_eq!(app.page().objects[&first], original);
        clean(app, root);
    }
    #[test]
    fn image_math_flows_through_review_and_solver_without_replacing_image() {
        let (mut app, root) = fixture();
        mock_ocr(&app, &root);
        std::fs::create_dir_all(&app.assets).unwrap();
        image::RgbImage::from_pixel(80, 40, image::Rgb([255, 255, 255]))
            .save(app.assets.join("problem.png"))
            .unwrap();
        let object = Object::Image(ImageObject {
            id: Id::new_v4(),
            asset: "problem.png".into(),
            rect: Rect::new(20., 30., 160., 80.),
            transform: Transform::default(),
            crop: Some(Rect::new(0., 0., 0.5, 1.)),
        });
        let id = object.id();
        let page = app.page().id;
        app.commit(
            "Image",
            vec![Change::Object {
                page,
                id,
                before: None,
                after: Some(Arc::new(object)),
                index: 0,
            }],
        );
        let original = app.page().objects[&id].clone();
        app.session_mut().selection = HashSet::from([id]);
        app.open_math_solver().unwrap();
        wait(&mut app);
        assert_eq!(
            app.math_session.as_ref().unwrap().request.expression,
            "2x+3=11"
        );
        app.run_math_solver(app.math_session.as_ref().unwrap().request.clone())
            .unwrap();
        wait(&mut app);
        app.insert_math_result(false, false).unwrap();
        assert_eq!(app.page().objects[&id], original);
        clean(app, root);
    }
    #[test]
    fn pdf_region_uses_local_rendering_and_rejects_changed_background() {
        let (mut app, root) = fixture();
        mock_ocr(&app, &root);
        let source = root.join("problem.pdf");
        app.add_text("2x+3=11".into(), Point::new(60., 80.));
        folio_export::pdf(&app.session().document, &app.assets, &source).unwrap();
        let mut pages = folio_pdf::import(&source, &app.assets).unwrap();
        app.apply_pdf(app.active, std::mem::take(&mut pages));
        let pdf_index = app
            .session()
            .document
            .pages
            .iter()
            .position(|p| p.properties.pdf.is_some())
            .unwrap();
        app.change_page(pdf_index);
        app.open_math_solver().unwrap();
        assert!(app.read_pdf_math(Rect::new(0., 0., 1.1, 0.5)).is_err());
        app.read_pdf_math(Rect::new(0., 0., 0.7, 0.4)).unwrap();
        wait(&mut app);
        assert_eq!(
            app.math_session.as_ref().unwrap().request.expression,
            "2x+3=11"
        );
        let snapshot = app.math_session.as_ref().unwrap().sources.clone();
        assert!(app.math_sources_current(&snapshot));
        app.session_mut().document.pages[pdf_index].properties.pdf = None;
        assert!(!app.math_sources_current(&snapshot));
        clean(app, root);
    }
}
