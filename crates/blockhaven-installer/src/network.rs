use crate::{
    engine::{check_cancel, validate_exe, Progress, Source},
    manifest::Manifest,
    Result, MANIFEST_URL,
};
use reqwest::{blocking::Client, Url};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::Duration,
};
use tempfile::NamedTempFile;

#[derive(Clone, Copy)]
pub enum DownloadKind {
    Github,
    Microsoft,
}
pub fn approved(url: &Url, kind: DownloadKind) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    let host = url.host_str().unwrap_or("");
    match kind {
        DownloadKind::Github => matches!(
            host,
            "github.com"
                | "raw.githubusercontent.com"
                | "release-assets.githubusercontent.com"
                | "objects.githubusercontent.com"
        ),
        DownloadKind::Microsoft => {
            host == "microsoft.com"
                || host.ends_with(".microsoft.com")
                || host.ends_with(".msedge.net")
        }
    }
}
pub fn client(kind: DownloadKind) -> Result<Client> {
    Client::builder()
        .user_agent("BlockhavenSetup/0.1.0")
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 8 || !approved(attempt.url(), kind) {
                attempt.error("Unapproved download redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}
pub struct GithubSource {
    client: Client,
}
impl GithubSource {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: client(DownloadKind::Github)?,
        })
    }
}
pub fn stage_download(
    mut reader: impl Read,
    length: Option<u64>,
    root: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    label: &str,
) -> Result<NamedTempFile> {
    const MAX: u64 = 512 * 1024 * 1024;
    if length.is_some_and(|n| n > MAX) {
        return Err("Download exceeds 512 MiB".into());
    }
    let mut temp = NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    let mut buffer = [0; 65536];
    let mut received = 0;
    let mut last_percent = None;
    let mut last_mb = u64::MAX;
    loop {
        check_cancel(cancel)?;
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("Download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        received += n as u64;
        if received > MAX {
            return Err("Download exceeds 512 MiB".into());
        }
        temp.write_all(&buffer[..n])
            .map_err(|e| format!("Could not save download: {e}"))?;
        let percent = length
            .filter(|n| *n > 0)
            .map(|n| ((received.saturating_mul(100) / n).min(100)) as u32);
        let mb = received / (1024 * 1024);
        if percent != last_percent || mb != last_mb {
            progress(Progress {
                message: format!(
                    "{label}: {:.1} MiB downloaded",
                    received as f64 / (1024.0 * 1024.0)
                ),
                percent,
            });
            last_percent = percent;
            last_mb = mb;
        }
    }
    if length.is_some_and(|n| n != received) {
        return Err("Download is incomplete; the installed launcher was kept".into());
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    Ok(temp)
}
impl Source for GithubSource {
    fn manifest(&self) -> Result<Manifest> {
        let response = self
            .client
            .get(MANIFEST_URL)
            .header("Cache-Control", "no-cache")
            .timeout(Duration::from_secs(20))
            .send()
            .map_err(|e| format!("Could not check updates: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Manifest request failed: {e}"))?;
        let mut text = String::new();
        response
            .take(8193)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        Manifest::parse(&text)
    }
    fn download(
        &self,
        manifest: &Manifest,
        root: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<NamedTempFile> {
        let response = self
            .client
            .get(&manifest.url)
            .send()
            .map_err(|e| format!("Could not download launcher: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Launcher download failed: {e}"))?;
        let length = response.content_length();
        let stage = stage_download(response, length, root, cancel, progress, "Blockhaven")?;
        validate_exe(stage.path(), manifest.sha256.as_deref())?;
        Ok(stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_rejects_truncation_limits_and_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        assert!(stage_download(
            &b"abc"[..],
            Some(5),
            root.path(),
            &cancel,
            &mut |_| {},
            "Test"
        )
        .is_err());
        assert!(stage_download(
            &b"abc"[..],
            Some(600 * 1024 * 1024),
            root.path(),
            &cancel,
            &mut |_| {},
            "Test"
        )
        .is_err());
        let cancelled = AtomicBool::new(true);
        assert!(stage_download(
            &b"abc"[..],
            Some(3),
            root.path(),
            &cancelled,
            &mut |_| {},
            "Test"
        )
        .is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[test]
    fn redirects_remain_on_trusted_https_hosts() {
        for (url, kind, valid) in [
            (
                "https://release-assets.githubusercontent.com/x",
                DownloadKind::Github,
                true,
            ),
            ("http://github.com/x", DownloadKind::Github, false),
            ("https://evil.example/x", DownloadKind::Github, false),
            (
                "https://msedge.sf.dl.delivery.mp.microsoft.com/x",
                DownloadKind::Microsoft,
                true,
            ),
            (
                "https://microsoft.com.evil.example/x",
                DownloadKind::Microsoft,
                false,
            ),
        ] {
            assert_eq!(approved(&Url::parse(url).unwrap(), kind), valid);
        }
    }
}
