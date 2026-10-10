use crate::{
    model::{ModRecord, Profile, Progress, Result, Settings},
    net::{safe_component, safe_join, Download, Net},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualMod {
    pub filename: String,
    pub enabled: bool,
}
pub fn manual_mods(p: &Profile, settings: &Settings) -> Result<Vec<ManualMod>> {
    let root = safe_join(&settings.game_root, &format!("instances/{}/mods", p.id))?;
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Could not read manual mods: {e}")),
    };
    let mut mods = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let filename = entry.file_name();
        let Some(filename) = filename.to_str() else {
            continue;
        };
        let lower = filename.to_ascii_lowercase();
        let enabled = !lower.ends_with(".jar.disabled");
        if (enabled && !lower.ends_with(".jar")) || safe_component(filename).is_err() {
            continue;
        }
        let managed = p.mods.iter().any(|m| {
            let disabled = format!("{}.disabled", m.filename);
            if cfg!(windows) {
                filename.eq_ignore_ascii_case(&m.filename)
                    || filename.eq_ignore_ascii_case(&disabled)
            } else {
                filename == m.filename || filename == disabled
            }
        });
        if !managed {
            mods.push(ManualMod {
                filename: filename.into(),
                enabled,
            });
        }
    }
    mods.sort_by(|a, b| {
        a.filename
            .to_lowercase()
            .cmp(&b.filename.to_lowercase())
            .then_with(|| a.filename.cmp(&b.filename))
    });
    Ok(mods)
}
pub fn toggle_manual_mod(p: &Profile, settings: &Settings, filename: &str) -> Result<()> {
    safe_component(filename)?;
    let m = manual_mods(p, settings)?
        .into_iter()
        .find(|m| m.filename == filename)
        .ok_or("Manual mod file changed or is no longer present; refresh the list")?;
    let destination = if m.enabled {
        format!("{filename}.disabled")
    } else {
        filename[..filename.len() - ".disabled".len()].into()
    };
    let root = safe_join(&settings.game_root, &format!("instances/{}/mods", p.id))?;
    let from = safe_join(&root, filename)?;
    let to = safe_join(&root, &destination)?;
    if to.exists() {
        return Err("A file with the destination name already exists; nothing was changed".into());
    }
    fs::rename(from, to).map_err(|e| format!("Could not toggle manual mod: {e}"))
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Hit {
    #[serde(default)]
    pub icon_url: Option<String>,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    #[serde(default)]
    pub author: String,
}
#[derive(Deserialize)]
pub struct Project {
    pub title: String,
    pub icon_url: Option<String>,
}
pub fn project(net: &Net, id: &str) -> Result<Project> {
    safe_component(id)?;
    net.get(&format!("https://api.modrinth.com/v2/project/{id}"))
}
#[derive(Deserialize)]
pub struct Search {
    pub hits: Vec<Hit>,
    pub total_hits: usize,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct ModFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub hashes: BTreeMap<String, String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Dependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub dependency_type: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct ModVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}
pub fn search(net: &Net, p: &Profile, query: &str, offset: usize) -> Result<Search> {
    fabric(p)?;
    let facets = serde_json::json!([
        ["project_type:mod"],
        [format!("versions:{}", p.version)],
        ["categories:fabric"]
    ])
    .to_string();
    let mut url = reqwest::Url::parse("https://api.modrinth.com/v2/search").unwrap();
    url.query_pairs_mut()
        .append_pair("query", query)
        .append_pair("facets", &facets)
        .append_pair("limit", "12")
        .append_pair("offset", &offset.to_string())
        .append_pair("index", "relevance");
    net.get(url.as_str())
}
fn fabric(p: &Profile) -> Result<()> {
    if p.loader != "fabric" {
        Err("Modrinth mods need a Fabric profile. Create or select one first.".into())
    } else {
        Ok(())
    }
}
pub fn versions(net: &Net, p: &Profile, id: &str) -> Result<Vec<ModVersion>> {
    fabric(p)?;
    safe_component(id)?;
    let mut url = reqwest::Url::parse(&format!("https://api.modrinth.com/v2/project/{id}/version"))
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("loaders", "[\"fabric\"]")
        .append_pair("game_versions", &serde_json::json!([p.version]).to_string());
    net.get(url.as_str())
}
fn compatible(v: &ModVersion, p: &Profile) -> Result<()> {
    if !v.game_versions.contains(&p.version) || !v.loaders.iter().any(|s| s == "fabric") {
        Err("Dependency does not support this Minecraft version and Fabric".into())
    } else {
        Ok(())
    }
}
fn latest(items: Vec<ModVersion>) -> Result<ModVersion> {
    items
        .iter()
        .find(|v| v.version_type == "release")
        .cloned()
        .or_else(|| items.into_iter().next())
        .ok_or_else(|| "No compatible mod version found".into())
}
fn resolve(
    net: &Net,
    p: &Profile,
    v: ModVersion,
    plan: &mut BTreeMap<String, ModVersion>,
    visiting: &mut BTreeSet<String>,
    progress: &Arc<Progress>,
) -> Result<()> {
    progress.check()?;
    compatible(&v, p)?;
    if let Some(prior) = plan.get(&v.project_id) {
        return if prior.id == v.id {
            Ok(())
        } else {
            Err("Conflicting required dependency versions".into())
        };
    }
    if plan.len() + visiting.len() >= 64 {
        return Err("Dependency graph exceeds 64 projects".into());
    }
    if !visiting.insert(v.project_id.clone()) {
        return Err("Cyclic mod dependency".into());
    }
    for d in &v.dependencies {
        if d.dependency_type != "required" {
            continue;
        }
        let dep = if let Some(id) = &d.version_id {
            safe_component(id)?;
            net.get::<ModVersion>(&format!("https://api.modrinth.com/v2/version/{id}"))?
        } else if let Some(id) = &d.project_id {
            if let Some(existing) = plan.get(id) {
                existing.clone()
            } else {
                latest(versions(net, p, id)?)?
            }
        } else {
            return Err("Required dependency has no project or version ID".into());
        };
        resolve(net, p, dep, plan, visiting, progress)?;
    }
    visiting.remove(&v.project_id);
    plan.insert(v.project_id.clone(), v);
    Ok(())
}
pub fn install(
    net: &Net,
    p: &Profile,
    s: &Settings,
    id: &str,
    title: &str,
    progress: &Arc<Progress>,
    persist: impl FnOnce(&Profile) -> Result<()>,
) -> Result<Profile> {
    fabric(p)?;
    safe_component(id)?;
    let root: ModVersion = net.get(&format!("https://api.modrinth.com/v2/version/{id}"))?;
    let root_project = root.project_id.clone();
    let mut plan = BTreeMap::new();
    resolve(net, p, root, &mut plan, &mut BTreeSet::new(), progress)?;
    for v in plan.values() {
        for dep in &v.dependencies {
            if dep.dependency_type == "incompatible" {
                if let Some(id) = &dep.project_id {
                    if plan.contains_key(id) || p.mods.iter().any(|m| &m.project == id && m.enabled)
                    {
                        return Err("Mod declares an incompatibility with an installed mod".into());
                    }
                }
                if let Some(id) = &dep.version_id {
                    if plan.values().any(|v| &v.id == id)
                        || p.mods.iter().any(|m| &m.version == id && m.enabled)
                    {
                        return Err("Incompatible mod version is installed".into());
                    }
                }
            }
        }
    }
    // An update must preserve exact version requirements of already installed dependants.
    for m in &p.mods {
        if !plan.contains_key(&m.project) {
            for required in &m.required {
                if let Some((project, version)) = required.split_once('@') {
                    if plan.get(project).is_some_and(|v| v.id != version) {
                        return Err(format!(
                            "{} requires a different dependency version",
                            m.title
                        ));
                    }
                }
            }
        }
    }
    let mods = safe_join(&s.game_root, &format!("instances/{}/mods", p.id))?;
    fs::create_dir_all(&mods).map_err(|e| e.to_string())?;
    let stage = tempfile::tempdir_in(&mods).map_err(|e| e.to_string())?;
    let mut records = Vec::new();
    let mut names = BTreeSet::new();
    for (i, v) in plan.values().enumerate() {
        let file = v
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| v.files.first())
            .ok_or("Mod version has no file")?;
        safe_component(&file.filename)?;
        if !file.filename.ends_with(".jar") || !names.insert(file.filename.clone()) {
            return Err("Invalid or conflicting mod filename".into());
        }
        let dest = safe_join(&mods, &file.filename)?;
        let disabled = safe_join(&mods, &format!("{}.disabled", file.filename))?;
        if (dest.exists() || disabled.exists())
            && !p
                .mods
                .iter()
                .any(|m| m.filename == file.filename && m.project == v.project_id)
        {
            return Err("A manually installed file already uses this name".into());
        }
        progress.set(i, plan.len(), format!("Downloading {}", v.name));
        let hash = file
            .hashes
            .get("sha512")
            .ok_or("Modrinth file has no SHA-512")?;
        net.download(
            &Download {
                url: file.url.clone(),
                path: file.filename.clone(),
                algorithm: "sha512".into(),
                hash: hash.clone(),
                size: Some(file.size),
            },
            stage.path(),
            progress,
        )?;
        let mut required = Vec::new();
        for d in &v.dependencies {
            if d.dependency_type == "required" {
                let found = if let Some(id) = &d.project_id {
                    plan.get(id)
                } else {
                    plan.values().find(|v| Some(&v.id) == d.version_id.as_ref())
                };
                let dep = found.ok_or("Unresolved required dependency")?;
                required.push(format!("{}@{}", dep.project_id, dep.id));
            }
        }
        let info = project(net, &v.project_id).ok();
        records.push(ModRecord {
            icon_url: info.as_ref().and_then(|i| i.icon_url.clone()),
            project: v.project_id.clone(),
            version: v.id.clone(),
            title: if let Some(info) = info {
                info.title
            } else if v.project_id == root_project {
                title.into()
            } else {
                v.name.clone()
            },
            filename: file.filename.clone(),
            hash: hash.clone(),
            enabled: true,
            required,
        });
    }
    progress.check()?;
    let backup = tempfile::tempdir_in(&mods).map_err(|e| e.to_string())?;
    let mut moved: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    let mut placed = Vec::new();
    let mut result = p.clone();
    result.mods.retain(|m| !plan.contains_key(&m.project));
    result.mods.extend(records.clone());
    let transaction = (|| -> Result<()> {
        for old in &p.mods {
            if plan.contains_key(&old.project) {
                let name = if old.enabled {
                    old.filename.clone()
                } else {
                    format!("{}.disabled", old.filename)
                };
                let from = safe_join(&mods, &name)?;
                if from.exists() {
                    let to = backup.path().join(&name);
                    fs::rename(&from, &to).map_err(|e| e.to_string())?;
                    moved.push((from, to));
                }
            }
        }
        for m in &records {
            let dest = safe_join(&mods, &m.filename)?;
            fs::rename(stage.path().join(&m.filename), &dest).map_err(|e| e.to_string())?;
            placed.push(dest);
        }
        persist(&result)?;
        Ok(())
    })();
    if let Err(e) = transaction {
        for path in placed {
            let _ = fs::remove_file(path);
        }
        for (from, to) in moved.into_iter().rev() {
            let _ = fs::rename(to, from);
        }
        return Err(e);
    }
    progress.set(1, 1, "Mods installed".into());
    Ok(result)
}
pub fn toggle(p: &mut Profile, s: &Settings, project: &str) -> Result<()> {
    let i = p
        .mods
        .iter()
        .position(|m| m.project == project)
        .ok_or("Managed mod not found")?;
    if p.mods[i].enabled {
        check_dependants(p, project)?;
    } else {
        for dep in &p.mods[i].required {
            let (project, version) = dep.split_once('@').ok_or("Invalid dependency record")?;
            if !p
                .mods
                .iter()
                .any(|m| m.project == project && m.version == version && m.enabled)
            {
                return Err("Enable required dependencies first".into());
            }
        }
    }
    let m = &p.mods[i];
    let root = safe_join(&s.game_root, &format!("instances/{}/mods", p.id))?;
    let active = safe_join(&root, &m.filename)?;
    let disabled = safe_join(&root, &format!("{}.disabled", m.filename))?;
    let (from, to) = if m.enabled {
        (active, disabled)
    } else {
        (disabled, active)
    };
    if to.exists() {
        return Err("Destination already exists".into());
    }
    fs::rename(from, to).map_err(|e| e.to_string())?;
    p.mods[i].enabled = !p.mods[i].enabled;
    Ok(())
}
fn check_dependants(p: &Profile, id: &str) -> Result<()> {
    if let Some(m) = p.mods.iter().find(|m| {
        m.enabled
            && m.required
                .iter()
                .any(|d| d.split_once('@').is_some_and(|(project, _)| project == id))
    }) {
        Err(format!("{} still requires this mod", m.title))
    } else {
        Ok(())
    }
}
pub fn remove(p: &mut Profile, s: &Settings, project: &str) -> Result<()> {
    check_dependants(p, project)?;
    let i = p
        .mods
        .iter()
        .position(|m| m.project == project)
        .ok_or("Mod not found")?;
    let m = &p.mods[i];
    let name = if m.enabled {
        m.filename.clone()
    } else {
        format!("{}.disabled", m.filename)
    };
    let from = safe_join(&s.game_root, &format!("instances/{}/mods/{name}", p.id))?;
    if from.exists() {
        let trash = safe_join(
            &s.game_root,
            &format!(
                "instances/{}/removed-mods/{}-{name}",
                p.id,
                uuid::Uuid::new_v4()
            ),
        )?;
        fs::create_dir_all(trash.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::rename(from, trash).map_err(|e| e.to_string())?;
    }
    p.mods.remove(i);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_search_deserializes() {
        let s: Search =
            serde_json::from_str(include_str!("../tests/fixtures/modrinth-search.json")).unwrap();
        assert!(!s.hits.is_empty());
        assert!(s.total_hits >= s.hits.len());
    }
    #[test]
    fn required_mod_cannot_be_disabled() {
        let mut p = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Test".into(),
            version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        };
        p.mods.push(ModRecord {
            icon_url: None,
            project: "a".into(),
            version: "v".into(),
            title: "Dependant".into(),
            filename: "a.jar".into(),
            hash: String::new(),
            enabled: true,
            required: vec!["b@v2".into()],
        });
        assert!(check_dependants(&p, "b").is_err());
    }
}
#[cfg(test)]
mod manual_tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Profile, Settings, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Manual test".into(),
            version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        };
        let settings = Settings {
            game_root: dir.path().into(),
            ..Settings::default()
        };
        let mods = dir.path().join("instances").join(&p.id).join("mods");
        (dir, p, settings, mods)
    }
    #[test]
    fn scan_and_toggle_real_manual_files_without_config_records() {
        let (_dir, p, s, root) = setup();
        assert!(manual_mods(&p, &s).unwrap().is_empty());
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Example mod.jar"), b"preserve these bytes").unwrap();
        fs::write(root.join("other.jar.disabled"), b"disabled").unwrap();
        fs::write(root.join("notes.txt"), b"ignore").unwrap();
        fs::create_dir(root.join("folder.jar")).unwrap();
        let listed = manual_mods(&p, &s).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].enabled);
        assert!(!listed[1].enabled);
        toggle_manual_mod(&p, &s, "Example mod.jar").unwrap();
        assert!(!root.join("Example mod.jar").exists());
        assert_eq!(
            fs::read(root.join("Example mod.jar.disabled")).unwrap(),
            b"preserve these bytes"
        );
        toggle_manual_mod(&p, &s, "Example mod.jar.disabled").unwrap();
        assert_eq!(
            fs::read(root.join("Example mod.jar")).unwrap(),
            b"preserve these bytes"
        );
        assert!(p.mods.is_empty());
    }
    #[test]
    fn manual_scan_excludes_managed_aliases_and_toggle_preserves_collisions() {
        let (_dir, mut p, s, root) = setup();
        fs::create_dir_all(&root).unwrap();
        p.mods.push(ModRecord {
            project: "managed".into(),
            version: "v1".into(),
            title: "Managed".into(),
            filename: "managed.jar".into(),
            hash: String::new(),
            enabled: true,
            required: vec![],
            icon_url: None,
        });
        for name in [
            "managed.jar",
            "managed.jar.disabled",
            "manual.jar",
            "manual.jar.disabled",
        ] {
            fs::write(root.join(name), name).unwrap();
        }
        assert_eq!(manual_mods(&p, &s).unwrap().len(), 2);
        assert!(toggle_manual_mod(&p, &s, "managed.jar").is_err());
        assert!(toggle_manual_mod(&p, &s, "manual.jar").is_err());
        assert!(toggle_manual_mod(&p, &s, "../manual.jar").is_err());
        assert_eq!(fs::read(root.join("manual.jar")).unwrap(), b"manual.jar");
        assert_eq!(
            fs::read(root.join("manual.jar.disabled")).unwrap(),
            b"manual.jar.disabled"
        );
        fs::remove_file(root.join("manual.jar")).unwrap();
        assert!(toggle_manual_mod(&p, &s, "manual.jar").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn manual_scan_and_toggle_ignore_symlinks() {
        let (dir, p, s, root) = setup();
        fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("outside.jar");
        fs::write(&outside, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked.jar")).unwrap();
        assert!(manual_mods(&p, &s).unwrap().is_empty());
        assert!(toggle_manual_mod(&p, &s, "linked.jar").is_err());
        assert_eq!(fs::read(outside).unwrap(), b"outside");
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    #[test]
    #[ignore = "downloads real Modrinth files into an isolated temporary folder"]
    fn live_dependency_install_and_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let net = Net::new().unwrap();
        let progress = Arc::new(Progress::default());
        let p = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: "QA only".into(),
            version: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java: String::new(),
            memory_mb: 4096,
            mods: vec![],
        };
        let s = Settings {
            game_root: dir.path().to_path_buf(),
            ..Settings::default()
        };
        let version = latest(versions(&net, &p, "sodium-extra").unwrap()).unwrap();
        let mut installed = install(&net, &p, &s, &version.id, "Sodium Extra", &progress, |_| {
            Ok(())
        })
        .unwrap();
        assert!(installed.mods.len() >= 2);
        let root = installed
            .mods
            .iter()
            .find(|m| m.project == version.project_id)
            .unwrap();
        assert!(!root.required.is_empty());
        let dependency = root.required[0].split('@').next().unwrap().to_owned();
        assert!(toggle(&mut installed, &s, &dependency).is_err());
        assert!(install(
            &net,
            &installed,
            &s,
            &version.id,
            "Sodium Extra",
            &progress,
            |_| Err("simulated config write failure".into())
        )
        .is_err());
        for m in &installed.mods {
            crate::net::verify(
                &s.game_root
                    .join(format!("instances/{}/mods/{}", p.id, m.filename)),
                "sha512",
                &m.hash,
                None,
            )
            .unwrap();
        }
        toggle(&mut installed, &s, &version.project_id).unwrap();
        assert!(
            !installed
                .mods
                .iter()
                .find(|m| m.project == version.project_id)
                .unwrap()
                .enabled
        );
        toggle(&mut installed, &s, &version.project_id).unwrap();
        remove(&mut installed, &s, &version.project_id).unwrap();
        assert!(!installed
            .mods
            .iter()
            .any(|m| m.project == version.project_id));
    }
}
