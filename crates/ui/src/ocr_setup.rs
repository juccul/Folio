use super::*;
impl NotesView {
    /// Recognition under way, for when the selection toolbar (which shows the
    /// same progress) is not on screen: the status beside a pulse, and an
    /// outlined Cancel like the bar's other actions.
    pub(super) fn recognition_notice(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        div()
            .px_4()
            .py_2()
            .flex()
            .items_center()
            .gap_3()
            .child(working_dots(theme.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .child(self.controller.recognition_status.clone()),
            )
            .child(
                self.button(
                    "ocr-cancel",
                    "Cancel",
                    false,
                    cx,
                    |this, _, _| this.controller.cancel_recognition(),
                )
                .text_xs()
                .px_3()
                .py_0p5()
                .min_h(rems(1.75))
                .border_1()
                .border_color(theme.cover_edge()),
            )
    }
}
