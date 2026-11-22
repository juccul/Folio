use super::*;
use std::path::PathBuf;
impl NotesView {
    pub(super) fn backup_dialog(&mut self, cx: &mut Context<Self>) {
        let suggested = format!("Folio-{}.foliobackup", folio_document::now_ms());
        let options = rune::FileDialogOptions {
            title: "Back up library".into(),
            directory: Some(self.controller.data_dir.clone()),
            file_name: Some(suggested),
            ..Default::default()
        };
        cx.save_file(options, |view, path, _, _| {
            if let Some(path) = path
                && let Err(e) = view.controller.backup_library(path)
            {
                view.controller.error = Some(e);
            }
        });
    }
    pub(super) fn restore_dialog(&mut self, cx: &mut Context<Self>) {
        let options = rune::FileDialogOptions {
            title: "Choose a Folio library backup".into(),
            ..Default::default()
        };
        cx.open_file(options, |_, path, _, cx| {
            let Some(path) = path else { return };
            let options = rune::FileDialogOptions {
                title: "Choose a parent folder for the restored library".into(),
                ..Default::default()
            };
            cx.open_folder(options, move |view, parent, _, _| {
                if let Some(parent) = parent
                    && let Err(error) = view.controller.restore_library(path.clone(), parent)
                {
                    view.controller.error = Some(error);
                }
            });
        });
    }
    pub(super) fn open_restored_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.controller.restored_library.clone() else {
            return;
        };
        self.switch_library(path, window, cx);
    }
    pub(super) fn switch_library_dialog(&mut self, _: &Window, cx: &mut Context<Self>) {
        let options = rune::FileDialogOptions {
            title: "Choose an existing Folio library folder".into(),
            ..Default::default()
        };
        cx.open_folder(options, |view, path, window, cx| {
            if let Some(path) = path {
                view.switch_library(path, window, cx);
            }
        });
    }
    fn switch_library(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if !path.join("notes.sqlite3").is_file() {
            self.controller.error = Some(
                "Choose a folder containing an existing Folio library (notes.sqlite3).".into(),
            );
            cx.notify();
            return;
        }
        if self.controller.has_background_work() {
            self.controller.status =
                "Finish background work before opening the restored library".into();
            cx.notify();
            return;
        }
        match self.controller.flush().and_then(|_| Controller::open(path)) {
            Ok(controller) => {
                self.controller = controller;
                self.controller.status = "Library opened for this session. Choose Use this library by default in Settings to keep it on restart.".into();
                self.open_tabs = self.controller.settings.workspace.open_tabs.clone();
                self.list_view = self.controller.settings.workspace.list_view;
                self.sort_by_name = self.controller.settings.workspace.sort_by_name;
                self.sort_reverse = self.controller.settings.workspace.sort_reverse;
                self.clear_math_inputs(window);
                self.region_selection = None;
                self.modal = None;
                self.settings_open = false;
                self.pages_open = self.controller.settings.workspace.pages_open;
                self.library_open = true;
                self.canvas_bounds = None;
                self.controller.filter = NoteFilter::All;
                self.focus.focus(window);
            }
            Err(e) => self.controller.error = Some(e),
        }
        cx.notify();
    }
}
