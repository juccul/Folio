#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use folio_app::Controller;
use folio_ui::NotesView;
use std::{path::PathBuf, time::Duration};

/// Rune's own flags (headless snapshots, scripts, element trees), passed
/// through to it; each name maps to whether it takes a value.
const RUNE_FLAGS: &[(&str, bool)] = &[
    ("--snap", true),
    ("--size", true),
    ("--scale", true),
    ("--theme", true),
    ("--scene", true),
    ("--at", true),
    ("--script", true),
    ("--frames", true),
    ("--out", true),
    ("--tree", true),
    ("--trace", true),
    ("--start", true),
    ("--every", true),
    ("--cols", true),
    ("--strip", true),
    ("--crop", true),
    ("--video", true),
    ("--fps", true),
    ("--duration", true),
    ("--window", true),
    ("--list-scenes", false),
    ("--native", false),
    ("--keys", false),
    ("--pointer", false),
    ("--chrome", false),
    ("--first-pixels", false),
];

fn main() -> anyhow::Result<()> {
    folio_platform::tune_allocator();
    let startup = std::time::Instant::now();
    let profile_startup = std::env::var_os("FOLIO_PROFILE_STARTUP").is_some();
    let mark = move |stage: &str| {
        if profile_startup {
            eprintln!(
                "FOLIO_STARTUP {stage}: {:.3} ms",
                startup.elapsed().as_secs_f64() * 1000.
            );
        }
    };
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|s| s == "--version" || s == "-V") {
        println!("Folio {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "Folio — offline vector handwriting\n\nUsage: folio [--data-dir PATH] [--new-note] [--open-note UUID] [--recover] [--smoke-titlebar] [PDF/IMAGE…]\n\nData: FOLIO_DATA_DIR or the platform's local application-data directory\n--smoke-titlebar: isolated pointer fixture; requires an explicit, empty --data-dir. Ctrl+Alt+U resets its simulated update.\nP/E/L/H/T/S: tools · Ctrl+S: save · Ctrl+F: search · Ctrl+0: fit"
        );
        return Ok(());
    }
    let mut data_dir = folio_platform::data_dir();
    let mut explicit_path = std::env::var_os("FOLIO_DATA_DIR").is_some();
    let mut files = vec![];
    let mut rune_args = Vec::new();
    let mut titlebar_smoke = false;
    let mut explicit_cli_path = false;
    let mut new_note = false;
    let mut open_note = None;
    let mut recover = false;
    let mut restart_after_update = false;
    let mut args = args.iter().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--data-dir" => {
                explicit_cli_path = true;
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
            value if RUNE_FLAGS.iter().any(|(name, _)| *name == value) => {
                rune_args.push(value.to_owned());
                if RUNE_FLAGS
                    .iter()
                    .any(|(name, takes)| *name == value && *takes)
                {
                    rune_args.push(
                        args.next()
                            .ok_or_else(|| anyhow::anyhow!("{value} requires a value"))?
                            .clone(),
                    );
                }
            }
            "--smoke-titlebar" => titlebar_smoke = true,
            "--new-note" => new_note = true,
            "--recover" => recover = true,
            "--restart-after-update" => restart_after_update = true,
            value if value.starts_with('-') => anyhow::bail!("Unknown option: {value}"),
            value => files.push(PathBuf::from(value)),
        }
    }
    if titlebar_smoke {
        anyhow::ensure!(
            explicit_cli_path,
            "--smoke-titlebar requires an explicit --data-dir"
        );
        anyhow::ensure!(
            !new_note
                && open_note.is_none()
                && !recover
                && !restart_after_update
                && files.is_empty(),
            "--smoke-titlebar cannot be combined with other startup actions"
        );
        validate_titlebar_fixture_directory(&data_dir)?;
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
    // Fixtures and recovery stay isolated; update restarts skip activating the
    // exiting parent process, but still listen for future launches once running.
    let headless = !rune_args.is_empty() && !rune_args.iter().any(|a| a == "--native");
    let listen_single_instance = !(titlebar_smoke || recover || headless);
    if listen_single_instance && !restart_after_update && activate_running_instance(&data_dir) {
        return Ok(());
    }
    let open_editor = titlebar_smoke || new_note || open_note.is_some() || !files.is_empty();
    // Opening the library (integrity check, settings, first document) and
    // the window's GPU setup are independent; overlap them.
    let library = {
        let data_dir = data_dir.clone();
        std::thread::Builder::new()
            .name("folio-startup".into())
            .spawn(move || {
            if !explicit_path
                && data_dir != folio_platform::default_data_dir()
                && !data_dir.join("notes.sqlite3").is_file()
            {
                Err(format!(
                    "The chosen startup library is unavailable: {}. Reconnect its drive or open an existing library with --data-dir.",
                    data_dir.display()
                ))
            } else {
                Controller::open_for_ui(data_dir, open_editor)
            }
        })?
    };
    mark("before_platform");
    rune::prewarm_gpu();
    mark("platform_ready");
    let result = library
        .join()
        .unwrap_or_else(|_| Err("Opening the library failed unexpectedly.".into()));
    mark("library_ready");
    let mut controller = match result {
        Ok(controller) => controller,
        Err(error) => {
            folio_ui::run_recovery(data_dir, error);
            return Ok(());
        }
    };
    if new_note || titlebar_smoke {
        controller.create_note();
    }
    if let Some(id) = open_note {
        if !controller.notes.iter().any(|n| n.id == id) {
            anyhow::bail!("Document not found: {id}");
        }
        controller.switch_note(id);
    }
    if titlebar_smoke {
        println!(
            "FOLIO_TITLEBAR_FIXTURE: simulated updates only; library {}",
            data_dir.display()
        );
    }
    for file in files {
        controller.import_as_note(file);
    }
    let view = if titlebar_smoke {
        NotesView::new_titlebar_smoke(controller)
    } else {
        NotesView::new(controller)
    };
    mark("view_ready");
    folio_ui::run(view, open_editor, rune_args, move |handle| {
        mark("event_loop");
        if listen_single_instance {
            let listening = folio_platform::listen_for_activation(&data_dir, move |token| {
                match token {
                    Some(token) => handle.activate(token),
                    None => handle.open(),
                }
                true
            });
            if let Err(error) = listening {
                eprintln!("Folio could not accept activation from later launches: {error}");
            }
        }
    });
    Ok(())
}

/// Hand this launch to the instance already running for `data_dir`. While
/// another process holds the library lock but is not answering yet (starting
/// or exiting), keep retrying briefly instead of opening a broken window.
fn activate_running_instance(data_dir: &std::path::Path) -> bool {
    let token = ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|token| !token.is_empty()));
    let start = std::time::Instant::now();
    loop {
        if folio_platform::activate_running_instance(data_dir, token.as_deref()) {
            return true;
        }
        match folio_platform::lock_data_dir(data_dir) {
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && start.elapsed() < Duration::from_secs(10) =>
            {
                std::thread::sleep(Duration::from_millis(100))
            }
            _ => return false,
        }
    }
}

fn validate_titlebar_fixture_directory(path: &std::path::Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "--smoke-titlebar requires a fresh directory, not a file or symlink"
            );
            anyhow::ensure!(
                std::fs::read_dir(path)?.next().is_none(),
                "--smoke-titlebar refuses an existing library or nonempty directory"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(test)]
mod titlebar_fixture_tests {
    use super::validate_titlebar_fixture_directory;

    #[test]
    fn fixture_rejects_existing_content_before_opening_a_library() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "folio-titlebar-fixture-{}-{unique}",
            std::process::id()
        ));
        assert!(validate_titlebar_fixture_directory(&directory).is_ok());
        std::fs::create_dir(&directory).unwrap();
        assert!(validate_titlebar_fixture_directory(&directory).is_ok());
        let library = directory.join("notes.sqlite3");
        std::fs::write(&library, b"existing library remains untouched").unwrap();
        assert!(validate_titlebar_fixture_directory(&directory).is_err());
        assert!(validate_titlebar_fixture_directory(&library).is_err());
        assert_eq!(
            std::fs::read(&library).unwrap(),
            b"existing library remains untouched"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
