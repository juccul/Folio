//! Reproduce held normal-pen shapes in throwaway storage, using real hold timing.
use folio_app::Controller;
use folio_document::{Color, Object, Paper, Point};
use folio_input::{Device, PenEvent, Phase, Tool};
use std::{path::PathBuf, time::Duration};

fn trace(app: &mut Controller, vertices: &[(f32, f32)]) {
    let mut samples = vec![Point::new(vertices[0].0, vertices[0].1 + 80.)];
    for pair in vertices.windows(2) {
        let a = Point::new(pair[0].0, pair[0].1 + 80.);
        let b = Point::new(pair[1].0, pair[1].1 + 80.);
        for i in 1..=12 {
            samples.push(a.lerp(b, i as f32 / 12.));
        }
    }
    let event = |app: &Controller, p, phase| PenEvent {
        device: Device::Tablet,
        tool: Tool::Pen,
        phase,
        position: app.session().viewport.to_screen(p),
        pressure: 0.7,
        tilt_x: 12.,
        tilt_y: -8.,
        buttons: 0,
        timestamp: 100,
    };
    for (i, &p) in samples.iter().enumerate() {
        app.pointer(event(
            app,
            p,
            if i == 0 { Phase::Down } else { Phase::Move },
        ));
    }
    std::thread::sleep(Duration::from_millis(650));
    app.tick();
    app.pointer(event(app, *samples.last().unwrap(), Phase::Up));
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("Provide an empty fixture directory"),
    );
    assert!(
        !root.join("notes.sqlite3").exists(),
        "Fixture already contains notes"
    );
    let mut app = Controller::open(root.clone())?;
    app.rename("Held pen shapes".into());
    app.paper(Paper::Ruled);
    app.set_color(Color::from_rgb(0x293647));
    app.style.width = 3.;
    for vertices in [
        vec![
            (31., 71.),
            (35., 55.),
            (52., 41.),
            (78., 34.),
            (109., 40.),
            (126., 55.),
            (132., 70.),
            (130., 86.),
            (121., 102.),
        ],
        vec![
            (344., 23.),
            (347., 51.),
            (344., 111.),
            (381., 97.),
            (415., 99.),
        ],
        vec![
            (178., 119.),
            (176., 143.),
            (169., 174.),
            (169., 190.),
            (182., 196.),
            (223., 195.),
            (224., 161.),
            (225., 126.),
            (168., 126.),
        ],
        vec![(83., 185.), (110., 230.), (143., 295.), (172., 350.)],
        vec![
            (278., 308.),
            (293., 274.),
            (310., 211.),
            (335., 223.),
            (354., 245.),
            (389., 292.),
            (400., 251.),
            (406., 192.),
            (364., 223.),
            (334., 260.),
            (307., 309.),
        ],
        vec![(452., 301.), (517., 203.)],
        vec![(480., 214.), (517., 203.), (532., 257.)],
        vec![
            (241., 428.),
            (250., 389.),
            (254., 362.),
            (279., 396.),
            (307., 425.),
            (235., 427.),
        ],
        vec![
            (429., 368.),
            (452., 382.),
            (453., 404.),
            (444., 428.),
            (424., 444.),
            (401., 447.),
            (384., 437.),
            (378., 416.),
            (379., 396.),
            (389., 381.),
            (413., 373.),
            (435., 374.),
            (457., 386.),
        ],
    ] {
        trace(&mut app, &vertices);
    }
    let kinds = app
        .page()
        .order
        .iter()
        .filter_map(|id| match app.page().objects[id].as_ref() {
            Object::Shape(s) => Some(format!("{:?}", s.kind)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "Arc",
            "Polyline",
            "Rectangle",
            "Line",
            "Polyline",
            "Arrow",
            "Triangle",
            "Circle"
        ]
    );
    app.flush().map_err(std::io::Error::other)?;
    folio_export::png(app.page(), &app.assets, &root.with_extension("png"), 1.)?;
    println!("Held normal-pen shapes: {}", kinds.join(", "));
    Ok(())
}
