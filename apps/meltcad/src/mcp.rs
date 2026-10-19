//! `meltcad --mcp [--control PORT]`: an MCP server on stdin/stdout, so an AI app (Claude, ...)
//! can model with Melt 3D. Without `--control` it works on its own scene in memory (no window);
//! with `--control PORT` it forwards to the window that was started with `meltcad --control PORT`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use serde_json::{Value, json};

use crate::model::Scene;

enum Backend {
    Local(Box<Scene>),
    Remote { reader: BufReader<TcpStream>, writer: TcpStream, token: String },
}

impl Backend {
    fn run(&mut self, cmd: &Value) -> Result<Value, String> {
        match self {
            Backend::Local(scene) => crate::automation::execute(scene, cmd),
            Backend::Remote { reader, writer, token } => {
                let mut c = cmd.clone();
                if let Some(o) = c.as_object_mut() {
                    o.insert("token".into(), json!(token));
                    o.insert("id".into(), json!(1));
                }
                writeln!(writer, "{c}").map_err(|e| format!("Lost the window: {e}"))?;
                let mut line = String::new();
                reader.read_line(&mut line).map_err(|e| format!("Lost the window: {e}"))?;
                let v: Value = serde_json::from_str(&line).map_err(|e| format!("Bad answer: {e}"))?;
                if v.get("ok").and_then(Value::as_bool) == Some(true) {
                    Ok(v.get("result").cloned().unwrap_or(Value::Null))
                } else {
                    Err(v.get("error").and_then(Value::as_str).unwrap_or("Failed").to_string())
                }
            }
        }
    }
}

fn tools() -> Value {
    let list: String = crate::automation::COMMANDS.iter().map(|(n, d)| format!("- {n}: {d}\n")).collect();
    json!([
        {
            "name": "melt3d_run",
            "description": format!("Run Melt 3D commands in order and return each answer. Units are mm, Z is up. Shapes melt together smoothly (SDF modelling). Commands:\n{list}Example: [{{\"cmd\":\"shape.add\",\"kind\":\"Sphere\",\"pos\":[0,0,30],\"size\":[20,20,20]}}]"),
            "inputSchema": {
                "type": "object",
                "properties": {"commands": {"type": "array", "items": {"type": "object"}, "description": "Command objects, each with a \"cmd\" field"}},
                "required": ["commands"]
            }
        },
        {
            "name": "melt3d_view",
            "description": "Draw the scene (four views) as a picture so you can check your work.",
            "inputSchema": {"type": "object", "properties": {"size": {"type": "integer", "description": "Pixels per view (40-600, default 220)"}}}
        }
    ])
}

fn call(backend: &mut Backend, name: &str, args: &Value) -> Value {
    let text = |s: String, err: bool| json!({"content": [{"type": "text", "text": s}], "isError": err});
    match name {
        "melt3d_run" => {
            let Some(cmds) = args.get("commands").and_then(Value::as_array) else {
                return text("\"commands\" must be an array of command objects".into(), true);
            };
            let mut out = Vec::new();
            let mut failed = false;
            for c in cmds.iter().take(500) {
                match backend.run(c) {
                    Ok(v) => out.push(json!({"ok": true, "result": v})),
                    Err(e) => {
                        failed = true;
                        out.push(json!({"ok": false, "error": e}));
                    }
                }
            }
            text(Value::Array(out).to_string(), failed)
        }
        "melt3d_view" => {
            let path = std::env::temp_dir().join(format!("melt3d-view-{}.png", std::process::id()));
            let size = args.get("size").and_then(Value::as_u64).unwrap_or(220);
            let r = backend.run(&json!({"cmd": "render", "path": path.to_string_lossy(), "size": size}));
            let bytes = r.and_then(|_| std::fs::read(&path).map_err(|e| e.to_string()));
            let _ = std::fs::remove_file(&path);
            match bytes {
                Ok(b) => json!({"content": [{"type": "image", "data": base64(&b), "mimeType": "image/png"}]}),
                Err(e) => text(e, true),
            }
        }
        other => text(format!("Unknown tool {other}"), true),
    }
}

/// One JSON-RPC message in, the answer (if any) out.
fn handle(backend: &mut Backend, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone(); // no id: a notification, no answer
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2024-11-05"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "melt3d", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Melt 3D is a light SDF modeller. Use melt3d_run to build, melt3d_view to look."
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            Ok(call(backend, name, params.get("arguments").unwrap_or(&Value::Null)))
        }
        _ => Err(json!({"code": -32601, "message": format!("Method not found: {method}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

pub fn cli(args: &[std::ffi::OsString]) -> bool {
    if !args.iter().any(|a| a.to_str() == Some("--mcp")) {
        return false;
    }
    let mut backend = match crate::control::port_arg(args) {
        None => Backend::Local(Box::new(Scene { groups: Vec::new(), ..Scene::default() })),
        Some(port) => {
            let token = std::env::var("MELT3D_CONTROL_TOKEN")
                .ok()
                .or_else(|| std::fs::read_to_string(crate::control::token_file(port)).ok().map(|t| t.trim().to_string()))
                .unwrap_or_default();
            let conn = TcpStream::connect(("127.0.0.1", port)).and_then(|w| w.try_clone().map(|r| (r, w)));
            match conn {
                Ok((r, writer)) => Backend::Remote { reader: BufReader::new(r), writer, token },
                Err(e) => {
                    eprintln!("Cannot reach the Melt 3D window on port {port}: {e}. Start it with: meltcad --control {port}");
                    return true;
                }
            }
        }
    };
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(reply) = handle(&mut backend, &msg) {
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
    true
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> Backend {
        Backend::Local(Box::new(Scene { groups: Vec::new(), ..Scene::default() }))
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_server_answers_the_mcp_calls() {
        let mut b = local();
        let init = handle(&mut b, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}));
        assert_eq!(init.as_ref().and_then(|v| v["result"]["protocolVersion"].as_str()), Some("2025-03-26"));
        assert!(handle(&mut b, &json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let list = handle(&mut b, &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        assert_eq!(list.as_ref().and_then(|v| v["result"]["tools"].as_array()).map(Vec::len), Some(2));
        let run = handle(&mut b, &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"melt3d_run","arguments":{"commands":[{"cmd":"shape.add","kind":"Sphere"},{"cmd":"scene.summary"}]}}}));
        let t = run.as_ref().and_then(|v| v["result"]["content"][0]["text"].as_str()).unwrap_or("");
        assert!(t.contains("\"shape_count\":1"), "{t}");
        let bad = handle(&mut b, &json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"melt3d_run","arguments":{}}}));
        assert_eq!(bad.as_ref().and_then(|v| v["result"]["isError"].as_bool()), Some(true));
        let view = handle(&mut b, &json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"melt3d_view","arguments":{"size":40}}}));
        assert_eq!(view.as_ref().and_then(|v| v["result"]["content"][0]["type"].as_str()), Some("image"));
        let unk = handle(&mut b, &json!({"jsonrpc":"2.0","id":6,"method":"nope"}));
        assert!(unk.is_some_and(|v| v.get("error").is_some()));
    }
}
