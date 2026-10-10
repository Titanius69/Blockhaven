use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    pub url: String,
    pub sha256: Option<String>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > 8192 {
            return Err("Manifest exceeds 8 KiB".into());
        }
        let mut version = None;
        let mut url = None;
        let mut sha256 = None;
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or("Invalid manifest line; expected key = value")?;
            let value = value.trim();
            let slot = match key.trim() {
                "version" => &mut version,
                "url" => &mut url,
                "sha256" => &mut sha256,
                _ => return Err(format!("Unknown manifest key: {}", key.trim())),
            };
            if slot.replace(value.to_owned()).is_some() {
                return Err(format!("Duplicate manifest key: {}", key.trim()));
            }
        }
        let version = version.ok_or("Manifest has no version")?;
        if version.is_empty()
            || version.len() > 64
            || !version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err("Invalid launcher version".into());
        }
        let url = url.ok_or("Manifest has no download URL")?;
        let parsed = reqwest::Url::parse(&url).map_err(|_| "Invalid launcher download URL")?;
        if parsed.scheme() != "https"
            || parsed.host_str() != Some("github.com")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.port().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !parsed
                .path()
                .starts_with("/Titanius69/Blockhaven/releases/download/")
            || !parsed.path().ends_with(".exe")
        {
            return Err(
                "Launcher URL must be an HTTPS executable release from Titanius69/Blockhaven"
                    .into(),
            );
        }
        let sha256 = sha256.map(|v| v.to_ascii_lowercase());
        if sha256
            .as_ref()
            .is_some_and(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid manifest SHA-256".into());
        }
        Ok(Self {
            version,
            url,
            sha256,
        })
    }
    pub fn text(&self) -> String {
        let mut text = format!("version = {}\nurl = {}\n", self.version, self.url);
        if let Some(hash) = &self.sha256 {
            text.push_str(&format!("sha256 = {hash}\n"));
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_manifest_roundtrips_and_compares_semantically() {
        let a = Manifest::parse("version = 0.3.2\nurl = https://github.com/Titanius69/Blockhaven/releases/download/0.3.2/blockhaven.exe\n").unwrap();
        let b = Manifest::parse(&format!(
            "\u{feff}# Release\r\nurl={}\r\n version={} \r\n",
            a.url, a.version
        ))
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(a, Manifest::parse(&a.text()).unwrap());
        assert_ne!(
            a,
            Manifest {
                version: "0.3.3".into(),
                ..a.clone()
            }
        );
    }
    #[test]
    fn malformed_or_untrusted_manifests_are_rejected() {
        for text in ["version=0.3.2", "version=../x\nurl=https://github.com/Titanius69/Blockhaven/releases/download/a/b.exe", "version=a\nversion=b\nurl=https://github.com/Titanius69/Blockhaven/releases/download/a/b.exe", "version=a\nurl=http://github.com/Titanius69/Blockhaven/releases/download/a/b.exe", "version=a\nurl=https://github.com/other/repo/releases/download/a/b.exe", "version=a\nurl=https://github.com/Titanius69/Blockhaven/releases/download/a/b.exe\nsha256=no"] {
            assert!(Manifest::parse(text).is_err(), "{text}");
        }
        assert!(Manifest::parse(&"a".repeat(8193)).is_err());
    }
}
