use crate::model::{Progress, Result};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use sha1::Digest;
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path},
    sync::Arc,
    time::Duration,
};
#[derive(Clone)]
pub struct Net {
    pub client: Client,
}
impl Net {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent("Blockhaven/0.3.2 (FastChapp desktop; personal launcher)")
                .connect_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(180))
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() >= 5 || check_url(attempt.url().as_str()).is_err() {
                        attempt.error("Unapproved redirect")
                    } else {
                        attempt.follow()
                    }
                }))
                .build()
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn text(&self, url: &str) -> Result<String> {
        check_url(url)?;
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|_| "Checksum request failed")?
            .error_for_status()
            .map_err(|_| "Checksum was not found")?;
        let mut text = String::new();
        response
            .take(4096)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        Ok(text)
    }
    pub fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        check_url(url)?;
        self.client
            .get(url)
            .send()
            .map_err(|_| format!("Network request failed: {}", host(url)))?
            .error_for_status()
            .map_err(|e| {
                format!(
                    "HTTP {} from {}",
                    e.status().map(|s| s.as_u16()).unwrap_or(0),
                    host(url)
                )
            })?
            .json()
            .map_err(|_| format!("Invalid JSON from {}", host(url)))
    }
    pub fn download(&self, item: &Download, root: &Path, progress: &Arc<Progress>) -> Result<()> {
        progress.check()?;
        check_url(&item.url)?;
        let path = safe_join(root, &item.path)?;
        if path.is_file() && verify(&path, &item.algorithm, &item.hash, item.size).is_ok() {
            return Ok(());
        }
        let parent = path.parent().ok_or("Invalid download path")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        reject_symlinks(root, &path)?;
        let mut response = self
            .client
            .get(&item.url)
            .send()
            .map_err(|_| format!("Download failed: {}", host(&item.url)))?
            .error_for_status()
            .map_err(|e| {
                format!(
                    "Download returned {}",
                    e.status().map(|s| s.as_u16()).unwrap_or(0)
                )
            })?;
        if response.content_length().unwrap_or(0) > 1024 * 1024 * 1024 {
            return Err("Download exceeds 1 GiB safety limit".into());
        }
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let mut buffer = [0; 64 * 1024];
        let mut total = 0u64;
        loop {
            progress.check()?;
            let n = response.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > 1024 * 1024 * 1024 {
                return Err("Download exceeds 1 GiB".into());
            }
            temp.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        temp.flush().map_err(|e| e.to_string())?;
        verify(temp.path(), &item.algorithm, &item.hash, item.size)?;
        temp.persist(&path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown host".into())
}
pub fn check_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid download URL")?;
    let host = url.host_str().unwrap_or("");
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Only credential-free HTTPS URLs are allowed".into());
    }
    let allowed = [
        "piston-meta.mojang.com",
        "piston-data.mojang.com",
        "launchermeta.mojang.com",
        "launcher.mojang.com",
        "libraries.minecraft.net",
        "resources.download.minecraft.net",
        "maven.fabricmc.net",
        "meta.fabricmc.net",
        "api.modrinth.com",
        "cdn.modrinth.com",
    ];
    if !allowed.contains(&host) {
        return Err(format!("Unapproved download host: {host}"));
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub path: String,
    pub algorithm: String,
    pub hash: String,
    pub size: Option<u64>,
}
pub fn verify(path: &Path, algorithm: &str, expected: &str, size: Option<u64>) -> Result<()> {
    if let Some(size) = size {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() != size {
            return Err("Downloaded file size mismatch".into());
        }
    }
    if expected.is_empty() {
        return Err("Download has no integrity hash".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = [0; 64 * 1024];
    let mut sha1 = sha1::Sha1::new();
    let mut sha512 = sha2::Sha512::new();
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if algorithm == "sha1" {
            sha1.update(&buffer[..n])
        } else if algorithm == "sha512" {
            sha512.update(&buffer[..n])
        } else {
            return Err("Unsupported hash algorithm".into());
        }
    }
    let actual = if algorithm == "sha1" {
        hex::encode(sha1.finalize())
    } else {
        hex::encode(sha512.finalize())
    };
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err("Downloaded file hash mismatch".into())
    }
}
pub fn safe_component(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 180
        || name == "."
        || name == ".."
        || name.ends_with('.')
        || name.ends_with(' ')
        || name.chars().any(|c| c < ' ' || "/\\:*?\"<>|".contains(c))
    {
        return Err("Unsafe file name".into());
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        return Err("Reserved Windows file name".into());
    }
    Ok(())
}
pub fn safe_join(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    if relative.contains('\\') {
        return Err("Unsafe relative path".into());
    }
    let rel = Path::new(relative);
    if rel.as_os_str().is_empty() {
        return Err("Empty path".into());
    }
    for part in rel.components() {
        match part {
            Component::Normal(name) => safe_component(name.to_str().ok_or("Non-UTF8 path")?)?,
            _ => return Err("Path traversal rejected".into()),
        }
    }
    let path = root.join(rel);
    reject_symlinks(root, &path)?;
    Ok(path)
}
pub fn reject_symlinks(root: &Path, path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Symlink in managed path".into());
        }
        if ancestor == root {
            break;
        }
    }
    Ok(())
}
pub fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().ok_or("Invalid configuration path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn download_all(
    net: &Net,
    items: &[Download],
    root: &Path,
    progress: &Arc<Progress>,
    workers: usize,
    label: &str,
) -> Result<()> {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    let index = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let error = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, 16) {
            scope.spawn(|| loop {
                if error.lock().unwrap().is_some() {
                    break;
                }
                let i = index.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                if let Err(e) = net.download(&items[i], root, progress) {
                    *error.lock().unwrap() = Some(e);
                    break;
                }
                let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress.set(
                    count,
                    items.len(),
                    format!("{label}: {count}/{}", items.len()),
                );
            });
        }
    });
    match error.into_inner().unwrap() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_urls_and_hashes_are_validated() {
        let dir = tempfile::tempdir().unwrap();
        for p in ["../x", "/x", "a/../../x", "a\\x", "C:/x", "CON.jar"] {
            assert!(safe_join(dir.path(), p).is_err(), "{p}")
        }
        assert!(safe_join(dir.path(), "org/example/hello.jar").is_ok());
        assert!(check_url("https://cdn.modrinth.com/a.jar").is_ok());
        for url in [
            "http://cdn.modrinth.com/a",
            "https://cdn.modrinth.com.evil/a",
            "https://localhost/a",
            "https://user@cdn.modrinth.com/a",
        ] {
            assert!(check_url(url).is_err())
        }
        let p = dir.path().join("x");
        fs::write(&p, b"hello").unwrap();
        assert!(verify(
            &p,
            "sha1",
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d",
            Some(5)
        )
        .is_ok());
        assert!(verify(&p, "sha1", "000000", None).is_err());
    }
}
