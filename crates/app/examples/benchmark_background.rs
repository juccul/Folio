//! CPU-only background bookkeeping and database-open measurements.
use folio_app::Controller;
use folio_document::*;
use folio_storage::{Delta, Store};
use std::{sync::Arc, time::Instant};
fn measure(mut action: impl FnMut(), repetitions: usize) -> serde_json::Value {
    let mut times = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        let start = Instant::now();
        action();
        times.push(start.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(f64::total_cmp);
    serde_json::json!({"p50_us":times[times.len()/2],"p95_us":times[(times.len()-1)*95/100],"repetitions":repetitions})
}
fn main() {
    let count = std::env::args()
        .nth(1)
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(10000)
        .clamp(100, 100000);
    let root = std::env::temp_dir().join(format!("folio-background-benchmark-{}", Id::new_v4()));
    let mut app = Controller::open(root.clone()).unwrap();
    app.set_autosave(false);
    let page = app.page().id;
    let changes = (0..count)
        .map(|index| {
            let stroke = Object::Stroke(InkStroke {
                id: Id::new_v4(),
                raw: Arc::new(vec![StrokePoint::new(Point::new(0., 0.), 0.5, 0)]),
                path: Arc::new(vec![PathPoint {
                    position: Point::new(0., 0.),
                    radius: 2.,
                }]),
                style: PenStyle::default(),
                transform: Transform::translate(
                    (index % 100) as f32 * 20.,
                    (index / 100) as f32 * 20.,
                ),
                created_at: 0,
                fragment_path: None,
                refined_path: None,
                refinement_enabled: false,
            });
            Change::Object {
                page,
                id: stroke.id(),
                before: None,
                after: Some(Arc::new(stroke)),
                index,
            }
        })
        .collect();
    app.commit("Benchmark fixture", changes);
    app.flush().unwrap();
    let metadata_scan = measure(
        || {
            std::hint::black_box(app.math_variables().unwrap());
        },
        1000,
    );
    let full_open = measure(
        || {
            std::hint::black_box(Store::open(&app.database).unwrap());
        },
        25,
    );
    let reader_open = measure(
        || {
            std::hint::black_box(Store::open_reader(&app.database).unwrap());
        },
        25,
    );
    app.session_mut()
        .document
        .pages
        .extend((1..10_000).map(|_| Page::new()));
    let page_navigation = measure(|| app.synchronize_page_view(800., 600.), 1000);
    let mut document = Document::new("Many-page autosave");
    document.pages.extend((1..10_000).map(|_| Page::new()));
    let last = document.pages.last_mut().unwrap();
    let command = Command {
        label: "Edit objects on last page".into(),
        changes: app
            .page()
            .ordered_objects()
            .take(512)
            .enumerate()
            .map(|(index, object)| {
                last.order.push(object.id());
                last.objects.insert(object.id(), object.clone());
                Change::Object {
                    page: last.id,
                    id: object.id(),
                    before: Some(object.clone()),
                    after: Some(object.clone()),
                    index,
                }
            })
            .collect(),
    };
    let delta_build = measure(
        || {
            let delta = Delta::command(&document, &command);
            assert_eq!(delta.objects.len(), command.changes.len());
            assert!(delta.objects.iter().all(|write| write.value.is_some()));
            std::hint::black_box(delta);
        },
        100,
    );
    let early = Command {
        label: "Small edit on first pages".into(),
        changes: document.pages[..2]
            .iter_mut()
            .zip(app.page().ordered_objects())
            .map(|(page, source)| {
                let mut object = source.as_ref().clone();
                object.set_id(Id::new_v4());
                let object = Arc::new(object);
                page.order.push(object.id());
                page.objects.insert(object.id(), object.clone());
                Change::Object {
                    page: page.id,
                    id: object.id(),
                    before: Some(object.clone()),
                    after: Some(object),
                    index: 0,
                }
            })
            .collect(),
    };
    let small_delta = measure(
        || {
            std::hint::black_box(Delta::command(&document, &early));
        },
        100,
    );
    let small_apply = measure(|| early.apply(&mut document, true), 100);
    let many_page_apply = measure(|| command.apply(&mut document, true), 100);
    let mut store = Store::open(root.join("many-pages.sqlite3")).unwrap();
    store.save(&Delta::full(&document)).unwrap();
    let delta = Delta::command(&document, &early);
    let incremental_save = measure(|| store.save(&delta).unwrap(), 50);
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "measurement":"CPU-only synthetic fixture; excludes rendering, GPU, OCR inference and physical input",
        "objects":count,"math_variable_lookup":metadata_scan,"full_database_open":full_open,"background_reader_open":reader_open,
        "pages":10_000,"continuous_page_navigation":page_navigation,
        "last_page_changed_objects":command.changes.len(),"many_page_autosave_delta":delta_build,
        "small_early_page_delta":small_delta,"many_page_incremental_save":incremental_save,
        "small_early_page_command":small_apply,"many_page_command":many_page_apply
    })).unwrap());
    drop(app);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
