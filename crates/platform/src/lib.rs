//! Native data paths, durable file publication, and bounded offline subprocesses.
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
    #[cfg(windows)]
    {
        return windows_data_dir().join("Folio");
    }
    #[cfg(not(windows))]
    if let Some(p) = std::env::var_os("XDG_DATA_HOME").filter(|p| Path::new(p).is_absolute()) {
        PathBuf::from(p).join("folio")
    } else {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/folio")
    }
}

#[cfg(windows)]
fn windows_data_dir() -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
    };
    let mut path = std::ptr::null_mut();
    // SAFETY: the API owns the returned nul-terminated buffer until CoTaskMemFree.
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, std::ptr::null_mut(), &mut path) };
    if result >= 0 && !path.is_null() {
        let value = unsafe {
            let mut length = 0;
            while *path.add(length) != 0 {
                length += 1;
            }
            let value = std::ffi::OsString::from_wide(std::slice::from_raw_parts(path, length));
            CoTaskMemFree(path.cast());
            value
        };
        return value.into();
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData/Local")))
        .expect("Windows did not provide a local application-data directory; use --data-dir")
}

/// Find resources beside the executable or at the root of a bin/ installation.
pub fn bundled_resource(relative: impl AsRef<Path>) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let directory = exe.parent()?;
    [Some(directory), directory.parent()]
        .into_iter()
        .flatten()
        .map(|root| root.join(relative.as_ref()))
        .find(|path| path.is_file())
}

fn executable_name(program: &str) -> std::ffi::OsString {
    #[cfg(windows)]
    if Path::new(program).extension().is_none() {
        return format!("{program}.exe").into();
    }
    program.into()
}

pub fn tool_path(program: &str) -> PathBuf {
    let name = executable_name(program);
    for relative in [
        PathBuf::from("bin").join(&name),
        PathBuf::from("pdf/bin").join(&name),
    ] {
        if let Some(path) = bundled_resource(relative) {
            return path;
        }
    }
    name.into()
}

/// Background tools must not create a console window in the Windows GUI app.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    configure_command(&mut command);
    command
}

pub fn configure_command(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub fn command_available(program: &str) -> bool {
    let resolved = tool_path(program);
    if resolved.is_absolute() {
        return resolved.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(&resolved).is_file()))
}

pub trait BackgroundCommand {
    fn spawn_background(&mut self) -> std::io::Result<std::process::Child>;
}
impl BackgroundCommand for Command {
    fn spawn_background(&mut self) -> std::io::Result<std::process::Child> {
        configure_command(self);
        let child = self.spawn()?;
        #[cfg(windows)]
        let mut child = child;
        #[cfg(windows)]
        if let Err(error) = own_windows_worker(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(child)
    }
}

#[cfg(windows)]
fn own_windows_worker(child: &std::process::Child) -> std::io::Result<()> {
    use std::{os::windows::io::AsRawHandle, sync::OnceLock};
    use windows_sys::Win32::System::JobObjects::*;
    // Windows closes this process-owned handle even on an app crash, killing
    // all assigned workers and their descendants. Rust statics live until exit.
    static JOB: OnceLock<Result<usize, u32>> = OnceLock::new();
    let job = JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(1) as u32);
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
        {
            let error = std::io::Error::last_os_error().raw_os_error().unwrap_or(1) as u32;
            windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(error);
        }
        Ok(job as usize)
    });
    let job = job
        .as_ref()
        .map_err(|code| std::io::Error::from_raw_os_error(*code as i32))?;
    // SAFETY: both the process handle and the process-lifetime job are live.
    if unsafe { AssignProcessToJobObject(*job as *mut _, child.as_raw_handle()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
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
    configure_command(command);
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_background()?;
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
        // Asset copies are owned by Folio. Do not inherit a source PDF/image's
        // read-only attribute; Windows requires write access when flushing it.
        let mut input = std::fs::File::open(source)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        std::io::copy(&mut input, &mut output)?;
        drop(output);
        publish_file(&temporary, &target)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    Ok(target)
}

#[derive(Debug)]
pub struct DataDirLock(std::fs::File);
impl Drop for DataDirLock {
    fn drop(&mut self) {
        // Explicit unlock also releases transient fork-inherited descriptors in
        // other workers before exec closes them. Closing our descriptor alone
        // can otherwise delay a same-process reopen under concurrent spawning.
        let _ = self.0.unlock();
    }
}

/// Keep one writer process per data directory for the lifetime of the guard.
pub fn lock_data_dir(root: &Path) -> std::io::Result<DataDirLock> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("session.lock"))?;
    file.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "Folio is already running for this data directory",
        ),
        std::fs::TryLockError::Error(error) => error,
    })?;
    Ok(DataDirLock(file))
}

/// Flush a completed temporary file and publish it on the same filesystem.
/// Callers must close writers before calling this function. On Unix we also
/// fsync the parent directory; Windows uses a write-through MoveFileEx instead.
pub fn publish_file(temporary: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(temporary)?
        .sync_all()?;
    #[cfg(not(windows))]
    std::fs::rename(temporary, target)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let wide = |path: &Path| -> std::io::Result<Vec<u16>> {
            let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
            if value.contains(&0) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Path contains a nul character",
                ));
            }
            value.push(0);
            Ok(value)
        };
        let source = wide(temporary)?;
        let destination = wide(target)?;
        // SAFETY: both paths remain valid nul-terminated UTF-16 buffers.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    sync_directory(
        target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
}

pub fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(not(windows))]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(windows)]
    let _ = path; // Windows has no equivalent to opening a directory for fsync.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "folio-platform-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn importing_read_only_assets_preserves_source_and_publishes_owned_copy() {
        let root = directory("read-only");
        let source = root.join("source.pdf");
        std::fs::write(&source, b"read-only source").unwrap();
        let mut permissions = std::fs::metadata(&source).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&source, permissions.clone()).unwrap();
        let target = copy_asset(&source, &root, "owned", "pdf").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"read-only source");
        assert!(!std::fs::metadata(target).unwrap().permissions().readonly());
        assert!(std::fs::metadata(&source).unwrap().permissions().readonly());
        #[cfg(windows)]
        {
            permissions.set_readonly(false);
            std::fs::set_permissions(source, permissions).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lock_rejects_second_writer_and_releases_on_drop() {
        let root = directory("lock");
        let first = lock_data_dir(&root).unwrap();
        assert_eq!(
            lock_data_dir(&root).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        drop(first);
        drop(lock_data_dir(&root).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn owner_drop_releases_lock_even_with_an_inherited_handle() {
        let root = directory("inherited-lock");
        let owner = lock_data_dir(&root).unwrap();
        let inherited = owner.0.try_clone().unwrap();
        drop(owner);
        drop(lock_data_dir(&root).unwrap());
        drop(inherited);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn publication_replaces_existing_file_and_preserves_it_on_failure() {
        let root = directory("publish");
        let target = root.join("résultat with spaces.txt");
        let temporary = root.join("staged.tmp");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&temporary, b"new").unwrap();
        publish_file(&temporary, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!temporary.exists());
        assert!(publish_file(&temporary, &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        std::fs::remove_dir_all(root).unwrap();
    }
}
