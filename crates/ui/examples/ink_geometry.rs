//! Device-free GPUI tessellation benchmark, including translucent long ink.
use folio_document::{PathPoint, Point};
use gpui::*;
use std::time::Instant;
fn main() {
    for count in [256, 1024, 4096, 12000] {
        let points = (0..count)
            .map(|i| PathPoint {
                position: Point::new(i as f32 * 1.2, (i as f32 * 0.025).sin() * 25.),
                radius: 2. + (i as f32 * 0.015).sin() * 0.5,
            })
            .collect::<Vec<_>>();
        let mut timings = vec![];
        for _ in 0..5 {
            let start = Instant::now();
            let contours = folio_ink::outline(&points);
            let mut b = PathBuilder::fill().with_style(PathStyle::Fill(
                FillOptions::default().with_fill_rule(FillRule::NonZero),
            ));
            for contour in &contours {
                b.move_to(point(px(contour[0].x), px(contour[0].y)));
                for p in &contour[1..] {
                    b.line_to(point(px(p.x), px(p.y)));
                }
                b.close();
            }
            std::hint::black_box(
                b.build()
                    .expect("Long ink must tessellate without index overflow"),
            );
            timings.push(start.elapsed().as_secs_f64() * 1000.);
        }
        timings.sort_by(f64::total_cmp);
        println!(
            "points={count}, median_ms={:.3}, max_ms={:.3}",
            timings[2], timings[4]
        );
    }
}
