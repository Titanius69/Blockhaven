//! Bounded, non-blocking Modrinth artwork cache. The WebView sees only sanitized PNG data URLs.
use crate::{
    model::{Result, Shared},
    net::{safe_join, Net},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha1::Digest;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
pub fn approved_url(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("cdn.modrinth.com")
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
    })
}
fn cache_key(url: &str) -> String {
    format!("{}.png", hex::encode(sha1::Sha1::digest(url.as_bytes())))
}
fn encode(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}
pub fn sanitize(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|_| "Invalid mod icon")?
        .thumbnail(128, 128);
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}
pub fn cached(root: &Path, url: &str) -> Option<String> {
    if !approved_url(url) {
        return None;
    }
    let path = safe_join(root, &cache_key(url)).ok()?;
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    Some(encode(&sanitize(&bytes).ok()?))
}
fn fetch(net: &Net, root: &Path, url: &str) -> Result<String> {
    if !approved_url(url) {
        return Err("Unapproved icon host".into());
    }
    if let Some(data) = cached(root, url) {
        return Ok(data);
    }
    let response = net
        .client
        .get(url)
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .map_err(|_| "Icon request failed")?
        .error_for_status()
        .map_err(|_| "Icon was not found")?;
    if response.content_length().unwrap_or(0) > 1024 * 1024 {
        return Err("Mod icon is too large".into());
    }
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Mod icon is too large".into());
    }
    let png = sanitize(&bytes)?;
    let path = safe_join(root, &cache_key(url))?;
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut tmp = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    tmp.write_all(&png).map_err(|e| e.to_string())?;
    tmp.persist(path).map_err(|e| e.to_string())?;
    Ok(encode(&png))
}
pub fn load_saved(s: &mut crate::model::State) {
    let root = s.config_path.parent().unwrap().join("icons");
    for p in &s.config.profiles {
        for m in &p.mods {
            if let Some(url) = &m.icon_url {
                if let Some(data) = cached(&root, url) {
                    s.icons.insert(m.project.clone(), data);
                }
            }
        }
    }
}
pub fn queue(shared: &Shared) {
    let jobs;
    let root;
    {
        let mut s = shared.lock().unwrap();
        root = s.config_path.parent().unwrap().join("icons");
        let mut items = BTreeMap::new();
        for h in &s.hits {
            items.insert(h.project_id.clone(), h.icon_url.clone());
        }
        for p in &s.config.profiles {
            for m in &p.mods {
                items
                    .entry(m.project.clone())
                    .or_insert_with(|| m.icon_url.clone());
            }
        }
        jobs = items
            .into_iter()
            .filter(|(id, _)| !s.icons.contains_key(id) && !s.icons_inflight.contains(id))
            .collect::<Vec<_>>();
        for (id, _) in &jobs {
            s.icons_inflight.insert(id.clone());
        }
    }
    if jobs.is_empty() {
        return;
    }
    let shared = shared.clone();
    std::thread::spawn(move || {
        let Ok(net) = Net::new() else {
            let mut s = shared.lock().unwrap();
            for (id, _) in jobs {
                s.icons_inflight.remove(&id);
            }
            return;
        };
        let index = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let shared = &shared;
                let jobs = &jobs;
                let root = &root;
                let net = &net;
                let index = &index;
                scope.spawn(move || loop {
                    let i = index.fetch_add(1, Ordering::Relaxed);
                    let Some((id, url)) = jobs.get(i) else { break };
                    let url = url
                        .clone()
                        .or_else(|| crate::mods::project(net, id).ok().and_then(|p| p.icon_url));
                    let data = url.as_ref().and_then(|url| fetch(net, root, url).ok());
                    let mut s = shared.lock().unwrap();
                    s.icons_inflight.remove(id);
                    if let Some(data) = data {
                        s.icons.insert(id.clone(), data);
                        s.icons_dirty = true;
                    }
                    if let Some(url) = url {
                        let mut config = s.config.clone();
                        let mut changed = false;
                        for p in &mut config.profiles {
                            for m in &mut p.mods {
                                if m.project == *id && m.icon_url.as_ref() != Some(&url) {
                                    m.icon_url = Some(url.clone());
                                    changed = true;
                                }
                            }
                        }
                        if changed && crate::net::atomic_json(&s.config_path, &config).is_ok() {
                            s.config = config;
                        }
                    }
                });
            }
        });
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_cdn_images_and_valid_bounded_content() {
        for u in [
            "http://cdn.modrinth.com/x",
            "https://evil.com/x",
            "https://cdn.modrinth.com.evil/x",
            "https://user@cdn.modrinth.com/x",
        ] {
            assert!(!approved_url(u));
        }
        assert!(approved_url("https://cdn.modrinth.com/data/x/icon.png"));
        assert!(sanitize(b"<svg onload='evil()'></svg>").is_err());
        let png = include_bytes!("../resources/icons/blockhaven.png");
        let out = sanitize(png).unwrap();
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (128, 128));
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn corrupt_cache_is_ignored_and_legacy_records_migrate() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://cdn.modrinth.com/data/example/icon.png";
        std::fs::write(dir.path().join(cache_key(url)), b"\x89PNG\r\n\x1a\ninvalid").unwrap();
        assert!(cached(dir.path(), url).is_none());
        let old = serde_json::json!({"project":"abc","version":"v","title":"Example","filename":"a.jar","hash":"xyz","enabled":true,"required":[]});
        let record: crate::model::ModRecord = serde_json::from_value(old).unwrap();
        assert!(record.icon_url.is_none());
    }
    #[test]
    #[ignore = "fetches a real Modrinth icon into a temporary cache"]
    fn live_modrinth_artwork() {
        let net = Net::new().unwrap();
        let project = crate::mods::project(&net, "sodium").unwrap();
        let url = project.icon_url.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let data = fetch(&net, dir.path(), &url).unwrap();
        assert!(data.starts_with("data:image/png;base64,"));
        assert_eq!(cached(dir.path(), &url).unwrap(), data);
    }
}
