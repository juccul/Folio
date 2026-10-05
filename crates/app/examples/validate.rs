//! End-to-end, headless validation using real storage and document workers.
use folio_app::{Controller, ExportKind};
use folio_document::*;
use folio_input::*;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn wait(app: &mut Controller) {
    let start = Instant::now();
    while app.has_background_work() {
        app.tick();
        assert!(start.elapsed() < Duration::from_secs(60), "worker timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(error) = app.error.take() {
        panic!("{error}");
    }
}
fn main() {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/validation".into()),
    );
    std::fs::create_dir_all(&root).unwrap();
    let mut app = Controller::open(root.join(format!("data-{}", Id::new_v4()))).unwrap();
    app.rename("Field notes".into());
    app.paper(Paper::Dots);
    app.add_text("A place to think\nMixed vector ink & typed text — café, 研究\nAn offline notebook for Linux.".into(),Point::new(64.,54.));
    for line in 0..4 {
        let sample = |i: u32, phase: Phase| PenEvent {
            device: Device::Tablet,
            tool: Tool::Pen,
            phase,
            position: Point::new(
                56. + 80. + i as f32 * 3.,
                36. + 230. + line as f32 * 54. + (i as f32 * 0.16).sin() * 16.,
            ),
            pressure: 0.2 + i as f32 / 140.,
            tilt_x: 20.,
            tilt_y: -8.,
            buttons: 0,
            timestamp: line * 1000 + i as u64 * 8,
        };
        app.pointer(sample(0, Phase::Down));
        for i in 1..100 {
            app.pointer(sample(i, Phase::Move));
        }
        app.pointer(sample(100, Phase::Up));
    }
    app.insert_equation(r"\int_0^1 x^2\,dx = \frac{1}{3}".into());
    wait(&mut app);
    app.add_page();
    app.add_text(
        "A second page\nPDF annotations stay separate from originals.".into(),
        Point::new(80., 80.),
    );
    app.change_page(0);
    app.flush().unwrap();
    for (kind, name) in [
        (ExportKind::Svg, "field-notes.svg"),
        (ExportKind::Png, "field-notes.png"),
        (ExportKind::Pdf, "field-notes.pdf"),
        (ExportKind::Text, "field-notes.txt"),
    ] {
        app.export(root.join(name), kind);
        wait(&mut app);
        assert!(std::fs::metadata(root.join(name)).unwrap().len() > 0);
    }
    app.search("cafe".into());
    wait(&mut app);
    assert_eq!(app.search_results.len(), 1);
    assert_eq!(app.search_results[0].page, app.page().id);
    let pages = app.session().document.pages.len();
    app.import(root.join("field-notes.pdf"));
    wait(&mut app);
    assert_eq!(app.session().document.pages.len(), pages + 2);
    assert!(app.page().properties.pdf.is_some());
    assert!((app.page().properties.width - 794.).abs() < 1.);
    assert!((app.page().properties.height - 1123.).abs() < 1.);
    app.flush().unwrap();
    let store = folio_storage::Store::open(&app.database).unwrap();
    let reloaded = store.load(app.active).unwrap().unwrap();
    reloaded.validate().unwrap();
    assert_eq!(reloaded.pages.len(), 4);
    println!(
        "VALIDATED: vector exports, Unicode text, FTS navigation, 2-page PDF import, durable reload\nArtifacts: {}",
        root.display()
    );
}
