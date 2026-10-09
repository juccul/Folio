//! Bounded, resumable downloads, published only after length and digest checks.
use crate::manifest::Asset;
use reqwest::{
    blocking::Client,
    header::{CONTENT_RANGE, RANGE},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub fn verified(path: &Path, asset: &Asset) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    if file.metadata().map(|m| m.len()).ok() != Some(asset.size) {
        return false;
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hash.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    format!("{:x}", hash.finalize()) == asset.sha256
}
fn range_start(value: &str, offset: u64, size: u64) -> bool {
    let Some(value) = value.strip_prefix("bytes ") else {
        return false;
    };
    let Some((range, total)) = value.split_once('/') else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    start.parse::<u64>().ok() == Some(offset)
        && end.parse::<u64>().ok() == size.checked_sub(1)
        && total.parse::<u64>().ok() == Some(size)
}
pub fn fetch(
    client: &Client,
    asset: &Asset,
    target: &Path,
    cancel: &Arc<AtomicBool>,
    mut progress: impl FnMut(u8),
) -> Result<(), String> {
    if verified(target, asset) {
        progress(100);
        return Ok(());
    }
    let partial = target.with_extension("part");
    let mut offset = partial.metadata().map(|m| m.len()).unwrap_or(0);
    if offset >= asset.size {
        if verified(&partial, asset) {
            folio_platform::publish_file(&partial, target).map_err(|e| e.to_string())?;
            progress(100);
            return Ok(());
        }
        std::fs::remove_file(&partial).map_err(|e| e.to_string())?;
        offset = 0;
    }
    let mut request = client.get(&asset.url).header("Accept-Encoding", "identity");
    if offset != 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let mut response = request
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        if !response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| range_start(h, offset, asset.size))
        {
            return Err("The server returned an invalid download range".into());
        }
    } else if response.status() == reqwest::StatusCode::OK {
        offset = 0; // Servers may ignore Range; replace rather than append.
    } else {
        return Err("Unexpected download response".into());
    }
    if response
        .content_length()
        .is_some_and(|n| n != asset.size - offset)
    {
        return Err("The update download length does not match the signed release".into());
    }
    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset != 0)
        .truncate(offset == 0)
        .open(&partial)
        .map_err(|e| e.to_string())?;
    let mut received = offset;
    let mut buffer = [0; 64 * 1024];
    let mut previous = 101;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Download paused; click Update to resume".into());
        }
        let count = response.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > asset.size {
            return Err("The update download exceeded its signed size".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        let percent = (received * 100 / asset.size).min(99) as u8;
        if percent != previous {
            progress(percent);
            previous = percent;
        }
    }
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    if !verified(&partial, asset) {
        // A damaged prefix must not survive and poison every retry.
        if received == asset.size {
            let _ = std::fs::remove_file(&partial);
        }
        return Err(
            "The update is incomplete or its checksum did not match; click Update to retry".into(),
        );
    }
    folio_platform::publish_file(&partial, target).map_err(|e| e.to_string())?;
    progress(100);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread};
    fn serve(status: &str, headers: &str, body: &[u8]) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response = [
            format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
                body.len()
            )
            .into_bytes(),
            body.to_vec(),
        ]
        .concat();
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![];
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream.write_all(&response).unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}/update"), task)
    }
    fn asset(url: String, bytes: &[u8]) -> Asset {
        Asset {
            url,
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size: bytes.len() as u64,
        }
    }
    #[test]
    fn resumes_partial_and_handles_servers_that_ignore_range() {
        for ranged in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("update.exe");
            std::fs::write(path.with_extension("part"), b"first").unwrap();
            let bytes = b"first-second";
            let (url, task) = if ranged {
                serve(
                    "206 Partial Content",
                    "Content-Range: bytes 5-11/12\r\n",
                    &bytes[5..],
                )
            } else {
                serve("200 OK", "", bytes)
            };
            let asset = asset(url, bytes);
            fetch(
                &Client::new(),
                &asset,
                &path,
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap();
            assert!(
                task.join()
                    .unwrap()
                    .to_ascii_lowercase()
                    .contains("range: bytes=5-")
            );
            assert!(verified(&path, &asset));
            assert!(!path.with_extension("part").exists());
        }
    }
    #[test]
    fn rejects_bad_ranges_and_corruption_without_publishing() {
        for (status, headers, body) in [
            (
                "206 Partial Content",
                "Content-Range: bytes 1-11/12\r\n",
                &b"-second"[..],
            ),
            ("200 OK", "", &b"first-broken"[..]),
        ] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("update.exe");
            std::fs::write(path.with_extension("part"), b"first").unwrap();
            let (url, task) = serve(status, headers, body);
            let asset = asset(url, b"first-second");
            assert!(
                fetch(
                    &Client::new(),
                    &asset,
                    &path,
                    &Arc::new(AtomicBool::new(false)),
                    |_| {}
                )
                .is_err()
            );
            task.join().unwrap();
            assert!(!path.exists());
            if status == "200 OK" {
                assert!(!path.with_extension("part").exists());
            }
        }
    }
    #[test]
    fn cache_requires_both_digest_and_length_and_never_connects_when_valid() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("update.exe");
        let asset = asset("http://127.0.0.1:1/never".into(), b"verified");
        std::fs::write(&path, b"verified").unwrap();
        fetch(
            &Client::new(),
            &asset,
            &path,
            &Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();
        std::fs::write(&path, b"tampered").unwrap();
        assert!(!verified(&path, &asset));
        std::fs::write(&path, b"verifiedextra").unwrap();
        assert!(!verified(&path, &asset));
    }
}
