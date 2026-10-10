use blockhaven_installer::{
    engine::{self, Integration, Options, Progress, Source},
    manifest::Manifest,
    network::stage_download,
    Result,
};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use tempfile::NamedTempFile;

fn payload(marker: u8) -> Vec<u8> {
    let mut bytes = vec![0; 1024];
    bytes[512..].fill(marker);
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[70..72].copy_from_slice(&1u16.to_le_bytes());
    bytes[84..86].copy_from_slice(&240u16.to_le_bytes());
    bytes[86..88].copy_from_slice(&2u16.to_le_bytes());
    bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
    bytes[148..152].copy_from_slice(&512u32.to_le_bytes());
    bytes[344..348].copy_from_slice(&512u32.to_le_bytes());
    bytes[348..352].copy_from_slice(&512u32.to_le_bytes());
    bytes
}
fn manifest(version: &str) -> Manifest {
    Manifest::parse(&format!("version = {version}\nurl = https://github.com/Titanius69/Blockhaven/releases/download/{version}/blockhaven.exe\n")).unwrap()
}
struct FakeSource {
    manifest: Manifest,
    payload: Vec<u8>,
    downloads: Cell<u32>,
}
impl FakeSource {
    fn new(version: &str) -> Self {
        Self {
            manifest: manifest(version),
            payload: payload(7),
            downloads: Cell::new(0),
        }
    }
}
impl Source for FakeSource {
    fn manifest(&self) -> Result<Manifest> {
        Ok(self.manifest.clone())
    }
    fn download(
        &self,
        _: &Manifest,
        root: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<NamedTempFile> {
        self.downloads.set(self.downloads.get() + 1);
        stage_download(
            self.payload.as_slice(),
            Some(self.payload.len() as u64),
            root,
            cancel,
            progress,
            "Test",
        )
    }
}
struct FakeIntegration {
    runtime: Cell<bool>,
    events: RefCell<Vec<String>>,
    root: PathBuf,
}
impl Integration for FakeIntegration {
    fn runtime_installed(&self) -> bool {
        self.runtime.get()
    }
    fn install_runtime(&self, _: &AtomicBool, _: &mut dyn FnMut(Progress)) -> Result<()> {
        self.events.borrow_mut().push("runtime".into());
        self.runtime.set(true);
        Ok(())
    }
    fn shortcuts(&self, updater: &Path, options: &Options) -> Result<()> {
        assert_eq!(updater, self.root.join("BlockhavenSetup.exe"));
        assert_eq!(fs::read(updater).unwrap(), b"test updater");
        self.events.borrow_mut().push(format!(
            "shortcuts:{}/{}",
            options.desktop, options.start_menu
        ));
        Ok(())
    }
    fn launch(&self, exe: &Path) -> Result<()> {
        assert_eq!(exe, self.root.join("blockhaven.exe"));
        assert!(engine::installed_launcher(&self.root));
        assert!(self.root.join("installer.json").is_file());
        assert_eq!(
            fs::read(self.root.join("WebView2Loader.dll")).unwrap(),
            b"test loader"
        );
        self.events.borrow_mut().push("launch".into());
        Ok(())
    }
}
fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, FakeIntegration) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Blockhaven/launcher");
    fs::create_dir_all(&root).unwrap();
    let updater = dir.path().join("downloaded-setup.exe");
    fs::write(&updater, b"test updater").unwrap();
    let integration = FakeIntegration {
        root: root.clone(),
        runtime: Cell::new(false),
        events: RefCell::new(Vec::new()),
    };
    (dir, root, updater, integration)
}
fn run(
    source: &FakeSource,
    integration: &FakeIntegration,
    root: &Path,
    current: &Path,
    options: &Options,
) -> Result<()> {
    engine::run_install(
        source,
        integration,
        engine::InstallRequest {
            root,
            current,
            loader: b"test loader",
            options,
            cancel: &AtomicBool::new(false),
        },
        &mut |_| {},
    )
}
#[test]
fn first_install_saves_manifest_copies_updater_installs_runtime_and_launches_last() {
    let (_dir, root, updater, integration) = setup();
    let source = FakeSource::new("0.3.2");
    let options = Options {
        desktop: false,
        start_menu: true,
        install_webview: true,
    };
    run(&source, &integration, &root, &updater, &options).unwrap();
    assert_eq!(source.downloads.get(), 1);
    assert_eq!(engine::read_manifest(&root), Some(source.manifest));
    assert_eq!(
        *integration.events.borrow(),
        ["shortcuts:false/true", "runtime", "launch"]
    );
    let saved = engine::read_options(&root);
    assert!(!saved.desktop && saved.start_menu && saved.install_webview);
}
#[test]
fn same_manifest_skips_download_and_self_copy_but_repairs_missing_executable() {
    let (_dir, root, updater, integration) = setup();
    let source = FakeSource::new("0.3.2");
    run(&source, &integration, &root, &updater, &Options::default()).unwrap();
    run(
        &source,
        &integration,
        &root,
        &root.join("BlockhavenSetup.exe"),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(source.downloads.get(), 1);
    fs::remove_file(root.join("blockhaven.exe")).unwrap();
    run(&source, &integration, &root, &updater, &Options::default()).unwrap();
    assert_eq!(source.downloads.get(), 2);
    assert_eq!(
        integration
            .events
            .borrow()
            .iter()
            .filter(|e| *e == "runtime")
            .count(),
        1
    );
}
#[test]
fn version_or_url_changes_and_corrupt_local_binary_trigger_updates() {
    let (_dir, root, updater, integration) = setup();
    let source = FakeSource::new("0.3.2");
    run(&source, &integration, &root, &updater, &Options::default()).unwrap();
    let next = FakeSource::new("0.3.3");
    assert!(engine::needs_update(&root, &next.manifest));
    run(&next, &integration, &root, &updater, &Options::default()).unwrap();
    assert_eq!(engine::read_manifest(&root).unwrap().version, "0.3.3");
    let different = Manifest {
        url: next.manifest.url.replace("blockhaven.exe", "alternate.exe"),
        ..next.manifest.clone()
    };
    assert!(engine::needs_update(&root, &different));
    fs::write(root.join("blockhaven.exe"), b"broken").unwrap();
    assert!(engine::needs_update(&root, &next.manifest));
}
#[test]
fn invalid_or_checksum_mismatched_downloads_preserve_previous_installation() {
    let (_dir, root, updater, integration) = setup();
    let old = FakeSource::new("0.3.2");
    run(&old, &integration, &root, &updater, &Options::default()).unwrap();
    let old_bytes = fs::read(root.join("blockhaven.exe")).unwrap();
    let old_manifest = fs::read(root.join("manifest.txt")).unwrap();
    for bad in [
        FakeSource {
            payload: b"<html>404</html>".to_vec(),
            ..FakeSource::new("0.3.3")
        },
        FakeSource {
            manifest: Manifest {
                sha256: Some("0".repeat(64)),
                ..manifest("0.3.3")
            },
            ..FakeSource::new("0.3.3")
        },
    ] {
        assert!(run(&bad, &integration, &root, &updater, &Options::default()).is_err());
        assert_eq!(fs::read(root.join("blockhaven.exe")).unwrap(), old_bytes);
        assert_eq!(fs::read(root.join("manifest.txt")).unwrap(), old_manifest);
    }
}
#[test]
fn metadata_commit_failure_rolls_back_previous_binary() {
    let (_dir, root, updater, integration) = setup();
    let old = payload(3);
    fs::write(root.join("blockhaven.exe"), &old).unwrap();
    fs::create_dir(root.join("manifest.txt")).unwrap();
    assert!(run(
        &FakeSource::new("0.3.3"),
        &integration,
        &root,
        &updater,
        &Options::default()
    )
    .is_err());
    assert_eq!(fs::read(root.join("blockhaven.exe")).unwrap(), old);
    assert!(integration.events.borrow().is_empty());
}
#[test]
fn truncated_pe_with_valid_magic_never_replaces_installed_release() {
    let (_dir, root, updater, integration) = setup();
    let old = FakeSource::new("0.3.2");
    run(&old, &integration, &root, &updater, &Options::default()).unwrap();
    let mut truncated = FakeSource::new("0.3.3");
    truncated.payload.truncate(600);
    assert!(run(
        &truncated,
        &integration,
        &root,
        &updater,
        &Options::default()
    )
    .unwrap_err()
    .contains("incomplete"));
    assert_eq!(fs::read(root.join("blockhaven.exe")).unwrap(), old.payload);
    assert_eq!(engine::read_manifest(&root).unwrap().version, "0.3.2");
}
#[test]
fn unchecked_runtime_never_installs_it_and_does_not_launch_without_it() {
    let (_dir, root, updater, integration) = setup();
    let options = Options {
        install_webview: false,
        ..Options::default()
    };
    assert!(run(
        &FakeSource::new("0.3.2"),
        &integration,
        &root,
        &updater,
        &options
    )
    .unwrap_err()
    .contains("missing"));
    assert_eq!(*integration.events.borrow(), ["shortcuts:true/true"]);
    assert!(root.join("BlockhavenSetup.exe").is_file());
    integration.runtime.set(true);
    run(
        &FakeSource::new("0.3.2"),
        &integration,
        &root,
        &updater,
        &options,
    )
    .unwrap();
    assert_eq!(
        *integration.events.borrow(),
        ["shortcuts:true/true", "shortcuts:true/true", "launch"]
    );
}
#[test]
fn cancellation_keeps_install_and_prevents_launch() {
    let (_dir, root, updater, integration) = setup();
    let cancelled = AtomicBool::new(true);
    assert!(engine::run_install(
        &FakeSource::new("0.3.2"),
        &integration,
        engine::InstallRequest {
            root: &root,
            current: &updater,
            loader: b"test loader",
            options: &Options::default(),
            cancel: &cancelled
        },
        &mut |_| {}
    )
    .is_err());
    assert!(!root.join("blockhaven.exe").exists());
    assert!(integration.events.borrow().is_empty());
}
