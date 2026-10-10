pub mod engine;
pub mod manifest;
#[cfg(target_os = "windows")]
pub mod native;
pub mod network;
#[cfg(target_os = "windows")]
pub mod platform;

pub type Result<T> = std::result::Result<T, String>;
pub const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/Titanius69/Blockhaven/refs/heads/main/manifest.txt";
pub const WEBVIEW_BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
