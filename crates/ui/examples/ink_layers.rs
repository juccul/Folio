//! Native isolated rendering smoke: cargo run --release -p folio-ui --example ink_layers
//! Generated notes only; no installed updater cache or input-system changes.
use folio_app::Controller;
use folio_document::*;
use folio_ui::NotesView;
use gpui::*;
use std::{sync::Arc, time::Duration};

fn main() {
    let count: usize = std::env::var("FOLIO_INK_FIXTURE_STROKES")
        .ok()
        .map(|value| value.parse().expect("invalid stroke count"))
        .unwrap_or(1024);
    assert!((1024..=4096).contains(&count) && count.is_multiple_of(32));
    let root = std::env::temp_dir().join(format!("folio-ink-layers-{}", Id::new_v4()));
    let mut controller = Controller::open(root.clone()).unwrap();
    controller.set_autosave(false);
    controller.session_mut().selection.clear();
    controller.session_mut().viewport.pan = folio_document::Point::new(30., 30.);
    let page = controller.page().id;
    let changes = (0..count)
        .map(|i| {
            let mut builder = folio_ink::StrokeBuilder::new(PenStyle::default());
            for j in 0..24 {
                builder.push(StrokePoint::new(
                    folio_document::Point::new(
                        (i % 64) as f32 * 9. + j as f32 * 0.25,
                        (i / 64) as f32 * 8. + (j as f32 * 0.3).sin() * 3.,
                    ),
                    0.7,
                    j * 8,
                ));
            }
            let object = Arc::new(Object::Stroke(builder.finish().unwrap()));
            Change::Object {
                page,
                id: object.id(),
                before: None,
                after: Some(object),
                index: i,
            }
        })
        .collect();
    controller.commit("Fixture", changes);
    let note = controller.active;
    Application::new().with_assets(folio_ui::IconAssets).run(move |cx| {
        let window=cx.open_window(WindowOptions {window_bounds:Some(WindowBounds::Windowed(Bounds::new(point(px(0.),px(0.)),size(px(1000.),px(800.))))),..Default::default()},|window,cx| {
            cx.new(|cx| {let mut view=NotesView::new_titlebar_smoke(controller,window,cx);view.navigation_smoke_open(note);view})
        }).unwrap();
        cx.activate(true);
        cx.spawn(async move |cx| {
            let result:Result<(),Box<dyn std::error::Error>>=async {
                let mut ready=false;
                for _ in 0..200 {
                    cx.background_executor().timer(Duration::from_millis(50)).await;
                    let (_,strokes)=window.update(cx,|view,_,_|view.smoke_ink_layers(None))?;
                    if strokes==count {ready=true;break;}
                }
                if !ready { return Err("Ink batches did not settle".into()); }
                window.update(cx,|view,_,cx| {view.controller.select_all();cx.notify();})?;
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let (layers,strokes)=window.update(cx,|view,_,_|view.smoke_ink_layers(None))?;
                if layers!=0 || strokes!=0 { return Err("Selected ink remained raster-cached".into()); }
                window.update(cx,|view,_,cx| {view.controller.session_mut().selection.clear();view.controller.settings.appearance.adapt_ink=true;view.controller.session_mut().viewport.zoom=1.25;cx.notify();})?;
                for _ in 0..200 {
                    cx.background_executor().timer(Duration::from_millis(50)).await;
                    let (layers,strokes)=window.update(cx,|view,_,_|view.smoke_ink_layers(None))?;
                    if strokes==count {println!("FOLIO_INK_CACHE_OK {count} strokes / {layers} layers; default caching, selection, theme and zoom invalidation passed");return Ok(());}
                }
                Err("Ink batches did not recover after theme/selection changes".into())
            }.await;
            if let Err(error)=result {eprintln!("FOLIO_INK_CACHE_FAILED {error:#}");std::process::exit(1);}
            cx.update(|cx|cx.quit()).ok();
        }).detach();
    });
    std::fs::remove_dir_all(root).unwrap();
}
