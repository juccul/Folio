use super::*;
use folio_input::Device;
use std::f32::consts::TAU;

fn draw(app: &mut Controller, points: &[Point], start: u64) {
    for (i, &position) in points.iter().enumerate() {
        let phase = if i == 0 {
            Phase::Down
        } else if i + 1 == points.len() {
            Phase::Up
        } else {
            Phase::Move
        };
        app.pointer(frame(app, position, phase, start + i as u64 * 12));
    }
}

fn begin_loop(app: &mut Controller, points: &[Point], start: u64) {
    for (i, &p) in points.iter().enumerate() {
        app.pointer(frame(
            app,
            p,
            if i == 0 { Phase::Down } else { Phase::Move },
            start + i as u64 * 12,
        ));
    }
}

fn hold(app: &mut Controller) {
    let Some(Interaction::Ink { last_move, .. }) = &mut app.interaction else {
        panic!("Expected active ink")
    };
    *last_move = Instant::now() - Duration::from_millis(650);
}

fn draw_held(app: &mut Controller, points: &[Point], start: u64) {
    begin_loop(app, points, start);
    hold(app);
    app.pointer(frame(app, *points.last().unwrap(), Phase::Up, start + 2000));
}

fn frame(app: &Controller, position: Point, phase: Phase, timestamp: u64) -> PenEvent {
    PenEvent {
        device: Device::Tablet,
        tool: PenTool::Pen,
        phase,
        position: app.session().viewport.to_screen(position),
        pressure: 0.65,
        tilt_x: 12.,
        tilt_y: -8.,
        buttons: 0,
        timestamp,
    }
}

fn oval(turns: f32) -> Vec<Point> {
    (0..=80)
        .map(|i| {
            let a = i as f32 / 80. * TAU * turns;
            Point::new(140. + a.cos() * 70., 160. + a.sin() * 45.)
        })
        .collect()
}

fn narrow_overdrawn_oval() -> Vec<Point> {
    (0..=120)
        .map(|i| {
            let a = i as f32 / 120. * TAU * 1.08;
            Point::new(140. + a.cos() * 15., 160. + a.sin() * 50.)
        })
        .collect()
}

fn with_app(test: impl FnOnce(&mut Controller)) {
    let root = std::env::temp_dir().join(format!("folio-gesture-test-{}", Id::new_v4()));
    let mut app = Controller::open(root.clone()).unwrap();
    test(&mut app);
    app.flush().unwrap();
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn short_scratch_erases_one_stroke_preserves_neighbors_and_survives_undo_reload() {
    let root = std::env::temp_dir().join(format!("folio-scratch-test-{}", Id::new_v4()));
    let mut app = Controller::open(root.clone()).unwrap();
    app.settings.scratch_erase = true;
    draw(
        &mut app,
        &[Point::new(110., 90.), Point::new(110., 130.)],
        0,
    );
    let original = app.page().ordered_objects().next().unwrap().clone();
    draw(
        &mut app,
        &[
            Point::new(90., 80.),
            Point::new(170., 80.),
            Point::new(170., 130.),
        ],
        1000,
    );
    app.add_text("Keep this text".into(), Point::new(80., 90.));
    draw(
        &mut app,
        &[
            Point::new(80., 100.),
            Point::new(160., 103.),
            Point::new(80., 106.),
            Point::new(160., 109.),
            Point::new(80., 112.),
        ],
        2000,
    );
    assert_eq!(app.page().objects.len(), 2);
    assert!(!app.page().objects.contains_key(&original.id()));
    assert_eq!(
        app.session()
            .history
            .entries()
            .filter(|(_, applied)| *applied)
            .last()
            .unwrap()
            .0
            .label,
        "Scratch erase"
    );
    app.undo();
    assert_eq!(app.page().objects[&original.id()], original);
    app.redo();
    app.flush().unwrap();
    drop(app);
    let reopened = Controller::open(root.clone()).unwrap();
    assert_eq!(reopened.page().objects.len(), 2);
    assert!(!reopened.page().objects.contains_key(&original.id()));
    drop(reopened);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn gapped_encircle_selects_and_can_immediately_move_ink_without_drawing_the_loop() {
    with_app(|app| {
        app.settings.encircle_select = true;
        draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
        let original = app.page().ordered_objects().next().unwrap().clone();
        draw_held(app, &oval(0.88), 1000);
        assert_eq!(
            app.page().objects.len(),
            1,
            "The selection loop must not become ink"
        );
        assert_eq!(app.session().selection, HashSet::from([original.id()]));
        assert_eq!(app.tool, Tool::Lasso);
        draw(app, &[Point::new(140., 160.), Point::new(160., 175.)], 3000);
        assert_eq!(app.page().objects.len(), 1);
        assert_eq!(
            app.page().objects[&original.id()].transform(),
            Transform::translate(20., 15.)
        );
        let Object::Stroke(before) = original.as_ref() else {
            panic!()
        };
        let Object::Stroke(after) = app.page().objects[&original.id()].as_ref() else {
            panic!()
        };
        assert_eq!(before.raw, after.raw);
        app.undo();
        assert_eq!(app.page().objects[&original.id()], original);
    });
}

#[test]
fn encircle_selects_enclosed_ink_and_leaves_crossing_ink_unselected() {
    with_app(|app| {
        draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
        let enclosed = app.page().order[0];
        draw(app, &[Point::new(30., 160.), Point::new(260., 160.)], 1000);
        app.settings.encircle_select = true;
        draw_held(app, &oval(1.1), 2000);
        assert_eq!(app.session().selection, HashSet::from([enclosed]));
        assert_eq!(app.page().objects.len(), 2);
    });
}

#[test]
fn encircle_selects_visible_shapes_without_selecting_hidden_source_ink() {
    with_app(|app| {
        app.settings.encircle_select = true;
        app.set_tool(Tool::Shape);
        draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
        let shape = app
            .page()
            .ordered_objects()
            .find(|object| matches!(object.as_ref(), Object::Shape(_)))
            .unwrap()
            .clone();
        let hidden = app.page().hidden_sources();
        assert_eq!(hidden.len(), 1);
        app.set_tool(Tool::Pen);
        draw_held(app, &oval(1.), 1000);
        assert_eq!(app.session().selection, HashSet::from([shape.id()]));
        assert_eq!(app.page().objects.len(), 2);
        draw(app, &[Point::new(140., 160.), Point::new(160., 175.)], 3000);
        for id in hidden.into_iter().chain([shape.id()]) {
            assert_eq!(
                app.page().objects[&id].transform(),
                Transform::translate(20., 15.)
            );
        }
        app.undo();
        assert_eq!(app.page().objects[&shape.id()], shape);
    });
}

#[test]
fn holding_an_encircle_selects_before_lift_and_takes_priority_over_shape_snapping() {
    with_app(|app| {
        app.settings.encircle_select = true;
        draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
        let enclosed = app.page().order[0];
        let points = oval(1.);
        for (i, &p) in points.iter().enumerate() {
            app.pointer(frame(
                app,
                p,
                if i == 0 { Phase::Down } else { Phase::Move },
                1000 + i as u64 * 12,
            ));
        }
        if let Some(Interaction::Ink { last_move, .. }) = &mut app.interaction {
            *last_move = Instant::now() - Duration::from_secs(1);
        }
        app.tick();
        assert!(app.interaction.is_none());
        assert_eq!(app.session().selection, HashSet::from([enclosed]));
        app.pointer(frame(app, *points.last().unwrap(), Phase::Up, 3000));
        assert_eq!(app.session().selection, HashSet::from([enclosed]));
        assert_eq!(app.page().objects.len(), 1);
    });
}

#[test]
fn held_mouse_encircle_takes_priority_over_scratch_without_erasing_ink() {
    for tick_before_lift in [false, true] {
        with_app(|app| {
            app.settings.encircle_select = true;
            app.settings.scratch_erase = true;
            draw(app, &[Point::new(153., 155.), Point::new(153., 165.)], 0);
            let original = app.page().ordered_objects().next().unwrap().clone();
            let points = narrow_overdrawn_oval();
            let samples = points
                .iter()
                .enumerate()
                .map(|(i, &p)| StrokePoint::new(p, 0.65, 1000 + i as u64 * 8))
                .collect::<Vec<_>>();
            let scratch = folio_gestures::scratch(&samples).unwrap();
            assert!(scratch.erases(&original));
            assert!(folio_gestures::encloses(
                &folio_gestures::selection_loop(&points).unwrap(),
                &original
            ));
            for (i, &point) in points.iter().enumerate() {
                let mut event = frame(
                    app,
                    point,
                    if i == 0 { Phase::Down } else { Phase::Move },
                    1000 + i as u64 * 8,
                );
                event.device = Device::Mouse;
                app.pointer(event);
            }
            hold(app);
            if tick_before_lift {
                app.tick();
            }
            let mut up = frame(app, *points.last().unwrap(), Phase::Up, 2500);
            up.device = Device::Mouse;
            app.pointer(up);
            assert_eq!(app.session().selection, HashSet::from([original.id()]));
            assert_eq!(app.page().objects.len(), 1);
            assert_eq!(app.page().objects[&original.id()], original);
            assert_eq!(app.tool, Tool::Lasso);
            // Selection must not add an undo command or mutate the original ink.
            app.undo();
            assert!(app.page().objects.is_empty());
        });
    }
}

#[test]
fn overlapping_scratch_still_erases_without_enabled_held_selection() {
    for encircle_enabled in [false, true] {
        with_app(|app| {
            app.settings.encircle_select = encircle_enabled;
            app.settings.scratch_erase = true;
            draw(app, &[Point::new(153., 155.), Point::new(153., 165.)], 0);
            let original = app.page().ordered_objects().next().unwrap().clone();
            let points = narrow_overdrawn_oval();
            if encircle_enabled {
                // An unheld loop keeps ordinary scratch behavior.
                draw(app, &points, 1000);
            } else {
                // A hold cannot select when circle selection is disabled.
                draw_held(app, &points, 1000);
            }
            assert!(app.page().objects.is_empty());
            assert!(app.session().selection.is_empty());
            assert_eq!(app.tool, Tool::Pen);
            app.undo();
            assert_eq!(app.page().objects[&original.id()], original);
        });
    }
}

#[test]
fn disabled_gestures_highlighter_and_shape_tools_keep_their_ink() {
    with_app(|app| {
        draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
        draw(app, &oval(1.), 1000);
        assert_eq!(app.page().objects.len(), 2);
        assert!(app.session().selection.is_empty());
        app.settings.encircle_select = true;
        app.settings.scratch_erase = true;
        app.style.tool = InkTool::Highlighter;
        draw_held(app, &oval(1.), 2000);
        assert_eq!(app.page().objects.len(), 3);
        assert!(app.session().selection.is_empty());
        app.style.tool = InkTool::Ballpoint;
        app.set_tool(Tool::Shape);
        draw(app, &oval(1.), 3000);
        assert!(
            app.page()
                .ordered_objects()
                .any(|o| matches!(o.as_ref(), Object::Shape(_)))
        );
        assert!(app.session().selection.is_empty());
    });
}

#[test]
fn encircle_returns_to_pen_on_first_outside_contact_but_explicit_lasso_stays() {
    with_app(|app| {
        app.settings.encircle_select = true;
        draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
        draw_held(app, &oval(1.), 1000);
        let outside = Point::new(300., 300.);
        app.pointer(frame(app, outside, Phase::Down, 3000));
        assert_eq!(app.tool, Tool::Pen);
        assert!(app.session().selection.is_empty());
        assert!(
            matches!(&app.interaction, Some(Interaction::Ink { builder, .. }) if builder.raw()[0].position() == outside)
        );
        app.pointer(frame(app, Point::new(350., 320.), Phase::Up, 3100));
        assert_eq!(app.page().objects.len(), 2);
        draw_held(app, &oval(1.), 4000);
        app.set_tool(Tool::Lasso);
        app.pointer(frame(app, outside, Phase::Down, 6000));
        assert_eq!(app.tool, Tool::Lasso);
        assert!(matches!(app.interaction, Some(Interaction::Lasso { .. })));
        app.cancel();
    });
}

#[test]
fn uneven_scratch_removes_letter_parts_and_dots_as_one_undo_command() {
    with_app(|app| {
        app.settings.scratch_erase = true;
        // Two letter bodies, an i-dot above the scribble, and a short crossbar
        // touched by only one pass. A neighboring word must survive.
        for (start, end) in [
            (Point::new(105., 90.), Point::new(105., 125.)),
            (Point::new(105., 80.), Point::new(105., 81.)),
            (Point::new(140., 90.), Point::new(140., 125.)),
            (Point::new(132., 98.), Point::new(150., 98.)),
        ] {
            draw(app, &[start, end], 0);
        }
        let letters = app.page().objects.clone();
        draw(app, &[Point::new(195., 90.), Point::new(195., 125.)], 0);
        let neighbor = *app.page().order.last().unwrap();
        draw(
            app,
            &[
                Point::new(80., 98.),
                Point::new(160., 102.),
                Point::new(95., 108.),
                Point::new(145., 114.),
            ],
            1000,
        );
        assert_eq!(app.page().objects.len(), 1);
        assert!(app.page().objects.contains_key(&neighbor));
        app.undo();
        for (id, object) in letters {
            assert_eq!(app.page().objects[&id], object);
        }
        app.redo();
        assert_eq!(app.page().objects.len(), 1);
    });
}

#[test]
fn circle_resize_handles_and_page_changes_keep_temporary_tool_state_consistent() {
    with_app(|app| {
        app.settings.encircle_select = true;
        draw(app, &[Point::new(110., 150.), Point::new(170., 170.)], 0);
        draw_held(app, &oval(1.), 1000);
        let bounds = app.selection_bounds().unwrap();
        app.pointer(frame(app, bounds.max, Phase::Down, 3000));
        assert!(matches!(app.interaction, Some(Interaction::Resize { .. })));
        assert_eq!(app.tool, Tool::Lasso);
        app.pointer(frame(
            app,
            Point::new(bounds.max.x + 20., bounds.max.y + 10.),
            Phase::Up,
            3100,
        ));
        app.change_page(0);
        assert_eq!(app.tool, Tool::Pen);
        assert!(app.session().selection.is_empty());
    });
}

#[test]
fn unheld_circles_stay_ink_and_resuming_movement_restarts_the_hold() {
    for device in [Device::Mouse, Device::Tablet] {
        for resume_phase in [Phase::Move, Phase::Up] {
            with_app(|app| {
                app.settings.encircle_select = true;
                draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
                let points = oval(1.);
                for (i, &p) in points.iter().enumerate() {
                    let mut event = frame(
                        app,
                        p,
                        if i == 0 { Phase::Down } else { Phase::Move },
                        1000 + i as u64 * 12,
                    );
                    event.device = device;
                    app.pointer(event);
                }
                app.tick();
                assert!(app.session().selection.is_empty());
                app.pointer(frame(app, points[80], Phase::Up, 2500));
                assert!(app.session().selection.is_empty());
                assert_eq!(app.page().objects.len(), 2);
                assert_eq!(app.tool, Tool::Pen);
                app.undo();
                begin_loop(app, &points, 3000);
                hold(app);
                // A new segment even on the final Up cannot reuse an old hold.
                app.pointer(frame(app, Point::new(210., 170.), resume_phase, 4500));
                if resume_phase == Phase::Move {
                    app.tick();
                    app.pointer(frame(app, Point::new(210., 170.), Phase::Up, 4600));
                }
                assert!(app.session().selection.is_empty());
                assert_eq!(app.page().objects.len(), 2);
                assert!(matches!(
                    app.page().ordered_objects().last().unwrap().as_ref(),
                    Object::Stroke(_)
                ));
            });
        }
    }
}

#[test]
fn held_circle_works_without_shape_snapping_and_tolerates_screen_pixel_tremor() {
    for zoom in [0.5, 1., 4.] {
        with_app(|app| {
            app.settings.encircle_select = true;
            app.settings.hold_shapes = false;
            app.session_mut().viewport.zoom = zoom;
            draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
            let enclosed = app.page().order[0];
            let points = oval(1.);
            begin_loop(app, &points, 1000);
            hold(app);
            app.pointer(frame(
                app,
                Point::new(210. + 1.5 / zoom, 160.),
                Phase::Move,
                2500,
            ));
            app.tick();
            assert_eq!(app.session().selection, HashSet::from([enclosed]));
            assert_eq!(app.tool, Tool::Lasso);
            assert!(app.interaction.is_none());
            app.pointer(frame(app, points[80], Phase::Up, 2700));
            assert_eq!(app.page().objects.len(), 1);
        });
    }
}

#[test]
fn held_encircle_uses_screen_size_at_every_zoom() {
    for (device, tick_before_lift) in [
        (Device::Mouse, true),
        (Device::Mouse, false),
        (Device::Tablet, true),
        (Device::Tablet, false),
    ] {
        for zoom in [0.25, 0.5, 1., 4., 8.] {
            with_app(|app| {
                app.settings.encircle_select = true;
                app.settings.hold_shapes = false;
                app.session_mut().viewport.zoom = zoom;
                let center = Point::new(300., 300.);
                draw(
                    app,
                    &[
                        Point::new(center.x - 10. / zoom, center.y),
                        Point::new(center.x + 10. / zoom, center.y),
                    ],
                    0,
                );
                let enclosed = app.page().order[0];
                let points = (0..=80)
                    .map(|i| {
                        let a = i as f32 / 80. * TAU;
                        Point::new(
                            center.x + a.cos() * 35. / zoom,
                            center.y + a.sin() * 25. / zoom,
                        )
                    })
                    .collect::<Vec<_>>();
                for (i, &point) in points.iter().enumerate() {
                    let mut event = frame(
                        app,
                        point,
                        if i == 0 { Phase::Down } else { Phase::Move },
                        1000 + i as u64 * 12,
                    );
                    event.device = device;
                    app.pointer(event);
                }
                hold(app);
                if tick_before_lift {
                    app.tick();
                    assert!(app.interaction.is_none());
                }
                let mut up = frame(app, points[80], Phase::Up, 3000);
                up.device = device;
                app.pointer(up);
                assert_eq!(
                    app.session().selection,
                    HashSet::from([enclosed]),
                    "A 70 × 50 screen-pixel circle must select at zoom {zoom}"
                );
                assert_eq!(app.page().objects.len(), 1);
            });
        }
    }
}

#[test]
fn outside_tap_clears_temporary_selection_without_ink_or_an_undo_entry() {
    for zoom in [0.5, 1., 4.] {
        with_app(|app| {
            app.settings.encircle_select = true;
            app.session_mut().viewport.zoom = zoom;
            draw(app, &[Point::new(110., 155.), Point::new(170., 165.)], 0);
            let original = app.page().ordered_objects().next().unwrap().clone();
            draw_held(app, &oval(1.), 1000);
            let outside = Point::new(300., 300.);
            app.pointer(frame(app, outside, Phase::Down, 4000));
            assert!(app.session().selection.is_empty());
            assert_eq!(app.tool, Tool::Pen);
            hold(app);
            app.last_draft = Instant::now() - Duration::from_secs(2);
            app.tick();
            app.pointer(frame(
                app,
                Point::new(300. + 1.5 / zoom, 300.),
                Phase::Up,
                5000,
            ));
            assert_eq!(app.page().objects.len(), 1);
            assert_eq!(app.page().objects[&original.id()], original);
            app.undo();
            assert!(
                app.page().objects.is_empty(),
                "A dismissal must not add an undo entry"
            );
            app.redo();
            // Subsequent pen taps remain valid dots.
            draw(app, &[outside, outside], 6000);
            assert_eq!(app.page().objects.len(), 2);
            app.undo();
            app.flush().unwrap();
            let store = Store::open(app.data_dir.join("notes.sqlite3")).unwrap();
            let reloaded = store.load(app.active).unwrap().unwrap();
            assert_eq!(
                reloaded.pages[0].objects.len(),
                1,
                "Dismissal must not persist a recovery dot"
            );
        });
    }
}

#[test]
fn vertical_scrubbing_across_a_word_erases_as_one_undoable_operation() {
    with_app(|app| {
        app.settings.scratch_erase = true;
        for x in [100., 120., 140., 160., 180.] {
            draw(app, &[Point::new(x, 95.), Point::new(x, 125.)], 0);
        }
        let letters = app.page().objects.clone();
        draw(app, &[Point::new(100., 160.), Point::new(180., 160.)], 0);
        let neighbor = *app.page().order.last().unwrap();
        draw(
            app,
            &[
                Point::new(100., 90.),
                Point::new(120., 130.),
                Point::new(140., 90.),
                Point::new(160., 130.),
                Point::new(180., 90.),
                Point::new(160., 130.),
                Point::new(140., 90.),
                Point::new(120., 130.),
                Point::new(100., 90.),
            ],
            1000,
        );
        assert_eq!(app.page().objects.len(), 1);
        assert!(app.page().objects.contains_key(&neighbor));
        app.undo();
        for (id, object) in letters {
            assert_eq!(app.page().objects[&id], object);
        }
        app.redo();
        assert_eq!(app.page().objects.len(), 1);
    });
}
