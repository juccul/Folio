//! Update notice lives in the shared title bar, beside the window controls.
use super::workspace::Hint;
use super::*;
use folio_update::State;

impl NotesView {
    pub(super) fn update_notice(&self, cx: &mut Context<Self>) -> Div {
        let state = &self.updater.state;
        let Some(version) = state.version() else {
            return div();
        };
        let label = if self.update_preparing {
            "Saving and backing up…".to_owned()
        } else {
            match state {
                State::Available { .. } => format!("Update · {version}"),
                State::Downloading { percent, .. } => format!("Downloading {percent}%"),
                State::Ready { .. } => "Restart to update".into(),
                State::Preparing { .. } => "Preparing update…".into(),
                State::Failed { ready: true, .. } => "Retry restart".into(),
                State::Failed { .. } => "Retry update".into(),
                _ => return div(),
            }
        };
        let detail = match state {
            State::Failed { error, .. } => error.clone(),
            State::Ready { .. } => format!(
                "Folio {version} is ready. Save, back up your library and restart to update."
            ),
            _ => {
                format!("Folio {version} is available. Downloads begin only when you click Update.")
            }
        };
        let theme = Theme::new(&self.controller.settings);
        div().flex_shrink_0().mb(px(3.)).child(
            self.control(
                "app-update",
                label.clone(),
                div().text_xs().child(label).into_any_element(),
                false,
                cx,
                |this, window, cx| this.update_action(window, cx),
            )
            .h(px(30.))
            .min_h(px(30.))
            .px_3()
            .py_0()
            .flex_shrink_0()
            .bg(rgb(theme.selected))
            .text_color(rgb(theme.accent))
            .tooltip(move |_, cx| cx.new(|_| Hint(detail.clone().into(), theme)).into()),
        )
    }
    pub(super) fn update_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_preparing {
            return;
        }
        if self.updater.state.can_download() {
            self.updater.download();
            cx.notify();
            return;
        }
        if !self.updater.state.can_restart() {
            return;
        }
        if !self.controller.can_restart_for_update() {
            self.controller.error = Some("Wait for the current import, export or recognition task to finish before restarting".into());
            cx.notify();
            return;
        }
        self.finish_inline_text(window, cx);
        self.store_workspace();
        if let Err(error) = self.controller.flush() {
            self.controller.error = Some(format!("Could not save before updating: {error}"));
            cx.notify();
            return;
        }
        let data = self.controller.data_dir.clone();
        let version = self.updater.state.version().unwrap().to_owned();
        let unique = folio_document::Id::new_v4();
        let backup = data.join(format!("before-update-{version}-{unique}.foliobackup"));
        self.update_preparing = true;
        let task = cx
            .background_executor()
            .spawn(async move { folio_app::portable::backup(&data, &backup).map(|()| data) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.update_preparing = false;
                match result {
                    Ok(data) => {
                        this.updater.restart(data);
                        this.update_preparing = true;
                    }
                    Err(error) => {
                        this.controller.error = Some(format!(
                            "Could not back up your library; Folio stayed open: {error}"
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn update_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::new(&self.controller.settings);
        let description =
            self.updater
                .check_error
                .clone()
                .unwrap_or_else(|| match &self.updater.state {
                    State::Checking => "Checking for a newer stable release…".into(),
                    State::Current => "No newer release is available.".into(),
                    State::Available { version } => format!("Folio {version} is available."),
                    State::Downloading { percent, .. } => {
                        format!("Downloading the update: {percent}%")
                    }
                    State::Ready { version } => {
                        format!("Folio {version} is downloaded and verified.")
                    }
                    State::Preparing { .. } => "Preparing a safe restart…".into(),
                    State::Failed { error, .. } => error.clone(),
                    State::Exit => "Restarting…".into(),
                });
        let supported = self.updater.supported();
        let action = if self.updater.state.can_restart() {
            "Restart to update"
        } else {
            "Update"
        };
        div().flex().flex_col()
            .child(self.settings_row("Installed version", env!("CARGO_PKG_VERSION")))
            .child(div().mt_4().text_sm().text_color(rgb(theme.muted)).child(if supported { description } else { "Use your distribution's package manager to update this build.".into() }))
            .when(supported, |body| body.child(div().flex().flex_wrap().gap_2().mt_4()
                .child(self.button("update-check", "Check for updates", false, cx, |this, _, _| this.updater.check()))
                .when(self.updater.state.can_download() || self.updater.state.can_restart(), |row| row.child(self.button("update-settings-action", action, true, cx, |this, window, cx| {
                    this.settings_open = false; this.update_action(window, cx);
                })))
            ))
            .child(div().mt_4().text_xs().text_color(rgb(theme.muted)).child("Folio checks release metadata periodically. Updates download only after you click Update. Restart saves your documents and creates a library backup first."))
    }
}

impl NotesView {
    /// Native smoke verification displays real accessible controls without
    /// involving an update service or modifying an installed application.
    pub fn update_smoke_phase(&mut self, phase: u8) {
        self.updater = folio_update::Updater::disabled();
        self.updater.state = match phase {
            0 => State::Available {
                version: "0.1.4".into(),
            },
            1 => State::Downloading {
                version: "0.1.4".into(),
                percent: 42,
            },
            2 => State::Ready {
                version: "0.1.4".into(),
            },
            _ => State::Current,
        };
    }
    pub fn update_smoke_verify(&self, phase: u8) -> Result<(), String> {
        let label = match phase {
            0 => "Update · 0.1.4",
            1 => "Downloading 42%",
            2 => "Restart to update",
            _ => return Ok(()),
        };
        let notice = self
            .accessibility
            .control_bounds(label)
            .ok_or_else(|| format!("Update notice is missing: {label}"))?;
        let close = self
            .accessibility
            .control_bounds("Close window")
            .ok_or("Close window control is missing")?;
        if notice.x1 > close.x0 || (notice.y0 - close.y0).abs() > 4.0 {
            return Err("Update notice is not beside the window controls".into());
        }
        for control in ["Minimize window", "Maximize window"] {
            if let Some(bounds) = self.accessibility.control_bounds(control)
                && notice.x1 > bounds.x0
            {
                return Err("Update notice overlaps the window controls".into());
            }
        }
        Ok(())
    }
}
