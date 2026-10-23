//! The control channel: lets a program or an AI drive the open window.
//!
//! `meltcad --control 7777` listens on 127.0.0.1 only. Each line is one JSON command (see
//! `automation.rs`) with an optional `"id"`; the first request of a connection must carry the launch
//! token as `"token"`. The answer is one line: `{"id":1,"ok":true,"result":...}` or
//! `{"id":1,"ok":false,"error":"..."}`. Commands run on the window's own thread, so undo works.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::{Value, json};

/// One request waiting for the window thread: the command and where to send the answer.
pub struct Request {
    pub cmd: Value,
    pub reply: Sender<Result<Value, String>>,
}

const MAX_LINE: u64 = 1 << 20;
const MAX_CONNECTIONS: usize = 8;

/// The port after `--control`, if given.
pub fn port_arg(args: &[std::ffi::OsString]) -> Option<u16> {
    let i = args.iter().position(|a| a.to_str() == Some("--control"))?;
    args.get(i + 1)?.to_str()?.parse().ok()
}

/// Melt 3D opens on SolidCraft's own screen unless a file was given (that is a melt design).
pub fn starts_in_solidcraft(file: Option<&std::path::Path>) -> bool {
    file.is_none()
}

/// The file to open: the first argument that is not an option.
pub fn file_arg(args: &[std::ffi::OsString]) -> Option<std::path::PathBuf> {
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a.to_str() == Some("--control") {
            it.next();
        } else if !a.to_string_lossy().starts_with("--") {
            return Some(std::path::PathBuf::from(a));
        }
    }
    None
}

/// A fresh secret for this launch (or the one in `MELT3D_CONTROL_TOKEN`).
pub fn make_token() -> String {
    if let Ok(t) = std::env::var("MELT3D_CONTROL_TOKEN")
        && t.len() >= 8
    {
        return t;
    }
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for salt in 0..4u32 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u32(salt);
        h.write_u32(std::process::id());
        if let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            h.write_u128(d.as_nanos());
        }
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// A token must never be placed in a shared /tmp directory. Use the
/// operating system's private per-user app-state folder and a stable per-port
/// filename so a separate `meltcad --mcp --control PORT` process can find it.
pub fn token_file(port: u16) -> Result<std::path::PathBuf, String> {
    let state: Option<std::path::PathBuf> = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")).map(std::path::PathBuf::from)
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/state")))
    };
    let state = state.ok_or("Cannot start control mode: no private user directory is available")?;
    Ok(state.join("Melt3D").join("control").join(format!("melt3d-{port}.token")))
}

/// Publish a control token only after the listener has bound its port and after
/// creating the private directory. An entry left by a crashed server is stale:
/// a live server could not have bound the same port. Remove that entry, then
/// exclusively create the replacement without following a pre-existing symlink.
pub fn write_token(port: u16, token: &str) -> Result<std::path::PathBuf, String> {
    let path = token_file(port)?;
    write_token_at(&path, token)?;
    Ok(path)
}

fn write_token_at(path: &std::path::Path, token: &str) -> Result<(), String> {
    secure_token_parent(path)?;
    if let Err(e) = std::fs::remove_file(path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("Cannot remove stale control token: {e}"));
    }
    write_token_to(path, token)
}

fn secure_token_parent(path: &std::path::Path) -> Result<&std::path::Path, String> {
    let parent = path.parent().ok_or("The token path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create control directory: {e}"))?;
    let metadata = std::fs::symlink_metadata(parent).map_err(|e| format!("Cannot inspect control directory: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("The control directory must be a real private directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(|e| format!("Cannot secure control directory: {e}"))?;
    }
    Ok(parent)
}

fn write_token_to(path: &std::path::Path, token: &str) -> Result<(), String> {
    secure_token_parent(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("Cannot exclusively create control token: {e}"))?;
        if let Err(e) = file.write_all(token.as_bytes()).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(path);
            return Err(format!("Cannot publish control token: {e}"));
        }
    }
    #[cfg(not(unix))]
    {
        // Windows LOCALAPPDATA is per-user and carries the user's ACL.
        // create_new refuses any pre-existing file or link.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("Cannot exclusively create control token: {e}"))?;
        if let Err(e) = file.write_all(token.as_bytes()).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(path);
            return Err(format!("Cannot publish control token: {e}"));
        }
    }
    Ok(())
}

/// Start listening. Returns the requests channel and the port really used (0 = pick one).
pub fn start(port: u16, token: String, wake: impl Fn() + Send + Sync + Clone + 'static) -> Result<(Receiver<Request>, u16), String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("Cannot listen on port {port}: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let (tx, rx) = channel::<Request>();
    let live = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if live.load(std::sync::atomic::Ordering::SeqCst) >= MAX_CONNECTIONS {
                continue;
            }
            live.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (tx, token, wake, live) = (tx.clone(), token.clone(), wake.clone(), live.clone());
            std::thread::spawn(move || {
                serve(stream, &token, &tx, &wake);
                live.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            });
        }
    });
    Ok((rx, port))
}

fn serve(stream: TcpStream, token: &str, tx: &Sender<Request>, wake: &impl Fn()) {
    let Ok(mut out) = stream.try_clone() else { return };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(600)));
    let mut reader = BufReader::new(stream);
    let mut authed = false;
    loop {
        let mut buf = Vec::new();
        match reader.by_ref().take(MAX_LINE + 1).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let line = if buf.len() as u64 > MAX_LINE {
            json!({"ok": false, "error": "Request too long"}).to_string()
        } else {
            let text = String::from_utf8_lossy(&buf).into_owned();
            handle_line(&text, token, &mut authed, |cmd| {
                let (reply, answer) = channel();
                tx.send(Request { cmd: cmd.clone(), reply }).map_err(|_| "The window is closed".to_string())?;
                wake();
                answer.recv_timeout(Duration::from_secs(60)).map_err(|_| "The window did not answer in time".to_string())?
            })
        };
        if writeln!(out, "{line}").is_err() || buf.len() as u64 > MAX_LINE {
            return;
        }
    }
}

/// One request line to one answer line. `run` executes a command (on the window thread).
pub fn handle_line(line: &str, token: &str, authed: &mut bool, run: impl FnOnce(&Value) -> Result<Value, String>) -> String {
    let mut v: Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(e) => return json!({"ok": false, "error": format!("Not JSON: {e}")}).to_string(),
    };
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let reply = |r: Result<Value, String>| match r {
        Ok(result) => json!({"id": id, "ok": true, "result": result}).to_string(),
        Err(error) => json!({"id": id, "ok": false, "error": error}).to_string(),
    };
    if !*authed {
        let given = v.get("token").and_then(Value::as_str).unwrap_or("");
        if !same(given, token) {
            return reply(Err("Missing or wrong \"token\" in the first request".into()));
        }
        *authed = true;
    }
    if let Some(o) = v.as_object_mut() {
        o.remove("token");
        o.remove("id");
    }
    reply(run(&v))
}

/// Compares without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scene;

    #[test]
    fn the_first_request_needs_the_token() {
        let mut sc = Scene::default();
        let mut authed = false;
        let bad = handle_line(r#"{"id":1,"cmd":"scene.summary"}"#, "secret-token", &mut authed, |c| crate::automation::execute(&mut sc, c));
        assert!(bad.contains("\"ok\":false") && !authed);
        let ok = handle_line(r#"{"id":2,"token":"secret-token","cmd":"scene.summary"}"#, "secret-token", &mut authed, |c| {
            crate::automation::execute(&mut sc, c)
        });
        assert!(ok.contains("\"ok\":true") && ok.contains("\"id\":2") && authed);
        let junk = handle_line("not json", "secret-token", &mut authed, |_| Ok(Value::Null));
        assert!(junk.contains("Not JSON"));
    }

    #[test]
    fn a_real_connection_runs_commands_on_the_scene_thread() {
        let (rx, port) = start(0, "tok-12345678".into(), || {}).expect("listen");
        std::thread::spawn(move || {
            let mut sc = Scene { groups: Vec::new(), ..Scene::default() };
            while let Ok(r) = rx.recv() {
                let _ = r.reply.send(crate::automation::execute(&mut sc, &r.cmd));
            }
        });
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(10))).ok();
        writeln!(s, r#"{{"id":1,"token":"tok-12345678","cmd":"shape.add","kind":"Sphere"}}"#).ok();
        writeln!(s, r#"{{"id":2,"cmd":"scene.summary"}}"#).ok();
        writeln!(s, r#"{{"id":3,"cmd":"nope"}}"#).ok();
        let mut r = BufReader::new(s);
        let mut lines = Vec::new();
        for _ in 0..3 {
            let mut l = String::new();
            r.read_line(&mut l).ok();
            lines.push(l);
        }
        assert!(lines[0].contains("\"ok\":true"), "{lines:?}");
        assert!(lines[1].contains("\"shape_count\":1"), "{lines:?}");
        assert!(lines[2].contains("\"ok\":false"), "{lines:?}");
    }

    #[test]
    fn control_token_path_is_discoverable_from_a_separate_client() {
        let path = token_file(7777).unwrap();
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("melt3d-7777.token"));
    }

    #[test]
    fn publishing_replaces_a_stale_launch_token() {
        let folder = std::env::temp_dir().join(format!("melt3d-stale-token-test-{}", std::process::id()));
        let path = folder.join("control").join("melt3d-7777.token");
        write_token_to(&path, "old-token").unwrap();
        write_token_at(&path, "new-token").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new-token");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn control_token_is_exclusive_and_user_private() {
        let folder = std::env::temp_dir().join(format!("melt3d-token-test-{}", std::process::id()));
        let path = folder.join("control").join("test.token");
        write_token_to(&path, "secret-1234").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret-1234");
        assert!(write_token_to(&path, "replaced").is_err(), "do not overwrite an existing token");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "secret-1234");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777;
            let file_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
            assert_eq!(file_mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_file_argument_skips_options() {
        let a = |v: &[&str]| v.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert_eq!(port_arg(&a(&["m", "--control", "7777"])), Some(7777));
        assert_eq!(file_arg(&a(&["m", "--control", "7777", "x.yura"])), Some("x.yura".into()));
        assert_eq!(file_arg(&a(&["m", "--control", "7777"])), None);
        assert_eq!(port_arg(&a(&["m", "--control", "abc"])), None);
    }
}
