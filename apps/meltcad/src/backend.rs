//! Which graphics API (backend) the window uses, and how it falls back.
//!
//! On Windows the order is Vulkan, then DirectX 12, then OpenGL: DirectX 12 compiles shaders with
//! FXC unless a `dxcompiler.dll` is shipped, and FXC is slow and fragile for the big scene shader.
//! `MELTCAD_BACKEND=vulkan|dx12|gl|metal|auto` forces one. A small settings file remembers the
//! backend that worked last and the ones that failed, so a broken one is not tried first again.
//! If a backend cannot even make a device, the program starts itself again with the next one
//! (winit allows only one event loop per process, so it cannot be done inside the process).
//! On Linux and macOS `auto` means the defaults of egui-wgpu (Vulkan or Metal, with OpenGL behind).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    #[serde(rename = "vulkan")]
    Vulkan,
    #[serde(rename = "dx12")]
    Dx12,
    #[serde(rename = "gl")]
    Gl,
    #[serde(rename = "metal")]
    Metal,
}

impl Backend {
    #[cfg(test)]
    pub const ALL: [Backend; 4] = [Backend::Vulkan, Backend::Dx12, Backend::Gl, Backend::Metal];

    /// The word used in `MELTCAD_BACKEND` and in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Vulkan => "vulkan",
            Backend::Dx12 => "dx12",
            Backend::Gl => "gl",
            Backend::Metal => "metal",
        }
    }

    /// The name shown to people.
    pub fn label(self) -> &'static str {
        match self {
            Backend::Vulkan => "Vulkan",
            Backend::Dx12 => "DirectX 12",
            Backend::Gl => "OpenGL",
            Backend::Metal => "Metal",
        }
    }

    pub fn parse(text: &str) -> Option<Backend> {
        match text.trim().to_ascii_lowercase().as_str() {
            "vulkan" | "vk" => Some(Backend::Vulkan),
            "dx12" | "d3d12" | "directx12" | "directx" | "dx" => Some(Backend::Dx12),
            "gl" | "opengl" | "gles" | "opengles" => Some(Backend::Gl),
            "metal" | "mtl" => Some(Backend::Metal),
            _ => None,
        }
    }

    pub fn to_wgpu(self) -> egui_wgpu::wgpu::Backends {
        use egui_wgpu::wgpu::Backends;
        match self {
            Backend::Vulkan => Backends::VULKAN,
            Backend::Dx12 => Backends::DX12,
            Backend::Gl => Backends::GL,
            Backend::Metal => Backends::METAL,
        }
    }

    pub fn from_wgpu(b: egui_wgpu::wgpu::Backend) -> Option<Backend> {
        use egui_wgpu::wgpu::Backend as B;
        match b {
            B::Vulkan => Some(Backend::Vulkan),
            B::Dx12 => Some(Backend::Dx12),
            B::Gl => Some(Backend::Gl),
            B::Metal => Some(Backend::Metal),
            _ => None,
        }
    }

    /// The backends that make sense to offer on this platform, in menu order.
    pub fn offered(windows: bool, macos: bool) -> Vec<Backend> {
        if windows {
            vec![Backend::Vulkan, Backend::Dx12, Backend::Gl]
        } else if macos {
            vec![Backend::Metal, Backend::Gl]
        } else {
            vec![Backend::Vulkan, Backend::Gl]
        }
    }
}

/// What `MELTCAD_BACKEND` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Auto,
    Fixed(Backend),
}

/// Read the value of `MELTCAD_BACKEND` (`None` = not set = auto). An unknown word is an error
/// message (the program then goes on with auto and says so in the log).
pub fn parse_choice(text: Option<&str>) -> Result<Choice, String> {
    match text.map(str::trim) {
        None | Some("") => Ok(Choice::Auto),
        Some(t) if t.eq_ignore_ascii_case("auto") => Ok(Choice::Auto),
        Some(t) => Backend::parse(t).map(Choice::Fixed).ok_or_else(|| format!("MELTCAD_BACKEND={t} は知らない値です (vulkan / dx12 / gl / metal / auto)")),
    }
}

/// What is remembered between runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The backend that made a working picture last time.
    pub last_ok: Option<Backend>,
    /// Backends that failed (no device, or the shader did not compile) since.
    pub failed: Vec<Backend>,
    /// The person chose the simple view ("かんたん表示").
    pub easy: bool,
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// This backend worked: it is first next time and no longer counts as failed.
    pub fn note_ok(&mut self, b: Backend) {
        self.last_ok = Some(b);
        self.failed.retain(|f| *f != b);
    }

    /// This backend failed: it goes to the back of the line.
    pub fn note_failed(&mut self, b: Backend) {
        if self.last_ok == Some(b) {
            self.last_ok = None;
        }
        if !self.failed.contains(&b) {
            self.failed.push(b);
        }
    }
}

/// What the window is created with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// The defaults of egui-wgpu (all the usual backends; the best one wins).
    Default,
    /// Only this backend.
    Only(Backend),
}

/// The order `auto` tries backends in on a platform: Vulkan, DirectX 12, OpenGL on Windows; nothing
/// special elsewhere (empty = the defaults).
pub fn auto_order(windows: bool) -> Vec<Backend> {
    if windows { vec![Backend::Vulkan, Backend::Dx12, Backend::Gl] } else { Vec::new() }
}

/// What to start with. `tried` are the backends earlier runs of this launch already failed to
/// start (they are skipped). `None` means nothing is left to try.
pub fn plan(choice: Choice, windows: bool, settings: &Settings, tried: &[Backend]) -> Option<Plan> {
    if let Choice::Fixed(b) = choice {
        return Some(Plan::Only(b));
    }
    let order = auto_order(windows);
    if order.is_empty() {
        // (the defaults have no order to walk through; they are tried once)
        return if tried.is_empty() { Some(Plan::Default) } else { None };
    }
    // the one that worked last first, the ones that failed last, the rest as they are
    let mut ranked: Vec<Backend> = Vec::new();
    if let Some(ok) = settings.last_ok.filter(|b| order.contains(b)) {
        ranked.push(ok);
    }
    for pass in 0..2 {
        for b in &order {
            if !ranked.contains(b) && (pass == 1 || !settings.failed.contains(b)) {
                ranked.push(*b);
            }
        }
    }
    ranked.into_iter().find(|b| !tried.contains(b)).map(Plan::Only)
}

/// After `current` could not start: what to start next. An explicit `MELTCAD_BACKEND` is never
/// replaced (None), and neither is anything once every backend has been tried.
pub fn next_after_failure(choice: Choice, windows: bool, settings: &Settings, tried: &[Backend], current: Plan) -> Option<Plan> {
    if matches!(choice, Choice::Fixed(_)) {
        return None;
    }
    let mut all = tried.to_vec();
    match current {
        Plan::Only(b) if !all.contains(&b) => all.push(b),
        Plan::Only(_) => {}
        // (the defaults are one try: any entry stands for it)
        Plan::Default => all.push(Backend::Vulkan),
    }
    plan(Choice::Auto, windows, settings, &all)
}

/// "vulkan,dx12" <-> the backends tried so far (`MELTCAD_BACKEND_TRIED`).
pub fn parse_tried(text: Option<&str>) -> Vec<Backend> {
    let mut out: Vec<Backend> = Vec::new();
    for part in text.unwrap_or("").split(',') {
        if let Some(b) = Backend::parse(part)
            && !out.contains(&b)
        {
            out.push(b);
        }
    }
    out
}

pub fn tried_text(tried: &[Backend]) -> String {
    tried.iter().map(|b| b.name()).collect::<Vec<_>>().join(",")
}

/// The window's wgpu setup for a plan.
pub fn wgpu_configuration(plan: Plan) -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    if let Plan::Only(b) = plan {
        setup.instance_descriptor.backends = b.to_wgpu();
    }
    // timestamp queries (when the adapter has them) let the viewport time its own scene passes
    // and choose the picture size from the real cost (see `gpu_timing.rs`)
    let usual = setup.device_descriptor.clone();
    setup.device_descriptor = std::sync::Arc::new(move |adapter| {
        let mut d = usual(adapter);
        d.required_features |= adapter.features() & egui_wgpu::wgpu::Features::TIMESTAMP_QUERY;
        d
    });
    egui_wgpu::WgpuConfiguration { wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup), ..Default::default() }
}

// ---------------------------------------------------------------------------------------------
// The settings file

/// The folder for MELTCAD's small settings: `%APPDATA%` on Windows, `~/Library/Application Support`
/// on macOS, `$XDG_CONFIG_HOME` or `~/.config` elsewhere.
pub fn config_dir_from(os: &str, var: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let base = match os {
        "windows" => var("APPDATA").map(PathBuf::from),
        "macos" => var("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support")),
        _ => var("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".config"))),
    }?;
    Some(base.join("meltcad"))
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir_from(std::env::consts::OS, &|k| std::env::var_os(k)).map(|d| d.join("settings.json"))
}

pub fn load_from(path: &Path) -> Settings {
    std::fs::read_to_string(path).map(|t| Settings::parse(&t)).unwrap_or_default()
}

pub fn save_to(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, settings.to_text())?;
    std::fs::rename(&tmp, path)
}

pub fn load() -> Settings {
    settings_path().map(|p| load_from(&p)).unwrap_or_default()
}

/// Change the settings and save them (a failure is only written to the log).
pub fn update(change: impl FnOnce(&mut Settings)) {
    let Some(path) = settings_path() else { return };
    let mut s = load_from(&path);
    let before = s.clone();
    change(&mut s);
    if s != before
        && let Err(e) = save_to(&path, &s)
    {
        crate::gpu_log::log(&format!("settings: cannot write {}: {e}", path.display()));
    }
}

/// Remember that `b` made a working picture (`ok`) or failed.
pub fn remember(b: Backend, ok: bool) {
    update(|s| if ok { s.note_ok(b) } else { s.note_failed(b) });
}

// ---------------------------------------------------------------------------------------------
// Starting again

/// How to start the program again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relaunch {
    /// `MELTCAD_BACKEND` for the new run (None = leave the choice to auto).
    pub choice: Option<String>,
    /// Backends that are not to be tried again (`MELTCAD_BACKEND_TRIED`).
    pub tried: Vec<Backend>,
    /// The design to open.
    pub file: Option<PathBuf>,
}

/// The command line and environment of the new run.
pub fn relaunch_args(r: &Relaunch) -> (Vec<OsString>, Vec<(String, String)>) {
    let args: Vec<OsString> = r.file.iter().map(|f| f.clone().into_os_string()).collect();
    let mut envs = Vec::new();
    envs.push(("MELTCAD_BACKEND".to_string(), r.choice.clone().unwrap_or_default()));
    envs.push(("MELTCAD_BACKEND_TRIED".to_string(), tried_text(&r.tried)));
    (args, envs)
}

/// Start this program again. The caller then closes its own window.
pub fn relaunch(r: &Relaunch) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("プログラムの場所がわかりません: {e}"))?;
    let (args, envs) = relaunch_args(r);
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(args);
    for (k, v) in envs {
        if v.is_empty() {
            cmd.env_remove(k);
        } else {
            cmd.env(k, v);
        }
    }
    crate::gpu_log::log(&format!("restarting {} with {r:?}", exe.display()));
    cmd.spawn().map(|_| ()).map_err(|e| format!("もういちど起動できませんでした: {e}"))
}

/// The backends tried before this run, from the environment.
pub fn tried_now() -> Vec<Backend> {
    parse_tried(std::env::var("MELTCAD_BACKEND_TRIED").ok().as_deref())
}

/// What `MELTCAD_BACKEND` says now.
pub fn choice_now() -> (Choice, Option<String>) {
    let text = std::env::var("MELTCAD_BACKEND").ok();
    match parse_choice(text.as_deref()) {
        Ok(c) => (c, None),
        Err(e) => (Choice::Auto, Some(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_words_are_understood() {
        for (text, want) in [
            ("vulkan", Backend::Vulkan),
            ("VK", Backend::Vulkan),
            ("dx12", Backend::Dx12),
            ("D3D12", Backend::Dx12),
            ("DirectX12", Backend::Dx12),
            (" gl ", Backend::Gl),
            ("OpenGL", Backend::Gl),
            ("metal", Backend::Metal),
        ] {
            assert_eq!(Backend::parse(text), Some(want), "{text}");
        }
        assert_eq!(Backend::parse("auto"), None);
        assert_eq!(Backend::parse(""), None);
        for b in Backend::ALL {
            assert_eq!(Backend::parse(b.name()), Some(b), "names round-trip");
            assert!(!b.label().is_empty());
        }
    }

    #[test]
    fn the_environment_variable_picks_a_choice() {
        assert_eq!(parse_choice(None), Ok(Choice::Auto));
        assert_eq!(parse_choice(Some("")), Ok(Choice::Auto));
        assert_eq!(parse_choice(Some("AUTO")), Ok(Choice::Auto));
        assert_eq!(parse_choice(Some("dx12")), Ok(Choice::Fixed(Backend::Dx12)));
        assert_eq!(parse_choice(Some("vulkan")), Ok(Choice::Fixed(Backend::Vulkan)));
        let err = parse_choice(Some("directfoo")).unwrap_err();
        assert!(err.contains("directfoo") && err.contains("vulkan"), "{err}");
    }

    #[test]
    fn windows_tries_vulkan_then_dx12_then_gl() {
        let s = Settings::default();
        assert_eq!(plan(Choice::Auto, true, &s, &[]), Some(Plan::Only(Backend::Vulkan)));
        assert_eq!(plan(Choice::Auto, true, &s, &[Backend::Vulkan]), Some(Plan::Only(Backend::Dx12)));
        assert_eq!(plan(Choice::Auto, true, &s, &[Backend::Vulkan, Backend::Dx12]), Some(Plan::Only(Backend::Gl)));
        assert_eq!(plan(Choice::Auto, true, &s, &[Backend::Vulkan, Backend::Dx12, Backend::Gl]), None, "nothing left");
    }

    #[test]
    fn other_systems_use_the_defaults_once() {
        let s = Settings::default();
        assert_eq!(plan(Choice::Auto, false, &s, &[]), Some(Plan::Default));
        assert_eq!(plan(Choice::Auto, false, &s, &[Backend::Vulkan]), None);
        assert_eq!(auto_order(false), Vec::<Backend>::new());
    }

    #[test]
    fn an_explicit_choice_wins_everywhere_and_ignores_the_memory() {
        let s = Settings { last_ok: Some(Backend::Gl), failed: vec![Backend::Dx12], easy: false };
        assert_eq!(plan(Choice::Fixed(Backend::Dx12), true, &s, &[]), Some(Plan::Only(Backend::Dx12)));
        assert_eq!(plan(Choice::Fixed(Backend::Gl), false, &s, &[]), Some(Plan::Only(Backend::Gl)));
        assert_eq!(plan(Choice::Fixed(Backend::Vulkan), true, &s, &[Backend::Vulkan]), Some(Plan::Only(Backend::Vulkan)), "it was asked for");
    }

    #[test]
    fn the_memory_reorders_auto() {
        let mut s = Settings::default();
        s.note_failed(Backend::Vulkan);
        assert_eq!(plan(Choice::Auto, true, &s, &[]), Some(Plan::Only(Backend::Dx12)), "a failed backend is not first");
        s.note_ok(Backend::Gl);
        assert_eq!(plan(Choice::Auto, true, &s, &[]), Some(Plan::Only(Backend::Gl)), "the last good one is first");
        assert_eq!(plan(Choice::Auto, true, &s, &[Backend::Gl]), Some(Plan::Only(Backend::Dx12)));
        assert_eq!(plan(Choice::Auto, true, &s, &[Backend::Gl, Backend::Dx12]), Some(Plan::Only(Backend::Vulkan)), "failed ones still get their turn at the end");
    }

    #[test]
    fn noting_results_updates_the_lists() {
        let mut s = Settings::default();
        s.note_ok(Backend::Vulkan);
        assert_eq!(s.last_ok, Some(Backend::Vulkan));
        s.note_failed(Backend::Vulkan);
        assert_eq!((s.last_ok, s.failed.clone()), (None, vec![Backend::Vulkan]));
        s.note_failed(Backend::Vulkan);
        assert_eq!(s.failed, vec![Backend::Vulkan], "no duplicates");
        s.note_ok(Backend::Vulkan);
        assert_eq!((s.last_ok, s.failed.len()), (Some(Backend::Vulkan), 0), "working again clears the failure");
    }

    #[test]
    fn settings_round_trip_and_survive_garbage() {
        let s = Settings { last_ok: Some(Backend::Dx12), failed: vec![Backend::Vulkan, Backend::Gl], easy: true };
        let text = s.to_text();
        assert!(text.contains("\"dx12\"") && text.contains("\"vulkan\""), "{text}");
        assert_eq!(Settings::parse(&text), s);
        assert_eq!(Settings::parse("not json at all"), Settings::default());
        assert_eq!(Settings::parse("{}"), Settings::default());
        assert_eq!(Settings::parse("{\"last_ok\":\"nonsense\"}"), Settings::default(), "an unknown backend word is dropped with the file");
        assert_eq!(Settings::parse("{\"easy\":true,\"future_field\":1}").easy, true);
    }

    #[test]
    fn settings_are_saved_and_loaded_through_a_file() {
        let dir = std::env::temp_dir().join(format!("meltcad-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("deep").join("settings.json");
        assert_eq!(load_from(&path), Settings::default(), "no file is the defaults");
        let s = Settings { last_ok: Some(Backend::Gl), failed: vec![], easy: true };
        save_to(&path, &s).expect("saved");
        assert_eq!(load_from(&path), s);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_start_moves_on_but_never_past_an_explicit_choice() {
        let s = Settings::default();
        assert_eq!(next_after_failure(Choice::Auto, true, &s, &[], Plan::Only(Backend::Vulkan)), Some(Plan::Only(Backend::Dx12)));
        assert_eq!(next_after_failure(Choice::Auto, true, &s, &[Backend::Vulkan], Plan::Only(Backend::Dx12)), Some(Plan::Only(Backend::Gl)));
        assert_eq!(next_after_failure(Choice::Auto, true, &s, &[Backend::Vulkan, Backend::Dx12], Plan::Only(Backend::Gl)), None);
        assert_eq!(next_after_failure(Choice::Fixed(Backend::Vulkan), true, &s, &[], Plan::Only(Backend::Vulkan)), None, "no loop on an explicit choice");
        assert_eq!(next_after_failure(Choice::Auto, false, &s, &[], Plan::Default), None, "the defaults are tried once");
    }

    #[test]
    fn the_tried_list_is_read_and_written() {
        assert_eq!(parse_tried(None), vec![]);
        assert_eq!(parse_tried(Some("")), vec![]);
        assert_eq!(parse_tried(Some("vulkan,dx12,vulkan,bogus")), vec![Backend::Vulkan, Backend::Dx12]);
        assert_eq!(tried_text(&[Backend::Vulkan, Backend::Gl]), "vulkan,gl");
        assert_eq!(parse_tried(Some(&tried_text(&[Backend::Dx12, Backend::Gl]))), vec![Backend::Dx12, Backend::Gl]);
    }

    #[test]
    fn the_config_folder_follows_the_platform() {
        let vars = |k: &str| -> Option<OsString> {
            match k {
                "APPDATA" => Some("C:\\Users\\a\\AppData\\Roaming".into()),
                "HOME" => Some("/home/a".into()),
                "XDG_CONFIG_HOME" => Some("/home/a/cfg".into()),
                _ => None,
            }
        };
        assert_eq!(config_dir_from("windows", &vars), Some(PathBuf::from("C:\\Users\\a\\AppData\\Roaming").join("meltcad")));
        assert_eq!(config_dir_from("linux", &vars), Some(PathBuf::from("/home/a/cfg/meltcad")));
        assert_eq!(config_dir_from("macos", &vars), Some(PathBuf::from("/home/a/Library/Application Support/meltcad")));
        let no_xdg = |k: &str| -> Option<OsString> { (k == "HOME").then(|| "/home/a".into()) };
        assert_eq!(config_dir_from("linux", &no_xdg), Some(PathBuf::from("/home/a/.config/meltcad")));
        assert_eq!(config_dir_from("linux", &|_| None), None);
        assert_eq!(config_dir_from("windows", &no_xdg), None);
    }

    #[test]
    fn a_restart_carries_the_choice_the_tried_list_and_the_file() {
        let r = Relaunch { choice: Some("dx12".into()), tried: vec![Backend::Vulkan], file: Some(PathBuf::from("/tmp/作品.meltcad")) };
        let (args, envs) = relaunch_args(&r);
        assert_eq!(args, vec![OsString::from("/tmp/作品.meltcad")]);
        assert!(envs.contains(&("MELTCAD_BACKEND".to_string(), "dx12".to_string())));
        assert!(envs.contains(&("MELTCAD_BACKEND_TRIED".to_string(), "vulkan".to_string())));
        let auto = Relaunch { choice: None, tried: vec![], file: None };
        let (args, envs) = relaunch_args(&auto);
        assert!(args.is_empty());
        assert!(envs.contains(&("MELTCAD_BACKEND".to_string(), String::new())), "empty means: remove it, auto");
    }

    #[test]
    fn the_wgpu_setup_is_limited_to_the_planned_backend() {
        use egui_wgpu::WgpuSetup;
        let only = |p| match wgpu_configuration(p).wgpu_setup {
            WgpuSetup::CreateNew(c) => c.instance_descriptor.backends,
            WgpuSetup::Existing(_) => egui_wgpu::wgpu::Backends::empty(),
        };
        assert_eq!(only(Plan::Only(Backend::Vulkan)), egui_wgpu::wgpu::Backends::VULKAN);
        assert_eq!(only(Plan::Only(Backend::Dx12)), egui_wgpu::wgpu::Backends::DX12);
        assert_eq!(only(Plan::Only(Backend::Gl)), egui_wgpu::wgpu::Backends::GL);
        assert!(only(Plan::Default).contains(egui_wgpu::wgpu::Backends::VULKAN), "the default keeps egui-wgpu's usual set");
    }

    #[test]
    fn the_menu_offers_backends_that_exist_on_the_platform() {
        assert_eq!(Backend::offered(true, false), vec![Backend::Vulkan, Backend::Dx12, Backend::Gl]);
        assert_eq!(Backend::offered(false, true), vec![Backend::Metal, Backend::Gl]);
        assert_eq!(Backend::offered(false, false), vec![Backend::Vulkan, Backend::Gl]);
        assert_eq!(Backend::from_wgpu(egui_wgpu::wgpu::Backend::Dx12), Some(Backend::Dx12));
        assert_eq!(Backend::from_wgpu(egui_wgpu::wgpu::Backend::BrowserWebGpu), None);
    }
}
