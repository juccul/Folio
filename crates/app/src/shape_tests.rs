use super::*;
use folio_input::Device;
use std::f32::consts::TAU;

fn event(app: &Controller, p: Point, phase: Phase) -> PenEvent {
    PenEvent {
        device: Device::Tablet,
        tool: PenTool::Pen,
        phase,
        position: app.session().viewport.to_screen(p),
        pressure: 0.7,
        tilt_x: 12.,
        tilt_y: -8.,
        buttons: 0,
        timestamp: 100,
    }
}
fn begin(app: &mut Controller, vertices: &[Point]) {
    let mut first = true;
    for pair in vertices.windows(2) {
        for i in 0..=12 {
            let p = pair[0].lerp(pair[1], i as f32 / 12.);
            app.pointer(event(
                app,
                p,
                if first {
                    first = false;
                    Phase::Down
                } else {
                    Phase::Move
                },
            ));
        }
    }
}
fn hold(app: &mut Controller) {
    let Some(Interaction::Ink { last_move, .. }) = &mut app.interaction else {
        panic!("No active ink");
    };
    *last_move = Instant::now() - Duration::from_millis(650);
}
fn finish(app: &mut Controller, p: Point) {
    app.pointer(event(app, p, Phase::Up));
}
fn with_app(test: impl FnOnce(&mut Controller)) {
    let root = std::env::temp_dir().join(format!("folio-shape-test-{}", Id::new_v4()));
    let mut app = Controller::open(root.clone()).unwrap();
    test(&mut app);
    app.flush().unwrap();
    drop(app);
    std::fs::remove_dir_all(root).unwrap();
}
fn shapes(app: &Controller) -> Vec<&Shape> {
    app.page()
        .objects
        .values()
        .filter_map(|o| {
            if let Object::Shape(s) = o.as_ref() {
                Some(s)
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn pen_hold_survives_endpoint_tremor_at_different_zoom_levels() {
    with_app(|app| {
        for zoom in [0.5, 1., 4.] {
            app.session_mut().viewport.zoom = zoom;
            let end = Point::new(180., 180.);
            begin(app, &[Point::new(60., 60.), end]);
            hold(app);
            for delta in [
                Point::new(1.5, 1.),
                Point::new(-1., 1.5),
                Point::new(1., -1.),
            ] {
                app.pointer(event(
                    app,
                    Point::new(end.x + delta.x / zoom, end.y + delta.y / zoom),
                    Phase::Move,
                ));
            }
            app.tick();
            assert!(
                matches!(&app.interaction,Some(Interaction::Ink{preview:Some(fit),..}) if fit.kind==ShapeKind::Line)
            );
            finish(app, end);
            assert_eq!(shapes(app).last().unwrap().kind, ShapeKind::Line);
        }
    });
}
#[test]
fn pen_up_checks_a_hold_even_when_preview_tick_was_missed() {
    with_app(|app| {
        let vertices = [
            Point::new(241., 428.),
            Point::new(250., 389.),
            Point::new(254., 362.),
            Point::new(279., 396.),
            Point::new(307., 425.),
            Point::new(235., 427.),
        ];
        begin(app, &vertices);
        hold(app);
        finish(app, *vertices.last().unwrap());
        assert_eq!(shapes(app)[0].kind, ShapeKind::Triangle);
        let source = shapes(app)[0].source_strokes[0];
        let original = app.page().objects[&source].clone();
        assert!(
            matches!(original.as_ref(),Object::Stroke(s) if s.raw[0].pressure==0.7&&s.raw[0].tilt_x==12.)
        );
        app.undo();
        assert!(app.page().objects.is_empty());
        app.redo();
        assert_eq!(app.page().objects[&source], original);
    });
}
#[test]
fn ordinary_pen_unheld_or_disabled_and_held_highlighter_stay_ink() {
    with_app(|app| {
        let vertices = [Point::new(100., 100.), Point::new(200., 200.)];
        begin(app, &vertices);
        finish(app, vertices[1]);
        assert!(shapes(app).is_empty());
        app.settings.hold_shapes = false;
        begin(app, &vertices);
        hold(app);
        app.tick();
        finish(app, vertices[1]);
        assert!(shapes(app).is_empty());
        app.settings.hold_shapes = true;
        app.style.tool = InkTool::Highlighter;
        begin(app, &vertices);
        hold(app);
        app.tick();
        finish(app, vertices[1]);
        assert!(shapes(app).is_empty());
    });
}
#[test]
fn explicit_shape_tool_still_snaps_on_lift_without_a_hold() {
    with_app(|app| {
        app.set_tool(Tool::Shape);
        app.settings.hold_shapes = false;
        let vertices = [
            Point::new(344., 23.),
            Point::new(347., 51.),
            Point::new(344., 111.),
            Point::new(381., 97.),
            Point::new(415., 99.),
        ];
        begin(app, &vertices);
        finish(app, *vertices.last().unwrap());
        assert_eq!(shapes(app)[0].kind, ShapeKind::Polyline);
        assert_eq!(shapes(app)[0].vertices.len(), 3);
        let v = &shapes(app)[0].vertices;
        let dot = (v[1].x - v[0].x) * (v[2].x - v[1].x) + (v[1].y - v[0].y) * (v[2].y - v[1].y);
        assert!(dot.abs() < 0.01);
    });
}
#[test]
fn held_two_stroke_arrow_keeps_both_sources_and_undo_restores_the_shaft() {
    with_app(|app| {
        let shaft = [Point::new(452., 301.), Point::new(517., 203.)];
        let head = [
            Point::new(480., 214.),
            Point::new(517., 203.),
            Point::new(532., 257.),
        ];
        begin(app, &shaft);
        hold(app);
        finish(app, shaft[1]);
        let original = app.page().objects.clone();
        assert_eq!(shapes(app)[0].kind, ShapeKind::Line);
        begin(app, &head);
        hold(app);
        app.tick();
        finish(app, head[2]);
        assert_eq!(shapes(app).len(), 1);
        let arrow = shapes(app)[0];
        assert_eq!(arrow.kind, ShapeKind::Arrow);
        assert_eq!(arrow.source_strokes.len(), 2);
        for id in &arrow.source_strokes {
            assert!(matches!(app.page().objects[id].as_ref(), Object::Stroke(_)));
        }
        app.undo();
        assert_eq!(app.page().objects, original);
        app.redo();
        assert_eq!(shapes(app)[0].kind, ShapeKind::Arrow);
        app.flush().unwrap();
        let store = Store::open(&app.database).unwrap();
        let restored = store.load(app.active).unwrap().unwrap();
        assert!(restored.pages[0].objects.values().any(|o|matches!(o.as_ref(),Object::Shape(s) if s.kind==ShapeKind::Arrow&&s.source_strokes.len()==2)));
    });
}
#[test]
fn held_arc_serializes_and_reloads_as_an_open_shape_with_original_ink() {
    with_app(|app| {
        let arc = (0..=32)
            .map(|i| {
                let a = i as f32 / 32. * TAU * 0.64;
                Point::new(150. + 50. * a.cos(), 150. + 50. * a.sin())
            })
            .collect::<Vec<_>>();
        begin(app, &arc);
        hold(app);
        app.tick();
        finish(app, *arc.last().unwrap());
        let fitted = shapes(app)[0];
        assert_eq!(fitted.kind, ShapeKind::Arc);
        assert!(fitted.vertices[0].distance(*fitted.vertices.last().unwrap()) > 20.);
        app.flush().unwrap();
        let store = Store::open(&app.database).unwrap();
        let restored = store.load(app.active).unwrap().unwrap();
        assert_eq!(restored.pages[0].objects, app.page().objects);
    });
}
#[test]
fn arrow_join_requires_a_recent_matching_trace_and_shape_intent() {
    for case in ["old", "different_preset", "no_hold"] {
        with_app(|app| {
            let shaft = [Point::new(452., 301.), Point::new(517., 203.)];
            begin(app, &shaft);
            hold(app);
            finish(app, shaft[1]);
            let line = shapes(app)[0].id;
            if case == "old" {
                let source = shapes(app)[0].source_strokes[0];
                let mut original = app.page().objects[&source].as_ref().clone();
                if let Object::Stroke(s) = &mut original {
                    s.created_at -= 6000;
                }
                let page = app.session().page;
                app.session_mut().document.pages[page]
                    .objects
                    .insert(source, Arc::new(original));
            } else if case == "different_preset" {
                app.style.width *= 2.;
            }
            let head = [
                Point::new(480., 214.),
                Point::new(517., 203.),
                Point::new(532., 257.),
            ];
            begin(app, &head);
            if case != "no_hold" {
                hold(app);
            }
            finish(app, head[2]);
            assert!(app.page().objects.contains_key(&line));
            assert!(shapes(app).iter().all(|s| s.kind != ShapeKind::Arrow));
        });
    }
}

#[test]
fn failed_hold_is_not_retried_until_pen_moves() {
    with_app(|app| {
        app.pointer(event(app, Point::new(40., 40.), Phase::Down));
        hold(app);
        app.tick();
        assert!(matches!(
            &app.interaction,
            Some(Interaction::Ink {
                fit_attempted: true,
                preview: None,
                ..
            })
        ));
        // A stationary failed fit must stay quiescent on subsequent UI ticks.
        for _ in 0..8 {
            app.tick();
        }
        app.pointer(event(app, Point::new(100., 40.), Phase::Move));
        assert!(matches!(
            &app.interaction,
            Some(Interaction::Ink {
                fit_attempted: false,
                ..
            })
        ));
        for x in 101..180 {
            app.pointer(event(app, Point::new(x as f32, 40.), Phase::Move));
        }
        hold(app);
        app.tick();
        assert!(
            matches!(&app.interaction, Some(Interaction::Ink { preview: Some(fit), .. }) if fit.kind == ShapeKind::Line)
        );
        finish(app, Point::new(180., 40.));
    });
}
