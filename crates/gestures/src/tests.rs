use super::*;
use folio_document::*;
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

// Rounded, unequal passes like a hand-drawn scrub, rather than ideal sawteeth.
fn rounded_scrub() -> Vec<Point> {
    let corners = [
        Point::new(0., 28.),
        Point::new(26., 0.),
        Point::new(34., 44.),
        Point::new(72., 8.),
        Point::new(86., 40.),
        Point::new(116., 2.),
        Point::new(136., 48.),
        Point::new(172., 12.),
        Point::new(193., 42.),
        Point::new(231., 0.),
        Point::new(252., 46.),
        Point::new(284., 9.),
        Point::new(300., 38.),
    ];
    let mut path = vec![corners[0]];
    for triple in corners.windows(3) {
        let a = triple[1].lerp(triple[0], 0.3);
        let b = triple[1].lerp(triple[2], 0.3);
        path.push(a);
        for i in 1..=8 {
            let t = i as f32 / 8.;
            path.push(a.lerp(triple[1], t).lerp(triple[1].lerp(b, t), t));
        }
    }
    path.push(*corners.last().unwrap());
    path
}

#[test]
fn rounded_unequal_scrubbing_is_not_required_to_make_sharp_turns() {
    for angle in [0., PI * 0.25, PI * 0.5, PI * 0.75] {
        let transform = Transform::around(Point::new(150., 24.), 1., angle);
        let path = rounded_scrub()
            .into_iter()
            .map(|p| transform.apply(p))
            .collect::<Vec<_>>();
        assert!(scratch(&samples(&path)).is_some(), "angle={angle}");
    }
}

#[test]
fn long_slow_scrubs_and_dense_reports_have_the_same_targets() {
    let path = rounded_scrub();
    let underlying = stroke(&[Point::new(0., 24.), Point::new(300., 24.)]);
    for subdivisions in [1, 4, 16] {
        let dense = path
            .windows(2)
            .flat_map(|pair| {
                (0..subdivisions)
                    .map(move |i| pair[0].lerp(pair[1], i as f32 / subdivisions as f32))
            })
            .chain(std::iter::once(*path.last().unwrap()))
            .collect::<Vec<_>>();
        for duration in [800, 4000, 15000] {
            let mut points = samples(&dense);
            let count = points.len();
            for (i, sample) in points.iter_mut().enumerate() {
                sample.timestamp = duration * i as u64 / (count - 1) as u64;
            }
            let gesture = scratch(&points).unwrap();
            assert_eq!(gesture.targets(&[&underlying]), vec![underlying.id()]);
        }
    }
}

#[test]
fn long_scrub_preserves_coverage_near_every_peak() {
    let path = (0..=80)
        .map(|i| Point::new(i as f32 * 18., if i % 2 == 0 { 0. } else { 45. }))
        .collect::<Vec<_>>();
    let gesture = scratch(&samples(&path)).unwrap();
    for i in (1..80).step_by(2) {
        let fragment = stroke(&[
            Point::new(i as f32 * 18., 39.),
            Point::new(i as f32 * 18., 40.),
        ]);
        assert!(gesture.erases(&fragment), "missed peak {i}");
    }
}

#[test]
fn flat_exit_tails_cross_ink_without_filling_a_large_empty_wedge() {
    let mut path = (0..=10)
        .map(|i| Point::new(i as f32 * 18., if i % 2 == 0 { 0. } else { 45. }))
        .collect::<Vec<_>>();
    path.push(Point::new(400., 0.));
    let gesture = scratch(&samples(&path)).unwrap();
    assert!(!gesture.erases(&stroke(&[Point::new(250., 12.), Point::new(254., 12.)])));
    assert!(gesture.erases(&stroke(&[Point::new(250., 0.), Point::new(254., 0.)])));
    assert!(gesture.erases(&stroke(&[Point::new(162., 9.), Point::new(162., 14.)])));
}

#[test]
fn swept_area_includes_ink_between_legs_but_not_the_entire_bounding_box() {
    let path = (0..=10)
        .map(|i| Point::new(i as f32 * 18., if i % 2 == 0 { 0. } else { 45. }))
        .collect::<Vec<_>>();
    let gesture = scratch(&samples(&path)).unwrap();
    // Entirely inside the first pair of passes, touching neither white leg.
    assert!(gesture.erases(&stroke(&[Point::new(18., 10.), Point::new(18., 14.)])));
    // Inside the global box, but outside the locally swept triangle at the end.
    assert!(!gesture.erases(&stroke(&[Point::new(1., 38.), Point::new(1., 42.)])));
    assert!(!gesture.erases(&stroke(&[Point::new(80., 65.), Point::new(90., 65.)])));
}

#[test]
fn screenshot_shaped_scrubs_with_unequal_amplitudes_and_tails_are_candidates() {
    // Approximate the white gesture geometry in the reported screenshots.
    // These are synthetic fixtures, not captured pointer recordings.
    for coordinates in [
        vec![
            (62., 78.),
            (98., 42.),
            (77., 89.),
            (137., 40.),
            (133., 79.),
            (170., 60.),
            (178., 100.),
            (215., 57.),
            (228., 94.),
            (268., 55.),
            (260., 88.),
            (286., 65.),
            (309., 92.),
            (348., 61.),
            (367., 44.),
            (381., 92.),
        ],
        vec![
            (50., 76.),
            (102., 47.),
            (117., 75.),
            (157., 42.),
            (180., 78.),
            (208., 48.),
            (230., 82.),
            (278., 51.),
            (300., 85.),
            (338., 57.),
            (365., 82.),
            (414., 53.),
            (439., 86.),
            (492., 50.),
            (517., 82.),
            (587., 23.),
            (590., 75.),
            (638., 44.),
            (653., 83.),
            (723., 43.),
            (717., 80.),
            (825., 61.),
        ],
        vec![
            (58., 82.),
            (82., 51.),
            (76., 83.),
            (107., 50.),
            (99., 80.),
            (133., 63.),
            (151., 77.),
            (203., 38.),
            (180., 88.),
            (236., 31.),
            (209., 84.),
            (267., 34.),
            (246., 89.),
            (300., 39.),
            (296., 91.),
            (341., 54.),
            (363., 79.),
            (404., 40.),
            (385., 87.),
            (438., 48.),
            (416., 81.),
            (473., 47.),
            (447., 93.),
            (508., 66.),
            (533., 81.),
            (590., 37.),
            (563., 87.),
            (617., 49.),
            (641., 74.),
            (676., 46.),
            (657., 81.),
            (710., 44.),
            (711., 85.),
            (752., 58.),
            (740., 96.),
            (783., 68.),
        ],
    ] {
        let path = coordinates
            .into_iter()
            .map(|(x, y)| Point::new(x, y))
            .collect::<Vec<_>>();
        let gesture = scratch(&samples(&path)).expect("screenshot-shaped scrub");
        let underlying = stroke(&[
            Point::new(path[0].x, 70.),
            Point::new(path.last().unwrap().x, 70.),
        ]);
        assert_eq!(gesture.targets(&[&underlying]), vec![underlying.id()]);
    }
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
    // A long wave and a rounded scrub have indistinguishable geometry. The
    // destructive decision now requires repeated crossings of existing ink.
    let wave = scratch(&samples(&line)).unwrap();
    assert!(wave.targets(&[]).is_empty());
    let incidental = stroke(&[Point::new(-5., -20.), Point::new(2., 20.)]);
    assert!(wave.targets(&[&incidental]).is_empty());
    for path in [jitter, w.to_vec(), oval(1., 40., 20.), zigzag(2, 24, 0.)] {
        assert!(
            scratch(&samples(&path)).is_none(),
            "unexpected scratch: {path:?}"
        );
    }
}

#[test]
fn erase_targets_require_existing_ink_across_passes_and_include_swept_fragments() {
    let path = (0..=10)
        .map(|i| Point::new(i as f32 * 18., if i % 2 == 0 { 0. } else { 45. }))
        .collect::<Vec<_>>();
    let gesture = scratch(&samples(&path)).unwrap();
    let body = stroke(&[Point::new(0., 25.), Point::new(180., 25.)]);
    let fragment = stroke(&[Point::new(18., 10.), Point::new(18., 14.)]);
    let corner = stroke(&[Point::new(1., 38.), Point::new(1., 42.)]);
    assert!(gesture.targets(&[&fragment]).is_empty());
    let targets = gesture.targets(&[&body, &fragment, &corner]);
    assert!(targets.contains(&body.id()));
    assert!(targets.contains(&fragment.id()));
    assert!(!targets.contains(&corner.id()));
    let mut highlighter = body.clone();
    if let Object::Stroke(s) = &mut highlighter {
        s.style.tool = InkTool::Highlighter;
    }
    assert!(gesture.targets(&[&highlighter, &fragment]).is_empty());
}

#[test]
fn short_vertical_scrubs_across_a_wide_word_use_travel_direction() {
    let path = [
        Point::new(0., 0.),
        Point::new(20., 40.),
        Point::new(40., 0.),
        Point::new(60., 40.),
        Point::new(80., 0.),
        Point::new(60., 40.),
        Point::new(40., 0.),
        Point::new(20., 40.),
        Point::new(0., 0.),
    ];
    for angle in [0., PI * 0.25, PI * 0.5, PI * 0.75] {
        for subdivisions in [1, 8, 40] {
            let transform = Transform::around(Point::new(40., 20.), 1., angle);
            let points = path
                .windows(2)
                .flat_map(|pair| {
                    (0..subdivisions).map(move |i| {
                        transform.apply(pair[0].lerp(pair[1], i as f32 / subdivisions as f32))
                    })
                })
                .chain(std::iter::once(transform.apply(*path.last().unwrap())))
                .collect::<Vec<_>>();
            let gesture = scratch(&samples(&points))
                .unwrap_or_else(|| panic!("angle={angle} subdivisions={subdivisions}"));
            let target = [Point::new(0., 20.), Point::new(80., 20.)].map(|p| transform.apply(p));
            assert!(gesture.erases(&stroke(&target)));
            let neighbor = [Point::new(0., 65.), Point::new(80., 65.)].map(|p| transform.apply(p));
            assert!(!gesture.erases(&stroke(&neighbor)));
            // One advancing W has no return passes through the same corridors.
            assert!(
                scratch(&samples(
                    &path[..5]
                        .iter()
                        .map(|&p| transform.apply(p))
                        .collect::<Vec<_>>()
                ))
                .is_none()
            );
        }
    }
}

#[test]
fn advancing_vertical_scrubs_cross_a_word_without_returning_horizontally() {
    let path = (0..=10)
        .map(|i| Point::new(i as f32 * 12., if i % 2 == 0 { 0. } else { 30. }))
        .collect::<Vec<_>>();
    for angle in [0., PI * 0.25, PI * 0.5, PI * 0.75] {
        for subdivisions in [1, 8, 30] {
            let transform = Transform::around(Point::new(60., 15.), 1., angle);
            let points = path
                .windows(2)
                .flat_map(|pair| {
                    (0..subdivisions).map(move |i| {
                        transform.apply(pair[0].lerp(pair[1], i as f32 / subdivisions as f32))
                    })
                })
                .chain(std::iter::once(transform.apply(*path.last().unwrap())))
                .collect::<Vec<_>>();
            let mut points = samples(&points);
            for (i, p) in points.iter_mut().enumerate() {
                p.timestamp = i as u64 * 3;
            }
            let gesture = scratch(&points)
                .unwrap_or_else(|| panic!("angle={angle} subdivisions={subdivisions}"));
            let target = [Point::new(0., 15.), Point::new(120., 15.)].map(|p| transform.apply(p));
            assert!(gesture.erases(&stroke(&target)));
            let neighbor = [Point::new(0., 50.), Point::new(120., 50.)].map(|p| transform.apply(p));
            assert!(!gesture.erases(&stroke(&neighbor)));
            assert!(
                scratch(&samples(
                    &path[..5]
                        .iter()
                        .map(|&p| transform.apply(p))
                        .collect::<Vec<_>>()
                ))
                .is_none()
            );
        }
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
fn minimum_encircle_size_is_measured_on_screen() {
    let small = oval(1., 8., 8.);
    assert!(selection_loop_with_scale(&small, 4.).is_some());
    assert!(selection_loop_with_scale(&small, 1.).is_none());
    assert!(selection_loop_with_scale(&oval(1., 40., 30.), 0.1).is_none());
    for scale in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(selection_loop_with_scale(&small, scale).is_none());
    }
    let polygon = selection_loop_with_scale(&small, 4.).unwrap();
    assert!(folio_ink::inside_polygon(Point::new(100., 100.), &polygon));
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
    let body = stroke(&[Point::new(40., -5.), Point::new(40., 25.)]);
    let dot = stroke(&[Point::new(41., -12.), Point::new(41., -11.)]);
    assert!(gesture.completes_letter(&dot, &[&body]));
    let neighboring_dot = stroke(&[Point::new(57., -12.), Point::new(57., -11.)]);
    assert!(!gesture.completes_letter(&neighboring_dot, &[&body]));
    let another_letter = stroke(&[Point::new(43., -5.), Point::new(45., 25.)]);
    assert!(!gesture.completes_letter(&another_letter, &[&body]));
}
