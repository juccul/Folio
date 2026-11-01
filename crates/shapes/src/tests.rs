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
/// Hand-drawn trace: a bowed path with slow wobble and per-sample jitter.
fn hand(vertices: &[Point], amplitude: f32, seed: f32) -> Vec<Point> {
    let mut result = vec![];
    for pair in vertices.windows(2) {
        let steps = (pair[0].distance(pair[1]) / 3.).ceil().max(2.) as usize;
        let length = pair[0].distance(pair[1]).max(1e-3);
        let normal = Point::new(
            (pair[0].y - pair[1].y) / length,
            (pair[1].x - pair[0].x) / length,
        );
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let k = result.len() as f32;
            let jitter = ((k + seed) * 12.9898).sin() * 43758.547;
            let offset = amplitude
                * ((t * PI).sin() * 1.5 * (seed * 0.37).sin()
                    + (k * 0.21 + seed).sin() * 0.6
                    + jitter
                    - jitter.floor()
                    - 0.5);
            let p = pair[0].lerp(pair[1], t);
            result.push(Point::new(p.x + normal.x * offset, p.y + normal.y * offset));
        }
    }
    result.push(*vertices.last().unwrap());
    result
}
fn placed(points: &[Point], angle: f32, scale: f32) -> Vec<Point> {
    let t = Transform::translate(300., 300.).compose(Transform::around(
        Point::new(0., 0.),
        scale,
        angle,
    ));
    points.iter().map(|&p| t.apply(p)).collect()
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
fn irregular_single_stroke_arrows_are_not_left_as_jagged_lines() {
    let p = Point::new;
    let (tail, tip, left, right) = (p(-80., 0.), p(80., 0.), p(45., -28.), p(47., 30.));
    for vertices in [
        // Imperfect retrace back to the tip, overshoot, head first, closed
        // triangle head, rounded tip, landing hook, short and lopsided barbs.
        vec![tail, tip, left, p(76., 4.), right],
        vec![tail, p(86., 1.), left, tip, right],
        vec![left, tip, right, p(76., 4.), tail],
        vec![tail, tip, left, right, tip],
        vec![tail, p(74., 0.), p(79., -3.), left, p(74., 2.), right],
        vec![p(-86., 6.), tail, tip, left, p(77., -3.), right],
        vec![tail, tip, p(62., -14.), tip, p(64., 13.)],
        vec![tail, tip, p(40., -20.), p(76., 4.), p(60., 35.)],
    ] {
        for (i, angle) in [0., 0.7, 1.9, 3.3, 4.6].into_iter().enumerate() {
            for scale in [0.4, 1., 2.5] {
                for amplitude in [1.6, 4.8] {
                    let points = placed(
                        &hand(&vertices, amplitude, i as f32 + amplitude),
                        angle,
                        scale,
                    );
                    let kind = fit(&points).map(|f| f.kind);
                    assert_eq!(
                        kind,
                        Some(ShapeKind::Arrow),
                        "{vertices:?} {angle} {scale} {amplitude}"
                    );
                }
            }
        }
    }
    // Ordinary jagged lines, ticks, forks and zigzags keep their own fits.
    for vertices in [
        vec![p(0., 0.), p(60., 40.), p(40., 60.), p(100., 100.)],
        vec![p(0., 0.), p(30., 30.), p(60., 0.), p(90., 30.), p(120., 0.)],
        vec![p(0., 50.), p(30., 80.), p(100., 0.)],
        vec![p(0., 0.), p(150., 0.), p(130., -20.)],
        vec![
            p(0., 0.),
            p(40., 8.),
            p(70., -6.),
            p(110., 10.),
            p(150., 0.),
            p(160., -30.),
        ],
        vec![p(0., 0.), p(100., 0.), p(0., 80.), p(100., 80.)],
        vec![p(0., 100.), p(0., 0.), p(70., 100.), p(70., 0.)],
        vec![p(0., 0.), p(150., 0.), p(135., -25.), p(150., -28.)],
        vec![
            p(0., 0.),
            p(100., 0.),
            p(140., -30.),
            p(100., 0.),
            p(140., 30.),
        ],
    ] {
        for amplitude in [0., 1.2, 2.4] {
            let kind = fit(&hand(&vertices, amplitude, 3.)).map(|f| f.kind);
            assert_ne!(kind, Some(ShapeKind::Arrow), "{vertices:?} {amplitude}");
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
    let p = Point::new;
    let (tail, tip, left, right) = (p(-80., 0.), p(80., 0.), p(45., -28.), p(47., 30.));
    let gap = |dx: f32, dy: f32| p(80. + dx, dy);
    // Any stroke order and direction, pen-lift gaps, V, barb and triangle heads.
    for traces in [
        vec![vec![tip, tail], vec![left, tip, right]],
        vec![vec![tail, gap(-8., 2.)], vec![left, tip, right]],
        vec![vec![tail, tip], vec![left, tip], vec![right, tip]],
        vec![vec![tip, left], vec![tip, right], vec![tail, tip]],
        vec![
            vec![tail, tip],
            vec![left, gap(-5., -4.)],
            vec![gap(-3., 6.), right],
        ],
        vec![vec![tail, tip], vec![tip, left, right, tip]],
    ] {
        for (i, angle) in [0., 1.9, 4.6].into_iter().enumerate() {
            for scale in [0.4, 2.5] {
                let paths = traces
                    .iter()
                    .enumerate()
                    .map(|(j, v)| placed(&hand(v, 3.2, (i + j * 5) as f32), angle, scale))
                    .collect::<Vec<_>>();
                let paths = paths.iter().map(Vec::as_slice).collect::<Vec<_>>();
                let kind = fit_strokes(&paths).map(|f| f.kind);
                assert_eq!(kind, Some(ShapeKind::Arrow), "{traces:?} {angle} {scale}");
            }
        }
    }
    // A crossbar, a one-sided barb, a fork, a corner, a detached head and
    // neighboring lines are not arrows and are not absorbed into one.
    for traces in [
        vec![vec![tail, tip], vec![p(80., -30.), p(80., 30.)]],
        vec![vec![tail, tip], vec![p(0., -60.), p(0., 60.)]],
        vec![vec![tail, tip], vec![left, tip]],
        vec![
            vec![tail, tip],
            vec![left, tip],
            vec![p(-80., 30.), p(60., 30.)],
        ],
        vec![vec![tail, tip], vec![p(-80., 12.), p(80., 12.)]],
        vec![
            vec![tail, tip],
            vec![p(65., -28.), p(100., 0.), p(67., 30.)],
        ],
        vec![vec![tail, tip], vec![p(120., -30.), tip, p(120., 30.)]],
        vec![vec![left, tip, right], vec![p(-80., 60.), p(80., 60.)]],
        vec![vec![tail, tip], vec![tip, p(80., 120.)]],
    ] {
        let paths = traces
            .iter()
            .enumerate()
            .map(|(j, v)| hand(v, 2., j as f32 * 5.))
            .collect::<Vec<_>>();
        let paths = paths.iter().map(Vec::as_slice).collect::<Vec<_>>();
        assert!(fit_strokes(&paths).is_none(), "{traces:?}");
    }
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
