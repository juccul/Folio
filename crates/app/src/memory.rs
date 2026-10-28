//! Estimates for retained model caches, counting shared ink buffers only once.
use super::*;
use std::mem::size_of;

#[derive(Default)]
struct Usage {
    bytes: usize,
    objects: HashSet<usize>,
    buffers: HashSet<usize>,
}
impl Usage {
    fn add(&mut self, bytes: usize) {
        self.bytes = self.bytes.saturating_add(bytes);
    }
    fn buffer<T>(&mut self, buffer: &Arc<Vec<T>>) {
        if self.buffers.insert(Arc::as_ptr(buffer) as usize) {
            self.add(size_of::<Vec<T>>() + 2 * size_of::<usize>());
            self.add(buffer.capacity().saturating_mul(size_of::<T>()));
        }
    }
    fn object(&mut self, object: &Arc<Object>) {
        if !self.objects.insert(Arc::as_ptr(object) as usize) {
            return;
        }
        self.add(size_of::<Object>() + 2 * size_of::<usize>());
        match object.as_ref() {
            Object::Stroke(stroke) => {
                self.buffer(&stroke.raw);
                self.buffer(&stroke.path);
                for path in [&stroke.fragment_path, &stroke.refined_path]
                    .into_iter()
                    .flatten()
                {
                    self.add(path.capacity() * size_of::<PathPoint>());
                }
            }
            Object::Text(text) => self.add(text.text.capacity() + text.font_family.capacity()),
            Object::Image(image) => self.add(image.asset.capacity()),
            Object::Shape(shape) => self.add(
                shape.vertices.capacity() * size_of::<Point>()
                    + shape.source_strokes.capacity() * size_of::<Id>(),
            ),
            Object::Equation(equation) => {
                self.add(
                    equation.latex.capacity()
                        + equation.rendered_svg.as_ref().map_or(0, String::capacity)
                        + equation.source_strokes.capacity() * size_of::<Id>(),
                );
                if let Some(link) = &equation.math_link {
                    for value in [
                        &link.expression,
                        &link.operation,
                        &link.variable,
                        &link.domain,
                        &link.angle,
                        &link.method,
                    ] {
                        self.add(value.capacity());
                    }
                    self.add(
                        link.sources.capacity() * size_of::<Id>()
                            + link.last_error.as_ref().map_or(0, String::capacity),
                    );
                }
            }
        }
    }
    fn properties(&mut self, properties: &PageProperties) {
        self.add(properties.bookmark.as_ref().map_or(0, String::capacity));
        if let Some(pdf) = &properties.pdf {
            self.add(pdf.asset.capacity() + pdf.preview_asset.as_ref().map_or(0, String::capacity));
        }
    }
    fn groups(&mut self, groups: &Vec<InkGroup>) {
        self.add(groups.capacity() * size_of::<InkGroup>());
        for group in groups {
            self.add(
                group.stroke_ids.capacity() * size_of::<Id>()
                    + group.language.capacity()
                    + group.recognized_text.as_ref().map_or(0, String::capacity)
                    + group.engine.as_ref().map_or(0, String::capacity),
            );
            self.add(group.alternatives.capacity() * size_of::<String>());
            for text in &group.alternatives {
                self.add(text.capacity());
            }
        }
    }
    fn ink_text(&mut self, entries: &Vec<InkText>) {
        self.add(entries.capacity() * size_of::<InkText>());
        for entry in entries {
            self.add(entry.text.capacity() + entry.sources.capacity() * size_of::<Id>());
        }
    }
    fn metadata(&mut self, metadata: &NoteMetadata) {
        self.add(metadata.title.capacity() + metadata.tags.capacity() * size_of::<String>());
        for tag in &metadata.tags {
            self.add(tag.capacity());
        }
    }
    fn page(&mut self, page: &Page) {
        self.properties(&page.properties);
        self.add(page.order.capacity() * size_of::<Id>());
        // B-tree occupancy is approximate; immutable samples dominate ink notes.
        self.add(page.objects.len() * (size_of::<(Id, Arc<Object>)>() + 4 * size_of::<usize>()));
        self.groups(&page.groups);
        self.ink_text(&page.ink_text);
        for object in page.objects.values() {
            self.object(object);
        }
    }
    fn command(&mut self, command: &Command) {
        self.add(command.label.capacity() + command.changes.capacity() * size_of::<Change>());
        for change in &command.changes {
            match change {
                Change::Object { before, after, .. } => {
                    for object in before.iter().chain(after.iter()) {
                        self.object(object);
                    }
                }
                Change::Page { before, after, .. } => {
                    for page in before.iter().chain(after.iter()) {
                        self.page(page);
                    }
                }
                Change::Properties { before, after, .. } => {
                    self.properties(before);
                    self.properties(after);
                }
                Change::Metadata { before, after } => {
                    self.metadata(before);
                    self.metadata(after);
                }
                Change::Groups { before, after, .. } => {
                    self.groups(before);
                    self.groups(after);
                }
                Change::InkText { before, after, .. } => {
                    self.ink_text(before);
                    self.ink_text(after);
                }
            }
        }
    }
}
impl Session {
    pub(super) fn memory_bytes(&self) -> usize {
        if let Some(bytes) = self.memory_usage.get() {
            return bytes;
        }
        let mut usage = Usage::default();
        usage.add(size_of::<Self>() + self.document.pages.capacity() * size_of::<Page>());
        usage.metadata(&self.document.metadata);
        for page in &self.document.pages {
            usage.page(page);
        }
        for (command, _) in self.history.entries() {
            usage.command(command);
        }
        if let Some(cover) = &self.library_cover {
            usage.page(cover);
        }
        for (_, text) in self.search_text.borrow().values() {
            usage.add(text.len());
        }
        usage.add(self.order_positions.capacity() * (size_of::<(Id, usize)>() + 1));
        // Allowance for active-page spatial/source lookup maps.
        usage.add(self.page().objects.len() * 128);
        self.memory_usage.set(Some(usage.bytes));
        usage.bytes
    }
}
impl Controller {
    /// Release a closed tab after its document and journal are durable. Deferred
    /// edits and background imports retain their session until it is safe.
    pub fn close_session(&mut self, id: Id) {
        if self.pending_note == Some(id) {
            self.pending_note = None;
            self.pending_navigation = None;
        }
        self.closed_sessions.insert(id);
        self.trim_caches();
    }
    pub(super) fn can_release_session(&self, id: Id) -> bool {
        !self.equation_pending
            && !self.pending_actions.contains_key(&id)
            && !self.pending_imports.contains_key(&id)
            && !self.provisional_imports.contains(&id)
            && self
                .sessions
                .get(&id)
                .is_some_and(|session| !session.history_needs_save)
    }
    pub(super) fn release_session(&mut self, id: Id) {
        if let Some(session) = self.sessions.remove(&id) {
            let page = session.page().id;
            // Keep navigation within this app run independent of the preference
            // that controls restoring documents after an application restart.
            self.session_views.retain(|(note, _, _)| *note != id);
            self.session_views.push_back((id, page, session.viewport));
            if self.session_views.len() > 128 {
                self.session_views.pop_front();
            }
            let pages = &mut self.settings.workspace.current_pages;
            if !pages.contains(&(id, page)) {
                pages.retain(|(note, _)| *note != id);
                pages.push((id, page));
                if pages.len() > 128 {
                    pages.remove(0);
                }
                self.store_settings();
            }
        }
        self.closed_sessions.remove(&id);
        self.session_recency.retain(|note| *note != id);
        self.previews.retain(|(note, _, _), _| *note != id);
        self.preview_pending.retain(|(note, _, _), _| *note != id);
        self.preview_failed.retain(|(note, _, _), _| *note != id);
        self.preview_errors.retain(|(note, _), _| *note != id);
    }
}
impl Controller {
    pub(crate) fn trim_sessions(&mut self, budget: usize) {
        if self.session_recency.back() != Some(&self.active) {
            self.session_recency.retain(|id| *id != self.active);
            self.session_recency.push_back(self.active);
        }
        if self.saved >= self.queued && self.dirty_notes.is_empty() && self.interaction.is_none() {
            let closed: Vec<_> = self.closed_sessions.iter().copied().collect();
            for id in closed {
                if !self.sessions.contains_key(&id) && !self.loading_notes.contains(&id) {
                    self.closed_sessions.remove(&id);
                } else if self.can_release_session(id)
                    && (id != self.active || self.pending_note.is_none())
                {
                    if id == self.active {
                        let mut document = Document::new("Untitled document");
                        document.pages[0].properties = self.default_page_properties();
                        let next = document.metadata.id;
                        self.sessions.insert(next, Session::new(document));
                        self.activate_note(next);
                        self.bootstrap_document = Some(next);
                        self.provisional_imports.insert(next);
                    }
                    self.release_session(id);
                }
            }
            let mut bytes = self
                .sessions
                .iter()
                .filter(|(id, _)| **id != self.active)
                .map(|(_, session)| session.memory_bytes())
                .sum::<usize>();
            while self.settings.autosave && (self.sessions.len() > 8 || bytes > budget) {
                let victim = self.session_recency.iter().copied().find(|id| {
                    *id != self.active
                        && Some(*id) != self.pending_note
                        && self.can_release_session(*id)
                });
                let Some(victim) = victim else {
                    break;
                };
                bytes = bytes.saturating_sub(self.sessions[&victim].memory_bytes());
                self.release_session(victim);
            }
        }
    }
    pub(crate) fn trim_caches(&mut self) {
        self.trim_sessions(96 * 1024 * 1024);
        const BUDGET: usize = 128 * 1024 * 1024;
        if self.previews.values().map(|p| p.bgra.len()).sum::<usize>() > BUDGET {
            let note = self.active;
            let page = self.page().id;
            self.previews
                .retain(|(n, p, _), _| *n == note && *p == page);
            let mut bytes = self.previews.values().map(|p| p.bgra.len()).sum::<usize>();
            if bytes > BUDGET {
                let visible = self
                    .session()
                    .index
                    .query(self.session().viewport.visible(1920., 1200.));
                self.previews.retain(|(_, _, id), p| {
                    let keep = visible.contains(id) || bytes <= BUDGET;
                    if !keep {
                        bytes = bytes.saturating_sub(p.bgra.len());
                    }
                    keep
                });
            }
        }
        let mut bytes = self.previews.values().map(|p| p.bgra.len()).sum::<usize>();
        if bytes > BUDGET {
            let mut candidates = self
                .previews
                .iter()
                .map(|(key, p)| (*key, p.pixels_budget))
                .collect::<Vec<_>>();
            candidates.sort_unstable_by_key(|(_, budget)| std::cmp::Reverse(*budget));
            for (key, _) in candidates {
                if bytes <= BUDGET {
                    break;
                }
                if let Some(preview) = self.previews.remove(&key) {
                    bytes = bytes.saturating_sub(preview.bgra.len());
                }
            }
        }
    }
}
