use folio_document::*;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};
#[derive(Clone, Copy, Debug)]
pub enum ExportKind {
    Notebook,
    Svg,
    Png,
    Pdf,
    Text,
}
pub enum Job {
    Backup {
        root: PathBuf,
        path: PathBuf,
    },
    Restore {
        path: PathBuf,
        destination: PathBuf,
    },
    ImportNotebook {
        note: Id,
        path: PathBuf,
        assets: PathBuf,
    },
    LibraryPreview {
        note: Id,
        updated: u64,
        cover: Option<Id>,
        database: PathBuf,
    },
    Cleanup {
        root: PathBuf,
    },
    PdfPreview {
        background: PdfBackground,
        assets: PathBuf,
    },
    Preview {
        pixels_budget: u32,
        note: Id,
        page: Id,
        object: std::sync::Arc<Object>,
        assets: PathBuf,
    },
    Search {
        query: String,
        database: PathBuf,
        generation: u64,
    },
    Load {
        id: Id,
        database: PathBuf,
    },
    Export {
        doc: Document,
        page: Id,
        assets: PathBuf,
        path: PathBuf,
        kind: ExportKind,
    },
    ImportPdf {
        note: Id,
        path: PathBuf,
        assets: PathBuf,
        password: Option<String>,
    },
    ImportImage {
        note: Id,
        page: Id,
        path: PathBuf,
        assets: PathBuf,
        position: Point,
    },
    Equation {
        existing: Option<Arc<Object>>,
        note: Id,
        page: Id,
        latex: String,
        rect: Rect,
    },
    RecognizedEquation {
        generation: u64,
        review: crate::recognition::RecognitionReview,
    },
}
pub enum Finished {
    Restored(PathBuf),
    NotebookImported {
        note: Id,
        document: Document,
    },
    LibraryPreview {
        note: Id,
        updated: u64,
        result: Result<Option<(Page, usize)>, String>,
    },
    Cleaned(usize),
    Equation {
        note: Id,
        page: Id,
        before: Option<Arc<Object>>,
        after: Object,
    },
    RecognizedEquation {
        generation: u64,
        review: crate::recognition::RecognitionReview,
        result: Result<Equation, String>,
    },
    PdfPreview(String),
    PasswordRequired(Id, PathBuf),
    Preview {
        pixels_budget: u32,
        note: Id,
        page: Id,
        object: std::sync::Arc<Object>,
        bounds: Rect,
        width: u32,
        height: u32,
        bgra: Vec<u8>,
    },
    Search {
        generation: u64,
        results: Vec<folio_search::SearchResult>,
    },
    Loaded(Document, History),
    Exported(PathBuf),
    LoadError {
        id: Id,
        message: String,
    },
    ImportError {
        note: Id,
        message: String,
    },
    Pdf {
        note: Id,
        pages: Vec<Page>,
    },
    Image {
        note: Id,
        page: Id,
        object: Object,
    },
    Cancelled,
    PreviewError {
        note: Id,
        page: Id,
        object: Arc<Object>,
        message: String,
    },
    PdfPreviewError {
        asset: String,
        message: String,
    },
    Error(String),
}
struct Task {
    job: Job,
    token: Arc<AtomicU64>,
    generation: u64,
}
type JobKey = (Id, Id, u8);
pub struct Workers {
    documents: mpsc::SyncSender<Task>,
    search: mpsc::SyncSender<Task>,
    generations: Mutex<std::collections::HashMap<JobKey, Arc<AtomicU64>>>,
    pub results: mpsc::Receiver<Finished>,
}
impl Workers {
    pub fn new() -> Self {
        let (tx, results) = mpsc::sync_channel(8);
        let spawn = |name: &str| {
            let (sender, receiver) = mpsc::sync_channel::<Task>(16);
            let tx = tx.clone();
            thread::Builder::new()
                .name(name.into())
                .spawn(move || {
                    while let Ok(task) = receiver.recv() {
                        let cancelled = || task.token.load(Ordering::Acquire) != task.generation;
                        let result = if cancelled() {
                            Finished::Cancelled
                        } else {
                            let load_note = match &task.job {
                                Job::Load { id, .. } => Some(*id),
                                _ => None,
                            };
                            let import_note = match &task.job {
                                Job::ImportPdf { note, .. }
                                | Job::ImportImage { note, .. }
                                | Job::ImportNotebook { note, .. } => Some(*note),
                                _ => None,
                            };
                            let failure = match &task.job {
                                Job::Preview {
                                    note, page, object, ..
                                } => Some((Some((*note, *page, object.clone())), None)),
                                Job::PdfPreview { background, .. } => {
                                    Some((None, background.preview_asset.clone()))
                                }
                                _ => None,
                            };
                            let result =
                                process(task.job).unwrap_or_else(|message| match failure {
                                    Some((Some((note, page, object)), _)) => {
                                        Finished::PreviewError {
                                            note,
                                            page,
                                            object,
                                            message,
                                        }
                                    }
                                    Some((_, Some(asset))) => {
                                        Finished::PdfPreviewError { asset, message }
                                    }
                                    _ => match import_note {
                                        Some(note) => Finished::ImportError { note, message },
                                        None => match load_note {
                                            Some(id) => Finished::LoadError { id, message },
                                            None => Finished::Error(message),
                                        },
                                    },
                                });
                            if cancelled() {
                                Finished::Cancelled
                            } else {
                                result
                            }
                        };
                        if tx.send(result).is_err() {
                            break;
                        }
                    }
                })
                .expect("start offline worker");
            sender
        };
        Self {
            documents: spawn("folio-documents"),
            search: spawn("folio-search"),
            generations: Mutex::new(std::collections::HashMap::new()),
            results,
        }
    }
    pub fn submit(&self, job: Job) -> Result<(), String> {
        let sender = match &job {
            Job::Search { .. } | Job::Load { .. } | Job::LibraryPreview { .. } => &self.search,
            _ => &self.documents,
        };
        let key = match &job {
            Job::Search { .. } => Some((Id::nil(), Id::nil(), 1)),
            _ => None,
        };
        let mut map = self
            .generations
            .lock()
            .map_err(|_| "Worker queue unavailable")?;
        map.retain(|_, token| Arc::strong_count(token) > 1);
        let token = if let Some(key) = key {
            map.entry(key)
                .or_insert_with(|| Arc::new(AtomicU64::new(0)))
                .clone()
        } else {
            Arc::new(AtomicU64::new(0))
        };
        let generation = token.load(Ordering::Acquire) + 1;
        // The worker waits on this shared version, so update before it can start.
        let old = token.swap(generation, Ordering::AcqRel);
        match sender.try_send(Task {
            job,
            token: token.clone(),
            generation,
        }) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => {
                token.store(old, Ordering::Release);
                Err("Background queue is full; try again after pending work finishes".into())
            }
            Err(_) => Err("Background worker stopped".into()),
        }
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        if let Ok(map) = self.generations.lock() {
            for token in map.values() {
                token.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
}
fn process(job: Job) -> Result<Finished, String> {
    (|| -> Result<Finished, Box<dyn std::error::Error>> {
        Ok(match job {
            Job::Backup { root, path } => {
                crate::portable::backup(&root, &path)?;
                Finished::Exported(path)
            }
            Job::Restore { path, destination } => {
                crate::portable::restore(&path, &destination)?;
                Finished::Restored(destination)
            }
            Job::ImportNotebook { note, path, assets } => Finished::NotebookImported {
                note,
                document: crate::portable::import_notebook(&path, &assets)?,
            },
            Job::LibraryPreview {
                note,
                updated,
                cover,
                database,
            } => Finished::LibraryPreview {
                note,
                updated,
                result: folio_storage::Store::open_reader(database)
                    .and_then(|store| store.library_preview(note, cover))
                    .map_err(|e| e.to_string()),
            },
            Job::Cleanup { root } => {
                Finished::Cleaned(folio_storage::recovery::quarantine_orphans(&root)?)
            }
            Job::PdfPreview { background, assets } => {
                folio_pdf::render_preview(&background, &assets)?;
                Finished::PdfPreview(background.preview_asset.unwrap_or_default())
            }
            Job::Preview {
                pixels_budget,
                note,
                page,
                object,
                assets,
            } => {
                let (bounds, pixmap) =
                    folio_export::raster_object_limited(&object, &assets, pixels_budget)?;
                let width = pixmap.width();
                let height = pixmap.height();
                let mut bgra = pixmap.take();
                for p in bgra.as_chunks_mut::<4>().0 {
                    let a = p[3] as u32;
                    for c in p.iter_mut().take(3) {
                        *c = (*c as u32 * 255 + a / 2)
                            .checked_div(a)
                            .unwrap_or(0)
                            .min(255) as u8;
                    }
                    p.swap(0, 2);
                }
                Finished::Preview {
                    pixels_budget,
                    note,
                    page,
                    object,
                    bounds,
                    width,
                    height,
                    bgra,
                }
            }
            Job::Search {
                query,
                database,
                generation,
            } => {
                let store = folio_storage::Store::open_reader(database)?;
                Finished::Search {
                    generation,
                    results: folio_search::search(&store.connection, &query)?,
                }
            }
            Job::Load { id, database } => {
                let store = folio_storage::Store::open_reader(database)?;
                let (document, history) = store
                    .load_with_history(id)?
                    .ok_or("Note no longer exists")?;
                Finished::Loaded(document, history)
            }
            Job::Export {
                doc,
                page,
                assets,
                path,
                kind,
            } => {
                let p = doc.page(page).ok_or("Page no longer exists")?;
                match kind {
                    ExportKind::Notebook => crate::portable::export_notebook(&doc, &assets, &path)?,
                    ExportKind::Svg => {
                        if let Some(bg) = &p.properties.pdf {
                            folio_pdf::render_preview(bg, &assets)?;
                        }
                        folio_export::svg(p, &assets, &path)?;
                    }
                    ExportKind::Png => {
                        if let Some(bg) = &p.properties.pdf {
                            folio_pdf::render_preview(bg, &assets)?;
                        }
                        folio_export::png(p, &assets, &path, 2.)?;
                    }
                    ExportKind::Pdf => folio_export::pdf(&doc, &assets, &path)?,
                    ExportKind::Text => {
                        folio_export::atomic_write(&path, folio_export::text(&doc).as_bytes())?
                    }
                }
                Finished::Exported(path)
            }
            Job::ImportPdf {
                note,
                path,
                assets,
                password,
            } => match folio_pdf::import_with_password(&path, &assets, password.as_deref()) {
                Ok(pages) => Finished::Pdf { note, pages },
                Err(folio_pdf::Error::PasswordRequired) => Finished::PasswordRequired(note, path),
                Err(e) => return Err(Box::new(e)),
            },
            Job::ImportImage {
                note,
                page,
                path,
                assets,
                position,
            } => {
                let reader = image::ImageReader::open(&path)?.with_guessed_format()?;
                let size = reader.into_dimensions()?;
                if size.0 as u64 * size.1 as u64 > 32_000_000 {
                    return Err("Image exceeds 32 million pixels".into());
                }
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
                    return Err("Supported images: PNG, JPEG, WebP".into());
                }
                let id = Id::new_v4();
                let target = folio_platform::copy_asset(&path, &assets, &id.to_string(), &ext)?;
                let scale = (500. / size.0 as f32).min(1.);
                Finished::Image {
                    note,
                    page,
                    object: Object::Image(ImageObject {
                        id,
                        asset: target.file_name().unwrap().to_string_lossy().into_owned(),
                        rect: Rect::new(
                            position.x,
                            position.y,
                            size.0 as f32 * scale,
                            size.1 as f32 * scale,
                        ),
                        transform: Transform::default(),
                        crop: None,
                    }),
                }
            }
            Job::Equation {
                existing,
                note,
                page,
                latex,
                rect,
            } => {
                let source = existing.as_ref().and_then(|o| {
                    if let Object::Equation(e) = o.as_ref() {
                        Some(e)
                    } else {
                        None
                    }
                });
                let mut equation = folio_math::equation(
                    latex,
                    source.map(|e| e.source_strokes.clone()).unwrap_or_default(),
                    rect,
                )?;
                if let Some(source) = source {
                    equation.id = source.id;
                    equation.transform = source.transform;
                }
                Finished::Equation {
                    note,
                    page,
                    before: existing,
                    after: Object::Equation(equation),
                }
            }
            Job::RecognizedEquation { generation, review } => {
                let bounds = review.bounds;
                let rect = Rect::new(
                    bounds.min.x,
                    bounds.min.y,
                    bounds.width().max(80.),
                    bounds.height().max(32.),
                );
                let result = folio_math::equation(review.text.clone(), Vec::new(), rect)
                    .map_err(|error| error.to_string());
                Finished::RecognizedEquation {
                    generation,
                    review,
                    result,
                }
            }
        })
    })()
    .map_err(|e| e.to_string())
}
