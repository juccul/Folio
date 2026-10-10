#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use folio_app::Controller;
use folio_ui::NotesView;
use gpui::*;
use std::{path::PathBuf, time::Duration};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|s| s == "--version" || s == "-V") {
        println!("Folio {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "Folio — offline vector handwriting\n\nUsage: folio [--data-dir PATH] [--new-note] [--open-note UUID] [--recover] [--smoke-test] [PDF/IMAGE…]\n\nData: FOLIO_DATA_DIR or the platform's local application-data directory\nP/E/L/H/T/S: tools · Ctrl+S: save · Ctrl+F: search · Ctrl+0: fit"
        );
        return Ok(());
    }
    let mut data_dir = folio_platform::data_dir();
    let mut explicit_path = std::env::var_os("FOLIO_DATA_DIR").is_some();
    let mut files = vec![];
    let mut smoke = false;
    let mut new_note = false;
    let mut open_note = None;
    let mut recover = false;
    let mut restart_after_update = false;
    let mut args = args.iter().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--data-dir" => {
                explicit_path = true;
                data_dir = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--data-dir requires a path"))?,
                )
            }
            "--open-note" => {
                open_note = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--open-note requires a document UUID"))?
                        .parse()?,
                )
            }
            "--smoke-test" => smoke = true,
            "--new-note" => new_note = true,
            "--recover" => recover = true,
            "--restart-after-update" => restart_after_update = true,
            value if value.starts_with('-') => anyhow::bail!("Unknown option: {value}"),
            value => files.push(PathBuf::from(value)),
        }
    }
    if recover {
        let report = folio_storage::recovery::recover(&data_dir)?;
        println!(
            "Recovered {} notes into {}",
            report.recovered_notes,
            report.destination.display()
        );
        data_dir = report.destination;
    }
    if restart_after_update {
        let start = std::time::Instant::now();
        loop {
            match folio_platform::lock_data_dir(&data_dir) {
                Ok(_) => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < Duration::from_secs(120) =>
                {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(_) => break,
            }
        }
    }
    let result = if !explicit_path
        && data_dir != folio_platform::default_data_dir()
        && !data_dir.join("notes.sqlite3").is_file()
    {
        Err(format!(
            "The chosen startup library is unavailable: {}. Reconnect its drive or open an existing library with --data-dir.",
            data_dir.display()
        ))
    } else {
        Controller::open(data_dir.clone())
    };
    let mut controller = match result {
        Ok(controller) => controller,
        Err(error) => {
            Application::new()
                .with_assets(folio_ui::IconAssets)
                .run(move |cx| {
                    cx.open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                                None,
                                size(px(900.), px(600.)),
                                cx,
                            ))),
                            ..Default::default()
                        },
                        |window, cx| {
                            cx.new(|cx| folio_ui::RecoveryView::new(data_dir, error, window, cx))
                        },
                    )
                    .expect("open recovery window");
                });
            return Ok(());
        }
    };
    if new_note {
        controller.create_note();
    }
    if let Some(id) = open_note {
        if !controller.notes.iter().any(|n| n.id == id) {
            anyhow::bail!("Document not found: {id}");
        }
        controller.switch_note(id);
    }
    let open_editor = smoke || new_note || open_note.is_some() || !files.is_empty();
    for file in files {
        controller.import_as_note(file);
    }
    Application::new().with_assets(folio_ui::IconAssets).run(move |cx| {
        NotesView::bindings(cx);
        let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("io.github.folio.Notes".into()),
            titlebar: Some(TitlebarOptions {title:Some("Folio - Notes".into()), appears_transparent: true, ..Default::default()}),
            window_decorations: Some(WindowDecorations::Client),
            window_min_size: Some(size(px(1000.), px(620.))),
            ..Default::default()
        }, |window,cx|cx.new(|cx| {
            let mut view=NotesView::new(controller,window,cx);
            if open_editor { view.show_editor(); }
            view
        })).expect("Open GPUI window");
        cx.on_window_closed(|cx|cx.quit()).detach();
        cx.activate(true);
        if smoke {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_millis(600)).await;
                let result: anyhow::Result<()> = (async {
                    for phase in 0..3 {
                        window.update(cx, |view, _, cx| { view.update_smoke_phase(phase); cx.notify(); })?;
                        cx.background_executor().timer(Duration::from_millis(200)).await;
                        window.update(cx, |view, _, _| view.update_smoke_verify(phase))?.map_err(anyhow::Error::msg)?;

                    }
                    window.update(cx, |view, _, cx| { view.update_smoke_phase(3); cx.notify(); })?;
                    let events=window.update(cx,|view,_,_|view.smoke_events())?.map_err(anyhow::Error::msg)?;
                    for event in events {
                        cx.update_window(window.into(),|_,window,cx|{window.dispatch_tablet_event(event,cx);})?;
                        cx.background_executor().timer(Duration::from_millis(8)).await;
                    }
                    window.update(cx,|view,_,cx|view.smoke_toolbar_setup(cx))?;
                    let mut toolbar_events=Err("Contextual selection toolbar did not settle".to_owned());
                    for _ in 0..200 {
                        cx.background_executor().timer(Duration::from_millis(50)).await;
                        toolbar_events=window.update(cx,|view,_,_|view.smoke_toolbar_events())?;
                        if toolbar_events.is_ok() { break; }
                    }
                    let events=toolbar_events.map_err(anyhow::Error::msg)?;
                    for event in events {
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_input_event(event,cx))?;
                        cx.background_executor().timer(Duration::from_millis(12)).await;
                    }
                    window.update(cx,|view,_,cx|view.smoke_verify(cx))?.map_err(anyhow::Error::msg)?;
                    let (a,b)=window.update(cx,|view,_,cx| {let ids=view.navigation_smoke_setup();cx.notify();ids})?;
                    cx.background_executor().timer(Duration::from_millis(150)).await;
                    for (label,right) in [("Open Smoke A",true),("Duplicate",false)] {
                        let events=window.update(cx,|view,_,_|view.navigation_smoke_click(label,right))?.map_err(anyhow::Error::msg)?;
                        for event in events {cx.update_window(window.into(),|_,window,cx|window.dispatch_input_event(event,cx))?;cx.background_executor().timer(Duration::from_millis(20)).await;}
                        cx.background_executor().timer(Duration::from_millis(100)).await;
                        if right {window.update(cx,|view,_,_|view.navigation_smoke_context(a))?.map_err(anyhow::Error::msg)?;}
                    }
                    window.update(cx,|view,_,cx| {view.navigation_smoke_open(a);cx.notify();})?;
                    cx.background_executor().timer(Duration::from_millis(150)).await;
                    for label in ["Open or create a document · Ctrl+T","Open Smoke B"] {
                        let events=window.update(cx,|view,_,_|view.navigation_smoke_click(label,false))?.map_err(anyhow::Error::msg)?;
                        for event in events {cx.update_window(window.into(),|_,window,cx|window.dispatch_input_event(event,cx))?;cx.background_executor().timer(Duration::from_millis(20)).await;}
                        cx.background_executor().timer(Duration::from_millis(150)).await;
                    }
                    let events=window.update(cx,|view,_,_|view.navigation_smoke_drag())?.map_err(anyhow::Error::msg)?;
                    for event in events {cx.update_window(window.into(),|_,window,cx|window.dispatch_input_event(event,cx))?;cx.background_executor().timer(Duration::from_millis(20)).await;}
                    cx.background_executor().timer(Duration::from_millis(150)).await;
                    window.update(cx,|view,_,_|view.navigation_smoke_verify(a,b))?.map_err(anyhow::Error::msg)?;
                    window.update(cx,|view,window,cx|view.region_smoke_setup(window,cx))?.map_err(anyhow::Error::msg)?;
                    cx.background_executor().timer(Duration::from_millis(150)).await;
                    let events=window.update(cx,|view,_,_|view.region_smoke_events())?.map_err(anyhow::Error::msg)?;
                    for (index,event) in events.into_iter().enumerate() {
                        cx.update_window(window.into(),|_,window,cx|window.dispatch_input_event(event,cx))?;
                        cx.background_executor().timer(Duration::from_millis(20)).await;
                        if index==1 {window.update(cx,|view,_,_|view.region_smoke_outline())?.map_err(anyhow::Error::msg)?;}
                    }
                    window.update(cx,|view,_,_|view.region_smoke_verify())?.map_err(anyhow::Error::msg)?;
                    for phase in 0..2 {
                        window.update(cx, |view, _, cx| { view.update_smoke_phase(phase); cx.notify(); })?;
                        cx.background_executor().timer(Duration::from_millis(200)).await;
                        window.update(cx, |view, _, _| view.update_smoke_verify(phase))?.map_err(anyhow::Error::msg)?;
                        if phase < 2 {
                            let label = if phase == 0 { "Update · 0.1.4" } else { "Downloading 42%" };
                            let events=window.update(cx,|view,_,_|view.navigation_smoke_click(label,false))?.map_err(anyhow::Error::msg)?;
                            for event in events {
                                cx.update_window(window.into(),|_,window,cx|{window.dispatch_input_event(event,cx);})?;
                                cx.background_executor().timer(Duration::from_millis(20)).await;
                            }
                        }
                    }
                    window.update(cx, |view, _, cx| { view.update_smoke_phase(3); cx.notify(); })?;
                    cx.background_executor().timer(Duration::from_millis(200)).await;
                    let events=window.update(cx,|view,_,_|view.navigation_smoke_click("Library · Ctrl+Shift+L",false))?.map_err(anyhow::Error::msg)?;
                    for event in events {
                        cx.update_window(window.into(),|_,window,cx|{window.dispatch_input_event(event,cx);})?;
                        cx.background_executor().timer(Duration::from_millis(20)).await;
                    }
                    window.update(cx, |view, _, cx| view.titlebar_smoke_verify_home(cx))?.map_err(anyhow::Error::msg)?;
                    cx.background_executor().timer(Duration::from_millis(200)).await;
                    Ok(())
                }).await;
                match result {
                    Ok(())=>println!("FOLIO_SMOKE_OK: native event dispatch, pressure/tilt, vector canvas, undo/redo, pages, right-click card menu, duplication, tab picker, native tab drag, drawn crop outline/undo, durable save"),
                    Err(error)=>{eprintln!("FOLIO_SMOKE_FAILED: {error:#}");std::process::exit(1)}
                }
                cx.background_executor().timer(Duration::from_millis(300)).await;
                let _=cx.update(|cx|cx.quit());
            }).detach();
        }
    });
    Ok(())
}
