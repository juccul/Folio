//! Updates through Flatpak's own signed repository and sandbox portal. No host
//! filesystem or unrestricted host-command permission is added to the package.
use futures_lite::{StreamExt, future};
use std::{
    collections::HashMap,
    io::Read,
    os::{fd::AsFd, unix::net::UnixStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};
const PORTAL: &str = "org.freedesktop.portal.Flatpak";
const OBJECT: &str = "/org/freedesktop/portal/Flatpak";
const MONITOR: &str = "org.freedesktop.portal.Flatpak.UpdateMonitor";
type Info = HashMap<String, OwnedValue>;
fn text<'a>(info: &'a Info, key: &str) -> Option<&'a str> {
    info.get(key).and_then(|v| <&str>::try_from(v).ok())
}
fn number(info: &Info, key: &str) -> Option<u32> {
    info.get(key).and_then(|v| u32::try_from(v).ok())
}

pub struct Monitor {
    connection: Connection,
    path: OwnedObjectPath,
    expected_commit: String,
    cancel: Arc<AtomicBool>,
}
impl Monitor {
    fn proxy(&self) -> Result<Proxy<'_>, String> {
        Proxy::new(&self.connection, PORTAL, &self.path, MONITOR).map_err(|e| e.to_string())
    }
    fn verify_deployment(&self) -> Result<(), String> {
        let (mut reader, writer) = UnixStream::pair().map_err(|e| e.to_string())?;
        reader
            .set_read_timeout(Some(Duration::from_millis(200)))
            .map_err(|e| e.to_string())?;
        let portal =
            Proxy::new(&self.connection, PORTAL, OBJECT, PORTAL).map_err(|e| e.to_string())?;
        let fds = HashMap::from([(1u32, zbus::zvariant::Fd::from(writer.as_fd()))]);
        let env: HashMap<&str, &str> = HashMap::new();
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        if self.cancel.load(Ordering::Relaxed) {
            return Err("The Flatpak update was cancelled".into());
        }
        // LATEST_VERSION reads the actual newly deployed sandbox. This command
        // never opens the app or its library; stdout is the only passed FD.
        let _: u32 = portal
            .call(
                "Spawn",
                &(
                    b"/\0".to_vec(),
                    vec![b"/usr/bin/cat\0".to_vec(), b"/.flatpak-info\0".to_vec()],
                    fds,
                    env,
                    2u32,
                    options,
                ),
            )
            .map_err(|e| format!("Could not verify the updated Flatpak deployment: {e}"))?;
        drop(writer);
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return Err("The Flatpak update was cancelled".into());
            }
            if Instant::now() >= deadline {
                return Err("Timed out verifying the updated Flatpak deployment".into());
            }
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if bytes.len() + count > 65_536 {
                        return Err("Flatpak deployment metadata is too large".into());
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => {
                    return Err(format!(
                        "Could not read Flatpak deployment metadata: {error}"
                    ));
                }
            }
        }
        let metadata = std::str::from_utf8(&bytes)
            .map_err(|_| "Flatpak deployment metadata is not valid UTF-8".to_owned())?;
        verify_commit(metadata, &self.expected_commit)
    }
    pub fn restart(&self, data: &Path) -> Result<(), String> {
        use std::os::unix::ffi::OsStrExt;
        if data.starts_with("/tmp") || data.starts_with("/var/tmp") {
            return Err("This library is in the sandbox's temporary directory. Move it to persistent app storage before restarting to update".into());
        }
        self.verify_deployment()?;
        let proxy =
            Proxy::new(&self.connection, PORTAL, OBJECT, PORTAL).map_err(|e| e.to_string())?;
        let nul = |s: &[u8]| {
            let mut bytes = s.to_vec();
            bytes.push(0);
            bytes
        };
        let argv = vec![
            nul(b"/app/bin/folio"),
            nul(b"--data-dir"),
            nul(data.as_os_str().as_bytes()),
            nul(b"--restart-after-update"),
        ];
        let fds: HashMap<u32, zbus::zvariant::Fd<'_>> = HashMap::new();
        let env: HashMap<&str, &str> = HashMap::new();
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        // LATEST_VERSION; deliberately omit WATCH_BUS so the new instance
        // survives the old instance exiting. It waits for the library lock.
        let _: u32 = proxy
            .call("Spawn", &(nul(b"/"), argv, fds, env, 2u32, options))
            .map_err(|e| format!("Could not restart through Flatpak: {e}"))?;
        Ok(())
    }
}
fn verify_commit(metadata: &str, expected: &str) -> Result<(), String> {
    let mut instance = false;
    let mut commit = None;
    for line in metadata.lines().map(str::trim) {
        if line.starts_with('[') {
            instance = line == "[Instance]";
        } else if instance
            && let Some(value) = line.strip_prefix("app-commit=")
            && commit.replace(value.trim()).is_some()
        {
            return Err("Flatpak deployment metadata has duplicate commits".into());
        }
    }
    if commit != Some(expected) {
        return Err("The installed Flatpak does not match this update. Check for updates again or update Folio with your software manager".into());
    }
    Ok(())
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if let Ok(proxy) = self.proxy() {
            let _: Result<(), _> = proxy.call("Close", &());
        }
    }
}
pub fn download(
    expected_commit: &str,
    cancel: &Arc<AtomicBool>,
    progress: impl FnMut(u8),
) -> Result<Monitor, String> {
    let connection = zbus::blocking::connection::Builder::session()
        .map_err(|e| e.to_string())?
        .method_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    download_on_connection(connection, expected_commit, cancel, progress)
}
fn download_on_connection(
    connection: Connection,
    expected_commit: &str,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(u8),
) -> Result<Monitor, String> {
    let portal = Proxy::new(&connection, PORTAL, OBJECT, PORTAL).map_err(|e| e.to_string())?;
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    // Subscribe before creating the object and starting Update: progress can
    // precede its method reply. Filter messages by the returned monitor path.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(PORTAL)
        .map_err(|e| e.to_string())?
        .interface(MONITOR)
        .map_err(|e| e.to_string())?
        .build();
    let mut stream = future::block_on(zbus::MessageStream::for_match_rule(
        rule,
        connection.inner(),
        Some(256),
    ))
    .map_err(|e| e.to_string())?;
    let path: OwnedObjectPath = portal
        .call("CreateUpdateMonitor", &(options,))
        .map_err(|e| format!("This Flatpak installation cannot update itself: {e}"))?;
    let monitor = Monitor {
        connection: connection.clone(),
        path,
        expected_commit: expected_commit.into(),
        cancel: cancel.clone(),
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("The Flatpak update was cancelled".into());
    }
    // Availability is polled only twice an hour by the default portal. The
    // user already approved this transaction by clicking Update, so start it
    // directly instead of waiting for the first availability signal.
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    monitor.proxy()?.call::<_, _, ()>("Update", &("", options)).map_err(|e| format!("Flatpak could not start the update: {e}. If permissions changed, update Folio with your software manager"))?;
    let mut received_progress = false;
    let mut last_progress = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("The Flatpak update was cancelled".into());
        }
        // The first-use permission dialog can stay open while the user reads
        // it. After transaction progress begins, retain a shorter stall limit.
        let timeout = if received_progress { 180 } else { 600 };
        if last_progress.elapsed() > Duration::from_secs(timeout) {
            return Err("Flatpak stopped reporting progress. Your current version is safe; retry when the connection is available".into());
        }
        let message = future::block_on(future::race(async { stream.next().await }, async {
            async_io::Timer::after(Duration::from_millis(500)).await;
            None
        }));
        let Some(message) = message else { continue };
        let message = message.map_err(|e| e.to_string())?;
        let header = message.header();
        if header.path() != Some(&monitor.path.as_ref()) {
            continue;
        }
        let info: Info = message.body().deserialize().map_err(|e| e.to_string())?;
        if let Some("Progress") = header.member().map(|m| m.as_str()) {
            received_progress = true;
            last_progress = Instant::now();
            match number(&info, "status").unwrap_or(0) {
                0 => {
                    let operations = number(&info, "n_ops").unwrap_or(1).max(1);
                    let operation = number(&info, "op").unwrap_or(0).min(operations - 1);
                    let percent = number(&info, "progress").unwrap_or(0).min(100);
                    progress(((operation * 100 + percent) / operations).min(99) as u8);
                }
                1 | 2 => {
                    monitor.verify_deployment()?;
                    progress(100);
                    return Ok(monitor);
                }
                _ => {
                    return Err(format!(
                        "Flatpak update failed: {}",
                        text(&info, "error_message")
                            .unwrap_or("permission denied or connection unavailable")
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Child, Command, Stdio},
        sync::{Mutex, atomic::AtomicUsize},
    };
    const HANDLE: &str = "/org/freedesktop/portal/Flatpak/update_monitor/test";
    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Portal {
        metadata: Arc<Mutex<String>>,
        availability: bool,
    }
    #[zbus::interface(name = "org.freedesktop.portal.Flatpak")]
    impl Portal {
        async fn create_update_monitor(
            &self,
            _options: HashMap<String, OwnedValue>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            if self.availability {
                let info = HashMap::from([
                    ("running-commit", Value::from("old")),
                    ("local-commit", Value::from("old")),
                    ("remote-commit", Value::from("stale")),
                ]);
                connection
                    .emit_signal(None::<&str>, HANDLE, MONITOR, "UpdateAvailable", &info)
                    .await
                    .unwrap();
            }
            Ok(OwnedObjectPath::try_from(HANDLE).unwrap())
        }
        fn spawn(
            &self,
            cwd: Vec<u8>,
            argv: Vec<Vec<u8>>,
            mut fds: HashMap<u32, zbus::zvariant::OwnedFd>,
            env: HashMap<String, String>,
            flags: u32,
            options: HashMap<String, OwnedValue>,
        ) -> u32 {
            assert_eq!(cwd, b"/\0");
            assert_eq!(
                argv,
                [b"/usr/bin/cat\0".to_vec(), b"/.flatpak-info\0".to_vec()]
            );
            assert_eq!(flags, 2, "LATEST_VERSION without WATCH_BUS");
            assert!(env.is_empty() && options.is_empty());
            assert_eq!(fds.len(), 1, "only stdout is passed");
            let fd: std::os::fd::OwnedFd = fds.remove(&1).unwrap().into();
            let mut stdout = UnixStream::from(fd);
            stdout
                .write_all(self.metadata.lock().unwrap().as_bytes())
                .unwrap();
            123
        }
    }
    struct Update {
        calls: Arc<AtomicUsize>,
        status: u32,
        deny: bool,
        cancel: Arc<AtomicBool>,
    }
    #[zbus::interface(name = "org.freedesktop.portal.Flatpak.UpdateMonitor")]
    impl Update {
        async fn update(
            &self,
            _parent: &str,
            _options: HashMap<String, OwnedValue>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.deny {
                return Err(zbus::fdo::Error::NotSupported(
                    "Permissions increased".into(),
                ));
            }
            let info = HashMap::from([
                ("status", Value::from(0u32)),
                ("n_ops", Value::from(2u32)),
                ("op", Value::from(1u32)),
                ("progress", Value::from(50u32)),
            ]);
            connection
                .emit_signal(None::<&str>, HANDLE, MONITOR, "Progress", &info)
                .await
                .unwrap();
            if self.status == 4 {
                self.cancel.store(true, Ordering::Relaxed);
            }
            let mut info = HashMap::from([("status", Value::from(self.status))]);
            if self.status == 3 {
                info.insert(
                    "error_message",
                    Value::from("Update permission was declined"),
                );
            }
            connection
                .emit_signal(None::<&str>, HANDLE, MONITOR, "Progress", &info)
                .await
                .unwrap();
            Ok(())
        }
        fn close(&self) {}
    }
    fn run(
        metadata: &str,
        availability: bool,
        status: u32,
        deny: bool,
        cancelled: bool,
    ) -> (Result<(), String>, usize, Vec<u8>) {
        run_with_restart(metadata, availability, status, deny, cancelled, false)
    }
    fn run_with_restart(
        metadata: &str,
        availability: bool,
        status: u32,
        deny: bool,
        cancelled: bool,
        changed_before_restart: bool,
    ) -> (Result<(), String>, usize, Vec<u8>) {
        let metadata = Arc::new(Mutex::new(metadata.to_owned()));
        let cancel = Arc::new(AtomicBool::new(cancelled));
        let child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon needed for portal contract tests");
        let mut bus = Bus(child);
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let service = zbus::blocking::connection::Builder::address(address.trim())
            .unwrap()
            .name(PORTAL)
            .unwrap()
            .serve_at(
                OBJECT,
                Portal {
                    metadata: metadata.clone(),
                    availability,
                },
            )
            .unwrap()
            .serve_at(
                HANDLE,
                Update {
                    calls: calls.clone(),
                    status,
                    deny,
                    cancel: cancel.clone(),
                },
            )
            .unwrap()
            .build()
            .unwrap();
        let connection = zbus::blocking::connection::Builder::address(address.trim())
            .unwrap()
            .method_timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let mut progress = vec![];
        let result = download_on_connection(connection, "expected", &cancel, |p| progress.push(p))
            .and_then(|monitor| {
                if changed_before_restart {
                    *metadata.lock().unwrap() = "[Instance]\napp-commit=different\n".into();
                    monitor.restart(Path::new("/home/folio-private-library"))
                } else {
                    Ok(())
                }
            });
        drop(service);
        (result, calls.load(Ordering::SeqCst), progress)
    }
    const EXPECTED: &str =
        "[Application]\nname=io.github.folio.Notes\n[Instance]\napp-commit=expected\n";
    #[test]
    fn starts_update_without_availability_and_catches_progress_before_reply() {
        let (result, calls, progress) = run(EXPECTED, false, 2, false, false);
        result.unwrap();
        assert_eq!(calls, 1);
        assert_eq!(progress, [75, 100]);
    }
    #[test]
    fn stale_availability_does_not_block_explicit_update() {
        let (result, calls, _) = run(EXPECTED, true, 2, false, false);
        result.unwrap();
        assert_eq!(calls, 1);
    }
    #[test]
    fn empty_transaction_is_ready_only_when_expected_update_is_deployed() {
        let (result, calls, progress) = run(EXPECTED, false, 1, false, false);
        result.unwrap();
        assert_eq!(calls, 1);
        assert!(progress.contains(&100));
        let (result, _, progress) = run("[Instance]\napp-commit=old\n", false, 1, false, false);
        assert!(result.unwrap_err().contains("does not match"));
        assert!(!progress.contains(&100));
    }
    #[test]
    fn completed_transaction_with_wrong_commit_never_reports_ready() {
        let (result, calls, progress) =
            run("[Instance]\napp-commit=different\n", false, 2, false, false);
        assert!(result.unwrap_err().contains("does not match"));
        assert_eq!(calls, 1);
        assert!(!progress.contains(&100));
    }
    #[test]
    fn portal_denial_and_permission_prompt_failure_never_report_ready() {
        for (status, deny) in [(3, false), (2, true)] {
            let (result, calls, progress) = run(EXPECTED, false, status, deny, false);
            let error = result.unwrap_err();
            assert!(error.contains(if deny {
                "Permissions increased"
            } else {
                "Update permission was declined"
            }));
            assert_eq!(calls, 1);
            assert!(!progress.contains(&100));
        }
    }
    #[test]
    fn cancellation_before_transaction_prevents_update() {
        let (result, calls, progress) = run(EXPECTED, false, 2, false, true);
        assert!(result.unwrap_err().contains("cancelled"));
        assert_eq!(calls, 0);
        assert!(progress.is_empty());
    }
    #[test]
    fn cancellation_during_transaction_never_reports_ready() {
        let (result, calls, progress) = run(EXPECTED, false, 4, false, false);
        assert!(result.unwrap_err().contains("cancelled"));
        assert_eq!(calls, 1);
        assert!(!progress.contains(&100));
    }
    #[test]
    fn restart_rechecks_latest_commit_before_starting_app() {
        let (result, calls, progress) = run_with_restart(EXPECTED, false, 2, false, false, true);
        assert!(result.unwrap_err().contains("does not match"));
        assert_eq!(calls, 1);
        assert!(
            progress.contains(&100),
            "download was verified before deployment changed"
        );
    }
    #[test]
    fn oversized_deployment_metadata_is_rejected() {
        let (result, _, progress) = run(&"x".repeat(65_537), false, 2, false, false);
        assert!(result.unwrap_err().contains("too large"));
        assert!(!progress.contains(&100));
    }
    #[test]
    fn commit_metadata_requires_instance_section_and_one_commit() {
        verify_commit(EXPECTED, "expected").unwrap();
        for metadata in [
            "",
            "[Application]\napp-commit=expected\n",
            "[Instance]\napp-commit=expected\napp-commit=expected\n",
        ] {
            assert!(verify_commit(metadata, "expected").is_err());
        }
    }
}
