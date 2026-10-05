use super::*;
use std::f32::consts::{PI, TAU};

fn samples(points: &[Point]) -> Vec<StrokePoint> {
    points
        .iter()
        .enumerate()
        .map(|(i, &p)| StrokePoint::new(p, 0.7, i as u64 * 12))
        .collect()
}
fn zigzag(passes: usize, subdivisions: usize, angle: f32) -> Vec<Point> {
    let transform = Transform::around(Point::new(40., 8.), 1., angle);
    (0..passes)
        .flat_map(|pass| {
            (0..=subdivisions).map(move |i| {
                let t = i as f32 / subdivisions as f32;
                transform.apply(Point::new(
                    if pass % 2 == 0 {
                        80. * t
                    } else {
                        80. * (1. - t)
                    },
                    pass as f32 * 3. + t * 3.,
                ))
            })
        })
        .collect()
}
fn oval(turns: f32, radius_x: f32, radius_y: f32) -> Vec<Point> {
    (0..=96)
        .map(|i| {
            let angle = i as f32 / 96. * TAU * turns;
            Point::new(100. + radius_x * angle.cos(), 100. + radius_y * angle.sin())
        })
        .collect()
}
fn stroke(points: &[Point]) -> Object {
    let mut builder = folio_ink::StrokeBuilder::new(PenStyle::default());
    for p in samples(points) {
        builder.push(p);
    }
    Object::Stroke(builder.finish().unwrap())
}

#[test]
fn four_pass_scratch_is_rotation_and_report_rate_independent() {
    for angle in [0., PI * 0.25, PI * 0.5, PI * 0.75] {
        for subdivisions in [1, 4, 24, 80] {
            let mut points = samples(&zigzag(4, subdivisions, angle));
            // Dense, fast samples and sparse samples describe the same gesture.
            for (i, p) in points.iter_mut().enumerate() {
                p.timestamp = i as u64 * 3;
            }
            assert!(
                scratch(&points).is_some(),
                "angle={angle} subdivisions={subdivisions}"
            );
        }
    }
}

#[test]
fn handwriting_hooks_strikes_and_tremor_are_not_scratch() {
    let line = (0..100)
        .map(|i| Point::new(i as f32, (i as f32 * 0.4).sin() * 12.))
        .collect::<Vec<_>>();
    let jitter = (0..100)
        .map(|i| Point::new((i as f32).sin() * 2., i as f32 * 0.1))
        .collect::<Vec<_>>();
    let w = [
        Point::new(0., 0.),
        Point::new(6., 30.),
        Point::new(12., 0.),
        Point::new(18., 30.),
        Point::new(24., 0.),
    ];
    for path in [
        line,
        jitter,
        w.to_vec(),
        oval(1., 40., 20.),
        zigzag(2, 24, 0.),
    ] {
        assert!(scratch(&samples(&path)).is_none());
    }
}

#[test]
fn validated_scratch_targets_every_real_crossing_without_box_only_hits() {
    let gesture = scratch(&samples(&zigzag(4, 24, 0.))).unwrap();
    assert!(gesture.erases(&stroke(&[Point::new(40., -10.), Point::new(40., 30.)])));
    // Sparse segments must be hit between their endpoints too.
    let mut sparse = stroke(&[Point::new(40., -10.), Point::new(40., 30.)]);
    if let Object::Stroke(s) = &mut sparse {
        s.path = vec![
            PathPoint {
                position: Point::new(40., -10.),
                radius: 1.,
            },
            PathPoint {
                position: Point::new(40., 30.),
                radius: 1.,
            },
        ]
        .into();
    }
    assert!(gesture.erases(&sparse));
    // Its box surrounds the scribble, but its actual ink goes around it.
    assert!(!gesture.erases(&stroke(&[
        Point::new(-15., -15.),
        Point::new(95., -15.),
        Point::new(95., 30.)
    ])));
    assert!(gesture.erases(&stroke(&[Point::new(40., -10.), Point::new(40., 1.)])));
}

#[test]
fn rough_gapped_and_overdrawn_ovals_are_selection_loops() {
    for turns in [0.88, 1., 1.08, 1.15] {
        for (rx, ry) in [(45., 35.), (130., 25.)] {
            for rotation in [0., PI * 0.25, PI * 0.5] {
                for clockwise in [false, true] {
                    let mut path = oval(turns, rx, ry);
                    for (i, p) in path.iter_mut().enumerate() {
                        p.x += (i as f32 * 0.7).sin() * 1.2;
                    }
                    for p in &mut path {
                        *p = Transform::around(Point::new(100., 100.), 1., rotation).apply(*p);
                    }
                    if clockwise {
                        path.reverse();
                    }
                    let polygon = selection_loop(&path).unwrap_or_else(|| {
                        panic!("turns={turns} rx={rx} rotation={rotation} clockwise={clockwise}")
                    });
                    assert!(folio_ink::inside_polygon(Point::new(100., 100.), &polygon));
                }
            }
        }
    }
}

#[test]
fn arcs_letters_and_double_loops_do_not_become_selection() {
    let eight = (0..=100)
        .map(|i| {
            let a = i as f32 / 100. * TAU;
            Point::new(100. + a.sin() * 40., 100. + (2. * a).sin() * 30.)
        })
        .collect::<Vec<_>>();
    for path in [
        oval(0.65, 40., 30.),
        oval(2., 40., 30.),
        oval(1., 8., 8.),
        eight,
        zigzag(4, 24, 0.),
    ] {
        assert!(selection_loop(&path).is_none());
    }
}

#[test]
fn encircle_uses_transformed_ink_and_rejects_long_crossing_strokes() {
    let polygon = selection_loop(&oval(0.9, 50., 40.)).unwrap();
    assert!(encloses(
        &polygon,
        &stroke(&[Point::new(80., 95.), Point::new(120., 105.)])
    ));
    assert!(!encloses(
        &polygon,
        &stroke(&[Point::new(0., 100.), Point::new(200., 100.)])
    ));
    let mut moved = stroke(&[Point::new(0., 0.), Point::new(20., 10.)]);
    moved.set_transform(Transform::translate(90., 95.));
    assert!(encloses(&polygon, &moved));
    moved.set_transform(Transform::translate(0., 0.));
    assert!(!encloses(&polygon, &moved));
}

#[test]
fn three_pass_uneven_and_circular_scribbles_trigger() {
    let uneven = [
        Point::new(0., 0.),
        Point::new(80., 3.),
        Point::new(15., 7.),
        Point::new(65., 10.),
    ];
    for path in [
        zigzag(3, 1, 0.),
        zigzag(3, 24, PI * 0.5),
        uneven.to_vec(),
        oval(2., 35., 30.),
    ] {
        assert!(scratch(&samples(&path)).is_some(), "{path:?}");
    }
}

#[test]
fn detached_marks_attach_to_the_crossed_letter_without_spreading_to_neighbors() {
    let gesture = scratch(&samples(&zigzag(3, 12, 0.))).unwrap();
    let body = Rect::new(40., -5., 3., 30.);
    let dot = stroke(&[Point::new(41., -12.), Point::new(41., -11.)]);
    assert!(gesture.completes_letter(&dot, &[body]));
    let neighboring_dot = stroke(&[Point::new(57., -12.), Point::new(57., -11.)]);
    assert!(!gesture.completes_letter(&neighboring_dot, &[body]));
    let another_letter = stroke(&[Point::new(43., -5.), Point::new(45., 25.)]);
    assert!(!gesture.completes_letter(&another_letter, &[body]));
}
