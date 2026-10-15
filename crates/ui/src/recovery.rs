use gpui::{prelude::*, *};
use std::path::PathBuf;
/// A failed database open still presents a usable native recovery window.
pub struct RecoveryView {
    root: PathBuf,
    message: String,
    running: bool,
    focus: FocusHandle,
}
impl RecoveryView {
    pub fn new(root: PathBuf, error: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        Self {
            root,
            message: error,
            running: false,
            focus,
        }
    }
    fn recover(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let root = self.root.clone();
        self.running = true;
        self.message = "Checking snapshots and readable note data…".into();
        let task = cx.background_executor().spawn(async move {
            folio_storage::recovery::recover(&root).map_err(|e| e.to_string())
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.running = false;
                match result {
                    Ok(report) => {
                        let launch = std::env::current_exe().and_then(|exe|
                            std::process::Command::new(exe).arg("--data-dir").arg(&report.destination).spawn());
                        match launch {
                            Ok(_) => cx.quit(),
                            Err(e) => view.message = format!("Recovered {} notes into {}. Launch Folio with --data-dir pointing there. {e}", report.recovered_notes, report.destination.display()),
                        }
                    }
                    Err(e) => view.message = e,
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
impl Render for RecoveryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0xffffff)).text_color(rgb(0x0a0a0a)).flex().items_center().justify_center()
            .child(div().w(px(620.)).p_8().flex().flex_col().gap_4().child(div().text_2xl().child("Recover your notes"))
                .child(self.message.clone()).child(format!("Data directory: {}",self.root.display()))
                .child("Recovery creates a separate data folder using the newest healthy snapshot or readable note records. The original files remain available.")
                .child(div().id("recover").track_focus(&self.focus).tab_stop(true).p_3().rounded_md().bg(rgb(0x171717)).text_color(rgb(0xffffff)).cursor_pointer().child(if self.running{"Recovering…"} else {"Recover into a new folder"})
                    .on_click(cx.listener(|this,_,_,cx|this.recover(cx)))
                    .on_key_down(cx.listener(|this,e:&KeyDownEvent,_,cx|{if e.keystroke.key=="enter" {this.recover(cx);}}))))
    }
}
