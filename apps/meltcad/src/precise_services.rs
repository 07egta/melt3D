//! Native host services for the embedded SolidCraft v0.20 UI.
//!
//! The implementation follows the SolidCraft desktop host (`apps/solidcraft/src/main.rs`),
//! adapted to the Melt 3D process and its parented file dialog. SolidCraft updater downloads are
//! intentionally not exposed here: an updater package would replace the Melt 3D host binary.

use std::path::{Path, PathBuf};

use rfd::AsyncFileDialog;
use solidcraft_engine::io;
use solidcraft_ui_egui::Services;

/// The graphics preferences read before Melt 3D creates its shared WGPU device.
pub fn graphics_path() -> Option<PathBuf> {
    let dir = eframe::storage_dir("MELTCAD")?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("solidcraft-graphics.json"))
}

/// The shared window honours the standard SolidCraft anti-aliasing preference on restart.
pub fn msaa() -> u16 {
    let value =
        graphics_path().and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    msaa_value(value.as_ref())
}

fn msaa_value(value: Option<&serde_json::Value>) -> u16 {
    value
        .and_then(|v| v.get("msaa"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| [1, 2, 4, 8].contains(v))
        .unwrap_or(4)
}

/// Build the native services supplied to the embedded SolidCraft UI.
pub fn services(parent: AsyncFileDialog) -> Services {
    let open_parent = parent.clone();
    let program_parent = parent.clone();
    let file_parent = parent.clone();
    Services {
        pick_open: Some(Box::new(move |first: &[&str]| {
            // The file type the command reads comes first, so the picker opens on it.
            let mut dialog = open_parent.clone();
            if !first.is_empty() {
                let label = first.iter().map(|e| e.to_uppercase()).collect::<Vec<_>>().join(", ");
                let both: Vec<String> = first.iter().flat_map(|e| [e.to_lowercase(), e.to_uppercase()]).collect();
                dialog = dialog.add_filter(label, &both);
            }
            let future = dialog
                .add_filter(
                    "Designs, CAD files and meshes",
                    &[
                        io::DESIGN_EXT,
                        io::LEGACY_DESIGN_EXT,
                        "json",
                        "step",
                        "stp",
                        "STEP",
                        "STP",
                        "igs",
                        "iges",
                        "IGS",
                        "IGES",
                        "FCStd",
                        "fcstd",
                        "3dm",
                        "3DM",
                        "3mf",
                        "3MF",
                        "stl",
                        "STL",
                        "obj",
                        "OBJ",
                        "glb",
                        "GLB",
                        "gltf",
                        "GLTF",
                    ],
                )
                .add_filter("SolidCraft design (container or JSON)", &[io::DESIGN_EXT, io::LEGACY_DESIGN_EXT, "json"])
                .add_filter("STEP", &["step", "stp", "STEP", "STP"])
                .add_filter("IGES", &["igs", "iges", "IGS", "IGES"])
                .add_filter("FreeCAD and Rhino", &["FCStd", "fcstd", "3dm", "3DM"])
                .add_filter("Mesh (3MF, STL, OBJ, glTF)", &["3mf", "3MF", "stl", "STL", "obj", "OBJ", "glb", "GLB", "gltf", "GLTF"])
                .add_filter("Sketch drawings (DXF, SVG)", &["dxf", "DXF", "svg", "SVG"])
                .add_filter("All files", &["*"])
                .pick_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        // One kind of file (the agent chat's attachments): macOS only offers the types named.
        pick_file: Some(Box::new(move |label: &str, exts: &[&str]| {
            let mut all: Vec<String> = Vec::new();
            for e in exts {
                for v in [e.to_string(), e.to_ascii_lowercase(), e.to_ascii_uppercase()] {
                    if !all.contains(&v) {
                        all.push(v);
                    }
                }
            }
            let all: Vec<&str> = all.iter().map(String::as_str).collect();
            let future = file_parent.clone().add_filter(label, &all).add_filter("All files", &["*"]).pick_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        pick_program: Some(Box::new(move || {
            let dialog = program_parent.clone().set_title("Choose your slicer");
            let dialog = if cfg!(target_os = "windows") { dialog.add_filter("Programs", &["exe"]) } else { dialog };
            let future = dialog.pick_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        pick_save: Some(Box::new(move |name: &str, exts: &[&str]| {
            let dialog = parent.clone().set_file_name(name);
            let dialog = if exts == [io::DESIGN_EXT, "json"] {
                if name.ends_with(".json") {
                    dialog.add_filter("Plain JSON design", &["json"]).add_filter("SolidCraft container", &[io::DESIGN_EXT])
                } else {
                    dialog.add_filter("SolidCraft container", &[io::DESIGN_EXT]).add_filter("Plain JSON design", &["json"])
                }
            } else {
                dialog.add_filter("File", exts)
            };
            let future = dialog.save_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        show_in_folder: Some(Box::new(show_in_folder)),
        // Offscreen verification uses its own device/sample count, not the user's saved device.
        graphics_path: if cfg!(test) { None } else { graphics_path().map(|p| p.to_string_lossy().into_owned()) },
        agent_http: Some(std::sync::Arc::new(agent_http)),
        secrets: Some(std::sync::Arc::new(Keychain)),
        // An updater would install standalone SolidCraft over the Melt 3D host.
        http_get: None,
        http_download: None,
        ..Default::default()
    }
}

fn show_in_folder(path: &str) {
    let folder = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
    let mut command = std::process::Command::new(if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    });
    command.arg(folder);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let _ = command.spawn();
}

/// HTTP for the agent chat's OpenAI-compatible backends, including local Ollama/LM Studio URLs.
/// Redirects are disabled so an API key cannot be forwarded to another host.
fn agent_http(r: &solidcraft_ui_egui::agent_chat::backend::HttpRequest) -> Result<(u16, Vec<u8>), String> {
    use std::time::Duration;
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .unversioned_rustls_crypto_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(r.timeout_s.clamp(1, 900))))
        .timeout_connect(Some(Duration::from_secs(10)))
        .user_agent("SolidCraft")
        .max_redirects(0)
        .build()
        .into();
    let mut response = if r.method == "POST" {
        let mut req = agent.post(&r.url);
        for (k, v) in &r.headers {
            req = req.header(k, v);
        }
        req.send(r.body.as_deref().unwrap_or_default())
    } else {
        let mut req = agent.get(&r.url);
        for (k, v) in &r.headers {
            req = req.header(k, v);
        }
        req.call()
    }
    .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response.body_mut().with_config().limit(16 << 20).read_to_vec().map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// API keys for embedded agent configurations, stored in the operating system keychain.
struct Keychain;

impl Keychain {
    fn entry(id: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new("SolidCraft agent chat", id).map_err(|e| e.to_string())
    }
}

impl solidcraft_ui_egui::agent_chat::backend::Secrets for Keychain {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match Self::entry(id)?.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        Self::entry(id)?.set_password(secret).map_err(|e| e.to_string())
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn graphics_samples_accept_only_supported_counts() {
        use serde_json::json;
        assert_eq!(super::msaa_value(None), 4);
        for n in [1, 2, 4, 8] {
            assert_eq!(super::msaa_value(Some(&json!({"msaa": n}))), n);
        }
        for n in [0_u64, 3, 16, 65540, u64::MAX] {
            assert_eq!(super::msaa_value(Some(&json!({"msaa": n}))), 4);
        }
    }
}
