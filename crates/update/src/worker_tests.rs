use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use sha2::{Digest, Sha256};
use std::thread::JoinHandle;

struct Distribution {
    key: Ed25519KeyPair,
    channel: Channel,
    package: Vec<u8>,
}

impl Distribution {
    fn new() -> Self {
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let channel = Channel {
            repository: "example/Folio".into(),
            public_key: STANDARD.encode(key.public_key().as_ref()),
        };
        Self {
            key,
            channel,
            package: vec![42; 2048],
        }
    }

    fn release(&self, version: &str) -> Release {
        let asset = |suffix| manifest::Asset {
            url: format!(
                "https://github.com/{}/releases/download/v{version}/folio-{version}-{suffix}",
                self.channel.repository
            ),
            sha256: format!("{:x}", Sha256::digest(&self.package)),
            size: self.package.len() as u64,
        };
        Release {
            schema: 1,
            version: version.into(),
            windows_installer: asset("windows-x64-setup.exe"),
            windows_portable: asset("windows-x64.zip"),
            windows_binary_sha256: "b".repeat(64),
            flatpak_commit: "c".repeat(64),
        }
    }

    fn sign(&self, release: &Release) -> Vec<u8> {
        let payload = serde_json::to_vec(release).unwrap();
        serde_json::to_vec(&manifest::Envelope {
            signature: STANDARD.encode(self.key.sign(&payload).as_ref()),
            payload: STANDARD.encode(payload),
        })
        .unwrap()
    }

    fn prepare_cache(&self, cache: &Path, release: &Release, staged: bool, backend: Backend) {
        fs::create_dir_all(cache.join("job-existing")).unwrap();
        fs::write(cache.join("release.json"), self.sign(release)).unwrap();
        fs::write(cache.join("job-existing/intent.json"), b"recovery").unwrap();
        if staged {
            fs::write(package_path(cache, backend, release), &self.package).unwrap();
        }
    }
}

// Exercise the production worker while controlling only its metadata transport.
// Each fetch pauses at a channel barrier; there are no timer sleeps, network
// requests, update installations or accesses to the user's application data.
struct Session {
    commands: Sender<Command>,
    events: Receiver<Event>,
    fetches: Receiver<()>,
    responses: Sender<Result<Vec<u8>, String>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl Session {
    fn start(cache: &Path, channel: &Channel, backend: Backend) -> Self {
        let (commands, receive_commands) = mpsc::channel();
        let (send_events, events) = mpsc::channel();
        let (send_fetches, fetches) = mpsc::channel();
        let (responses, receive_responses) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let config = WorkerConfig {
            backend,
            channel: channel.clone(),
            cache: cache.to_owned(),
            current: "0.1.0",
        };
        let worker = thread::spawn(move || {
            worker_session(
                receive_commands,
                send_events,
                stop,
                config,
                Client::builder().https_only(true).build().unwrap(),
                || {
                    send_fetches.send(()).unwrap();
                    receive_responses.recv().unwrap()
                },
            )
        });
        let session = Self {
            commands,
            events,
            fetches,
            responses,
            cancel,
            worker: Some(worker),
        };
        session.wait_for_fetch();
        session
    }

    fn wait_for_fetch(&self) {
        self.fetches
            .recv_timeout(Duration::from_secs(5))
            .expect("worker did not begin its metadata check");
    }

    fn reply(&self, result: Result<Vec<u8>, String>) {
        self.responses.send(result).unwrap();
    }

    fn next_check(&self) {
        self.commands.send(Command::Check).unwrap();
        self.wait_for_fetch();
    }

    fn event(&self) -> Event {
        self.events
            .recv_timeout(Duration::from_secs(5))
            .expect("worker did not publish its check result")
    }

    fn expect_state(&self, expected: State) {
        match self.event() {
            Event::State(actual) => assert_eq!(actual, expected),
            Event::CheckError(error) => panic!("expected {expected:?}, got check error {error:?}"),
        }
    }

    fn expect_check_ok(&self) {
        assert!(matches!(self.event(), Event::CheckError(None)));
    }

    fn expect_check_error(&self, message: &str) {
        match self.event() {
            Event::CheckError(Some(error)) => assert!(error.contains(message), "{error}"),
            _ => panic!("expected check error containing {message:?}"),
        }
    }

    fn expect_no_events(&self) {
        assert!(matches!(
            self.events.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.commands.send(Command::Check);
        // Also unblock a fetch paused by a test, including during panic unwinding.
        let _ = self.responses.send(Err("test session stopped".into()));
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.unwrap().unwrap();
            }
        }
    }
}

#[test]
fn current_channel_retires_cached_available_and_ready_targets_and_persists_the_reset() {
    let distribution = Distribution::new();
    for backend in [
        Backend::WindowsInstaller,
        Backend::WindowsPortable,
        Backend::Flatpak,
    ] {
        for staged in [false, true] {
            let data = tempfile::tempdir().unwrap();
            let cache = cache_dir_at(data.path());
            let retired = distribution.release("0.1.4");
            let current = distribution.sign(&distribution.release("0.1.0"));
            distribution.prepare_cache(&cache, &retired, staged, backend);
            let package = package_path(&cache, backend, &retired);
            assert_eq!(
                download::verified(&package, backend.asset(&retired)),
                staged
            );

            {
                let session = Session::start(&cache, &distribution.channel, backend);
                session.expect_no_events(); // Cached Available/Ready never reaches the UI.
                session.reply(Ok(distribution.sign(&retired)));
                session.expect_check_ok();
                session.expect_state(if staged && backend != Backend::Flatpak {
                    State::Ready {
                        version: "0.1.4".into(),
                    }
                } else {
                    State::Available {
                        version: "0.1.4".into(),
                    }
                });
                session.next_check();
                session.expect_no_events();
                session.reply(Ok(current.clone()));
                session.expect_check_ok();
                session.expect_state(State::Current);
                session.commands.send(Command::Download).unwrap();
                session.expect_state(State::Current);
                session
                    .commands
                    .send(Command::Restart(data.path().join("notes")))
                    .unwrap();
                session.expect_state(State::Current);
                assert_eq!(fs::read(cache.join("release.json")).unwrap(), current);
                assert!(cached_release(&cache, &distribution.channel, "0.1.0").is_none());
                assert_eq!(
                    fs::read(cache.join("job-existing/intent.json")).unwrap(),
                    b"recovery"
                );
                if staged {
                    assert_eq!(fs::read(&package).unwrap(), distribution.package);
                } else {
                    assert!(!package.exists());
                }
            }

            let restarted = Session::start(&cache, &distribution.channel, backend);
            restarted.expect_no_events();
            restarted.reply(Ok(current));
            restarted.expect_check_ok();
            restarted.expect_state(State::Current);
            restarted.next_check(); // Barrier confirms persistence finished on restart too.
            restarted.expect_no_events();
        }
    }
}

#[test]
fn current_initial_flatpak_check_never_surfaces_the_retired_cached_offer() {
    let distribution = Distribution::new();
    let data = tempfile::tempdir().unwrap();
    let cache = cache_dir_at(data.path());
    distribution.prepare_cache(
        &cache,
        &distribution.release("0.1.4"),
        false,
        Backend::Flatpak,
    );
    let current = distribution.sign(&distribution.release("0.1.0"));
    let session = Session::start(&cache, &distribution.channel, Backend::Flatpak);
    session.expect_no_events();
    session.reply(Ok(current.clone()));
    session.expect_check_ok();
    session.expect_state(State::Current);
    session.next_check();
    session.expect_no_events();
    assert_eq!(fs::read(cache.join("release.json")).unwrap(), current);
}

#[test]
fn retired_cache_does_not_block_future_release_and_download_requires_an_explicit_command() {
    let distribution = Distribution::new();
    let data = tempfile::tempdir().unwrap();
    let cache = cache_dir_at(data.path());
    let backend = Backend::WindowsInstaller;
    distribution.prepare_cache(&cache, &distribution.release("0.1.4"), false, backend);
    let session = Session::start(&cache, &distribution.channel, backend);
    session.reply(Ok(distribution.sign(&distribution.release("0.1.0"))));
    session.expect_check_ok();
    session.expect_state(State::Current);

    session.next_check();
    let future = distribution.release("0.1.1");
    session.reply(Ok(distribution.sign(&future)));
    session.expect_check_ok();
    session.expect_state(State::Available {
        version: "0.1.1".into(),
    });
    session.next_check();
    session.expect_no_events();
    assert!(!package_path(&cache, backend, &future).exists());
    assert!(
        !package_path(&cache, backend, &future)
            .with_extension("part")
            .exists()
    );
    assert_eq!(
        cached_release(&cache, &distribution.channel, "0.1.0").unwrap(),
        future
    );
    session.reply(Ok(distribution.sign(&future)));
    session.expect_check_ok();
    session.expect_state(State::Available {
        version: "0.1.1".into(),
    });

    // Holding the production cache lock lets us observe the user action without
    // permitting a real package request or touching an installation.
    let _lock = folio_platform::lock_data_dir(&cache).unwrap();
    session.commands.send(Command::Download).unwrap();
    match session.event() {
        Event::State(State::Failed {
            version,
            error,
            ready,
        }) => {
            assert_eq!(version, "0.1.1");
            assert!(error.contains("Another Folio window"), "{error}");
            assert!(!ready);
        }
        _ => panic!("explicit download did not reach cache-lock handling"),
    }
    assert!(!package_path(&cache, backend, &future).exists());
}

#[test]
fn failed_or_untrusted_initial_checks_preserve_cached_metadata_without_offering_it() {
    let distribution = Distribution::new();
    let retired = distribution.release("0.1.4");
    let original = distribution.sign(&retired);
    let mut tampered: manifest::Envelope = serde_json::from_slice(&original).unwrap();
    tampered.signature = STANDARD.encode([0; 64]);
    for response in [
        Err("offline test".into()),
        Ok(serde_json::to_vec(&tampered).unwrap()),
        Ok(b"invalid metadata".to_vec()),
    ] {
        let data = tempfile::tempdir().unwrap();
        let cache = cache_dir_at(data.path());
        let backend = Backend::WindowsInstaller;
        distribution.prepare_cache(&cache, &retired, true, backend);
        let session = Session::start(&cache, &distribution.channel, backend);
        session.expect_no_events();
        session.reply(response);
        session.expect_check_error("Could not check for updates");
        session.next_check();
        session.expect_no_events();
        assert_eq!(fs::read(cache.join("release.json")).unwrap(), original);
        assert_eq!(
            fs::read(cache.join("job-existing/intent.json")).unwrap(),
            b"recovery"
        );
        assert_eq!(
            fs::read(package_path(&cache, backend, &retired)).unwrap(),
            distribution.package
        );

        // The retained target is actionable only after a fresh signed confirmation.
        session.reply(Ok(original.clone()));
        session.expect_check_ok();
        session.expect_state(State::Ready {
            version: "0.1.4".into(),
        });
    }
}

#[test]
fn fresh_checks_reject_rollback_and_changed_same_version_windows_payloads() {
    let distribution = Distribution::new();
    let previous = distribution.release("0.1.2");
    let mut changed_asset = previous.clone();
    changed_asset.windows_installer.sha256 = "d".repeat(64);
    let mut changed_binary = previous.clone();
    changed_binary.windows_binary_sha256 = "e".repeat(64);
    for (candidate, error) in [
        (distribution.release("0.1.1"), "older update"),
        (changed_asset, "without increasing its version"),
        (changed_binary, "without increasing its version"),
    ] {
        let data = tempfile::tempdir().unwrap();
        let cache = cache_dir_at(data.path());
        let backend = Backend::WindowsInstaller;
        distribution.prepare_cache(&cache, &previous, true, backend);
        let original = distribution.sign(&previous);
        let session = Session::start(&cache, &distribution.channel, backend);
        session.reply(Ok(distribution.sign(&candidate)));
        session.expect_check_error(error);
        session.next_check();
        session.expect_no_events();
        assert_eq!(fs::read(cache.join("release.json")).unwrap(), original);
        assert!(download::verified(
            &package_path(&cache, backend, &previous),
            backend.asset(&previous)
        ));
        session.reply(Ok(original));
        session.expect_check_ok();
        session.expect_state(State::Ready {
            version: "0.1.2".into(),
        });
    }
}

#[test]
fn metadata_persistence_failure_is_reported_without_reactivating_a_retired_target() {
    let distribution = Distribution::new();
    let data = tempfile::tempdir().unwrap();
    let cache = cache_dir_at(data.path());
    fs::create_dir_all(cache.join("release.json")).unwrap();
    fs::write(cache.join("release.json/keep"), b"existing directory").unwrap();
    fs::create_dir_all(cache.join("job-existing")).unwrap();
    fs::write(cache.join("job-existing/intent.json"), b"recovery").unwrap();
    let session = Session::start(&cache, &distribution.channel, Backend::WindowsInstaller);
    session.reply(Ok(distribution.sign(&distribution.release("0.1.0"))));
    session.expect_check_ok();
    session.expect_state(State::Current);
    session.expect_check_error("Could not save the checked update metadata");
    session.commands.send(Command::Download).unwrap();
    session.expect_state(State::Current);
    session
        .commands
        .send(Command::Restart(data.path().join("notes")))
        .unwrap();
    session.expect_state(State::Current);
    assert_eq!(
        fs::read(cache.join("release.json/keep")).unwrap(),
        b"existing directory"
    );
    assert_eq!(
        fs::read(cache.join("job-existing/intent.json")).unwrap(),
        b"recovery"
    );
}
