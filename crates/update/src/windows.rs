//! The worker uses a private copy of Python outside the installation, so Inno
//! Setup can replace every application/runtime file after all Folio writers exit.
use crate::manifest::Release;
use std::path::Path;

#[cfg(windows)]
pub fn prepare(
    cache: &Path,
    package: &Path,
    release: &Release,
    portable: bool,
    data: &Path,
) -> Result<(), String> {
    use ring::rand::{SecureRandom, SystemRandom};
    use std::{
        fs,
        time::{Duration, Instant},
    };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let root = exe
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid installation directory")?;
    if exe.file_name().is_none_or(|n| n != "folio.exe")
        || exe
            .parent()
            .and_then(Path::file_name)
            .is_none_or(|n| n != "bin")
    {
        return Err(
            "Use the complete Folio installer or portable package to enable updates".into(),
        );
    }
    let data = fs::canonicalize(data).map_err(|e| e.to_string())?;
    let real_root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let real_cache = fs::canonicalize(cache).map_err(|e| e.to_string())?;
    if data.starts_with(&real_root) || real_cache.starts_with(&real_root) {
        return Err("Move your library outside the Folio installation before updating".into());
    }
    let asset = if portable {
        &release.windows_portable
    } else {
        &release.windows_installer
    };
    if !crate::download::verified(package, asset) {
        return Err("The staged update changed; download it again".into());
    }
    let mut random = [0; 16];
    SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| "Could not create an update job")?;
    let name = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let job = cache.join(format!("job-{name}"));
    fs::create_dir(&job).map_err(|e| e.to_string())?;
    let python = root.join("python");
    if !python.join("pythonw.exe").is_file() {
        return Err(
            "The embedded update runtime is missing; reinstall the complete Folio package".into(),
        );
    }
    fs::create_dir(job.join("python")).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(&python).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_file() {
            fs::copy(entry.path(), job.join("python").join(entry.file_name()))
                .map_err(|e| e.to_string())?;
        }
    }
    fs::write(
        job.join("update.py"),
        include_str!("../../../scripts/windows-update-worker.py"),
    )
    .map_err(|e| e.to_string())?;
    let intent = serde_json::json!({"parent_pid": std::process::id(), "root": root, "data": data, "package": package,
        "version": release.version, "binary_sha256": release.windows_binary_sha256, "sha256": asset.sha256, "size": asset.size, "portable": portable});
    fs::write(
        job.join("intent.json"),
        serde_json::to_vec(&intent).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut child = folio_platform::command(job.join("python/pythonw.exe"))
        .arg("-I")
        .arg(job.join("update.py"))
        .arg(job.join("intent.json"))
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(30) {
        if job.join("ready").is_file() {
            return Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(fs::read_to_string(job.join("error.txt"))
                .unwrap_or_else(|_| format!("Could not prepare the update ({status})")));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("The update helper did not become ready; Folio has stayed open".into())
}
#[cfg(not(windows))]
pub fn prepare(_: &Path, _: &Path, _: &Release, _: bool, _: &Path) -> Result<(), String> {
    Err("Windows updates require Windows".into())
}
