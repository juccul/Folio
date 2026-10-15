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
    remembered_library(&default_data_dir().join("startup-library.json"))
        .unwrap_or_else(default_data_dir)
}
pub fn default_data_dir() -> PathBuf {
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

fn remembered_library(config: &Path) -> Option<PathBuf> {
    serde_json::from_slice::<PathBuf>(&std::fs::read(config).ok()?)
        .ok()
        .filter(|p| p.is_absolute())
}
fn remember_library(config: &Path, library: &Path) -> std::io::Result<()> {
    let library = std::fs::canonicalize(library)?;
    if !library.join("notes.sqlite3").is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Choose an existing Folio library",
        ));
    }
    let parent = config
        .parent()
        .ok_or_else(|| std::io::Error::other("Invalid preference path"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = config.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(
        &temporary,
        serde_json::to_vec(&library).map_err(std::io::Error::other)?,
    )?;
    publish_file(&temporary, config)
}
pub fn use_library_by_default(library: &Path) -> std::io::Result<()> {
    remember_library(&default_data_dir().join("startup-library.json"), library)
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
        #[cfg(windows)]
        {
            spawn_windows_worker(self, None)
        }
        #[cfg(not(windows))]
        {
            self.spawn()
        }
    }
}

#[cfg(windows)]
fn new_windows_worker_job() -> Result<usize, u32> {
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
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
    }
}

#[cfg(windows)]
fn windows_worker_job() -> std::io::Result<usize> {
    use std::sync::OnceLock;
    // Windows closes this process-owned handle even on an app crash, killing
    // all assigned workers and their descendants. Rust statics live until exit.
    static JOB: OnceLock<Result<usize, u32>> = OnceLock::new();
    let job = JOB.get_or_init(new_windows_worker_job);
    job.as_ref()
        .copied()
        .map_err(|code| std::io::Error::from_raw_os_error(*code as i32))
}

#[cfg(windows)]
fn spawn_windows_worker(
    command: &mut Command,
    call_job: Option<usize>,
) -> std::io::Result<std::process::Child> {
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    use windows_sys::Win32::System::{
        JobObjects::AssignProcessToJobObject,
        Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED},
    };
    let app_job = windows_worker_job()?;
    // Assign both jobs before the launcher can run, spawn a helper, or exit.
    // Restore the builder flags even if spawning fails so it remains reusable.
    command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    let child = command.spawn();
    configure_command(command);
    let mut child = child?;
    let result = (|| {
        for job in std::iter::once(app_job).chain(call_job) {
            // SAFETY: the job and suspended child's process handle are live.
            if unsafe { AssignProcessToJobObject(job as *mut _, child.as_raw_handle()) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        resume_windows_worker(child.id())
    })();
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}

#[cfg(windows)]
fn resume_windows_worker(pid: u32) -> std::io::Result<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{ERROR_NO_MORE_FILES, FILETIME, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::*,
            Threading::{
                GetThreadTimes, OpenThread, ResumeThread, THREAD_QUERY_LIMITED_INFORMATION,
                THREAD_SUSPEND_RESUME,
            },
        },
    };
    // Stable Rust doesn't expose Child's primary thread handle. A suspended
    // process's earliest-created thread is its primary thread; injected helper
    // threads, if present, must not be mistaken for it.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let snapshot = OwnedHandle::from_raw_handle(snapshot);
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut primary: Option<(u64, OwnedHandle)> = None;
        let mut found = Thread32First(snapshot.as_raw_handle(), &mut entry);
        while found != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(
                    THREAD_QUERY_LIMITED_INFORMATION | THREAD_SUSPEND_RESUME,
                    0,
                    entry.th32ThreadID,
                );
                if thread.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                let thread = OwnedHandle::from_raw_handle(thread);
                let mut created: FILETIME = std::mem::zeroed();
                let mut exited: FILETIME = std::mem::zeroed();
                let mut kernel: FILETIME = std::mem::zeroed();
                let mut user: FILETIME = std::mem::zeroed();
                if GetThreadTimes(
                    thread.as_raw_handle(),
                    &mut created,
                    &mut exited,
                    &mut kernel,
                    &mut user,
                ) == 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                let created = (created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64;
                if primary
                    .as_ref()
                    .is_none_or(|(earliest, _)| created < *earliest)
                {
                    primary = Some((created, thread));
                }
            }
            found = Thread32Next(snapshot.as_raw_handle(), &mut entry);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
            return Err(error);
        }
        let (_, primary) = primary
            .ok_or_else(|| std::io::Error::other("Suspended worker has no primary thread"))?;
        if ResumeThread(primary.as_raw_handle()) == u32::MAX {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Offline calls own their descendants as well as their direct child. Inherited
/// pipes must close before joining the input/output threads, including when a
/// launcher exits successfully while one of its helpers remains alive.
struct WorkerGroup {
    #[cfg(unix)]
    pid: u32,
    #[cfg(windows)]
    job: Option<usize>,
}
impl WorkerGroup {
    fn spawn(command: &mut Command) -> std::io::Result<(std::process::Child, Self)> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        let group = Self {
            job: Some(
                new_windows_worker_job()
                    .map_err(|code| std::io::Error::from_raw_os_error(code as i32))?,
            ),
        };
        #[cfg(windows)]
        let child = spawn_windows_worker(command, group.job)?;
        #[cfg(not(windows))]
        let child = command.spawn_background()?;
        #[cfg(not(windows))]
        let group = Self {
            #[cfg(unix)]
            pid: child.id(),
        };
        Ok((child, group))
    }
}
impl Drop for WorkerGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: process_group(0) gives this call its own child-led group.
        unsafe {
            libc::killpg(self.pid as libc::pid_t, libc::SIGKILL);
        }
        #[cfg(windows)]
        if let Some(job) = self.job.take() {
            // KILL_ON_JOB_CLOSE closes inherited pipes even after the root exits.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job as *mut _) };
        }
    }
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
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (mut child, group) = WorkerGroup::spawn(command)?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let read = |mut reader: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut data = Vec::new();
            reader.by_ref().take(8_000_000).read_to_end(&mut data)?;
            // Retain bounded diagnostics while continuing to drain the pipe.
            // Closing it early can abort or stall an otherwise successful tool.
            std::io::copy(&mut reader, &mut std::io::sink())?;
            Ok::<_, std::io::Error>(data)
        })
    };
    let stdout = read(Box::new(out));
    let stderr = read(Box::new(err));
    let start = Instant::now();
    let status = thread::scope(|scope| {
        // A worker that stops reading must not hold the caller indefinitely in
        // write_all before the deadline even starts. Borrow input without a copy.
        let writer = input.map(|input| {
            let mut stdin = child.stdin.take().unwrap();
            scope.spawn(move || stdin.write_all(input))
        });
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Err(error) => break Err(error),
                Ok(None) if start.elapsed() >= timeout => {
                    break Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Offline worker exceeded its time limit",
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
            }
        };
        if status.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        drop(group);
        let written = writer
            .map(|writer| {
                writer
                    .join()
                    .map_err(|_| std::io::Error::other("stdin worker failed"))?
            })
            .unwrap_or(Ok(()));
        // Prefer the deadline/process error over the resulting broken pipe.
        let status = status?;
        written?;
        Ok::<_, std::io::Error>(status)
    });
    let stdout = stdout
        .join()
        .map_err(|_| std::io::Error::other("stdout worker failed"))??;
    let stderr = stderr
        .join()
        .map_err(|_| std::io::Error::other("stderr worker failed"))??;
    let status = status?;
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
    #[test]
    #[ignore = "subprocess fixture launched by the deadline/output regression"]
    #[allow(clippy::zombie_processes)] // Deliberate orphan; the caller owns and kills its process group/job.
    fn subprocess_fixture() {
        match std::env::var("FOLIO_SUBPROCESS_FIXTURE").as_deref() {
            Ok("blocked") => thread::sleep(Duration::from_secs(5)),
            #[cfg(windows)]
            Ok("background-owner") => {
                let child = command(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "tests::subprocess_fixture",
                        "--nocapture",
                    ])
                    .env("FOLIO_SUBPROCESS_FIXTURE", "blocked")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn_background()
                    .unwrap();
                println!("WORKER_PID={}", child.id());
                std::io::stdout().flush().unwrap();
                thread::sleep(Duration::from_secs(5));
            }
            Ok(mode @ ("descendant-output" | "descendant-blocked" | "descendant-fast")) => {
                let mut child = command(std::env::current_exe().unwrap());
                child
                    .args([
                        "--ignored",
                        "--exact",
                        "tests::subprocess_fixture",
                        "--nocapture",
                    ])
                    .env("FOLIO_SUBPROCESS_FIXTURE", "blocked")
                    .stderr(Stdio::null());
                // Inherit the pipe while the launcher either exits or blocks.
                let _child = child.spawn().unwrap();
                if mode == "descendant-output" {
                    std::io::stdout()
                        .lock()
                        .write_all(&vec![b'x'; 9_000_000])
                        .unwrap();
                } else if mode == "descendant-blocked" {
                    thread::sleep(Duration::from_secs(5));
                }
            }
            Ok("output") => std::io::stdout()
                .lock()
                .write_all(&vec![b'x'; 9_000_000])
                .unwrap(),
            _ => panic!("subprocess fixture needs a mode"),
        }
    }
    #[test]
    fn offline_deadline_includes_blocked_input_and_output_remains_bounded() {
        let fixture = |mode| {
            let mut command = command(std::env::current_exe().unwrap());
            command
                .args([
                    "--ignored",
                    "--exact",
                    "tests::subprocess_fixture",
                    "--nocapture",
                ])
                .env("FOLIO_SUBPROCESS_FIXTURE", mode);
            command
        };
        let start = Instant::now();
        let error = run(
            &mut fixture("blocked"),
            Some(&vec![0; 1_000_000]),
            Duration::from_millis(100),
        )
        .err()
        .expect("blocked input must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(3));
        let output = run(&mut fixture("output"), None, Duration::from_secs(10)).unwrap();
        assert_eq!(output.stdout.len(), 8_000_000);
        assert!(output.stdout.ends_with(b"xxxx"));
        let start = Instant::now();
        let output = run(
            &mut fixture("descendant-output"),
            None,
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(output.stdout.len(), 8_000_000);
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "A successful launcher left its output pipe open"
        );
        let start = Instant::now();
        let error = run(
            &mut fixture("descendant-blocked"),
            Some(&vec![0; 1_000_000]),
            Duration::from_millis(100),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "A descendant held stdin/stdout open past the deadline"
        );
        // A fast launcher must belong to its cleanup group/job before it can
        // fork a pipe-inheriting helper or exit. Reuse the same command builder.
        let mut fast = fixture("descendant-fast");
        for _ in 0..16 {
            let start = Instant::now();
            run(&mut fast, None, Duration::from_secs(2)).unwrap();
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "A fast launcher escaped descendant cleanup"
            );
        }
        // A failed spawn must also leave the command builder usable.
        fast.current_dir(directory("worker-cwd"));
        let cwd = fast.get_current_dir().unwrap().to_path_buf();
        std::fs::remove_dir(&cwd).unwrap();
        assert!(run(&mut fast, None, Duration::from_secs(2)).is_err());
        std::fs::create_dir(&cwd).unwrap();
        run(&mut fast, None, Duration::from_secs(2)).unwrap();
        std::fs::remove_dir(cwd).unwrap();
        #[cfg(windows)]
        {
            use std::{
                io::BufRead,
                os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
            };
            use windows_sys::Win32::{
                Foundation::WAIT_OBJECT_0,
                System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
            };
            // Launch a separate app owner without our jobs, then crash it.
            // Its long-lived background worker must die when its job closes.
            let mut owner = fixture("background-owner")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let output = std::io::BufReader::new(owner.stdout.take().unwrap());
            let pid: u32 = output
                .lines()
                .map(Result::unwrap)
                .find_map(|line| line.strip_prefix("WORKER_PID=").map(str::to_owned))
                .expect("Worker owner did not start its child")
                .parse()
                .unwrap();
            let worker = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            assert!(!worker.is_null(), "Worker exited before its owner");
            let worker = unsafe { OwnedHandle::from_raw_handle(worker) };
            owner.kill().unwrap();
            owner.wait().unwrap();
            assert_eq!(
                unsafe { WaitForSingleObject(worker.as_raw_handle(), 2000) },
                WAIT_OBJECT_0,
                "Crashing the app left its background worker alive"
            );
        }
    }
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
    fn remembered_library_survives_restart_and_failed_replacement() {
        let root = directory("startup");
        let library = root.join("recovered");
        std::fs::create_dir(&library).unwrap();
        std::fs::write(library.join("notes.sqlite3"), b"test").unwrap();
        let config = root.join("startup-library.json");
        remember_library(&config, &library).unwrap();
        assert_eq!(
            remembered_library(&config),
            Some(std::fs::canonicalize(&library).unwrap())
        );
        assert!(remember_library(&config, &root.join("missing")).is_err());
        assert_eq!(
            remembered_library(&config),
            Some(std::fs::canonicalize(&library).unwrap())
        );
        std::fs::remove_dir_all(root).unwrap();
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
