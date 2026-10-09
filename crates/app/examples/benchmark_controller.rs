//! CPU-only controller bookkeeping; generated documents and temporary storage.
use folio_app::Controller;
use folio_document::*;
use std::{collections::HashSet, sync::Arc, time::Instant};
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
        .unwrap_or(10_000)
        .clamp(100, 100_000);
    let root = std::env::temp_dir().join(format!("folio-controller-benchmark-{}", Id::new_v4()));
    let mut app = Controller::open(root.clone()).unwrap();
    app.set_autosave(false);
    let page = app.page().id;
    let changes = (0..count)
        .map(|index| {
            let object = Object::Stroke(InkStroke {
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
                id: object.id(),
                before: None,
                after: Some(Arc::new(object)),
                index,
            }
        })
        .collect();
    app.commit("Fixture", changes);
    let note = app.active;
    let cover = measure(
        || {
            std::hint::black_box(app.library_preview(note).unwrap());
        },
        200,
    );
    app.session_mut().selection = HashSet::from([app.page().order[count - 1]]);
    let single_selection = measure(
        || app.transform_selection(Transform::translate(1., 0.), "Move one"),
        200,
    );
    app.session_mut().selection.clear();
    let autosave_disabled_edit = measure(
        || app.add_text("Short text".into(), Point::new(0., 0.)),
        200,
    );
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "measurement":"CPU-only controller; generated fixture; no UI/GPU or physical input",
        "objects":count,"loaded_library_cover":cover,"single_object_transform_autosave_disabled":single_selection,
        "autosave_disabled_edit":autosave_disabled_edit
    })).unwrap());
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}
