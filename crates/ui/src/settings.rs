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
    /// A setting's label and description, with its control appended by the
    /// caller. Rows sit inside a [`Self::settings_group`] card.
    pub(super) fn settings_row(&self, label: &str, description: &str) -> Div {
        let theme = Theme::new(&self.controller.settings);
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_4()
            .px_4()
            .py_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(label.to_owned()),
                    )
                    .when(!description.is_empty(), |row| {
                        row.child(
                            div()
                                .text_sm()
                                .text_color(rgb(theme.secondary()))
                                .child(description.to_owned()),
                        )
                    }),
            )
    }
    /// Rows framed as one card with dividers, under an optional heading.
    pub(super) fn settings_group(
        &self,
        heading: Option<&str>,
        rows: impl IntoIterator<Item = Div>,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut card = div()
            .flex()
            .flex_col()
            .rounded(px(theme.corner_md()))
            .border_1()
            .border_color(theme.glass_edge())
            .bg(theme.card())
            .overflow_hidden();
        for (index, row) in rows.into_iter().enumerate() {
            card = card.child(row.when(index > 0, |row| row.border_t_1().border_color(theme.border)));
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mb_5()
            .when_some(heading, |group, heading| {
                group.child(
                    div()
                        .px_1()
                        .text_size(rems(0.8125))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.secondary()))
                        .child(heading.to_owned()),
                )
            })
            .child(card)
    }
    /// A secondary action button with a visible outline.
    pub(super) fn settings_action(
        &self,
        id: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::new(&self.controller.settings);
        self.button(id, label, false, cx, action)
            .flex_shrink_0()
            .px_3()
            .py_1()
            .border_color(theme.border)
    }
    /// An outlined − value + control. `step` receives −1 or +1.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn settings_stepper(
        &self,
        (less, less_label): (&'static str, &'static str),
        (more, more_label): (&'static str, &'static str),
        value: String,
        cx: &mut Context<Self>,
        step: impl Fn(&mut Self, f32) + Clone + 'static,
    ) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let mut stepper = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .p(px(2.))
            .rounded(px(theme.corner_md()))
            .border_1()
            .border_color(theme.glass_edge())
            .bg(theme.raised());
        for (id, label, sign) in [(less, less_label, -1.), (more, more_label, 1.)] {
            if sign > 0. {
                stepper = stepper.child(
                    div()
                        .w(rems(3.5))
                        .text_sm()
                        .text_align(TextAlign::Center)
                        .child(value.clone()),
                );
            }
            let step = step.clone();
            stepper = stepper.child(
                self.control(
                    id,
                    label,
                    div()
                        .child(if sign < 0. { "−" } else { "+" })
                        .into_any_element(),
                    false,
                    cx,
                    move |this, _, _| {
                        step(this, sign);
                        this.controller.store_settings();
                    },
                )
                .size(rems(1.875))
                .p_0()
                .bg(transparent_black()),
            );
        }
        stepper
    }
    fn writing_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let mut gestures = Vec::new();
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
            gestures.push(self.settings_row(label, description).child(self.appearance_switch(
                id,
                label,
                enabled,
                cx,
                move |this| {
                    let s = &mut this.controller.settings;
                    match kind {
                        1 => s.scratch_erase = !s.scratch_erase,
                        2 => s.hold_shapes = !s.hold_shapes,
                        _ => s.encircle_select = !s.encircle_select,
                    }
                },
            )));
        }
        let pad = self.pad_button_rows(cx);
        let wheel = self.tool_wheel_rows(cx);
        div()
            .flex()
            .flex_col()
            .child(self.settings_group(Some("Smart Ink gestures"), gestures))
            .child(self.settings_group(Some("Tablet pad buttons"), pad))
            .child(self.settings_group(Some("Tool wheel"), wheel))
    }
    /// The wheel's tools, picked in clockwise order, and whether the pen's
    /// barrel button opens it.
    fn tool_wheel_rows(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let theme = Theme::new(&self.controller.settings);
        let settings = &self.controller.settings;
        let default = folio_app::Settings::default();
        let changed = settings.wheel_tools != default.wheel_tools
            || settings.barrel_opens_wheel != default.barrel_opens_wheel;
        let chosen = self.wheel_tools();
        let mut picker = div()
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(theme.corner_md()))
            .border_1()
            .border_color(theme.glass_edge())
            .bg(theme.raised());
        for tool in tool_wheel::WheelTool::ALL {
            let position = chosen.iter().position(|&slot| slot == tool);
            let order = position.map(|index| {
                div()
                    .size(rems(1.125))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(rgb(theme.accent))
                    .text_color(rgb(theme.primary_foreground))
                    .text_xs()
                    .line_height(rems(0.75))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child((index + 1).to_string())
            });
            let status = match position {
                Some(index) => format!("On, position {}", index + 1),
                None => "Off".into(),
            };
            picker = picker.child(
                self.control(
                    format!("tool-wheel-{}", tool.slug()),
                    format!("{} on the tool wheel: {status}", tool.label()),
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .children(order)
                        .child(tool.label())
                        .into_any_element(),
                    position.is_some(),
                    cx,
                    move |this, _, _| {
                        let mut tools = this.wheel_tools();
                        if let Some(index) = tools.iter().position(|&slot| slot == tool) {
                            if tools.len() <= folio_app::MIN_WHEEL_TOOLS {
                                return;
                            }
                            tools.remove(index);
                        } else {
                            tools.push(tool);
                        }
                        this.controller.settings.wheel_tools =
                            tools.iter().map(|slot| slot.slug().to_owned()).collect();
                        this.controller.store_settings();
                    },
                )
                .px_3()
                .py_1()
                .min_h(rems(1.875))
                .when(position.is_none(), |pill| pill.text_color(rgb(theme.muted)))
                .bg(theme.tint(if position.is_some() { 1. } else { 0. })),
            );
        }
        let tools = div()
            .flex()
            .flex_col()
            .child(
                self.settings_row(
                    "Tools",
                    "Clockwise from the top, in the order you turn them on. At least two.",
                )
                .children(changed.then(|| {
                    self.settings_action("tool-wheel-reset", "Reset", cx, |this, _, _| {
                        let default = folio_app::Settings::default();
                        this.controller.settings.wheel_tools = default.wheel_tools;
                        this.controller.settings.barrel_opens_wheel = default.barrel_opens_wheel;
                        this.controller.store_settings();
                    })
                    .text_xs()
                })),
            )
            .child(div().px_4().pb_3().child(picker));
        let barrel = self
            .settings_row(
                "Open with the pen button",
                "Press the pen's lower barrel button to open the wheel at the pen.",
            )
            .child(self.appearance_switch(
                "tool-wheel-barrel",
                "Open with the pen button",
                self.controller.settings.barrel_opens_wheel,
                cx,
                |this| {
                    this.controller.settings.barrel_opens_wheel =
                        !this.controller.settings.barrel_opens_wheel
                },
            ));
        vec![tools, barrel]
    }
    /// Selects the row of a pressed pad button while the Writing settings
    /// are shown, adding rows up to it.
    pub(super) fn pad_button_pressed(&mut self, event: &TabletPadEvent, cx: &mut Context<Self>) {
        let Some(button) = event.button.map(|b| b as usize) else {
            return;
        };
        if !event.pressed
            || self.settings_section != Section::Writing
            || button >= folio_app::MAX_PAD_BUTTONS
        {
            return;
        }
        let buttons = &mut self.controller.settings.pad_buttons;
        if buttons.len() <= button {
            buttons.resize(button + 1, String::new());
            self.controller.store_settings();
        }
        self.pad_button = Some(button);
        cx.notify();
    }
    /// One row per pad button, each opening an inline action chooser.
    fn pad_button_rows(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let theme = Theme::new(&self.controller.settings);
        let buttons = self.controller.settings.pad_buttons.clone();
        let changed = buttons != folio_app::Settings::default().pad_buttons;
        let mut rows = vec![
            self.settings_row(
                "Pad buttons",
                "Press a button on your tablet to find its row.",
            )
            .children(changed.then(|| {
                self.settings_action("pad-reset", "Reset", cx, |this, _, _| {
                    this.controller.settings.pad_buttons =
                        folio_app::Settings::default().pad_buttons;
                    this.pad_button = None;
                    this.controller.store_settings();
                })
                .text_xs()
            }))
            .when(buttons.len() < folio_app::MAX_PAD_BUTTONS, |row| {
                row.child(
                    self.settings_action("pad-add", "Add button", cx, |this, _, _| {
                        let buttons = &mut this.controller.settings.pad_buttons;
                        buttons.push(String::new());
                        this.pad_button = Some(buttons.len() - 1);
                        this.controller.store_settings();
                    }),
                )
            }),
        ];
        for (index, slug) in buttons.iter().enumerate() {
            let open = self.pad_button == Some(index);
            let name = format!("Button {}", index + 1);
            let chooser = self
                .control(
                    format!("pad-button-{index}"),
                    format!("{name} action: {}", folio_app::pad_action_label(slug)),
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(folio_app::pad_action_label(slug))
                        .child(icon(Icon::Down, theme.muted).size(rems(0.875)))
                        .into_any_element(),
                    open,
                    cx,
                    move |this, _, _| {
                        this.pad_button = (this.pad_button != Some(index)).then_some(index);
                    },
                )
                .flex_shrink_0()
                .w(rems(10.))
                .px_3()
                .py_1()
                .border_color(theme.border);
            // Removing a middle row would shift every later button's action.
            let remove = (index + 1 == buttons.len()).then(|| {
                self.control(
                    "pad-remove",
                    format!("Remove {name}"),
                    icon(Icon::Close, theme.muted).into_any_element(),
                    false,
                    cx,
                    |this, _, _| {
                        let buttons = &mut this.controller.settings.pad_buttons;
                        buttons.pop();
                        if this.pad_button.is_some_and(|b| b >= buttons.len()) {
                            this.pad_button = None;
                        }
                        this.controller.store_settings();
                    },
                )
                .size(rems(1.875))
                .p_0()
                .bg(transparent_black())
            });
            let row = self.settings_row(&name, "").py_1p5().child(chooser).child(
                // Keeps choosers aligned whether or not a row can be removed.
                div().w(rems(1.875)).flex_shrink_0().children(remove),
            );
            if !open {
                rows.push(row);
                continue;
            }
            let mut actions = div()
                .flex()
                .flex_wrap()
                .gap(px(2.))
                .p(px(2.))
                .rounded(px(theme.corner_md()))
                .border_1()
                .border_color(theme.glass_edge())
                .bg(theme.raised());
            for (action, label) in folio_app::PAD_ACTIONS {
                let selected = folio_app::pad_action_label(slug) == label;
                actions = actions.child(
                    self.control(
                        format!(
                            "pad-action-{}",
                            if action.is_empty() { "none" } else { action }
                        ),
                        format!("{name}: {label}"),
                        div().child(label).into_any_element(),
                        selected,
                        cx,
                        move |this, _, _| {
                            if let Some(slot) = this.controller.settings.pad_buttons.get_mut(index)
                            {
                                *slot = action.to_owned();
                                this.controller.store_settings();
                            }
                        },
                    )
                    .px_3()
                    .py_1()
                    .min_h(rems(1.875))
                    .bg(theme.tint(if selected { 1. } else { 0. })),
                );
            }
            // Part of the button's row, without a divider between them.
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .bg(theme.tint(0.5))
                    .child(row)
                    .child(div().px_4().pb_3().child(actions)),
            );
        }
        rows
    }
    fn library_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let location = self
            .settings_row(
                "Current library",
                &self.controller.data_dir.display().to_string(),
            )
            .child(self.settings_action(
                "switch-library",
                "Open another…",
                cx,
                |this, w, cx| this.switch_library_dialog(w, cx),
            ))
            .child(
                self.settings_action("default-library", "Use by default", cx, |this, _, _| {
                    if let Err(e) = this.controller.use_library_by_default() {
                        this.controller.error = Some(e);
                    }
                }),
            );
        let saving = [
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
        ];
        let mut backup = vec![
            self.settings_row(
                "Back up library",
                "Includes documents, folders, assets and editing history.",
            )
            .child(self.settings_action(
                "backup-library",
                "Back up…",
                cx,
                |this, _, cx| this.backup_dialog(cx),
            )),
            self.settings_row("Restore a backup", "Restores into a new library folder.")
                .child(self.settings_action(
                    "restore-library",
                    "Restore…",
                    cx,
                    |this, _, cx| this.restore_dialog(cx),
                )),
        ];
        if let Some(path) = &self.controller.restored_library {
            backup.push(
                self.settings_row("Restored library", &path.display().to_string())
                    .child(
                        self.button(
                            "open-restored-library",
                            "Open restored library",
                            true,
                            cx,
                            |this, w, cx| this.open_restored_library(w, cx),
                        )
                        .flex_shrink_0()
                        .px_3()
                        .py_1(),
                    ),
            );
        }
        let maintenance = [
            self.settings_row(
                "Recovery snapshot",
                "Keep a recovery point for the current document.",
            )
            .child(self.settings_action(
                "checkpoint-settings",
                "Create",
                cx,
                |this, _, _| this.controller.recovery_checkpoint(),
            )),
            self.settings_row(
                "Unused assets",
                "Quarantine files once all edits have saved.",
            )
            .child(self.settings_action(
                "cleanup-assets-settings",
                "Clean up",
                cx,
                |this, _, _| this.controller.cleanup_assets(),
            )),
        ];
        let mut status = vec![
            self.settings_row("Saving", "").child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(self.controller.activity_status()),
            ),
            self.settings_row(
                "Handwriting recognition",
                if self.controller.recognition_ready() {
                    "Ready offline."
                } else {
                    "Offers setup when you first use it."
                },
            ),
        ];
        for task in self.controller.tasks.iter().rev().take(8) {
            let state = match &task.state {
                folio_app::TaskState::Running => "Running…".into(),
                folio_app::TaskState::Complete => "Completed".into(),
                folio_app::TaskState::Failed(e) => format!("Failed: {e}"),
            };
            status.push(self.settings_row(&task.label, &state));
        }
        div()
            .flex()
            .flex_col()
            .child(self.settings_group(Some("Location"), [location]))
            .child(self.settings_group(Some("Saving and startup"), saving))
            .child(self.settings_group(Some("Backup and restore"), backup))
            .child(self.settings_group(Some("Maintenance"), maintenance))
            .child(self.settings_group(Some("Status"), status))
    }
    fn accessibility_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let mut rows = vec![
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
        ];
        let default = folio_app::Settings::default();
        for (label, description, interface) in [
            ("Interface size", "Resize text, buttons and toolbars.", true),
            (
                "Pen cursor size",
                "Make the cursor easier to see on the page.",
                false,
            ),
        ] {
            let settings = &self.controller.settings;
            let (value, changed, less, more) = if interface {
                (
                    format!("{:.0}%", settings.ui_scale * 100.),
                    (settings.ui_scale - default.ui_scale).abs() > 0.01,
                    ("ui-smaller", "Decrease Interface size"),
                    ("ui-larger", "Increase Interface size"),
                )
            } else {
                (
                    format!("{:.0} px", settings.cursor_size),
                    (settings.cursor_size - default.cursor_size).abs() > 0.01,
                    ("cursor-smaller", "Decrease Pen cursor size"),
                    ("cursor-larger", "Increase Pen cursor size"),
                )
            };
            let reset = changed.then(|| {
                self.settings_action(
                    if interface { "ui-reset" } else { "cursor-reset" },
                    "Reset",
                    cx,
                    move |this, _, _| {
                        let default = folio_app::Settings::default();
                        if interface {
                            this.controller.settings.ui_scale = default.ui_scale;
                        } else {
                            this.controller.settings.cursor_size = default.cursor_size;
                        }
                        this.controller.store_settings();
                    },
                )
                .text_xs()
            });
            rows.push(
                self.settings_row(label, description)
                    .children(reset)
                    .child(self.settings_stepper(less, more, value, cx, move |this, sign| {
                        let s = &mut this.controller.settings;
                        if interface {
                            s.ui_scale = (s.ui_scale + sign * 0.1).clamp(0.8, 1.6);
                        } else {
                            s.cursor_size = (s.cursor_size + sign * 2.).clamp(4., 64.);
                        }
                    })),
            );
        }
        div().flex().flex_col().child(self.settings_group(None, rows))
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
            .border_color(theme.glass_edge())
            .flex_shrink_0();
        for (key, target, kind) in [
            ("appearance", Section::Appearance, Icon::Palette),
            ("writing", Section::Writing, Icon::Pen),
            ("library", Section::Library, Icon::Library),
            ("accessibility", Section::Accessibility, Icon::Accessible),
            ("updates", Section::Updates, Icon::Import),
        ] {
            let active = target == section;
            navigation = navigation.child(
                self.control(
                    format!("settings-section-{key}"),
                    target.title(),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            icon(kind, if active { theme.ink } else { theme.muted })
                                .size(rems(1.125)),
                        )
                        .child(target.title())
                        .into_any_element(),
                    active,
                    cx,
                    move |this, _, _| this.settings_section = target,
                )
                .justify_start()
                .py_1p5()
                .bg(theme.tint(if section == target { 1. } else { 0. }))
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
        // A thin scrollbar thumb, from the last layout, when the section
        // is taller than the dialog.
        let scrollbar = {
            let viewport = f32::from(self.settings_scroll.bounds().size.height);
            let range = f32::from(self.settings_scroll.max_offset().height);
            // Only when meaningfully more content is below.
            (viewport > 0. && range > 48.).then(|| {
                let thumb = (viewport * viewport / (viewport + range)).max(32.);
                let progress = (-f32::from(self.settings_scroll.offset().y) / range).clamp(0., 1.);
                div()
                    .absolute()
                    .right(px(5.))
                    .top(px(4. + (viewport - thumb - 8.) * progress))
                    .w(px(4.))
                    .h(px(thumb))
                    .rounded_full()
                    .bg(rgba((theme.ink << 8) | 0x4d))
            })
        };
        let body = div()
            .id(("settings-content", section as usize))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .p_6()
            .pb_10()
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
                            .mb_5()
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
            .scrim(theme)
            .flex()
            .items_center()
            .justify_center()
            .child(transitions::Transitions::dialog_box(
                1. - self.dialog_rise,
                div()
                    .w(px(900.))
                    .max_w(px((f32::from(window.viewport_size().width) - 32.).max(300.)))
                    .h(px(
                        (f32::from(window.viewport_size().height) - 64.).min(880.)
                    ))
                    .glass_dialog(theme, theme.popover)
                    .rounded(px(theme.corner_lg()))
                    .overflow_hidden()
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
                            .child(
                                div()
                                    .relative()
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .child(body)
                                    // A soft fade marks content below the fold.
                                    .child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .right_0()
                                            .bottom_0()
                                            .h(rems(1.5))
                                            .bg(linear_gradient(
                                                180.,
                                                linear_color_stop(rgba(theme.popover << 8), 0.),
                                                linear_color_stop(
                                                    rgba((theme.popover << 8) | 0xcc),
                                                    1.,
                                                ),
                                            )),
                                    )
                                    .children(scrollbar),
                            ),
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
            ))
    }
}
