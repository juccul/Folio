//! Per-document editor state and the derived indexes kept in sync with it.
use super::*;

/// An open document together with its undo history, view and derived indexes.
///
/// The derived fields (`index`, `order_positions`, `linked_math`) describe only
/// the current page and must be refreshed whenever the document or the current
/// page changes: use [`Session::refresh`] after arbitrary changes and
/// `refresh_command` after applying a command.
pub struct Session {
    pub document: Document,
    pub history: History,
    /// Index of the current page in `document.pages`.
    pub page: usize,
    pub viewport: Viewport,
    pub selection: HashSet<Id>,
    pub index: SpatialIndex,
    /// Drawing-order position of each object on the current page.
    pub order_positions: HashMap<Id, usize>,
    /// Page that `index`, `order_positions` and `linked_math` were built for.
    pub(crate) indexed_page: Id,
    // Mark unsaved history without cloning every prior command on each edit.
    pub(crate) history_needs_save: bool,
    pub(crate) library_cover: Option<Arc<Page>>,
    /// Equations on the current page that are linked to live math.
    pub(crate) linked_math: HashSet<Id>,
    pub(crate) page_stack: RefCell<Option<Arc<PageStack>>>,
    pub(crate) content_bounds: RefCell<Option<(Id, u64, Option<Rect>)>>,
    pub(crate) search_text: RefCell<HashMap<Id, (u64, Arc<str>)>>,
    pub(crate) memory_usage: Cell<Option<usize>>,
}
impl Session {
    pub(crate) fn new(document: Document) -> Self {
        Self::with_page(document, None)
    }
    pub(crate) fn with_page(document: Document, initial_page: Option<Id>) -> Self {
        let page = initial_page
            .and_then(|id| document.pages.iter().position(|p| p.id == id))
            .unwrap_or(0);
        let mut session = Self {
            indexed_page: document.pages[page].id,
            document,
            linked_math: HashSet::new(),
            history: History::default(),
            page,
            viewport: Viewport::default(),
            selection: HashSet::new(),
            index: SpatialIndex::default(),
            order_positions: HashMap::new(),
            history_needs_save: false,
            library_cover: None,
            page_stack: RefCell::new(None),
            content_bounds: RefCell::new(None),
            search_text: RefCell::new(HashMap::new()),
            memory_usage: Cell::new(None),
        };
        session.refresh();
        session
    }
    pub fn page(&self) -> &Page {
        &self.document.pages[self.page]
    }
    pub fn page_mut(&mut self) -> &mut Page {
        self.memory_usage.set(None);
        self.page_stack.get_mut().take();
        self.content_bounds.get_mut().take();
        self.search_text
            .get_mut()
            .remove(&self.document.pages[self.page].id);
        &mut self.document.pages[self.page]
    }
    /// Make the page with `id` current, or the first page if it no longer exists.
    pub(crate) fn show_page(&mut self, id: Id) {
        self.page = self
            .document
            .pages
            .iter()
            .position(|page| page.id == id)
            .unwrap_or(0);
        self.refresh();
    }
    /// Reuse finite-page layout across drawing and navigation frames. Only page
    /// structure or dimensions affect it; ink revisions and paper colors do not.
    pub fn page_stack(&self) -> Option<Arc<PageStack>> {
        if self.page().properties.infinite {
            return None;
        }
        let mut cached = self.page_stack.borrow_mut();
        if cached.as_ref().is_none_or(|stack| {
            stack
                .pages
                .first()
                .zip(stack.pages.last())
                .is_none_or(|(first, last)| self.page < first.0 || self.page > last.0)
        }) {
            *cached = PageStack::new(&self.document.pages, self.page).map(Arc::new);
        }
        cached.clone()
    }
    /// Bounding box of content on the active infinite page, falling back to
    /// the initial page frame when empty. Cached by `(page.id, page.revision)`.
    pub fn infinite_content_bounds(&self) -> Rect {
        let page = self.page();
        let cached = self.content_bounds.borrow();
        if let Some((id, rev, bounds)) = *cached
            && id == page.id
            && rev == page.revision
        {
            return bounds.unwrap_or_else(|| {
                Rect::new(
                    0.,
                    0.,
                    page.properties.width.max(400.),
                    page.properties.height.max(400.),
                )
            });
        }
        drop(cached);
        let bounds = page_object_bounds(page);
        *self.content_bounds.borrow_mut() = Some((page.id, page.revision, bounds));
        bounds.unwrap_or_else(|| {
            Rect::new(
                0.,
                0.,
                page.properties.width.max(400.),
                page.properties.height.max(400.),
            )
        })
    }
    /// Bounding box of objects on the active page in stack coordinates when
    /// any object extends outside its page frame; cached by `(page.id, rev)`.
    pub fn stack_content_bounds(&self, stack: &PageStack) -> Option<Rect> {
        let page = self.page();
        let cached = self.content_bounds.borrow();
        let local = if let Some((id, rev, bounds)) = *cached
            && id == page.id
            && rev == page.revision
        {
            bounds
        } else {
            drop(cached);
            let bounds = page_object_bounds(page);
            *self.content_bounds.borrow_mut() = Some((page.id, page.revision, bounds));
            bounds
        }?;
        let origin = stack.frame(self.page).min;
        Some(Rect {
            min: Point::new(local.min.x + origin.x, local.min.y + origin.y),
            max: Point::new(local.max.x + origin.x, local.max.y + origin.y),
        })
    }
    /// Incrementally update derived state after `command` was applied. Falls
    /// back to full rebuilds when the current page or page list changed.
    pub(crate) fn refresh_command(&mut self, command: &Command) {
        self.memory_usage.set(None);
        if command.changes.iter().any(|change| match change {
            Change::Page { .. } => true,
            Change::Properties { before, after, .. } => {
                (before.width, before.height, before.infinite)
                    != (after.width, after.height, after.infinite)
            }
            _ => false,
        }) {
            self.page_stack.get_mut().take();
        }
        self.refresh_search_text(command);
        let old_page = self.page;
        self.page = self.page.min(self.document.pages.len() - 1);
        let page = &self.document.pages[self.page];
        let pages_changed = command
            .changes
            .iter()
            .any(|c| matches!(c, Change::Page { .. }));
        if self.library_cover.as_ref().is_some_and(|cover| {
            let cover_id = self
                .document
                .metadata
                .cover_page
                .filter(|id| self.document.page(*id).is_some())
                .unwrap_or(self.document.pages[0].id);
            // A page replacement can preserve both identity and revision.
            pages_changed
                || cover.id != cover_id
                || self
                    .document
                    .page(cover.id)
                    .is_none_or(|page| page.revision != cover.revision)
        }) {
            self.library_cover = None;
        }
        let object_changes = command
            .changes
            .iter()
            .filter_map(|c| match c {
                Change::Object {
                    page: id,
                    id: object,
                    ..
                } if *id == page.id => Some(*object),
                _ => None,
            })
            .collect::<Vec<_>>();
        let page_changed = self.indexed_page != page.id || old_page != self.page;

        // Drawing order: most edits only append objects, which can be recorded
        // without renumbering the whole page.
        let inserted = object_changes
            .iter()
            .copied()
            .filter(|id| !self.order_positions.contains_key(id) && page.objects.contains_key(id))
            .collect::<HashSet<_>>();
        let removed = object_changes
            .iter()
            .any(|id| self.order_positions.contains_key(id) && !page.objects.contains_key(id));
        let suffix = page.order.len().saturating_sub(inserted.len());
        let appended = page.order[suffix..].iter().all(|id| inserted.contains(id));
        if self.indexed_page != page.id || pages_changed || removed || !appended {
            self.order_positions = order_positions(page);
        } else {
            for (offset, id) in page.order[suffix..].iter().enumerate() {
                self.order_positions.insert(*id, suffix + offset);
            }
        }

        if page_changed || pages_changed {
            self.linked_math = linked_math_objects(page);
        } else {
            for id in &object_changes {
                if page
                    .objects
                    .get(id)
                    .is_some_and(|object| is_linked_math(object))
                {
                    self.linked_math.insert(*id);
                } else {
                    self.linked_math.remove(id);
                }
            }
        }
        self.selection.retain(|id| page.objects.contains_key(id));
        if page_changed {
            self.index.rebuild(page);
        } else {
            self.index.update(page, command);
        }
        self.indexed_page = page.id;
    }
    /// Rebuild all derived state for the current page.
    pub(crate) fn refresh(&mut self) {
        self.memory_usage.set(None);
        self.page = self.page.min(self.document.pages.len() - 1);
        let page = &self.document.pages[self.page];
        self.selection.retain(|id| page.objects.contains_key(id));
        self.index.rebuild(page);
        self.indexed_page = page.id;
        self.linked_math = linked_math_objects(page);
        self.order_positions = order_positions(page);
    }
}
fn order_positions(page: &Page) -> HashMap<Id, usize> {
    page.order
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect()
}
fn is_linked_math(object: &Object) -> bool {
    matches!(object, Object::Equation(equation) if equation.math_link.is_some())
}
fn linked_math_objects(page: &Page) -> HashSet<Id> {
    page.objects
        .values()
        .filter(|object| is_linked_math(object))
        .map(|object| object.id())
        .collect()
}
fn page_object_bounds(page: &Page) -> Option<Rect> {
    let mut union: Option<Rect> = None;
    for object in page.objects.values() {
        let b = object.bounds();
        if !b.min.x.is_finite()
            || !b.min.y.is_finite()
            || !b.max.x.is_finite()
            || !b.max.y.is_finite()
        {
            continue;
        }
        union = Some(match union {
            Some(acc) => acc.union(b),
            None => b,
        });
    }
    union
}
