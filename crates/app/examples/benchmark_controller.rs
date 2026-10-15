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
    distribution(times)
}
fn distribution(mut times: Vec<f64>) -> serde_json::Value {
    let repetitions = times.len();
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
    let recognition_eligibility = measure(
        || {
            std::hint::black_box(app.can_recognize_selection());
        },
        200,
    );
    let single_copy = measure(
        || {
            std::hint::black_box(app.copy_objects());
        },
        200,
    );
    let single_selection = measure(
        || app.transform_selection(Transform::translate(1., 0.), "Move one"),
        200,
    );
    app.session_mut().selection.clear();
    app.tool = folio_app::Tool::Text;
    let empty_text_click = measure(
        || {
            app.pointer(folio_input::PenEvent {
                device: folio_input::Device::Mouse,
                tool: folio_input::Tool::Pen,
                phase: folio_input::Phase::Down,
                position: Point::new(300., 300.),
                pressure: 0.5,
                tilt_x: 0.,
                tilt_y: 0.,
                buttons: 0,
                timestamp: 0,
            });
            app.cancel();
        },
        200,
    );
    let autosave_disabled_edit = measure(
        || app.add_text("Short text".into(), Point::new(0., 0.)),
        200,
    );
    let math_panel_open = measure(|| app.open_math_solver().unwrap(), 200);
    let mut document = app.session().document.clone();
    let deletion = Command {
        label: "Delete alternating objects".into(),
        changes: document.pages[0]
            .order
            .iter()
            .enumerate()
            .step_by(2)
            .map(|(index, id)| Change::Object {
                page,
                id: *id,
                before: Some(document.pages[0].objects[id].clone()),
                after: None,
                index,
            })
            .collect(),
    };
    let mut deletes = Vec::new();
    let mut restores = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        deletion.apply(&mut document, true);
        deletes.push(start.elapsed().as_secs_f64() * 1e6);
        let start = Instant::now();
        deletion.apply(&mut document, false);
        restores.push(start.elapsed().as_secs_f64() * 1e6);
    }
    assert_eq!(document.pages[0].order, app.page().order);
    assert_eq!(document.pages[0].objects, app.page().objects);
    let replacement = Command {
        label: "Replace alternating objects".into(),
        changes: deletion
            .changes
            .iter()
            .flat_map(|change| {
                let Change::Object {
                    page,
                    before,
                    index,
                    ..
                } = change
                else {
                    unreachable!()
                };
                let mut object = before.as_ref().unwrap().as_ref().clone();
                object.set_id(Id::new_v4());
                [
                    change.clone(),
                    Change::Object {
                        page: *page,
                        id: object.id(),
                        before: None,
                        after: Some(Arc::new(object)),
                        index: *index,
                    },
                ]
            })
            .collect(),
    };
    let mut replacements = Vec::new();
    let mut replacement_undos = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        replacement.apply(&mut document, true);
        replacements.push(start.elapsed().as_secs_f64() * 1e6);
        let start = Instant::now();
        replacement.apply(&mut document, false);
        replacement_undos.push(start.elapsed().as_secs_f64() * 1e6);
    }
    assert_eq!(document.pages[0].order, app.page().order);
    assert_eq!(document.pages[0].objects, app.page().objects);
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "measurement":"CPU-only controller; generated fixture; no UI/GPU or physical input",
        "objects":count,"loaded_library_cover":cover,"single_object_transform_autosave_disabled":single_selection,
        "single_object_copy":single_copy,"autosave_disabled_edit":autosave_disabled_edit,
        "empty_text_click":empty_text_click,
        "single_stroke_recognition_eligibility":recognition_eligibility,"single_text_math_panel_open":math_panel_open,
        "batch_objects":document.pages[0].objects.len(),"deleted_objects":deletion.changes.len(),
        "delete_alternating_objects":distribution(deletes),"undo_alternating_deletion":distribution(restores),
        "replace_alternating_objects":distribution(replacements),"undo_alternating_replacement":distribution(replacement_undos)
    })).unwrap());
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}
