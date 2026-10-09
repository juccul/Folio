//! Run only inside a disposable, signed Flatpak installation during validation.
#[cfg(target_os = "linux")]
fn main() -> Result<(), String> {
    use std::sync::{Arc, atomic::AtomicBool};
    let args = std::env::args().collect::<Vec<_>>();
    let expected = args
        .get(1)
        .ok_or("Pass the expected signed OSTree commit")?;
    let monitor =
        folio_update::flatpak::download(expected, &Arc::new(AtomicBool::new(false)), |progress| {
            println!("PROGRESS {progress}")
        })?;
    println!("FLATPAK_UPDATE_READY");
    if let Some(data) = args.get(2) {
        monitor.restart(std::path::Path::new(data))?;
        println!("FLATPAK_RESTART_STARTED");
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {}
