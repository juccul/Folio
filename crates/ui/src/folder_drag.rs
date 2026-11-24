//! Folder rows dragged like document tabs: the row (with its open
//! subfolders) lifts and follows the pointer, the other rows spring aside to
//! open a gap where it would go, and it settles into its place on release.
//!
//! Folders have no manual order (siblings stay sorted by name), so where a
//! row is dropped only chooses its parent: onto a folder nests it there;
//! into a gap makes it a sibling of the row below the gap; above the list,
//! below it, or in its own gap at the top level or where it was.
use super::*;

/// Where a dragged folder goes when it is released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FolderDrop {
    /// Between the rows that stay: the gap's index among them.
    Gap(usize),
    /// Onto a folder, to nest inside it.
    Into(Id),
}

#[derive(Clone, Debug)]
pub(super) struct FolderDrag {
    pub id: Id,
    /// Pointer y when the row was pressed, and now.
    pub start: f32,
    pub pointer: f32,
    /// Set once the pointer has moved far enough to be a drag, not a click.
    pub moving: bool,
    /// The visible rows when the drag started, in order: id, depth, parent.
    pub rows: Vec<(Id, usize, Option<Id>)>,
    /// The first row's top in the window, and the distance between rows.
    pub top: f32,
    pub pitch: f32,
    pub target: FolderDrop,
}

impl FolderDrag {
    /// Movement before a press becomes a drag, as for tabs.
    pub const THRESHOLD: f32 = 4.;
    /// The share of a row at either edge that opens a gap rather than nesting.
    const EDGE: f32 = 0.25;

    pub fn new(
        id: Id,
        pointer: f32,
        rows: Vec<(Id, usize, Option<Id>)>,
        top: f32,
        pitch: f32,
    ) -> Option<Self> {
        let from = rows.iter().position(|(row, _, _)| *row == id)?;
        Some(Self {
            id,
            start: pointer,
            pointer,
            moving: false,
            rows,
            top,
            pitch,
            target: FolderDrop::Gap(from),
        })
    }
    /// The dragged row and its visible subfolders, which move as one.
    fn block(&self) -> std::ops::Range<usize> {
        let Some(from) = self.rows.iter().position(|(id, _, _)| *id == self.id) else {
            return 0..0;
        };
        let depth = self.rows[from].1;
        let len = self.rows[from + 1..]
            .iter()
            .take_while(|(_, d, _)| *d > depth)
            .count();
        from..from + 1 + len
    }
    /// The rows that stay, with their index among all rows.
    fn rest(&self) -> Vec<(usize, &(Id, usize, Option<Id>))> {
        let block = self.block();
        self.rows
            .iter()
            .enumerate()
            .filter(|(index, _)| !block.contains(index))
            .collect()
    }
    fn gap(&self) -> usize {
        match self.target {
            FolderDrop::Gap(gap) => gap,
            FolderDrop::Into(_) => self.home(),
        }
    }
    /// The gap the dragged rows came from.
    fn home(&self) -> usize {
        self.block().start
    }
    /// Follow the pointer: a row's middle nests the folder there (when
    /// `accepts` allows it), its edges move the gap before or after it, and
    /// the open gap keeps it. Rows are hit where they are drawn, gap
    /// included, so a row that moves aside never moves back under the
    /// pointer.
    pub fn follow(&mut self, pointer: f32, accepts: impl Fn(Option<Id>) -> bool) {
        self.pointer = pointer;
        if !self.moving && (pointer - self.start).abs() > Self::THRESHOLD {
            self.moving = true;
        }
        if !self.moving {
            return;
        }
        let rest = self.rest();
        let (n, k, h) = (rest.len(), self.block().len(), self.pitch);
        let p = pointer - self.top;
        let gap = self.gap();
        let gap_top = gap as f32 * h;
        self.target = if p < 0. {
            FolderDrop::Gap(0)
        } else if p >= (n + k) as f32 * h {
            FolderDrop::Gap(n)
        } else if p >= gap_top && p < gap_top + k as f32 * h {
            FolderDrop::Gap(gap)
        } else {
            let (index, row_top) = if p < gap_top {
                let index = (p / h) as usize;
                (index, index as f32 * h)
            } else {
                let index = ((p - k as f32 * h) / h) as usize;
                (index, (index + k) as f32 * h)
            };
            let index = index.min(n.saturating_sub(1));
            let local = (p - row_top) / h;
            let row = rest[index].1.0;
            if local < Self::EDGE {
                FolderDrop::Gap(index)
            } else if local > 1. - Self::EDGE {
                FolderDrop::Gap(index + 1)
            } else if accepts(Some(row)) {
                FolderDrop::Into(row)
            } else {
                FolderDrop::Gap(if local < 0.5 { index } else { index + 1 })
            }
        };
    }
    /// The parent the folder would get, or `None` where it stays put.
    pub fn destination(&self) -> Option<Option<Id>> {
        if !self.moving {
            return None;
        }
        match self.target {
            FolderDrop::Into(id) => Some(Some(id)),
            FolderDrop::Gap(gap) if gap == self.home() => None,
            FolderDrop::Gap(gap) => Some(self.rest().get(gap).and_then(|(_, row)| row.2)),
        }
    }
    /// How far a row is drawn from its place: the dragged rows follow the
    /// pointer within the list, and the rows below the gap move down by
    /// their height.
    pub fn offset(&self, id: Id) -> f32 {
        if !self.moving {
            return 0.;
        }
        let block = self.block();
        let h = self.pitch;
        if self.rows[block.clone()].iter().any(|(row, _, _)| *row == id) {
            let first = self.top + block.start as f32 * h;
            let last = self.top + (self.rows.len() - block.len()) as f32 * h;
            let lifted = (first + self.pointer - self.start).clamp(self.top, last.max(self.top));
            return lifted - first;
        }
        let gap = self.gap();
        let k = block.len();
        self.rest()
            .iter()
            .enumerate()
            .find(|(_, (_, row))| row.0 == id)
            .map_or(0., |(index, (origin, _))| {
                let shown = if index >= gap { index + k } else { index };
                (shown as f32 - *origin as f32) * h
            })
    }
    /// Whether a row is one of the dragged ones.
    pub fn lifts(&self, id: Id) -> bool {
        self.moving && self.rows[self.block()].iter().any(|(row, _, _)| *row == id)
    }
    /// The folder a drop would nest the dragged one in, highlighted.
    pub fn nesting(&self) -> Option<Id> {
        match self.target {
            FolderDrop::Into(id) if self.moving => Some(id),
            _ => None,
        }
    }
    /// A dragged row's depth for where it would land, so its indent shows
    /// its new parent; other rows keep theirs.
    pub fn depth(&self, id: Id, depth: usize, valid: bool) -> usize {
        let Some(parent) = self.destination().filter(|_| valid && self.lifts(id)) else {
            return depth;
        };
        let landing = parent.map_or(0, |parent| {
            self.rows
                .iter()
                .find(|(row, _, _)| *row == parent)
                .map_or(0, |(_, d, _)| d + 1)
        });
        let from = self.rows[self.block().start].1;
        landing + depth.saturating_sub(from)
    }
}

impl NotesView {
    /// Start a possible folder drag from a press on its row.
    pub(super) fn press_folder(&mut self, id: Id, y: f32) {
        let tree = self.controller.folder_tree();
        let visible: std::collections::HashSet<_> = self.folder_rows.iter().copied().collect();
        let rows: Vec<_> = tree
            .iter()
            .filter(|(folder, _)| visible.contains(&folder.id))
            .map(|(folder, depth)| (folder.id, *depth, folder.parent))
            .collect();
        let laid: Vec<_> = rows
            .iter()
            .enumerate()
            .filter_map(|(index, (row, _, _))| {
                let bounds = self.accessibility.control_bounds_for_key(&format!("folder-{row}"))?;
                Some((index, bounds.y0 as f32))
            })
            .collect();
        let pitch = match laid.as_slice() {
            [(a, top_a), .., (b, top_b)] if b > a => (top_b - top_a) / (b - a) as f32,
            _ => 48.,
        };
        let Some((index, top)) = laid.first().copied() else {
            return;
        };
        self.folder_drag = FolderDrag::new(id, y, rows, top - index as f32 * pitch, pitch);
    }
    pub(super) fn drag_folder(&mut self, y: f32, cx: &mut Context<Self>) {
        let Some(mut drag) = self.folder_drag.take() else {
            return;
        };
        let was = (drag.moving, drag.target, drag.offset(drag.id));
        let (controller, id) = (&self.controller, drag.id);
        drag.follow(y, |destination| {
            super::folder_menu::accepts_move(
                controller,
                super::folder_menu::LibraryItem::Folder(id),
                destination,
            )
        });
        let changed = was != (drag.moving, drag.target, drag.offset(drag.id));
        self.folder_drag = Some(drag);
        if changed {
            // The rows it passes spring aside; it follows the pointer.
            cx.animate();
            cx.notify();
        }
    }
    /// Move the folder to where it was dropped, through the same checked
    /// move (and library Undo) as the Move dialog; it then settles into its
    /// place among its new siblings.
    pub(super) fn drop_folder(&mut self, cancelled: bool, cx: &mut Context<Self>) {
        let Some(drag) = self.folder_drag.take() else {
            return;
        };
        if !drag.moving {
            return;
        }
        if !cancelled
            && !self.update_preparing
            && !self.blocking_overlay()
            && let Some(destination) = drag.destination()
            && super::folder_menu::accepts_move(
                &self.controller,
                super::folder_menu::LibraryItem::Folder(drag.id),
                destination,
            )
        {
            if let Err(error) = self.controller.move_notebook(drag.id, destination) {
                self.controller.error = Some(error);
            } else if let Some(id) = destination {
                let collapsed = &mut self.controller.settings.collapsed_folders;
                if collapsed.contains(&id) {
                    collapsed.retain(|folder| *folder != id);
                    self.controller.store_settings();
                }
            }
        }
        self.folder_settling = Some(std::time::Instant::now());
        cx.animate();
        cx.notify();
    }
    /// Whether a folder drag ended this moment, so its release is no click.
    pub(super) fn folder_just_dropped(&self) -> bool {
        self.folder_drag.as_ref().is_some_and(|drag| drag.moving)
            || self
                .folder_settling
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(250))
    }
    /// Whether folder rows fly to new places between list slots: while one
    /// is dragged and as it settles.
    pub(super) fn folder_rows_fly(&self) -> bool {
        self.folder_drag.as_ref().is_some_and(|drag| drag.moving)
            || self
                .folder_settling
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(900))
    }
}

#[cfg(test)]
mod tests {
    use super::{FolderDrag, FolderDrop};
    use core::prelude::v1::test;
    use folio_document::Id;
    #[test]
    fn dragged_folders_open_a_gap_nest_and_leave_their_parent() {
        let [projects, personal, deep, reading] =
            [Id::new_v4(), Id::new_v4(), Id::new_v4(), Id::new_v4()];
        // Projects > Personal > Deep, then Reading; rows 48 apart from y 100.
        let rows = vec![
            (projects, 0, None),
            (personal, 1, Some(projects)),
            (deep, 2, Some(personal)),
            (reading, 0, None),
        ];
        let mut drag = FolderDrag::new(personal, 170., rows, 100., 48.).unwrap();
        let any = |_: Option<Id>| true;
        // A press that barely moves is still a click.
        drag.follow(172., any);
        assert!(!drag.moving && drag.destination().is_none());
        // Its own gap keeps it where it was; its subfolder moves with it.
        drag.follow(180., any);
        assert_eq!(drag.target, FolderDrop::Gap(1));
        assert_eq!(drag.destination(), None);
        assert!(drag.lifts(deep) && !drag.lifts(reading));
        assert_eq!((drag.offset(personal), drag.offset(deep)), (10., 10.));
        // Over Reading's middle (drawn below the gap): nest there.
        drag.follow(100. + 3. * 48. + 24., any);
        assert_eq!(drag.target, FolderDrop::Into(reading));
        assert_eq!(drag.destination(), Some(Some(reading)));
        assert_eq!(drag.depth(personal, 1, true), 1);
        assert_eq!(drag.depth(deep, 2, true), 2);
        // Below Reading's bottom edge: the gap opens after it, Reading
        // moves up into the space, and the folder goes to the top level.
        drag.follow(100. + 3. * 48. + 44., any);
        assert_eq!(drag.target, FolderDrop::Gap(2));
        assert_eq!(drag.offset(reading), -96.);
        assert_eq!(drag.destination(), Some(None));
        assert_eq!(drag.depth(personal, 1, true), 0);
        assert_eq!(drag.depth(deep, 2, true), 1);
        // The rows stay where they are while the pointer is in the gap.
        drag.follow(100. + 3. * 48. + 10., any);
        assert_eq!(drag.target, FolderDrop::Gap(2));
        // The lifted rows stop at the end of the list.
        drag.follow(900., any);
        assert_eq!(drag.offset(personal), 48.);
        // Above the list (the Folders heading): the top level, the gap first.
        drag.follow(60., any);
        assert_eq!(drag.target, FolderDrop::Gap(0));
        assert_eq!(drag.offset(projects), 96.);
        assert_eq!(drag.destination(), Some(None));
        // A folder that would refuse it opens a gap instead of nesting.
        drag.follow(100. + 2. * 48. + 20., |destination| destination != Some(projects));
        assert_eq!(drag.target, FolderDrop::Gap(0));
        // Unknown rows never start a drag.
        assert!(FolderDrag::new(Id::new_v4(), 0., Vec::new(), 0., 48.).is_none());
    }
}
