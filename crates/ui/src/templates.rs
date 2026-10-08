use super::*;
impl NotesView {
    pub(super) fn template_picker(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        let mut list = div()
            .id("template-list")
            .max_h(px(400.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3();
        if self.controller.settings.templates.is_empty() {
            return list.child(
                "Save a page as a template from the document menu to reuse its layout and content.",
            );
        }
        for template in self.controller.settings.templates.clone() {
            let id = template.id;
            let preview = template
                .preview
                .and_then(|asset| self.controller.asset_path(&asset));
            let row = div()
                .flex()
                .items_center()
                .gap_3()
                .when_some(preview, |row, path| {
                    row.child(
                        img(path)
                            .w(px(58.))
                            .h(px(78.))
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .child(div().flex_1().min_w_0().text_sm().child(template.name))
                .child(self.button(
                    format!("use-template-{id}"),
                    "Add page",
                    true,
                    cx,
                    move |this, w, cx| {
                        if let Err(e) = this.controller.add_template_page(id) {
                            this.controller.error = Some(e);
                        } else {
                            this.close_modal(w, cx);
                        }
                    },
                ))
                .child(self.button(
                    format!("rename-template-{id}"),
                    "Rename…",
                    false,
                    cx,
                    move |this, w, cx| this.modal(Modal::RenameTemplate(id), w, cx),
                ))
                .child(self.button(
                    format!("remove-template-{id}"),
                    "Remove",
                    false,
                    cx,
                    move |this, _, _| this.controller.remove_template(id),
                ));
            list = list.child(row.border_b_1().border_color(theme.border).pb_3());
        }
        list
    }
}
