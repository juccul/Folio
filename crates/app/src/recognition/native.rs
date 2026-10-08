//! First-use, verified GLM-OCR Q8 installation and resident local Vulkan inference.
//! Only asset downloads use the Internet. Document images go to authenticated loopback.
use super::*;
use base64::Engine;
use folio_platform::BackgroundCommand;
use reqwest::blocking::Client as Http;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    net::TcpListener,
    path::Path,
    thread,
};

type Result<T> = std::result::Result<T, String>;
const REVISION: &str = "65a42de1148dbed2297e922b5dbc7d9b70c36578";
#[cfg(not(windows))]
const RUNTIME: &str = "llama-b11457-bin-ubuntu-vulkan-x64.tar.gz";
#[cfg(windows)]
const RUNTIME: &str = "llama-b11457-bin-win-vulkan-x64.zip";
#[cfg(not(windows))]
const RUNTIME_BYTES: u64 = 31_679_935;
#[cfg(windows)]
const RUNTIME_BYTES: u64 = 33_377_746;
#[cfg(not(windows))]
const RUNTIME_SHA: &str = "cb528b7f75e466f5113685d8aca7a9966a5dac3192f2e12bd4f96d7505fbea39";
#[cfg(windows)]
const RUNTIME_SHA: &str = "d01301582c711a69b9747b5984710d6ca99e57d95f680d3d33753cec570b4cb6";
#[cfg(not(windows))]
const SERVER_PATH: &str = "llama-b11457/llama-server";
#[cfg(windows)]
const SERVER_PATH: &str = "llama-server.exe";
const MODEL: &str = "GLM-OCR-Q8_0.gguf";
const VISION: &str = "mmproj-GLM-OCR-Q8_0.gguf";
const TOTAL: u64 = 950_433_408 + 484_403_648 + RUNTIME_BYTES;
const ASSETS: [(&str, u64, &str); 3] = [
    (
        MODEL,
        950_433_408,
        "45bc244a6446aff850521dc41f18bc8d7105ad5f0c2c8c28af04e7cc4f4d50b1",
    ),
    (
        VISION,
        484_403_648,
        "9c4b58e33e316ed142eb5dcb41abec3844d3e6e5dc361ffb782c3fa9d175141f",
    ),
    (RUNTIME, RUNTIME_BYTES, RUNTIME_SHA),
];

pub(super) struct Context<'a> {
    pub generation: u64,
    pub current: &'a AtomicU64,
    pub process: &'a Mutex<Option<Child>>,
    pub progress: &'a dyn Fn(String),
}
impl Context<'_> {
    fn check(&self) -> Result<()> {
        if self.current.load(Ordering::Acquire) != self.generation {
            Err("Recognition cancelled".into())
        } else {
            Ok(())
        }
    }
    fn stage(&self, value: impl Into<String>) {
        (self.progress)(value.into());
    }
}
fn io(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(any(windows, test))]
fn extract_zip(path: &Path, destination: &Path, context: &Context<'_>) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(path).map_err(io)?).map_err(io)?;
    if archive.len() > 256 {
        return Err("OCR runtime archive has too many files".into());
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        context.check()?;
        let mut item = archive.by_index(index).map_err(io)?;
        let name = item
            .enclosed_name()
            .ok_or("Invalid OCR runtime archive path")?;
        // The Windows pack contains ordinary files, never links or drive paths.
        if name
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || item.name().contains('\\')
            || item.name().contains(':')
            || item
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Invalid OCR runtime archive path".into());
        }
        total = total
            .checked_add(item.size())
            .ok_or("OCR archive is too large")?;
        if total > 512 * 1024 * 1024 {
            return Err("OCR archive is too large".into());
        }
        let target = destination.join(name);
        if item.is_dir() {
            fs::create_dir_all(target).map_err(io)?;
        } else {
            fs::create_dir_all(target.parent().ok_or("Invalid runtime path")?).map_err(io)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
                .map_err(io)?;
            let mut buffer = [0; 64 * 1024];
            loop {
                context.check()?;
                let count = item.read(&mut buffer).map_err(io)?;
                if count == 0 {
                    break;
                }
                output.write_all(&buffer[..count]).map_err(io)?;
            }
            output.sync_all().map_err(io)?;
        }
    }
    Ok(())
}
fn hash(path: &Path, context: &Context<'_>) -> Result<String> {
    let mut file = File::open(path).map_err(io)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1024 * 1024];
    loop {
        context.check()?;
        let n = file.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn verified(path: &Path, size: u64, sha: &str, context: &Context<'_>) -> Result<bool> {
    if !path.is_file() || fs::metadata(path).map_err(io)?.len() != size {
        return Ok(false);
    }
    Ok(hash(path, context)? == sha)
}
fn asset_url(name: &str) -> String {
    if name == RUNTIME {
        format!("https://github.com/ggml-org/llama.cpp/releases/download/b11457/{name}")
    } else {
        format!("https://huggingface.co/ggml-org/GLM-OCR-GGUF/resolve/{REVISION}/{name}")
    }
}
// Short, bounded range requests allow cancellation and resumable retries even on slow links.
fn download(
    client: &Http,
    url: &str,
    path: &Path,
    size: u64,
    sha: &str,
    completed: u64,
    context: &Context<'_>,
) -> Result<()> {
    context.stage(format!(
        "Checking OCR files… ({:.0} / {:.0} MB)",
        completed as f64 / 1e6,
        TOTAL as f64 / 1e6
    ));
    if verified(path, size, sha, context)? {
        return Ok(());
    }
    let partial = path.with_extension("partial");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .append(true)
        .open(&partial)
        .map_err(io)?;
    let mut offset = file.metadata().map_err(io)?.len();
    if offset > size {
        file.set_len(0).map_err(io)?;
        offset = 0;
    }
    // A complete partial is useful after a crash between verification and activation.
    let mut failures = 0;
    while offset < size {
        context.check()?;
        context.stage(format!(
            "Downloading OCR · {:.0} / {:.0} MB",
            (completed + offset) as f64 / 1e6,
            TOTAL as f64 / 1e6
        ));
        let end = (offset + 4 * 1024 * 1024 - 1).min(size - 1);
        let attempt: Result<()> = (|| {
            context.check()?;
            let mut response = client
                .get(url)
                .header("Range", format!("bytes={offset}-{end}"))
                .send()
                .map_err(io)?
                .error_for_status()
                .map_err(io)?;
            // Never append a whole-file response to a partial, or accept an unexpected range.
            let expected = format!("bytes {offset}-{end}/{size}");
            if response.status() != reqwest::StatusCode::PARTIAL_CONTENT
                || response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    != Some(&expected)
            {
                return Err("Download server did not provide the requested byte range".into());
            }
            let mut remaining = end - offset + 1;
            let mut buf = [0u8; 64 * 1024];
            let mut last_update = Instant::now();
            while remaining > 0 {
                context.check()?;
                let length = (buf.len() as u64).min(remaining) as usize;
                let n = response.read(&mut buf[..length]).map_err(io)?;
                if n == 0 {
                    return Err("OCR download interrupted".into());
                }
                file.write_all(&buf[..n]).map_err(io)?;
                offset += n as u64;
                remaining -= n as u64;
                if last_update.elapsed() > Duration::from_millis(250) {
                    context.stage(format!(
                        "Downloading OCR · {:.0} / {:.0} MB",
                        (completed + offset) as f64 / 1e6,
                        TOTAL as f64 / 1e6
                    ));
                    last_update = Instant::now();
                }
            }
            Ok(())
        })();
        if let Err(error) = attempt {
            context.check()?;
            failures += 1;
            if failures >= 3 {
                return Err(format!(
                    "OCR download failed: {error}. Check your connection and request OCR again to resume."
                ));
            }
        } else {
            failures = 0;
        }
    }
    file.sync_all().map_err(io)?;
    drop(file);
    context.stage("Verifying OCR download…");
    if !verified(&partial, size, sha, context)? {
        let _ = fs::remove_file(&partial);
        return Err("OCR checksum mismatch. Request OCR again to download a fresh copy.".into());
    }
    context.check()?;
    folio_platform::publish_file(&partial, path).map_err(io)
}

pub(super) fn installed(pack: &Path) -> bool {
    let native = fs::read(pack)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|v| v["backend"] == "llama-vulkan");
    let Some(parent) = pack.parent() else {
        return false;
    };
    let root = parent.join("glm-ocr-q8-b11457");
    native
        && root.join("runtime").join(SERVER_PATH).is_file()
        && ASSETS
            .iter()
            .all(|(name, size, _)| fs::metadata(root.join(name)).is_ok_and(|m| m.len() == *size))
}
fn install(pack: &Path, context: &Context<'_>) -> Result<PathBuf> {
    if !cfg!(all(
        any(target_os = "linux", target_os = "windows"),
        target_arch = "x86_64"
    )) {
        return Err("Automatic OCR setup supports x86_64 Linux and Windows. Configure a local recognition pack on this platform.".into());
    }
    let parent = pack.parent().ok_or("Invalid recognition directory")?;
    fs::create_dir_all(parent).map_err(io)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join("setup.lock"))
        .map_err(io)?;
    context.stage(if pack.is_file() {
        "Checking installed OCR…"
    } else {
        "Preparing OCR · first download is about 1.47 GB…"
    });
    loop {
        context.check()?;
        match lock.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {
                context.stage("Waiting for another Folio window to finish OCR setup…");
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(io(error)),
        }
    }
    let root = parent.join("glm-ocr-q8-b11457");
    fs::create_dir_all(&root).map_err(io)?;
    let client = Http::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .user_agent(concat!(
            "Folio/",
            env!("CARGO_PKG_VERSION"),
            " OCR asset setup"
        ))
        .build()
        .map_err(io)?;
    let mut completed = 0;
    for (name, size, sha) in ASSETS {
        download(
            &client,
            &asset_url(name),
            &root.join(name),
            size,
            sha,
            completed,
            context,
        )?;
        completed += size;
    }
    debug_assert_eq!(completed, TOTAL);
    context.check()?;
    context.stage("Installing OCR runtime…");
    // Extract the verified archive into a fresh directory, never over an active runtime.
    let staging = root.join(format!("runtime-{}", Id::new_v4()));
    fs::create_dir(&staging).map_err(io)?;
    let extraction: Result<()> = (|| {
        #[cfg(not(windows))]
        {
            let compressed =
                flate2::read::GzDecoder::new(File::open(root.join(RUNTIME)).map_err(io)?);
            let mut archive = tar::Archive::new(compressed);
            for item in archive.entries().map_err(io)? {
                context.check()?;
                let mut item = item.map_err(io)?;
                if !item.unpack_in(&staging).map_err(io)? {
                    return Err("Invalid OCR runtime archive path".into());
                }
            }
        }
        #[cfg(windows)]
        extract_zip(&root.join(RUNTIME), &staging, context)?;
        if !staging.join(SERVER_PATH).is_file() {
            return Err("OCR runtime archive is incomplete".into());
        }
        let runtime = root.join("runtime");
        if runtime.exists() {
            fs::remove_dir_all(&runtime).map_err(io)?;
        }
        fs::rename(&staging, &runtime).map_err(io)?;
        Ok(())
    })();
    if extraction.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    extraction?;
    fs::write(
        root.join("GLM-OCR-NOTICE.txt"),
        include_str!("../../../../third_party/ocr/GLM-OCR-NOTICE.txt"),
    )
    .map_err(io)?;
    fs::write(
        root.join("GLM-OCR-MODEL-CARD.md"),
        include_str!("../../../../third_party/ocr/GLM-OCR-MODEL-CARD.md"),
    )
    .map_err(io)?;
    fs::write(
        root.join("llama.cpp-THIRD-PARTY.txt"),
        include_str!("../../../../third_party/ocr/llama.cpp-THIRD-PARTY.txt"),
    )
    .map_err(io)?;
    let selected = fs::read(pack)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|config| {
            config["device"]
                .as_str()
                .filter(|_| config["backend"] == "llama-vulkan")
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "auto".into());
    let config = serde_json::json!({"backend":"llama-vulkan", "quantization":"Q8_0",
        "model_revision":REVISION,"runtime":"b11457", "device":selected});
    // Publishing this marker is the final operation. Interrupted setup never looks installed.
    folio_export::atomic_write(
        pack,
        serde_json::to_string_pretty(&config)
            .map_err(io)?
            .as_bytes(),
    )
    .map_err(io)?;
    Ok(root)
}

#[derive(Debug, PartialEq)]
struct Device {
    id: String,
    name: String,
    free: u64,
}
fn devices(output: &str) -> Vec<Device> {
    let mut found = Vec::new();
    for line in output.lines() {
        let Some((id, rest)) = line.trim().split_once(": ") else {
            continue;
        };
        if !id.starts_with("Vulkan")
            || !id[6..].chars().all(|c| c.is_ascii_digit())
            || id.len() == 6
        {
            continue;
        }
        let Some((name, memory)) = rest.rsplit_once(" (") else {
            continue;
        };
        let free = memory
            .split(", ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let lower = name.to_lowercase();
        if lower.contains("llvmpipe")
            || lower.contains("lavapipe")
            || lower.contains("software")
            || free < 2048
        {
            continue;
        }
        found.push(Device {
            id: id.into(),
            name: name.into(),
            free,
        });
    }
    // Prefer discrete GPUs over integrated UMA reporting a larger shared-memory budget.
    found.sort_by_key(|d| {
        let name = d.name.to_lowercase();
        let discrete =
            name.contains("nvidia") || name.contains("radeon rx") || name.contains("intel arc");
        (std::cmp::Reverse(discrete), std::cmp::Reverse(d.free))
    });
    found
}

pub(super) struct Native {
    pub pack: PathBuf,
    http: Http,
    url: String,
    token: String,
    label: String,
}
impl Native {
    pub fn load(pack: &Path, context: &Context<'_>) -> Result<Self> {
        Self::load_inner(pack, context, false)
    }
    pub fn load_cpu(pack: &Path, context: &Context<'_>) -> Result<Self> {
        Self::load_inner(pack, context, true)
    }
    pub fn is_gpu(&self) -> bool {
        self.label.starts_with("Vulkan")
    }
    fn load_inner(pack: &Path, context: &Context<'_>, force_cpu: bool) -> Result<Self> {
        let selected = if pack.is_file() {
            let config: serde_json::Value = serde_json::from_slice(&fs::read(pack).map_err(io)?)
                .unwrap_or(serde_json::Value::Null);
            if config["backend"] == "llama-vulkan" {
                config["device"].as_str().unwrap_or("auto").to_string()
            } else {
                "auto".into()
            }
        } else {
            "auto".into()
        };
        if selected != "auto" && selected != "cpu" && !selected.starts_with("Vulkan") {
            return Err("OCR device must be auto, cpu or a Vulkan device ID".into());
        }
        let root = install(pack, context)?;
        let binary = root.join("runtime").join(SERVER_PATH);
        context.stage("Checking Vulkan GPUs…");
        let mut probe = folio_platform::command(&binary)
            .arg("--list-devices")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn_background()
            .map_err(io)?;
        let stdout = probe.stdout.take().unwrap();
        // Put even discovery under cancellation ownership; no detached GPU processes.
        {
            let mut shared = context.process.lock().unwrap();
            if context.check().is_err() {
                let _ = probe.kill();
                let _ = probe.wait();
                return Err("Recognition cancelled".into());
            }
            *shared = Some(probe);
        }
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut output = String::new();
            let _ = stdout.take(65536).read_to_string(&mut output);
            let _ = tx.send(output);
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        let output = loop {
            context.check()?;
            if let Ok(output) = rx.recv_timeout(Duration::from_millis(100)) {
                break output;
            }
            if Instant::now() > deadline {
                break String::new();
            }
        };
        stop_process(context.process);
        let mut devices = devices(&output);
        if selected == "cpu" || force_cpu {
            devices.clear();
        } else if selected != "auto" {
            devices.retain(|d| d.id == selected);
        }
        let http = Http::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(io)?;
        let token = Id::new_v4().to_string();
        let mut last_error = None;
        for device in devices.iter().map(Some).chain(std::iter::once(None)) {
            context.check()?;
            let label = device
                .map(|d| format!("Vulkan Q8 · {}", d.name))
                .unwrap_or_else(|| "CPU Q8 · Vulkan unavailable".into());
            context.stage(format!("Loading OCR · {label}…"));
            let port = TcpListener::bind("127.0.0.1:0")
                .map_err(io)?
                .local_addr()
                .map_err(io)?
                .port();
            let mut command = folio_platform::command(&binary);
            command
                .args(["--model"])
                .arg(root.join(MODEL))
                .arg("--mmproj")
                .arg(root.join(VISION))
                .args(["--host", "127.0.0.1", "--port"])
                .arg(port.to_string())
                .args([
                    "--api-key",
                    &token,
                    "--offline",
                    "--fit",
                    "off",
                    "--threads",
                    "4",
                    "--threads-batch",
                    "4",
                    "--ctx-size",
                    "16384",
                    "--image-min-tokens",
                    "16",
                    "--image-max-tokens",
                    "12288",
                    "--parallel",
                    "1",
                    "--no-warmup",
                    "--no-cache-prompt",
                    "--cache-ram",
                    "0",
                    "--reasoning-format",
                    "none",
                    "--no-ui",
                    "--no-context-shift",
                    "--verbosity",
                    "0",
                ]);
            if let Some(device) = device {
                command.args([
                    "--device",
                    &device.id,
                    "--mmproj-device",
                    &device.id,
                    "--gpu-layers",
                    "all",
                ]);
            } else {
                command.args([
                    "--device",
                    "none",
                    "--mmproj-device",
                    "none",
                    "--gpu-layers",
                    "0",
                    "--no-mmproj-offload",
                    "--no-op-offload",
                ]);
            }
            let log = File::create(root.join("last-runtime.log")).map_err(io)?;
            let child = command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(log)
                .spawn_background()
                .map_err(io)?;
            {
                let mut shared = context.process.lock().unwrap();
                if context.check().is_err() {
                    let mut child = child;
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Recognition cancelled".into());
                }
                *shared = Some(child);
            }
            let url = format!("http://127.0.0.1:{port}");
            let startup_http = Http::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .map_err(io)?;
            let deadline = Instant::now() + Duration::from_secs(90);
            let ready = loop {
                context.check()?;
                let alive = context
                    .process
                    .lock()
                    .unwrap()
                    .as_mut()
                    .is_some_and(|p| matches!(p.try_wait(), Ok(None)));
                if !alive || Instant::now() > deadline {
                    break false;
                }
                if startup_http
                    .get(format!("{url}/health"))
                    .bearer_auth(&token)
                    .send()
                    .is_ok_and(|r| r.status().is_success())
                {
                    break true;
                }
                thread::sleep(Duration::from_millis(100));
            };
            if ready {
                if let Some(error) = last_error {
                    eprintln!("Folio OCR fallback: {error}; using {label}");
                }
                return Ok(Self {
                    pack: pack.into(),
                    http,
                    url,
                    token,
                    label,
                });
            }
            last_error = Some(format!(
                "{label} failed to load (see {})",
                root.join("last-runtime.log").display()
            ));
            stop_process(context.process);
        }
        Err(format!(
            "OCR could not start: {}",
            last_error.unwrap_or_default()
        ))
    }
    pub fn recognize(&self, request: &Request, context: &Context<'_>) -> Result<String> {
        context.check()?;
        context.stage(format!("Recognizing · {}…", self.label));
        let bytes = match &request.image_path {
            Some(path) => image_bytes(path)?,
            None => render_ink(&request.strokes, request.kind)?,
        };
        let prompt = match request.kind {
            RecognitionKind::Text => "Text Recognition:",
            RecognitionKind::Math => "Formula Recognition:",
        };
        let limit = match request.kind {
            RecognitionKind::Text => 1024,
            RecognitionKind::Math => 512,
        };
        let payload = serde_json::json!({"messages":[{"role":"user","content":[
            {"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))}},
            {"type":"text","text":prompt}]}], "temperature":0,"max_tokens":limit,"seed":0,
            "stream":false,"cache_prompt":false,"repeat_penalty":1.0});
        let response = self
            .http
            .post(format!("{}/v1/chat/completions", self.url))
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .map_err(|error| format!("OCR runtime unavailable: {error}"))?;
        let response = response.error_for_status().map_err(|error| {
            if error
                .status()
                .is_some_and(|status| status.is_server_error())
            {
                format!("OCR runtime unavailable: {error}")
            } else {
                io(error)
            }
        })?;
        let response: serde_json::Value =
            serde_json::from_reader(response.take(1024 * 1024)).map_err(io)?;
        context.check()?;
        let choice = &response["choices"][0];
        if choice["finish_reason"] != "stop" {
            return Err("Recognition was truncated; select a smaller region".into());
        }
        let text = choice["message"]["content"]
            .as_str()
            .ok_or("Invalid OCR response")?
            .trim();
        if text.is_empty() {
            return Err("No text recognized; select a clearer line of writing".into());
        }
        if text.len() > 32768 {
            return Err("Recognized text is too long".into());
        }
        Ok(if matches!(request.kind, RecognitionKind::Math) {
            strip_math_wrappers(text)
        } else {
            text.into()
        })
    }
}

fn image_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut reader = image::ImageReader::open(path)
        .map_err(io)?
        .with_guessed_format()
        .map_err(io)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader.decode().map_err(io)?.to_rgba8();
    if rgba.width() as u64 * rgba.height() as u64 > 2_000_000 {
        return Err("Select a smaller OCR image region".into());
    }
    // Rotation/cropping leaves transparent pixels. Composite onto white rather than
    // letting an RGB decoder turn transparent background into black handwriting.
    let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let pixel = rgba.get_pixel(x, y);
        let alpha = pixel[3] as u32;
        image::Rgb(std::array::from_fn(|i| {
            ((pixel[i] as u32 * alpha + 255 * (255 - alpha) + 127) / 255) as u8
        }))
    });
    let mut output = std::io::Cursor::new(Vec::new());
    rgb.write_to(&mut output, image::ImageFormat::Png)
        .map_err(io)?;
    Ok(output.into_inner())
}

fn strip_math_wrappers(input: &str) -> String {
    let mut text = input.trim();
    for (left, right) in [
        ("```latex", "```"),
        ("```", "```"),
        ("\\[", "\\]"),
        ("\\(", "\\)"),
        ("$$", "$$"),
        ("$", "$"),
    ] {
        if text.len() > left.len() + right.len() && text.starts_with(left) && text.ends_with(right)
        {
            let inner = text[left.len()..text.len() - right.len()].trim();
            if left.starts_with('$') && inner.contains('$') {
                continue;
            }
            text = inner;
        }
    }
    text.into()
}
fn render_ink(strokes: &[Vec<[f32; 2]>], kind: RecognitionKind) -> Result<Vec<u8>> {
    if strokes.is_empty()
        || strokes.len() > 4096
        || strokes.iter().map(Vec::len).sum::<usize>() > 250000
    {
        return Err("Select a smaller passage of writing".into());
    }
    let points: Vec<_> = strokes.iter().flatten().collect();
    if points.is_empty()
        || points
            .iter()
            .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return Err("No valid ink in selection".into());
    }
    let x0 = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let y0 = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let w = (points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        - x0)
        .max(1.);
    let h = (points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        - y0)
        .max(1.);
    let scale = match kind {
        RecognitionKind::Math => (608. / w).min(480. / h),
        RecognitionKind::Text => (1400. / w).min(1000. / h).min(2.),
    };
    let width = (w * scale).round() + 32.;
    let height = (h * scale).round() + 32.;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>"
    );
    use std::fmt::Write as _;
    for stroke in strokes.iter().filter(|s| !s.is_empty()) {
        svg.push_str("<path fill=\"none\" stroke=\"black\" stroke-width=\"1.75\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"");
        for (i, p) in stroke.iter().enumerate() {
            let _ = write!(
                svg,
                "{}{} {} ",
                if i == 0 { "M" } else { "L" },
                (p[0] - x0) * scale + 16.,
                (p[1] - y0) * scale + 16.
            );
        }
        svg.push_str("\"/>");
        if stroke.len() == 1 {
            let p = stroke[0];
            let _ = write!(
                svg,
                "<circle cx=\"{}\" cy=\"{}\" r=\"0.875\" fill=\"black\"/>",
                (p[0] - x0) * scale + 16.,
                (p[1] - y0) * scale + 16.
            );
        }
    }
    svg.push_str("</svg>");
    let pixmap = folio_export::raster_svg(&svg, 4.).map_err(io)?;
    let image = image::RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixmap.data().to_vec())
        .ok_or("Invalid ink image")?;
    let image = image::DynamicImage::ImageRgba8(image)
        .resize_exact(
            width as u32,
            height as u32,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgb8();
    let mut output = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(io)?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_runtime_zip_extracts_files_and_rejects_traversal() {
        let root = root();
        let archive = root.join("runtime.zip");
        let generation = AtomicU64::new(1);
        let process = Mutex::new(None);
        let stage = |_: String| {};
        let context = Context {
            generation: 1,
            current: &generation,
            process: &process,
            progress: &stage,
        };
        for (name, valid) in [
            ("llama-server.exe", true),
            ("../escape.exe", false),
            ("C:/escape.exe", false),
            ("folder\\escape.exe", false),
        ] {
            let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"fixture").unwrap();
            writer.finish().unwrap();
            let destination = root.join(Id::new_v4().to_string());
            fs::create_dir(&destination).unwrap();
            let result = extract_zip(&archive, &destination, &context);
            assert_eq!(result.is_ok(), valid, "{name}: {result:?}");
            if valid {
                assert_eq!(fs::read(destination.join(name)).unwrap(), b"fixture");
            }
            assert!(!root.join("escape.exe").exists());
        }
        fs::remove_dir_all(root).unwrap();
    }
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("folio-ocr-native-{}", Id::new_v4()));
        fs::create_dir(&root).unwrap();
        root
    }
    fn mock_server(data: Vec<u8>, correct_range: bool) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = thread::spawn(move || {
            for _ in 0..if correct_range { 1 } else { 3 } {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = String::new();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                let range = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                    .unwrap()
                    .split_once("bytes=")
                    .unwrap()
                    .1;
                let (start, end) = range.split_once('-').unwrap();
                let start: usize = start.parse().unwrap();
                let end: usize = end.trim().parse().unwrap();
                if correct_range {
                    write!(socket,"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",data.len(),end-start+1).unwrap();
                    let _ = socket.write_all(&data[start..=end]);
                } else {
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        data.len()
                    )
                    .unwrap();
                    let _ = socket.write_all(&data);
                }
            }
        });
        (format!("http://{address}"), thread)
    }
    #[test]
    fn resumes_partial_verifies_and_reuses_without_network() {
        let root = root();
        let path = root.join("model.gguf");
        let data = vec![42u8; 300000];
        fs::write(path.with_extension("partial"), &data[..713]).unwrap();
        let sha = format!("{:x}", Sha256::digest(&data));
        let (url, thread) = mock_server(data.clone(), true);
        let generation = AtomicU64::new(1);
        let process = Mutex::new(None);
        let stage = |_: String| {};
        let context = Context {
            generation: 1,
            current: &generation,
            process: &process,
            progress: &stage,
        };
        let http = Http::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        download(&http, &url, &path, data.len() as u64, &sha, 0, &context).unwrap();
        thread.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), data);
        assert!(!path.with_extension("partial").exists());
        download(
            &http,
            "http://127.0.0.1:1",
            &path,
            data.len() as u64,
            &sha,
            0,
            &context,
        )
        .unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_corrupt_complete_partial_without_activation() {
        let root = root();
        let path = root.join("model.gguf");
        fs::write(path.with_extension("partial"), b"bad").unwrap();
        let generation = AtomicU64::new(1);
        let process = Mutex::new(None);
        let stage = |_: String| {};
        let context = Context {
            generation: 1,
            current: &generation,
            process: &process,
            progress: &stage,
        };
        let sha = format!("{:x}", Sha256::digest(b"yes"));
        let error = download(
            &Http::new(),
            "http://127.0.0.1:1",
            &path,
            3,
            &sha,
            0,
            &context,
        )
        .unwrap_err();
        assert!(error.contains("checksum"));
        assert!(!path.exists());
        assert!(!path.with_extension("partial").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn refuses_whole_file_response_when_resuming() {
        let root = root();
        let path = root.join("model.gguf");
        fs::write(path.with_extension("partial"), b"ye").unwrap();
        let (url, thread) = mock_server(b"yes".to_vec(), false);
        let generation = AtomicU64::new(1);
        let process = Mutex::new(None);
        let stage = |_: String| {};
        let context = Context {
            generation: 1,
            current: &generation,
            process: &process,
            progress: &stage,
        };
        let sha = format!("{:x}", Sha256::digest(b"yes"));
        let http = Http::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        assert!(download(&http, &url, &path, 3, &sha, 0, &context).is_err());
        thread.join().unwrap();
        assert_eq!(fs::read(path.with_extension("partial")).unwrap(), b"ye");
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_keeps_partial_and_prevents_network_or_activation() {
        let root = root();
        let path = root.join("model.gguf");
        fs::write(path.with_extension("partial"), b"ye").unwrap();
        let generation = AtomicU64::new(2);
        let process = Mutex::new(None);
        let stage = |_: String| {};
        let context = Context {
            generation: 1,
            current: &generation,
            process: &process,
            progress: &stage,
        };
        assert!(
            download(
                &Http::new(),
                "http://127.0.0.1:1",
                &path,
                3,
                "invalid",
                0,
                &context
            )
            .unwrap_err()
            .contains("cancelled")
        );
        assert_eq!(fs::read(path.with_extension("partial")).unwrap(), b"ye");
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn waiting_for_another_install_can_be_cancelled_without_publishing_a_pack() {
        let root = root();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("setup.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let generation = AtomicU64::new(1);
        let process = Mutex::new(None);
        let stage = |text: String| {
            if text.starts_with("Waiting") {
                generation.store(2, Ordering::Release);
            }
        };
        let context = Context {
            generation: 1,
            current: &generation,
            process: &process,
            progress: &stage,
        };
        let pack = root.join("pack.json");
        assert!(install(&pack, &context).unwrap_err().contains("cancelled"));
        assert!(!pack.exists());
        assert!(!root.join("glm-ocr-q8-b11457").exists());
        drop(lock);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn composites_transparent_ocr_images_onto_white() {
        let root = root();
        let path = root.join("transparent.png");
        let mut image = image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 0]));
        image.put_pixel(1, 0, image::Rgba([0, 0, 0, 255]));
        image.save(&path).unwrap();
        let rgb = image::load_from_memory(&image_bytes(&path).unwrap())
            .unwrap()
            .to_rgb8();
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(rgb.get_pixel(1, 0).0, [0, 0, 0]);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn prefers_discrete_gpu_and_filters_low_memory_and_software() {
        let output = "Available devices:\n Vulkan0: AMD Radeon 780M Graphics (RADV PHOENIX) (16000 MiB, 13000 MiB free)\n Vulkan1: NVIDIA RTX 4070 (8000 MiB, 7000 MiB free)\n Vulkan2: llvmpipe (16000 MiB, 16000 MiB free)\n Vulkan3: Intel Arc (4000 MiB, 900 MiB free)";
        let found = devices(output);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "Vulkan1");
        assert_eq!(found[1].id, "Vulkan0");
    }
    #[test]
    fn renders_bounded_ink_and_preserves_formula_meaning() {
        let png = render_ink(&[vec![[0., 0.], [f32::MAX, 0.]]], RecognitionKind::Text).unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert!(image.width() <= 1432);
        assert!(image.height() <= 1032);
        assert!(render_ink(&[vec![[f32::NAN, 0.]]], RecognitionKind::Text).is_err());
        assert!(render_ink(&[vec![]], RecognitionKind::Text).is_err());
        assert_eq!(strip_math_wrappers("```latex\n\\[x^2=4\\]\n```"), "x^2=4");
        assert_eq!(strip_math_wrappers("$x$ + $y$"), "$x$ + $y$");
    }
    #[test]
    #[ignore = "Requires FOLIO_OCR_TEST_ASSETS with the pinned public benchmark files; runs real OCR"]
    fn real_q8_first_use_text_math_cpu_and_gpu() {
        let assets = PathBuf::from(
            std::env::var_os("FOLIO_OCR_TEST_ASSETS").expect("set public fixture directory"),
        );
        let root = assets.join(format!("validation-{}", Id::new_v4()));
        fs::create_dir(&root).unwrap();
        let pack = root.join("recognition/pack.json");
        let model_root = root.join("recognition/glm-ocr-q8-b11457");
        fs::create_dir_all(&model_root).unwrap();
        for (name, _, _) in ASSETS {
            // Leave the small runtime archive absent when explicitly testing the real downloader.
            if name == RUNTIME && std::env::var_os("FOLIO_OCR_TEST_DOWNLOAD").is_some() {
                continue;
            }
            fs::hard_link(assets.join(name), model_root.join(name)).unwrap();
        }
        let generation = AtomicU64::new(1);
        let process = Mutex::new(None);
        let stage = |text: String| eprintln!("{text}");
        let context = Context {
            generation: 1,
            current: &generation,
            process: &process,
            progress: &stage,
        };
        struct Guard<'a>(&'a Mutex<Option<Child>>);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                stop_process(self.0);
            }
        }
        let _guard = Guard(&process);
        for device in ["auto", "Vulkan0", "Vulkan999", "cpu"] {
            if device != "auto" {
                fs::write(
                    &pack,
                    serde_json::to_vec(
                        &serde_json::json!({"backend":"llama-vulkan","device":device}),
                    )
                    .unwrap(),
                )
                .unwrap();
            }
            let client = Native::load(&pack, &context).unwrap();
            assert!(pack.is_file());
            assert!(model_root.join("GLM-OCR-NOTICE.txt").is_file());
            if device == "cpu" || device == "Vulkan999" {
                assert!(client.label.starts_with("CPU"));
            }
            if device == "auto" {
                assert!(client.label.contains("NVIDIA"));
            }
            if device == "Vulkan0" {
                assert!(client.label.contains("AMD"));
            }
            let samples = assets.parent().unwrap().join("recognition-2026-10-04");
            for (kind, name) in [
                (RecognitionKind::Text, "text"),
                (RecognitionKind::Math, "math"),
            ] {
                let samples: serde_json::Value = serde_json::from_slice(
                    &fs::read(samples.join(format!("{name}-samples.json"))).unwrap(),
                )
                .unwrap();
                let request = Request {
                    kind,
                    strokes: vec![],
                    image_path: Some(PathBuf::from(samples[0]["image"].as_str().unwrap())),
                };
                let start = Instant::now();
                let text = client.recognize(&request, &context).unwrap();
                eprintln!("{device} {name} {:?}: {text}", start.elapsed());
                assert!(!text.is_empty());
                assert_eq!(text, client.recognize(&request, &context).unwrap());
            }
            stop_process(&process);
        }
        fs::remove_dir_all(root).unwrap();
    }
}
