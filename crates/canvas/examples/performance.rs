//! Repeatable CPU microbenchmarks: cargo run --release -p folio-canvas --example performance
//! These measure geometry kernels, not application frame time or input latency.
use folio_canvas::{PageStack, SpatialIndex, Viewport};
use folio_document::*;
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn measure<T>(name: &str, mut operation: impl FnMut() -> T) {
    for _ in 0..3 {
        black_box(operation());
    }
    let start = Instant::now();
    let mut iterations = 0;
    while start.elapsed() < Duration::from_millis(250) {
        black_box(operation());
        iterations += 1;
    }
    println!(
        "{name:28} {:10.2} us/op ({iterations} iterations)",
        start.elapsed().as_secs_f64() * 1e6 / iterations as f64
    );
}

fn main() {
    let path = (0..10_000)
        .map(|i| PathPoint {
            position: Point::new(i as f32 * 1.2, (i as f32 * 0.025).sin() * 25.),
            radius: 2. + (i as f32 * 0.015).sin() * 0.5,
        })
        .collect::<Vec<_>>();
    let mut builder = folio_ink::StrokeBuilder::new(PenStyle::default());
    for (i, p) in path.iter().enumerate() {
        builder.push(StrokePoint::new(p.position, 0.7, i as u64 * 8));
    }
    let mut stroke = builder.finish().unwrap();
    stroke.path = path.clone().into();
    measure("outline/10k", || folio_ink::outline(black_box(&path)));
    measure("refine/10k", || folio_ink::refine(black_box(&mut stroke)));
    measure("cut/miss/10k", || {
        folio_ink::cut_segment(
            black_box(&stroke),
            Point::new(4000., 200.),
            Point::new(4000., 300.),
            5.,
        )
    });
    measure("cut/hit/10k", || {
        folio_ink::cut_segment(
            black_box(&stroke),
            Point::new(4000., -100.),
            Point::new(4000., 100.),
            5.,
        )
    });
    measure("sweep/hit/10k", || {
        folio_ink::swept_hit(
            black_box(&stroke),
            Point::new(4000., -100.),
            Point::new(4000., 100.),
            5.,
        )
    });
    let mut fragment = stroke.clone();
    fragment.fragment_path = Some(path[..2000].to_vec());
    measure("rebuild/fragment/2k", || {
        folio_ink::rebuild(black_box(&mut fragment))
    });

    let mut index = SpatialIndex::default();
    for i in 0..2000 {
        index.insert(
            Id::new_v4(),
            Rect::new((i % 50) as f32 * 20., (i / 50) as f32 * 20., 512., 512.),
        );
    }
    measure("spatial/query/multicell", || {
        index.query(black_box(Rect::new(0., 0., 1400., 1400.)))
    });
    let pages = (0..10_000).map(|_| Page::new()).collect::<Vec<_>>();
    let stack = PageStack::new(&pages, 0).unwrap();
    measure("pages/nearest/10k", || {
        stack.nearest(
            Viewport::default(),
            0,
            black_box(Point::new(100., 500_000.)),
        )
    });
    measure("pages/hit/10k", || {
        stack.hit(
            Viewport::default(),
            0,
            black_box(Point::new(100., 500_000.)),
        )
    });

    let circle = (0..=1000)
        .map(|i| {
            let a = i as f32 / 1000. * std::f32::consts::TAU;
            Point::new(100. + 60. * a.cos(), 100. + 60. * a.sin())
        })
        .collect::<Vec<_>>();
    measure("shapes/circle/1k", || folio_shapes::fit(black_box(&circle)));
    let scratch = (0..4)
        .flat_map(|pass| {
            (0..=100).map(move |i| {
                StrokePoint::new(
                    Point::new(
                        if pass % 2 == 0 {
                            i as f32
                        } else {
                            100. - i as f32
                        },
                        pass as f32 * 3.,
                    ),
                    0.7,
                    (pass * 101 + i) as u64 * 3,
                )
            })
        })
        .collect::<Vec<_>>();
    measure("gestures/scratch/400", || {
        folio_gestures::scratch(black_box(&scratch))
    });
    let gesture = folio_gestures::scratch(&scratch).unwrap();
    let object = Object::Stroke(stroke);
    measure("gestures/erases/10k", || gesture.erases(black_box(&object)));
}
