//! Typed text boxes.
use super::*;

impl Controller {
    pub fn add_text(&mut self, text: String, position: Point) {
        if !text.is_empty() {
            self.insert_text_box(text, position);
        }
    }
    pub fn create_text_box(&mut self, position: Point) -> Id {
        if self.read_only() {
            return Id::nil();
        }
        self.insert_text_box(String::new(), position)
    }
    fn insert_text_box(&mut self, text: String, position: Point) -> Id {
        let object = Object::Text(TextBlock {
            id: Id::new_v4(),
            text,
            rect: Rect::new(position.x, position.y, 360., 160.),
            transform: Transform::default(),
            font_family: "sans-serif".into(),
            font_size: 20.,
            color: self.style.color,
            bold: false,
            italic: false,
            underline: false,
            alignment: Alignment::Left,
            list: ListStyle::None,
        });
        let id = object.id();
        self.commit(
            "Insert text",
            vec![Change::Object {
                page: self.page().id,
                id,
                before: None,
                after: Some(Arc::new(object)),
                index: self.page().order.len(),
            }],
        );
        self.session_mut().selection = HashSet::from([id]);
        self.pending_text = None;
        id
    }
    pub fn edit_text(&mut self, id: Id, change: impl FnOnce(&mut TextBlock)) {
        let Some(before) = self.page().objects.get(&id).cloned() else {
            return;
        };
        let mut after = before.as_ref().clone();
        if let Object::Text(t) = &mut after {
            change(t);
            if &after == before.as_ref() {
                return;
            }
            let index = self.page().order.iter().position(|v| *v == id).unwrap_or(0);
            self.commit(
                "Edit text",
                vec![Change::Object {
                    page: self.page().id,
                    id,
                    before: Some(before),
                    after: Some(Arc::new(after)),
                    index,
                }],
            );
        }
    }
}
