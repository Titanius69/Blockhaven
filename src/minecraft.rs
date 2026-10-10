use crate::{
    model::{Manifest, Profile, Progress, Result, Session, Settings, Version},
    net::{atomic_json, download_all, safe_component, safe_join, Download, Net},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Platform {
    pub os: String,
    pub arch: String,
    pub version: String,
}
impl Platform {
    pub fn current() -> Self {
        Self {
            os: if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "macos") {
                "osx"
            } else {
                "linux"
            }
            .into(),
            arch: std::env::consts::ARCH.into(),
            version: if cfg!(target_os = "windows") {
                "10.0"
            } else {
                ""
            }
            .into(),
        }
    }
}
pub fn allowed(
    rules: Option<&Value>,
    platform: &Platform,
    features: &BTreeMap<String, bool>,
) -> bool {
    let Some(rules) = rules.and_then(Value::as_array) else {
        return true;
    };
    let mut allow = false;
    for rule in rules {
        let mut matches = true;
        if let Some(os) = rule.get("os") {
            if let Some(name) = os.get("name").and_then(Value::as_str) {
                matches &= name == platform.os
            }
            for (key, value) in [("arch", &platform.arch), ("version", &platform.version)] {
                if let Some(pattern) = os.get(key).and_then(Value::as_str) {
                    matches &= regex::Regex::new(pattern).is_ok_and(|re| re.is_match(value));
                }
            }
        }
        if let Some(flags) = rule.get("features").and_then(Value::as_object) {
            for (key, value) in flags {
                matches &= value.as_bool() == Some(*features.get(key).unwrap_or(&false));
            }
        }
        if matches {
            allow = rule.get("action").and_then(Value::as_str) == Some("allow");
        }
    }
    allow
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub minecraft: String,
    pub loader: String,
    pub loader_version: String,
    pub java_major: u32,
    pub main_class: String,
    pub metadata: Value,
    pub classpath: Vec<String>,
    pub native_jars: Vec<String>,
    pub platform: Platform,
    pub asset_index: String,
}
pub fn install_key(profile: &Profile) -> String {
    if profile.loader == "fabric" {
        format!("{}-fabric-{}", profile.version, profile.loader_version)
    } else {
        profile.version.clone()
    }
}
pub fn receipt_path(root: &Path, profile: &Profile) -> PathBuf {
    let platform = Platform::current();
    root.join("versions")
        .join(install_key(profile))
        .join(format!("installed-{}-{}.json", platform.os, platform.arch))
}
pub fn maven_path(name: &str) -> Result<String> {
    let (name, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<_> = name.split(':').collect();
    if parts.len() < 3 || parts.len() > 4 {
        return Err("Invalid Maven coordinate".into());
    }
    for part in &parts {
        safe_component(part)?
    }
    safe_component(ext)?;
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    Ok(format!(
        "{}/{}/{}/{}-{}{classifier}.{ext}",
        parts[0].replace('.', "/"),
        parts[1],
        parts[2],
        parts[1],
        parts[2]
    ))
}
fn artifact(item: &Value, prefix: &str) -> Result<Download> {
    let path = item
        .get("path")
        .and_then(Value::as_str)
        .ok_or("Artifact path missing")?;
    Ok(Download {
        path: format!("{prefix}{path}"),
        url: item
            .get("url")
            .and_then(Value::as_str)
            .ok_or("Artifact URL missing")?
            .into(),
        algorithm: "sha1".into(),
        hash: item
            .get("sha1")
            .and_then(Value::as_str)
            .ok_or("Artifact hash missing")?
            .into(),
        size: item.get("size").and_then(Value::as_u64),
    })
}
pub fn plan_libraries(
    metadata: &Value,
    platform: &Platform,
) -> Result<(Vec<Download>, Vec<String>, Vec<String>)> {
    let mut items = Vec::new();
    let mut classpath = Vec::new();
    let mut natives = Vec::new();
    let flags = BTreeMap::new();
    for library in metadata
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or("Libraries missing")?
    {
        if !allowed(library.get("rules"), platform, &flags) {
            continue;
        }
        if let Some(item) = library.get("downloads").and_then(|v| v.get("artifact")) {
            let download = artifact(item, "libraries/")?;
            classpath.push(download.path.clone());
            items.push(download);
        } else if library.get("downloads").is_none() {
            let path = maven_path(
                library
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("Library name missing")?,
            )?;
            let base = library
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("https://libraries.minecraft.net/");
            let hash = library.get("sha1").and_then(Value::as_str).unwrap_or("");
            let download = Download {
                url: format!("{}{path}", base.trim_end_matches('/').to_owned() + "/"),
                path: format!("libraries/{path}"),
                hash: hash.into(),
                algorithm: "sha1".into(),
                size: library.get("size").and_then(Value::as_u64),
            };
            classpath.push(download.path.clone());
            items.push(download);
        }
        if let Some(classifier) = library
            .get("natives")
            .and_then(|v| v.get(&platform.os))
            .and_then(Value::as_str)
        {
            let arch = if platform.arch == "x86" { "32" } else { "64" };
            let classifier = classifier.replace("${arch}", arch);
            let item = library
                .get("downloads")
                .and_then(|v| v.get("classifiers"))
                .and_then(|v| v.get(&classifier))
                .ok_or("Native classifier missing for this platform")?;
            let download = artifact(item, "libraries/")?;
            natives.push(download.path.clone());
            items.push(download);
        }
        // Modern LWJGL metadata puts native-classifier artifacts directly on the classpath.
        if library
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.contains(":natives-"))
        {
            if let Some(path) = classpath.last() {
                natives.push(path.clone());
            }
        }
    }
    Ok((items, classpath, natives))
}
pub fn merge_fabric(mut vanilla: Value, fabric: &Value) -> Result<Value> {
    let extra = fabric
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or("Fabric libraries missing")?;
    let libraries = vanilla
        .get_mut("libraries")
        .and_then(Value::as_array_mut)
        .ok_or("Minecraft libraries missing")?;
    fn key(v: &Value) -> String {
        v.get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .split(':')
            .take(2)
            .collect::<Vec<_>>()
            .join(":")
    }
    for library in extra {
        let name = key(library);
        libraries.retain(|v| key(v) != name);
        libraries.push(library.clone());
    }
    vanilla["mainClass"] = fabric
        .get("mainClass")
        .cloned()
        .ok_or("Fabric main class missing")?;
    for kind in ["game", "jvm"] {
        if let Some(args) = fabric
            .get("arguments")
            .and_then(|a| a.get(kind))
            .and_then(Value::as_array)
        {
            if vanilla.get("arguments").is_none() {
                return Err("Fabric requires a modern Minecraft version".into());
            }
            if vanilla["arguments"].get(kind).is_none() {
                vanilla["arguments"][kind] = json!([])
            }
            vanilla["arguments"][kind]
                .as_array_mut()
                .ok_or("Invalid Minecraft arguments")?
                .extend(args.iter().cloned());
        }
    }
    Ok(vanilla)
}
pub fn fetch_metadata(
    net: &Net,
    entry: &Version,
    root: &Path,
    progress: &Arc<Progress>,
) -> Result<Value> {
    safe_component(&entry.id)?;
    let item = Download {
        url: entry.url.clone(),
        path: format!("versions/{}/{}.json", entry.id, entry.id),
        algorithm: "sha1".into(),
        hash: entry.sha1.clone(),
        size: None,
    };
    net.download(&item, root, progress)?;
    serde_json::from_slice(&fs::read(safe_join(root, &item.path)?).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub fn install(
    net: &Net,
    manifest: &Manifest,
    profile: &Profile,
    settings: &Settings,
    progress: &Arc<Progress>,
) -> Result<Receipt> {
    let root = &settings.game_root;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    safe_component(&profile.version)?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == profile.version)
        .ok_or("Minecraft version is not in the official manifest")?;
    progress.set(0, 1, "Fetching version metadata".into());
    let vanilla = fetch_metadata(net, entry, root, progress)?;
    let metadata = if profile.loader == "fabric" {
        safe_component(&profile.loader_version)?;
        let fabric: Value = net.get(&format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
            profile.version, profile.loader_version
        ))?;
        merge_fabric(vanilla.clone(), &fabric)?
    } else {
        vanilla.clone()
    };
    let platform = Platform::current();
    let (mut downloads, mut classpath, native_jars) = plan_libraries(&metadata, &platform)?;
    let client = vanilla
        .get("downloads")
        .and_then(|v| v.get("client"))
        .ok_or("This historical version is not supported by the client installer")?;
    let jar = format!("versions/{}/{}.jar", profile.version, profile.version);
    downloads.push(Download {
        url: client["url"].as_str().ok_or("Client URL missing")?.into(),
        path: jar.clone(),
        hash: client["sha1"]
            .as_str()
            .ok_or("Client SHA-1 missing")?
            .into(),
        algorithm: "sha1".into(),
        size: client["size"].as_u64(),
    });
    classpath.push(jar);
    for item in &mut downloads {
        if item.hash.is_empty() {
            let hash = net.text(&(item.url.clone() + ".sha1"))?;
            let hash = hash.split_whitespace().next().unwrap_or("");
            if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Invalid Fabric Maven SHA-1 sidecar".into());
            }
            item.hash = hash.to_owned();
        }
    }
    download_all(
        net,
        &downloads,
        root,
        progress,
        settings.workers,
        "Client and libraries",
    )?;
    let index = vanilla.get("assetIndex").ok_or("Asset index missing")?;
    let index_id = index["id"].as_str().ok_or("Asset index id missing")?;
    safe_component(index_id)?;
    let index_path = format!("assets/indexes/{index_id}.json");
    net.download(
        &Download {
            url: index["url"]
                .as_str()
                .ok_or("Asset index URL missing")?
                .into(),
            path: index_path.clone(),
            hash: index["sha1"]
                .as_str()
                .ok_or("Asset index hash missing")?
                .into(),
            algorithm: "sha1".into(),
            size: index["size"].as_u64(),
        },
        root,
        progress,
    )?;
    let assets: Value = serde_json::from_slice(
        &fs::read(safe_join(root, &index_path)?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let objects = assets["objects"]
        .as_object()
        .ok_or("Asset objects missing")?;
    let mut asset_downloads = Vec::new();
    for item in objects.values() {
        let hash = item["hash"].as_str().ok_or("Asset hash missing")?;
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid asset hash".into());
        }
        asset_downloads.push(Download {
            url: format!(
                "https://resources.download.minecraft.net/{}/{hash}",
                &hash[..2]
            ),
            path: format!("assets/objects/{}/{hash}", &hash[..2]),
            hash: hash.into(),
            algorithm: "sha1".into(),
            size: item["size"].as_u64(),
        });
    }
    asset_downloads.sort_by(|a, b| a.path.cmp(&b.path));
    asset_downloads.dedup_by(|a, b| a.path == b.path);
    download_all(
        net,
        &asset_downloads,
        root,
        progress,
        settings.workers,
        "Assets",
    )?;
    let instance = safe_join(root, &format!("instances/{}", profile.id))?;
    fs::create_dir_all(instance.join("mods")).map_err(|e| e.to_string())?;
    if assets["virtual"].as_bool() == Some(true)
        || assets["map_to_resources"].as_bool() == Some(true)
    {
        for (name, item) in objects {
            progress.check()?;
            let hash = item["hash"].as_str().unwrap();
            let source = safe_join(root, &format!("assets/objects/{}/{hash}", &hash[..2]))?;
            let target = if assets["map_to_resources"].as_bool() == Some(true) {
                safe_join(&instance.join("resources"), name)?
            } else {
                safe_join(root, &format!("assets/virtual/{index_id}/{name}"))?
            };
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            if !target.exists() {
                fs::copy(source, target).map_err(|e| e.to_string())?;
            }
        }
    }
    let key = install_key(profile);
    safe_component(&key)?;
    let native_root = safe_join(
        root,
        &format!("versions/{key}/natives-{}-{}", platform.os, platform.arch),
    )?;
    fs::create_dir_all(&native_root).map_err(|e| e.to_string())?;
    for jar in &native_jars {
        extract_natives(&safe_join(root, jar)?, &native_root)?;
    }
    let receipt = Receipt {
        minecraft: profile.version.clone(),
        loader: profile.loader.clone(),
        loader_version: profile.loader_version.clone(),
        java_major: vanilla
            .get("javaVersion")
            .and_then(|v| v.get("majorVersion"))
            .and_then(Value::as_u64)
            .unwrap_or(8) as u32,
        main_class: metadata["mainClass"]
            .as_str()
            .ok_or("Main class missing")?
            .into(),
        metadata,
        classpath,
        native_jars,
        platform,
        asset_index: index_id.into(),
    };
    atomic_json(&receipt_path(root, profile), &receipt)?;
    progress.set(1, 1, "Installation complete".into());
    Ok(receipt)
}
pub fn extract_natives(archive: &Path, root: &Path) -> Result<()> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut total = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.starts_with("META-INF/") || entry.is_dir() {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if ![".dll", ".so", ".dylib", ".jnilib"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Symlink native rejected".into());
        }
        let target = safe_join(root, &name)?;
        total += entry.size();
        if total > 256 * 1024 * 1024 {
            return Err("Native archive is too large".into());
        }
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn expand(text: &str, variables: &BTreeMap<String, String>) -> Result<String> {
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut output = String::new();
    let mut last = 0;
    for capture in re.captures_iter(text) {
        let m = capture.get(0).unwrap();
        output.push_str(&text[last..m.start()]);
        output.push_str(
            variables
                .get(&capture[1])
                .ok_or_else(|| format!("Unsupported launch placeholder: {}", &capture[1]))?,
        );
        last = m.end();
    }
    output.push_str(&text[last..]);
    Ok(output)
}
pub fn launch_arguments(
    receipt: &Receipt,
    profile: &Profile,
    settings: &Settings,
    session: &Session,
) -> Result<Vec<String>> {
    let root = &settings.game_root;
    let game = safe_join(root, &format!("instances/{}", profile.id))?;
    let natives = safe_join(
        root,
        &format!(
            "versions/{}/natives-{}-{}",
            install_key(profile),
            receipt.platform.os,
            receipt.platform.arch
        ),
    )?;
    let separator = if receipt.platform.os == "windows" {
        ";"
    } else {
        ":"
    };
    let classpath = receipt
        .classpath
        .iter()
        .map(|p| safe_join(root, p).map(|p| p.to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>>>()?
        .join(separator);
    let game_assets = root.join("assets/virtual").join(&receipt.asset_index);
    let vars: BTreeMap<String, String> = [
        ("auth_player_name", session.name.clone()),
        ("auth_uuid", session.uuid.clone()),
        ("auth_access_token", session.access.clone()),
        (
            "auth_session",
            format!("token:{}:{}", session.access, session.uuid),
        ),
        ("auth_xuid", session.xuid.clone()),
        ("clientid", settings.client_id.clone()),
        ("version_name", install_key(profile)),
        (
            "version_type",
            receipt.metadata["type"]
                .as_str()
                .unwrap_or("release")
                .into(),
        ),
        ("game_directory", game.to_string_lossy().into()),
        ("assets_root", root.join("assets").to_string_lossy().into()),
        ("game_assets", game_assets.to_string_lossy().into()),
        ("assets_index_name", receipt.asset_index.clone()),
        (
            "user_type",
            if session.offline {
                "legacy".into()
            } else if profile.version.starts_with("1.")
                && profile
                    .version
                    .split('.')
                    .nth(1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .is_some_and(|n| n < 17)
            {
                "mojang".into()
            } else {
                "msa".into()
            },
        ),
        ("user_properties", "{}".into()),
        ("natives_directory", natives.to_string_lossy().into()),
        ("launcher_name", "Blockhaven".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("classpath", classpath.clone()),
        ("classpath_separator", separator.into()),
        (
            "library_directory",
            root.join("libraries").to_string_lossy().into(),
        ),
        ("resolution_width", settings.width.to_string()),
        ("resolution_height", settings.height.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect();
    let features = BTreeMap::from([
        ("has_custom_resolution".into(), !settings.fullscreen),
        ("is_demo_user".into(), false),
    ]);
    fn values(
        args: &Value,
        platform: &Platform,
        features: &BTreeMap<String, bool>,
        vars: &BTreeMap<String, String>,
    ) -> Result<Vec<String>> {
        let mut output = Vec::new();
        for arg in args.as_array().ok_or("Invalid argument list")? {
            if let Some(text) = arg.as_str() {
                output.push(expand(text, vars)?)
            } else if allowed(arg.get("rules"), platform, features) {
                let value = &arg["value"];
                if let Some(text) = value.as_str() {
                    output.push(expand(text, vars)?)
                } else if let Some(items) = value.as_array() {
                    for item in items {
                        output.push(expand(item.as_str().ok_or("Invalid argument")?, vars)?);
                    }
                }
            }
        }
        Ok(output)
    }
    let mut args = vec!["-Xms512M".into(), format!("-Xmx{}M", profile.memory_mb)];
    if let Some(jvm) = receipt.metadata.get("arguments").and_then(|v| v.get("jvm")) {
        args.extend(values(jvm, &receipt.platform, &features, &vars)?);
    } else {
        args.extend([
            format!("-Djava.library.path={}", natives.display()),
            "-cp".into(),
            classpath,
        ]);
        if receipt.platform.os == "osx" {
            args.push("-XstartOnFirstThread".into());
        }
    }
    args.push(receipt.main_class.clone());
    if let Some(game) = receipt
        .metadata
        .get("arguments")
        .and_then(|v| v.get("game"))
    {
        args.extend(values(game, &receipt.platform, &features, &vars)?);
    } else {
        let legacy = receipt.metadata["minecraftArguments"]
            .as_str()
            .ok_or("Historical launch arguments are unsupported")?;
        args.extend(
            legacy
                .split_whitespace()
                .map(|s| expand(s, &vars))
                .collect::<Result<Vec<_>>>()?,
        );
    }
    if settings.fullscreen {
        args.push("--fullscreen".into());
    }
    Ok(args)
}
pub fn java_major(executable: &str) -> Result<u32> {
    let mut command = Command::new(executable);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .arg("-version")
        .output()
        .map_err(|_| "Java could not be started. Choose java/java.exe in Settings.")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let re = regex::Regex::new(r#"version\s+"(\d+)(?:\.(\d+))?"#).unwrap();
    let capture = re.captures(&text).ok_or("Cannot detect Java version")?;
    let major = capture[1].parse::<u32>().map_err(|e| e.to_string())?;
    if major == 1 {
        capture
            .get(2)
            .ok_or("Cannot detect legacy Java")?
            .as_str()
            .parse()
            .map_err(|e: std::num::ParseIntError| e.to_string())
    } else {
        Ok(major)
    }
}
pub fn start(
    receipt: &Receipt,
    profile: &Profile,
    settings: &Settings,
    session: &Session,
) -> Result<std::process::Child> {
    if !session.offline && !session.valid() {
        return Err("Online session expired; sign in again".into());
    }
    let executable = if profile.java.is_empty() {
        &settings.java
    } else {
        &profile.java
    };
    let actual = java_major(executable)?;
    if (receipt.java_major <= 8 && actual != 8)
        || (receipt.java_major > 8 && actual < receipt.java_major)
    {
        return Err(format!(
            "This version needs Java {}. Selected executable is Java {actual}.",
            receipt.java_major
        ));
    }
    let args = launch_arguments(receipt, profile, settings, session)?;
    for item in &receipt.classpath {
        if !safe_join(&settings.game_root, item)?.is_file() {
            return Err("Installation is incomplete. Install/repair the profile first.".into());
        }
    }
    let game = safe_join(&settings.game_root, &format!("instances/{}", profile.id))?;
    let mut command = Command::new(executable);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .args(args)
        .current_dir(game)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Minecraft could not start: {e}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Value {
        serde_json::from_str(
            &fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures")
                    .join(name),
            )
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn real_manifests_resolve_platform_libraries_and_java() {
        for name in [
            "minecraft-1.7.10.json",
            "minecraft-1.12.2.json",
            "minecraft-1.21.1.json",
            "minecraft-26.3.json",
        ] {
            let v = fixture(name);
            for os in ["windows", "linux", "osx"] {
                let p = Platform {
                    os: os.into(),
                    arch: "x86_64".into(),
                    version: "10.0".into(),
                };
                let (d, c, _) = plan_libraries(&v, &p).unwrap();
                assert!(d.len() > 10 && c.len() > 10);
                for item in d {
                    assert!(!item.hash.is_empty());
                    safe_join(Path::new("cache"), &item.path).unwrap();
                }
            }
        }
    }
    #[test]
    fn fabric_merge_and_argument_expansion_are_usable() {
        let v = merge_fabric(
            fixture("minecraft-1.21.1.json"),
            &fixture("fabric-profile.json"),
        )
        .unwrap();
        assert!(v["mainClass"].as_str().unwrap().contains("fabric"));
        let platform = Platform {
            os: "windows".into(),
            arch: "x86_64".into(),
            version: "10.0".into(),
        };
        let (_, mut classpath, natives) = plan_libraries(&v, &platform).unwrap();
        classpath.push("versions/1.21.1/1.21.1.jar".into());
        let receipt = Receipt {
            minecraft: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.19.5".into(),
            java_major: 21,
            main_class: v["mainClass"].as_str().unwrap().into(),
            metadata: v,
            classpath,
            native_jars: natives,
            platform,
            asset_index: "17".into(),
        };
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
        let s = Session {
            name: "LocalPlayer".into(),
            uuid: "0123456789abcdef0123456789abcdef".into(),
            xuid: String::new(),
            access: "0".into(),
            refresh: String::new(),
            expires: u64::MAX,
            offline: true,
        };
        let args = launch_arguments(&receipt, &p, &Settings::default(), &s).unwrap();
        assert!(args.contains(&"-Xmx4096M".into()));
        assert!(args.contains(&"LocalPlayer".into()));
        assert!(!args.iter().any(|s| s.contains("${")));
    }
    #[test]
    fn rules_obey_last_matching_action_and_features() {
        let p = Platform {
            os: "windows".into(),
            arch: "x86_64".into(),
            version: "10.0".into(),
        };
        assert!(!allowed(
            Some(&json!([{"action":"allow"},{"action":"disallow","os":{"name":"windows"}}])),
            &p,
            &BTreeMap::new()
        ));
        assert!(!allowed(
            Some(&json!([{"action":"allow","features":{"is_demo_user":true}}])),
            &p,
            &BTreeMap::new()
        ));
    }
}

#[cfg(test)]
mod archive_tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn native_zip_slip_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file("../evil.dll", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        assert!(extract_natives(&path, &dir.path().join("natives")).is_err());
        assert!(!dir.path().join("evil.dll").exists());
    }
    #[test]
    fn legacy_offline_arguments_and_java_requirements() {
        for (name, java) in [
            ("minecraft-1.7.10.json", 8),
            ("minecraft-1.12.2.json", 8),
            ("minecraft-1.21.1.json", 21),
            ("minecraft-26.3.json", 25),
        ] {
            let v: Value = serde_json::from_str(
                &fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures")
                        .join(name),
                )
                .unwrap(),
            )
            .unwrap();
            let platform = Platform {
                os: "linux".into(),
                arch: "x86_64".into(),
                version: String::new(),
            };
            let (_, classpath, native_jars) = plan_libraries(&v, &platform).unwrap();
            let major = v["javaVersion"]["majorVersion"].as_u64().unwrap_or(8) as u32;
            assert_eq!(major, java);
            let receipt = Receipt {
                minecraft: v["id"].as_str().unwrap().into(),
                loader: "vanilla".into(),
                loader_version: String::new(),
                java_major: major,
                main_class: v["mainClass"].as_str().unwrap().into(),
                asset_index: v["assetIndex"]["id"].as_str().unwrap().into(),
                metadata: v,
                classpath,
                native_jars,
                platform,
            };
            let p = Profile {
                id: uuid::Uuid::new_v4().to_string(),
                name: "QA".into(),
                version: receipt.minecraft.clone(),
                loader: "vanilla".into(),
                loader_version: String::new(),
                java: String::new(),
                memory_mb: 4096,
                mods: vec![],
            };
            let session = crate::auth::offline_session(&crate::auth::offline("Steve").unwrap());
            let args = launch_arguments(&receipt, &p, &Settings::default(), &session).unwrap();
            assert!(args.contains(&session.uuid));
            assert!(args.contains(&"Steve".into()));
            assert!(!args.iter().any(|a| a.contains("${")));
        }
    }
}
