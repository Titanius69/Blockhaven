mod artwork;
mod auth;
mod minecraft;
mod model;
mod mods;
mod net;
mod presence;
mod ui;
use fastchapp_desktop::{ActionEvent, Asset, DesktopApp, UiUpdate};
use model::*;
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    sync::{atomic::Ordering, Arc, Mutex},
};
include!(concat!(env!("OUT_DIR"), "/resources.rs"));
fn commit(s: &mut State, config: Config) -> Result<()> {
    validate_config(&config)?;
    net::atomic_json(&s.config_path, &config)?;
    s.config = config;
    Ok(())
}
fn redraw(s: &State) -> UiUpdate {
    UiUpdate::new().html("body", ui::shell(s))
}
fn job(
    shared: &Shared,
    label: &str,
    work: impl FnOnce(Shared, Arc<Progress>) -> Result<()> + Send + 'static,
) -> Result<()> {
    let progress;
    {
        let mut s = shared.lock().unwrap();
        s.idle()?;
        s.busy = true;
        s.error.clear();
        s.status = label.into();
        s.progress = Arc::new(Progress::default());
        progress = s.progress.clone();
        s.full_dirty = true;
        s.log(label);
    }
    let label = label.to_owned();
    let shared = shared.clone();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            work(shared.clone(), progress)
        }));
        let mut s = shared.lock().unwrap();
        s.busy = false;
        s.device_code.clear();
        s.device_url.clear();
        match result {
            Ok(Ok(())) => {
                if s.status == label {
                    s.status = if s.running {
                        "Minecraft is running"
                    } else {
                        "Ready"
                    }
                    .into();
                }
                s.log("Operation completed")
            }
            Ok(Err(error)) => {
                s.status = "Failed / cancelled".into();
                s.error = error.clone();
                s.log(error)
            }
            Err(_) => {
                s.status = "Error".into();
                s.error = "Operation failed unexpectedly; see the log".into();
            }
        }
        s.full_dirty = true;
    });
    Ok(())
}
fn snapshot(shared: &Shared) -> Result<(Profile, Settings, Manifest)> {
    let s = shared.lock().unwrap();
    Ok((
        s.config.profile()?.clone(),
        s.config.settings.clone(),
        s.manifest.clone(),
    ))
}
fn tick(shared: &Shared) -> UiUpdate {
    let mut s = shared.lock().unwrap();
    let visibility = s.pending_visibility.take();
    if !s.busy
        && (s.full_dirty
            || (s.page == "installed"
                && s.manual_mods_checked.elapsed() >= std::time::Duration::from_secs(2)))
        && s.refresh_manual_mods()
        && s.page == "installed"
    {
        s.full_dirty = true;
    }
    if s.full_dirty {
        s.full_dirty = false;
        return visibility_update(redraw(&s), visibility);
    }
    let label = s.progress.detail.lock().unwrap().2.clone();
    let percent = s.progress.percent();
    let mut patch = UiUpdate::new()
        .text("#status", s.status.clone())
        .text(
            "#progress-label",
            if s.busy { label } else { String::new() },
        )
        .attr(
            "#progress",
            "value",
            Some(if s.busy {
                percent.to_string()
            } else {
                "0".into()
            }),
        )
        .attr(
            "[data-action='cancel']",
            "hidden",
            if s.busy { None } else { Some(String::new()) },
        );
    if s.icons_dirty {
        s.icons_dirty = false;
        for id in s.icons.keys() {
            let class = ui::icon_class(id);
            let title = s
                .hits
                .iter()
                .find(|h| &h.project_id == id)
                .map(|h| h.title.as_str())
                .or_else(|| {
                    s.config
                        .profiles
                        .iter()
                        .flat_map(|p| &p.mods)
                        .find(|m| &m.project == id)
                        .map(|m| m.title.as_str())
                })
                .unwrap_or("Mod");
            let art = ui::mod_art(&s, id, title);
            let content = art
                .split_once('>')
                .and_then(|(_, v)| v.rsplit_once("</div>").map(|(v, _)| v))
                .unwrap_or("");
            patch = patch.html(&format!(".{class}"), content);
        }
    }
    patch = patch.attr(
        "#progress",
        "hidden",
        if s.busy { None } else { Some(String::new()) },
    );
    if s.page == "logs" {
        patch = patch.text("#log-lines", s.logs.join("\n"))
    }
    patch = patch.text("#discord-status", s.discord_status.clone());
    visibility_update(patch, visibility)
}
fn visibility_update(update: UiUpdate, visibility: Option<bool>) -> UiUpdate {
    match visibility {
        Some(visible) => update.window(visible, visible),
        None => update,
    }
}
fn number(event: &ActionEvent, name: &str) -> Result<u32> {
    event
        .field(name)
        .parse()
        .map_err(|_| format!("Invalid number: {name}"))
}
fn handle(shared: &Shared, event: ActionEvent) -> UiUpdate {
    match action(shared, event) {
        Ok(update) => update,
        Err(error) => {
            let mut s = shared.lock().unwrap();
            s.error = error.clone();
            s.log(error);
            redraw(&s)
        }
    }
}
fn search_job(shared: &Shared, query: String, offset: usize) -> Result<()> {
    let (p, _, _) = snapshot(shared)?;
    job(shared, "Searching Modrinth", move |shared, _| {
        let result = mods::search(&net::Net::new()?, &p, &query, offset)?;
        let mut s = shared.lock().unwrap();
        s.query = query;
        s.hits = result.hits;
        s.mod_total = result.total_hits;
        s.mod_offset = offset;
        s.mod_versions.clear();
        drop(s);
        artwork::queue(&shared);
        Ok(())
    })
}
fn action(shared: &Shared, event: ActionEvent) -> Result<UiUpdate> {
    match event.action.as_str() {
        "poll" => return Ok(tick(shared)),
        "dismiss_error" => {
            shared.lock().unwrap().error.clear();
            return Ok(UiUpdate::new().text("#error", ""));
        }
        "close_mod_details" => {
            shared.lock().unwrap().mod_versions.clear();
        }
        "clear_logs" => {
            shared.lock().unwrap().logs.clear();
        }
        "refresh_installed" => {
            let mut s = shared.lock().unwrap();
            s.refresh_manual_mods();
            s.page = "installed".into();
        }
        "navigate" => {
            if ![
                "home",
                "profiles",
                "modrinth",
                "installed",
                "accounts",
                "settings",
                "logs",
            ]
            .contains(&event.value.as_str())
            {
                return Err("Unknown page".into());
            }
            let mut s = shared.lock().unwrap();
            s.page = event.value;
            s.error.clear();
            s.refresh_manual_mods();
            let update = redraw(&s);
            let icons = s.page == "installed" || s.page == "modrinth";
            drop(s);
            if icons {
                artwork::queue(shared);
            }
            return Ok(update);
        }
        "cancel" => {
            let s = shared.lock().unwrap();
            s.progress.cancel.store(true, Ordering::Relaxed);
            return Ok(UiUpdate::new().text("#status", "Cancelling…"));
        }
        "refresh" => job(shared, "Refreshing version catalog", |shared, progress| {
            progress.check()?;
            let manifest: Manifest = net::Net::new()?.get(MANIFEST_URL)?;
            if manifest.versions.is_empty() {
                return Err("Version catalog is empty".into());
            }
            let mut s = shared.lock().unwrap();
            s.manifest = manifest;
            s.online = true;
            Ok(())
        })?,
        "create_profile" => {
            let name = event.field("name").trim().to_owned();
            let version = event.field("version").to_owned();
            let loader = event.field("loader").to_owned();
            let memory = number(&event, "memory")?;
            if name.is_empty()
                || name.len() > 80
                || !(1024..=32768).contains(&memory)
                || !["vanilla", "fabric"].contains(&loader.as_str())
            {
                return Err("Invalid profile values".into());
            }
            {
                let s = shared.lock().unwrap();
                s.idle()?;
                if !s.manifest.versions.iter().any(|v| v.id == version) {
                    return Err("Choose a version from the catalog".into());
                }
            }
            let create = move |shared: Shared, progress: Arc<Progress>| -> Result<()> {
                let loader_version = if loader == "fabric" {
                    progress.check()?;
                    let items: Vec<serde_json::Value> = net::Net::new()?.get(&format!(
                        "https://meta.fabricmc.net/v2/versions/loader/{version}"
                    ))?;
                    items
                        .iter()
                        .find(|v| v["loader"]["stable"].as_bool() == Some(true))
                        .or_else(|| items.first())
                        .and_then(|v| v["loader"]["version"].as_str())
                        .ok_or("No Fabric loader supports this version")?
                        .to_owned()
                } else {
                    String::new()
                };
                let p = Profile {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    version,
                    loader,
                    loader_version,
                    java: String::new(),
                    memory_mb: memory,
                    mods: Vec::new(),
                };
                let mut s = shared.lock().unwrap();
                let mut c = s.config.clone();
                c.selected = Some(p.id.clone());
                c.profiles.push(p);
                commit(&mut s, c)?;
                Ok(())
            };
            job(shared, "Creating instance", create)?;
        }
        "select_profile" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            if !s.config.profiles.iter().any(|p| p.id == event.value) {
                return Err("Profile not found".into());
            }
            let mut c = s.config.clone();
            c.selected = Some(event.value);
            commit(&mut s, c)?;
            s.hits.clear();
            s.mod_versions.clear();
            s.mod_offset = 0;
            s.mod_total = 0;
        }
        "edit_profile" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            let mut c = s.config.clone();
            let p = c.profile_mut()?;
            p.name = event.field("name").trim().into();
            p.memory_mb = number(&event, "memory")?;
            p.java = event.field("java").trim().into();
            commit(&mut s, c)?;
        }
        "remove_profile" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            let mut c = s.config.clone();
            c.profiles.retain(|p| p.id != event.value);
            if c.selected.as_deref() == Some(&event.value) {
                c.selected = c.profiles.first().map(|p| p.id.clone());
            }
            commit(&mut s, c)?;
            s.hits.clear();
            s.mod_versions.clear();
        }
        "save_settings" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            let mut c = s.config.clone();
            c.settings = Settings {
                discord: DiscordSettings {
                    enabled: event.fields.contains_key("discord_enabled"),
                    application_id: if event.field("discord_application_id").trim().is_empty() {
                        DEFAULT_DISCORD_APPLICATION_ID.into()
                    } else {
                        event.field("discord_application_id").trim().into()
                    },
                    image_key: event.field("discord_image_key").trim().into(),
                },
                java: event.field("java").trim().into(),
                game_root: PathBuf::from(event.field("game_root")),
                memory_mb: number(&event, "memory")?,
                width: number(&event, "width")?,
                height: number(&event, "height")?,
                workers: number(&event, "workers")? as usize,
                fullscreen: event.fields.contains_key("fullscreen"),
                snapshots: event.fields.contains_key("snapshots"),
                client_id: event.field("client_id").trim().into(),
            };
            commit(&mut s, c)?;
            s.status = "Settings saved".into();
        }
        "choose_java" | "choose_root" => {
            shared.lock().unwrap().idle()?;
            #[cfg(feature = "native")]
            {
                let chosen = if event.action == "choose_java" {
                    rfd::FileDialog::new()
                        .set_title("Choose java / java.exe")
                        .pick_file()
                } else {
                    rfd::FileDialog::new()
                        .set_title("Game storage")
                        .pick_folder()
                };
                if let Some(path) = chosen {
                    return Ok(UiUpdate::new().value(
                        if event.action == "choose_java" {
                            "input[name='java']"
                        } else {
                            "input[name='game_root']"
                        },
                        path.to_string_lossy().into_owned(),
                    ));
                }
            }
        }
        "check_java" => {
            let executable = if event.field("java").trim().is_empty() {
                shared.lock().unwrap().config.settings.java.clone()
            } else {
                event.field("java").trim().to_owned()
            };
            job(shared, "Checking Java", move |shared, _| {
                let major = minecraft::java_major(&executable)?;
                let mut s = shared.lock().unwrap();
                s.log(format!("Java {major}: {executable}"));
                s.status = format!("Java {major} is available");
                Ok(())
            })?
        }
        "add_offline" => {
            let a = auth::offline(event.field("name").trim())?;
            let mut s = shared.lock().unwrap();
            s.idle()?;
            if s.config.offline_accounts.iter().any(|v| v.uuid == a.uuid) {
                return Err("This offline account already exists".into());
            }
            let mut c = s.config.clone();
            c.offline_accounts.push(a.clone());
            c.selected_account = Some(a.uuid.clone());
            commit(&mut s, c)?;
            s.session = Some(auth::offline_session(&a));
            s.status = format!("Offline: {}", a.name);
        }
        "select_account" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            let a = s
                .config
                .offline_accounts
                .iter()
                .find(|a| a.uuid == event.value)
                .ok_or("Offline account not found")?
                .clone();
            let mut c = s.config.clone();
            c.selected_account = Some(a.uuid.clone());
            commit(&mut s, c)?;
            s.session = Some(auth::offline_session(&a));
        }
        "remove_account" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            let mut c = s.config.clone();
            c.offline_accounts.retain(|a| a.uuid != event.value);
            if c.selected_account.as_deref() == Some(&event.value) {
                c.selected_account = None;
            }
            commit(&mut s, c)?;
            if s.session.as_ref().is_some_and(|a| a.uuid == event.value) {
                s.session = None;
            }
        }
        "sign_out" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            let mut c = s.config.clone();
            c.selected_account = None;
            commit(&mut s, c)?;
            s.session = None;
        }
        "sign_in" => {
            let client = shared.lock().unwrap().config.settings.client_id.clone();
            job(shared, "Microsoft sign-in", move |shared, progress| {
                let net = net::Net::new()?;
                let device = auth::device(&net, &client)?;
                {
                    let mut s = shared.lock().unwrap();
                    s.device_code = device.user_code.clone();
                    s.device_url = device.verification_uri.clone();
                    s.page = "accounts".into();
                    s.full_dirty = true;
                }
                let session = auth::poll(&net, &client, &device, &progress)?;
                let mut s = shared.lock().unwrap();
                let mut c = s.config.clone();
                c.selected_account = None;
                commit(&mut s, c)?;
                s.session = Some(session);
                Ok(())
            })?
        }
        "open_signin" => {
            let url = shared.lock().unwrap().device_url.clone();
            let url = reqwest::Url::parse(&url).map_err(|_| "No sign-in page is available")?;
            if url.scheme() != "https"
                || ![
                    Some("microsoft.com"),
                    Some("www.microsoft.com"),
                    Some("login.microsoftonline.com"),
                    Some("login.live.com"),
                ]
                .contains(&url.host_str())
            {
                return Err("Unexpected sign-in URL".into());
            }
            #[cfg(feature = "native")]
            webbrowser::open(url.as_str()).map_err(|e| e.to_string())?;
        }
        "install" => {
            let (p, settings, manifest) = snapshot(shared)?;
            job(
                shared,
                "Installing / repairing Minecraft",
                move |_, progress| {
                    minecraft::install(&net::Net::new()?, &manifest, &p, &settings, &progress)?;
                    Ok(())
                },
            )?
        }
        "play" => {
            let (p, settings, manifest) = snapshot(shared)?;
            let session = shared
                .lock()
                .unwrap()
                .session
                .clone()
                .ok_or("Select an offline account or sign in first")?;
            job(shared, "Launching Minecraft", move |shared, progress| {
                let net = net::Net::new()?;
                let session = auth::refresh(&net, &settings.client_id, &session, &progress)?;
                let path = minecraft::receipt_path(&settings.game_root, &p);
                let receipt = if path.exists() {
                    serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                        .map_err(|_| "Installation receipt is corrupt; repair the profile")?
                } else {
                    minecraft::install(&net, &manifest, &p, &settings, &progress)?
                };
                progress.check()?;
                let child = minecraft::start(&receipt, &p, &settings, &session)?;
                monitor(&shared, child, &p.id, session);
                Ok(())
            })?
        }
        "stop" => {
            let s = shared.lock().unwrap();
            let mut child = s.child.lock().unwrap();
            if let Some(child) = child.as_mut() {
                child.kill().map_err(|e| e.to_string())?;
            }
        }
        "search_mods" => search_job(
            shared,
            event.field("query").trim().chars().take(200).collect(),
            0,
        )?,
        "mods_next" | "mods_prev" => {
            let s = shared.lock().unwrap();
            let offset = if event.action == "mods_next" {
                s.mod_offset + 12
            } else {
                s.mod_offset.saturating_sub(12)
            };
            let query = s.query.clone();
            drop(s);
            search_job(shared, query, offset)?
        }
        "mod_details" | "update_mod" => {
            let (p, _, _) = snapshot(shared)?;
            let id = event.value;
            let title = if event.action == "update_mod" {
                p.mods
                    .iter()
                    .find(|m| m.project == id)
                    .ok_or("Mod not found")?
                    .title
                    .clone()
            } else {
                shared
                    .lock()
                    .unwrap()
                    .hits
                    .iter()
                    .find(|h| h.project_id == id)
                    .ok_or("Search result not found")?
                    .title
                    .clone()
            };
            job(shared, "Loading compatible versions", move |shared, _| {
                let versions = mods::versions(&net::Net::new()?, &p, &id)?;
                if versions.is_empty() {
                    return Err("No compatible version is available".into());
                }
                let mut s = shared.lock().unwrap();
                s.mod_versions = versions;
                s.mod_title = title;
                s.mod_project = id;
                s.page = "modrinth".into();
                Ok(())
            })?
        }
        "install_mod" => {
            let (p, settings, _) = snapshot(shared)?;
            let id = event.field("version").to_owned();
            let title;
            {
                let s = shared.lock().unwrap();
                if !s.mod_versions.iter().any(|v| v.id == id) {
                    return Err("Choose a compatible mod version first".into());
                }
                title = s.mod_title.clone();
            }
            job(
                shared,
                "Installing mod and dependencies",
                move |shared, progress| {
                    mods::install(
                        &net::Net::new()?,
                        &p,
                        &settings,
                        &id,
                        &title,
                        &progress,
                        |updated| {
                            let mut s = shared.lock().unwrap();
                            let mut c = s.config.clone();
                            *c.profiles
                                .iter_mut()
                                .find(|v| v.id == p.id)
                                .ok_or("Profile removed")? = updated.clone();
                            commit(&mut s, c)
                        },
                    )?;
                    let mut s = shared.lock().unwrap();
                    s.page = "installed".into();
                    s.mod_versions.clear();
                    drop(s);
                    artwork::queue(&shared);
                    Ok(())
                },
            )?
        }
        "toggle_manual_mod" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            mods::toggle_manual_mod(s.config.profile()?, &s.config.settings, &event.value)?;
            s.refresh_manual_mods();
            s.status = "Manual mod updated".into();
        }
        "toggle_mod" | "remove_mod" => {
            let mut s = shared.lock().unwrap();
            s.idle()?;
            let mut c = s.config.clone();
            let settings = c.settings.clone();
            let p = c.profile_mut()?;
            if event.action == "toggle_mod" {
                mods::toggle(p, &settings, &event.value)?
            } else {
                mods::remove(p, &settings, &event.value)?
            }
            commit(&mut s, c)?;
        }
        "open_profile" | "open_version" => {
            let s = shared.lock().unwrap();
            let path = folder_path(&s.config, &event.value, event.action == "open_version")?;
            drop(s);
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            open_folder(path)?;
            let mut s = shared.lock().unwrap();
            let message = if event.action == "open_version" {
                "Opened version folder"
            } else {
                "Opened instance folder"
            };
            s.status = message.into();
            s.log(message);
        }
        "quit" => {
            shared
                .lock()
                .unwrap()
                .progress
                .cancel
                .store(true, Ordering::Relaxed);
            return Ok(UiUpdate::new().close());
        }
        _ => return Err("Unknown action".into()),
    }
    let mut s = shared.lock().unwrap();
    s.full_dirty = false;
    if !s.busy {
        s.refresh_manual_mods();
    }
    Ok(redraw(&s))
}
fn folder_path(config: &Config, id: &str, version: bool) -> Result<PathBuf> {
    let p = if id.is_empty() {
        config.profile()?
    } else {
        config
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or("Profile not found")?
    };
    let relative = if version {
        format!("versions/{}", minecraft::install_key(p))
    } else {
        format!("instances/{}", p.id)
    };
    net::safe_join(&config.settings.game_root, &relative)
}
fn open_folder(path: PathBuf) -> Result<()> {
    if !path.is_dir() {
        return Err(format!("Folder does not exist: {}", path.display()));
    }
    let path = std::fs::canonicalize(&path)
        .map_err(|e| format!("Could not resolve folder {}: {e}", path.display()))?;
    #[cfg(all(feature = "native", target_os = "windows"))]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let mut directory = windows_shell_path(&path.as_os_str().encode_wide().collect::<Vec<_>>());
        directory.push(0);
        let verb = "open\0".encode_utf16().collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                directory.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize;
        if result <= 32 {
            return Err(format!(
                "Could not open folder {} (Windows Shell error {result})",
                path.display()
            ));
        }
    }
    #[cfg(all(feature = "native", not(target_os = "windows")))]
    {
        let executable = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let mut process = std::process::Command::new(executable);
        process.arg(path);
        let mut child = process
            .spawn()
            .map_err(|e| format!("Could not open folder: {e}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    #[cfg(not(feature = "native"))]
    let _ = path;
    Ok(())
}
#[cfg(any(all(feature = "native", target_os = "windows"), test))]
fn windows_shell_path(path: &[u16]) -> Vec<u16> {
    let unc = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
    let prefix = r"\\?\".encode_utf16().collect::<Vec<_>>();
    if let Some(rest) = path.strip_prefix(unc.as_slice()) {
        r"\\".encode_utf16().chain(rest.iter().copied()).collect()
    } else if let Some(rest) = path.strip_prefix(prefix.as_slice()) {
        rest.to_vec()
    } else {
        path.to_vec()
    }
}
fn monitor(shared: &Shared, mut child: std::process::Child, id: &str, session: Session) {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let handle;
    {
        let mut s = shared.lock().unwrap();
        s.session = Some(session);
        s.running = true;
        s.running_profile = id.into();
        s.running_since = now();
        s.pending_visibility = Some(false);
        handle = s.child.clone();
        *handle.lock().unwrap() = Some(child);
        s.log("Minecraft process started");
        s.full_dirty = true;
    }
    for reader in [
        stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let shared = shared.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(reader).lines() {
                match line {
                    Ok(line) => shared.lock().unwrap().log(line),
                    Err(_) => break,
                }
            }
        });
    }
    let shared = shared.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let result = {
            let mut child = handle.lock().unwrap();
            child.as_mut().map(|c| c.try_wait())
        };
        match result {
            Some(Ok(Some(status))) => {
                let mut s = shared.lock().unwrap();
                finish_game(&mut s);
                *handle.lock().unwrap() = None;
                s.log(format!("Minecraft exited: {status}"));
                s.status = format!("Minecraft exited: {status}");
                s.full_dirty = true;
                break;
            }
            Some(Err(error)) => {
                let mut s = shared.lock().unwrap();
                // A transient monitoring error does not mean Minecraft exited.
                s.log(format!("Waiting for Minecraft process: {error}"));
            }
            None => {
                let mut s = shared.lock().unwrap();
                finish_game(&mut s);
                break;
            }
            _ => {}
        }
    });
}
fn finish_game(s: &mut State) {
    s.running = false;
    s.running_profile.clear();
    s.running_since = 0;
    s.pending_visibility = Some(true);
    s.full_dirty = true;
}
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn windows_folder_paths_preserve_unicode_spaces_and_unc() {
        for (input, expected) in [
            (
                r"\\?\C:\Users\Ábel\Game storage",
                r"C:\Users\Ábel\Game storage",
            ),
            (r"\\?\UNC\server\Game files", r"\\server\Game files"),
            (r"D:\Blockhaven games", r"D:\Blockhaven games"),
        ] {
            let input = input.encode_utf16().collect::<Vec<_>>();
            assert_eq!(
                windows_shell_path(&input),
                expected.encode_utf16().collect::<Vec<_>>()
            );
        }
    }
    fn state() -> (tempfile::TempDir, Shared) {
        let dir = tempfile::tempdir().unwrap();
        let state = State::load(dir.path().join("config.json"), true).unwrap();
        (dir, Arc::new(Mutex::new(state)))
    }
    #[test]
    fn visibility_survives_redraw_and_short_lived_games() {
        let (_dir, shared) = state();
        {
            let mut s = shared.lock().unwrap();
            s.full_dirty = true;
            s.pending_visibility = Some(false);
        }
        let update = tick(&shared);
        assert_eq!(
            update.window,
            Some(fastchapp_desktop::WindowUpdate {
                visible: false,
                focus: false
            })
        );
        assert!(tick(&shared).window.is_none());
        {
            let mut s = shared.lock().unwrap();
            s.pending_visibility = Some(false);
            s.running = true;
            finish_game(&mut s);
        }
        let update = tick(&shared);
        assert_eq!(
            update.window,
            Some(fastchapp_desktop::WindowUpdate {
                visible: true,
                focus: true
            })
        );
        assert!(tick(&shared).window.is_none());
    }
    #[cfg(unix)]
    #[test]
    fn real_child_stop_and_crash_restore_window() {
        for stop in [false, true] {
            let (_dir, shared) = state();
            let account = auth::offline("Tester").unwrap();
            let child = std::process::Command::new("sh")
                .args([
                    "-c",
                    if stop {
                        "exec sleep 20"
                    } else {
                        "sleep 0.2; exit 7"
                    },
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            monitor(
                &shared,
                child,
                "test-instance",
                auth::offline_session(&account),
            );
            assert!(shared.lock().unwrap().running);
            assert!(!tick(&shared).window.unwrap().visible);
            if stop {
                action(
                    &shared,
                    ActionEvent {
                        action: "stop".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
            while shared.lock().unwrap().running && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let mut s = shared.lock().unwrap();
            assert!(!s.running);
            assert!(s.running_profile.is_empty());
            assert_eq!(s.running_since, 0);
            assert!(s.child.lock().unwrap().is_none());
            assert!(s.status.contains("exited"));
            s.full_dirty = true;
            drop(s);
            assert_eq!(
                tick(&shared).window,
                Some(fastchapp_desktop::WindowUpdate {
                    visible: true,
                    focus: true
                })
            );
        }
    }
    #[test]
    fn rejected_launch_keeps_launcher_visible() {
        let (_dir, shared) = state();
        let update = handle(
            &shared,
            ActionEvent {
                action: "play".into(),
                ..Default::default()
            },
        );
        assert!(update.window.is_none());
        assert!(tick(&shared).window.is_none());
        assert!(!shared.lock().unwrap().running);
    }
    #[test]
    fn folder_actions_resolve_actual_instance_and_loader_version() {
        let (dir, shared) = state();
        let mut s = shared.lock().unwrap();
        let p = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Test".into(),
            version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        };
        s.config.selected = Some(p.id.clone());
        s.config.profiles.push(p.clone());
        assert_eq!(
            folder_path(&s.config, "", false).unwrap(),
            dir.path().join("games/instances").join(&p.id)
        );
        assert_eq!(
            folder_path(&s.config, &p.id, true).unwrap(),
            dir.path().join("games/versions/1.21.1-fabric-0.19.5")
        );
        assert!(folder_path(&s.config, "../../../arbitrary", true).is_err());
        s.config.profiles[0].loader = "vanilla".into();
        assert_eq!(
            folder_path(&s.config, &p.id, true).unwrap(),
            dir.path().join("games/versions/1.21.1")
        );
        s.config.profiles[0].version = "../../../arbitrary".into();
        assert!(folder_path(&s.config, &p.id, true).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn folder_actions_reject_symlinked_managed_directories() {
        let (dir, shared) = state();
        let mut s = shared.lock().unwrap();
        let p = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Test".into(),
            version: "1.21.1".into(),
            loader: "vanilla".into(),
            loader_version: String::new(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        };
        s.config.profiles.push(p.clone());
        std::fs::create_dir_all(&s.config.settings.game_root).unwrap();
        std::os::unix::fs::symlink(dir.path(), s.config.settings.game_root.join("versions"))
            .unwrap();
        assert!(folder_path(&s.config, &p.id, true).is_err());
    }
}
pub fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let test_root = args
        .iter()
        .position(|a| a == "--test-root")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    if test_root.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err("--test-root must be absolute".into());
    }
    let config = test_root
        .clone()
        .unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Blockhaven")
        })
        .join("config.json");
    let mut state = State::load(config, test_root.is_some())?;
    if let Some(id) = &state.config.selected_account {
        if let Some(a) = state.config.offline_accounts.iter().find(|a| &a.uuid == id) {
            state.session = Some(auth::offline_session(a));
        }
    }
    artwork::load_saved(&mut state);
    let shared = Arc::new(Mutex::new(state));
    if args.iter().any(|s| s == "--network-check") {
        let net = net::Net::new()?;
        let manifest: Manifest = net.get(MANIFEST_URL)?;
        let p = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Check".into(),
            version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java: String::new(),
            memory_mb: 4096,
            mods: Vec::new(),
        };
        let hits = mods::search(&net, &p, "sodium", 0)?;
        let versions =
            mods::versions(&net, &p, &hits.hits.first().ok_or("No results")?.project_id)?;
        println!(
            "{}",
            serde_json::json!({"latest":manifest.latest.release,"versions":manifest.versions.len(),"modrinth_hits":hits.total_hits,"compatible_versions":versions.len()})
        );
        return Ok(());
    }
    let view = shared.clone();
    let handler = shared.clone();
    let ticker = shared.clone();
    let app = DesktopApp {
        assets: ASSETS,
        resource_root: RESOURCE_ROOT,
        title: "Blockhaven — Minecraft Launcher".into(),
        icon: Some((
            include_bytes!("../resources/icons/window-icon.rgba"),
            64,
            64,
        )),
        view: Box::new(move || Ok(ui::document(&view.lock().unwrap()))),
        handler: Box::new(move |event| Ok(handle(&handler, event))),
        tick: Box::new(move || tick(&ticker)),
    };
    if args.iter().any(|s| s == "--render") {
        print!("{}", app.render()?);
        return Ok(());
    }
    if args.iter().any(|s| s == "--stdio") {
        for line in std::io::stdin().lock().lines() {
            let line = line?;
            let result = match ActionEvent::parse(&line) {
                Ok(event) => app.dispatch(event),
                Err(error) => Err(error),
            };
            match result {
                Ok(update) => println!("{}", serde_json::to_string(&update)?),
                Err(error) => println!("{}", serde_json::json!({"error":error})),
            }
            std::io::stdout().flush()?;
        }
        return Ok(());
    }
    #[cfg(feature = "native")]
    {
        let _presence = presence::start(&shared);
        app.run()?;
    }
    #[cfg(not(feature="native"))]return Err("Build with the native feature for the desktop window; --render/--stdio are available for checks".into());
    #[cfg(feature = "native")]
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event(action: &str, fields: serde_json::Value) -> ActionEvent {
        ActionEvent::parse(&serde_json::json!({"action":action,"fields":fields}).to_string())
            .unwrap()
    }
    #[test]
    fn offline_profile_persists_and_ui_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let shared = Arc::new(Mutex::new(
            State::load(dir.path().join("config.json"), true).unwrap(),
        ));
        let r = handle(
            &shared,
            event("add_offline", serde_json::json!({"name":["Steve"]})),
        );
        assert!(!r.patches.is_empty());
        assert!(shared.lock().unwrap().session.as_ref().unwrap().offline);
        let cfg = State::load(dir.path().join("config.json"), true).unwrap();
        assert_eq!(cfg.config.offline_accounts[0].name, "Steve");
        assert_eq!(
            cfg.config.selected_account.as_deref(),
            Some(cfg.config.offline_accounts[0].uuid.as_str())
        );
        assert!(!serde_json::to_string(&cfg.config)
            .unwrap()
            .contains("refresh_token"));
        let mut s = shared.lock().unwrap();
        s.config.profiles.push(Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "<script>bad</script>".into(),
            version: "1.21.1".into(),
            loader: "vanilla".into(),
            loader_version: String::new(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        });
        s.config.selected = Some(s.config.profiles[0].id.clone());
        let html = ui::document(&s);
        assert!(!html.contains("<script>bad"));
        assert!(html.contains("&lt;script&gt;"));
    }
}

#[cfg(test)]
mod interface_tests {
    use super::*;
    #[test]
    fn manual_mods_refresh_render_and_dispatch_with_running_guard() {
        let root = tempfile::tempdir().unwrap();
        let mut state = State::load(root.path().join("config.json"), true).unwrap();
        let p = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Test".into(),
            version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        };
        let dir = state
            .config
            .settings
            .game_root
            .join("instances")
            .join(&p.id)
            .join("mods");
        state.config.selected = Some(p.id.clone());
        state.config.profiles.push(p.clone());
        state.page = "installed".into();
        let shared = Arc::new(Mutex::new(state));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("A & B.jar"), b"jar").unwrap();
        action(
            &shared,
            ActionEvent {
                action: "refresh_installed".into(),
                ..Default::default()
            },
        )
        .unwrap();
        {
            let mut s = shared.lock().unwrap();
            assert_eq!(s.mod_counts(&p), (1, 1));
            let html = ui::document(&s);
            assert!(html.contains("toggle_manual_mod"));
            assert!(html.contains("A &amp; B"));
            assert!(html.contains("Manual"));
            s.running = true;
        }
        let toggle = || ActionEvent {
            action: "toggle_manual_mod".into(),
            value: "A & B.jar".into(),
            ..Default::default()
        };
        assert!(action(&shared, toggle()).is_err());
        assert!(dir.join("A & B.jar").exists());
        shared.lock().unwrap().running = false;
        action(&shared, toggle()).unwrap();
        assert!(dir.join("A & B.jar.disabled").exists());
        assert_eq!(shared.lock().unwrap().mod_counts(&p), (1, 0));
        std::fs::write(dir.join("new.jar"), b"new").unwrap();
        shared.lock().unwrap().manual_mods_checked =
            std::time::Instant::now() - std::time::Duration::from_secs(3);
        let update = tick(&shared);
        assert!(update
            .patches
            .iter()
            .any(|patch| patch["selector"] == "body"));
        assert_eq!(shared.lock().unwrap().mod_counts(&p), (2, 1));
    }
    #[test]
    fn artwork_patches_preserve_form_state() {
        let root = tempfile::tempdir().unwrap();
        let shared = Arc::new(Mutex::new(
            State::load(root.path().join("config.json"), true).unwrap(),
        ));
        {
            let mut s = shared.lock().unwrap();
            s.page = "modrinth".into();
            s.icons
                .insert("AANobbMI".into(), "data:image/png;base64,eA==".into());
            s.icons_dirty = true;
        }
        let update = tick(&shared);
        assert!(
            update
                .patches
                .iter()
                .any(|p| p["op"] == "html"
                    && p["selector"].as_str().unwrap().starts_with(".mod-art-"))
        );
        assert!(!update
            .patches
            .iter()
            .any(|p| p["selector"] == "body" || p["selector"] == "#view"));
    }
    #[test]
    fn mod_artwork_and_english_pages_render() {
        let root = tempfile::tempdir().unwrap();
        let mut s = State::load(root.path().join("config.json"), true).unwrap();
        let profile = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Creative & cozy".into(),
            version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        };
        s.config.selected = Some(profile.id.clone());
        s.config.profiles.push(profile);
        let data: mods::Search =
            serde_json::from_str(include_str!("../tests/fixtures/modrinth-search.json")).unwrap();
        s.hits = data.hits;
        s.mod_total = data.total_hits;
        s.icons.insert(
            s.hits[0].project_id.clone(),
            "data:image/png;base64,eA==".into(),
        );
        for page in [
            "home",
            "profiles",
            "modrinth",
            "installed",
            "accounts",
            "settings",
            "logs",
        ] {
            s.page = page.into();
            let html = ui::document(&s);
            assert!(html.contains("lang=\"en\""));
            assert!(html.contains("data-action=\"window_minimize\""));
            assert!(html.contains("data-window-drag"));
            assert!(
                !html.contains("Játék")
                    && !html.contains("Beállítások")
                    && !html.contains("Megszakítás")
            );
            if page == "modrinth" {
                assert!(html.contains("data:image/png;base64,"));
                assert!(html.contains(" icon\""));
            }
        }
    }
}
