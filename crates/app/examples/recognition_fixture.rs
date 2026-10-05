//! Opt-in end-to-end local-model check with externally supplied stroke JSON.
//! Usage: FOLIO_RECOGNITION_CONFIG=... cargo run -p folio-app --example
//! recognition_fixture -- EMPTY_DIRECTORY INPUT_JSON
use folio_app::{Controller, RecognitionKind};
use folio_document::{Object, Point, Rect};
use folio_input::{Device, PenEvent, Phase, Tool};
use serde::Deserialize;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Input {
    kind: String,
    strokes: Vec<Vec<[f32; 2]>>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).ok_or("Provide an empty fixture directory")?);
    if root.join("notes.sqlite3").exists() {
        return Err("Fixture already contains notes".into());
    }
    let input: Input =
        serde_json::from_slice(&std::fs::read(args.get(2).ok_or("Provide stroke JSON")?)?)?;
    let kind = match input.kind.as_str() {
        "text" => RecognitionKind::Text,
        "math" => RecognitionKind::Math,
        _ => return Err("kind must be text or math".into()),
    };
    let mut app = Controller::open(root.clone())?;
    app.settings.hold_shapes = false;
    app.style.stabilization = 0.;
    app.rename("Recognition fixture".into());
    let expected_strokes = input.strokes.iter().filter(|s| !s.is_empty()).count();
    let bounds = Rect::from_points(
        input
            .strokes
            .iter()
            .flatten()
            .map(|&[x, y]| Point::new(x, y)),
    );
    let scale = (600. / bounds.width().max(1.))
        .min(400. / bounds.height().max(1.))
        .min(1.);
    let mut timestamp = 0;
    for stroke in input.strokes {
        if stroke.is_empty() {
            continue;
        }
        // A one-point dot still needs both a down and up frame.
        let samples = if stroke.len() == 1 {
            vec![stroke[0], stroke[0]]
        } else {
            stroke
        };
        for (i, &[x, y]) in samples.iter().enumerate() {
            timestamp += 8;
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
                position: app.session().viewport.to_screen(Point::new(
                    (x - bounds.min.x) * scale + 100.,
                    (y - bounds.min.y) * scale + 100.,
                )),
                pressure: 0.65,
                tilt_x: 0.,
                tilt_y: 0.,
                buttons: 0,
                timestamp,
            });
        }
    }
    app.select_all();
    assert_eq!(
        app.page().objects.len(),
        expected_strokes,
        "All supplied ink must fit on the fixture page"
    );
    let originals = app.page().objects.clone();
    let start = Instant::now();
    app.recognize_selection(kind)?;
    while app.recognition_pending {
        app.tick();
        if start.elapsed() > Duration::from_secs(185) {
            return Err("Recognition timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let text = app
        .recognition_review
        .as_ref()
        .ok_or_else(|| app.error.clone().unwrap_or("No result".into()))?
        .text
        .clone();
    assert_eq!(
        app.page().objects,
        originals,
        "Recognition must retain ink until replacement"
    );
    let elapsed = start.elapsed().as_secs_f32();
    app.replace_recognized_writing(text.clone())?;
    while app.recognition_pending {
        app.tick();
        if start.elapsed() > Duration::from_secs(185) {
            return Err("Equation rendering timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.page().objects.len(), 1);
    let replacement = app.page().ordered_objects().next().unwrap().as_ref();
    match kind {
        RecognitionKind::Text => assert!(matches!(replacement, Object::Text(_))),
        RecognitionKind::Math => assert!(matches!(replacement,
            Object::Equation(equation) if equation.rendered_svg.is_some() && equation.latex == text)),
    }
    app.undo();
    assert_eq!(app.page().objects, originals);
    app.redo();
    app.flush()?;
    drop(app);
    let mut reopened = Controller::open(root)?;
    assert_eq!(
        reopened
            .page()
            .ordered_objects()
            .next()
            .unwrap()
            .searchable_text(),
        text
    );
    reopened.undo();
    assert_eq!(reopened.page().objects, originals);
    reopened.flush()?;
    println!(
        "{}",
        serde_json::json!({"recognized": text, "seconds_including_cold_start": elapsed,
        "review_before_replace": true, "undo_redo_reopen": true, "runtime": "offline"})
    );
    Ok(())
}
