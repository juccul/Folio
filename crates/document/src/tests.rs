use super::*;
#[test]
fn legacy_document_is_readable_and_editing_upgrades_its_format() {
    let mut d = Document::new("legacy");
    d.version = 1;
    d.validate().unwrap();
    Command {
        label: "Upgrade".into(),
        changes: vec![],
    }
    .apply(&mut d, true);
    assert_eq!(d.version, FORMAT_VERSION);
}
#[test]
fn affine_roundtrip() {
    let p = Point::new(3., 7.);
    let t = Transform::around(Point::new(10., 20.), 2., 0.8).compose(Transform::translate(-4., 9.));
    assert!(t.inverse().unwrap().apply(t.apply(p)).distance(p) < 0.0001);
}
#[test]
fn favorite_changes_preserve_edit_time_through_undo_and_redo() {
    let mut doc = Document::new("Lecture notes");
    doc.metadata.updated_at = 17;
    let mut history = History::default();
    let before = doc.metadata.clone();
    let mut after = before.clone();
    after.favorite = true;
    history.execute(
        Command {
            label: "Favorite".into(),
            changes: vec![Change::Metadata { before, after }],
        },
        &mut doc,
    );
    assert!(doc.metadata.favorite);
    assert_eq!(doc.metadata.updated_at, 17);
    doc = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    history.undo(&mut doc).unwrap();
    assert!(!doc.metadata.favorite);
    assert_eq!(doc.metadata.updated_at, 17);
    history.redo(&mut doc).unwrap();
    assert!(doc.metadata.favorite);
    assert_eq!(doc.metadata.updated_at, 17);
    let before = doc.metadata.clone();
    let mut after = before.clone();
    after.title = "Renamed lecture notes".into();
    history.execute(
        Command {
            label: "Rename".into(),
            changes: vec![Change::Metadata { before, after }],
        },
        &mut doc,
    );
    assert!(doc.metadata.updated_at > 17);
}
#[test]
fn undo_page_and_metadata() {
    let mut d = Document::new("before");
    let original = d.clone();
    let mut h = History::default();
    let mut m = d.metadata.clone();
    m.title = "after".into();
    let page = d.pages[0].id;
    let initial = d.pages[0].properties.clone();
    let mut resized = initial.clone();
    resized.width += 20.;
    h.execute(
        Command {
            label: "change".into(),
            changes: vec![
                Change::Metadata {
                    before: d.metadata.clone(),
                    after: m,
                },
                Change::Properties {
                    page,
                    before: initial.clone(),
                    after: resized.clone(),
                },
                Change::Page {
                    index: 0,
                    before: None,
                    after: Some(Page::new()),
                },
                Change::Properties {
                    page,
                    before: resized,
                    after: initial.clone(),
                },
            ],
        },
        &mut d,
    );
    assert_eq!(d.pages.len(), 2);
    assert_eq!(d.pages[1].id, page);
    assert_eq!(d.pages[1].properties, initial);
    h.undo(&mut d).unwrap();
    let mut expected = original.pages.clone();
    expected[0].revision = d.pages[0].revision;
    assert_eq!(d.pages, expected);
    assert_eq!(d.metadata.title, "before");
    h.redo(&mut d).unwrap();
    assert_eq!(d.metadata.title, "after");
}
#[test]
fn refuses_future_format() {
    let mut d = Document::new("x");
    d.version += 1;
    assert!(d.validate().is_err());
}
fn text() -> Arc<Object> {
    Arc::new(Object::Text(TextBlock {
        id: Id::new_v4(),
        text: "Example".into(),
        rect: Rect::new(0., 0., 100., 40.),
        transform: Transform::default(),
        font_family: "sans-serif".into(),
        font_size: 20.,
        color: Color::INK,
        bold: false,
        italic: false,
        underline: false,
        alignment: Alignment::Left,
        list: ListStyle::None,
    }))
}
#[test]
fn batch_deletion_restores_interleaved_order_across_pages() {
    let mut doc = Document::new("Batch edits");
    doc.pages.push(Page::new());
    for page in &mut doc.pages {
        for _ in 0..256 {
            let object = text();
            page.order.push(object.id());
            page.objects.insert(object.id(), object);
        }
    }
    let original = doc.pages.clone();
    let deletion = Command {
        label: "Delete alternating objects on two pages".into(),
        changes: original
            .iter()
            .enumerate()
            .flat_map(|(i, page)| {
                page.order
                    .iter()
                    .enumerate()
                    .step_by(i + 2)
                    .map(|(index, id)| Change::Object {
                        page: page.id,
                        id: *id,
                        before: Some(page.objects[id].clone()),
                        after: None,
                        index,
                    })
            })
            .collect(),
    };
    for _ in 0..3 {
        deletion.apply(&mut doc, true);
        doc.validate().unwrap();
        for (i, page) in doc.pages.iter().enumerate() {
            assert_eq!(
                page.order,
                original[i]
                    .order
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| index % (i + 2) != 0)
                    .map(|(_, id)| *id)
                    .collect::<Vec<_>>()
            );
        }
        deletion.apply(&mut doc, false);
        doc.validate().unwrap();
        for (page, original) in doc.pages.iter().zip(&original) {
            assert_eq!(page.order, original.order);
            assert_eq!(page.objects, original.objects);
        }
    }

    // Segment erasing interleaves deletion with insertion of fresh
    // fragments. Each replacement belongs at its source's drawing rank.
    let replacements = Command {
        label: "Replace alternating objects on two pages".into(),
        changes: deletion
            .changes
            .iter()
            .flat_map(|change| {
                let Change::Object { page, index, .. } = change else {
                    unreachable!()
                };
                let object = text();
                [
                    change.clone(),
                    Change::Object {
                        page: *page,
                        id: object.id(),
                        before: None,
                        after: Some(object),
                        index: *index,
                    },
                ]
            })
            .collect(),
    };
    for _ in 0..3 {
        replacements.apply(&mut doc, true);
        doc.validate().unwrap();
        for (page, source) in doc.pages.iter().zip(&original) {
            let mut expected = source.order.clone();
            for change in &replacements.changes {
                if let Change::Object {
                    page: target,
                    id,
                    before: None,
                    index,
                    ..
                } = change
                    && *target == page.id
                {
                    expected[*index] = *id;
                }
            }
            assert_eq!(page.order, expected);
        }
        replacements.apply(&mut doc, false);
        doc.validate().unwrap();
        for (page, source) in doc.pages.iter().zip(&original) {
            assert_eq!(page.order, source.order);
            assert_eq!(page.objects, source.objects);
        }
    }

    // An insertion after a deletion flushes the old order entry, even when
    // the same identity is reused within the command.
    let page = &original[0];
    let id = page.order[0];
    let remove = Change::Object {
        page: page.id,
        id,
        before: Some(page.objects[&id].clone()),
        after: None,
        index: 0,
    };
    let reinsert = Change::Object {
        page: page.id,
        id,
        before: None,
        after: Some(page.objects[&id].clone()),
        index: 0,
    };
    let mixed = Command {
        label: "Remove and reinsert".into(),
        changes: vec![remove.clone(), reinsert],
    };
    for forward in [true, false] {
        mixed.apply(&mut doc, forward);
        doc.validate().unwrap();
        assert_eq!(doc.pages[0].order, page.order);
    }

    let mut deleted = doc.pages[0].clone();
    deleted.objects.remove(&id);
    deleted.order.remove(0);
    let replaced = Command {
        label: "Delete then replace page".into(),
        changes: vec![
            remove,
            Change::Page {
                index: 0,
                before: Some(deleted),
                after: Some(page.clone()),
            },
        ],
    };
    for forward in [true, false] {
        replaced.apply(&mut doc, forward);
        doc.validate().unwrap();
        assert_eq!(doc.pages[0].order, page.order);
    }
}

#[test]
fn middle_insertions_preserve_equal_and_clamped_positions() {
    for positions in [[0, 2, 999, 1000], [1, 1, 1, 1]] {
        let mut doc = Document::new("Insertion order");
        let existing = [text(), text()];
        let inserted = [text(), text(), text(), text()];
        let page = &mut doc.pages[0];
        for object in &existing {
            page.order.push(object.id());
            page.objects.insert(object.id(), object.clone());
        }
        let original = page.order.clone();
        let command = Command {
            label: "Insert at requested positions".into(),
            changes: inserted
                .iter()
                .zip(positions)
                .map(|(object, index)| Change::Object {
                    page: page.id,
                    id: object.id(),
                    before: None,
                    after: Some(object.clone()),
                    index,
                })
                .collect(),
        };
        let expected = if positions[0] == 0 {
            vec![
                inserted[0].id(),
                existing[0].id(),
                inserted[1].id(),
                existing[1].id(),
                inserted[2].id(),
                inserted[3].id(),
            ]
        } else {
            vec![
                existing[0].id(),
                inserted[3].id(),
                inserted[2].id(),
                inserted[1].id(),
                inserted[0].id(),
                existing[1].id(),
            ]
        };
        for _ in 0..2 {
            command.apply(&mut doc, true);
            assert_eq!(doc.pages[0].order, expected);
            doc.validate().unwrap();
            command.apply(&mut doc, false);
            assert_eq!(doc.pages[0].order, original);
            doc.validate().unwrap();
        }
    }
}
#[test]
fn duplicate_object_ids_across_pages_are_rejected() {
    let mut doc = Document::new("Collision");
    doc.pages.push(Page::new());
    let object = text();
    for page in &mut doc.pages {
        page.order.push(object.id());
        page.objects.insert(object.id(), object.clone());
    }
    assert!(doc.validate().is_err());
}
#[test]
fn malformed_geometry_and_styles_are_rejected_before_rendering() {
    for variant in 0..5 {
        let mut doc = Document::new("Malformed");
        let mut object = text().as_ref().clone();
        if let Object::Text(text) = &mut object {
            match variant {
                0 => text.font_size = f32::INFINITY,
                1 => text.font_size = 0.,
                2 => text.rect.max.x = f32::NAN,
                3 => text.rect.max.y = text.rect.min.y - 1.,
                _ => text.transform.a = f32::NAN,
            }
        }
        let id = object.id();
        doc.pages[0].order.push(id);
        doc.pages[0].objects.insert(id, Arc::new(object));
        assert!(
            doc.validate().is_err(),
            "Accepted malformed variant {variant}"
        );
    }
    let mut doc = Document::new("Shape");
    let shape = Object::Shape(Shape {
        id: Id::new_v4(),
        kind: ShapeKind::Line,
        vertices: vec![Point::new(0., 0.), Point::new(f32::INFINITY, 10.)],
        style: PenStyle::default(),
        transform: Transform::default(),
        source_strokes: vec![],
    });
    doc.pages[0].order.push(shape.id());
    doc.pages[0].objects.insert(shape.id(), Arc::new(shape));
    assert!(doc.validate().is_err());
}
#[test]
fn anisotropic_ink_bounds_contain_the_transformed_brush() {
    let stroke = InkStroke {
        id: Id::new_v4(),
        raw: Arc::new(vec![StrokePoint::new(Point::new(0., 0.), 0.5, 0)].into()),
        path: Arc::new(
            vec![PathPoint {
                position: Point::new(0., 0.),
                radius: 4.,
            }]
            .into(),
        ),
        style: PenStyle::default(),
        transform: Transform {
            a: 10.,
            d: 0.1,
            c: 3.,
            ..Default::default()
        },
        created_at: 0,
        fragment_path: None,
        refined_path: None,
        refinement_enabled: false,
    };
    let object = Object::Stroke(stroke.clone());
    let bounds = object.bounds();
    for i in 0..360 {
        let angle = (i as f32).to_radians();
        let p = stroke
            .transform
            .apply(Point::new(4. * angle.cos(), 4. * angle.sin()));
        assert!(
            bounds.contains(p),
            "Bounds {bounds:?} exclude transformed brush {p:?}"
        );
    }
}
#[test]
fn nonfinite_or_singular_transforms_have_no_inverse() {
    for transform in [
        Transform {
            a: f32::NAN,
            ..Default::default()
        },
        Transform {
            tx: f32::INFINITY,
            ..Default::default()
        },
        Transform {
            a: 0.,
            ..Default::default()
        },
    ] {
        assert!(transform.inverse().is_none());
    }
}
#[test]
fn compact_samples_roundtrip_bit_exact_and_json_compatible() {
    let empty_raw = RawSamples::from_slice(&[]);
    assert!(empty_raw.is_empty());
    assert_eq!(empty_raw.to_vec(), Vec::new());
    let empty_path = PathSamples::from_slice(&[]);
    assert!(empty_path.is_empty());
    assert_eq!(empty_path.to_vec(), Vec::new());

    // Varying fields including -0.0, +0.0, subnormals, varying tilt/buttons/timestamps.
    let raw_pts = vec![
        StrokePoint {
            timestamp: 1000,
            x: -0.0,
            y: 0.0,
            pressure: 0.5,
            tilt_x: -0.0,
            tilt_y: 12.25,
            buttons: 0,
        },
        StrokePoint {
            timestamp: 1008,
            x: 1.5,
            y: -3.25,
            pressure: 0.75,
            tilt_x: 1.0,
            tilt_y: -0.0,
            buttons: 5,
        },
        StrokePoint {
            timestamp: 1021,
            x: 3.125,
            y: -6.5,
            pressure: 0.9,
            tilt_x: 0.0,
            tilt_y: f32::MIN_POSITIVE,
            buttons: 2,
        },
        StrokePoint {
            timestamp: 1021,
            x: 1000.5,
            y: -500.25,
            pressure: 0.125,
            tilt_x: -14.5,
            tilt_y: 8.75,
            buttons: 0,
        },
    ];
    let compact_raw = RawSamples::from_slice(&raw_pts);
    assert_eq!(compact_raw.len(), raw_pts.len());
    let decoded_raw = compact_raw.to_vec();
    for (got, want) in decoded_raw.iter().zip(&raw_pts) {
        assert_eq!(got.timestamp, want.timestamp);
        assert_eq!(got.x.to_bits(), want.x.to_bits());
        assert_eq!(got.y.to_bits(), want.y.to_bits());
        assert_eq!(got.pressure.to_bits(), want.pressure.to_bits());
        assert_eq!(got.tilt_x.to_bits(), want.tilt_x.to_bits());
        assert_eq!(got.tilt_y.to_bits(), want.tilt_y.to_bits());
        assert_eq!(got.buttons, want.buttons);
    }
    let json = serde_json::to_string(&compact_raw).unwrap();
    let json_vec: Vec<StrokePoint> = serde_json::from_str(&json).unwrap();
    assert_eq!(json_vec, raw_pts);
    let from_json: RawSamples = serde_json::from_str(&json).unwrap();
    assert_eq!(from_json, compact_raw);

    // Smooth handwriting stroke compresses substantially and agrees on O(1) bounds.
    let smooth_raw: Vec<StrokePoint> = (0..64)
        .map(|i| {
            StrokePoint::new(
                Point::new(i as f32 * 2.5, (i as f32 * 0.2).sin() * 14. + 100.),
                0.7,
                1_700_000_000 + i as u64 * 8,
            )
        })
        .collect();
    let smooth_compact_raw = RawSamples::from_slice(&smooth_raw);
    assert_eq!(smooth_compact_raw.to_vec(), smooth_raw);
    assert!(
        smooth_compact_raw.byte_len() < std::mem::size_of_val(smooth_raw.as_slice()) / 3,
        "expected >3x raw compression, got {} vs {}",
        smooth_compact_raw.byte_len(),
        std::mem::size_of_val(smooth_raw.as_slice())
    );

    let smooth_path: Vec<PathPoint> = (0..96)
        .map(|i| PathPoint {
            position: Point::new(i as f32 * 1.5, (i as f32 * 0.15).cos() * 12. + 50.),
            radius: 1.5 + (i as f32 * 0.1).sin() * 0.4,
        })
        .collect();
    let compact_path = PathSamples::from_slice(&smooth_path);
    for (got, want) in compact_path.iter().zip(&smooth_path) {
        assert_eq!(got.position.x.to_bits(), want.position.x.to_bits());
        assert_eq!(got.position.y.to_bits(), want.position.y.to_bits());
        assert_eq!(got.radius.to_bits(), want.radius.to_bits());
    }
    let stroke = InkStroke {
        id: Id::new_v4(),
        raw: Arc::new(smooth_compact_raw),
        path: Arc::new(compact_path),
        style: PenStyle::default(),
        transform: Transform::translate(25., -10.),
        created_at: 0,
        fragment_path: None,
        refined_path: None,
        refinement_enabled: false,
    };
    let fast_bounds = stroke.bounds();
    let mut rotated = stroke.clone();
    rotated.fragment_path = Some(smooth_path);
    assert_eq!(fast_bounds, rotated.bounds());

    // Reject malformed PathSamples headers (unknown flags, mismatched HAS_NAN, overlong varints).
    let mut tampered = stroke.path.as_bytes().to_vec();
    tampered[0] |= 0x80;
    assert!(PathSamples::from_encoded(stroke.path.len() as u32, &tampered).is_none());
    let mut bad_nan = stroke.path.as_bytes().to_vec();
    bad_nan[0] ^= 0x02;
    assert!(PathSamples::from_encoded(stroke.path.len() as u32, &bad_nan).is_none());
    // Overlong ULEB32 encoding of delta 0 (0x80, 0x00) in a 2-point path must be rejected.
    let overlong = [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80, 0x00, 0x00];
    assert!(PathSamples::from_encoded(2, &overlong).is_none());

    // Multi-change Command::apply must not leave dangling IDs in page.order when an
    // object is inserted and deleted (or its page is replaced) within the same command.
    let mut doc = Document::new("Order");
    let page_id = doc.pages[0].id;
    let obj = Arc::new(Object::Stroke(stroke));
    let oid = obj.id();
    let cancel_cmd = Command {
        label: "Insert then delete".into(),
        changes: vec![
            Change::Object {
                page: page_id,
                id: oid,
                before: None,
                after: Some(obj.clone()),
                index: 0,
            },
            Change::Object {
                page: page_id,
                id: oid,
                before: Some(obj.clone()),
                after: None,
                index: 0,
            },
        ],
    };
    cancel_cmd.apply(&mut doc, true);
    assert!(doc.validate().is_ok());
    assert!(doc.pages[0].order.is_empty());

    let replacement = doc.pages[0].clone();
    let replace_cmd = Command {
        label: "Insert then replace page".into(),
        changes: vec![
            Change::Object {
                page: page_id,
                id: oid,
                before: None,
                after: Some(obj),
                index: 0,
            },
            Change::Page {
                index: 0,
                before: Some(replacement.clone()),
                after: Some(replacement),
            },
        ],
    };
    replace_cmd.apply(&mut doc, true);
    assert!(doc.validate().is_ok());
    assert!(doc.pages[0].order.is_empty());
}
