//! Stable folder hierarchy and unambiguous paths shared by navigation and pickers.
use super::*;
impl Controller {
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
        let mut ordered: Vec<_> = self.notebooks.iter().collect();
        ordered.sort_by_key(|n| (n.name.to_lowercase(), n.name.clone(), n.id));
        let roots: Vec<_> = ordered
            .iter()
            .filter(|n| {
                n.parent.is_none() || !self.notebooks.iter().any(|p| Some(p.id) == n.parent)
            })
            .map(|n| n.id)
            .collect();
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        // Orphans and invalid cycles remain visible instead of disappearing.
        for root in roots.into_iter().chain(ordered.iter().map(|n| n.id)) {
            let mut stack = vec![(root, 0)];
            while let Some((id, depth)) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let Some(folder) = self.notebooks.iter().find(|n| n.id == id) else {
                    continue;
                };
                result.push((folder.clone(), depth));
                stack.extend(
                    ordered
                        .iter()
                        .rev()
                        .filter(|n| n.parent == Some(id))
                        .map(|n| (n.id, depth + 1)),
                );
            }
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
