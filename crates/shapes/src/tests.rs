use super::*;
use std::f32::consts::{PI, TAU};

fn sampled(vertices: &[Point], steps: usize) -> Vec<Point> {
    let mut result = vec![vertices[0]];
    for pair in vertices.windows(2) {
        for i in 1..=steps {
            result.push(pair[0].lerp(pair[1], i as f32 / steps as f32));
        }
    }
    result
}
fn oval(turns: f32, rx: f32, ry: f32) -> Vec<Point> {
    (0..=96)
        .map(|i| {
            let a = i as f32 / 96. * TAU * turns;
            Point::new(100. + rx * a.cos(), 100. + ry * a.sin())
        })
        .collect()
}
#[test]
fn line_including_a_fast_two_point_stroke() {
    for path in [
        vec![Point::new(0., 0.), Point::new(100., 180.)],
        (0..50)
            .map(|i| Point::new(i as f32 * 3., (i as f32).sin() * 0.5))
            .collect(),
    ] {
        assert_eq!(fit(&path).unwrap().kind, ShapeKind::Line);
    }
}
#[test]
fn closed_shapes_and_rotated_rectangles() {
    for (vertices, kind) in [
        (
            vec![
                Point::new(10., 10.),
                Point::new(100., 10.),
                Point::new(100., 100.),
                Point::new(10., 100.),
                Point::new(10., 10.),
            ],
            ShapeKind::Square,
        ),
        (
            vec![
                Point::new(0., 100.),
                Point::new(50., 0.),
                Point::new(100., 100.),
                Point::new(0., 100.),
            ],
            ShapeKind::Triangle,
        ),
        (
            vec![
                Point::new(50., 0.),
                Point::new(100., 50.),
                Point::new(50., 100.),
                Point::new(0., 50.),
                Point::new(50., 0.),
            ],
            ShapeKind::Diamond,
        ),
    ] {
        assert_eq!(fit(&sampled(&vertices, 24)).unwrap().kind, kind);
    }
    let rectangle = sampled(
        &[
            Point::new(0., 0.),
            Point::new(140., 0.),
            Point::new(140., 70.),
            Point::new(0., 70.),
            Point::new(0., 0.),
        ],
        24,
    );
    for angle in [0.13, 0.4, PI * 0.5] {
        let points = rectangle
            .iter()
            .map(|&p| Transform::around(Point::new(0., 0.), 1., angle).apply(p))
            .collect::<Vec<_>>();
        assert_eq!(fit(&points).unwrap().kind, ShapeKind::Rectangle);
    }
}
#[test]
fn rough_closures_curved_edges_and_hooks_from_reported_image() {
    for (vertices, kind) in [
        (
            vec![
                Point::new(178., 119.),
                Point::new(176., 143.),
                Point::new(169., 174.),
                Point::new(169., 190.),
                Point::new(182., 196.),
                Point::new(223., 195.),
                Point::new(224., 161.),
                Point::new(225., 126.),
                Point::new(168., 126.),
            ],
            ShapeKind::Rectangle,
        ),
        (
            vec![
                Point::new(241., 428.),
                Point::new(250., 389.),
                Point::new(254., 362.),
                Point::new(279., 396.),
                Point::new(307., 425.),
                Point::new(235., 427.),
            ],
            ShapeKind::Triangle,
        ),
        (
            vec![
                Point::new(83., 185.),
                Point::new(110., 230.),
                Point::new(143., 295.),
                Point::new(172., 350.),
            ],
            ShapeKind::Line,
        ),
        (
            vec![
                Point::new(344., 23.),
                Point::new(347., 51.),
                Point::new(344., 111.),
                Point::new(381., 97.),
                Point::new(415., 99.),
            ],
            ShapeKind::Polyline,
        ),
    ] {
        let fitted = fit(&sampled(&vertices, 16)).unwrap_or_else(|| panic!("Missing {kind:?}"));
        assert_eq!(fitted.kind, kind);
        assert!(fitted.confidence >= 0.88);
    }
}
#[test]
fn circles_ellipses_and_open_arcs() {
    for (path, kind) in [
        (oval(1., 60., 60.), ShapeKind::Circle),
        (oval(0.9, 60., 58.), ShapeKind::Circle),
        (oval(1.10, 60., 60.), ShapeKind::Circle),
        (oval(0.64, 50., 50.), ShapeKind::Arc),
        (oval(1., 90., 35.), ShapeKind::Ellipse),
    ] {
        assert_eq!(fit(&path).unwrap().kind, kind);
    }
    let rough = oval(1.08, 40., 36.)
        .iter()
        .enumerate()
        .map(|(i, p)| {
            Point::new(
                p.x + (i as f32 * 0.43).sin() * 2.,
                p.y + (i as f32 * 0.71).sin() * 2.,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(fit(&rough).unwrap().kind, ShapeKind::Circle);
}
#[test]
fn arrows_accept_both_drawing_orders_and_uneven_sampling() {
    let shaft = Point::new(452., 301.);
    let tip = Point::new(517., 203.);
    let left = Point::new(480., 214.);
    let right = Point::new(532., 257.);
    for vertices in [
        vec![shaft, tip, left, tip, right],
        vec![left, tip, right, tip, shaft],
    ] {
        for steps in [1, 8, 40] {
            assert_eq!(
                fit(&sampled(&vertices, steps)).unwrap().kind,
                ShapeKind::Arrow
            );
        }
    }
}
#[test]
fn reports_rotation_scale_and_endpoint_dwell_do_not_change_fits() {
    for angle in [0., PI * 0.25, PI * 0.5] {
        for scale in [0.3, 1., 4.] {
            let mut path = oval(1., 60., 60.);
            path.extend(std::iter::repeat_n(*path.last().unwrap(), 200));
            for p in &mut path {
                *p = Transform::translate(420., 300.)
                    .compose(Transform::around(Point::new(100., 100.), scale, angle))
                    .apply(*p);
            }
            assert_eq!(fit(&path).unwrap().kind, ShapeKind::Circle);
        }
    }
}
#[test]
fn uncertain_waves_spirals_and_nonfinite_input_stay_ink() {
    let wave = (0..100)
        .map(|i| Point::new(i as f32, (i as f32 * 0.3).sin() * 20.))
        .collect::<Vec<_>>();
    let spiral = (0..100)
        .map(|i| {
            let a = i as f32 / 100. * TAU * 2.;
            Point::new(100. + i as f32 * a.cos(), 100. + i as f32 * a.sin())
        })
        .collect::<Vec<_>>();
    for points in [
        wave,
        spiral,
        vec![Point::new(0., 0.), Point::new(f32::NAN, 100.)],
    ] {
        assert!(fit(&points).is_none());
    }
}

#[test]
fn separate_shaft_and_head_join_without_a_phantom_connector() {
    let shaft = sampled(&[Point::new(452., 301.), Point::new(517., 203.)], 20);
    let head = sampled(
        &[
            Point::new(480., 214.),
            Point::new(517., 203.),
            Point::new(532., 257.),
        ],
        20,
    );
    for paths in [
        [shaft.as_slice(), head.as_slice()],
        [head.as_slice(), shaft.as_slice()],
    ] {
        assert_eq!(fit_strokes(&paths).unwrap().kind, ShapeKind::Arrow);
    }
    let away = head
        .iter()
        .map(|p| Point::new(p.x + 80., p.y))
        .collect::<Vec<_>>();
    assert!(fit_strokes(&[&shaft, &away]).is_none());
}
#[test]
fn reported_open_arc_and_crossing_polyline_are_geometric_objects() {
    let arc = sampled(
        &[
            Point::new(31., 71.),
            Point::new(35., 55.),
            Point::new(52., 41.),
            Point::new(78., 34.),
            Point::new(109., 40.),
            Point::new(126., 55.),
            Point::new(132., 70.),
            Point::new(130., 86.),
            Point::new(121., 102.),
        ],
        12,
    );
    assert_eq!(fit(&arc).unwrap().kind, ShapeKind::Arc);
    let crossing = sampled(
        &[
            Point::new(278., 308.),
            Point::new(293., 274.),
            Point::new(310., 211.),
            Point::new(335., 223.),
            Point::new(354., 245.),
            Point::new(389., 292.),
            Point::new(400., 251.),
            Point::new(406., 192.),
            Point::new(364., 223.),
            Point::new(334., 260.),
            Point::new(307., 309.),
        ],
        12,
    );
    assert_eq!(fit(&crossing).unwrap().kind, ShapeKind::Polyline);
}

#[test]
fn reported_rough_circle_with_closing_hook_is_not_a_polygon() {
    let points = sampled(
        &[
            Point::new(429., 368.),
            Point::new(452., 382.),
            Point::new(453., 404.),
            Point::new(444., 428.),
            Point::new(424., 444.),
            Point::new(401., 447.),
            Point::new(384., 437.),
            Point::new(378., 416.),
            Point::new(379., 396.),
            Point::new(389., 381.),
            Point::new(413., 373.),
            Point::new(435., 374.),
            Point::new(457., 386.),
        ],
        12,
    );
    assert_eq!(fit(&points).unwrap().kind, ShapeKind::Circle);
}
