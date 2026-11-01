//! Single-instance handoff for one data directory.
//!
//! The process holding the data-directory lock listens on a per-library local
//! endpoint (a Unix socket in the private runtime directory, or a local named
//! pipe on Windows). A later launch for the same library asks it to activate
//! its window and exits instead of opening a second window. The data-directory
//! lock stays authoritative: only its owner binds the endpoint, so a socket left
//! by a crashed process is simply replaced.
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    time::Duration,
};

const REQUEST: &str = "folio-activate";
const REPLY: &str = "ok";
const TIMEOUT: Duration = Duration::from_secs(3);

/// Ask the instance that owns `root` to activate its window. Returns `true`
/// only when that instance confirmed the request; `false` means no running
/// instance answered and the caller should start normally.
pub fn activate_running_instance(root: &Path, activation_token: Option<&str>) -> bool {
    let Ok(name) = endpoint(root) else {
        return false;
    };
    let Ok(mut stream) = connect(&name) else {
        return false;
    };
    let token: String = activation_token
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if writeln!(stream, "{REQUEST} {token}").is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(stream.take(64))
        .read_line(&mut reply)
        .is_ok_and(|_| reply.trim_end() == REPLY)
}

/// Accept activation requests for `root` on a background thread. The caller
/// must hold the data-directory lock. `activate` receives the launcher's
/// activation token and returns whether the window was activated.
pub fn listen_for_activation(
    root: &Path,
    activate: impl Fn(Option<String>) -> bool + Send + 'static,
) -> std::io::Result<()> {
    let name = endpoint(root)?;
    let mut listener = bind(&name)?;
    std::thread::Builder::new()
        .name("folio-instance".into())
        .spawn(move || {
            while let Ok(stream) = accept(&mut listener) {
                handle(stream, &activate);
            }
        })?;
    Ok(())
}

fn handle(mut stream: Stream, activate: &impl Fn(Option<String>) -> bool) {
    let mut line = String::new();
    if BufReader::new((&mut stream).take(4096))
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let Some(token) = line.trim_end().strip_prefix(REQUEST) else {
        return;
    };
    let token = Some(token.trim()).filter(|t| !t.is_empty()).map(str::to_owned);
    if activate(token) {
        let _ = writeln!(stream, "{REPLY}");
        finish(&stream);
    }
}

/// A stable FNV-1a hash; builds from different releases must agree on it.
fn library_key(root: &Path) -> std::io::Result<String> {
    let root = std::fs::canonicalize(root)?;
    let hash = root
        .to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    Ok(format!("io.github.folio.Notes-{hash:016x}"))
}

#[cfg(unix)]
use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

#[cfg(unix)]
fn endpoint(root: &Path) -> std::io::Result<std::path::PathBuf> {
    Ok(runtime_dir()?.join(format!("{}.sock", library_key(root)?)))
}

/// The per-user runtime directory. Flatpak shares only `app/$FLATPAK_ID`
/// between sandbox instances of the same app.
#[cfg(unix)]
fn runtime_dir() -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
    {
        if let Some(shared) = std::env::var_os("FLATPAK_ID")
            .map(|id| runtime.join("app").join(id))
            .filter(|path| path.is_dir())
        {
            return Ok(shared);
        }
        return Ok(runtime);
    }
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    let private = std::env::temp_dir().join(format!("folio-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&private) {
        Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => return Err(error),
        _ => {}
    }
    let metadata = std::fs::symlink_metadata(&private)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Folio's runtime directory is not private",
        ));
    }
    Ok(private)
}

#[cfg(unix)]
fn connect(path: &Path) -> std::io::Result<Stream> {
    let stream = Stream::connect(path)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    Ok(stream)
}

#[cfg(unix)]
fn bind(path: &Path) -> std::io::Result<Listener> {
    // The data-directory lock proves any existing socket is stale.
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    Listener::bind(path)
}

#[cfg(unix)]
fn accept(listener: &mut Listener) -> std::io::Result<Stream> {
    let (stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    Ok(stream)
}

#[cfg(unix)]
fn finish(_: &Stream) {}

#[cfg(windows)]
type Stream = std::fs::File;

#[cfg(windows)]
fn endpoint(root: &Path) -> std::io::Result<String> {
    Ok(format!(r"\\.\pipe\{}", library_key(root)?))
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
fn connect(name: &str) -> std::io::Result<Stream> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::ERROR_PIPE_BUSY, System::Pipes::GetNamedPipeServerProcessId,
        System::Pipes::WaitNamedPipeW, UI::WindowsAndMessaging::AllowSetForegroundWindow,
    };
    let open = || std::fs::OpenOptions::new().read(true).write(true).open(name);
    let stream = match open() {
        Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
            // SAFETY: the name is a nul-terminated UTF-16 buffer.
            unsafe { WaitNamedPipeW(wide(name).as_ptr(), TIMEOUT.as_millis() as u32) };
            open()?
        }
        result => result?,
    };
    // This launch owns the user's foreground grant; pass it on so the running
    // instance may bring its window forward.
    let mut pid = 0;
    // SAFETY: the handle is an open pipe client owned by `stream`.
    if unsafe { GetNamedPipeServerProcessId(stream.as_raw_handle(), &mut pid) } != 0 {
        // SAFETY: plain Win32 call with a process identifier.
        unsafe { AllowSetForegroundWindow(pid) };
    }
    Ok(stream)
}

#[cfg(windows)]
struct Listener {
    name: Vec<u16>,
    next: Stream,
}
#[cfg(windows)]
impl Listener {
    fn create(name: &[u16], first: bool) -> std::io::Result<Stream> {
        use std::os::windows::io::FromRawHandle;
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX},
            System::Pipes::{
                CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
                PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            },
        };
        let first = if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        // SAFETY: the name is a nul-terminated UTF-16 buffer; default security
        // grants write access only to the owner, administrators and SYSTEM.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | first,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                512,
                512,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: CreateNamedPipeW returned a new handle that we now own.
        Ok(unsafe { Stream::from_raw_handle(handle) })
    }
    fn bind(name: &str) -> std::io::Result<Self> {
        // Pipes disappear with their last handle, so they cannot go stale. An
        // exiting predecessor may still briefly own the name.
        let name = wide(name);
        let start = std::time::Instant::now();
        loop {
            match Self::create(&name, true) {
                Ok(next) => return Ok(Self { name, next }),
                Err(_) if start.elapsed() < TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(error) => return Err(error),
            }
        }
    }
    fn accept(&mut self) -> std::io::Result<Stream> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::ERROR_PIPE_CONNECTED, System::Pipes::ConnectNamedPipe,
        };
        loop {
            // SAFETY: `next` is an owned server pipe handle; no overlapped I/O.
            let connected =
                unsafe { ConnectNamedPipe(self.next.as_raw_handle(), std::ptr::null_mut()) } != 0;
            let error = std::io::Error::last_os_error();
            // Create the following instance before serving this client so later
            // launches never find the name missing.
            let next = Self::create(&self.name, false)?;
            let current = std::mem::replace(&mut self.next, next);
            if connected || error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
                return Ok(current);
            }
        }
    }
}

#[cfg(windows)]
fn bind(name: &str) -> std::io::Result<Listener> {
    Listener::bind(name)
}

#[cfg(windows)]
fn accept(listener: &mut Listener) -> std::io::Result<Stream> {
    listener.accept()
}

/// Wait until the client has read the reply before the pipe closes.
#[cfg(windows)]
fn finish(stream: &Stream) {
    let _ = stream.sync_all();
}
