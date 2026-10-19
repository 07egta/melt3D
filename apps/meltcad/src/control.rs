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
    if let Ok(t) = std::env::var("MELT3D_CONTROL_TOKEN") {
        if t.len() >= 8 {
            return t;
        }
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

pub fn token_file(port: u16) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("melt3d-control-{port}.token"))
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
        let ok = handle_line(r#"{"id":2,"token":"secret-token","cmd":"scene.summary"}"#, "secret-token", &mut authed, |c| crate::automation::execute(&mut sc, c));
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
    fn the_file_argument_skips_options() {
        let a = |v: &[&str]| v.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert_eq!(port_arg(&a(&["m", "--control", "7777"])), Some(7777));
        assert_eq!(file_arg(&a(&["m", "--control", "7777", "x.yura"])), Some("x.yura".into()));
        assert_eq!(file_arg(&a(&["m", "--control", "7777"])), None);
        assert_eq!(port_arg(&a(&["m", "--control", "abc"])), None);
    }
}
