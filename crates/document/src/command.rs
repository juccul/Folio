//! Reversible commands and undo/redo history.
use super::*;

/// Reversible, object-sized commands. History never copies a whole document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Change {
    InkText {
        page: Id,
        before: Vec<InkText>,
        after: Vec<InkText>,
    },
    Groups {
        page: Id,
        before: Vec<InkGroup>,
        after: Vec<InkGroup>,
    },
    Object {
        page: Id,
        id: Id,
        before: Option<Arc<Object>>,
        after: Option<Arc<Object>>,
        index: usize,
    },
    Page {
        index: usize,
        before: Option<Page>,
        after: Option<Page>,
    },
    Properties {
        page: Id,
        before: PageProperties,
        after: PageProperties,
    },
    Metadata {
        before: NoteMetadata,
        after: NoteMetadata,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    pub label: String,
    pub changes: Vec<Change>,
}
impl Command {
    pub fn apply(&self, doc: &mut Document, forward: bool) {
        #[derive(Default)]
        struct PageLookup {
            last: Option<(Id, Option<usize>)>,
            indices: Option<std::collections::HashMap<Id, usize>>,
            searched: usize,
        }
        fn page_mut<'a>(
            doc: &'a mut Document,
            id: Id,
            lookup: &mut PageLookup,
        ) -> Option<&'a mut Page> {
            if let Some((previous, index)) = lookup.last
                && previous == id
            {
                return index.and_then(|index| doc.pages.get_mut(index));
            }
            // Reuse grouped page changes without indexing a long document for
            // a small edit near its start. Index only after scans justify it.
            let index = if lookup.indices.is_some() || lookup.searched >= doc.pages.len() {
                lookup
                    .indices
                    .get_or_insert_with(|| {
                        doc.pages
                            .iter()
                            .enumerate()
                            .map(|(index, page)| (page.id, index))
                            .collect()
                    })
                    .get(&id)
                    .copied()
            } else {
                let index = doc.pages.iter().position(|page| page.id == id);
                lookup.searched += index.map_or(doc.pages.len(), |index| index + 1);
                index
            };
            lookup.last = Some((id, index));
            index.and_then(|index| doc.pages.get_mut(index))
        }
        #[derive(Default)]
        struct Removed {
            ids: Vec<Id>,
            indexed: Option<std::collections::HashSet<Id>>,
        }
        impl Removed {
            fn insert(&mut self, id: Id) {
                if let Some(ids) = &mut self.indexed {
                    ids.insert(id);
                } else {
                    self.ids.push(id);
                }
            }
            fn remove(&mut self, id: &Id) -> bool {
                self.indexed
                    .get_or_insert_with(|| std::mem::take(&mut self.ids).into_iter().collect())
                    .remove(id)
            }
        }
        fn remove_from_order(page: &mut Page, removed: Removed) {
            let ids = if let Some(ids) = removed.indexed {
                ids
            } else if removed.ids.len() == 1 {
                page.order.retain(|id| *id != removed.ids[0]);
                return;
            } else {
                removed.ids.into_iter().collect()
            };
            if !ids.is_empty() {
                page.order.retain(|id| !ids.contains(id));
            }
        }
        // A favorite is library organization, not a content edit. Preserve its
        // edit time in both directions so toggling it cannot reorder the shelf
        // or invalidate the cover preview.
        let edited_at = doc.metadata.updated_at;
        let favorite_only = !self.changes.is_empty()
            && self.changes.iter().all(|change| {
                if let Change::Metadata { before, after } = change {
                    let mut comparable = after.clone();
                    comparable.favorite = before.favorite;
                    before.favorite != after.favorite && comparable == *before
                } else {
                    false
                }
            });
        doc.version = FORMAT_VERSION;
        let mut inserted = Vec::new();
        let mut removed: BTreeMap<Id, Removed> = BTreeMap::new();
        let mut page_indices = PageLookup::default();
        let mut apply = |c: &Change, doc: &mut Document| match c {
            Change::InkText {
                page,
                before,
                after,
            } => {
                if let Some(page) = page_mut(doc, *page, &mut page_indices) {
                    page.ink_text = if forward {
                        after.clone()
                    } else {
                        before.clone()
                    };
                    page.revision += 1;
                }
            }
            Change::Groups {
                page,
                before,
                after,
            } => {
                if let Some(p) = page_mut(doc, *page, &mut page_indices) {
                    p.groups = if forward {
                        after.clone()
                    } else {
                        before.clone()
                    };
                    p.revision += 1;
                }
            }
            Change::Object {
                page,
                id,
                before,
                after,
                index,
            } => {
                if let Some(p) = page_mut(doc, *page, &mut page_indices) {
                    let value = if forward { after } else { before };
                    if let Some(o) = value {
                        if !p.objects.contains_key(id) {
                            // Only a reused identity needs its old order entry
                            // removed now. Unrelated replacement fragments can
                            // leave batch deletion deferred until the end.
                            if removed.get_mut(page).is_some_and(|ids| ids.remove(id)) {
                                p.order.retain(|old| old != id);
                            }
                            p.order.push(*id);
                            inserted.push((*page, *id, *index));
                        }
                        p.objects.insert(*id, o.clone());
                    } else {
                        p.objects.remove(id);
                        if self.changes.len() == 1 {
                            p.order.retain(|v| v != id);
                        } else {
                            removed.entry(*page).or_default().insert(*id);
                        }
                    }
                    p.revision += 1;
                    p.groups.clear();
                }
            }
            Change::Page {
                index,
                before,
                after,
            } => {
                page_indices = PageLookup::default();
                let old = if forward { before } else { after };
                let new = if forward { after } else { before };
                if let Some(old) = old {
                    removed.remove(&old.id);
                    doc.pages.retain(|p| p.id != old.id)
                }
                if let Some(new) = new {
                    removed.remove(&new.id);
                    doc.pages.insert((*index).min(doc.pages.len()), new.clone())
                }
            }
            Change::Properties {
                page,
                before,
                after,
            } => {
                if let Some(p) = page_mut(doc, *page, &mut page_indices) {
                    p.properties = if forward {
                        after.clone()
                    } else {
                        before.clone()
                    };
                    p.revision += 1;
                }
            }
            Change::Metadata { before, after } => {
                // Apply only the fields this command changed. Library
                // organization (folder, favorite, trash) is not part of undo
                // history, so a stored snapshot must not revert it.
                let (from, to) = if forward {
                    (before, after)
                } else {
                    (after, before)
                };
                let m = &mut doc.metadata;
                if from.title != to.title {
                    m.title.clone_from(&to.title);
                }
                if from.tags != to.tags {
                    m.tags.clone_from(&to.tags);
                }
                if from.cover_page != to.cover_page {
                    m.cover_page = to.cover_page;
                }
                if from.notebook != to.notebook {
                    m.notebook = to.notebook;
                }
                if from.favorite != to.favorite {
                    m.favorite = to.favorite;
                }
                if from.trashed != to.trashed {
                    m.trashed = to.trashed;
                }
            }
        };
        if forward {
            for c in &self.changes {
                apply(c, doc)
            }
        } else {
            for c in self.changes.iter().rev() {
                apply(c, doc)
            }
        }
        // Batch erasing visits a page's drawing order once instead of once per
        // deleted object. Object-map changes and revision increments stay ordered.
        for (id, ids) in removed {
            if let Some(page) = page_mut(doc, id, &mut page_indices) {
                remove_from_order(page, ids);
            }
        }
        // Restore original z-order after a batch deletion. Inserting immediately
        // while undoing in reverse shifts later objects into the wrong positions.
        inserted.sort_by_key(|(page, _, index)| (*page, *index));
        // Ordinary moves and metadata edits do not alter z-order. Appending
        // strokes already leaves them in the correct position; avoid scanning
        // unrelated pages or existing ink for either case.
        let mut start = 0;
        while start < inserted.len() {
            let page_id = inserted[start].0;
            let end = start
                + inserted[start..]
                    .iter()
                    .take_while(|(id, _, _)| *id == page_id)
                    .count();
            let objects = &inserted[start..end];
            if let Some(page) = page_mut(doc, page_id, &mut page_indices) {
                let suffix = page.order.len().saturating_sub(objects.len());
                let already_ordered = objects.iter().enumerate().all(|(offset, (_, id, index))| {
                    *index == suffix + offset && page.order.get(*index) == Some(id)
                });
                if !already_ordered {
                    let ids = objects
                        .iter()
                        .map(|(_, id, _)| *id)
                        .collect::<std::collections::HashSet<_>>();
                    page.order.retain(|id| !ids.contains(id));
                    if objects.len() > 1 && objects.windows(2).all(|pair| pair[0].2 < pair[1].2) {
                        // Merge distinct positions in one pass when restoring a
                        // large selection; repeated Vec::insert shifts the tail.
                        let mut order = Vec::with_capacity(page.order.len() + objects.len());
                        let mut remaining = std::mem::take(&mut page.order).into_iter();
                        for (_, id, index) in objects {
                            order
                                .extend(remaining.by_ref().take(index.saturating_sub(order.len())));
                            order.push(*id);
                        }
                        order.extend(remaining);
                        page.order = order;
                    } else {
                        // Equal indices preserve the existing stable insertion semantics.
                        for (_, id, index) in objects {
                            page.order.insert((*index).min(page.order.len()), *id);
                        }
                    }
                }
            }
            start = end;
        }
        doc.metadata.updated_at = if favorite_only { edited_at } else { now_ms() };
    }
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
}
impl History {
    pub fn entries(&self) -> impl Iterator<Item = (&Command, bool)> {
        self.undo
            .iter()
            .map(|c| (c, true))
            .chain(self.redo.iter().rev().map(|c| (c, false)))
    }
    pub fn from_commands(undo: Vec<Command>, redo: Vec<Command>) -> Self {
        Self { undo, redo }
    }

    pub fn execute(&mut self, cmd: Command, doc: &mut Document) {
        if cmd.changes.is_empty() {
            return;
        }
        cmd.apply(doc, true);
        self.undo.push(cmd);
        self.redo.clear();
        if self.undo.len() > 512 {
            self.undo.remove(0);
        }
    }
    pub fn undo(&mut self, doc: &mut Document) -> Option<Command> {
        let cmd = self.undo.pop()?;
        cmd.apply(doc, false);
        self.redo.push(cmd.clone());
        Some(cmd)
    }
    pub fn redo(&mut self, doc: &mut Document) -> Option<Command> {
        let cmd = self.redo.pop()?;
        cmd.apply(doc, true);
        self.undo.push(cmd.clone());
        Some(cmd)
    }
    /// Drop the most recently undone command so it cannot be redone.
    pub fn forget_redo(&mut self) -> Option<Command> {
        self.redo.pop()
    }
    /// The command that the next undo would revert.
    pub fn last(&self) -> Option<&Command> {
        self.undo.last()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
