//! User-initiated updates. Polling reads small signed metadata; it never fetches packages.
mod download;
#[cfg(target_os = "linux")]
pub mod flatpak;
pub mod manifest;
mod windows;

use manifest::{Channel, Release};
use reqwest::blocking::Client;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Checking,
    Current,
    Available {
        version: String,
    },
    Downloading {
        version: String,
        percent: u8,
    },
    Ready {
        version: String,
    },
    Preparing {
        version: String,
    },
    Failed {
        version: String,
        error: String,
        ready: bool,
    },
    Exit,
}
impl State {
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Available { version }
            | Self::Downloading { version, .. }
            | Self::Ready { version }
            | Self::Preparing { version }
            | Self::Failed { version, .. } => Some(version),
            _ => None,
        }
    }
    pub fn can_download(&self) -> bool {
        matches!(
            self,
            Self::Available { .. } | Self::Failed { ready: false, .. }
        )
    }
    pub fn can_restart(&self) -> bool {
        matches!(self, Self::Ready { .. } | Self::Failed { ready: true, .. })
    }
    pub fn busy(&self) -> bool {
        matches!(self, Self::Downloading { .. } | Self::Preparing { .. })
    }
}
enum Command {
    Check,
    Download,
    Restart(PathBuf),
}
pub struct Updater {
    pub state: State,
    pub check_error: Option<String>,
    command: Option<Sender<Command>>,
    events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}
enum Event {
    State(State),
    CheckError(Option<String>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    WindowsInstaller,
    WindowsPortable,
    Flatpak,
    Unsupported,
}
impl Backend {
    fn detect() -> Self {
        if cfg!(windows) {
            let installed = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent()?.parent().map(Path::to_path_buf))
                .is_some_and(|p| p.join("unins000.exe").is_file());
            if installed {
                Self::WindowsInstaller
            } else {
                Self::WindowsPortable
            }
        } else if Path::new("/.flatpak-info").is_file() {
            Self::Flatpak
        } else {
            Self::Unsupported
        }
    }
    fn asset(self, release: &Release) -> &manifest::Asset {
        if self == Self::WindowsPortable {
            &release.windows_portable
        } else {
            &release.windows_installer
        }
    }
}
impl Updater {
    pub fn disabled() -> Self {
        let (_, events) = mpsc::channel();
        Self {
            state: State::Current,
            check_error: None,
            command: None,
            events,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn new() -> Self {
        let (commands, receive) = mpsc::channel();
        let (events, incoming) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let backend = Backend::detect();
        let command = if backend == Backend::Unsupported {
            None
        } else {
            let stop = cancel.clone();
            thread::Builder::new()
                .name("folio-updates".into())
                .spawn(move || {
                    if let Err(error) = worker(receive, events.clone(), stop, backend) {
                        let _ = events.send(Event::CheckError(Some(error)));
                    }
                })
                .ok()
                .map(|_| commands)
        };
        Self {
            state: if command.is_some() {
                State::Checking
            } else {
                State::Current
            },
            check_error: None,
            command,
            events: incoming,
            cancel,
        }
    }
    pub fn supported(&self) -> bool {
        self.command.is_some()
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for event in self.events.try_iter() {
            match event {
                Event::State(state) => self.state = state,
                Event::CheckError(error) => {
                    self.check_error = error;
                    if self.state == State::Checking {
                        self.state = State::Current;
                    }
                }
            }
            changed = true;
        }
        changed
    }
    pub fn check(&mut self) {
        if !self.state.busy()
            && let Some(command) = &self.command
        {
            self.check_error = None;
            let _ = command.send(Command::Check);
        }
    }
    pub fn download(&mut self) {
        if self.state.can_download()
            && let Some(command) = &self.command
        {
            let version = self.state.version().unwrap().to_owned();
            // Change state before sending so double clicks cannot enqueue two operations.
            self.state = State::Downloading {
                version,
                percent: 0,
            };
            let _ = command.send(Command::Download);
        }
    }
    pub fn restart(&mut self, data: PathBuf) {
        if self.state.can_restart()
            && let Some(command) = &self.command
        {
            self.state = State::Preparing {
                version: self.state.version().unwrap().into(),
            };
            let _ = command.send(Command::Restart(data));
        }
    }
}
impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for Updater {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn cache_dir() -> PathBuf {
    // Keep packages outside custom libraries and outside the installation being replaced.
    folio_platform::default_data_dir().join("updates")
}
fn package_path(cache: &Path, backend: Backend, release: &Release) -> PathBuf {
    cache.join(format!(
        "folio-{}-{}.{}",
        release.version,
        backend.asset(release).sha256,
        if backend == Backend::WindowsPortable {
            "zip"
        } else {
            "exe"
        }
    ))
}
fn client() -> Result<Client, String> {
    Client::builder()
        .https_only(true)
        .user_agent(concat!("Folio/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60 * 60))
        .build()
        .map_err(|e| e.to_string())
}
fn read_manifest(client: &Client, channel: &Channel) -> Result<Vec<u8>, String> {
    let response = client
        .get(channel.manifest_url())
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
fn worker(
    commands: Receiver<Command>,
    events: Sender<Event>,
    cancel: Arc<AtomicBool>,
    backend: Backend,
) -> Result<(), String> {
    let client = client()?;
    let channel = Channel::bundled();
    let cache = cache_dir();
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let metadata = cache.join("release.json");
    let mut release = fs::read(&metadata)
        .ok()
        .and_then(|b| manifest::verify(&b, &channel).ok())
        .filter(|r| r.newer_than(env!("CARGO_PKG_VERSION")).unwrap_or(false));
    let mut ready = false;
    if let Some(r) = &release {
        ready = backend != Backend::Flatpak
            && download::verified(&package_path(&cache, backend, r), backend.asset(r));
        let _ = events.send(Event::State(if ready {
            State::Ready {
                version: r.version.clone(),
            }
        } else {
            State::Available {
                version: r.version.clone(),
            }
        }));
    }
    let mut next_check = Instant::now();
    #[cfg(target_os = "linux")]
    let mut flatpak_ready = None;
    loop {
        let command = if Instant::now() >= next_check {
            Command::Check
        } else {
            match commands.recv_timeout(next_check.saturating_duration_since(Instant::now())) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => Command::Check,
            }
        };
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match command {
            Command::Check => {
                match read_manifest(&client, &channel)
                    .and_then(|bytes| manifest::verify(&bytes, &channel).map(|r| (bytes, r)))
                {
                    Ok((bytes, candidate)) => {
                        let _ = events.send(Event::CheckError(None));
                        next_check = Instant::now() + Duration::from_secs(10 * 60);
                        if candidate.newer_than(env!("CARGO_PKG_VERSION"))? {
                            if let Some(previous) = &release {
                                let ordering = semver::Version::parse(&candidate.version)
                                    .unwrap()
                                    .cmp(&semver::Version::parse(&previous.version).unwrap());
                                if ordering.is_lt() {
                                    continue;
                                }
                                if ordering.is_eq() && previous != &candidate {
                                    let _ = events.send(Event::CheckError(Some("The published release changed without increasing its version. Waiting for a new immutable release".into())));
                                    continue;
                                }
                            }
                            if release
                                .as_ref()
                                .is_none_or(|r| r.version != candidate.version)
                            {
                                ready = backend != Backend::Flatpak
                                    && download::verified(
                                        &package_path(&cache, backend, &candidate),
                                        backend.asset(&candidate),
                                    );
                                let _ = events.send(Event::State(if ready {
                                    State::Ready {
                                        version: candidate.version.clone(),
                                    }
                                } else {
                                    State::Available {
                                        version: candidate.version.clone(),
                                    }
                                }));
                                release = Some(candidate);
                            }
                            let partial =
                                metadata.with_extension(format!("{}.tmp", std::process::id()));
                            if fs::write(&partial, bytes).is_ok() {
                                let _ = folio_platform::publish_file(&partial, &metadata);
                            }
                        } else if release.is_none() {
                            let _ = events.send(Event::State(State::Current));
                        }
                    }
                    Err(error) => {
                        next_check = Instant::now() + Duration::from_secs(5 * 60);
                        let _ = events.send(Event::CheckError(Some(format!(
                            "Could not check for updates: {error}"
                        ))));
                    }
                }
            }
            Command::Download => {
                let Some(r) = &release else { continue };
                if ready {
                    continue;
                }
                let version = r.version.clone();
                let progress = |percent| {
                    let _ = events.send(Event::State(State::Downloading {
                        version: version.clone(),
                        percent,
                    }));
                };
                let cache_lock = if backend == Backend::Flatpak {
                    None
                } else {
                    match folio_platform::lock_data_dir(&cache) {
                        Ok(lock) => Some(lock),
                        Err(_) => {
                            let _ = events.send(Event::State(State::Failed { version, error: "Another Folio window is preparing an update. Wait for it to finish, then retry".into(), ready: false }));
                            continue;
                        }
                    }
                };
                let result = if backend == Backend::Flatpak {
                    #[cfg(target_os = "linux")]
                    {
                        flatpak::download(&r.flatpak_commit, &cancel, progress).map(|monitor| {
                            flatpak_ready = Some(monitor);
                        })
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        Err("Flatpak updates require Linux".into())
                    }
                } else {
                    download::fetch(
                        &client,
                        backend.asset(r),
                        &package_path(&cache, backend, r),
                        &cancel,
                        progress,
                    )
                };
                drop(cache_lock);
                ready = result.is_ok();
                let _ = events.send(Event::State(match result {
                    Ok(()) => State::Ready { version },
                    Err(error) => State::Failed {
                        version,
                        error,
                        ready: false,
                    },
                }));
            }
            Command::Restart(data) => {
                let Some(r) = &release else { continue };
                if !ready {
                    continue;
                }
                if backend != Backend::Flatpak
                    && !download::verified(&package_path(&cache, backend, r), backend.asset(r))
                {
                    ready = false;
                    let _ = events.send(Event::State(State::Failed {
                        version: r.version.clone(),
                        error: "The staged update changed; click Update to download it again"
                            .into(),
                        ready: false,
                    }));
                    continue;
                }
                let result = if backend == Backend::Flatpak {
                    #[cfg(target_os = "linux")]
                    {
                        flatpak_ready
                            .as_ref()
                            .ok_or_else(|| "Flatpak update is not ready".to_owned())
                            .and_then(|m| m.restart(&data))
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        Err("Flatpak updates require Linux".into())
                    }
                } else {
                    windows::prepare(
                        &cache,
                        &package_path(&cache, backend, r),
                        r,
                        backend == Backend::WindowsPortable,
                        &data,
                    )
                };
                let _ = events.send(Event::State(match result {
                    Ok(()) => State::Exit,
                    Err(error) => State::Failed {
                        version: r.version.clone(),
                        error,
                        ready: true,
                    },
                }));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_require_available_or_verified_ready_state_and_double_clicks_are_ignored() {
        let (tx, commands) = mpsc::channel();
        let (_, events) = mpsc::channel();
        let mut updater = Updater {
            state: State::Current,
            check_error: None,
            command: Some(tx),
            events,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        updater.download();
        updater.restart("notes".into());
        assert!(commands.try_recv().is_err());
        updater.state = State::Available {
            version: "0.1.4".into(),
        };
        updater.restart("notes".into());
        assert!(commands.try_recv().is_err());
        updater.download();
        updater.download();
        assert!(matches!(commands.try_recv(), Ok(Command::Download)));
        assert!(commands.try_recv().is_err());
        updater.restart("notes".into());
        assert!(commands.try_recv().is_err());
        updater.state = State::Ready {
            version: "0.1.4".into(),
        };
        updater.download();
        assert!(commands.try_recv().is_err());
        updater.restart("custom notes".into());
        updater.restart("notes".into());
        assert!(
            matches!(commands.try_recv(), Ok(Command::Restart(path)) if path == Path::new("custom notes"))
        );
        assert!(commands.try_recv().is_err());
    }
}
