//! CPU-only background bookkeeping and database-open measurements.
use folio_app::Controller;
use folio_document::*;
use folio_storage::Store;
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
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "measurement":"CPU-only synthetic fixture; excludes rendering, GPU, OCR inference and physical input",
        "objects":count,"math_variable_lookup":metadata_scan,"full_database_open":full_open,"background_reader_open":reader_open
    })).unwrap());
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}
