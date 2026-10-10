use sha1::{Digest, Sha1};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("read resources")
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        assert!(!path.is_symlink(), "resource symlinks are not supported");
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}
fn main() {
    println!("cargo:rerun-if-changed=resources");
    println!("cargo:rerun-if-env-changed=BLOCKHAVEN_RC_TOOLKIT");
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("resources");
    let mut files = Vec::new();
    collect(&root, &mut files);
    let mut source = format!(
        "pub const RESOURCE_ROOT: &str = {:?};\npub static ASSETS: &[Asset] = &[\n",
        root.to_str().unwrap()
    );
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let data = fs::read(&path).expect("read resource");
        let hash = format!("{:x}", Sha1::digest(&data));
        let logical = format!("/{relative}");
        let hashed = match logical.rsplit_once('.') {
            Some((base, ext)) => format!("{base}.{}.{ext}", &hash[..12]),
            None => format!("{logical}.{}", &hash[..12]),
        };
        source.push_str(&format!("Asset {{ path: {logical:?}, hashed_path: {hashed:?}, mime: {:?}, bytes: &{data:?} }},\n", mime(&path)));
    }
    source.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("resources.rs"),
        source,
    )
    .unwrap();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("resources/icons/blockhaven.ico")
            .set("ProductName", "Blockhaven")
            .set("FileDescription", "Blockhaven Minecraft Launcher")
            .set("FileVersion", "0.3.2.0")
            .set("ProductVersion", "0.3.2.0");
        if let Ok(path) = env::var("BLOCKHAVEN_RC_TOOLKIT") {
            res.set_toolkit_path(&path)
                .set_windres_path(&format!("{path}/x86_64-w64-mingw32-windres"))
                .set_ar_path(&format!("{path}/x86_64-w64-mingw32-ar"));
        }
        res.compile().expect("embed application icon");
    }
}
