//! Updates through Flatpak's own signed repository and sandbox portal. No host
//! filesystem or unrestricted host-command permission is added to the package.
use futures_lite::{StreamExt, future};
use std::{
    collections::HashMap,
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
}
impl Monitor {
    fn proxy(&self) -> Result<Proxy<'_>, String> {
        Proxy::new(&self.connection, PORTAL, &self.path, MONITOR).map_err(|e| e.to_string())
    }
    pub fn restart(&self, data: &Path) -> Result<(), String> {
        use std::os::unix::ffi::OsStrExt;
        if data.starts_with("/tmp") || data.starts_with("/var/tmp") {
            return Err("This library is in the sandbox's temporary directory. Move it to persistent app storage before restarting to update".into());
        }
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
    // Subscribe before creating the object, because the initial availability
    // signal can precede the method reply. Filtering happens by returned path.
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
    };
    let available_by = Instant::now() + Duration::from_secs(45);
    let mut updating = false;
    let mut last_progress = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("The Flatpak update was cancelled".into());
        }
        if !updating && Instant::now() >= available_by {
            return Err("The Folio update repository is not ready. Install the new .flatpakref once to connect it, then retry".into());
        }
        if updating && last_progress.elapsed() > Duration::from_secs(180) {
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
        match header.member().map(|m| m.as_str()) {
            Some("UpdateAvailable") if !updating => {
                if text(&info, "local-commit") == Some(expected_commit) {
                    return Ok(monitor);
                }
                if text(&info, "remote-commit") != Some(expected_commit) {
                    return Err("The Flatpak repository has not published the advertised version yet. Retry shortly".into());
                }
                let options: HashMap<&str, Value<'_>> = HashMap::new();
                monitor.proxy()?.call::<_, _, ()>("Update", &("", options)).map_err(|e| format!("Flatpak could not start the update: {e}. If permissions changed, update Folio with your software manager"))?;
                updating = true;
                last_progress = Instant::now();
            }
            Some("Progress") if updating => {
                last_progress = Instant::now();
                match number(&info, "status").unwrap_or(0) {
                    0 => {
                        let operations = number(&info, "n_ops").unwrap_or(1).max(1);
                        let operation = number(&info, "op").unwrap_or(0).min(operations - 1);
                        let percent = number(&info, "progress").unwrap_or(0).min(100);
                        progress(((operation * 100 + percent) / operations).min(99) as u8);
                    }
                    2 => {
                        progress(100);
                        return Ok(monitor);
                    }
                    1 => {
                        return Err(
                            "Flatpak found no matching update. Refresh the Folio remote and retry"
                                .into(),
                        );
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
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::atomic::AtomicUsize,
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
        remote: String,
        local: String,
    }
    #[zbus::interface(name = "org.freedesktop.portal.Flatpak")]
    impl Portal {
        async fn create_update_monitor(
            &self,
            _options: HashMap<String, OwnedValue>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let info = HashMap::from([
                ("running-commit", Value::from("old")),
                ("local-commit", Value::from(self.local.as_str())),
                ("remote-commit", Value::from(self.remote.as_str())),
            ]);
            // Deliberately emit before the method returns to cover the race.
            connection
                .emit_signal(None::<&str>, HANDLE, MONITOR, "UpdateAvailable", &info)
                .await
                .unwrap();
            Ok(OwnedObjectPath::try_from(HANDLE).unwrap())
        }
    }
    struct Update {
        calls: Arc<AtomicUsize>,
        status: u32,
        deny: bool,
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
            let info = HashMap::from([("status", Value::from(self.status))]);
            connection
                .emit_signal(None::<&str>, HANDLE, MONITOR, "Progress", &info)
                .await
                .unwrap();
            Ok(())
        }
        fn close(&self) {}
    }
    fn run(
        remote: &str,
        local: &str,
        status: u32,
        deny: bool,
    ) -> (Result<(), String>, usize, Vec<u8>) {
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
                    remote: remote.into(),
                    local: local.into(),
                },
            )
            .unwrap()
            .serve_at(
                HANDLE,
                Update {
                    calls: calls.clone(),
                    status,
                    deny,
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
        let result = download_on_connection(
            connection,
            "expected",
            &Arc::new(AtomicBool::new(false)),
            |p| progress.push(p),
        )
        .map(|_| ());
        drop(service);
        (result, calls.load(Ordering::SeqCst), progress)
    }
    #[test]
    fn catches_initial_signal_before_reply_and_reports_download_progress() {
        let (result, calls, progress) = run("expected", "old", 2, false);
        result.unwrap();
        assert_eq!(calls, 1);
        assert_eq!(progress, [75, 100]);
    }
    #[test]
    fn already_deployed_update_needs_no_new_download() {
        let (result, calls, _) = run("expected", "expected", 2, false);
        result.unwrap();
        assert_eq!(calls, 0);
    }
    #[test]
    fn wrong_remote_commit_never_starts_an_installation() {
        let (result, calls, _) = run("different", "old", 2, false);
        assert!(result.unwrap_err().contains("not published"));
        assert_eq!(calls, 0);
    }
    #[test]
    fn portal_denial_empty_update_and_failure_never_report_ready() {
        for (status, deny) in [(1, false), (3, false), (2, true)] {
            let (result, calls, progress) = run("expected", "old", status, deny);
            assert!(result.is_err());
            assert_eq!(calls, 1);
            assert!(!progress.contains(&100));
        }
    }
}
