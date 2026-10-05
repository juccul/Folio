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
