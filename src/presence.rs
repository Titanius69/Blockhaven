//! Local Discord IPC only; no tokens, Discord HTTP requests or UI-thread I/O.
use crate::model::{Shared, State};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const MAX_FRAME: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, PartialEq)]
struct Desired {
    application_id: String,
    activity: Value,
}

fn short(text: &str) -> String {
    // Discord limits these fields to 128 bytes. Preserve UTF-8 boundaries.
    text.chars()
        .scan(0, |size, ch| {
            *size += ch.len_utf8();
            (*size <= 128).then_some(ch)
        })
        .collect()
}
fn desired(s: &State) -> Option<Desired> {
    let d = &s.config.settings.discord;
    if !d.enabled || d.application_id.is_empty() {
        return None;
    }
    let mut activity = if s.running {
        let profile = s.config.profiles.iter().find(|p| p.id == s.running_profile);
        let state = profile
            .map(|p| {
                format!(
                    "Minecraft {} · {}",
                    p.version,
                    if p.loader == "fabric" {
                        "Fabric"
                    } else {
                        "Vanilla"
                    }
                )
            })
            .unwrap_or_else(|| "Minecraft Java Edition".into());
        json!({"details": "Playing Minecraft", "state": short(&state), "timestamps": {"start":s.running_since}})
    } else {
        let state = match s.page.as_str() {
            "modrinth" => "Discovering mods",
            "installed" => "Managing mods",
            "profiles" => "Managing instances",
            "settings" => "Configuring the launcher",
            _ => "Choosing the next adventure",
        };
        json!({"details":"Blockhaven Launcher","state":state})
    };
    if !d.image_key.is_empty() {
        activity["assets"] =
            json!({"large_image":d.image_key,"large_text":"Blockhaven Minecraft Launcher"});
    }
    Some(Desired {
        application_id: d.application_id.clone(),
        activity,
    })
}

pub struct PresenceGuard {
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for PresenceGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // I/O is bounded; allow the worker to clear its activity before returning.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg_attr(not(feature = "native"), allow(dead_code))]
pub fn start(shared: &Shared) -> PresenceGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let shared = shared.clone();
    let worker = std::thread::spawn(move || {
        let mut connected: Option<Client> = None;
        let mut sent: Option<Desired> = None;
        let mut next_send = Instant::now();
        let mut next_connect = Instant::now();
        loop {
            if worker_stop.load(Ordering::Relaxed) {
                break;
            }
            let target = { desired(&shared.lock().unwrap()) };
            if connected.as_ref().is_some_and(|c| {
                target
                    .as_ref()
                    .is_none_or(|t| t.application_id != c.application_id)
            }) {
                if let Some(mut client) = connected.take() {
                    let _ = client.activity(Value::Null);
                }
                sent = None;
                next_connect = Instant::now();
            }
            if let Some(target) = target {
                if connected.is_none() && Instant::now() >= next_connect {
                    match Client::connect(&target.application_id) {
                        Ok(client) => {
                            connected = Some(client);
                            sent = None;
                        }
                        Err(_) => {
                            status(&shared, "Waiting for desktop Discord (retrying)");
                            next_connect = Instant::now() + Duration::from_secs(10);
                        }
                    }
                }
                if let Some(client) = connected.as_mut() {
                    if sent.as_ref() != Some(&target) || Instant::now() >= next_send {
                        match client.activity(target.activity.clone()) {
                            Ok(()) => {
                                status(
                                    &shared,
                                    if target.activity["details"] == "Playing Minecraft" {
                                        "Connected · Playing Minecraft"
                                    } else {
                                        "Connected · Launcher activity"
                                    },
                                );
                                sent = Some(target);
                                next_send = Instant::now() + Duration::from_secs(15);
                            }
                            Err(error) => {
                                status(
                                    &shared,
                                    &format!("Discord unavailable: {error}. Retrying…"),
                                );
                                connected = None;
                                sent = None;
                                next_connect = Instant::now() + Duration::from_secs(10);
                            }
                        }
                    }
                }
            } else {
                let s = shared.lock().unwrap();
                let enabled = s.config.settings.discord.enabled;
                drop(s);
                status(
                    &shared,
                    if enabled {
                        "Enter a Discord Application ID to connect"
                    } else {
                        "Rich Presence is off"
                    },
                );
            }
            // Responsive shutdown and settings changes without spinning.
            for _ in 0..5 {
                if worker_stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        if let Some(mut client) = connected {
            let _ = client.activity(Value::Null);
        }
    });
    PresenceGuard {
        stop,
        worker: Some(worker),
    }
}
fn status(shared: &Shared, text: &str) {
    shared.lock().unwrap().discord_status = text.into();
}

#[cfg(unix)]
type Pipe = std::os::unix::net::UnixStream;
#[cfg(windows)]
struct Pipe(std::fs::File);
#[cfg(windows)]
impl Read for Pipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        use std::os::windows::io::AsRawHandle;
        let mut available = 0;
        let ok = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.0.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if available == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = buffer.len().min(available as usize);
        self.0.read(&mut buffer[..count])
    }
}
#[cfg(windows)]
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.0.write(bytes) {
            Ok(0) => Err(io::ErrorKind::WouldBlock.into()),
            Err(e) if e.raw_os_error() == Some(232) => Err(io::ErrorKind::WouldBlock.into()),
            result => result,
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
fn connect_pipe() -> io::Result<Pipe> {
    #[cfg(windows)]
    {
        for index in 0..10 {
            if let Ok(file) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!(r"\\.\pipe\discord-ipc-{index}"))
            {
                use std::os::windows::io::AsRawHandle;
                use windows_sys::Win32::System::Pipes::{SetNamedPipeHandleState, PIPE_NOWAIT};
                let mode = PIPE_NOWAIT;
                let ok = unsafe {
                    SetNamedPipeHandleState(
                        file.as_raw_handle(),
                        &mode,
                        std::ptr::null(),
                        std::ptr::null(),
                    )
                };
                if ok != 0 {
                    return Ok(Pipe(file));
                }
            }
        }
    }
    #[cfg(unix)]
    {
        let mut directories = Vec::new();
        for name in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            if let Some(path) = std::env::var_os(name) {
                directories.push(std::path::PathBuf::from(path));
            }
        }
        directories.push(std::path::PathBuf::from("/tmp"));
        for directory in directories {
            for base in [
                &directory,
                &directory.join("app/com.discordapp.Discord"),
                &directory.join("snap.discord"),
            ] {
                for index in 0..10 {
                    if let Ok(stream) = Pipe::connect(base.join(format!("discord-ipc-{index}"))) {
                        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
                        stream.set_write_timeout(Some(IO_TIMEOUT))?;
                        return Ok(stream);
                    }
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "desktop Discord is not running",
    ))
}

struct Client {
    pipe: Pipe,
    application_id: String,
    sequence: u64,
}
impl Client {
    fn connect(application_id: &str) -> io::Result<Self> {
        let mut client = Self {
            pipe: connect_pipe()?,
            application_id: application_id.into(),
            sequence: 0,
        };
        client.handshake()?;
        Ok(client)
    }
    fn handshake(&mut self) -> io::Result<()> {
        self.send(0, &json!({"v":1,"client_id":self.application_id}))?;
        let ready = self.receive(Instant::now() + IO_TIMEOUT)?;
        if ready["evt"] != "READY" {
            return Err(io::Error::other("Discord rejected the application ID"));
        }
        Ok(())
    }
    fn send(&mut self, opcode: u32, payload: &Value) -> io::Result<()> {
        let bytes = serde_json::to_vec(payload)?;
        if bytes.len() > MAX_FRAME {
            return Err(io::Error::other("Discord frame too large"));
        }
        let mut frame = Vec::with_capacity(8 + bytes.len());
        frame.extend_from_slice(&opcode.to_le_bytes());
        frame.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        frame.extend_from_slice(&bytes);
        let deadline = Instant::now() + IO_TIMEOUT;
        let mut remaining = frame.as_slice();
        while !remaining.is_empty() {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Discord write timed out",
                ));
            }
            match self.pipe.write(remaining) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => remaining = &remaining[n..],
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn read_exact_until(&mut self, mut bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
        while !bytes.is_empty() {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Discord response timed out",
                ));
            }
            match self.pipe.read(bytes) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => {
                    bytes = &mut bytes[n..];
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn receive(&mut self, deadline: Instant) -> io::Result<Value> {
        loop {
            let mut header = [0; 8];
            self.read_exact_until(&mut header, deadline)?;
            let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
            let len = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
            if len > MAX_FRAME {
                return Err(io::Error::other("Discord response too large"));
            }
            let mut bytes = vec![0; len];
            self.read_exact_until(&mut bytes, deadline)?;
            let payload: Value = serde_json::from_slice(&bytes)?;
            match opcode {
                1 => return Ok(payload),
                3 => self.send(4, &payload)?,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "Discord disconnected",
                    ))
                }
            }
        }
    }
    fn activity(&mut self, activity: Value) -> io::Result<()> {
        self.sequence += 1;
        let nonce = format!("blockhaven-{}-{}", std::process::id(), self.sequence);
        self.send(1,&json!({"cmd":"SET_ACTIVITY","args":{"pid":std::process::id(),"activity":activity},"nonce":nonce}))?;
        let deadline = Instant::now() + IO_TIMEOUT;
        loop {
            let response = self.receive(deadline)?;
            if response["nonce"] == nonce {
                if response["evt"] == "ERROR" {
                    return Err(io::Error::other(
                        "Discord rejected the presence. Check the Application ID and artwork key",
                    ));
                }
                if response["cmd"] != "SET_ACTIVITY" {
                    return Err(io::Error::other("Unexpected Discord response"));
                }
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::now;
    #[test]
    fn legacy_config_and_activity_exclude_account_secrets() {
        let root = tempfile::tempdir().unwrap();
        let mut s = State::load(root.path().join("config.json"), true).unwrap();
        let old = serde_json::to_value(&s.config.settings).unwrap();
        let mut old = old.as_object().unwrap().clone();
        old.remove("discord");
        let decoded: crate::model::Settings = serde_json::from_value(Value::Object(old)).unwrap();
        assert!(decoded.discord.enabled);
        assert_eq!(
            decoded.discord.application_id,
            crate::model::DEFAULT_DISCORD_APPLICATION_ID
        );
        assert_eq!(
            desired(&s).unwrap().application_id,
            crate::model::DEFAULT_DISCORD_APPLICATION_ID
        );
        // Existing blank IDs migrate, while custom IDs and disabled presence stay intact.
        for id in ["", "123456789012345678"] {
            let mut config = s.config.clone();
            config.settings.discord.application_id = id.into();
            config.settings.discord.enabled = false;
            crate::net::atomic_json(&s.config_path, &config).unwrap();
            let loaded = State::load(s.config_path.clone(), true).unwrap();
            assert_eq!(
                loaded.config.settings.discord.application_id,
                if id.is_empty() {
                    crate::model::DEFAULT_DISCORD_APPLICATION_ID
                } else {
                    id
                }
            );
            assert!(!loaded.config.settings.discord.enabled);
        }
        s.config.settings.discord.application_id = "123456789012345678".into();
        let idle = desired(&s).unwrap();
        assert_eq!(idle.activity["details"], "Blockhaven Launcher");
        s.running = true;
        s.running_since = now();
        let playing = desired(&s).unwrap();
        assert_eq!(playing.activity["details"], "Playing Minecraft");
        assert_eq!(playing.activity["timestamps"]["start"], s.running_since);
        assert!(!playing.activity.to_string().contains("uuid"));
        s.config.settings.discord.enabled = false;
        assert!(desired(&s).is_none());
    }
    #[cfg(unix)]
    #[test]
    fn real_ipc_frames_acknowledge_ping_playing_and_clear() {
        let (pipe, mut server) = Pipe::pair().unwrap();
        pipe.set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        server.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        let thread = std::thread::spawn(move || {
            fn read_frame(server: &mut Pipe) -> (u32, Value) {
                let mut header = [0; 8];
                server.read_exact(&mut header).unwrap();
                let size = u32::from_le_bytes(header[4..].try_into().unwrap());
                let mut body = vec![0; size as usize];
                server.read_exact(&mut body).unwrap();
                (
                    u32::from_le_bytes(header[..4].try_into().unwrap()),
                    serde_json::from_slice(&body).unwrap(),
                )
            }
            fn send_frame(server: &mut Pipe, opcode: u32, value: Value) {
                let body = serde_json::to_vec(&value).unwrap();
                let mut frame = Vec::new();
                frame.extend_from_slice(&opcode.to_le_bytes());
                frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
                frame.extend_from_slice(&body);
                server.write_all(&frame).unwrap();
            }
            let (opcode, hello) = read_frame(&mut server);
            assert_eq!(opcode, 0);
            assert_eq!(hello["client_id"], "123456789012345678");
            send_frame(&mut server, 1, json!({"evt":"READY"}));
            for stage in 0..2 {
                let mut header = [0; 8];
                server.read_exact(&mut header).unwrap();
                assert_eq!(u32::from_le_bytes(header[..4].try_into().unwrap()), 1);
                let size = u32::from_le_bytes(header[4..].try_into().unwrap());
                let mut body = vec![0; size as usize];
                server.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(request["cmd"], "SET_ACTIVITY");
                if stage == 0 {
                    send_frame(&mut server, 3, json!({"keepalive":1}));
                    let (opcode, pong) = read_frame(&mut server);
                    assert_eq!(opcode, 4);
                    assert_eq!(pong["keepalive"], 1);
                }
                if stage == 0 {
                    assert_eq!(request["args"]["activity"]["details"], "Playing Minecraft");
                } else {
                    assert!(request["args"]["activity"].is_null());
                }
                let response = json!({"cmd":"SET_ACTIVITY","nonce":request["nonce"]});
                let body = serde_json::to_vec(&response).unwrap();
                // Deliberately fragmented frame: exercise bounded partial reads.
                server.write_all(&1u32.to_le_bytes()).unwrap();
                server
                    .write_all(&(body.len() as u32).to_le_bytes())
                    .unwrap();
                for chunk in body.chunks(3) {
                    server.write_all(chunk).unwrap();
                }
            }
        });
        let mut client = Client {
            pipe,
            application_id: "123456789012345678".into(),
            sequence: 0,
        };
        client.handshake().unwrap();
        client
            .activity(json!({"details":"Playing Minecraft"}))
            .unwrap();
        client.activity(Value::Null).unwrap();
        thread.join().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn stalled_and_oversized_discord_never_wait_forever() {
        let (pipe, mut server) = Pipe::pair().unwrap();
        pipe.set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut client = Client {
            pipe,
            application_id: String::new(),
            sequence: 0,
        };
        assert!(client
            .receive(Instant::now() + Duration::from_millis(50))
            .is_err());
        server.write_all(&1u32.to_le_bytes()).unwrap();
        server
            .write_all(&((MAX_FRAME + 1) as u32).to_le_bytes())
            .unwrap();
        assert!(client.receive(Instant::now() + IO_TIMEOUT).is_err());
    }
}
