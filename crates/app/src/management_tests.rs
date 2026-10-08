use super::*;
fn app() -> Controller {
    Controller::open(std::env::temp_dir().join(format!("folio-management-{}", Id::new_v4())))
        .unwrap()
}
fn settle(a: &mut Controller) {
    let start = Instant::now();
    while a.has_background_work() {
        a.tick();
        assert!(start.elapsed() < Duration::from_secs(15), "worker stalled");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(a.error.is_none(), "{:?}", a.error);
}
fn pdf(a: &Controller) -> PathBuf {
    let mut doc = Document::new("source");
    doc.pages[0].properties.width = 400.;
    doc.pages[0].properties.height = 600.;
    let mut p = Page::new();
    p.properties.width = 800.;
    p.properties.height = 500.;
    doc.pages.push(p);
    let mut p = Page::new();
    p.properties.width = 300.;
    p.properties.height = 900.;
    doc.pages.push(p);
    let path = a.data_dir.join("Lecture.pdf");
    folio_export::pdf(&doc, &a.assets, &path).unwrap();
    path
}
#[test]
fn home_pdf_is_a_new_named_document_with_exact_pages_and_durable_order() {
    let mut a = app();
    let previous = a.active;
    a.add_text("Existing note".into(), Point::new(50., 50.));
    let old = a.session().document.clone();
    let path = pdf(&a);
    let imported = a.import_as_note(path);
    assert_ne!(previous, imported);
    a.switch_note(previous);
    settle(&mut a);
    assert_eq!(a.active, previous);
    assert_eq!(a.session().document, old);
    let d = &a.sessions[&imported].document;
    assert_eq!(d.metadata.title, "Lecture");
    assert_eq!(d.pages.len(), 3);
    for (i, p) in d.pages.iter().enumerate() {
        assert_eq!(p.properties.pdf.as_ref().unwrap().page, i as u32 + 1);
    }
    let ids = d.pages.iter().map(|p| p.id).collect::<Vec<_>>();
    a.switch_note(imported);
    a.undo();
    assert_eq!(a.session().document.pages.len(), 1);
    assert!(a.page().properties.pdf.is_none());
    a.redo();
    assert_eq!(
        a.session()
            .document
            .pages
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        ids
    );
    a.flush().unwrap();
    let root = a.data_dir.clone();
    drop(a);
    let mut a = Controller::open(root).unwrap();
    a.switch_note(imported);
    settle(&mut a);
    assert_eq!(
        a.session()
            .document
            .pages
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        ids
    );
}
#[test]
fn editor_import_appends_ordered_pages_and_preserves_annotated_first_page() {
    let mut a = app();
    a.add_text("Keep me".into(), Point::new(20., 20.));
    let first = a.page().clone();
    let path = pdf(&a);
    let target = a.active;
    a.import_into(target, path);
    a.create_note();
    let next = a.active;
    settle(&mut a);
    assert_eq!(a.active, next);
    let d = &a.sessions[&target].document;
    assert_eq!(d.pages.len(), 4);
    assert_eq!(d.pages[0], first);
    assert_eq!(
        d.pages
            .iter()
            .skip(1)
            .map(|p| p.properties.pdf.as_ref().unwrap().page)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    a.switch_note(target);
    a.undo();
    assert_eq!(a.session().document.pages, vec![first]);
}
#[test]
fn library_actions_load_the_target_without_switching_or_touching_the_active_note() {
    let mut a = app();
    let target = a.active;
    a.add_text("Library target".into(), Point::new(30., 30.));
    let object = a.page().order[0];
    a.create_note();
    let active = a.active;
    let unchanged = a.session().document.clone();
    a.flush().unwrap();
    a.sessions.remove(&target);
    a.manage_note(target, NoteAction::Rename("Renamed".into()));
    a.manage_note(target, NoteAction::Favorite);
    a.manage_note(target, NoteAction::Tags(vec!["work".into()]));
    a.manage_note(target, NoteAction::Duplicate);
    a.manage_note(target, NoteAction::Trash);
    settle(&mut a);
    assert_eq!(a.active, active);
    assert_eq!(a.session().document, unchanged);
    let target_doc = &a.sessions[&target].document;
    assert_eq!(target_doc.metadata.title, "Renamed");
    assert!(target_doc.metadata.favorite && target_doc.metadata.trashed);
    let copy = a
        .notes
        .iter()
        .find(|n| n.title == "Renamed (copy)")
        .unwrap()
        .id;
    assert_eq!(a.sessions[&copy].page().objects.len(), 1);
    assert!(!a.sessions[&copy].page().objects.contains_key(&object));
    a.manage_note(target, NoteAction::Trash);
    a.flush().unwrap();
    let store = Store::open(&a.database).unwrap();
    let restored = store.load(target).unwrap().unwrap();
    assert!(!restored.metadata.trashed);
    assert_eq!(restored.metadata.tags, vec!["work"]);
    a.switch_note(target);
    a.undo();
    assert!(a.session().document.metadata.trashed);
}
#[test]
fn background_pdf_import_is_pinned_while_other_notes_open() {
    let mut a = app();
    let id = a.active;
    *a.pending_imports.entry(id).or_default() += 1;
    for _ in 0..12 {
        a.create_note();
        a.flush().unwrap();
        a.trim_caches();
    }
    assert!(a.sessions.contains_key(&id));
    let mut p = Page::new();
    p.properties.pdf = Some(PdfBackground {
        asset: "fixture.pdf".into(),
        page: 1,
        preview_asset: None,
    });
    a.apply_pdf(id, vec![p]);
    assert_eq!(a.sessions[&id].document.pages.len(), 1);
    assert!(a.sessions[&id].page().properties.pdf.is_some());
    a.import_finished(id);
}

#[test]
fn organized_pages_keep_content_bookmarks_order_and_history_after_reopen() {
    let mut a = app();
    a.add_text("Lecture one".into(), Point::new(30., 30.));
    a.bookmark_page("Introduction".into());
    let first = a.page().id;
    a.duplicate_page();
    let second = a.page().id;
    assert_ne!(first, second);
    assert_eq!(a.page().text(), "Lecture one");
    assert_ne!(a.session().document.pages[0].order, a.page().order);
    a.reorder_page(second, first);
    assert_eq!(a.session().document.pages[0].id, second);
    assert_eq!(a.page().id, second);
    a.undo();
    assert_eq!(a.session().document.pages[0].id, first);
    assert_eq!(a.page().id, second, "Undo must keep viewing the same page");
    a.redo();
    assert_eq!(a.page().id, second);
    a.flush().unwrap();
    let id = a.active;
    let root = a.data_dir.clone();
    drop(a);
    let mut a = Controller::open(root.clone()).unwrap();
    a.switch_note(id);
    settle(&mut a);
    assert_eq!(a.session().document.pages[0].id, second);
    assert_eq!(
        a.session().document.pages[0].properties.bookmark.as_deref(),
        Some("Introduction")
    );
    a.undo();
    assert_eq!(a.session().document.pages[0].id, first);
    drop(a);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn moving_only_page_saves_a_remapped_copy_and_keeps_source_undoable() {
    let mut a = app();
    let source = a.active;
    a.add_text("Keep this page".into(), Point::new(30., 30.));
    let object = a.page().order[0];
    a.create_note();
    let target = a.active;
    a.switch_note(source);
    a.move_page_to(target);
    settle(&mut a);
    assert_eq!(a.sessions[&source].document.pages.len(), 1);
    assert!(a.sessions[&source].document.pages[0].objects.is_empty());
    let copy = &a.sessions[&target].document.pages[1];
    assert_eq!(copy.text(), "Keep this page");
    assert_ne!(copy.order[0], object);
    a.undo();
    assert_eq!(a.page().text(), "Keep this page");
    a.flush().unwrap();
    let store = Store::open_reader(&a.database).unwrap();
    assert_eq!(
        store.load(target).unwrap().unwrap().pages[1].text(),
        "Keep this page"
    );
}

#[test]
fn library_covers_read_only_selected_page_without_opening_a_session() {
    let mut a = app();
    a.add_text("First page".into(), Point::new(20., 20.));
    a.add_page();
    a.add_text("Custom cover".into(), Point::new(20., 20.));
    a.use_page_as_cover();
    let note = a.active;
    a.flush().unwrap();
    a.create_note();
    a.sessions.remove(&note);
    assert!(a.library_preview(note).is_none());
    settle(&mut a);
    let (cover, count) = a.library_preview(note).unwrap();
    assert_eq!(cover.text(), "Custom cover");
    assert_eq!(count, 2);
    assert!(!a.sessions.contains_key(&note));
    let store = Store::open_reader(&a.database).unwrap();
    let (fallback, _) = store
        .library_preview(note, Some(Id::new_v4()))
        .unwrap()
        .unwrap();
    assert_eq!(fallback.text(), "First page");
}

#[test]
fn editable_notebook_import_is_one_undoable_command_and_reopens_with_assets() {
    let mut a = app();
    a.add_text("Shared notes".into(), Point::new(20., 20.));
    a.bookmark_page("Chapter".into());
    let exported = a.data_dir.join("shared.folio");
    portable::export_notebook(&a.session().document, &a.assets, &exported).unwrap();
    let old = a.active;
    let imported = a.import_as_note(exported);
    settle(&mut a);
    assert_ne!(old, imported);
    assert_eq!(a.session().document.pages.len(), 1);
    assert_eq!(a.page().text(), "Shared notes");
    assert_eq!(a.page().properties.bookmark.as_deref(), Some("Chapter"));
    a.undo();
    assert!(a.page().objects.is_empty());
    a.redo();
    assert_eq!(a.page().text(), "Shared notes");
    a.flush().unwrap();
    let store = Store::open_reader(&a.database).unwrap();
    assert_eq!(
        store.load(imported).unwrap().unwrap().pages[0].text(),
        "Shared notes"
    );
}

#[test]
fn configured_notebooks_preserve_canvas_choices_and_new_pages_after_save() {
    for infinite in [false, true] {
        let mut a = app();
        a.create_notebook("School".into(), None);
        let folder = a.notebooks[0].id;
        a.filter = NoteFilter::Notebook(folder);
        let properties = PageProperties {
            width: 1056.,
            height: 816.,
            infinite,
            paper: Paper::Dots,
            color: Some(Color::from_rgb(0xfff7e6)),
            pdf: None,
            bookmark: None,
        };
        let id = a
            .create_note_with_properties("  Calculus · ∫  ".into(), properties.clone())
            .unwrap();
        assert_eq!(a.session().document.metadata.title, "Calculus · ∫");
        assert_eq!(a.session().document.metadata.notebook, Some(folder));
        assert_eq!(a.page().properties, properties);
        assert!(a.session().history.entries().next().is_none());
        a.add_page();
        assert_eq!(a.page().properties, properties);
        a.undo();
        assert_eq!(a.session().document.pages.len(), 1);
        a.redo();
        a.flush().unwrap();
        let loaded = Store::open(&a.database).unwrap().load(id).unwrap().unwrap();
        assert_eq!(loaded.pages.len(), 2);
        assert!(
            loaded
                .pages
                .iter()
                .all(|page| page.properties == properties)
        );
        loaded.validate().unwrap();
    }
}

#[test]
fn invalid_notebook_setup_does_not_create_or_switch_a_document() {
    let mut a = app();
    let active = a.active;
    let count = a.notes.len();
    for name in ["", "  \n  "] {
        assert!(
            a.create_note_with_properties(name.into(), PageProperties::default())
                .is_err()
        );
    }
    for width in [f32::NAN, f32::INFINITY, 0., 100001.] {
        assert!(
            a.create_note_with_properties(
                "Test".into(),
                PageProperties {
                    width,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    assert_eq!(a.notes.len(), count);
    assert_eq!(a.active, active);
}

#[test]
fn old_documents_without_paper_color_keep_the_theme_default() {
    let mut value = serde_json::to_value(Document::new("Legacy")).unwrap();
    value["pages"][0]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("color");
    let loaded: Document = serde_json::from_value(value).unwrap();
    assert_eq!(loaded.pages[0].properties.color, None);
    loaded.validate().unwrap();
}

#[test]
fn continuous_scroll_rebases_and_writing_targets_the_clicked_page() {
    let mut a = app();
    a.create_note_with_properties(
        "stack".into(),
        PageProperties {
            width: 400.,
            height: 600.,
            paper: Paper::Blank,
            ..Default::default()
        },
    )
    .unwrap();
    a.add_page();
    a.add_page();
    a.change_page(0);
    a.session_mut().viewport.pan = Point::new(100., -650.);
    let stack = PageStack::new(&a.session().document.pages, 0).unwrap();
    let before = stack
        .viewport(a.session().viewport, 0, 1)
        .to_screen(Point::new(100., 100.));
    a.synchronize_page_view(800., 700.);
    assert_eq!(a.session().page, 1);
    assert!(
        a.session()
            .viewport
            .to_screen(Point::new(100., 100.))
            .distance(before)
            < 0.001
    );
    // Both pages are visible at this zoom. Down on a neighboring page must
    // rebase before converting its pen sample, without moving the document.
    a.session_mut().viewport.zoom = 0.5;
    a.session_mut().viewport.pan = Point::new(100., 32.);
    let stack = PageStack::new(&a.session().document.pages, 1).unwrap();
    let position = stack
        .viewport(a.session().viewport, 1, 2)
        .to_screen(Point::new(100., 100.));
    let event = |phase, position| PenEvent {
        phase,
        position,
        device: folio_input::Device::Mouse,
        tool: PenTool::Pen,
        pressure: 0.6,
        tilt_x: 0.,
        tilt_y: 0.,
        buttons: 0,
        timestamp: 100,
    };
    a.pointer(event(Phase::Down, position));
    assert_eq!(a.session().page, 2);
    a.synchronize_page_view(800., 700.); // Must not switch during a stroke.
    assert_eq!(a.session().page, 2);
    a.pointer(event(Phase::Move, Point::new(position.x + 30., position.y)));
    a.pointer(event(Phase::Up, Point::new(position.x + 30., position.y)));
    assert!(
        a.session().document.pages[..2]
            .iter()
            .all(|p| p.objects.is_empty())
    );
    assert_eq!(a.page().objects.len(), 1);
    let Object::Stroke(stroke) = a.page().ordered_objects().next().unwrap().as_ref() else {
        panic!("expected ink");
    };
    assert!(stroke.path[0].position.distance(Point::new(100., 100.)) < 0.01);
    a.undo();
    assert!(a.page().objects.is_empty());
    a.redo();
    assert_eq!(a.page().objects.len(), 1);
    a.flush().unwrap();
    let note = a.active;
    let path = a.data_dir.clone();
    drop(a);
    let mut reopened = Controller::open(path).unwrap();
    reopened.switch_note(note);
    settle(&mut reopened);
    assert_eq!(reopened.session().document.pages[2].objects.len(), 1);
    assert!(
        reopened.session().document.pages[..2]
            .iter()
            .all(|p| p.objects.is_empty())
    );
}

#[test]
fn page_navigation_jumps_to_top_and_infinite_canvas_stays_unbounded() {
    let mut a = app();
    a.add_page();
    a.session_mut().viewport.pan.y = -500.;
    a.change_page(0);
    assert_eq!(a.session().viewport.pan.y, 36.);
    a.session_mut().viewport.pan.y = -99999.;
    a.synchronize_page_view(800., 700.);
    assert_eq!(a.session().page, 1);
    let stack = PageStack::new(&a.session().document.pages, 1).unwrap();
    let bottom = stack
        .viewport(a.session().viewport, 1, 1)
        .to_screen(Point::new(0., a.page().properties.height));
    assert!((bottom.y - 664.).abs() < 0.01);
    a.page_size(794., 1123., true);
    a.session_mut().viewport.pan.y = -99999.;
    a.synchronize_page_view(800., 700.);
    assert_eq!(a.session().viewport.pan.y, -99999.);
}

#[test]
fn inactive_page_raster_previews_do_not_switch_or_edit_the_current_page() {
    let mut a = app();
    a.add_text("first page".into(), Point::new(100., 100.));
    let first = a.page().id;
    let id = a.page().order[0];
    a.add_page();
    let current = a.page().id;
    a.request_page_previews(0, &HashSet::from([id]));
    settle(&mut a);
    assert_eq!(a.page().id, current);
    assert!(a.previews.contains_key(&(a.active, first, id)));
    assert!(a.page().objects.is_empty());
}

#[test]
fn pdf_scroll_preview_upgrades_without_switching_pages_or_changing_content() {
    let mut a = app();
    let source = pdf(&a);
    a.import_as_note(source);
    settle(&mut a);
    let original = a.session().document.clone();
    let current = a.page().id;
    let background = original.pages[1].properties.pdf.clone().unwrap();
    let quick = a
        .assets
        .join(Controller::pdf_scroll_preview_asset(&background).unwrap());
    let sharp = a.assets.join(background.preview_asset.as_ref().unwrap());
    a.request_pdf_scroll_preview(background.clone());
    settle(&mut a);
    let dimensions = |path: &std::path::Path| {
        let png = std::fs::read(path).unwrap();
        (
            u32::from_be_bytes(png[16..20].try_into().unwrap()),
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
        )
    };
    let (width, height) = dimensions(&quick);
    assert_eq!(width.max(height), 960);
    assert!(!sharp.exists());
    a.request_pdf_background(background);
    settle(&mut a);
    let (width, height) = dimensions(&sharp);
    assert_eq!(width.max(height), 2400);
    assert!(
        quick.is_file(),
        "Quick image remains available during sharp-image decoding"
    );
    assert_eq!(a.page().id, current);
    assert_eq!(a.session().document, original);
}

#[test]
fn text_click_selects_then_edits_but_dragging_keeps_moving_the_box() {
    let mut a = app();
    a.add_text("editable text".into(), Point::new(100., 100.));
    let id = a.page().order[0];
    a.session_mut().selection.clear();
    let event = |phase, p: Point| PenEvent {
        phase,
        position: p,
        device: folio_input::Device::Mouse,
        tool: PenTool::Pen,
        pressure: 0.6,
        tilt_x: 0.,
        tilt_y: 0.,
        buttons: 0,
        timestamp: 100,
    };
    let p = a.session().viewport.to_screen(Point::new(180., 140.));
    a.pointer(event(Phase::Down, p));
    a.pointer(event(Phase::Up, p));
    assert_eq!(a.session().selection, HashSet::from([id]));
    assert!(a.pending_text_edit.is_none());
    a.pointer(event(Phase::Down, p));
    a.pointer(event(Phase::Up, p));
    assert_eq!(a.pending_text_edit.take(), Some(id));
    let end = Point::new(p.x + 30., p.y + 20.);
    a.pointer(event(Phase::Down, p));
    a.pointer(event(Phase::Move, end));
    a.pointer(event(Phase::Up, end));
    assert!(a.pending_text_edit.is_none());
    assert_eq!(
        a.page().objects[&id].transform(),
        Transform::translate(30., 20.)
    );
    assert_eq!(a.page().objects.len(), 1); // No ink dot or accidental new box.
    a.undo();
    assert_eq!(a.page().objects[&id].transform(), Transform::default());
    // A drag that returns to its origin is still a drag, not an edit click.
    a.pointer(event(Phase::Down, p));
    a.pointer(event(Phase::Move, end));
    a.pointer(event(Phase::Up, p));
    assert!(a.pending_text_edit.is_none());
    let mut down = event(Phase::Down, p);
    down.device = folio_input::Device::Tablet;
    let mut up = event(Phase::Up, p);
    up.device = folio_input::Device::Tablet;
    a.pointer(down);
    a.pointer(up);
    assert_eq!(a.pending_text_edit, Some(id));
}

#[test]
fn resizing_text_reflows_the_frame_preserves_fonts_and_is_reversible() {
    let mut a = app();
    a.add_text(
        "A sentence that should wrap when its box gets narrower.".into(),
        Point::new(100., 100.),
    );
    let id = a.page().order[0];
    let original = a.page().objects[&id].clone();
    a.scale_selection(0.5);
    let Object::Text(text) = a.page().objects[&id].as_ref() else {
        panic!("text");
    };
    assert_eq!(text.rect.width(), 180.);
    assert_eq!(text.rect.height(), 80.);
    assert_eq!(text.font_size, 20.);
    assert_eq!(
        [
            text.transform.a,
            text.transform.b,
            text.transform.c,
            text.transform.d
        ],
        [1., 0., 0., 1.]
    );
    a.undo();
    assert_eq!(a.page().objects[&id], original);
    a.redo();
    a.rotate_selection(0.4);
    a.resize_selection(
        Transform {
            a: 0.5,
            d: 2.,
            ..Default::default()
        },
        "Resize text",
    );
    let Object::Text(text) = a.page().objects[&id].as_ref() else {
        panic!("text");
    };
    assert!((text.transform.a.hypot(text.transform.b) - 1.).abs() < 0.001);
    assert!((text.transform.c.hypot(text.transform.d) - 1.).abs() < 0.001);
    assert!(
        (text.transform.a * text.transform.c + text.transform.b * text.transform.d).abs() < 0.001
    );
    assert_eq!(text.font_size, 20.);
    assert_eq!(
        text.text,
        "A sentence that should wrap when its box gets narrower."
    );
    a.flush().unwrap();
}

#[test]
fn failed_object_preview_is_local_and_retryable() {
    let mut a = app();
    let id = Id::new_v4();
    let object = Object::Image(folio_document::ImageObject {
        id,
        asset: "missing.png".into(),
        rect: Rect::new(0., 0., 100., 100.),
        transform: Transform::default(),
        crop: None,
    });
    a.commit(
        "Image",
        vec![Change::Object {
            page: a.page().id,
            id,
            before: None,
            after: Some(Arc::new(object)),
            index: 0,
        }],
    );
    a.request_previews(&HashSet::from([id]));
    settle(&mut a);
    assert!(a.page_preview_error().is_some());
    assert!(a.error.is_none());
    a.retry_previews();
    assert!(a.page_preview_error().is_none());
    assert!(a.preview_failed.is_empty());
}

#[test]
fn retry_saving_preserves_warning_until_all_documents_are_saved() {
    let mut a = app();
    let first = a.active;
    a.add_text("Recovered work".into(), Point::new(10., 10.));
    a.flush().unwrap();
    a.dirty_notes.insert(first);
    a.save_error = Some("Disk unavailable".into());
    a.create_note();
    a.retry_save();
    assert!(a.save_error.is_some());
    let start = Instant::now();
    while a.save_error.is_some() {
        a.tick();
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(a.dirty_notes.is_empty());
    assert!(a.error.is_none());
}

#[test]
fn equation_failures_remain_retryable_and_cancellation_does_not_insert() {
    let mut a = app();
    let before = a.page().clone();
    a.insert_equation("\\input{private}".into());
    settle(&mut a);
    assert!(matches!(a.equation_result, Some(Err(_))));
    assert!(!a.equation_pending);
    assert_eq!(a.page(), &before);
    a.insert_equation("x^2".into());
    a.cancel_equation_render();
    settle(&mut a);
    assert_eq!(a.page(), &before);
    a.insert_equation("x^2".into());
    settle(&mut a);
    assert!(matches!(a.equation_result, Some(Ok(()))));
    assert_eq!(a.page().objects.len(), 1);
}

#[test]
fn trash_is_read_only_for_all_content_and_metadata_until_restored() {
    let mut a = app();
    a.add_text("Keep this".into(), Point::new(20., 20.));
    let text = *a.page().order.first().unwrap();
    a.manage_note(a.active, NoteAction::Trash);
    let before = a.session().document.clone();
    a.add_text("Blocked".into(), Point::new(30., 30.));
    a.edit_text(text, |t| t.text = "Blocked edit".into());
    a.add_page();
    a.rename("Blocked rename".into());
    a.manage_note(a.active, NoteAction::Favorite);
    a.undo();
    a.redo();
    a.insert_equation("x^2".into());
    assert_eq!(a.session().document, before);
    a.manage_note(a.active, NoteAction::Trash);
    assert!(!a.read_only());
    a.add_text("Allowed".into(), Point::new(30., 30.));
    assert_eq!(a.page().objects.len(), 2);
}

#[test]
fn failed_home_import_is_provisional_and_can_retry_without_losing_annotations() {
    let mut a = app();
    let path = a.data_dir.join("missing.png");
    let imported = a.import_as_note(path.clone());
    settle(&mut a);
    assert!(a.import_is_provisional(imported));
    assert!(a.library_imports[&imported].error.is_some());
    a.flush().unwrap();
    let store = Store::open(&a.database).unwrap();
    assert!(store.load(imported).unwrap().is_none());
    a.dismiss_failed_import(imported);
    assert!(!a.notes.iter().any(|n| n.id == imported));
    let imported = a.import_as_note(path.clone());
    a.add_text("Keep my work".into(), Point::new(10., 10.));
    settle(&mut a);
    assert!(!a.import_is_provisional(imported));
    a.flush().unwrap();
    assert!(store.load(imported).unwrap().is_some());
    let source = a.data_dir.join("valid.png");
    folio_export::png(a.page(), &a.assets, &source, 0.1).unwrap();
    std::fs::copy(source, path).unwrap();
    a.retry_library_import(imported);
    settle(&mut a);
    assert!(!a.library_imports.contains_key(&imported));
    assert!(
        a.page()
            .ordered_objects()
            .any(|o| matches!(o.as_ref(),Object::Text(t) if t.text == "Keep my work"))
    );
    assert!(
        a.page()
            .ordered_objects()
            .any(|o| matches!(o.as_ref(), Object::Image(_)))
    );
}

#[test]
fn search_states_clear_stale_results_and_associate_latest_query_and_page() {
    let mut a = app();
    a.add_text("alpha".into(), Point::new(10., 10.));
    a.add_page();
    a.add_text("beta".into(), Point::new(10., 10.));
    a.flush().unwrap();
    a.search("alpha".into());
    assert_eq!(a.search_state, SearchState::Searching);
    settle(&mut a);
    assert_eq!(a.search_state, SearchState::Complete);
    assert_eq!(a.search_results[0].page_number, Some(1));
    a.search("beta".into());
    assert!(a.search_results.is_empty());
    assert_eq!(a.search_state, SearchState::Searching);
    a.search("alpha".into());
    settle(&mut a);
    assert_eq!(a.search_results.len(), 1);
    assert_eq!(a.search_results[0].page_number, Some(1));
    a.search(" ".into());
    assert_eq!(a.search_state, SearchState::Idle);
    assert!(a.search_results.is_empty());
    a.database = a.data_dir.join("missing-folder").join("broken.sqlite");
    a.search("alpha".into());
    settle(&mut a);
    assert!(matches!(a.search_state, SearchState::Failed(_)));
    assert!(a.error.is_none());
}
