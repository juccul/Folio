use super::*;
impl NotesView {
    pub(super) fn recognition_notice(&self, cx: &mut Context<Self>) -> Div {
        div()
            .px_4()
            .py_1()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .child(self.controller.recognition_status.clone()),
            )
            .child(
                self.button(
                    "ocr-cancel",
                    "Cancel recognition",
                    false,
                    cx,
                    |this, _, _| this.controller.cancel_recognition(),
                )
                .text_xs(),
            )
    }
}
