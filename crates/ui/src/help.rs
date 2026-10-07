use super::*;

impl NotesView {
    pub(super) fn help_panel(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut shortcuts = div().flex().flex_col().gap_1();
        for (label, keys) in [
            ("Pen · Eraser · Lasso · Pan", "P · E · L · H"),
            ("Text · Shapes", "T · S"),
            ("Undo / Redo", "Ctrl Z / Ctrl Shift Z"),
            ("Copy / Cut / Paste", "Ctrl C / X / V"),
            ("Select all / Delete selection", "Ctrl A / Delete"),
            ("Search / Save", "Ctrl F / Ctrl S"),
            ("New document / New page", "Ctrl N / Ctrl Shift N"),
            ("Open a tab / Library", "Ctrl T / Ctrl Shift L"),
            ("Pages / Settings", "Ctrl Shift P / Ctrl ,"),
            ("Fit page / Zoom", "Ctrl 0 / Ctrl scroll"),
        ] {
            shortcuts = shortcuts.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .py_2()
                    .text_sm()
                    .child(label)
                    .child(div().text_xs().text_color(rgb(theme.muted)).child(keys)),
            );
        }
        div().absolute().inset_0().occlude().bg(rgba(0x00000070)).p_6()
            .flex().items_center().justify_center()
            .child(div().id("help-panel").w(px(560.)).max_w_full()
                .max_h(px((f32::from(window.viewport_size().height) / self.controller.settings.ui_scale - 48.).max(240.)))
                .overflow_y_scroll().p_6().bg(rgb(theme.popover))
                .border_1().border_color(theme.border).rounded(px(theme.radius + 4.)).shadow_sm()
                .flex().flex_col().gap_4()
                .child(div().flex().items_center().justify_between()
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Help and shortcuts"))
                    .child(self.button("close-help", "Got it", true, cx, |this, w, _| {
                        this.help_open = false; this.focus.focus(w);
                    })))
                .child(div().text_sm().text_color(rgb(theme.muted)).child("Learn writing, selection, recognition and math with an editable example notebook."))
                .child(self.button("create-starter", "Open starter notebook", false, cx, |this, w, _| {
                    this.controller.create_starter_notebook(); this.help_open = false; this.show_editor(); this.focus.focus(w);
                }))
                .child(shortcuts)
                .child(div().pt_3().border_t_1().border_color(theme.border).text_sm().text_color(rgb(theme.muted))
                    .child("Select handwriting and choose Recognize text or Recognize math, then review, Copy text or Replace writing. Replacement can be undone. Two-finger scrolling pans the canvas. Hold at the end of a stroke to snap a shape. Escape closes a menu or cancels recognition and your current stroke.")))
    }
}
