//! A focused settings workspace with persistent navigation and consistent rows.
use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Section {
    #[default]
    Appearance,
    Writing,
    Library,
    Accessibility,
    Updates,
}
impl Section {
    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Writing => "Writing",
            Self::Library => "Library",
            Self::Accessibility => "Accessibility",
            Self::Updates => "Updates",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Appearance => "Theme, paper and interface colors.",
            Self::Writing => "Choose how your pen and gestures behave.",
            Self::Library => "Manage your documents, saving and backups.",
            Self::Accessibility => "Adjust the interface for comfortable writing.",
            Self::Updates => "New releases and installation status.",
        }
    }
}
impl NotesView {
    pub(super) fn settings_row(&self, label: &str, description: &str) -> Div {
        let theme = Theme::new(&self.controller.settings);
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_5()
            .py_4()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(label.to_owned()),
                    )
                    .when(!description.is_empty(), |row| {
                        row.child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.muted))
                                .child(description.to_owned()),
                        )
                    }),
            )
    }
    fn settings_heading(&self, label: &str) -> Div {
        let theme = Theme::new(&self.controller.settings);
        div()
            .mt_5()
            .mb_1()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(theme.ink))
            .child(label.to_owned())
    }
    fn writing_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let mut body = div().flex().flex_col();
        for (id, label, description, enabled, kind) in [
            (
                "scratch-toggle",
                "Scratch to erase",
                "Erase ink with a deliberate scribble.",
                self.controller.settings.scratch_erase,
                1,
            ),
            (
                "hold-toggle",
                "Hold to snap shapes",
                "Pause at the end of a stroke to make a clean shape.",
                self.controller.settings.hold_shapes,
                2,
            ),
            (
                "encircle-toggle",
                "Circle to select",
                "Circle ink, then hold to select it.",
                self.controller.settings.encircle_select,
                3,
            ),
        ] {
            body = body.child(
                self.settings_row(label, description)
                    .child(self.appearance_switch(id, label, enabled, cx, move |this| {
                        let s = &mut this.controller.settings;
                        match kind {
                            1 => s.scratch_erase = !s.scratch_erase,
                            2 => s.hold_shapes = !s.hold_shapes,
                            _ => s.encircle_select = !s.encircle_select,
                        }
                    })),
            );
        }
        body.child(
            self.settings_row(
                "Tablet pad",
                "Customize the buttons on your drawing tablet.",
            )
            .child(
                self.button("pad-buttons", "Configure…", false, cx, |this, w, cx| {
                    this.modal(Modal::PadButtons, w, cx)
                })
                .flex_shrink_0(),
            ),
        )
    }
    fn library_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Current library"),
            )
            .child(
                div()
                    .mt_2()
                    .p_3()
                    .rounded(px(6.))
                    .bg(rgb(theme.bg))
                    .text_xs()
                    .child(self.controller.data_dir.display().to_string()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .mt_3()
                    .child(self.button(
                        "switch-library",
                        "Open another library…",
                        false,
                        cx,
                        |this, w, cx| this.switch_library_dialog(w, cx),
                    ))
                    .child(self.button(
                        "default-library",
                        "Use by default",
                        false,
                        cx,
                        |this, _, _| {
                            if let Err(e) = this.controller.use_library_by_default() {
                                this.controller.error = Some(e);
                            }
                        },
                    )),
            )
            .child(self.settings_heading("Saving & startup"))
            .child(
                self.settings_row("Autosave", "Save after each completed edit.")
                    .child(self.appearance_switch(
                        "autosave-toggle",
                        "Autosave",
                        self.controller.settings.autosave,
                        cx,
                        |this| {
                            this.controller
                                .set_autosave(!this.controller.settings.autosave)
                        },
                    )),
            )
            .child(
                self.settings_row(
                    "Reopen documents",
                    "Continue with your previous tabs and pages.",
                )
                .child(self.appearance_switch(
                    "reopen-documents",
                    "Reopen documents",
                    self.controller.settings.reopen_documents,
                    cx,
                    |this| {
                        this.controller.settings.reopen_documents =
                            !this.controller.settings.reopen_documents
                    },
                )),
            )
            .child(self.settings_heading("Backup & restore"))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Backups include documents, folders, assets and editing history."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .mt_3()
                    .child(self.button(
                        "backup-library",
                        "Back up library…",
                        false,
                        cx,
                        |this, _, cx| this.backup_dialog(cx),
                    ))
                    .child(self.button(
                        "restore-library",
                        "Restore backup…",
                        false,
                        cx,
                        |this, _, cx| this.restore_dialog(cx),
                    )),
            );
        if let Some(path) = &self.controller.restored_library {
            body = body
                .child(
                    div()
                        .mt_3()
                        .text_xs()
                        .child(format!("Restored to {}", path.display())),
                )
                .child(self.button(
                    "open-restored-library",
                    "Open restored library",
                    true,
                    cx,
                    |this, w, cx| this.open_restored_library(w, cx),
                ));
        }
        body = body
            .child(self.settings_heading("Maintenance"))
            .child(
                self.settings_row(
                    "Recovery snapshot",
                    "Keep a recovery point for the current document.",
                )
                .child(
                    self.button("checkpoint-settings", "Create", false, cx, |this, _, _| {
                        this.controller.recovery_checkpoint()
                    })
                    .flex_shrink_0(),
                ),
            )
            .child(
                self.settings_row(
                    "Unused assets",
                    "Quarantine files once all edits have saved.",
                )
                .child(
                    self.button(
                        "cleanup-assets-settings",
                        "Clean up",
                        false,
                        cx,
                        |this, _, _| this.controller.cleanup_assets(),
                    )
                    .flex_shrink_0(),
                ),
            )
            .child(self.settings_heading("Activity"))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(self.controller.activity_status()),
            )
            .child(div().mt_2().text_xs().text_color(rgb(theme.muted)).child(
                if self.controller.recognition_ready() {
                    "Handwriting recognition is ready offline."
                } else {
                    "Handwriting recognition will offer setup when you first use it."
                },
            ));
        for task in self.controller.tasks.iter().rev().take(8) {
            let state = match &task.state {
                folio_app::TaskState::Running => "Running…".into(),
                folio_app::TaskState::Complete => "Completed".into(),
                folio_app::TaskState::Failed(e) => format!("Failed: {e}"),
            };
            body = body.child(
                div()
                    .mt_2()
                    .text_xs()
                    .child(format!("{} · {state}", task.label)),
            );
        }
        body
    }
    fn accessibility_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut body = div().flex().flex_col().child(
            self.settings_row(
                "Reduce motion",
                "Keep transitions and interface movement to a minimum.",
            )
            .child(self.appearance_switch(
                "reduce-motion",
                "Reduce motion",
                self.controller.settings.reduce_motion,
                cx,
                |this| {
                    this.controller.settings.reduce_motion = !this.controller.settings.reduce_motion
                },
            )),
        );
        for (label, description, kind, value) in [
            (
                "Interface size",
                "Resize text, buttons and toolbars.",
                0,
                format!("{:.0}%", self.controller.settings.ui_scale * 100.),
            ),
            (
                "Pen cursor size",
                "Make the cursor easier to see on the page.",
                1,
                format!("{:.0} px", self.controller.settings.cursor_size),
            ),
        ] {
            let (less, more) = if kind == 0 {
                ("ui-smaller", "ui-larger")
            } else {
                ("cursor-smaller", "cursor-larger")
            };
            let mut stepper = div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap_1()
                .rounded(px(6.))
                .bg(rgb(theme.bg));
            for (id, sign) in [(less, -1.), (more, 1.)] {
                if sign > 0. {
                    stepper = stepper.child(
                        div()
                            .w(rems(3.5))
                            .text_sm()
                            .text_align(TextAlign::Center)
                            .child(value.clone()),
                    );
                }
                stepper = stepper.child(
                    self.control(
                        id,
                        format!(
                            "{} {label}",
                            if sign < 0. { "Decrease" } else { "Increase" }
                        ),
                        div()
                            .child(if sign < 0. { "−" } else { "+" })
                            .into_any_element(),
                        false,
                        cx,
                        move |this, _, _| {
                            if kind == 0 {
                                this.controller.settings.ui_scale =
                                    (this.controller.settings.ui_scale + sign * 0.1)
                                        .clamp(0.8, 1.6);
                            } else {
                                this.controller.settings.cursor_size =
                                    (this.controller.settings.cursor_size + sign * 2.)
                                        .clamp(4., 64.);
                            }
                            this.controller.store_settings();
                        },
                    )
                    .size(rems(2.))
                    .p_0()
                    .bg(transparent_black()),
                );
            }
            body = body.child(self.settings_row(label, description).child(stepper));
        }
        body
    }
    pub(super) fn settings_panel(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::new(&self.controller.settings);
        let compact =
            f32::from(window.viewport_size().width) / self.controller.settings.ui_scale < 800.;
        let section = self.settings_section;
        let mut navigation = div()
            .flex()
            .gap_1()
            .when(!compact, |nav| {
                nav.flex_col().w(rems(10.)).p_3().border_r_1()
            })
            .when(compact, |nav| nav.flex_wrap().px_4().py_2().border_b_1())
            .border_color(theme.border)
            .bg(rgb(theme.sidebar))
            .flex_shrink_0();
        for (key, target) in [
            ("appearance", Section::Appearance),
            ("writing", Section::Writing),
            ("library", Section::Library),
            ("accessibility", Section::Accessibility),
            ("updates", Section::Updates),
        ] {
            navigation = navigation.child(
                self.button(
                    format!("settings-section-{key}"),
                    target.title(),
                    target == section,
                    cx,
                    move |this, _, _| this.settings_section = target,
                )
                .justify_start()
                .bg(rgb(if section == target {
                    theme.selected
                } else {
                    theme.sidebar
                }))
                .when(!compact, |button| button.w_full())
                .when(compact, |button| button.py_1()),
            );
        }
        let content = match section {
            Section::Appearance => self.appearance_panel(cx),
            Section::Writing => self.writing_settings(cx),
            Section::Library => self.library_settings(cx),
            Section::Accessibility => self.accessibility_settings(cx),
            Section::Updates => self.update_settings(cx),
        };
        let body = div()
            .id(("settings-content", section as usize))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .p_6()
            .when(compact, |body| body.p_4())
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(section.title()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .mb_4()
                            .text_sm()
                            .text_color(rgb(theme.muted))
                            .child(section.description()),
                    )
                    .child(content),
            );
        div()
            .occlude()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000070))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(900.))
                    .max_w(px((f32::from(window.viewport_size().width) - 32.).max(300.)))
                    .h(px(
                        (f32::from(window.viewport_size().height) - 64.).min(720.)
                    ))
                    .bg(rgb(theme.popover))
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.))
                    .overflow_hidden()
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_6()
                            .py_4()
                            .when(compact, |header| header.px_4().py_2())
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Settings"),
                            )
                            .child(
                                self.control(
                                    "close-settings",
                                    "Close settings",
                                    icon(Icon::Close, theme.muted).into_any_element(),
                                    false,
                                    cx,
                                    |this, w, _| {
                                        this.settings_open = false;
                                        this.focus.focus(w);
                                    },
                                )
                                .size(rems(2.375))
                                .p_0()
                                .bg(transparent_black()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .when(compact, |body| body.flex_col())
                            .child(navigation)
                            .child(body),
                    )
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .when(compact, |footer| footer.px_4().py_2())
                            .border_t_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(rgb(theme.muted))
                            .flex_shrink_0()
                            .child("Changes are saved automatically."),
                    ),
            )
    }
}
