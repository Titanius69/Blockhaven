use blockhaven_installer::{
    engine::Source,
    network::{client, DownloadKind, GithubSource},
    WEBVIEW_BOOTSTRAPPER_URL,
};
use std::{io::Read, sync::atomic::AtomicBool};

#[test]
#[ignore = "downloads the current GitHub release into a temporary directory; never executes it"]
fn live_manifest_and_release_validate() {
    let source = GithubSource::new().unwrap();
    let manifest = source.manifest().unwrap();
    let root = tempfile::tempdir().unwrap();
    let staged = source
        .download(&manifest, root.path(), &AtomicBool::new(false), &mut |_| {})
        .unwrap();
    println!(
        "Verified GitHub launcher {}: {} bytes",
        manifest.version,
        staged.as_file().metadata().unwrap().len()
    );
}
#[test]
#[ignore = "reads the Microsoft bootstrapper header; never executes or installs it"]
fn live_microsoft_bootstrapper_redirect_is_valid() {
    let mut response = client(DownloadKind::Microsoft)
        .unwrap()
        .get(WEBVIEW_BOOTSTRAPPER_URL)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();
    let mut header = [0; 2];
    response.read_exact(&mut header).unwrap();
    assert_eq!(&header, b"MZ");
}
