//! Importing files into documents and exporting documents.
use super::*;

/// Immutable target captured before a native export dialog can switch focus.
#[derive(Clone)]
pub struct ExportSnapshot {
    document: Document,
    page: Id,
}
impl ExportSnapshot {
    pub fn map_pages(&mut self, mut map: impl FnMut(&mut Page)) {
        for page in &mut self.document.pages {
            map(page);
        }
    }
}

impl Controller {
    pub fn prepare_export(&mut self) -> ExportSnapshot {
        self.finish();
        ExportSnapshot {
            document: self.session().document.clone(),
            page: self.page().id,
        }
    }
    pub fn export_prepared(&mut self, snapshot: ExportSnapshot, path: PathBuf, kind: ExportKind) {
        let extension = match kind {
            ExportKind::Notebook => "folio",
            ExportKind::Pdf => "pdf",
            ExportKind::Svg => "svg",
            ExportKind::Png => "png",
            ExportKind::Text => "txt",
        };
        if let Err(error) = self.validate_export_destination(&path, extension) {
            self.error = Some(error);
            return;
        }
        if self.task_running("export") {
            self.status = "Finish the current export before starting another.".into();
            return;
        }
        self.submit(Job::Export {
            doc: snapshot.document,
            page: snapshot.page,
            assets: self.assets.clone(),
            path,
            kind,
        });
    }
    pub fn export(&mut self, path: PathBuf, kind: ExportKind) {
        let snapshot = self.prepare_export();
        self.export_prepared(snapshot, path, kind);
    }
    pub fn import(&mut self, path: PathBuf) {
        self.import_into(self.active, path);
    }
    /// Home imports have their own document; editor imports retain their captured target.
    pub fn import_as_note(&mut self, path: PathBuf) -> Id {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported document")
            .to_owned();
        let properties = PageProperties {
            paper: self.settings.paper,
            ..Default::default()
        };
        let id = self
            .create_note_with_properties_inner(title, properties, false)
            .expect("Valid import placeholder");
        self.library_imports.insert(
            id,
            LibraryImport {
                path: path.clone(),
                pending: true,
                error: None,
            },
        );
        self.import_into(id, path);
        id
    }
    pub fn retry_library_import(&mut self, note: Id) {
        let Some(import) = self.library_imports.get_mut(&note) else {
            return;
        };
        if import.pending {
            return;
        }
        import.pending = true;
        import.error = None;
        let path = import.path.clone();
        self.import_into(note, path);
    }
    pub fn import_is_provisional(&self, note: Id) -> bool {
        self.provisional_imports.contains(&note)
    }
    pub fn dismiss_failed_import(&mut self, note: Id) {
        if note == self.active {
            self.finish();
        }
        if !self
            .library_imports
            .get(&note)
            .is_some_and(|import| !import.pending && import.error.is_some())
        {
            return;
        }
        self.library_imports.remove(&note);
        if self.provisional_imports.remove(&note) {
            self.notes.retain(|n| n.id != note);
            self.settings.recent_documents.retain(|id| *id != note);
            self.store_settings();
            if note == self.active {
                if let Some(id) = self.notes.iter().find(|n| !n.trashed).map(|n| n.id) {
                    self.switch_note(id);
                } else {
                    self.start_empty_session();
                }
            }
            if self.active != note {
                self.sessions.remove(&note);
            } else {
                self.provisional_imports.insert(note);
            }
            self.status = "Failed import removed".into();
        } else {
            self.status = "Document kept; failed import dismissed".into();
        }
    }
    pub(crate) fn library_import_failed(&mut self, note: Id, message: String) {
        self.import_finished(note);
        if let Some(import) = self.library_imports.get_mut(&note) {
            import.pending = false;
            import.error = Some(message);
        } else {
            self.error = Some(message);
        }
    }
    pub fn import_into(&mut self, note: Id, path: PathBuf) {
        self.manage_note(note, NoteAction::Import(path));
    }
    pub(crate) fn import_loaded(&mut self, note: Id, path: PathBuf) {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let job = if ext == "folio" {
            Job::ImportNotebook {
                note,
                path,
                assets: self.assets.clone(),
            }
        } else if ext == "pdf" {
            Job::ImportPdf {
                note,
                path,
                assets: self.assets.clone(),
                password: None,
            }
        } else {
            Job::ImportImage {
                note,
                page: self.sessions[&note].page().id,
                path,
                assets: self.assets.clone(),
                position: Point::new(80., 80.),
            }
        };
        if let Err(e) = self.workers.submit(job) {
            self.library_import_failed(note, e);
        } else {
            self.busy += 1;
            *self.pending_imports.entry(note).or_default() += 1;
        }
    }
    pub(crate) fn import_finished(&mut self, note: Id) {
        if let Some(count) = self.pending_imports.get_mut(&note) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.pending_imports.remove(&note);
            }
        }
    }
    pub(crate) fn apply_pdf(&mut self, note: Id, pages: Vec<Page>) {
        self.apply_pages(note, pages, "Import PDF", None);
    }
    pub(crate) fn apply_pages(
        &mut self,
        note: Id,
        pages: Vec<Page>,
        label: &str,
        metadata: Option<NoteMetadata>,
    ) {
        if pages.is_empty() {
            return;
        }
        let Some(s) = self.sessions.get(&note) else {
            return;
        };
        // Replace only an untouched placeholder. Never replace a page with annotations.
        let placeholder = is_blank_placeholder(&s.document);
        let index = if placeholder {
            0
        } else {
            s.document.pages.len()
        };
        let mut changes = Vec::new();
        let mut pages = pages.into_iter();
        if placeholder {
            changes.push(Change::Page {
                index: 0,
                before: Some(s.document.pages[0].clone()),
                after: pages.next(),
            });
        }
        changes.extend(pages.enumerate().map(|(i, page)| Change::Page {
            index: index + i + usize::from(placeholder),
            before: None,
            after: Some(page),
        }));
        if let Some(metadata) = metadata {
            let before = self.sessions[&note].document.metadata.clone();
            let mut after = before.clone();
            after.title = metadata.title;
            after.tags = metadata.tags;
            after.cover_page = metadata.cover_page;
            changes.push(Change::Metadata { before, after });
        }
        self.commit_to(note, label, changes);
        if note == self.active {
            self.change_page(index);
            self.session_mut().viewport = Viewport::default();
        }
    }
    pub fn cancel_pdf_import(&mut self, note: Id) {
        self.library_import_failed(note, "PDF import cancelled".into());
    }
    pub fn unlock_pdf(&mut self, note: Id, path: PathBuf, password: String) {
        self.submit(Job::ImportPdf {
            note,
            path,
            assets: self.assets.clone(),
            password: Some(password),
        });
    }
    pub(crate) fn image_imported(&mut self, note: Id, page: Id, object: Object) {
        self.import_finished(note);
        self.library_imports.remove(&note);
        if let Some(p) = self.sessions.get(&note).and_then(|s| s.document.page(page)) {
            let index = p.order.len();
            let id = object.id();
            self.commit_to(
                note,
                "Insert object",
                vec![Change::Object {
                    page,
                    id,
                    before: None,
                    after: Some(Arc::new(object)),
                    index,
                }],
            );
        }
    }
    /// Imported notebooks adopt their title and tags only when they fill an
    /// untouched placeholder document.
    pub(crate) fn notebook_imported(&mut self, note: Id, document: Document) {
        self.import_finished(note);
        self.library_imports.remove(&note);
        let empty = self
            .sessions
            .get(&note)
            .is_some_and(|s| is_blank_placeholder(&s.document));
        self.apply_pages(
            note,
            document.pages,
            "Import editable notebook",
            empty.then_some(document.metadata),
        );
    }
}
/// A document that still has only its single, empty starting page.
fn is_blank_placeholder(document: &Document) -> bool {
    document.pages.len() == 1
        && document.pages[0].objects.is_empty()
        && document.pages[0].properties.pdf.is_none()
}
