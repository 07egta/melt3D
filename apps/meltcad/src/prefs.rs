//! Application preferences: language, display units, grid and snap defaults, navigation,
//! autosave and export defaults. They live in eframe's storage under [`PREFS_KEY`], so they
//! follow the same mechanism as the other remembered settings and never touch a design file.
//!
//! Loading is deliberately forgiving: a damaged, hand-edited or old file keeps the values it
//! can and replaces everything else with a safe default. No value is trusted as a number.

use serde::{Deserialize, Serialize};

/// The eframe storage key. This name is shared with the translation-catalog branch.
pub const PREFS_KEY: &str = "meltcad.preferences.v1";
/// Japanese is the product language; every missing preference starts here.
pub const DEFAULT_LANG: &str = "ja";
/// Millimetres are the internal unit and the default display unit.
pub const DEFAULT_UNITS: &str = "mm";
pub const DEFAULT_GRID_MM: f32 = 10.0;
pub const DEFAULT_AUTOSAVE_MINUTES: f32 = 0.5;
pub const MIN_AUTOSAVE_MINUTES: f32 = 0.5;
pub const MAX_AUTOSAVE_MINUTES: f32 = 60.0;
pub const MIN_GRID_MM: f32 = 1.0;
pub const MAX_GRID_MM: f32 = 100.0;
pub const MAX_DECIMALS: u32 = 4;
pub const MIN_ORBIT: f32 = 0.25;
pub const MAX_ORBIT: f32 = 2.0;

/// A language shown in Preferences, with the name its own speakers read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language {
    pub id: &'static str,
    pub native: &'static str,
}

/// The languages the UI offers. `zh-Hans` is the fixed id for Simplified Chinese.
pub const LANGS: [Language; 7] = [
    Language { id: "ja", native: "日本語" },
    Language { id: "en", native: "English" },
    Language { id: "zh-Hans", native: "简体中文" },
    Language { id: "ko", native: "한국어" },
    Language { id: "de", native: "Deutsch" },
    Language { id: "fr", native: "Français" },
    Language { id: "es", native: "Español" },
];

/// The canonical id of a supported language (`None` for anything else).
pub fn canonical_lang(lang: &str) -> Option<&'static str> {
    LANGS.iter().find(|l| l.id == lang).map(|l| l.id)
}

/// The language the translation hook uses on this thread. `i18n` owns the catalog and its
/// thread-local; this hook stays for the settings branch and its tests.
#[allow(dead_code)]
pub fn current_lang() -> &'static str {
    crate::i18n::language()
}

/// Use `lang` for the translation hook. Unknown ids leave the current language alone and return
/// false, so a hand-edited value can never select a language that has no catalog.
#[allow(dead_code)]
pub fn set_lang(lang: &str) -> bool {
    match canonical_lang(lang) {
        Some(id) => {
            crate::i18n::set_language(id);
            true
        }
        None => false,
    }
}

/// Units offered for dimension display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DisplayUnits {
    #[default]
    Mm,
    Cm,
    M,
    Inch,
}

impl DisplayUnits {
    /// The stored id: `mm`, `cm`, `m` or `in`.
    pub fn id(self) -> &'static str {
        match self {
            DisplayUnits::Mm => "mm",
            DisplayUnits::Cm => "cm",
            DisplayUnits::M => "m",
            DisplayUnits::Inch => "in",
        }
    }

    /// The label people read (`inch`, not `in`).
    pub fn label(self) -> &'static str {
        match self {
            DisplayUnits::Mm => "mm",
            DisplayUnits::Cm => "cm",
            DisplayUnits::M => "m",
            DisplayUnits::Inch => "inch",
        }
    }

    /// `in` is the stored id; `inch` is accepted from older or hand-edited files.
    pub fn from_id(id: &str) -> Option<DisplayUnits> {
        match id {
            "mm" => Some(DisplayUnits::Mm),
            "cm" => Some(DisplayUnits::Cm),
            "m" => Some(DisplayUnits::M),
            "in" | "inch" => Some(DisplayUnits::Inch),
            _ => None,
        }
    }

    pub fn per_mm(self) -> f32 {
        match self {
            DisplayUnits::Mm => 1.0,
            DisplayUnits::Cm => 0.1,
            DisplayUnits::M => 0.001,
            DisplayUnits::Inch => 1.0 / 25.4,
        }
    }

    /// A number in this unit, with at most [`MAX_DECIMALS`] decimal places.
    pub fn number(self, mm: f32, decimals: u32) -> String {
        let value = if mm.is_finite() { mm * self.per_mm() } else { 0.0 };
        format!("{:.*}", decimals.min(MAX_DECIMALS) as usize, value)
    }

    /// `number` plus the unit label ("3.00 cm").
    pub fn length(self, mm: f32, decimals: u32) -> String {
        format!("{} {}", self.number(mm, decimals), self.label())
    }
}

/// The mouse mapping for the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Navigation {
    /// Middle drag orbits, Shift+middle pans, right drag pans (the Melt 3D default).
    #[default]
    Melt,
    /// Middle drag pans, Shift+middle orbits, right drag pans (the CAD convention).
    Cad,
}

/// What a middle-button drag does under a navigation preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiddleAction {
    Orbit,
    Pan,
    Zoom,
}

impl Navigation {
    pub fn id(self) -> &'static str {
        match self {
            Navigation::Melt => "melt",
            Navigation::Cad => "cad",
        }
    }

    pub fn from_id(id: &str) -> Option<Navigation> {
        match id {
            "melt" => Some(Navigation::Melt),
            "cad" => Some(Navigation::Cad),
            _ => None,
        }
    }

    /// Ctrl always zooms (as before). CAD swaps the plain and Shift+middle actions.
    pub fn middle(self, shift: bool, ctrl: bool) -> MiddleAction {
        if ctrl {
            return MiddleAction::Zoom;
        }
        match (self, shift) {
            (Navigation::Melt, true) | (Navigation::Cad, false) => MiddleAction::Pan,
            (Navigation::Melt, false) | (Navigation::Cad, true) => MiddleAction::Orbit,
        }
    }
}

/// What the renderers need to know about the floor grid. Kept tiny and `Copy` so it can be sent
/// to the simple-view worker with the rest of its request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridView {
    pub on: bool,
    pub size_mm: f32,
}

impl Default for GridView {
    fn default() -> Self {
        GridView { on: true, size_mm: DEFAULT_GRID_MM }
    }
}

impl GridView {
    pub fn sanitized(self) -> GridView {
        GridView { on: self.on, size_mm: finite(self.size_mm, MIN_GRID_MM, MAX_GRID_MM, DEFAULT_GRID_MM) }
    }
}

/// What the Preferences window and eframe storage keep.
///
/// Strings are used for the enumerated values so one bad field can be replaced without throwing
/// the whole file away; [`Self::sanitize`] maps them to the supported ids.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Language id (`ja`, `en`, `zh-Hans`, `ko`, `de`, `fr`, `es`).
    pub lang: String,
    /// Display units id (`mm`, `cm`, `m`, `in`).
    pub units: String,
    /// Decimal places shown for lengths, 0..=4.
    pub decimals: u32,
    /// The floor grid is shown.
    pub grid: bool,
    /// Grid spacing in mm, 1..=100.
    pub grid_size_mm: f32,
    /// Sketch snapping to the grid.
    pub snap_grid: bool,
    /// Sketch snapping to points and edges.
    pub snap_vertex: bool,
    /// Navigation preset id (`melt`, `cad`).
    pub nav: String,
    /// Multiplies the orbit angle, 0.25..=2.
    pub orbit_sensitivity: f32,
    /// Minutes between recovery checkpoints, 0.5..=60.
    pub autosave_minutes: f32,
    /// Export format id (`stl`, `stl-ascii`, `obj`, `3mf`, `ply`, `glb`).
    pub export_format: String,
    /// Export unit id (`mm`, `cm`, `m`, `in`).
    pub export_units: String,
    /// Export up axis id (`z`, `y`).
    pub export_up: String,
}

impl Default for Preferences {
    fn default() -> Preferences {
        Preferences {
            lang: DEFAULT_LANG.to_owned(),
            units: DEFAULT_UNITS.to_owned(),
            decimals: 1,
            grid: true,
            grid_size_mm: DEFAULT_GRID_MM,
            snap_grid: true,
            snap_vertex: true,
            nav: Navigation::Melt.id().to_owned(),
            orbit_sensitivity: 1.0,
            autosave_minutes: DEFAULT_AUTOSAVE_MINUTES,
            export_format: "stl".to_owned(),
            export_units: "mm".to_owned(),
            export_up: "z".to_owned(),
        }
    }
}

fn finite(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() { value.clamp(low, high) } else { fallback }
}

impl Preferences {
    /// Replace anything unsupported and clamp every number. Safe to call after any edit or load.
    pub fn sanitize(&mut self) {
        self.lang = canonical_lang(&self.lang).unwrap_or(DEFAULT_LANG).to_owned();
        self.units = DisplayUnits::from_id(&self.units).unwrap_or(DisplayUnits::Mm).id().to_owned();
        self.decimals = self.decimals.min(MAX_DECIMALS);
        self.grid_size_mm = finite(self.grid_size_mm, MIN_GRID_MM, MAX_GRID_MM, DEFAULT_GRID_MM);
        self.nav = Navigation::from_id(&self.nav).unwrap_or(Navigation::Melt).id().to_owned();
        self.orbit_sensitivity = finite(self.orbit_sensitivity, MIN_ORBIT, MAX_ORBIT, 1.0);
        self.autosave_minutes = finite(self.autosave_minutes, MIN_AUTOSAVE_MINUTES, MAX_AUTOSAVE_MINUTES, DEFAULT_AUTOSAVE_MINUTES);
        self.export_format =
            crate::export_formats::Format::from_id(&self.export_format).unwrap_or(crate::export_formats::Format::Stl).id().to_owned();
        self.export_units = crate::export_formats::Units::from_id(&self.export_units).unwrap_or(crate::export_formats::Units::Mm).id().to_owned();
        self.export_up = crate::export_job::Up::from_id(&self.export_up).unwrap_or(crate::export_job::Up::Z).id().to_owned();
    }

    /// Parse storage text; a damaged file simply becomes the defaults.
    pub fn parse(text: &str) -> Preferences {
        let mut prefs: Preferences = serde_json::from_str(text).unwrap_or_default();
        prefs.sanitize();
        prefs
    }

    pub fn to_text(&self) -> String {
        let mut safe = self.clone();
        safe.sanitize();
        serde_json::to_string_pretty(&safe).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn units(&self) -> DisplayUnits {
        DisplayUnits::from_id(&self.units).unwrap_or(DisplayUnits::Mm)
    }

    pub fn decimals(&self) -> u32 {
        self.decimals.min(MAX_DECIMALS)
    }

    pub fn navigation(&self) -> Navigation {
        Navigation::from_id(&self.nav).unwrap_or(Navigation::Melt)
    }

    pub fn grid_view(&self) -> GridView {
        GridView { on: self.grid, size_mm: self.grid_size_mm }.sanitized()
    }

    pub fn autosave(&self) -> std::time::Duration {
        let minutes = finite(self.autosave_minutes, MIN_AUTOSAVE_MINUTES, MAX_AUTOSAVE_MINUTES, DEFAULT_AUTOSAVE_MINUTES);
        std::time::Duration::from_secs_f64(f64::from(minutes) * 60.0)
    }

    pub fn export_format(&self) -> crate::export_formats::Format {
        crate::export_formats::Format::from_id(&self.export_format).unwrap_or(crate::export_formats::Format::Stl)
    }

    pub fn export_units(&self) -> crate::export_formats::Units {
        crate::export_formats::Units::from_id(&self.export_units).unwrap_or(crate::export_formats::Units::Mm)
    }

    pub fn export_up(&self) -> crate::export_job::Up {
        crate::export_job::Up::from_id(&self.export_up).unwrap_or(crate::export_job::Up::Z)
    }

    /// Set the language; an unknown id is refused without changing the old one.
    pub fn set_language(&mut self, lang: &str) -> bool {
        match canonical_lang(lang) {
            Some(id) => {
                self.lang = id.to_owned();
                true
            }
            None => false,
        }
    }

    /// Load from eframe storage (the same place the window keeps its other remembered state).
    pub fn load_storage(storage: Option<&dyn eframe::Storage>) -> Preferences {
        storage.and_then(|s| s.get_string(PREFS_KEY)).map(|text| Preferences::parse(&text)).unwrap_or_default()
    }

    /// Write to eframe storage. The caller decides when eframe saves.
    pub fn save_storage(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(PREFS_KEY, self.to_text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Memory(std::collections::HashMap<String, String>);
    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn defaults_are_japanese_millimetres_with_grid_snap_and_melt_navigation() {
        let p = Preferences::default();
        assert_eq!(p.lang, "ja");
        assert_eq!(p.units, "mm");
        assert_eq!(p.decimals, 1);
        assert!(p.grid && p.snap_grid && p.snap_vertex);
        assert_eq!(p.navigation(), Navigation::Melt);
        assert_eq!(p.grid_view(), GridView::default());
        assert_eq!(p.autosave(), std::time::Duration::from_secs(30));
        assert_eq!(p.export_format(), crate::export_formats::Format::Stl);
        assert_eq!(p.export_units(), crate::export_formats::Units::Mm);
        assert_eq!(p.export_up(), crate::export_job::Up::Z);
    }

    #[test]
    fn every_language_id_is_unique_and_has_a_native_name() {
        let mut seen = std::collections::BTreeSet::new();
        for l in LANGS {
            assert!(seen.insert(l.id), "duplicate {}", l.id);
            assert!(!l.native.is_empty(), "{} has a native name", l.id);
        }
        assert_eq!(LANGS[0].id, "ja");
        assert_eq!(LANGS.iter().find(|l| l.id == "zh-Hans").map(|l| l.native), Some("简体中文"));
    }

    #[test]
    fn display_units_and_decimals_format_lengths() {
        assert_eq!(DisplayUnits::Mm.length(30.0, 1), "30.0 mm");
        assert_eq!(DisplayUnits::Cm.length(30.0, 2), "3.00 cm");
        assert_eq!(DisplayUnits::M.length(30.0, 3), "0.030 m");
        assert_eq!(DisplayUnits::Inch.length(30.0, 2), "1.18 inch");
        assert_eq!(DisplayUnits::Inch.length(f32::NAN, 1), "0.0 inch");
        assert_eq!(DisplayUnits::from_id("inch"), Some(DisplayUnits::Inch));
        assert_eq!(DisplayUnits::from_id("mm").map(DisplayUnits::id), Some("mm"));
        assert_eq!(DisplayUnits::from_id("furlong"), None);
    }

    #[test]
    fn navigation_swaps_the_middle_button_but_keeps_ctrl_zoom() {
        assert_eq!(Navigation::Melt.middle(false, false), MiddleAction::Orbit);
        assert_eq!(Navigation::Melt.middle(true, false), MiddleAction::Pan);
        assert_eq!(Navigation::Melt.middle(false, true), MiddleAction::Zoom);
        assert_eq!(Navigation::Cad.middle(false, false), MiddleAction::Pan);
        assert_eq!(Navigation::Cad.middle(true, false), MiddleAction::Orbit);
        assert_eq!(Navigation::Cad.middle(false, true), MiddleAction::Zoom);
    }

    #[test]
    fn unknown_values_fall_back_to_defaults_and_numbers_are_clamped() {
        let p = Preferences::parse(
            r#"{"lang":"xx","units":"furlong","grid":false,"grid_size_mm":900,
                "snap_grid":false,"snap_vertex":false,"nav":"turbo","orbit_sensitivity":99,
                "autosave_minutes":-5,"export_format":"doc","export_units":"feet","export_up":"side"}"#,
        );
        assert_eq!(p.lang, "ja");
        assert_eq!(p.units, "mm");
        assert!(!p.grid && !p.snap_grid && !p.snap_vertex);
        assert_eq!(p.grid_size_mm, MAX_GRID_MM);
        assert_eq!(p.navigation(), Navigation::Melt);
        assert_eq!(p.orbit_sensitivity, MAX_ORBIT);
        assert_eq!(p.autosave_minutes, MIN_AUTOSAVE_MINUTES);
        assert_eq!(p.export_format(), crate::export_formats::Format::Stl);
        assert_eq!(p.export_units(), crate::export_formats::Units::Mm);
        assert_eq!(p.export_up(), crate::export_job::Up::Z);

        let p = Preferences::parse(r#"{"autosave_minutes":5000,"decimals":900,"grid_size_mm":0}"#);
        assert_eq!(p.autosave_minutes, MAX_AUTOSAVE_MINUTES);
        assert_eq!(p.decimals(), MAX_DECIMALS);
        assert_eq!(p.grid_size_mm, MIN_GRID_MM);
    }

    #[test]
    fn hostile_or_wrong_types_never_panic_and_keep_only_safe_values() {
        for text in [
            "",
            "not json",
            "null",
            "[1,2,3]",
            r#"{"lang":5,"units":[]}"#,
            r#"{"grid":"yes","autosave_minutes":{}}"#,
            r#"{"autosave_minutes":1e400}"#,
        ] {
            let p = Preferences::parse(text);
            assert_eq!(p.lang, "ja", "{text}");
            assert_eq!(p.units, "mm", "{text}");
            assert!(p.autosave_minutes.is_finite(), "{text}");
        }
        let p = Preferences::parse(r#"{"lang":"en","units":"inch","autosave_minutes":1e30}"#);
        assert_eq!(p.lang, "en");
        assert_eq!(p.units, "in");
        assert_eq!(p.autosave_minutes, MAX_AUTOSAVE_MINUTES);
    }

    #[test]
    fn round_trips_through_eframe_storage_under_the_shared_key() {
        let mut memory = Memory(std::collections::HashMap::new());
        let mut p = Preferences::default();
        assert!(p.set_language("en"));
        p.units = "cm".to_owned();
        p.decimals = 2;
        p.grid = false;
        p.grid_size_mm = 25.0;
        p.snap_grid = false;
        p.snap_vertex = false;
        p.nav = Navigation::Cad.id().to_owned();
        p.orbit_sensitivity = 1.5;
        p.autosave_minutes = 7.5;
        p.export_format = "obj".to_owned();
        p.export_units = "in".to_owned();
        p.export_up = "y".to_owned();
        p.save_storage(&mut memory);
        assert!(eframe::Storage::get_string(&memory, PREFS_KEY).is_some());
        let back = Preferences::load_storage(Some(&memory));
        assert_eq!(back, p);
        assert_eq!(Preferences::load_storage(None), Preferences::default());
    }

    #[test]
    fn language_api_is_thread_local_and_refuses_unknown_ids() {
        assert_eq!(current_lang(), "ja");
        assert!(set_lang("en"));
        assert_eq!(current_lang(), "en");
        assert!(!set_lang("xx"));
        assert_eq!(current_lang(), "en");
        assert!(set_lang("ja"));
        assert_eq!(current_lang(), "ja");
    }
}
