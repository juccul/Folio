use super::*;
use std::path::PathBuf;
impl NotesView {
    pub(super) fn backup_dialog(&mut self, cx: &mut Context<Self>) {
        let suggested = format!("Folio-{}.foliobackup", folio_document::now_ms());
        let path = cx.prompt_for_new_path(&self.controller.data_dir, Some(&suggested));
        cx.spawn(async move |view, cx| match path.await {
            Ok(Ok(Some(path))) => {
                let _ = view.update(cx, |view, cx| {
                    if let Err(e) = view.controller.backup_library(path) {
                        view.controller.error = Some(e);
                    }
                    cx.notify();
                });
            }
            Ok(Err(error)) => {
                let _ = view.update(cx, |view, cx| {
                    view.controller.error = Some(error.to_string());
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }
    pub(super) fn restore_dialog(&mut self, cx: &mut Context<Self>) {
        let source = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a Folio library backup".into()),
        });
        cx.spawn(async move |view, cx| {
            let paths = match source.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Err(error)) => {
                    let _ = view.update(cx, |view, cx| {
                        view.controller.error = Some(error.to_string());
                        cx.notify();
                    });
                    return;
                }
                _ => return,
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let Ok(destination) = cx.update(|cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Choose a parent folder for the restored library".into()),
                })
            }) else {
                return;
            };
            match destination.await {
                Ok(Ok(Some(parents))) => {
                    if let Some(parent) = parents.into_iter().next() {
                        let _ = view.update(cx, |view, cx| {
                            if let Err(error) = view.controller.restore_library(path, parent) {
                                view.controller.error = Some(error);
                            }
                            cx.notify();
                        });
                    }
                }
                Ok(Err(error)) => {
                    let _ = view.update(cx, |view, cx| {
                        view.controller.error = Some(error.to_string());
                        cx.notify();
                    });
                }
                _ => {}
            }
        })
        .detach();
    }
    pub(super) fn open_restored_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.controller.restored_library.clone() else {
            return;
        };
        self.switch_library(path, window, cx);
    }
    pub(super) fn switch_library_dialog(&mut self, window: &Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose an existing Folio library folder".into()),
        });
        cx.spawn_in(window, async move |view, cx| match paths.await {
            Ok(Ok(Some(paths))) => {
                if let Some(path) = paths.into_iter().next() {
                    let _ = view
                        .update_in(cx, |view, window, cx| view.switch_library(path, window, cx));
                }
            }
            Ok(Err(e)) => {
                let _ = view.update(cx, |view, cx| {
                    view.controller.error = Some(e.to_string());
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
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
                self.open_tabs.clear();
                self.math_inputs = None;
                self.region_selection = None;
                self.modal = None;
                self.settings_open = false;
                self.pages_open = false;
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
