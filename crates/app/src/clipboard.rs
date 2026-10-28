//! Copying and pasting objects, text and images.
use super::*;

impl Controller {
    pub fn copy_objects(&self) -> Vec<Object> {
        let ids = self.with_derived_sources(&self.session().selection);
        self.ordered_selection(&ids)
            .into_iter()
            .filter_map(|(_, id)| self.page().objects.get(&id))
            .map(|o| o.as_ref().clone())
            .collect()
    }
    pub fn paste_objects(&mut self, objects: Vec<Object>) {
        self.paste_objects_offset(objects, Point::new(24., 24.));
    }
    fn paste_objects_at(&mut self, objects: Vec<Object>, position: Point) {
        let Some(bounds) = objects.iter().map(Object::bounds).reduce(Rect::union) else {
            return;
        };
        self.paste_objects_offset(
            objects,
            Point::new(position.x - bounds.min.x, position.y - bounds.min.y),
        );
    }
    fn paste_objects_offset(&mut self, objects: Vec<Object>, offset: Point) {
        if self.read_only() || objects.is_empty() {
            return;
        }
        let map: HashMap<Id, Id> = objects.iter().map(|o| (o.id(), Id::new_v4())).collect();
        let mut changes = vec![];
        let index = self.page().order.len();
        let mut ids = HashSet::new();
        for (i, mut o) in objects.into_iter().enumerate() {
            o.set_id(map[&o.id()]);
            o.set_transform(Transform::translate(offset.x, offset.y).compose(o.transform()));
            match &mut o {
                Object::Shape(s) => {
                    s.source_strokes = s
                        .source_strokes
                        .iter()
                        .filter_map(|id| map.get(id).copied())
                        .collect()
                }
                Object::Equation(e) => {
                    e.source_strokes = e
                        .source_strokes
                        .iter()
                        .filter_map(|id| map.get(id).copied())
                        .collect();
                    if let Some(link) = &mut e.math_link {
                        link.sources = link
                            .sources
                            .iter()
                            .filter_map(|id| map.get(id).copied())
                            .collect();
                        link.ink_region = if link.sources.is_empty() {
                            None
                        } else {
                            link.ink_region.map(|r| {
                                Rect::new(
                                    r.min.x + offset.x,
                                    r.min.y + offset.y,
                                    r.width(),
                                    r.height(),
                                )
                            })
                        };
                    }
                }
                _ => {}
            }
            ids.insert(o.id());
            changes.push(Change::Object {
                page: self.page().id,
                id: o.id(),
                before: None,
                after: Some(Arc::new(o)),
                index: index + i,
            });
        }
        self.commit("Paste objects", changes);
        self.session_mut().selection = ids;
        self.tool = Tool::Lasso;
        self.temporary_selection = false;
    }
    pub fn encode_clipboard(&self) -> Option<String> {
        let objects = self.copy_objects();
        if objects.is_empty() {
            None
        } else {
            serde_json::to_string(&objects)
                .ok()
                .map(|s| format!("FOLIO-OBJECTS-V2\n{s}"))
        }
    }
    pub fn paste_text_or_objects(&mut self, text: String) {
        self.paste_clipboard_text(text, None);
    }
    pub fn paste_text_or_objects_at(&mut self, text: String, position: Point) {
        self.paste_clipboard_text(text, Some(position));
    }
    fn paste_clipboard_text(&mut self, text: String, position: Option<Point>) {
        if self.read_only() {
            return;
        }
        if let Some(data) = text
            .strip_prefix("FOLIO-OBJECTS-V2\n")
            .or_else(|| text.strip_prefix("FOLIO-OBJECTS-V1\n"))
        {
            match serde_json::from_str::<Vec<Object>>(data) {
                Ok(objects) => {
                    let mut page = Page::new();
                    for o in &objects {
                        page.order.push(o.id());
                        page.objects.insert(o.id(), Arc::new(o.clone()));
                    }
                    let mut doc = Document::new("clipboard");
                    doc.pages = vec![page];
                    if let Err(e) = doc.validate() {
                        self.error = Some(e)
                    } else {
                        if let Some(position) = position {
                            self.paste_objects_at(objects, position);
                        } else {
                            self.paste_objects(objects);
                        }
                    }
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        } else {
            self.add_text(
                text,
                position.or(self.cursor).unwrap_or(Point::new(100., 100.)),
            );
        }
    }
    pub fn paste_image(&mut self, data: &[u8], extension: &str) {
        self.paste_image_at(data, extension, Point::new(80., 80.));
    }
    pub fn paste_image_at(&mut self, data: &[u8], extension: &str, position: Point) {
        if self.read_only() {
            return;
        }
        let path =
            self.data_dir
                .join("jobs")
                .join(format!("clipboard-{}.{}", Id::new_v4(), extension));
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, data) {
            Ok(()) => {
                let note = self.active;
                let job = Job::ImportImage {
                    note,
                    page: self.page().id,
                    path,
                    assets: self.assets.clone(),
                    position,
                };
                if let Err(error) = self.workers.submit(job) {
                    self.error = Some(error);
                } else {
                    self.busy += 1;
                    *self.pending_imports.entry(note).or_default() += 1;
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}
