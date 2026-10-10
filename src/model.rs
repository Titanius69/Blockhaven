use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
pub type Result<T> = std::result::Result<T, String>;
pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
#[derive(Clone, Deserialize, Serialize)]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<Version>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Settings {
    #[serde(default)]
    pub discord: DiscordSettings,
    pub game_root: PathBuf,
    pub java: String,
    pub memory_mb: u32,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub workers: usize,
    pub snapshots: bool,
    pub client_id: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            discord: DiscordSettings::default(),
            game_root: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Blockhaven/games"),
            java: "java".into(),
            memory_mb: 4096,
            width: 1280,
            height: 720,
            fullscreen: false,
            workers: 4,
            snapshots: false,
            client_id: String::new(),
        }
    }
}
pub const DEFAULT_DISCORD_APPLICATION_ID: &str = "1558468312172732539";
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DiscordSettings {
    pub enabled: bool,
    pub application_id: String,
    pub image_key: String,
}
impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            application_id: DEFAULT_DISCORD_APPLICATION_ID.into(),
            image_key: String::new(),
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub struct ModRecord {
    #[serde(default)]
    pub icon_url: Option<String>,
    pub project: String,
    pub version: String,
    pub title: String,
    pub filename: String,
    pub hash: String,
    pub enabled: bool,
    #[serde(default)]
    pub required: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub version: String,
    pub loader: String,
    pub loader_version: String,
    pub java: String,
    pub memory_mb: u32,
    #[serde(default)]
    pub mods: Vec<ModRecord>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct OfflineAccount {
    pub name: String,
    pub uuid: String,
}
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Config {
    pub settings: Settings,
    pub profiles: Vec<Profile>,
    pub selected: Option<String>,
    #[serde(default)]
    pub offline_accounts: Vec<OfflineAccount>,
    #[serde(default)]
    pub selected_account: Option<String>,
}
impl Config {
    pub fn profile(&self) -> Result<&Profile> {
        let id = self
            .selected
            .as_deref()
            .ok_or("Create or select a profile first")?;
        self.profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| "Profile not found".into())
    }
    pub fn profile_mut(&mut self) -> Result<&mut Profile> {
        let id = self.selected.as_deref().ok_or("Select a profile first")?;
        self.profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "Profile not found".into())
    }
}
#[derive(Default)]
pub struct Progress {
    pub cancel: AtomicBool,
    pub detail: Mutex<(usize, usize, String)>,
}
impl Progress {
    pub fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("Operation cancelled".into())
        } else {
            Ok(())
        }
    }
    pub fn set(&self, done: usize, total: usize, label: String) {
        *self.detail.lock().unwrap() = (done, total, label)
    }
    pub fn percent(&self) -> usize {
        let v = self.detail.lock().unwrap();
        v.0.saturating_mul(100).checked_div(v.1).unwrap_or(0)
    }
}
#[derive(Clone)]
pub struct Session {
    pub name: String,
    pub uuid: String,
    pub xuid: String,
    pub access: String,
    pub refresh: String,
    pub expires: u64,
    pub offline: bool,
}
impl Session {
    pub fn valid(&self) -> bool {
        self.expires > now() + 60
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub struct State {
    pub config: Config,
    pub config_path: PathBuf,
    pub manifest: Manifest,
    pub page: String,
    pub busy: bool,
    pub progress: Arc<Progress>,
    pub full_dirty: bool,
    pub icons: std::collections::BTreeMap<String, String>,
    pub icons_inflight: std::collections::BTreeSet<String>,
    pub icons_dirty: bool,
    pub status: String,
    pub error: String,
    pub logs: Vec<String>,
    pub online: bool,
    pub session: Option<Session>,
    pub device_code: String,
    pub device_url: String,
    pub hits: Vec<crate::mods::Hit>,
    pub mod_versions: Vec<crate::mods::ModVersion>,
    pub mod_title: String,
    pub mod_project: String,
    pub query: String,
    pub mod_offset: usize,
    pub mod_total: usize,
    pub running: bool,
    pub child: Arc<Mutex<Option<std::process::Child>>>,
    pub running_profile: String,
    pub running_since: u64,
    pub pending_visibility: Option<bool>,
    pub discord_status: String,
    pub manual_mods: Vec<crate::mods::ManualMod>,
    pub manual_mods_profile: String,
    pub manual_mods_error: String,
    pub manual_mods_checked: std::time::Instant,
}
impl State {
    pub fn load(config_path: PathBuf, test: bool) -> Result<Self> {
        let mut config: Config = if config_path.exists() {
            serde_json::from_slice(&std::fs::read(&config_path).map_err(|e|e.to_string())?).map_err(|_|"Configuration is invalid; move config.json aside to recover without deleting games")?
        } else {
            Config::default()
        };
        if config.settings.discord.application_id.trim().is_empty() {
            config.settings.discord.application_id = DEFAULT_DISCORD_APPLICATION_ID.into();
        }
        if test {
            config.settings.game_root = config_path.parent().unwrap().join("games");
        }
        validate_config(&config)?;
        let manifest: Manifest =
            serde_json::from_str(include_str!("../tests/fixtures/manifest.json"))
                .map_err(|e| e.to_string())?;
        let mut state = Self {
            config,
            config_path,
            manifest,
            page: "home".into(),
            busy: false,
            progress: Arc::new(Progress::default()),
            full_dirty: false,
            icons: Default::default(),
            icons_inflight: Default::default(),
            icons_dirty: false,
            status: "Ready".into(),
            error: String::new(),
            logs: vec![
                "Blockhaven ready. Create a profile, choose Java, and sign in to play.".into(),
            ],
            online: false,
            session: None,
            device_code: String::new(),
            device_url: String::new(),
            hits: Vec::new(),
            mod_versions: Vec::new(),
            mod_title: String::new(),
            mod_project: String::new(),
            query: String::new(),
            mod_offset: 0,
            mod_total: 0,
            running: false,
            child: Arc::new(Mutex::new(None)),
            running_profile: String::new(),
            running_since: 0,
            pending_visibility: None,
            discord_status: "Not connected".into(),
            manual_mods: Vec::new(),
            manual_mods_profile: String::new(),
            manual_mods_error: String::new(),
            manual_mods_checked: std::time::Instant::now(),
        };
        state.refresh_manual_mods();
        Ok(state)
    }
    pub fn refresh_manual_mods(&mut self) -> bool {
        let (id, mods, error) = if let Ok(p) = self.config.profile() {
            match crate::mods::manual_mods(p, &self.config.settings) {
                Ok(mods) => (p.id.clone(), mods, String::new()),
                Err(e) => (p.id.clone(), Vec::new(), e),
            }
        } else {
            (String::new(), Vec::new(), String::new())
        };
        let changed = id != self.manual_mods_profile
            || mods != self.manual_mods
            || error != self.manual_mods_error;
        self.manual_mods_profile = id;
        self.manual_mods = mods;
        self.manual_mods_error = error;
        self.manual_mods_checked = std::time::Instant::now();
        changed
    }
    pub fn mod_counts(&self, p: &Profile) -> (usize, usize) {
        let manual = if self.manual_mods_profile == p.id {
            self.manual_mods.as_slice()
        } else {
            &[]
        };
        (
            p.mods.len() + manual.len(),
            p.mods.iter().filter(|m| m.enabled).count()
                + manual.iter().filter(|m| m.enabled).count(),
        )
    }
    pub fn idle(&self) -> Result<()> {
        if self.busy {
            Err("Wait for the current operation or cancel it first".into())
        } else if self.running {
            Err("Stop Minecraft before changing profiles or mods".into())
        } else {
            Ok(())
        }
    }
    pub fn log(&mut self, text: impl Into<String>) {
        let mut text = text.into();
        if let Some(session) = &self.session {
            for secret in [&session.access, &session.refresh] {
                if !session.offline && secret.len() >= 16 {
                    text = text.replace(secret, "[redacted]")
                }
            }
        }
        if text.len() > 3000 {
            text.truncate(
                text.char_indices()
                    .take_while(|(i, _)| *i < 3000)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0),
            );
        }
        self.logs.push(text);
        if self.logs.len() > 250 {
            self.logs.remove(0);
        }
    }
    pub fn installed(&self, p: &Profile) -> bool {
        crate::minecraft::receipt_path(&self.config.settings.game_root, p).is_file()
    }
}
pub type Shared = Arc<Mutex<State>>;
pub fn validate_config(config: &Config) -> Result<()> {
    validate_settings(&config.settings)?;
    let mut ids = std::collections::BTreeSet::new();
    for p in &config.profiles {
        if uuid::Uuid::parse_str(&p.id).is_err() || !ids.insert(&p.id) {
            return Err("Invalid/duplicate profile id".into());
        }
        crate::net::safe_component(&p.version)?;
        if !["vanilla", "fabric"].contains(&p.loader.as_str()) {
            return Err("Unsupported loader".into());
        }
        if !p.loader_version.is_empty() {
            crate::net::safe_component(&p.loader_version)?
        }
        if p.name.trim().is_empty() || p.name.len() > 80 {
            return Err("Invalid profile name".into());
        }
        if !(1024..=32768).contains(&p.memory_mb) {
            return Err("Memory must be 1024–32768 MiB".into());
        }
        for m in &p.mods {
            crate::net::safe_component(&m.filename)?;
            if !m.filename.ends_with(".jar") {
                return Err("Managed mods must be .jar files".into());
            }
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for a in &config.offline_accounts {
        let expected = crate::auth::offline(&a.name)?;
        if expected.uuid != a.uuid || !names.insert(a.uuid.clone()) {
            return Err("Invalid/duplicate offline account".into());
        }
    }
    Ok(())
}
pub fn validate_settings(s: &Settings) -> Result<()> {
    if !s.discord.application_id.is_empty()
        && (!(17..=20).contains(&s.discord.application_id.len())
            || !s.discord.application_id.bytes().all(|c| c.is_ascii_digit())
            || s.discord.application_id.parse::<u64>().unwrap_or(0) == 0)
    {
        return Err("Discord Application ID must be a 17–20 digit number".into());
    }
    if s.discord.image_key.len() > 64
        || !s
            .discord
            .image_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err("Discord image key must use letters, numbers, underscores or hyphens (up to 64 characters)".into());
    }
    if !(1024..=32768).contains(&s.memory_mb)
        || !(640..=7680).contains(&s.width)
        || !(480..=4320).contains(&s.height)
        || !(1..=16).contains(&s.workers)
    {
        return Err(
            "Settings out of range (RAM 1–32 GiB, resolution 640×480–7680×4320, downloads 1–16)"
                .into(),
        );
    }
    if s.java.trim().is_empty() || s.java.contains('\0') {
        return Err("Enter a Java executable".into());
    }
    if !s.client_id.is_empty() && uuid::Uuid::parse_str(&s.client_id).is_err() {
        return Err("Microsoft client ID must be a UUID".into());
    }
    if !s.game_root.is_absolute() {
        return Err("Game storage must be an absolute path".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_never_serializes_tokens_and_bounds_settings() {
        let mut c = Config::default();
        assert!(validate_config(&c).is_ok());
        c.settings.memory_mb = 1;
        assert!(validate_config(&c).is_err());
        let s = serde_json::to_string(&Config::default()).unwrap();
        assert!(!s.contains("access_token"));
    }
}
