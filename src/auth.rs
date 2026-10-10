use crate::{
    model::{now, OfflineAccount, Progress, Result, Session},
    net::Net,
};
use serde_json::{json, Value};
use sha1::Digest;
use std::{sync::Arc, time::Duration};
pub fn offline(name: &str) -> Result<OfflineAccount> {
    if !(3..=16).contains(&name.len())
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("Username must contain 3–16 letters, numbers or underscores".into());
    }
    let mut bytes: [u8; 16] = md5::Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(OfflineAccount {
        name: name.into(),
        uuid: uuid::Uuid::from_bytes(bytes).simple().to_string(),
    })
}
pub fn offline_session(a: &OfflineAccount) -> Session {
    Session {
        name: a.name.clone(),
        uuid: a.uuid.clone(),
        xuid: String::new(),
        access: "0".into(),
        refresh: String::new(),
        expires: u64::MAX,
        offline: true,
    }
}
#[derive(serde::Deserialize)]
pub struct Device {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "interval")]
    pub interval: u64,
}
fn interval() -> u64 {
    5
}
pub fn device(net: &Net, client: &str) -> Result<Device> {
    if uuid::Uuid::parse_str(client).is_err() {
        return Err("Set your registered Microsoft OAuth client ID in Settings first".into());
    }
    net.client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", client),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .map_err(|_| "Cannot reach Microsoft")?
        .error_for_status()
        .map_err(|_| "Microsoft rejected this client ID/device flow")?
        .json()
        .map_err(|_| "Invalid Microsoft device response".into())
}
fn post(net: &Net, url: &str, body: Value) -> Result<Value> {
    net.client.post(url).json(&body).send().map_err(|_|"Authentication network error")?.error_for_status().map_err(|e|format!("Authentication rejected (HTTP {}). Check account eligibility and OAuth registration.",e.status().map(|s|s.as_u16()).unwrap_or(0)))?.json().map_err(|_|"Invalid authentication response".into())
}
fn token(value: &Value) -> Result<String> {
    value["Token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Xbox returned no token".into())
}
pub fn poll(net: &Net, client: &str, d: &Device, progress: &Arc<Progress>) -> Result<Session> {
    let deadline = now() + d.expires_in;
    let mut wait = d.interval.max(5);
    loop {
        for _ in 0..wait * 4 {
            progress.check()?;
            std::thread::sleep(Duration::from_millis(250));
        }
        if now() >= deadline {
            return Err("Sign-in code expired; try again".into());
        }
        let response = net
            .client
            .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client),
                ("device_code", &d.device_code),
            ])
            .send()
            .map_err(|_| "Microsoft token request failed")?;
        let successful = response.status().is_success();
        let data: Value = response
            .json()
            .map_err(|_| "Invalid Microsoft token response")?;
        if successful {
            return exchange(net, &data, progress);
        }
        match data["error"].as_str().unwrap_or("") {
            "authorization_pending" => continue,
            "slow_down" => {
                wait += 5;
                continue;
            }
            "authorization_declined" => return Err("Sign-in declined".into()),
            "expired_token" => return Err("Sign-in code expired".into()),
            _ => {
                return Err(
                    "Microsoft rejected sign-in. Check your OAuth client registration.".into(),
                )
            }
        }
    }
}
pub fn refresh(
    net: &Net,
    client: &str,
    session: &Session,
    progress: &Arc<Progress>,
) -> Result<Session> {
    if session.offline || session.valid() {
        return Ok(session.clone());
    }
    let data: Value = net
        .client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client),
            ("refresh_token", session.refresh.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .map_err(|_| "Cannot refresh Microsoft session")?
        .error_for_status()
        .map_err(|_| "Session expired; sign in again")?
        .json()
        .map_err(|_| "Invalid token response")?;
    let mut next = exchange(net, &data, progress)?;
    if next.refresh.is_empty() {
        next.refresh = session.refresh.clone();
    }
    Ok(next)
}
fn exchange(net: &Net, data: &Value, progress: &Arc<Progress>) -> Result<Session> {
    progress.check()?;
    progress.set(0, 1, "Signing in to Xbox and Minecraft".into());
    let msa = data["access_token"]
        .as_str()
        .ok_or("No Microsoft access token")?;
    let xbox = post(
        net,
        "https://user.auth.xboxlive.com/user/authenticate",
        json!({"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={msa}")},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"}),
    )?;
    progress.check()?;
    let xsts = post(
        net,
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[token(&xbox)?]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"}),
    )?;
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or("No Xbox user hash")?;
    let login = post(
        net,
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        json!({"identityToken":format!("XBL3.0 x={uhs};{}",token(&xsts)?)}),
    )?;
    let access = login["access_token"]
        .as_str()
        .ok_or("No Minecraft access token")?;
    progress.check()?;
    let get = |url: &str| -> Result<Value> {
        net.client
            .get(url)
            .bearer_auth(access)
            .send()
            .map_err(|_| "Minecraft account request failed")?
            .error_for_status()
            .map_err(|_| "Minecraft Java account could not be verified")?
            .json()
            .map_err(|_| "Invalid account response".into())
    };
    let entitlement = get("https://api.minecraftservices.com/entitlements/mcstore")?;
    if !entitlement["items"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        return Err("This Microsoft account does not have a Minecraft Java entitlement".into());
    }
    let profile = get("https://api.minecraftservices.com/minecraft/profile")?;
    Ok(Session {
        name: profile["name"]
            .as_str()
            .ok_or("Minecraft profile name missing")?
            .into(),
        uuid: profile["id"]
            .as_str()
            .ok_or("Minecraft profile ID missing")?
            .into(),
        xuid: xsts["DisplayClaims"]["xui"][0]["xid"]
            .as_str()
            .unwrap_or("")
            .into(),
        access: access.into(),
        refresh: data["refresh_token"].as_str().unwrap_or("").into(),
        expires: now() + login["expires_in"].as_u64().unwrap_or(3600),
        offline: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_compatible_offline_uuid() {
        let a = offline("Notch").unwrap();
        assert_eq!(a.uuid, "b50ad385829d3141a2167e7d7539ba7f");
        assert!(offline_session(&a).offline);
        for n in ["a", "bad name", "../evil", "abcdefghijklmnopq"] {
            assert!(offline(n).is_err());
        }
    }
}
