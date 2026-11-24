use super::*;
impl NotesView {
    pub(super) fn input_check_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let check = self.diagnostics.check.as_ref().expect("Visible check");
        let running = check.running;
        let theme = Theme::new(&self.controller.settings);
        div().id("input-check-panel").px_4().py_2().flex().flex_col().gap_1()
            .border_b_1().border_color(theme.border)
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(div().text_sm().child(check.label()))
                .child(self.button("reset-input-check", "Reset check", false, cx, |this, _, _| {
                    this.diagnostics.check = Some(diagnostics::InputCheck::new());
                }))
                .child(self.button("stop-input-check", if running {"Stop check"} else {"Resume check"}, false, cx, |this, _, _| {
                    if let Some(check) = &mut this.diagnostics.check {
                        if check.running { check.stop(); }
                        else { this.diagnostics.check = Some(diagnostics::InputCheck::new()); }
                    }
                }))
                .child(self.button("save-input-report", "Save input report…", false, cx, |this, _, cx| {
                    this.save_input_report(cx);
                }))
                .child(self.button("close-input-check", "Close check", false, cx, |this, _, _| {
                    this.diagnostics.check = None;
                })))
            .child(div().text_xs().text_color(rgb(theme.muted))
                .child("Write on a test page. CPU paint timing excludes the display; measure physical latency with a camera. Resume starts a new check."))
    }
    fn save_input_report(&mut self, cx: &mut Context<Self>) {
        let Some(check) = &self.diagnostics.check else {
            return;
        };
        let report = serde_json::to_vec_pretty(&check.report()).expect("Serializable report");
        let suggested = format!("Folio-input-{}.json", folio_document::now_ms());
        let options = rune::FileDialogOptions {
            title: "Save input check".into(),
            directory: Some(self.controller.data_dir.clone()),
            file_name: Some(suggested),
            ..Default::default()
        };
        let report = std::rc::Rc::new(report);
        cx.save_file(options, move |view, path, _, _| {
            let Some(path) = path else { return };
            if let Err(error) = view.controller.validate_export_destination(&path, "json") {
                view.controller.error = Some(error);
                return;
            }
            if let Err(error) = folio_export::atomic_write(&path, &report) {
                view.controller.error = Some(error.to_string());
            } else {
                view.controller.status = "Input check saved".into();
            }
        });
    }
}
