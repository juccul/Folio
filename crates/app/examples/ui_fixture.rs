//! Throwaway documents for visual verification; never writes to the user's notes.
use folio_app::Controller;
use folio_document::{Color, Paper, Point};
use folio_input::{Device, PenEvent, Phase, Tool};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("Provide an empty fixture directory"),
    );
    assert!(
        !root.join("notes.sqlite3").exists(),
        "Fixture directory already contains notes"
    );
    let single = std::env::args().any(|argument| argument == "--single-note");
    let mut app = Controller::open(root)?;
    for folder in ["Personal", "Projects", "Reading"] {
        app.create_notebook(folder.into(), None).unwrap();
    }
    for (i, title) in [
        "Field notes",
        "Research journal",
        "Meeting notes",
        "Weekend plans",
        "Design sketches",
        "Reading list",
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            app.create_note();
        }
        app.rename(title.into());
        let folder = app.notebooks[i % app.notebooks.len()].id;
        app.metadata(|m| {
            m.notebook = Some(folder);
            m.favorite = i == 0 || i == 4;
        });
        if i == 0 {
            app.paper(Paper::Dots);
            app.add_text("A little room to think".into(), Point::new(64., 64.));
            app.add_text(
                "Write freely. Connect the ideas.\nKeep what matters.".into(),
                Point::new(64., 126.),
            );
            app.set_color(Color::from_rgb(0x3265a8));
            for line in 0..3 {
                for index in 0..=120 {
                    app.pointer(PenEvent {
                        device: Device::Tablet,
                        tool: Tool::Pen,
                        phase: if index == 0 {
                            Phase::Down
                        } else if index == 120 {
                            Phase::Up
                        } else {
                            Phase::Move
                        },
                        position: Point::new(
                            56. + 90. + index as f32 * 3.5,
                            36. + 250. + line as f32 * 54. + (index as f32 * 0.14).sin() * 15.,
                        ),
                        pressure: 0.4 + (index as f32 * 0.06).sin() * 0.2,
                        tilt_x: 12.,
                        tilt_y: -8.,
                        buttons: 0,
                        timestamp: line * 2000 + index as u64 * 8,
                    });
                }
            }
            // Regression samples for pressure ink at acute reversals, retraces
            // and crossings. These remain vector strokes in the test document.
            app.set_color(Color::from_rgb(0x293d35));
            app.style.width = 7.;
            app.style.stabilization = 0.;
            for (pattern, vertices) in [
                vec![
                    Point::new(80., 500.),
                    Point::new(210., 410.),
                    Point::new(155., 535.),
                ],
                vec![
                    Point::new(300., 440.),
                    Point::new(430., 510.),
                    Point::new(430., 440.),
                    Point::new(300., 510.),
                ],
                vec![
                    Point::new(80., 600.),
                    Point::new(250., 600.),
                    Point::new(80., 600.),
                ],
            ]
            .into_iter()
            .enumerate()
            {
                let mut samples = vec![vertices[0]];
                for pair in vertices.windows(2) {
                    for i in 1..=50 {
                        samples.push(pair[0].lerp(pair[1], i as f32 / 50.));
                    }
                }
                for (i, &p) in samples.iter().enumerate() {
                    app.pointer(PenEvent {
                        device: Device::Tablet,
                        tool: Tool::Pen,
                        phase: if i == 0 {
                            Phase::Down
                        } else if i + 1 == samples.len() {
                            Phase::Up
                        } else {
                            Phase::Move
                        },
                        position: app.session().viewport.to_screen(p),
                        pressure: 0.75,
                        tilt_x: 12.,
                        tilt_y: -8.,
                        buttons: 0,
                        timestamp: 10_000 + pattern as u64 * 2000 + i as u64 * 8,
                    });
                }
            }
            app.add_page();
            app.paper(Paper::Grid);
            app.add_text("Next steps\n1. Sketch the possibilities\n2. Find the useful connections\n3. Give the idea some space".into(),Point::new(64.,64.));
            app.change_page(0);
        }
        if single {
            break;
        }
    }
    app.flush().map_err(std::io::Error::other)?;
    println!(
        "Created visual-test notebooks in {}",
        app.data_dir.display()
    );
    Ok(())
}
