use crate::{manifest::Manifest, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub desktop: bool,
    pub start_menu: bool,
    pub install_webview: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            desktop: true,
            start_menu: true,
            install_webview: true,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Progress {
    pub message: String,
    pub percent: Option<u32>,
}
impl Progress {
    pub fn status(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            percent: None,
        }
    }
}
pub trait Source {
    fn manifest(&self) -> Result<Manifest>;
    fn download(
        &self,
        manifest: &Manifest,
        root: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<NamedTempFile>;
}
pub trait Integration {
    fn runtime_installed(&self) -> bool;
    fn install_runtime(
        &self,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<()>;
    fn shortcuts(&self, updater: &Path, options: &Options) -> Result<()>;
    fn launch(&self, executable: &Path) -> Result<()>;
}
pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("Cancelled".into())
    } else {
        Ok(())
    }
}
pub fn read_options(root: &Path) -> Options {
    fs::read(root.join("installer.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn read_manifest(root: &Path) -> Option<Manifest> {
    let mut text = String::new();
    fs::File::open(root.join("manifest.txt"))
        .ok()?
        .take(8193)
        .read_to_string(&mut text)
        .ok()?;
    Manifest::parse(&text).ok()
}
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
pub fn validate_exe(path: &Path, expected: Option<&str>) -> Result<()> {
    let mut file = fs::File::open(path).map_err(|e| format!("Could not read executable: {e}"))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if !(256..=512 * 1024 * 1024).contains(&size) {
        return Err("Invalid executable size".into());
    }
    let mut dos = [0; 64];
    file.read_exact(&mut dos).map_err(|e| e.to_string())?;
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if &dos[..2] != b"MZ" || !(64..=1024 * 1024).contains(&offset) || offset + 26 > size {
        return Err("Download is not a Windows executable".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut pe = [0; 26];
    file.read_exact(&mut pe).map_err(|e| e.to_string())?;
    let characteristics = u16::from_le_bytes(pe[22..24].try_into().unwrap());
    if &pe[..4] != b"PE\0\0"
        || u16::from_le_bytes(pe[4..6].try_into().unwrap()) != 0x8664
        || u16::from_le_bytes(pe[24..26].try_into().unwrap()) != 0x20b
        || characteristics & 2 == 0
        || characteristics & 0x2000 != 0
    {
        return Err("Download must be a Windows x64 application, not a DLL".into());
    }
    let sections = u16::from_le_bytes(pe[6..8].try_into().unwrap()) as u64;
    let optional_size = u16::from_le_bytes(pe[20..22].try_into().unwrap()) as u64;
    let table_end = offset + 24 + optional_size + sections * 40;
    if !(1..=96).contains(&sections) || !(112..=4096).contains(&optional_size) || table_end > size {
        return Err("Executable headers are incomplete or invalid".into());
    }
    file.seek(SeekFrom::Start(offset + 24))
        .map_err(|e| e.to_string())?;
    let mut optional = vec![0; optional_size as usize];
    file.read_exact(&mut optional).map_err(|e| e.to_string())?;
    let header_size = u32::from_le_bytes(optional[60..64].try_into().unwrap()) as u64;
    if header_size < table_end || header_size > size {
        return Err("Executable header data is incomplete".into());
    }
    // The certificate directory uses a file offset, unlike the other PE directories.
    if optional_size >= 152 {
        let certificate = u32::from_le_bytes(optional[144..148].try_into().unwrap()) as u64;
        let certificate_size = u32::from_le_bytes(optional[148..152].try_into().unwrap()) as u64;
        if certificate_size > 0
            && (certificate < header_size || certificate + certificate_size > size)
        {
            return Err("Executable certificate data is incomplete".into());
        }
    }
    let mut has_payload = false;
    for _ in 0..sections {
        let mut section = [0; 40];
        file.read_exact(&mut section).map_err(|e| e.to_string())?;
        let raw_size = u32::from_le_bytes(section[16..20].try_into().unwrap()) as u64;
        let raw_offset = u32::from_le_bytes(section[20..24].try_into().unwrap()) as u64;
        if raw_size > 0 {
            if raw_offset < header_size || raw_offset + raw_size > size {
                return Err(
                    "Executable section data is incomplete; the installed version was kept".into(),
                );
            }
            has_payload = true;
        }
    }
    if !has_payload {
        return Err("Executable has no file-backed sections".into());
    }
    if let Some(expected) = expected {
        if hash_file(path)? != expected {
            return Err("Launcher SHA-256 does not match the manifest".into());
        }
    }
    Ok(())
}
pub fn installed_launcher(root: &Path) -> bool {
    read_manifest(root)
        .is_some_and(|m| validate_exe(&root.join("blockhaven.exe"), m.sha256.as_deref()).is_ok())
}
pub fn needs_update(root: &Path, remote: &Manifest) -> bool {
    read_manifest(root).as_ref() != Some(remote)
        || validate_exe(&root.join("blockhaven.exe"), remote.sha256.as_deref()).is_err()
}
pub fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temp = NamedTempFile::new_in(path.parent().ok_or("Missing destination folder")?)
        .map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path)
        .map_err(|e| format!("Could not replace {}: {e}", path.display()))?;
    Ok(())
}
pub fn install_payload(root: &Path, staged: NamedTempFile, manifest: &Manifest) -> Result<()> {
    validate_exe(staged.path(), manifest.sha256.as_deref())?;
    let exe = root.join("blockhaven.exe");
    let backup = if exe.is_file() {
        let backup = NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
        fs::copy(&exe, backup.path()).map_err(|e| e.to_string())?;
        Some(backup)
    } else {
        None
    };
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    staged.persist(&exe).map_err(|e| {
        format!("Could not replace Blockhaven. Close any running launcher and retry: {e}")
    })?;
    if let Err(error) = atomic_bytes(&root.join("manifest.txt"), manifest.text().as_bytes()) {
        let rollback = if let Some(backup) = backup {
            backup.persist(&exe).map(|_| ()).map_err(|e| e.to_string())
        } else {
            fs::remove_file(&exe).map_err(|e| e.to_string())
        };
        return Err(match rollback {
            Ok(()) => error,
            Err(e) => format!("{error}; rollback failed: {e}. Run setup again to repair."),
        });
    }
    Ok(())
}
fn copy_updater(root: &Path, current: &Path) -> Result<PathBuf> {
    let target = root.join("BlockhavenSetup.exe");
    if target.is_file()
        && (fs::canonicalize(current).ok() == fs::canonicalize(&target).ok()
            || hash_file(current)? == hash_file(&target)?)
    {
        return Ok(target);
    }
    let mut temp = NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    let mut input = fs::File::open(current).map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut temp).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(&target)
        .map_err(|e| format!("Could not install updater; close any other setup window: {e}"))?;
    Ok(target)
}
pub struct InstallRequest<'a> {
    pub root: &'a Path,
    pub current: &'a Path,
    pub loader: &'a [u8],
    pub options: &'a Options,
    pub cancel: &'a AtomicBool,
}
pub fn run_install(
    source: &dyn Source,
    integration: &dyn Integration,
    request: InstallRequest<'_>,
    progress: &mut dyn FnMut(Progress),
) -> Result<()> {
    let InstallRequest {
        root,
        current,
        loader,
        options,
        cancel,
    } = request;
    fs::create_dir_all(root).map_err(|e| format!("Could not create installation folder: {e}"))?;
    check_cancel(cancel)?;
    progress(Progress::status("Checking for Blockhaven updates…"));
    let remote = source.manifest()?;
    check_cancel(cancel)?;
    if needs_update(root, &remote) {
        progress(Progress::status(format!(
            "Downloading Blockhaven {}…",
            remote.version
        )));
        let stage = source.download(&remote, root, cancel, progress)?;
        check_cancel(cancel)?;
        progress(Progress::status("Installing launcher…"));
        install_payload(root, stage, &remote)?;
    } else {
        progress(Progress::status(format!(
            "Blockhaven {} is up to date",
            remote.version
        )));
    }
    check_cancel(cancel)?;
    let loader_path = root.join("WebView2Loader.dll");
    if !loader_path.is_file() || hash_file(&loader_path)? != hex::encode(Sha256::digest(loader)) {
        atomic_bytes(&loader_path, loader)?;
    }
    check_cancel(cancel)?;
    progress(Progress::status("Installing updater and shortcuts…"));
    let updater = copy_updater(root, current)?;
    integration.shortcuts(&updater, options)?;
    atomic_bytes(
        &root.join("installer.json"),
        &serde_json::to_vec_pretty(options).map_err(|e| e.to_string())?,
    )?;
    if !integration.runtime_installed() {
        if !options.install_webview {
            return Err("Blockhaven is installed, but WebView2 Runtime is missing. Enable its installation below, or install it separately, then retry.".into());
        }
        integration.install_runtime(cancel, progress)?;
        if !integration.runtime_installed() {
            return Err("WebView2 installation finished, but the runtime was not detected. Retry or install it manually.".into());
        }
    }
    check_cancel(cancel)?;
    progress(Progress {
        message: "Starting Blockhaven…".into(),
        percent: Some(100),
    });
    integration.launch(&root.join("blockhaven.exe"))
}
