//! The worker uses a private copy of Python outside the installation, so Inno
//! Setup can replace every application/runtime file after all Folio writers exit.
use crate::manifest::Release;
use std::path::Path;
#[cfg(any(windows, test))]
use std::{fs, time::Duration};

// Preparation may unpack thousands of runtime files on a slow disk. Actual
// worker progress extends the inactivity deadline, never the absolute limit.
#[cfg(any(windows, test))]
#[derive(Default)]
struct PreparationProgress {
    sequence: u64,
    last_change: Duration,
}

#[cfg(any(windows, test))]
impl PreparationProgress {
    fn observe(&mut self, elapsed: Duration, sequence: Option<u64>) -> Result<(), &'static str> {
        if elapsed >= Duration::from_secs(30 * 60) {
            return Err("Preparing the update exceeded 30 minutes; Folio has stayed open");
        }
        if let Some(sequence) = sequence
            && sequence > self.sequence
        {
            self.sequence = sequence;
            self.last_change = elapsed;
        }
        if elapsed.saturating_sub(self.last_change) >= Duration::from_secs(120) {
            return Err("The update helper stopped making progress; Folio has stayed open");
        }
        Ok(())
    }
}

#[cfg(any(windows, test))]
fn preparation_sequence(job: &Path) -> Option<u64> {
    use std::io::Read;
    // A partial, malformed, stale or unsupported message must not keep a
    // wedged helper alive. Bound reads even if its file changes after metadata.
    let mut bytes = Vec::new();
    fs::File::open(job.join("progress.json"))
        .ok()?
        .take(4097)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    #[derive(serde::Deserialize)]
    struct Progress {
        protocol: u8,
        sequence: u64,
    }
    let progress: Progress = serde_json::from_slice(&bytes).ok()?;
    (progress.protocol == 1).then_some(progress.sequence)
}

#[cfg(windows)]
pub fn prepare(
    cache: &Path,
    package: &Path,
    release: &Release,
    portable: bool,
    data: &Path,
) -> Result<(), String> {
    use ring::rand::{SecureRandom, SystemRandom};
    use std::time::Instant;
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
    let mut progress = PreparationProgress::default();
    let result = loop {
        if let Err(error) = progress.observe(started.elapsed(), preparation_sequence(&job)) {
            break Err(error.to_owned());
        }
        if let Ok(error) = fs::read_to_string(job.join("error.txt")) {
            break Err(error);
        }
        if job.join("ready").is_file() {
            break Ok(());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Err(format!("Could not prepare the update ({status})")),
            Ok(None) => {}
            Err(error) => break Err(error.to_string()),
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if result.is_err() {
        // Every failed handshake stops the helper before the caller can later
        // close Folio, so an abandoned preparation cannot install in its place.
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}
#[cfg(not(windows))]
pub fn prepare(_: &Path, _: &Path, _: &Release, _: bool, _: &Path) -> Result<(), String> {
    Err("Windows updates require Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_preparation_can_finish_after_the_old_thirty_second_limit() {
        let mut progress = PreparationProgress::default();
        for seconds in 1..=180 {
            assert!(
                progress
                    .observe(Duration::from_secs(seconds), Some(seconds))
                    .is_ok()
            );
        }
    }

    #[test]
    fn missing_replayed_or_regressing_progress_cannot_hide_a_stall() {
        for sequence in [None, Some(7), Some(6)] {
            let mut progress = PreparationProgress::default();
            assert!(progress.observe(Duration::from_secs(30), Some(7)).is_ok());
            assert!(progress.observe(Duration::from_secs(149), sequence).is_ok());
            assert!(
                progress
                    .observe(Duration::from_secs(150), sequence)
                    .is_err()
            );
        }
        assert!(
            PreparationProgress::default()
                .observe(Duration::from_secs(120), None)
                .is_err()
        );
    }

    #[test]
    fn even_continuous_progress_cannot_exceed_the_absolute_preparation_limit() {
        let mut progress = PreparationProgress::default();
        for seconds in 1..1800 {
            assert!(
                progress
                    .observe(Duration::from_secs(seconds), Some(seconds))
                    .is_ok()
            );
        }
        assert!(
            progress
                .observe(Duration::from_secs(1800), Some(1800))
                .is_err()
        );
    }

    #[test]
    fn progress_protocol_parsing_is_versioned_and_bounded() {
        let job = tempfile::tempdir().unwrap();
        let path = job.path().join("progress.json");
        assert_eq!(preparation_sequence(job.path()), None);
        fs::write(&path, r#"{"protocol":1,"sequence":42,"phase":"extract"}"#).unwrap();
        assert_eq!(preparation_sequence(job.path()), Some(42));
        for value in [
            r#"{"protocol":2,"sequence":43}"#,
            r#"{"protocol":1,"sequence":-1}"#,
            "{",
            &" ".repeat(4097),
        ] {
            fs::write(&path, value).unwrap();
            assert_eq!(preparation_sequence(job.path()), None);
        }
    }
}
