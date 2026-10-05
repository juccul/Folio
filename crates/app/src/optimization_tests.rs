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
