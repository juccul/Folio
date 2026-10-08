use super::*;
impl NotesView {
    pub(super) fn ocr_setup_panel(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let paused = self.controller.recognition_status.contains("paused");
        div().absolute().inset_0().occlude().bg(rgba(0x00000070)).flex().items_center().justify_center()
            .child(div().w(rems(33.)).max_w_full().p_5().rounded_md().bg(rgb(theme.popover)).flex().flex_col().gap_4()
                .child(div().text_lg().child(if paused {"Resume handwriting recognition setup"} else {"Set up handwriting recognition"}))
                .child(div().text_sm().child("Folio needs to download about 1.47 GB of OCR models and a local runtime. This first setup needs an internet connection and enough disk space. Recognition then runs on your computer."))
                .child(div().text_sm().child("Your selected writing stays intact. Downloaded parts are retained if you pause or cancel, so a later attempt can resume."))
                .child(div().flex().flex_wrap().gap_2()
                    .child(self.button("ocr-download",if paused {"Resume download and recognize"} else {"Download and recognize"},true,cx,|this,_,_| {if let Err(e)=this.controller.accept_recognition_setup(){this.controller.cancel_recognition();this.controller.error=Some(e);}}))
                    .child(self.button("ocr-setup-cancel","Cancel",false,cx,|this,w,_| {this.controller.cancel_recognition();this.focus.focus(w);}))))
    }
    pub(super) fn recognition_notice(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div()
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
            );
        if !self.controller.recognition_ready() {
            row = row.child(
                self.button("ocr-pause", "Pause download", false, cx, |this, _, _| {
                    this.controller.pause_recognition_setup()
                })
                .text_xs(),
            );
        }
        row.child(
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
