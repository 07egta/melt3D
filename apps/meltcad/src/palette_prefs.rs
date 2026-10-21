//! What the search palette remembers between runs: pinned entries and the shortcuts people gave
//! to entries. It lives in `palette.json` next to the settings file. A missing, damaged or
//! hostile file only means "nothing remembered"; nothing here panics.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};

pub const MAX_PINS: usize = 24;
const MAX_KEYS: usize = 200;
const MAX_ID: usize = 120;

/// A key plus modifiers ("Ctrl+Shift+K"). `cmd` is Ctrl (Command on a Mac).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub cmd: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Shortcut {
    #[cfg(test)]
    pub fn new(cmd: bool, shift: bool, alt: bool, key: Key) -> Shortcut {
        Shortcut { cmd, shift, alt, key }
    }

    /// "Ctrl+Shift+K" (any case, `Cmd` and `Command` also mean Ctrl). Exactly one key.
    pub fn parse(text: &str) -> Option<Shortcut> {
        let (mut cmd, mut shift, mut alt, mut key) = (false, false, false, None);
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "command" | "control" => cmd = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                "" => return None,
                _ => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(Key::from_name(part)?);
                }
            }
        }
        Some(Shortcut { cmd, shift, alt, key: key? })
    }

    /// The text stored in the file (round-trips through `parse`).
    pub fn to_text(self) -> String {
        self.text(self.key.name())
    }

    /// The short form shown on the badge ("Ctrl+K", "Ctrl+←").
    pub fn badge(&self) -> String {
        crate::style::keys(&self.text(self.key.symbol_or_name()))
    }

    fn text(&self, key: &str) -> String {
        let mut s = String::new();
        for (on, name) in [(self.cmd, "Ctrl+"), (self.shift, "Shift+"), (self.alt, "Alt+")] {
            if on {
                s.push_str(name);
            }
        }
        s.push_str(key);
        s
    }

    pub fn modifiers(&self) -> Modifiers {
        let mut m = Modifiers::NONE;
        if self.cmd {
            m |= Modifiers::COMMAND;
        }
        if self.shift {
            m |= Modifiers::SHIFT;
        }
        if self.alt {
            m |= Modifiers::ALT;
        }
        m
    }

    /// The shortcut a key press makes. Escape is kept for cancelling.
    pub fn from_press(key: Key, m: Modifiers) -> Option<Shortcut> {
        (key != Key::Escape).then_some(Shortcut { cmd: m.command, shift: m.shift, alt: m.alt, key })
    }
}

/// Shortcuts the app already uses (a custom one may not take them).
const BUILT_IN: &[(&str, &str)] = &[
    ("Ctrl+Z", "Undo"),
    ("Ctrl+Shift+Z", "Redo"),
    ("Ctrl+Y", "Redo"),
    ("Ctrl+C", "Copy"),
    ("Ctrl+X", "Cut"),
    ("Ctrl+V", "Paste"),
    ("Ctrl+S", "Save"),
    ("Ctrl+O", "Open"),
    ("Ctrl+E", "Export"),
    ("Ctrl+D", "Duplicate"),
    ("Ctrl+G", "Group"),
    ("Ctrl+K", "Find"),
    ("Delete", "Delete"),
    ("Backspace", "Delete"),
    ("F1", "Help"),
    ("Shift+Questionmark", "Help"),
    ("Questionmark", "Help"),
    ("F3", "Performance"),
    ("Tab", "Add / Subtract"),
    ("F", "Find"),
    ("Shift+F", "Frame selection"),
    ("Home", "Frame all"),
    ("Enter", "Confirm"),
    ("1", "Front view"),
    ("3", "Right view"),
    ("5", "Perspective"),
    ("7", "Top view"),
    ("Ctrl+1", "Back view"),
    ("Ctrl+3", "Left view"),
    ("Ctrl+7", "Bottom view"),
    ("Alt+ArrowLeft", "Turn view"),
    ("Alt+ArrowRight", "Turn view"),
    ("Alt+ArrowUp", "Turn view"),
    ("Alt+ArrowDown", "Turn view"),
    ("ArrowLeft", "Nudge"),
    ("ArrowRight", "Nudge"),
    ("ArrowUp", "Nudge"),
    ("ArrowDown", "Nudge"),
    ("PageUp", "Nudge"),
    ("PageDown", "Nudge"),
    ("Shift+ArrowLeft", "Nudge"),
    ("Shift+ArrowRight", "Nudge"),
    ("Shift+ArrowUp", "Nudge"),
    ("Shift+ArrowDown", "Nudge"),
    ("Shift+PageUp", "Nudge"),
    ("Shift+PageDown", "Nudge"),
];

/// Why a shortcut cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    BuiltIn(&'static str),
    /// Another entry (its id) already has it.
    Custom(String),
}

fn built_in(sc: &Shortcut) -> Option<&'static str> {
    BUILT_IN.iter().find(|(t, _)| Shortcut::parse(t).as_ref() == Some(sc)).map(|(_, name)| *name)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Entry ids, in the order they were pinned.
    pub pins: Vec<String>,
    /// Entry id -> shortcut text.
    pub keys: BTreeMap<String, String>,
}

impl Prefs {
    pub fn parse(text: &str) -> Prefs {
        let mut p: Prefs = serde_json::from_str(text).unwrap_or_default();
        p.sanitize();
        p
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Drop whatever a damaged or hand-edited file got wrong.
    fn sanitize(&mut self) {
        let ok = |s: &String| !s.is_empty() && s.chars().count() <= MAX_ID;
        let mut seen = std::collections::HashSet::new();
        self.pins.retain(|p| ok(p) && seen.insert(p.clone()));
        self.pins.truncate(MAX_PINS);
        let mut used: Vec<Shortcut> = Vec::new();
        let old = std::mem::take(&mut self.keys);
        for (id, text) in old {
            if self.keys.len() >= MAX_KEYS || !ok(&id) {
                continue;
            }
            let Some(sc) = Shortcut::parse(&text) else { continue };
            if sc.key == Key::Escape || built_in(&sc).is_some() || used.contains(&sc) {
                continue;
            }
            used.push(sc);
            self.keys.insert(id, sc.to_text());
        }
    }

    pub fn is_pinned(&self, id: &str) -> bool {
        self.pins.iter().any(|p| p == id)
    }

    /// Pin or unpin. Returns whether it is pinned now (a full row of pins refuses new ones).
    pub fn toggle_pin(&mut self, id: &str) -> bool {
        if let Some(i) = self.pins.iter().position(|p| p == id) {
            self.pins.remove(i);
            return false;
        }
        if self.pins.len() >= MAX_PINS || id.is_empty() || id.chars().count() > MAX_ID {
            return false;
        }
        self.pins.push(id.to_string());
        true
    }

    pub fn shortcut(&self, id: &str) -> Option<Shortcut> {
        self.keys.get(id).and_then(|t| Shortcut::parse(t))
    }

    /// Everything with a shortcut, to match key presses against.
    pub fn bindings(&self) -> Vec<(Shortcut, &str)> {
        self.keys.iter().filter_map(|(id, t)| Shortcut::parse(t).map(|s| (s, id.as_str()))).collect()
    }

    /// Would `sc` clash with a built-in shortcut or another entry's?
    pub fn conflict(&self, id: &str, sc: &Shortcut) -> Option<Conflict> {
        if let Some(name) = built_in(sc) {
            return Some(Conflict::BuiltIn(name));
        }
        self.bindings().into_iter().find(|(s, other)| s == sc && *other != id).map(|(_, other)| Conflict::Custom(other.to_string()))
    }

    pub fn set_shortcut(&mut self, id: &str, sc: Shortcut) -> Result<(), Conflict> {
        if let Some(c) = self.conflict(id, &sc) {
            return Err(c);
        }
        if sc.key == Key::Escape || id.is_empty() || id.chars().count() > MAX_ID || (self.keys.len() >= MAX_KEYS && !self.keys.contains_key(id)) {
            return Err(Conflict::BuiltIn("Escape"));
        }
        self.keys.insert(id.to_string(), sc.to_text());
        Ok(())
    }

    pub fn clear_shortcut(&mut self, id: &str) {
        self.keys.remove(id);
    }

    // ----- the file -----

    pub fn path() -> Option<PathBuf> {
        crate::backend::settings_path().map(|p| p.with_file_name("palette.json"))
    }

    pub fn load_from(path: &Path) -> Prefs {
        // a huge file is not ours
        match std::fs::metadata(path) {
            Ok(m) if m.len() <= 1 << 20 => std::fs::read_to_string(path).map(|t| Prefs::parse(&t)).unwrap_or_default(),
            _ => Prefs::default(),
        }
    }

    pub fn load() -> Prefs {
        Prefs::path().map(|p| Prefs::load_from(&p)).unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, path)
    }

    /// Save, silently (a failure only means it is not remembered).
    pub fn save(&self) {
        if let Some(p) = Prefs::path() {
            let _ = self.save_to(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(t: &str) -> Shortcut {
        Shortcut::parse(t).unwrap()
    }

    #[test]
    fn shortcuts_parse_and_format() {
        let s = sc("ctrl+shift+k");
        assert_eq!(s, Shortcut::new(true, true, false, Key::K));
        assert_eq!(s.to_text(), "Ctrl+Shift+K");
        assert_eq!(sc(&s.to_text()), s);
        assert_eq!(sc("Cmd+Alt+4").to_text(), "Ctrl+Alt+4");
        assert_eq!(sc("F7"), Shortcut::new(false, false, false, Key::F7));
        for bad in ["", "+", "Ctrl+", "Ctrl+Shift", "K+L", "Ctrl+Nope", "++K", "💥"] {
            assert!(Shortcut::parse(bad).is_none(), "{bad}");
        }
        assert!(Shortcut::from_press(Key::Escape, Modifiers::NONE).is_none());
        assert_eq!(Shortcut::from_press(Key::K, Modifiers::COMMAND | Modifiers::SHIFT), Some(sc("Ctrl+Shift+K")));
        assert!(!sc("Ctrl+K").badge().is_empty());
        let m = sc("Ctrl+Shift+K").modifiers();
        assert!(m.command && m.shift && !m.alt);
    }

    #[test]
    fn every_built_in_text_parses() {
        for (t, _) in BUILT_IN {
            assert!(Shortcut::parse(t).is_some(), "{t}");
        }
    }

    #[test]
    fn pins_toggle() {
        let mut p = Prefs::default();
        assert!(p.toggle_pin("Command:Undo"));
        assert!(p.is_pinned("Command:Undo"));
        assert!(!p.toggle_pin("Command:Undo"));
        assert!(p.pins.is_empty());
        for i in 0..MAX_PINS + 5 {
            p.toggle_pin(&format!("x{i}"));
        }
        assert_eq!(p.pins.len(), MAX_PINS);
        assert!(!p.toggle_pin(""));
    }

    #[test]
    fn conflicts_are_found_and_refused() {
        let mut p = Prefs::default();
        assert_eq!(p.conflict("a", &sc("Ctrl+Z")), Some(Conflict::BuiltIn("Undo")));
        assert!(p.set_shortcut("a", sc("Ctrl+Z")).is_err());
        assert!(p.set_shortcut("a", sc("Escape")).is_err());
        assert_eq!(p.conflict("a", &sc("Ctrl+K")), Some(Conflict::BuiltIn("Find")));
        assert!(p.set_shortcut("a", sc("Ctrl+K")).is_err(), "palette shortcut is reserved");
        assert!(p.set_shortcut("a", sc("Alt+K")).is_ok());
        assert_eq!(p.conflict("b", &sc("Alt+K")), Some(Conflict::Custom("a".into())));
        assert!(p.set_shortcut("b", sc("Alt+K")).is_err());
        assert!(p.set_shortcut("a", sc("Alt+K")).is_ok(), "the same entry may keep its own");
        assert!(p.set_shortcut("b", sc("Ctrl+L")).is_ok());
        p.clear_shortcut("a");
        assert!(p.shortcut("a").is_none());
        assert!(p.set_shortcut("b", sc("Alt+K")).is_ok(), "freed by Clear");
        assert_eq!(p.shortcut("b"), Some(sc("Alt+K")));
    }

    #[test]
    fn legacy_ctrl_k_binding_is_removed_on_load() {
        let p = Prefs::parse(r#"{"keys":{"old-command":"Ctrl+K","custom":"Alt+K"}}"#);
        assert!(p.shortcut("old-command").is_none());
        assert_eq!(p.shortcut("custom"), Some(sc("Alt+K")));
    }

    #[test]
    fn saves_and_loads_again() {
        let dir = std::env::temp_dir().join(format!("meltcad_palette_{}", std::process::id()));
        let path = dir.join("sub").join("palette.json");
        let mut p = Prefs::default();
        p.toggle_pin("Shape:Add Sphere");
        p.toggle_pin("Command:Undo");
        assert!(p.set_shortcut("Command:Undo", sc("Alt+U")).is_ok());
        assert!(p.save_to(&path).is_ok());
        assert_eq!(Prefs::load_from(&path), p);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Prefs::load_from(&path), Prefs::default(), "missing file");
    }

    #[test]
    fn hostile_files_never_panic() {
        let long = "x".repeat(5000);
        let many: String = (0..500).map(|i| format!("\"k{i}\":\"Alt+F{}\",", i % 12 + 1)).collect();
        let texts = [
            String::new(),
            "not json".to_string(),
            "[1,2,3]".to_string(),
            "null".to_string(),
            "{\"pins\": 5, \"keys\": []}".to_string(),
            "{\"pins\": [1, null, {}], \"keys\": {\"a\": 7}}".to_string(),
            format!(
                "{{\"pins\":[\"{long}\",\"\",\"a\",\"a\"],\"keys\":{{\"{long}\":\"Alt+K\",\"b\":\"Ctrl+Z\",\"c\":\"💥\",\"d\":\"Escape\",\"e\":\"Alt+K\",\"f\":\"Alt+K\"}}}}"
            ),
            format!("{{\"keys\":{{{many}\"z\":\"Alt+Q\"}}}}"),
            "\u{0}\u{1}{{{{".to_string(),
        ];
        for t in &texts {
            let p = Prefs::parse(t);
            assert!(p.pins.len() <= MAX_PINS && p.keys.len() <= MAX_KEYS);
            assert!(p.keys.values().all(|v| Shortcut::parse(v).is_some()));
        }
        let p = Prefs::parse(&texts[6]);
        assert_eq!(p.pins, vec!["a".to_string()]);
        assert_eq!(p.keys.len(), 1, "{:?}", p.keys);
        // a file that is a folder, and a bad path
        let dir = std::env::temp_dir();
        assert_eq!(Prefs::load_from(&dir), Prefs::default());
        assert!(Prefs::default().save_to(&dir.join("no_such_dir_meltcad").join("\0bad")).is_err());
    }
}
