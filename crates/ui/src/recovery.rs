//! A failed database open still presents a usable native recovery window.
use std::path::PathBuf;

/// The recovery window's state.
pub struct RecoveryView {
    root: PathBuf,
    message: String,
    running: bool,
    result: std::sync::Arc<
        std::sync::Mutex<Option<Result<folio_storage::recovery::RecoveryReport, String>>>,
    >,
}
impl RecoveryView {
    pub fn new(root: PathBuf, error: String) -> Self {
        Self {
            root,
            message: error,
            running: false,
            result: Default::default(),
        }
    }
    fn recover(&mut self) {
        if self.running {
            return;
        }
        let root = self.root.clone();
        self.running = true;
        self.message = "Checking snapshots and readable note data…".into();
        let result = self.result.clone();
        std::thread::spawn(move || {
            let report = folio_storage::recovery::recover(&root).map_err(|e| e.to_string());
            if let Ok(mut slot) = result.lock() {
                *slot = Some(report);
            }
        });
    }
}

fn poll(s: &mut RecoveryView, _: f64, tx: &mut rune::Tx) -> bool {
    let finished = s.result.lock().ok().and_then(|mut r| r.take());
    let Some(result) = finished else {
        if s.running {
            tx.wake_after(std::time::Duration::from_millis(100));
        }
        return false;
    };
    s.running = false;
    match result {
        Ok(report) => {
            let launch = std::env::current_exe().and_then(|exe| {
                std::process::Command::new(exe)
                    .arg("--data-dir")
                    .arg(&report.destination)
                    .spawn()
            });
            match launch {
                Ok(_) => tx.quit(),
                Err(e) => {
                    s.message = format!(
                        "Recovered {} notes into {}. Launch Folio with --data-dir pointing there. {e}",
                        report.recovered_notes,
                        report.destination.display()
                    )
                }
            }
        }
        Err(e) => s.message = e,
    }
    true
}

fn view(s: &RecoveryView, _: &mut rune::Cx<RecoveryView>) -> rune::El {
    use rune::{TextStyle, col, text};
    let ink = crate::rgb(0x0a0a0a).color();
    let body = |t: String| {
        text(t)
            .font(TextStyle::new(16., 400.).lh(26.))
            .color(ink)
            .wrap()
    };
    col()
        .w_full()
        .h_full()
        .center()
        .fill(crate::rgb(0xffffff).color())
        .child(
            col()
                .w(620.)
                .padding(32.)
                .gap(16.)
                .child(text("Recover your notes").font(TextStyle::new(24., 400.).lh(38.)).color(ink))
                .child(body(s.message.clone()))
                .child(body(format!("Data directory: {}", s.root.display())))
                .child(body("Recovery creates a separate data folder using the newest healthy snapshot or readable note records. The original files remain available.".into()))
                .child(
                    rune::el()
                        .key("recover")
                        .role("button")
                        .label("Recover into a new folder")
                        .focusable(true)
                        .padding(12.)
                        .radius(6.)
                        .fill(crate::rgb(0x171717).color())
                        .cursor(rune::Cursor::Pointer)
                        .on_press(|s: &mut RecoveryView| s.recover())
                        .child(
                            text(if s.running { "Recovering…" } else { "Recover into a new folder" })
                                .font(TextStyle::new(16., 400.))
                                .color(crate::rgb(0xffffff).color()),
                        ),
                ),
        )
}

fn no_init() -> RecoveryView {
    unreachable!("the recovery window builds its state in App::init")
}

/// Opens the recovery window for a library that failed to open.
pub fn run_recovery(root: PathBuf, error: String) {
    rune::set_app_id(crate::shell::APP_ID);
    let spec = rune::AppSpec::<RecoveryView> {
        poll: Some(poll),
        ..rune::AppSpec::new("Folio — Recovery", (900., 600.), no_init, view)
    };
    rune::App::new(spec)
        .init(move || RecoveryView::new(root, error))
        .run();
}
