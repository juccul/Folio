use super::*;
use folio_input::{Device, Tool as PenTool};
fn app() -> Controller {
    Controller::open(std::env::temp_dir().join(format!("folio-app-{}", Id::new_v4()))).unwrap()
}
fn event(x: f32, y: f32, phase: Phase) -> PenEvent {
    PenEvent {
        device: Device::Tablet,
        tool: PenTool::Pen,
        phase,
        position: Point::new(x + 56., y + 36.),
        pressure: 0.75,
        tilt_x: 14.,
        tilt_y: -8.,
        buttons: 0,
        timestamp: (x * 8.) as u64,
    }
}
fn draw(a: &mut Controller) {
    a.pointer(event(20., 30., Phase::Down));
    for x in 21..100 {
        a.pointer(event(x as f32, 30., Phase::Move));
    }
    a.pointer(event(100., 30., Phase::Up));
}
#[test]
fn draw_edit_undo_save_reload() {
    let mut a = app();
    draw(&mut a);
    let s = match a.page().ordered_objects().next().unwrap().as_ref() {
        Object::Stroke(s) => s.clone(),
        _ => panic!(),
    };
    assert_eq!(s.raw[0].tilt_x, 14.);
    a.set_tool(Tool::Eraser);
    a.select_all();
    assert_eq!(a.tool, Tool::Lasso);
    a.clear_selection();
    assert_eq!(a.tool, Tool::Eraser); // Select All borrowed the lasso.
    a.select_all();
    a.transform_selection(Transform::translate(10., 40.), "move");
    assert_eq!(a.page().objects[&s.id].transform().tx, 10.);
    a.undo();
    assert_eq!(a.page().objects[&s.id].transform(), Transform::default());
    a.redo();
    a.flush().unwrap();
    let id = a.active;
    let dir = a.data_dir.clone();
    drop(a);
    let a = Controller::open(dir).unwrap();
    assert_eq!(a.active, id);
    assert_eq!(a.page().objects[&s.id].transform().ty, 40.);
}
#[test]
fn erasing_is_one_command_and_cancel_is_safe() {
    let mut a = app();
    draw(&mut a);
    a.set_tool(Tool::Eraser);
    a.pointer(event(50., 30., Phase::Down));
    assert!(matches!(&a.interaction,Some(Interaction::Erase {ids,..})if !ids.is_empty()));
    a.cancel();
    assert_eq!(a.page().objects.len(), 1);
    a.pointer(event(50., 30., Phase::Down));
    a.pointer(event(50., 30., Phase::Up));
    assert!(a.page().objects.is_empty());
    a.undo();
    assert_eq!(a.page().objects.len(), 1);
}
#[test]
fn page_operations_and_note_ids() {
    let mut a = app();
    let first = a.active;
    a.add_page();
    assert_eq!(a.session().document.pages.len(), 2);
    a.delete_page();
    assert_eq!(a.session().document.pages.len(), 1);
    a.undo();
    assert_eq!(a.session().document.pages.len(), 2);
    a.create_note();
    assert_ne!(first, a.active);
    a.switch_note(first);
    assert_eq!(a.session().document.pages.len(), 2);
}
#[test]
fn context_paste_places_mixed_objects_at_anchor_and_undoes_once() {
    let mut a = app();
    draw(&mut a);
    a.add_text("Text".into(), Point::new(120., 170.));
    a.select_all();
    a.transform_selection(
        Transform::around(Point::new(0., 0.), 1.3, 0.2).compose(Transform::translate(60., 90.)),
        "Move originals",
    );
    let originals = a.page().objects.clone();
    let clipboard = a.encode_clipboard().unwrap();
    let target = Point::new(450., 300.);
    a.paste_text_or_objects_at(clipboard.clone(), target);
    assert_eq!(a.page().objects.len(), 4);
    assert_eq!(a.session().selection.len(), 2);
    assert!(
        a.session()
            .selection
            .iter()
            .all(|id| !originals.contains_key(id))
    );
    let bounds = a.selection_bounds().unwrap();
    assert!((bounds.min.x - target.x).abs() < 0.01);
    assert!((bounds.min.y - target.y).abs() < 0.01);
    for (id, object) in &originals {
        assert_eq!(a.page().objects[id], *object);
    }
    a.undo();
    assert_eq!(a.page().objects, originals);
    a.redo();
    assert_eq!(a.page().objects.len(), 4);
    a.undo();
    a.paste_text_or_objects_at("Plain clipboard text".into(), target);
    let text = a.page().ordered_objects().last().unwrap();
    assert_eq!(text.bounds().min, target);
    a.undo();
    assert_eq!(a.page().objects, originals);
    a.session_mut().document.metadata.trashed = true;
    a.paste_text_or_objects_at(clipboard, target);
    assert_eq!(a.page().objects, originals);
}
#[test]
fn paste_image_anchors_at_target_and_selects_it_on_its_page() {
    let mut a = app();
    a.create_note();
    let note = a.active;
    let page = a.page().id;
    let image = image::DynamicImage::new_rgba8(4, 3);
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    a.paste_image_at(bytes.get_ref(), "png", Point::new(420., 260.));
    a.add_page();
    wait(&mut a);
    let pasted = a.sessions[&note]
        .document
        .pages
        .iter()
        .find(|p| p.id == page)
        .unwrap();
    assert_eq!(pasted.objects.len(), 1);
    assert_eq!(
        pasted.ordered_objects().next().unwrap().bounds().min,
        Point::new(420., 260.)
    );
    assert!(a.page().objects.is_empty());
    // Leaving the page before the import finished does not select there.
    assert!(a.session().selection.is_empty());
    // Ctrl+V pastes at the pointer, ready to move or resize.
    a.tool = Tool::Pen;
    a.cursor = Some(Point::new(120., 90.));
    a.paste_image(bytes.get_ref(), "png");
    wait(&mut a);
    let id = *a.session().selection.iter().next().unwrap();
    assert_eq!(a.session().selection.len(), 1);
    assert_eq!(a.page().objects[&id].bounds().min, Point::new(120., 90.));
    assert_eq!(a.tool, Tool::Lasso);
    a.undo();
    assert!(a.page().objects.is_empty());
}
fn wait(a: &mut Controller) {
    let start = Instant::now();
    while a.has_background_work() {
        a.tick();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(a.error.is_none(), "{:?}", a.error);
}
#[test]
fn save_with_autosave_off_removes_previously_saved_objects() {
    let mut a = app();
    draw(&mut a);
    a.flush().unwrap();
    a.settings.autosave = false;
    a.select_all();
    a.delete_selection();
    a.flush().unwrap();
    let d = Store::open(&a.database)
        .unwrap()
        .load(a.active)
        .unwrap()
        .unwrap();
    assert!(d.pages[0].objects.is_empty());
}
#[test]
fn long_active_stroke_recovers_and_cancel_removes_draft() {
    let mut a = app();
    a.pointer(event(20., 30., Phase::Down));
    a.pointer(event(22., 31., Phase::Move));
    a.last_draft = Instant::now() - Duration::from_secs(2);
    a.tick();
    a.persistence.flush().unwrap();
    let store = Store::open(&a.database).unwrap();
    let d = store.load(a.active).unwrap().unwrap();
    assert_eq!(d.pages[0].objects.len(), 1);
    a.cancel();
    a.persistence.flush().unwrap();
    assert!(
        store.load(a.active).unwrap().unwrap().pages[0]
            .objects
            .is_empty()
    );
}
#[test]
fn search_waits_for_new_text_to_be_durably_indexed() {
    let mut a = app();
    a.add_text("Freshly written heliotrope".into(), Point::new(10., 20.));
    a.search("heliotrope".into());
    wait(&mut a);
    assert_eq!(a.search_results.len(), 1);
    let parts = a.search_results[0].snippet_parts();
    assert!(parts.contains(&("heliotrope".to_string(), true)));
    assert_eq!(
        a.search_results[0].snippet_text(),
        "Freshly written heliotrope"
    );
    let page = a.page().id;
    a.navigate_search(a.active, page);
    assert!(!a.search_highlights.is_empty());
}
#[test]
fn converted_shapes_keep_raw_sources_through_copy_and_undo() {
    let mut a = app();
    a.set_tool(Tool::Shape);
    draw(&mut a);
    assert_eq!(a.page().objects.len(), 2);
    let original = a
        .page()
        .objects
        .values()
        .find_map(|o| {
            if let Object::Stroke(s) = o.as_ref() {
                Some(s.raw.clone())
            } else {
                None
            }
        })
        .unwrap();
    a.select_all();
    assert_eq!(a.session().selection.len(), 1);
    let shape = *a.session().selection.iter().next().unwrap();
    // Unrelated objects keep this a sparse copy after its hidden source is
    // included, exercising the ranked path and the source's drawing order.
    for i in 0..8 {
        a.add_text(format!("Unrelated {i}"), Point::new(500., i as f32 * 50.));
    }
    a.session_mut().selection = HashSet::from([shape]);
    let copy = a.copy_objects();
    assert_eq!(
        copy.iter().map(Object::id).collect::<Vec<_>>(),
        a.page().order[..2]
    );
    a.paste_objects(copy);
    assert_eq!(a.page().objects.len(), 12);
    assert_eq!(a.page().hidden_sources().len(), 2);
    a.undo();
    assert_eq!(a.page().objects.len(), 10);
    let raw = a
        .page()
        .objects
        .values()
        .find_map(|o| {
            if let Object::Stroke(s) = o.as_ref() {
                Some(&s.raw)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(raw, &original);
}
#[test]
fn only_one_controller_can_write_a_data_directory() {
    let a = app();
    assert!(Controller::open(a.data_dir.clone()).is_err());
}
#[test]
fn enabling_autosave_catches_up_all_pending_edits() {
    let mut a = app();
    a.set_autosave(false);
    draw(&mut a);
    a.add_text("pending text".into(), Point::new(40., 50.));
    a.set_autosave(true);
    a.persistence.flush().unwrap();
    let d = Store::open(&a.database)
        .unwrap()
        .load(a.active)
        .unwrap()
        .unwrap();
    assert_eq!(d.pages[0].objects.len(), 2);
}
fn with_unloaded_note() -> (Controller, Id) {
    let mut a = app();
    a.create_note();
    a.create_note();
    a.rename("Other note".into());
    a.flush().unwrap();
    let path = a.data_dir.clone();
    drop(a);
    let a = Controller::open(path).unwrap();
    let id = a.notes.iter().find(|n| n.id != a.active).unwrap().id;
    (a, id)
}
#[test]
fn late_load_cannot_replace_a_new_note_or_overwrite_edits() {
    let (mut a, target) = with_unloaded_note();
    for _ in 0..3 {
        a.switch_note(target);
    }
    a.create_note();
    let created = a.active;
    a.add_text("Keep this edit".into(), Point::new(50., 50.));
    wait(&mut a);
    assert_eq!(a.active, created);
    assert_eq!(a.page().objects.len(), 1);
}
#[test]
fn input_during_a_tab_load_cannot_edit_either_document() {
    let (mut a, target) = with_unloaded_note();
    let original = a.active;
    a.switch_note(target);
    a.pointer(event(30., 40., Phase::Down));
    a.pointer(event(35., 43., Phase::Move));
    wait(&mut a);
    assert_eq!(a.active, target);
    assert!(a.page().objects.is_empty());
    assert!(a.sessions[&original].document.pages[0].objects.is_empty());
    assert!(!a.loading_note());
    a.pointer(event(30., 40., Phase::Down));
    a.pointer(event(35., 43., Phase::Up));
    assert_eq!(a.page().objects.len(), 1);
}
fn root() -> PathBuf {
    std::env::temp_dir().join(format!("folio-upgrade-{}", Id::new_v4()))
}
#[test]
fn undo_and_redo_survive_reopening_with_autosave_disabled() {
    let root = root();
    let mut a = Controller::open(root.clone()).unwrap();
    a.set_autosave(false);
    a.add_text("first".into(), Point::new(20., 20.));
    a.add_text("second".into(), Point::new(20., 60.));
    a.undo();
    a.flush().unwrap();
    drop(a);
    let mut a = Controller::open(root).unwrap();
    assert_eq!(a.page().objects.len(), 1);
    assert!(a.session().history.can_redo());
    a.redo();
    assert_eq!(a.page().objects.len(), 2);
    a.undo();
    a.undo();
    assert!(a.page().objects.is_empty());
}
#[test]
fn resize_and_rotation_handles_create_reversible_commands() {
    let mut a = app();
    let id = insert(&mut a, ink());
    a.set_tool(Tool::Rectangle);
    a.session_mut().selection.insert(id);
    let before = a.page().objects[&id].clone();
    let bounds = before.bounds();
    let event = |position: Point, phase| PenEvent {
        device: folio_input::Device::Tablet,
        tool: folio_input::Tool::Pen,
        phase,
        position: Point::new(position.x + 56., position.y + 36.),
        pressure: 0.7,
        tilt_x: 0.,
        tilt_y: 0.,
        buttons: 0,
        timestamp: 1000,
    };
    let start = Point::new(bounds.max.x, bounds.min.y);
    let end = Point::new(start.x + bounds.width(), start.y - 10.);
    a.pointer(event(start, Phase::Down));
    assert!(matches!(a.interaction, Some(Interaction::Resize { .. })));
    a.pointer(event(end, Phase::Move));
    a.pointer(event(end, Phase::Up));
    assert!(a.page().objects[&id].bounds().width() > bounds.width() * 1.9);
    a.undo();
    assert_eq!(a.page().objects[&id], before);
    a.session_mut().selection.insert(id);
    let handle = Point::new(bounds.center().x, bounds.min.y - 24.);
    let end = Point::new(bounds.center().x + 30., bounds.center().y);
    a.pointer(event(handle, Phase::Down));
    assert!(matches!(a.interaction, Some(Interaction::Rotate { .. })));
    a.pointer(event(end, Phase::Move));
    a.pointer(event(end, Phase::Up));
    assert!(a.page().objects[&id].bounds().height() > bounds.width() * 0.9);
    a.undo();
    assert_eq!(a.page().objects[&id], before);
    let Object::Stroke(original) = before.as_ref() else {
        unreachable!()
    };
    let Object::Stroke(restored) = a.page().objects[&id].as_ref() else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(&restored.raw, &original.raw));
    assert!(Arc::ptr_eq(&restored.path, &original.path));
}
fn ink() -> InkStroke {
    let mut b = StrokeBuilder::new(PenStyle::default());
    for i in 0..80 {
        b.push(StrokePoint::new(
            Point::new(100. + i as f32 * 2., 100.),
            0.2 + i as f32 * 0.008,
            i * 8,
        ));
    }
    b.finish().unwrap()
}
fn insert(a: &mut Controller, s: InkStroke) -> Id {
    let id = s.id;
    let page = a.page().id;
    a.commit(
        "Ink",
        vec![Change::Object {
            page,
            id,
            before: None,
            after: Some(Arc::new(Object::Stroke(s))),
            index: 0,
        }],
    );
    id
}
#[test]
fn undo_and_redo_survive_reopening() {
    let mut a = app();
    let root = a.data_dir.clone();
    a.add_text("Durable undo".into(), Point::new(100., 100.));
    a.undo();
    a.flush().unwrap();
    drop(a);
    let mut a = Controller::open(root.clone()).unwrap();
    assert!(a.session().history.can_redo());
    assert!(a.page().objects.is_empty());
    a.redo();
    assert_eq!(a.page().text(), "Durable undo");
    a.flush().unwrap();
    drop(a);
    let mut a = Controller::open(root.clone()).unwrap();
    assert!(a.session().history.can_undo());
    a.undo();
    assert!(a.page().objects.is_empty());
    drop(a);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn segment_erase_is_atomic_and_preserves_original_sensor_data() {
    let mut a = app();
    let root = a.data_dir.clone();
    let mut original = ink();
    Arc::make_mut(&mut original.raw)[0].tilt_x = -0.0;
    Arc::make_mut(&mut original.raw)[1].buttons = 5;
    insert(&mut a, original.clone());
    a.settings.segment_eraser = true;
    a.set_tool(Tool::Eraser);
    let event = |phase| PenEvent {
        device: folio_input::Device::Tablet,
        tool: folio_input::Tool::Eraser,
        phase,
        position: Point::new(231., 136.),
        pressure: 0.8,
        tilt_x: 0.,
        tilt_y: 0.,
        buttons: 0,
        timestamp: 1000,
    };
    a.pointer(event(Phase::Down));
    a.pointer(event(Phase::Up));
    assert_eq!(a.page().objects.len(), 2);
    for object in a.page().objects.values() {
        let Object::Stroke(s) = object.as_ref() else {
            panic!("Expected fragments")
        };
        assert_eq!(s.raw, original.raw);
        assert!(s.fragment_path.is_some());
    }
    a.flush().unwrap();
    let expected = a.session().document.clone();
    let expected_history = serde_json::to_value(&a.session().history).unwrap();
    drop(a);
    let mut a = Controller::open(root.clone()).unwrap();
    assert_eq!(a.session().document, expected);
    assert_eq!(
        serde_json::to_value(&a.session().history).unwrap(),
        expected_history
    );
    let first = a.page().objects.values().next().unwrap().clone();
    let Object::Stroke(first) = first.as_ref() else {
        panic!()
    };
    for object in a.page().objects.values() {
        let Object::Stroke(stroke) = object.as_ref() else {
            panic!()
        };
        assert!(Arc::ptr_eq(&first.raw, &stroke.raw));
        assert!(Arc::ptr_eq(&first.path, &stroke.path));
        assert_eq!(stroke.raw[0].tilt_x.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(stroke.raw[1].buttons, 5);
    }
    for (command, _) in a.session().history.entries() {
        for change in &command.changes {
            if let Change::Object { before, after, .. } = change {
                for object in before.iter().chain(after.iter()) {
                    if let Object::Stroke(stroke) = object.as_ref() {
                        assert!(Arc::ptr_eq(&first.raw, &stroke.raw));
                        assert!(Arc::ptr_eq(&first.path, &stroke.path));
                    }
                }
            }
        }
    }
    a.undo();
    assert_eq!(a.page().objects.len(), 1);
    assert_eq!(
        a.page().objects[&original.id].as_ref(),
        &Object::Stroke(original)
    );
    a.redo();
    assert_eq!(a.page().objects.len(), 2);
    drop(a);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn ink_erasers_protect_media_and_keep_their_own_radius() {
    let mut a = app();
    let original = ink();
    insert(&mut a, original);
    a.add_text("Protect me".into(), Point::new(150., 90.));
    let text = *a.session().selection.iter().next().unwrap();
    a.style.width = 80.;
    assert_eq!(a.settings.eraser_radius, 10.);
    a.set_tool(Tool::Eraser);
    let event = |phase| PenEvent {
        device: folio_input::Device::Tablet,
        tool: PenTool::Eraser,
        phase,
        position: Point::new(231., 136.),
        pressure: 0.8,
        tilt_x: 0.,
        tilt_y: 0.,
        buttons: 0,
        timestamp: 1000,
    };
    for mode in [EraserMode::Stroke, EraserMode::Segment] {
        a.settings.eraser_mode = mode;
        a.pointer(event(Phase::Down));
        a.pointer(event(Phase::Up));
        assert!(a.page().objects.contains_key(&text));
        a.undo();
    }
    a.settings.eraser_mode = EraserMode::Object;
    a.pointer(event(Phase::Down));
    a.pointer(event(Phase::Up));
    assert!(!a.page().objects.contains_key(&text));
    a.undo();
    assert!(a.page().objects.contains_key(&text));
}
#[test]
fn refinement_applies_a_consistent_toggle_to_mixed_strokes_and_skips_media() {
    let mut a = app();
    let first = insert(&mut a, ink());
    let second = insert(&mut a, ink());
    a.session_mut().selection = HashSet::from([first]);
    a.refine_selection();
    a.session_mut().selection = HashSet::from([first, second]);
    a.refine_selection();
    assert!(
        a.page()
            .objects
            .values()
            .all(|o| matches!(o.as_ref(),Object::Stroke(s) if s.refinement_enabled))
    );
    a.refine_selection();
    assert!(
        a.page()
            .objects
            .values()
            .all(|o| matches!(o.as_ref(),Object::Stroke(s) if !s.refinement_enabled))
    );
    a.undo();
    assert!(
        a.page()
            .objects
            .values()
            .all(|o| matches!(o.as_ref(),Object::Stroke(s) if s.refinement_enabled))
    );
    a.add_text("Media".into(), Point::new(30., 30.));
    let before = a.session().document.clone();
    a.refine_selection();
    assert_eq!(a.session().document, before);
}
