//! Stable folder hierarchy and unambiguous paths shared by navigation and pickers.
use super::*;
pub struct FolderDetails {
    pub path: String,
    pub level: usize,
    pub branch: bool,
}
pub(super) struct DetailsCache {
    notebooks: Vec<Notebook>,
    indices: HashMap<Id, usize>,
    branches: HashSet<Id>,
    details: HashMap<Id, Arc<FolderDetails>>,
    bytes: usize,
    tree: Option<Vec<(Id, usize)>>,
}
impl DetailsCache {
    fn new(notebooks: &[Notebook]) -> Self {
        Self {
            indices: notebooks
                .iter()
                .enumerate()
                .map(|(index, folder)| (folder.id, index))
                .collect(),
            branches: notebooks
                .iter()
                .filter_map(|folder| folder.parent)
                .collect(),
            notebooks: notebooks.to_vec(),
            details: HashMap::new(),
            bytes: 0,
            tree: None,
        }
    }
}
impl Controller {
    /// Resolve only requested navigation labels against a shared identity index.
    /// The source comparison also honors direct mutations of the public model.
    pub fn folder_details(
        &self,
        ids: impl IntoIterator<Item = Id>,
    ) -> HashMap<Id, Arc<FolderDetails>> {
        let mut ids = ids.into_iter().peekable();
        if ids.peek().is_none() {
            return HashMap::new();
        }
        let mut cached = self.folder_details.borrow_mut();
        if cached
            .as_ref()
            .is_none_or(|cache| cache.notebooks != self.notebooks)
        {
            *cached = Some(DetailsCache::new(&self.notebooks));
        }
        let cache = cached.as_mut().unwrap();
        ids.filter_map(|id| {
            if let Some(details) = cache.details.get(&id) {
                return Some((id, details.clone()));
            }
            if !cache.indices.contains_key(&id) {
                return None;
            }
            let mut cursor = Some(id);
            let mut seen = HashSet::new();
            let mut names = Vec::new();
            while let Some(id) = cursor {
                if !seen.insert(id) {
                    break;
                }
                let Some(index) = cache.indices.get(&id) else {
                    break;
                };
                let folder = &cache.notebooks[*index];
                names.push(folder.name.as_str());
                cursor = folder.parent;
            }
            names.reverse();
            let details = Arc::new(FolderDetails {
                path: names.join(" / "),
                level: names.len(),
                branch: cache.branches.contains(&id),
            });
            // Deep imported trees must not retain unbounded tooltip text.
            if cache.bytes + details.path.len() <= 16 * 1024 * 1024 {
                cache.bytes += details.path.len();
                cache.details.insert(id, details.clone());
            }
            Some((id, details))
        })
        .collect()
    }
    /// Direct contents; child folders must be moved before their parent can be deleted.
    pub fn folder_contents(&self, id: Id) -> (usize, usize, usize) {
        let live = self
            .notes
            .iter()
            .filter(|n| n.notebook == Some(id) && !n.trashed)
            .count();
        let trash = self
            .notes
            .iter()
            .filter(|n| n.notebook == Some(id) && n.trashed)
            .count();
        let children = self
            .notebooks
            .iter()
            .filter(|n| n.parent == Some(id))
            .count();
        (live, trash, children)
    }
    pub fn folder_deletion_reason(&self, id: Id) -> Option<String> {
        if !self.notebooks.iter().any(|n| n.id == id) {
            return Some("This folder no longer exists.".into());
        }
        let (live, trash, children) = self.folder_contents(id);
        let mut contents = Vec::new();
        for (count, label) in [
            (live, "document"),
            (trash, "trashed document"),
            (children, "subfolder"),
        ] {
            if count > 0 {
                contents.push(format!(
                    "{count} {label}{}",
                    if count == 1 { "" } else { "s" }
                ));
            }
        }
        if contents.is_empty() {
            None
        } else {
            Some(format!(
                "Move {} before deleting this folder.{}",
                contents.join(", "),
                if trash > 0 {
                    " Restore and move trashed documents, or permanently delete them in Trash."
                } else {
                    ""
                }
            ))
        }
    }

    pub(super) fn validate_folder_name(
        &self,
        name: &str,
        parent: Option<Id>,
        exclude: Option<Id>,
    ) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a folder name.".into());
        }
        if name.chars().count() > 128 || name.chars().any(char::is_control) {
            return Err("Use a folder name of up to 128 characters on one line.".into());
        }
        if parent.is_some_and(|id| !self.notebooks.iter().any(|n| n.id == id)) {
            return Err("The parent folder no longer exists.".into());
        }
        if self.notebooks.iter().any(|n| {
            Some(n.id) != exclude
                && n.parent == parent
                && n.name.trim().to_lowercase() == name.to_lowercase()
        }) {
            return Err("A folder with this name already exists here. Choose another name.".into());
        }
        Ok(name.to_owned())
    }

    pub fn folder_ancestors(&self, id: Id) -> Vec<Notebook> {
        let mut path = Vec::new();
        let mut cursor = Some(id);
        let mut seen = HashSet::new();
        while let Some(id) = cursor {
            if !seen.insert(id) {
                break;
            }
            let Some(folder) = self.notebooks.iter().find(|n| n.id == id) else {
                break;
            };
            path.push(folder.clone());
            cursor = folder.parent;
        }
        path.reverse();
        path
    }
    pub fn folder_path(&self, id: Id) -> String {
        self.folder_ancestors(id)
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    }
    pub fn folder_tree(&self) -> Vec<(Notebook, usize)> {
        let mut cached = self.folder_details.borrow_mut();
        if cached
            .as_ref()
            .is_none_or(|cache| cache.notebooks != self.notebooks)
        {
            *cached = Some(DetailsCache::new(&self.notebooks));
        }
        let cache = cached.as_mut().unwrap();
        if let Some(tree) = &cache.tree {
            return tree
                .iter()
                .map(|(id, depth)| (cache.notebooks[cache.indices[id]].clone(), *depth))
                .collect();
        }
        let mut ordered: Vec<_> = self.notebooks.iter().collect();
        ordered.sort_by_cached_key(|n| (n.name.to_lowercase(), n.name.clone(), n.id));
        let folders: HashMap<_, _> = self.notebooks.iter().map(|n| (n.id, n)).collect();
        let mut children: HashMap<Id, Vec<Id>> = HashMap::new();
        let mut roots = Vec::new();
        for folder in &ordered {
            if let Some(parent) = folder.parent.filter(|id| folders.contains_key(id)) {
                children.entry(parent).or_default().push(folder.id);
            } else {
                roots.push(folder.id);
            }
        }
        let mut result = Vec::with_capacity(ordered.len());
        let mut seen = HashSet::new();
        // Orphans and invalid cycles remain visible instead of disappearing.
        for root in roots.into_iter().chain(ordered.iter().map(|n| n.id)) {
            let mut stack = vec![(root, 0)];
            while let Some((id, depth)) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let Some(folder) = folders.get(&id) else {
                    continue;
                };
                result.push(((*folder).clone(), depth));
                if let Some(children) = children.get(&id) {
                    stack.extend(children.iter().rev().map(|id| (*id, depth + 1)));
                }
            }
        }
        cache.tree = Some(
            result
                .iter()
                .map(|(folder, depth)| (folder.id, *depth))
                .collect(),
        );
        result
    }
}
impl Controller {
    pub fn create_notebook(&mut self, name: String, parent: Option<Id>) -> Result<Id, String> {
        let name = self.validate_folder_name(&name, parent, None)?;
        let n = Notebook {
            id: Id::new_v4(),
            name,
            parent,
        };
        let id = n.id;
        self.persistence.notebook(n.clone());
        self.notebooks.push(n);
        Ok(id)
    }
    pub fn rename_notebook(&mut self, id: Id, name: String) -> Result<(), String> {
        let parent = self
            .notebooks
            .iter()
            .find(|n| n.id == id)
            .ok_or("Folder no longer exists")?
            .parent;
        let name = self.validate_folder_name(&name, parent, Some(id))?;
        let n = self.notebooks.iter_mut().find(|n| n.id == id).unwrap();
        n.name = name;
        self.persistence.notebook(n.clone());
        Ok(())
    }
    pub fn validate_folder_move(&self, id: Id, parent: Option<Id>) -> Result<(), String> {
        if !self.notebooks.iter().any(|n| n.id == id) {
            return Err("Folder no longer exists".into());
        }
        let mut cursor = parent;
        let mut visited = HashSet::new();
        while let Some(next) = cursor {
            if next == id || !visited.insert(next) {
                return Err("A folder cannot be moved inside itself or a descendant".into());
            }
            cursor = self
                .notebooks
                .iter()
                .find(|n| n.id == next)
                .ok_or("Destination folder no longer exists")?
                .parent;
        }
        let name = &self.notebooks.iter().find(|n| n.id == id).unwrap().name;
        self.validate_folder_name(name, parent, Some(id))?;
        Ok(())
    }
    pub fn move_notebook(&mut self, id: Id, parent: Option<Id>) -> Result<(), String> {
        self.validate_folder_move(id, parent)?;
        let name = &self.notebooks.iter().find(|n| n.id == id).unwrap().name;
        self.validate_folder_name(name, parent, Some(id))?;
        let n = self.notebooks.iter_mut().find(|n| n.id == id).unwrap();
        n.parent = parent;
        self.persistence.notebook(n.clone());
        Ok(())
    }
    pub fn delete_empty_notebook(&mut self, id: Id) -> Result<(), String> {
        if let Some(reason) = self.folder_deletion_reason(id) {
            return Err(reason);
        }
        self.notebooks.retain(|n| n.id != id);
        self.persistence.delete_notebook(id);
        if matches!(self.filter, NoteFilter::Notebook(folder) | NoteFilter::NotebookTrash(folder) if folder == id)
        {
            self.filter = NoteFilter::All;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_folder_tree_retains_orphans_cycles_and_depth_order() {
        let root = std::env::temp_dir().join(format!("folio-large-tree-{}", Id::new_v4()));
        let mut app = Controller::open(root.clone()).unwrap();
        let ids: Vec<_> = (0..1000).map(|_| Id::new_v4()).collect();
        app.notebooks = ids
            .iter()
            .enumerate()
            .map(|(i, id)| Notebook {
                id: *id,
                name: format!("Folder {i:04}"),
                parent: if i == 0 { None } else { Some(ids[i - 1]) },
            })
            .collect();
        let orphan = Id::new_v4();
        let cycle = Id::new_v4();
        app.notebooks.push(Notebook {
            id: orphan,
            name: "Orphan".into(),
            parent: Some(Id::new_v4()),
        });
        app.notebooks.push(Notebook {
            id: cycle,
            name: "Cycle".into(),
            parent: Some(cycle),
        });
        let tree = app.folder_tree();
        let requested: Vec<_> = app.notebooks.iter().map(|folder| folder.id).collect();
        let details = app.folder_details(requested.iter().copied());
        let again = app.folder_details([ids[0]]);
        assert!(Arc::ptr_eq(&details[&ids[0]], &again[&ids[0]]));
        assert_eq!(again.len(), 1);
        assert_eq!(tree.len(), 1002);
        assert_eq!(
            tree.iter()
                .map(|(folder, _)| folder.id)
                .collect::<HashSet<_>>()
                .len(),
            1002
        );
        for (i, id) in ids.iter().enumerate() {
            assert_eq!((tree[i].0.id, tree[i].1), (*id, i));
            assert_eq!(details[id].level, i + 1);
            assert_eq!(
                details[id].path,
                (0..=i)
                    .map(|index| format!("Folder {index:04}"))
                    .collect::<Vec<_>>()
                    .join(" / ")
            );
            assert_eq!(details[id].branch, i + 1 < ids.len());
        }
        assert_eq!(details[&orphan].path, "Orphan");
        assert_eq!(details[&cycle].path, "Cycle");
        app.notebooks[0].name = "Renamed".into();
        let renamed = app.folder_details([ids[999]]);
        assert!(!Arc::ptr_eq(&details[&ids[999]], &renamed[&ids[999]]));
        assert!(renamed[&ids[999]].path.starts_with("Renamed / "));
        assert!(
            tree.iter()
                .any(|(folder, depth)| folder.id == orphan && *depth == 0)
        );
        assert!(
            tree.iter()
                .any(|(folder, depth)| folder.id == cycle && *depth == 0)
        );
        drop(app);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn shuffled_folders_form_an_ordered_tree_with_complete_paths() {
        let mut a =
            Controller::open(std::env::temp_dir().join(format!("folio-tree-{}", Id::new_v4())))
                .unwrap();
        let root = Id::new_v4();
        let child = Id::new_v4();
        let grandchild = Id::new_v4();
        let sibling = Id::new_v4();
        a.notebooks = vec![
            Notebook {
                id: grandchild,
                name: "Deep".into(),
                parent: Some(child),
            },
            Notebook {
                id: sibling,
                name: "Zebra".into(),
                parent: Some(root),
            },
            Notebook {
                id: root,
                name: "Root".into(),
                parent: None,
            },
            Notebook {
                id: child,
                name: "Alpha".into(),
                parent: Some(root),
            },
        ];
        let tree = a.folder_tree();
        assert_eq!(
            tree.iter().map(|(n, d)| (n.id, *d)).collect::<Vec<_>>(),
            vec![(root, 0), (child, 1), (grandchild, 2), (sibling, 1)]
        );
        assert_eq!(a.folder_path(grandchild), "Root / Alpha / Deep");
        a.notebooks.reverse();
        assert_eq!(
            a.folder_tree()
                .iter()
                .map(|(n, d)| (n.id, *d))
                .collect::<Vec<_>>(),
            tree.iter().map(|(n, d)| (n.id, *d)).collect::<Vec<_>>()
        );
    }
    #[test]
    fn folder_move_uses_identity_and_rejects_descendants_before_mutation() {
        let mut a = Controller::open(
            std::env::temp_dir().join(format!("folio-folder-move-{}", Id::new_v4())),
        )
        .unwrap();
        let root = Id::new_v4();
        let other = Id::new_v4();
        let first = Id::new_v4();
        let second = Id::new_v4();
        a.notebooks = vec![
            Notebook {
                id: root,
                name: "A".into(),
                parent: None,
            },
            Notebook {
                id: other,
                name: "B".into(),
                parent: None,
            },
            Notebook {
                id: first,
                name: "Same".into(),
                parent: Some(root),
            },
            Notebook {
                id: second,
                name: "Same".into(),
                parent: Some(other),
            },
        ];
        assert!(a.validate_folder_move(root, Some(first)).is_err());
        assert!(a.validate_folder_move(root, Some(root)).is_err());
        assert!(a.move_notebook(root, Some(first)).is_err());
        assert!(
            a.notebooks
                .iter()
                .find(|n| n.id == root)
                .unwrap()
                .parent
                .is_none()
        );
        a.move_notebook(first, Some(second)).unwrap();
        assert_eq!(a.folder_path(first), "B / Same / Same");
        a.move_notebook(first, None).unwrap();
        assert_eq!(a.folder_path(first), "Same");
    }
    #[test]
    fn folder_names_share_validation_and_are_unique_within_their_parent() {
        let mut a = Controller::open(
            std::env::temp_dir().join(format!("folio-folder-validation-{}", Id::new_v4())),
        )
        .unwrap();
        assert!(a.create_notebook(" ".into(), None).is_err());
        assert!(a.notebooks.is_empty());
        let root = a.create_notebook(" Projects ".into(), None).unwrap();
        assert_eq!(a.folder_path(root), "Projects");
        assert!(a.create_notebook("PROJECTS".into(), None).is_err());
        let child = a.create_notebook("Projects".into(), Some(root)).unwrap();
        assert!(a.rename_notebook(child, " ".into()).is_err());
        assert_eq!(a.folder_path(child), "Projects / Projects");
        assert!(a.move_notebook(child, None).is_err());
        a.rename_notebook(child, "Notes".into()).unwrap();
        a.move_notebook(child, None).unwrap();
        assert!(a.rename_notebook(child, "projects".into()).is_err());
        a.flush().unwrap();
        let root_dir = a.data_dir.clone();
        drop(a);
        let a = Controller::open(root_dir).unwrap();
        assert_eq!(a.folder_path(child), "Notes");
    }
}

#[cfg(test)]
mod deletion_tests {
    use super::*;
    #[test]
    fn folder_deletion_explains_hidden_trash_and_filters_recovery_by_identity() {
        let root = std::env::temp_dir().join(format!("folio-folder-delete-{}", Id::new_v4()));
        let mut a = Controller::open(root.clone()).unwrap();
        let folder = a.create_notebook("Course".into(), None).unwrap();
        let child = a.create_notebook("Child".into(), Some(folder)).unwrap();
        a.filter = NoteFilter::Notebook(folder);
        a.create_note();
        let hidden = a.active;
        a.metadata(|m| m.trashed = true);
        a.create_note();
        a.metadata(|m| m.trashed = true); // Other Trash is excluded from the folder view.
        a.filter = NoteFilter::NotebookTrash(folder);
        assert_eq!(
            a.visible_notes().iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![hidden]
        );
        assert_eq!(a.folder_contents(folder), (0, 1, 1));
        let reason = a.folder_deletion_reason(folder).unwrap();
        assert!(reason.contains("1 trashed document"));
        assert!(reason.contains("1 subfolder"));
        assert_eq!(a.delete_empty_notebook(folder).unwrap_err(), reason);
        a.move_notebook(child, None).unwrap();
        a.switch_note(hidden);
        a.metadata(|m| m.trashed = false);
        a.metadata(|m| m.notebook = None);
        assert_eq!(a.folder_contents(folder), (0, 0, 0));
        assert!(a.folder_deletion_reason(folder).is_none());
        a.delete_empty_notebook(folder).unwrap();
        assert_eq!(a.filter, NoteFilter::All);
        assert!(a.delete_empty_notebook(folder).is_err());
        a.flush().unwrap();
        drop(a);
        let a = Controller::open(root.clone()).unwrap();
        assert!(!a.notebooks.iter().any(|n| n.id == folder));
        assert!(!a.notes.iter().find(|n| n.id == hidden).unwrap().trashed);
        drop(a);
        std::fs::remove_dir_all(root).unwrap();
    }
}
