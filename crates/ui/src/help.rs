use super::*;

impl NotesView {
    pub(super) fn help_panel(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut shortcuts = div().flex().flex_col().gap(px(1.));
        // Keys as keycaps: words are keys (`Page_Up` is one), `/`, `·` and
        // `or` separate alternatives, and lowercase words (`scroll`) are
        // gestures written out. A note follows in muted text.
        for (label, keys, note) in [
            ("Pen · Eraser · Lasso · Pan", "P · E · L · H", None),
            ("Text · Shapes", "T · S", None),
            (
                "Tool wheel: click or tap a tool; choose its tools in Settings › Writing",
                "W",
                Some("Also the pen's lower button, or a pad button set to wheel"),
            ),
            ("Edit selected text / Bold · Italic · Underline", "Enter / Ctrl B · I · U", None),
            ("Undo / Redo", "Ctrl Z / Ctrl Shift Z or Ctrl Y", None),
            ("Copy / Cut / Paste", "Ctrl C / X / V", None),
            ("Select all / Delete selection", "Ctrl A / Delete or Backspace", None),
            ("Find / Save", "Ctrl F / Ctrl S", None),
            ("Next / Previous match", "Enter or F3 / Shift Enter or Shift F3", None),
            ("New document / New page", "Ctrl N / Ctrl Shift N", None),
            ("Open a tab / Library", "Ctrl T / Ctrl Shift L", None),
            ("Close tab / Next / Previous", "Ctrl W / Ctrl Tab / Ctrl Shift Tab", None),
            ("Previous / Next page", "Ctrl Page_Up / Ctrl Page_Down", None),
            ("Pages / Settings", "Ctrl Shift P / Ctrl ,", None),
            ("Import / Export", "Ctrl O / Ctrl Shift E", None),
            ("Zoom in / Zoom out / Fit page", "Ctrl + / Ctrl − / Ctrl 0", None),
            ("Zoom / Pan", "Ctrl scroll / middle-drag", None),
            ("Undo in the library", "Ctrl Z", None),
        ] {
            // Combinations stay on one line; alternatives wrap between them.
            let mut caps = div()
                .w(relative(0.5))
                .flex_shrink_0()
                .flex()
                .flex_wrap()
                .justify_end()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(theme.muted));
            let mut combo: Option<Div> = None;
            for token in keys.split_whitespace() {
                let gesture = token.len() > 1 && token.starts_with(|c: char| c.is_lowercase());
                if matches!(token, "/" | "·" | "or") {
                    caps = caps.children(combo.take()).child(div().child(token));
                } else {
                    let part = if gesture {
                        div().child(token)
                    } else {
                        super::keycap(token.replace('_', " "), theme)
                    };
                    combo = Some(
                        combo
                            .unwrap_or_else(|| div().flex().items_center().gap(px(2.)))
                            .child(part),
                    );
                }
            }
            caps = caps.children(combo);
            shortcuts = shortcuts.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .min_h(rems(2.))
                    .px_2()
                    .py_1()
                    .text_sm()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(label)
                            .children(note.map(|note| {
                                div().text_xs().text_color(rgb(theme.muted)).child(note)
                            })),
                    )
                    .child(caps),
            );
        }
        // Help opens as a popover under its button and reads as a menu: a
        // press outside or Escape closes it.
        let frame = self.popover_frame(window, 520., 720.);
        let separator = || document_actions::separator(theme);
        frame
            .place(div().id("help-panel"))
            .occlude()
            .overflow_y_scroll()
            .p_1p5()
            .glass(theme, theme.popover)
            .shadow_md()
            .rounded(px(theme.corner_md()))
            .flex()
            .flex_col()
            .gap(px(1.))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, w, cx| {
                this.help_open = false;
                this.focus.focus(w);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .min_h(rems(2.))
                    .px_2()
                    .py_1()
                    .text_sm()
                    .child(icon(Icon::Help, theme.muted).size(rems(1.125)))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Help and shortcuts")),
            )
            .child(separator())
            .child(self.menu_row(
                "create-starter",
                "Open starter document".into(),
                div().child("Open starter document"),
                Icon::Book,
                theme.muted,
                None,
                cx,
                |this, w, _| {
                    this.controller.create_starter_notebook();
                    this.help_open = false;
                    this.show_editor();
                    this.focus.focus(w);
                },
            ))
            .child(self.menu_row(
                "start-tour",
                "Take the tour".into(),
                div().child("Take the tour"),
                Icon::Highlighter,
                theme.muted,
                None,
                cx,
                |this, w, _| this.open_tour(w),
            ))
            .child(self.menu_row(
                "start-input-check",
                "Start input check".into(),
                div().child("Start input check"),
                Icon::Pen,
                theme.muted,
                None,
                cx,
                |this, w, _| {
                    this.controller.finish();
                    this.diagnostics.check = Some(diagnostics::InputCheck::new());
                    this.help_open = false;
                    this.show_editor();
                    this.focus.focus(w);
                },
            ))
            .child(div().px_2().pb_1().text_xs().text_color(rgb(theme.muted)).child(
                "Learn writing, selection, recognition and math with an editable example document.",
            ))
            .child(separator())
            .child(document_actions::heading(theme, "Shortcuts"))
            .child(shortcuts)
            .child(separator())
            .child(div().px_2().py_1().text_xs().text_color(rgb(theme.muted)).child(
                "Select handwriting and choose Recognize text or Recognize math, then review, Copy text or Replace writing. Replacement can be undone. Two-finger scrolling pans the canvas. Hold at the end of a stroke to snap a shape. Escape closes a menu or cancels recognition and your current stroke.",
            ))
    }
}
