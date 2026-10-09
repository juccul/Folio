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
    let math_config = std::env::var_os("FOLIO_MATH_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/math-solver/pack.json")
        });
    let math: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&math_config).unwrap()).unwrap();
    let python = PathBuf::from(math["python"].as_str().unwrap());
    let python = if python.is_absolute() {
        python
    } else {
        math_config.parent().unwrap().join(python)
    };
    std::fs::write(
        pack.join("pack.json"),
        serde_json::to_vec(&serde_json::json!({"python":python,"worker":"worker.py"})).unwrap(),
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

#[test]
fn reviewed_ink_index_keeps_raw_strokes_searches_unicode_and_tracks_undo_and_reopen() {
    let (mut app, root) = fixture();
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    let old = app.page().objects[&id].clone();
    review(&mut app);
    app.keep_ink_and_index("Café tomorrow".into()).unwrap();
    assert_eq!(app.page().objects[&id], old);
    assert_eq!(app.page().ink_text.len(), 1);
    let bounds = app.page().ink_text[0].bounds;
    app.transform_selection(Transform::translate(100., 50.), "Move writing");
    assert_eq!(app.page().ink_text[0].text, "Café tomorrow");
    assert!((app.page().ink_text[0].bounds.min.x - bounds.min.x - 100.).abs() < 0.001);
    app.flush().unwrap();
    app.search("cafe tom".into());
    let start = Instant::now();
    while app.has_background_work() {
        app.tick();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(app.search_results.len(), 1);
    let note = app.active;
    let page = app.page().id;
    app.navigate_search(note, page);
    assert_eq!(app.search_highlights, vec![app.page().ink_text[0].bounds]);
    app.session_mut().selection = HashSet::from([id]);
    app.delete_selection();
    assert_eq!(app.page().ink_text[0].text, "Café tomorrow");
    assert!(app.page().ink_text[0].stale);
    assert!(app.page().ink_text[0].sources.is_empty());
    app.session().document.validate().unwrap();
    app.undo();
    assert!(!app.page().ink_text[0].stale);
    assert_eq!(app.page().ink_text[0].text, "Café tomorrow");
    app.clear_handwriting_index();
    assert!(app.page().ink_text.is_empty());
    app.undo();
    app.flush().unwrap();
    let store = Store::open_reader(&app.database).unwrap();
    assert_eq!(
        store
            .connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        6
    );
    drop(store);
    drop(app);
    let mut app = Controller::open(root.clone()).unwrap();
    app.switch_note(note);
    assert_eq!(app.page().ink_text[0].text, "Café tomorrow");
    app.duplicate_page();
    assert_eq!(app.page().ink_text.len(), 1);
    assert_ne!(app.page().ink_text[0].sources[0], id);
    app.session().document.validate().unwrap();
    let cover = app.page().id;
    app.metadata(|metadata| metadata.cover_page = Some(cover));
    app.duplicate_note();
    assert_ne!(app.active, note);
    app.session().document.validate().unwrap();
    assert!(
        app.session()
            .document
            .pages
            .iter()
            .all(|p| p.ink_text[0].sources[0] != id)
    );
    assert_ne!(app.session().document.metadata.cover_page, Some(cover));
    clean(app, root);
}
#[test]
fn changed_or_overwritten_ink_cannot_keep_stale_index_and_undo_restores_reviewed_text() {
    let (mut app, root) = fixture();
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    review(&mut app);
    app.transform_selection(Transform::translate(20., 0.), "Move");
    assert!(app.keep_ink_and_index("stale".into()).is_err());
    app.session_mut().selection = HashSet::from([id]);
    review(&mut app);
    app.keep_ink_and_index("original".into()).unwrap();
    draw(&mut app, 40.);
    assert_eq!(app.page().ink_text[0].text, "original");
    assert!(app.page().ink_text[0].stale);
    app.undo();
    assert_eq!(app.page().ink_text[0].text, "original");
    assert!(!app.page().ink_text[0].stale);
    app.redo();
    assert_eq!(app.page().ink_text[0].text, "original");
    assert!(app.page().ink_text[0].stale);
    clean(app, root);
}

#[test]
fn salvage_keeps_valid_reviewed_ink_and_skips_annotations_with_missing_sources() {
    let (mut app, root) = fixture();
    let ink = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([ink]);
    review(&mut app);
    app.keep_ink_and_index("Recovered handwriting".into())
        .unwrap();
    app.add_text("Corrupt neighbor".into(), Point::new(300., 50.));
    let neighbor = *app.page().order.last().unwrap();
    let note = app.active;
    let page = app.page().id;
    app.flush().unwrap();
    drop(app);
    // Force object-by-object salvage, rather than restoring a healthy snapshot.
    for snapshot in folio_storage::recovery::snapshots(&root).unwrap() {
        std::fs::remove_file(snapshot).unwrap();
    }
    let store = Store::open(root.join("notes.sqlite3")).unwrap();
    store
        .connection
        .execute(
            "UPDATE objects SET data='corrupt' WHERE id=?1",
            [neighbor.to_string()],
        )
        .unwrap();
    let raw: String = store
        .connection
        .query_row(
            "SELECT header FROM pages WHERE id=?1",
            [page.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let mut header: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut invalid = header["ink_text"][0].clone();
    invalid["sources"] = serde_json::json!([Id::new_v4()]);
    header["ink_text"].as_array_mut().unwrap().push(invalid);
    store
        .connection
        .execute(
            "UPDATE pages SET header=?1 WHERE id=?2",
            [header.to_string(), page.to_string()],
        )
        .unwrap();
    drop(store);
    let report = folio_storage::recovery::recover(&root).unwrap();
    assert!(report.snapshot.is_none());
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("Search annotation"))
    );
    let mut recovered = Controller::open(report.destination.clone()).unwrap();
    recovered.switch_note(note);
    assert!(recovered.page().objects.contains_key(&ink));
    assert!(!recovered.page().objects.contains_key(&neighbor));
    assert_eq!(recovered.page().ink_text.len(), 1);
    assert_eq!(recovered.page().ink_text[0].text, "Recovered handwriting");
    recovered.search("Recovered handwriting".into());
    wait_workers(&mut recovered);
    assert_eq!(recovered.search_results.len(), 1);
    clean(recovered, report.destination);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn first_ocr_request_waits_for_setup_choice_without_dispatching_downloads() {
    let (mut app, root) = fixture();
    let id = draw(&mut app, 20.);
    app.session_mut().selection = HashSet::from([id]);
    let review = app.recognition_snapshot(RecognitionKind::Text).unwrap();
    let pack = root.join("recognition/pack.json");
    app.recognition_service
        .submit(
            Task {
                generation: 1,
                pack: pack.clone(),
                review,
                image: None,
                auto_setup: true,
            },
            false,
        )
        .unwrap();
    assert!(app.recognition_setup_needed());
    assert!(app.recognition_service.last_task.is_none());
    std::thread::sleep(Duration::from_millis(30));
    assert!(!pack.exists());
    assert!(app.recognition_service.results.try_recv().is_err());
    app.cancel_recognition();
    assert!(!app.recognition_setup_needed());
    assert!(!app.settings.ocr_download_allowed);
    clean(app, root);
}
