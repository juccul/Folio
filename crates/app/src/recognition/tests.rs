use super::*;
use folio_input::Device;

fn fixture() -> (Controller, PathBuf) {
    let root = std::env::temp_dir().join(format!("folio-recognition-{}", Id::new_v4()));
    let app = Controller::open(root.clone()).unwrap();
    (app, root)
}
fn draw(app: &mut Controller, x: f32) -> Id {
    for (i, phase) in [Phase::Down, Phase::Move, Phase::Up]
        .into_iter()
        .enumerate()
    {
        app.pointer(PenEvent {
            device: Device::Tablet,
            tool: PenTool::Pen,
            phase,
            position: app
                .session()
                .viewport
                .to_screen(Point::new(x + i as f32 * 10., 50. + i as f32 * 12.)),
            pressure: 0.7,
            tilt_x: 0.,
            tilt_y: 0.,
            buttons: 0,
            timestamp: i as u64 * 16,
        });
    }
    *app.page().order.last().unwrap()
}
fn review(app: &mut Controller) -> RecognitionReview {
    let mut review = app.recognition_snapshot(RecognitionKind::Text).unwrap();
    review.text = "original suggestion".into();
    app.recognition_review = Some(review.clone());
    review
}
fn clean(mut app: Controller, root: PathBuf) {
    app.flush().unwrap();
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reviewed_replacement_preserves_neighbors_order_raw_undo_and_durable_history() {
    let (mut app, root) = fixture();
    let first = draw(&mut app, 20.);
    app.add_text("untouched neighbor".into(), Point::new(300., 50.));
    let neighbor = app.page().order[1];
    let second = draw(&mut app, 60.);
    let old_order = app.page().order.clone();
    let originals = app.page().objects.clone();
    app.session_mut().selection = HashSet::from([first, second, neighbor]);
    let suggestion = review(&mut app);
    // Reviewing/copying text is read-only, and mixed selections capture ink only.
    assert_eq!(suggestion.sources.len(), 2);
    assert_eq!(app.page().objects, originals);
    app.replace_recognized_writing("Corrected café\n第二行".into())
        .unwrap();
    let text = *app.session().selection.iter().next().unwrap();
    assert_eq!(app.page().order, vec![text, neighbor]);
    assert_eq!(app.page().objects[&neighbor], originals[&neighbor]);
    assert_eq!(
        app.page().objects[&text].searchable_text(),
        "Corrected café\n第二行"
    );
    let replacement = app.page().objects[&text].clone();
    app.undo();
    assert_eq!(app.page().order, old_order);
    assert_eq!(app.page().objects, originals);
    app.redo();
    app.flush().unwrap();
    drop(app);
    let mut app = Controller::open(root.clone()).unwrap();
    assert_eq!(app.page().objects[&text], replacement);
    app.undo();
    assert_eq!(app.page().objects, originals);
    assert_eq!(app.page().order, old_order);
    app.redo();
    assert_eq!(app.page().objects[&text], replacement);
    clean(app, root);
}

fn math_review(app: &mut Controller) {
    let mut suggestion = app.recognition_snapshot(RecognitionKind::Math).unwrap();
    suggestion.text = r"x^2 + \frac{1}{2}".into();
    app.recognition_review = Some(suggestion);
}

fn wait_workers(app: &mut Controller) {
    let start = Instant::now();
    while app.busy > 0 {
        app.tick();
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn math_replacement_renders_corrected_latex_atomically_and_survives_undo_reopen() {
    let (mut app, root) = fixture();
    let first = draw(&mut app, 20.);
    app.add_text("untouched neighbor".into(), Point::new(300., 50.));
    let neighbor = app.page().order[1];
    let second = draw(&mut app, 60.);
    let original_order = app.page().order.clone();
    let originals = app.page().objects.clone();
    app.session_mut().selection = HashSet::from([first, second, neighbor]);
    math_review(&mut app);
    let latex = r"\frac{x-y}{\sqrt{2}}";
    app.replace_recognized_writing(latex.into()).unwrap();
    assert!(app.recognition_pending && app.recognition_replacing);
    assert_eq!(app.page().objects, originals);
    assert!(app.replace_recognized_writing("other".into()).is_err());
    wait_workers(&mut app);
    assert!(!app.recognition_pending && !app.recognition_replacing);
    assert!(app.recognition_review.is_none());
    let id = *app.session().selection.iter().next().unwrap();
    let replacement = app.page().objects[&id].clone();
    let Object::Equation(equation) = replacement.as_ref() else {
        panic!("Math OCR must insert a rendered equation");
    };
    assert_eq!(equation.latex, latex);
    assert!(equation.rendered_svg.as_ref().unwrap().contains("<path"));
    assert!(equation.source_strokes.is_empty());
    assert_eq!(app.page().order, vec![id, neighbor]);
    assert_eq!(app.page().objects[&neighbor], originals[&neighbor]);
    app.undo();
    assert_eq!(app.page().objects, originals);
    assert_eq!(app.page().order, original_order);
    app.redo();
    app.flush().unwrap();
    drop(app);
    let mut app = Controller::open(root.clone()).unwrap();
    assert_eq!(app.page().objects[&id], replacement);
    app.undo();
    assert_eq!(app.page().objects, originals);
    app.redo();
    app.edit_equation(id, "E=mc^2".into());
    wait_workers(&mut app);
    assert_eq!(app.page().objects[&id].searchable_text(), "E=mc^2");
    app.undo();
    assert_eq!(app.page().objects[&id], replacement);
    clean(app, root);
}

#[test]
fn invalid_math_preserves_ink_and_returns_the_corrected_review() {
    let (mut app, root) = fixture();
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    math_review(&mut app);
    let originals = app.page().objects.clone();
    assert!(
        app.replace_recognized_writing(r"\input{file}".into())
            .is_err()
    );
    assert!(!app.recognition_pending);
    assert_eq!(app.page().objects, originals);
    app.replace_recognized_writing(r"\frac{".into()).unwrap();
    wait_workers(&mut app);
    assert_eq!(app.page().objects, originals);
    assert!(app.error.as_ref().unwrap().contains("Equation rendering"));
    assert_eq!(app.recognition_review.as_ref().unwrap().text, r"\frac{");
    assert!(!app.recognition_pending && !app.recognition_replacing);
    clean(app, root);
}

#[test]
fn moved_offpage_and_cancelled_math_render_cannot_replace_the_source_ink() {
    let (mut app, root) = fixture();
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    math_review(&mut app);
    app.replace_recognized_writing("x^2".into()).unwrap();
    app.transform_selection(Transform::translate(10., 0.), "Move while rendering");
    let moved = app.page().objects.clone();
    wait_workers(&mut app);
    assert_eq!(app.page().objects, moved);
    assert!(app.error.as_ref().unwrap().contains("Writing changed"));
    math_review(&mut app);
    app.replace_recognized_writing("x^2".into()).unwrap();
    app.cancel_recognition();
    wait_workers(&mut app);
    assert_eq!(app.page().objects, moved);
    assert!(app.recognition_review.is_none());
    math_review(&mut app);
    app.replace_recognized_writing("x^2".into()).unwrap();
    app.add_page();
    wait_workers(&mut app);
    assert!(app.page().objects.is_empty());
    assert_eq!(app.session().document.pages[0].objects, moved);
    clean(app, root);
}

#[test]
fn changed_deleted_and_offpage_sources_reject_replacement_without_mutation() {
    let (mut app, root) = fixture();
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    let original = review(&mut app);
    app.transform_selection(Transform::translate(10., 0.), "Move ink");
    let modified = app.page().objects.clone();
    assert!(app.replace_recognized_writing("hello".into()).is_err());
    assert_eq!(app.page().objects, modified);
    app.undo();
    app.recognition_review = Some(original.clone());
    app.add_page();
    assert!(app.replace_recognized_writing("hello".into()).is_err());
    app.session_mut().page = 0;
    app.session_mut().selection = HashSet::from([id]);
    app.delete_selection();
    app.recognition_review = Some(original);
    assert!(app.replace_recognized_writing("hello".into()).is_err());
    assert!(app.page().objects.is_empty());
    clean(app, root);
}

fn fake_pack(root: &std::path::Path, delay: f32) {
    let pack = root.join("recognition");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(pack.join("worker.py"), format!(
        "import sys,json,time\nfor line in sys.stdin:\n request=json.loads(line)\n time.sleep({delay})\n print(json.dumps({{'text':'recognized locally'}}),flush=True)\n"
    )).unwrap();
    std::fs::write(
        pack.join("pack.json"),
        r#"{"python":"/usr/bin/python3","worker":"worker.py"}"#,
    )
    .unwrap();
}
fn wait_result(app: &mut Controller) {
    let start = Instant::now();
    while app.recognition_pending {
        app.tick();
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn asynchronous_results_remain_suggestions_and_stale_results_are_rejected() {
    let (mut app, root) = fixture();
    fake_pack(&root, 0.05);
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    let before = app.page().objects.clone();
    app.recognize_selection(RecognitionKind::Text).unwrap();
    wait_result(&mut app);
    assert_eq!(
        app.recognition_review.as_ref().unwrap().text,
        "recognized locally"
    );
    assert_eq!(app.page().objects, before);
    app.recognize_selection(RecognitionKind::Text).unwrap();
    app.transform_selection(Transform::translate(10., 0.), "Move during recognition");
    wait_result(&mut app);
    assert!(app.recognition_review.is_none());
    assert!(app.error.as_ref().unwrap().contains("Writing changed"));
    clean(app, root);
}

#[test]
fn cancel_kills_worker_and_late_result_cannot_reopen_review() {
    let (mut app, root) = fixture();
    fake_pack(&root, 5.);
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    app.recognize_selection(RecognitionKind::Text).unwrap();
    let start = Instant::now();
    while app.recognition_service.process.lock().unwrap().is_none() {
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(5));
    }
    app.cancel_recognition();
    assert!(!app.recognition_pending);
    assert!(app.recognition_service.process.lock().unwrap().is_none());
    std::thread::sleep(Duration::from_millis(50));
    app.tick();
    assert!(app.recognition_review.is_none());
    assert_eq!(app.page().objects.len(), 1);
    clean(app, root);
}
