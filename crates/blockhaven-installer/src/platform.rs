use crate::{
    engine::{self, Integration, Options, Progress},
    network::{self, DownloadKind},
    Result, WEBVIEW_BOOTSTRAPPER_URL,
};
use std::{
    ffi::OsString,
    fs,
    io::Read,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use windows::{
    core::{Interface, PCWSTR},
    Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{
            FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_RoamingAppData, IShellLinkW,
            SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT,
        },
    },
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::{
        Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ},
        Threading::CreateMutexW,
    },
};

pub fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn known_folder(id: &windows::core::GUID) -> Result<PathBuf> {
    unsafe {
        let raw = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(|e| e.to_string())?;
        let mut length = 0;
        while *raw.0.add(length) != 0 {
            length += 1;
        }
        let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(
            raw.0, length,
        )));
        CoTaskMemFree(Some(raw.0.cast()));
        Ok(path)
    }
}
pub fn install_root() -> Result<PathBuf> {
    Ok(known_folder(&FOLDERID_RoamingAppData)?
        .join("Blockhaven")
        .join("launcher"))
}
pub struct InstallLock(HANDLE);
impl InstallLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        use sha2::{Digest, Sha256};
        let name = format!(
            "Local\\BlockhavenSetup-{}",
            hex::encode(Sha256::digest(
                root.to_string_lossy().to_lowercase().as_bytes()
            ))
        );
        let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("Could not acquire installation lock".into());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }
            return Err("Another Blockhaven installer/updater is already open. Use that window or close it first.".into());
        }
        Ok(Self(handle))
    }
}
impl Drop for InstallLock {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct ComApartment;
impl ComApartment {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
        }
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub fn runtime_installed() -> bool {
    let key = r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    for (hive, key) in [
        (HKEY_CURRENT_USER, key.to_owned()),
        (
            HKEY_LOCAL_MACHINE,
            key.replace("Software\\", "Software\\WOW6432Node\\"),
        ),
        (HKEY_LOCAL_MACHINE, key.to_owned()),
    ] {
        let key = key.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let name = "pv\0".encode_utf16().collect::<Vec<_>>();
        let mut value = [0u16; 128];
        let mut size = std::mem::size_of_val(&value) as u32;
        let code = unsafe {
            RegGetValueW(
                hive,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                value.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code == 0 {
            let end = value.iter().position(|v| *v == 0).unwrap_or(value.len());
            let version = String::from_utf16_lossy(&value[..end]);
            if version
                .split('.')
                .any(|part| part.parse::<u32>().is_ok_and(|n| n > 0))
            {
                return true;
            }
        }
    }
    false
}
fn shortcut(target: &Path, link: &Path) -> Result<()> {
    fs::create_dir_all(link.parent().ok_or("Invalid shortcut folder")?)
        .map_err(|e| e.to_string())?;
    let target_wide = wide(target);
    let root_wide = wide(target.parent().ok_or("Invalid updater location")?);
    // Save beside the final link and then replace it atomically.
    let staged = tempfile::Builder::new()
        .suffix(".lnk")
        .tempfile_in(link.parent().unwrap())
        .map_err(|e| e.to_string())?;
    let staged_wide = wide(staged.path());
    unsafe {
        let shell: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        shell
            .SetPath(PCWSTR(target_wide.as_ptr()))
            .map_err(|e| e.to_string())?;
        shell
            .SetWorkingDirectory(PCWSTR(root_wide.as_ptr()))
            .map_err(|e| e.to_string())?;
        let description = "Check for updates and launch Blockhaven\0"
            .encode_utf16()
            .collect::<Vec<_>>();
        shell
            .SetDescription(PCWSTR(description.as_ptr()))
            .map_err(|e| e.to_string())?;
        shell
            .SetIconLocation(PCWSTR(target_wide.as_ptr()), 0)
            .map_err(|e| e.to_string())?;
        let persist: IPersistFile = shell.cast().map_err(|e| e.to_string())?;
        persist
            .Save(PCWSTR(staged_wide.as_ptr()), true)
            .map_err(|e| e.to_string())?;
    }
    staged
        .persist(link)
        .map_err(|e| format!("Could not save shortcut {}: {e}", link.display()))?;
    Ok(())
}
fn remove_own_shortcut(link: &Path, target: &Path) -> Result<()> {
    if !link.exists() {
        return Ok(());
    }
    // Never remove an unrelated shortcut that happens to use the same name.
    unsafe {
        let shell: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let persist: IPersistFile = shell.cast().map_err(|e| e.to_string())?;
        persist
            .Load(
                PCWSTR(wide(link).as_ptr()),
                windows::Win32::System::Com::STGM_READ,
            )
            .map_err(|e| e.to_string())?;
        let mut path = [0u16; 32768];
        shell
            .GetPath(
                &mut path,
                std::ptr::null_mut(),
                windows::Win32::UI::Shell::SLGP_RAWPATH.0 as u32,
            )
            .map_err(|e| e.to_string())?;
        let length = path.iter().position(|v| *v == 0).unwrap_or(path.len());
        let existing = PathBuf::from(OsString::from_wide(&path[..length]));
        if let (Ok(existing), Ok(target)) = (fs::canonicalize(existing), fs::canonicalize(target)) {
            if existing != target {
                return Ok(());
            }
            fs::remove_file(link).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
pub struct WindowsIntegration;
impl Integration for WindowsIntegration {
    fn runtime_installed(&self) -> bool {
        runtime_installed()
    }
    fn install_runtime(
        &self,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<()> {
        engine::check_cancel(cancel)?;
        progress(Progress::status(
            "Downloading Microsoft WebView2 Runtime installer…",
        ));
        let client = network::client(DownloadKind::Microsoft)?;
        let response = client
            .get(WEBVIEW_BOOTSTRAPPER_URL)
            .send()
            .map_err(|e| format!("Could not download WebView2 installer: {e}"))?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        if response
            .content_length()
            .is_some_and(|n| n > 16 * 1024 * 1024)
        {
            return Err("Unexpected WebView2 bootstrapper size".into());
        }
        let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
        let length = response.content_length();
        let stage = network::stage_download(
            response.take(16 * 1024 * 1024 + 1),
            length,
            temporary.path(),
            cancel,
            progress,
            "WebView2",
        )?;
        let installer = temporary.path().join("MicrosoftEdgeWebview2Setup.exe");
        stage.persist(&installer).map_err(|e| e.to_string())?;
        let mut file = fs::File::open(&installer).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let mut header = [0; 2];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        drop(file);
        if !(256..=16 * 1024 * 1024).contains(&size) || &header != b"MZ" {
            return Err("Invalid WebView2 bootstrapper".into());
        }
        engine::check_cancel(cancel)?;
        progress(Progress::status(
            "Installing Microsoft WebView2 Runtime… Please wait.",
        ));
        use std::os::windows::process::CommandExt;
        let mut child = std::process::Command::new(&installer)
            .args(["/silent", "/install"])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Could not start WebView2 installation: {e}"))?;
        // Let Microsoft's installer finish safely even if Cancel is requested.
        let status = child.wait().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while !runtime_installed() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
        if !runtime_installed() {
            return Err(format!("WebView2 installation failed (exit code {}). Retry or install it from Microsoft's website.", status.code().map(|n| n.to_string()).unwrap_or_else(|| "unknown".into())));
        }
        engine::check_cancel(cancel)
    }
    fn shortcuts(&self, updater: &Path, options: &Options) -> Result<()> {
        let _com = ComApartment::new()?;
        for (enabled, folder) in [
            (options.desktop, FOLDERID_Desktop),
            (options.start_menu, FOLDERID_Programs),
        ] {
            let link = known_folder(&folder)?.join("Blockhaven.lnk");
            if enabled {
                shortcut(updater, &link)?;
            } else {
                remove_own_shortcut(&link, updater)?;
            }
        }
        Ok(())
    }
    fn launch(&self, executable: &Path) -> Result<()> {
        let mut child = std::process::Command::new(executable)
            .current_dir(executable.parent().ok_or("Invalid launcher directory")?)
            .spawn()
            .map_err(|e| format!("Could not start Blockhaven: {e}"))?;
        // Catch loader / startup failures without keeping the updater open for play.
        let deadline = Instant::now() + Duration::from_millis(900);
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("Blockhaven exited during startup ({}). Check WebView2 Runtime or run setup again.", status.code().unwrap_or(-1)));
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }
}
