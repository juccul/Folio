//! Raster previews for media objects and PDF page backgrounds.
use super::*;

pub struct RasterPreview {
    pub pixels_budget: u32,
    pub object: Arc<Object>,
    pub bounds: Rect,
    pub width: u32,
    pub height: u32,
    pub bgra: Arc<Vec<u8>>,
}

impl Controller {
    pub fn page_preview_error(&self) -> Option<&str> {
        self.preview_errors
            .get(&(self.active, self.page().id))
            .map(String::as_str)
    }
    pub fn retry_previews(&mut self) {
        self.preview_errors.clear();
        self.preview_failed.clear();
        self.pdf_preview_failed.clear();
    }
    pub fn request_pdf_preview(&mut self) {
        let Some(background) = self.page().properties.pdf.clone() else {
            return;
        };
        self.request_pdf_background(background);
    }
    pub fn request_pdf_background(&mut self, background: PdfBackground) {
        self.request_pdf_background_scaled(background, 2400);
    }
    pub fn pdf_scroll_preview_asset(background: &PdfBackground) -> Option<String> {
        background
            .preview_asset
            .as_ref()
            .map(|asset| format!("{asset}.scroll.png"))
    }
    pub fn request_pdf_scroll_preview(&mut self, mut background: PdfBackground) {
        if background
            .preview_asset
            .as_ref()
            .is_some_and(|asset| self.assets.join(asset).is_file())
        {
            return;
        }
        background.preview_asset = Self::pdf_scroll_preview_asset(&background);
        self.request_pdf_background_scaled(background, 960);
    }
    fn request_pdf_background_scaled(&mut self, background: PdfBackground, size: u32) {
        let Some(asset) = &background.preview_asset else {
            return;
        };
        if self.assets.join(asset).is_file()
            || self.pdf_preview_pending.contains(asset)
            || self.pdf_preview_failed.contains(asset)
        {
            return;
        }
        if self
            .workers
            .submit(Job::PdfPreview {
                background: background.clone(),
                assets: self.assets.clone(),
                size,
            })
            .is_ok()
        {
            self.pdf_preview_pending.insert(asset.clone());
            self.busy += 1;
        }
    }
    pub(crate) fn invalidate_previews(&mut self, note: Id, command: &Command) {
        // Keep in-flight requests tracked until completion; removing them here
        // would allow duplicate jobs before stale results are rejected.
        if self.library_previews.remove(&note).is_some() {
            self.library_preview_recency.retain(|id| *id != note);
        }
        if self.previews.is_empty() && self.preview_failed.is_empty() {
            return;
        }
        let Some(session) = self.sessions.get(&note) else {
            return;
        };
        let current = |page, object: &Arc<Object>| {
            session
                .document
                .page(page)
                .and_then(|page| page.objects.get(&object.id()))
                .is_some_and(|source| Arc::ptr_eq(source, object))
        };
        // Ordinary edits touch only their own cache keys, even in large notes.
        let mut moved = Vec::new();
        for change in &command.changes {
            if let Change::Object { page, id, .. } = change {
                let key = (note, *page, *id);
                if let Some(preview) = self.previews.get_mut(&key)
                    && !current(*page, &preview.object)
                {
                    // A move only translates the raster. Keep its pixels so the
                    // object does not vanish until a worker renders it again.
                    let object = session
                        .document
                        .page(*page)
                        .and_then(|page| page.objects.get(id));
                    if object.is_some_and(|object| translate_preview(preview, object)) {
                        moved.push((*page, *id, preview.bounds));
                    } else {
                        self.previews.remove(&key);
                    }
                }
                if self
                    .preview_failed
                    .get(&key)
                    .is_some_and(|object| !current(*page, object))
                {
                    self.preview_failed.remove(&key);
                }
            }
        }
        let pages = command
            .changes
            .iter()
            .filter_map(|change| match change {
                Change::Page { before, after, .. } => Some((before, after)),
                _ => None,
            })
            .flat_map(|(before, after)| before.iter().chain(after.iter()).map(|page| page.id))
            .collect::<HashSet<_>>();
        if !pages.is_empty() {
            self.previews.retain(|(owner, page, _), preview| {
                *owner != note || !pages.contains(page) || current(*page, &preview.object)
            });
            self.preview_failed.retain(|(owner, page, _), object| {
                *owner != note || !pages.contains(page) || current(*page, object)
            });
        }
        // Match `accept_preview`, which indexes the drawn raster bounds.
        if let Some(session) = self.sessions.get_mut(&note) {
            for (page, id, bounds) in moved {
                if session.page().id == page && self.previews.contains_key(&(note, page, id)) {
                    session.index.insert(id, bounds);
                }
            }
        }
    }
    pub(crate) fn accept_preview(&mut self, note: Id, page: Id, preview: RasterPreview) -> bool {
        let id = preview.object.id();
        if !self.is_current_object(note, page, &preview.object) {
            return false;
        }
        if let Some(session) = self.sessions.get_mut(&note)
            && session.page().id == page
        {
            session.index.insert(id, preview.bounds);
        }
        self.previews.insert((note, page, id), preview);
        true
    }
    pub fn request_previews(&mut self, visible: &HashSet<Id>) {
        self.request_page_previews(self.session().page, visible);
    }
    pub fn request_page_previews(&mut self, index: usize, visible: &HashSet<Id>) {
        let Some(page) = self.session().document.pages.get(index) else {
            return;
        };
        let objects = &page.objects;
        let page = page.id;
        let objects = visible
            .iter()
            .filter_map(|id| objects.get(id))
            .filter(|o| !matches!(o.as_ref(), Object::Stroke(_) | Object::Shape(_)))
            .cloned()
            .collect::<Vec<_>>();
        let maximum =
            (24 * 1024 * 1024_u32 / (objects.len().max(1) as u32)).clamp(1, 4 * 1024 * 1024);
        let pixels_budget = 1 << (31 - maximum.leading_zeros());
        for object in objects {
            let key = (self.active, page, object.id());
            if self.previews.get(&key).is_some_and(|p| {
                Arc::ptr_eq(&p.object, &object) && p.pixels_budget >= pixels_budget
            }) || self.preview_pending.contains_key(&key)
                || self
                    .preview_failed
                    .get(&key)
                    .is_some_and(|failed| Arc::ptr_eq(failed, &object))
            {
                continue;
            }
            if self
                .workers
                .submit(Job::Preview {
                    pixels_budget,
                    note: self.active,
                    page,
                    object: object.clone(),
                    assets: self.assets.clone(),
                })
                .is_ok()
            {
                self.preview_pending.insert(key, object);
                self.busy += 1;
            } else {
                break;
            }
        }
    }
    pub(crate) fn object_preview_failed(
        &mut self,
        note: Id,
        page: Id,
        object: Arc<Object>,
        message: String,
    ) {
        let key = (note, page, object.id());
        self.preview_pending.remove(&key);
        if self.is_current_object(note, page, &object) {
            self.preview_failed.insert(key, object);
            self.preview_errors.insert((note, page), message);
        }
    }
    /// Report a PDF preview failure on every open page that uses that asset.
    pub(crate) fn pdf_preview_failed(&mut self, asset: String, message: String) {
        self.pdf_preview_pending.remove(&asset);
        for (&note, session) in &self.sessions {
            for page in &session.document.pages {
                if page.properties.pdf.as_ref().is_some_and(|p| {
                    p.preview_asset
                        .as_ref()
                        .is_some_and(|a| a == &asset || format!("{a}.scroll.png") == asset)
                }) {
                    self.preview_errors.insert((note, page.id), message.clone());
                }
            }
        }
        self.pdf_preview_failed.insert(asset);
    }
    /// Whether `object` is still the exact object stored on that page.
    fn is_current_object(&self, note: Id, page: Id, object: &Arc<Object>) -> bool {
        self.sessions
            .get(&note)
            .and_then(|s| s.document.page(page))
            .and_then(|p| p.objects.get(&object.id()))
            .is_some_and(|current| Arc::ptr_eq(current, object))
    }
}

/// Rebase `preview` onto `object` when the edit only translated it.
fn translate_preview(preview: &mut RasterPreview, object: &Arc<Object>) -> bool {
    let (before, after) = (preview.object.transform(), object.transform());
    if (before.a, before.b, before.c, before.d) != (after.a, after.b, after.c, after.d) {
        return false;
    }
    if relative_transform(&preview.object, object).is_none() {
        return false;
    }
    let shift = Point::new(after.tx - before.tx, after.ty - before.ty);
    let Rect { min, max } = preview.bounds;
    preview.bounds = Rect {
        min: Point::new(min.x + shift.x, min.y + shift.y),
        max: Point::new(max.x + shift.x, max.y + shift.y),
    };
    preview.object = object.clone();
    true
}

/// The document-space transform that carries `before` onto `after`, when the
/// two differ only in their transforms. A raster of `before` drawn with it
/// shows `after` until a fresh raster is ready.
pub fn relative_transform(before: &Object, after: &Object) -> Option<Transform> {
    let mut unmoved = after.clone();
    unmoved.set_transform(before.transform());
    (unmoved == *before)
        .then(|| before.transform().inverse())
        .flatten()
        .map(|inverse| after.transform().compose(inverse))
}
