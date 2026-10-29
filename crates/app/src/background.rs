//! Background work: submitting jobs and applying their results each tick.
use super::*;

impl Controller {
    pub fn task_running(&self, kind: &str) -> bool {
        self.tasks
            .iter()
            .any(|t| t.kind == kind && t.state == TaskState::Running)
    }
    pub fn activity_status(&self) -> String {
        if self.recognition_pending {
            return self.recognition_status.clone();
        }
        let pending: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Running)
            .map(|t| t.label.as_str())
            .collect();
        if !pending.is_empty() {
            return format!(
                "{}{}",
                pending[0],
                if pending.len() > 1 {
                    format!(" · {} more operations", pending.len() - 1)
                } else {
                    String::new()
                }
            );
        }
        if let Some(import) = self.library_imports.values().find(|i| i.pending) {
            return format!(
                "Importing {}",
                import
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
        }
        if self.busy > 0 {
            return "Preparing documents…".into();
        }
        self.status.clone()
    }
    pub(crate) fn submit(&mut self, job: Job) {
        let identity = match &job {
            Job::Backup { path, .. } => Some(("backup", format!("Backing up {}", path.display()))),
            Job::Restore { path, .. } => Some(("restore", format!("Restoring {}", path.display()))),
            Job::Export { path, .. } => Some(("export", format!("Exporting {}", path.display()))),
            Job::Cleanup { .. } => Some(("cleanup", "Checking unused assets".into())),
            Job::SaveTemplate { name, .. } => Some(("template", format!("Saving template {name}"))),
            _ => None,
        };
        let id = Id::new_v4();
        let job = if identity.is_some() {
            Job::Tracked {
                id,
                job: Box::new(job),
            }
        } else {
            job
        };
        match self.workers.submit(job) {
            Ok(()) => {
                self.busy += 1;
                if let Some((kind, label)) = identity {
                    if self.tasks.len() >= 32
                        && let Some(index) = self
                            .tasks
                            .iter()
                            .position(|t| t.state != TaskState::Running)
                    {
                        self.tasks.remove(index);
                    }
                    self.tasks.push(BackgroundTask {
                        id,
                        kind,
                        label,
                        state: TaskState::Running,
                    });
                }
            }
            Err(e) => self.error = Some(e),
        }
    }
    /// Periodic maintenance, called by the UI on every poll. Collects results
    /// from background workers and services and advances time-based behavior
    /// (draft saves, hold gestures, cache trimming). Returns whether anything
    /// visible may have changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        changed |= self.poll_recognition();
        changed |= self.poll_math_solver();
        if self.pending_pdf_password.is_none() {
            self.pending_pdf_password = self.pdf_password_queue.pop_front();
            changed |= self.pending_pdf_password.is_some();
        }
        changed |= self.poll_save_receipts();
        self.retry_dirty_saves();
        changed |= self.poll_worker_results();
        changed |= self.submit_pending_search();
        self.save_ink_draft();
        changed |= self.detect_ink_hold();
        if self.last_cache_trim.elapsed() >= Duration::from_millis(250) {
            let active = self.active;
            self.trim_caches();
            changed |= active != self.active;
            self.last_cache_trim = Instant::now();
        }
        changed
    }
    fn poll_worker_results(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.workers.results.try_recv() {
            changed = true;
            self.busy = self.busy.saturating_sub(1);
            let result = if let Finished::Tracked { id, result } = result {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                    task.state = if let Finished::Error(error) = result.as_ref() {
                        TaskState::Failed(error.clone())
                    } else {
                        TaskState::Complete
                    };
                }
                *result
            } else {
                result
            };
            self.apply_finished(result);
        }
        changed
    }
    fn apply_finished(&mut self, result: Finished) {
        match result {
            Finished::Tracked { .. } => unreachable!("tracked results are unwrapped first"),
            Finished::Cancelled => {}
            Finished::Error(e) => self.error = Some(e),
            Finished::Loaded(document, history) => self.note_loaded(document, history),
            Finished::LoadError { id, message } => {
                self.loading_notes.remove(&id);
                self.pending_actions.remove(&id);
                if self.pending_note == Some(id) {
                    self.pending_note = None;
                }
                self.error = Some(message);
            }
            Finished::LibraryPreview {
                note,
                updated,
                result,
            } => self.library_preview_finished(note, updated, result),
            Finished::Search {
                generation,
                results,
            } => {
                if generation == self.search_generation {
                    self.search_results = results;
                    self.search_state = SearchState::Complete;
                }
            }
            Finished::SearchError {
                generation,
                message,
            } => {
                if generation == self.search_generation {
                    self.search_state = SearchState::Failed(message);
                }
            }
            Finished::Equation {
                generation,
                note,
                page,
                before,
                after,
            } => self.equation_rendered(generation, note, page, before, after),
            Finished::EquationError {
                generation,
                message,
            } => {
                if generation == self.equation_generation {
                    self.equation_pending = false;
                    self.equation_result = Some(Err(message));
                }
            }
            Finished::RecognizedEquation {
                generation,
                review,
                result,
            } => self.finish_recognized_equation(generation, review, result),
            Finished::Preview {
                pixels_budget,
                note,
                page,
                object,
                bounds,
                width,
                height,
                bgra,
            } => {
                self.preview_pending.remove(&(note, page, object.id()));
                self.accept_preview(
                    note,
                    page,
                    RasterPreview {
                        pixels_budget,
                        object,
                        bounds,
                        width,
                        height,
                        bgra: Arc::new(bgra),
                    },
                );
            }
            Finished::PreviewError {
                note,
                page,
                object,
                message,
            } => self.object_preview_failed(note, page, object, message),
            Finished::PdfPreview(asset) => {
                self.pdf_preview_pending.remove(&asset);
            }
            Finished::PdfPreviewError { asset, message } => self.pdf_preview_failed(asset, message),
            Finished::PasswordRequired(note, path) => {
                if self.pending_pdf_password.is_none() {
                    self.pending_pdf_password = Some((note, path));
                } else {
                    self.pdf_password_queue.push_back((note, path));
                }
            }
            Finished::Pdf { note, pages } => {
                self.library_imports.remove(&note);
                self.apply_pdf(note, pages);
                self.import_finished(note);
            }
            Finished::Image {
                note,
                page,
                object,
                select,
            } => self.image_imported(note, page, object, select),
            Finished::NotebookImported { note, document } => self.notebook_imported(note, document),
            Finished::ImportError { note, message } => {
                self.library_import_failed(note, message);
            }
            Finished::TemplateSaved(template) => {
                self.settings.templates.push(template);
                self.store_settings();
                self.status = "Page template saved".into();
            }
            Finished::TemplatePage { note, page } => self.template_page_ready(note, page),
            Finished::Exported(path) => {
                self.status = format!(
                    "Exported {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            }
            Finished::Restored(path) => {
                self.status = format!("Backup restored to {}", path.display());
                self.restored_library = Some(path);
            }
            Finished::Cleaned(count) => {
                self.status = format!(
                    "Quarantined {count} unused assets; originals can be restored from orphaned-assets"
                )
            }
        }
    }
    pub fn can_cleanup_assets(&self) -> bool {
        self.busy == 0
            && self.saved >= self.queued
            && self.dirty_notes.is_empty()
            && self.interaction.is_none()
            && self.settings.autosave
    }
    pub fn cleanup_assets(&mut self) {
        if !self.can_cleanup_assets() {
            self.error =
                Some("Wait for saved changes and background work before asset maintenance".into());
            return;
        }
        self.submit(Job::Cleanup {
            root: self.data_dir.clone(),
        });
    }
}
