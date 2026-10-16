//! G-code Nodes: the Grasshopper-style G-code tool (a page that runs in the browser) opened from
//! the app with the current model already loaded into its slicer node.

use serde_json::{Value, json};
use solidcraft_io::Format;

use super::CommandSpec;
use crate::{
    EngineError, Result, Session,
    params::{bad, bool_},
};

pub const COMMANDS: &[CommandSpec] = &[CommandSpec::new("tools.gcode_nodes", "G-code Nodes", gcode_nodes)
    .icon("export")
    .noundo()
    .params("open?: bool (default false: only write the page; the app passes true) → writes the node-based G-code tool with the design's bodies as its STL input and opens it in the default browser. Desktop app only")];

/// The tool's page, with a marker where the model is handed over.
const PAGE: &str = include_str!("../../../../assets/tools/yura-gcode.html");

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(c.get(1).copied().unwrap_or(0)) << 8) | u32::from(c.get(2).copied().unwrap_or(0));
        let at = |s: u32| char::from(T[((n >> s) & 63) as usize]);
        out.push(at(18));
        out.push(at(12));
        out.push(if c.len() > 1 { at(6) } else { '=' });
        out.push(if c.len() > 2 { at(0) } else { '=' });
    }
    out
}

/// The page with the model's STL (when the design has bodies) in place of the marker.
pub fn page_with_model(s: &Session) -> String {
    let stl = solidcraft_io::export_with_metadata(&s.doc.painted(&s.world_state()), &[], Format::StlBinary, &s.doc.name, &s.doc.metadata).ok();
    let hand_over = match stl {
        Some(bytes) => {
            let name: String = s.doc.name.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(60).collect();
            format!("<script>window.__preload={{name:{},b64:\"{}\"}};</script>", json!(format!("{name}.stl")), base64(&bytes))
        }
        None => String::new(),
    };
    PAGE.replacen("<!--PRELOAD-->", &hand_over, 1)
}

fn gcode_nodes(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "tools.gcode_nodes";
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(bad(cmd, "the G-code tool opens from the desktop app only"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = std::env::temp_dir().join("meltcad");
        std::fs::create_dir_all(&dir).map_err(|e| EngineError::Other(format!("{}: {e}", dir.display())))?;
        let path = dir.join("yura-gcode.html");
        std::fs::write(&path, page_with_model(s)).map_err(|e| EngineError::Other(format!("{}: {e}", path.display())))?;
        let has_model = !s.world_state().bodies.is_empty();
        let mut opened = false;
        if bool_(p, "open").unwrap_or(false) {
            opened = open_in_browser(&path).map_err(|e| bad(cmd, e))?;
        }
        Ok(json!({"path": path, "model_loaded": has_model, "opened": opened}))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn open_in_browser(path: &std::path::Path) -> std::result::Result<bool, String> {
    use std::process::{Command, Stdio};
    let file = path.to_string_lossy().to_string();
    let mut c;
    if cfg!(windows) {
        c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", &file]);
    } else if cfg!(target_os = "macos") {
        c = Command::new("open");
        c.arg(&file);
    } else {
        c = Command::new("xdg-open");
        c.arg(&file);
    }
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| true)
        .map_err(|e| format!("couldn't open the browser ({e}); the page is saved at {file}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn the_page_carries_the_model_and_the_command_does_not_open_a_browser_by_default() {
        let mut s = Session::default();
        let empty = super::page_with_model(&s);
        assert!(!empty.contains("__preload=") && empty.contains("YURA-GCODE"));
        s.execute("solid.box", &json!({"length": 10, "width": 10, "height": 10})).unwrap();
        let page = super::page_with_model(&s);
        assert!(page.contains("window.__preload={name:") && page.contains("b64:\""), "the model is handed over");
        assert!(!page.contains("<!--PRELOAD-->"));
        let r = s.execute("tools.gcode_nodes", &json!({})).unwrap();
        assert_eq!(r["opened"], false);
        assert_eq!(r["model_loaded"], true);
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }
}
