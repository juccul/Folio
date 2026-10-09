use super::*;

fn fixture() -> (Controller, PathBuf) {
    let root = std::env::temp_dir().join(format!("folio-optimization-{}", Id::new_v4()));
    (Controller::open(root.clone()).unwrap(), root)
}
fn cleanup(mut app: Controller, root: PathBuf) {
    app.flush().unwrap();
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}
fn ranks(app: &Controller) {
    let expected = app
        .page()
        .order
        .iter()
        .enumerate()
        .map(|(rank, id)| (*id, rank))
        .collect::<HashMap<_, _>>();
    assert_eq!(app.session().order_positions, expected);
}
#[test]
fn mixed_middle_insert_and_move_keep_ranks_through_undo_redo() {
    let (mut app, root) = fixture();
    for i in 0..4 {
        app.add_text(format!("text {i}"), Point::new(0., i as f32 * 20.));
    }
    let original = app.page().order.clone();
    let before = app.page().objects[&original[0]].clone();
    let mut moved = before.as_ref().clone();
    moved.set_transform(Transform::translate(200., 0.));
    let mut inserted = before.as_ref().clone();
    let id = Id::new_v4();
    if let Object::Text(ref mut text) = inserted {
        text.id = id;
    } else {
        panic!();
    }
    let page = app.page().id;
    app.commit(
        "Move and insert",
        vec![
            Change::Object {
                page,
                id: original[0],
                before: Some(before),
                after: Some(Arc::new(moved)),
                index: 0,
            },
            Change::Object {
                page,
                id,
                before: None,
                after: Some(Arc::new(inserted)),
                index: 1,
            },
        ],
    );
    assert_eq!(
        app.page().order,
        [original[0], id, original[1], original[2], original[3]]
    );
    ranks(&app);
    app.undo();
    assert_eq!(app.page().order, original);
    ranks(&app);
    app.redo();
    ranks(&app);
    cleanup(app, root);
}
#[test]
fn preview_results_cannot_resurrect_edited_or_deleted_objects() {
    let (mut app, root) = fixture();
    app.add_text("before".into(), Point::new(10., 20.));
    let page = app.page().id;
    let note = app.active;
    let id = app.page().order[0];
    let old = app.page().objects[&id].clone();
    let preview = |object| RasterPreview {
        pixels_budget: 4,
        object,
        bounds: Rect::new(10., 20., 2., 2.),
        width: 2,
        height: 2,
        bgra: Arc::new(vec![0; 16]),
    };
    app.session_mut().selection.insert(id);
    app.transform_selection(Transform::translate(400., 0.), "Move text");
    assert!(!app.accept_preview(note, page, preview(old)));
    let current = app.page().objects[&id].clone();
    assert!(app.accept_preview(note, page, preview(current.clone())));
    app.delete_selection();
    assert!(!app.accept_preview(note, page, preview(current)));
    assert!(
        app.session()
            .index
            .query(Rect::new(-1000., -1000., 3000., 3000.))
            .is_empty()
    );
    cleanup(app, root);
}
#[test]
fn offscreen_page_preview_does_not_pollute_active_page_index() {
    let (mut app, root) = fixture();
    app.add_text("first page".into(), Point::new(10., 20.));
    let page = app.page().id;
    let object = app.page().objects[&app.page().order[0]].clone();
    let id = object.id();
    app.add_page();
    let note = app.active;
    assert!(app.accept_preview(
        note,
        page,
        RasterPreview {
            pixels_budget: 4,
            object,
            bounds: Rect::new(10., 20., 2., 2.),
            width: 2,
            height: 2,
            bgra: Arc::new(vec![0; 16])
        }
    ));
    assert!(
        !app.session()
            .index
            .query(Rect::new(0., 0., 100., 100.))
            .contains(&id)
    );
    assert!(app.previews.contains_key(&(note, page, id)));
    cleanup(app, root);
}
#[test]
fn repeated_note_load_requests_are_coalesced() {
    let (mut app, root) = fixture();
    app.create_note(); // Two explicitly created documents are needed for a background load.
    let first = app.active;
    app.create_note();
    app.flush().unwrap();
    app.sessions.remove(&first);
    assert!(app.queue_load(first));
    let busy = app.busy;
    for _ in 0..100 {
        assert!(app.queue_load(first));
    }
    assert_eq!(app.busy, busy);
    assert!(app.loading_notes.contains(&first));
    let start = Instant::now();
    while app.has_background_work() {
        app.tick();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.loading_notes.contains(&first));
    assert!(app.sessions.contains_key(&first));
    cleanup(app, root);
}

#[test]
fn late_save_receipt_cannot_hide_new_unsaved_edits() {
    let (mut app, root) = fixture();
    app.add_text("Saved earlier".into(), Point::new(0., 0.));
    app.persistence.flush().unwrap(); // Leave the successful receipt unpolled.
    app.set_autosave(false);
    app.add_text("Still unsaved".into(), Point::new(0., 200.));
    assert!(app.status.contains("Unsaved"));
    app.tick();
    assert!(app.status.contains("Unsaved"), "{}", app.status);
    cleanup(app, root);
}

#[test]
fn export_snapshot_keeps_note_page_and_content_after_navigation() {
    let (mut app, root) = fixture();
    app.add_text("Original export".into(), Point::new(0., 0.));
    let snapshot = app.prepare_export();
    app.add_page();
    app.add_text("Different page".into(), Point::new(0., 0.));
    app.create_note();
    app.add_text("Different document".into(), Point::new(0., 0.));
    let path = root.join("snapshot.txt");
    app.export_prepared(snapshot, path.clone(), ExportKind::Text);
    let start = Instant::now();
    while app.has_background_work() {
        app.tick();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), "Original export");
    cleanup(app, root);
}

#[test]
fn linked_math_index_tracks_edits_undo_redo_and_page_changes() {
    let (mut app, root) = fixture();
    app.add_text("Ordinary text".into(), Point::new(0., 0.));
    assert!(app.session().linked_math.is_empty());
    let equation = Object::Equation(Equation {
        id: Id::new_v4(),
        latex: "a=5".into(),
        rendered_svg: None,
        rect: Rect::new(0., 0., 180., 64.),
        source_strokes: vec![],
        transform: Transform::default(),
        math_link: Some(MathLink {
            last_error: None,
            expression: "a:=5".into(),
            operation: "assign".into(),
            variable: "x".into(),
            domain: "real".into(),
            angle: "radians".into(),
            method: String::new(),
            x_min: -10.,
            x_max: 10.,
            live: false,
            sources: vec![],
            ink_region: None,
        }),
    });
    let id = equation.id();
    app.commit(
        "Definition",
        vec![Change::Object {
            page: app.page().id,
            id,
            before: None,
            after: Some(Arc::new(equation)),
            index: app.page().order.len(),
        }],
    );
    assert_eq!(app.session().linked_math, HashSet::from([id]));
    assert_eq!(app.math_variables().unwrap()["a"], "5");
    app.undo();
    assert!(app.session().linked_math.is_empty());
    assert!(app.math_variables().unwrap().is_empty());
    app.redo();
    app.add_page();
    assert!(app.session().linked_math.is_empty());
    app.change_page(0);
    assert_eq!(app.session().linked_math, HashSet::from([id]));
    app.session_mut().selection = HashSet::from([id]);
    app.delete_selection();
    assert!(app.session().linked_math.is_empty());
    app.undo();
    assert_eq!(app.session().linked_math, HashSet::from([id]));
    cleanup(app, root);
}

#[test]
fn a_failed_close_flush_is_tracked_and_retried() {
    let (mut app, root) = fixture();
    app.add_text("Durable".into(), Point::new(0., 0.));
    app.flush().unwrap();
    app.tick();
    let injector = Store::open(&app.database).unwrap();
    injector.connection.execute_batch("CREATE TRIGGER reject_note_write BEFORE UPDATE ON notes BEGIN SELECT RAISE(FAIL,'simulated storage failure'); END;").unwrap();
    assert!(app.flush().is_err());
    app.tick();
    assert!(
        app.dirty_notes.contains(&app.active),
        "Failed full flush did not schedule recovery"
    );
    injector
        .connection
        .execute_batch("DROP TRIGGER reject_note_write;")
        .unwrap();
    drop(injector);
    app.last_retry = Instant::now() - Duration::from_secs(3);
    let start = Instant::now();
    while !app.dirty_notes.is_empty() || app.saved < app.queued {
        app.tick();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        Store::open_reader(&app.database)
            .unwrap()
            .load(app.active)
            .unwrap()
            .unwrap(),
        app.session().document
    );
    cleanup(app, root);
}

#[test]
fn loaded_library_cover_reuses_snapshot_and_invalidates_after_commands() {
    let (mut app, root) = fixture();
    app.add_text("original".into(), Point::new(0., 0.));
    let note = app.active;
    let first = app.library_preview(note).unwrap().0;
    let again = app.library_preview(note).unwrap().0;
    assert!(Arc::ptr_eq(&first, &again));
    let id = app.page().order[0];
    app.edit_text(id, |text| text.text = "edited".into());
    let edited = app.library_preview(note).unwrap().0;
    assert!(!Arc::ptr_eq(&first, &edited));
    assert!(edited.text().contains("edited"));
    assert!(first.text().contains("original"));
    app.undo();
    let undone = app.library_preview(note).unwrap().0;
    assert!(!Arc::ptr_eq(&edited, &undone));
    assert!(undone.text().contains("original"));
    let before = app.page().clone();
    let mut after = before.clone();
    after.properties.paper = Paper::Grid;
    app.commit(
        "Replace page snapshot",
        vec![Change::Page {
            index: 0,
            before: Some(before),
            after: Some(after),
        }],
    );
    let replaced = app.library_preview(note).unwrap().0;
    assert!(!Arc::ptr_eq(&undone, &replaced));
    assert_eq!(replaced.properties.paper, Paper::Grid);
    app.add_page();
    app.add_text("second page".into(), Point::new(0., 0.));
    app.use_page_as_cover();
    let (cover, pages) = app.library_preview(note).unwrap();
    assert_eq!(pages, 2);
    assert_eq!(cover.id, app.page().id);
    assert!(cover.text().contains("second page"));
    app.delete_page();
    assert_eq!(app.library_preview(note).unwrap().0.id, first.id);
    cleanup(app, root);
}

#[test]
fn deferred_history_saves_current_execute_undo_redo_state() {
    let (mut app, root) = fixture();
    app.set_autosave(false);
    for i in 0..20 {
        app.add_text(format!("entry {i}"), Point::new(0., i as f32 * 200.));
    }
    app.undo();
    app.undo();
    app.redo();
    assert!(app.session().history_needs_save);
    app.save();
    assert!(!app.session().history_needs_save);
    app.persistence.flush().unwrap();
    let reader = Store::open_reader(&app.database).unwrap();
    assert_eq!(
        reader.load(app.active).unwrap().unwrap(),
        app.session().document
    );
    assert_eq!(
        serde_json::to_value(reader.history(app.active).unwrap()).unwrap(),
        serde_json::to_value(&app.session().history).unwrap()
    );
    drop(reader);
    app.undo();
    assert!(app.session().history_needs_save);
    app.set_autosave(true);
    assert!(!app.session().history_needs_save);
    app.persistence.flush().unwrap();
    let reader = Store::open_reader(&app.database).unwrap();
    assert_eq!(
        reader.load(app.active).unwrap().unwrap(),
        app.session().document
    );
    assert_eq!(
        serde_json::to_value(reader.history(app.active).unwrap()).unwrap(),
        serde_json::to_value(&app.session().history).unwrap()
    );
    drop(reader);
    cleanup(app, root);
}

#[test]
fn sparse_selection_commands_follow_page_order_and_ignore_unknown_ids() {
    let (mut app, root) = fixture();
    for i in 0..6 {
        app.add_text(format!("entry {i}"), Point::new(0., i as f32 * 200.));
    }
    let order = app.page().order.clone();
    app.session_mut().selection = HashSet::from([order[5], order[1], Id::new_v4()]);
    let changes = app.object_changes(&app.session().selection, |_| None);
    assert_eq!(changes.len(), 2);
    assert!(matches!(&changes[0], Change::Object { id, index: 1, .. } if *id == order[1]));
    assert!(matches!(&changes[1], Change::Object { id, index: 5, .. } if *id == order[5]));
    app.delete_selection();
    assert_eq!(
        app.page().order,
        vec![order[0], order[2], order[3], order[4]]
    );
    app.undo();
    assert_eq!(app.page().order, order);
    ranks(&app);
    cleanup(app, root);
}

#[test]
fn indexed_handwriting_replacement_keeps_disjoint_annotations() {
    let (mut app, root) = fixture();
    let mut sources = Vec::new();
    for x in [10., 500.] {
        let mut builder = StrokeBuilder::new(PenStyle::default());
        builder.push(StrokePoint::new(Point::new(x, 10.), 0.5, 0));
        builder.push(StrokePoint::new(Point::new(x + 20., 20.), 0.5, 16));
        let object = Arc::new(Object::Stroke(builder.finish().unwrap()));
        sources.push(object.clone());
        app.commit(
            "Ink",
            vec![Change::Object {
                page: app.page().id,
                id: object.id(),
                before: None,
                after: Some(object),
                index: app.page().order.len(),
            }],
        );
    }
    for (i, object) in sources.iter().enumerate() {
        app.recognition_review = Some(RecognitionReview {
            kind: RecognitionKind::Text,
            text: String::new(),
            note: app.active,
            page: app.page().id,
            bounds: object.bounds(),
            sources: vec![object.clone()],
            pdf_source: None,
        });
        app.keep_ink_and_index(format!("entry {i}")).unwrap();
    }
    let object = sources[0].clone();
    app.recognition_review = Some(RecognitionReview {
        kind: RecognitionKind::Text,
        text: String::new(),
        note: app.active,
        page: app.page().id,
        bounds: object.bounds(),
        sources: vec![object],
        pdf_source: None,
    });
    app.keep_ink_and_index("replacement".into()).unwrap();
    assert_eq!(app.page().ink_text.len(), 2);
    assert!(app.page().text().contains("replacement"));
    assert!(app.page().text().contains("entry 1"));
    assert!(!app.page().text().contains("entry 0"));
    app.undo();
    assert!(app.page().text().contains("entry 0"));
    cleanup(app, root);
}
