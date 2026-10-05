//! Linux paths and bounded offline subprocesses; no shell or network operations.
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("FOLIO_DATA_DIR") {
        return PathBuf::from(p);
    }
    if let Some(p) = std::env::var_os("XDG_DATA_HOME").filter(|p| Path::new(p).is_absolute()) {
        PathBuf::from(p).join("folio")
    } else {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/folio")
    }
}
pub fn command_available(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(program).is_file()))
}
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
pub fn run(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
) -> std::io::Result<Output> {
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let read = |reader: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut data = Vec::new();
            reader.take(8_000_000).read_to_end(&mut data).map(|_| data)
        })
    };
    let stdout = read(Box::new(out));
    let stderr = read(Box::new(err));
    if let Some(input) = input
        && let Some(mut stdin) = child.stdin.take()
        && let Err(e) = stdin.write_all(input)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Offline worker exceeded its time limit",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout
        .join()
        .map_err(|_| std::io::Error::other("stdout worker failed"))??;
    let stderr = stderr
        .join()
        .map_err(|_| std::io::Error::other("stderr worker failed"))??;
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "Offline tool exited with {status}: {}",
            String::from_utf8_lossy(&stderr)
        )));
    }
    Ok(Output { stdout, stderr })
}
pub fn copy_asset(
    source: &Path,
    root: &Path,
    id: &str,
    extension: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let target = root.join(format!("{id}.{extension}"));
    let temporary = root.join(format!(
        ".asset-{}",
        format_args!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    ));
    let result = (|| {
        std::fs::copy(source, &temporary)?;
        std::fs::File::open(&temporary)?.sync_all()?;
        std::fs::rename(&temporary, &target)?;
        std::fs::File::open(root)?.sync_all()?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    Ok(target)
}

/// Keep one writer process per data directory. Dropping the file releases the lock.
pub fn lock_data_dir(root: &Path) -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("session.lock"))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: flock receives an owned, live descriptor and no pointers.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Err(std::io::Error::new(
                    error.kind(),
                    "Folio is already running for this data directory",
                ));
            }
            return Err(error);
        }
    }
    Ok(file)
}
