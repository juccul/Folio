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
        draw(app, &oval(0.88), 1000);
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
        draw(app, &oval(1.1), 2000);
        assert_eq!(app.session().selection, HashSet::from([enclosed]));
        assert_eq!(app.page().objects.len(), 2);
    });
}

#[test]
fn pausing_at_the_end_of_an_encircle_does_not_steal_selection() {
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
        assert!(
            matches!(
                &app.interaction,
                Some(Interaction::Ink {
                    preview: Some(_),
                    ..
                })
            ),
            "Hold-to-shape must have offered a preview"
        );
        app.pointer(frame(app, *points.last().unwrap(), Phase::Up, 3000));
        assert_eq!(app.session().selection, HashSet::from([enclosed]));
        assert_eq!(app.page().objects.len(), 1);
    });
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
        draw(app, &oval(1.), 2000);
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
        draw(app, &oval(1.), 1000);
        let outside = Point::new(300., 300.);
        app.pointer(frame(app, outside, Phase::Down, 3000));
        assert_eq!(app.tool, Tool::Pen);
        assert!(app.session().selection.is_empty());
        assert!(
            matches!(&app.interaction, Some(Interaction::Ink { builder, .. }) if builder.raw()[0].position() == outside)
        );
        app.pointer(frame(app, Point::new(350., 320.), Phase::Up, 3100));
        assert_eq!(app.page().objects.len(), 2);
        draw(app, &oval(1.), 4000);
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
        draw(app, &oval(1.), 1000);
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
