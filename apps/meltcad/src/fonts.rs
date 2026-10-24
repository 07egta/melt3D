//! Japanese UI text: a system CJK font installed as the last fallback.
//!
//! egui's default Latin fonts stay first in the Proportional and Monospace families, and the CJK
//! font is appended after them. English text renders as before, and only the glyphs the Latin
//! fonts lack (kana, kanji, full-width punctuation) come from the CJK face. When no usable CJK
//! font is on the machine, [`setup`] changes nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Name of the CJK entry in [`egui::FontDefinitions`].
const CJK_FALLBACK: &str = "cjk_fallback";

/// Largest font file read. Real CJK fonts are a few MB to about 30 MB; bigger files are skipped.
const MAX_FONT_BYTES: u64 = 40 * 1024 * 1024;

/// How many folders deep [`scan_dir_for_font`] looks under each system font root.
const SCAN_DEPTH: usize = 3;

/// Well-known CJK font paths, tried in order before any directory scan.
const FONT_CANDIDATES: &[&str] = &[
    // Windows
    "C:\\Windows\\Fonts\\meiryo.ttc",
    "C:\\Windows\\Fonts\\YuGothM.ttc",
    "C:\\Windows\\Fonts\\YuGothR.ttc",
    "C:\\Windows\\Fonts\\msgothic.ttc",
    "C:\\Windows\\Fonts\\BIZ-UDGothicR.ttc",
    // macOS
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    // Linux
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJKjp-Regular.otf",
    "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
    "/usr/share/fonts/truetype/ipafont/ipag.ttf",
    "/usr/share/fonts/opentype/ipafont/ipag.ttf",
    "/usr/share/fonts/truetype/takao-gothic/TakaoPGothic.ttf",
];

/// Lower-case file-name fragments that identify CJK fonts for the directory scan. Specific on
/// purpose: a generic word such as "noto" or "gothic" also matches Latin-only fonts.
const CJK_FONT_KEYWORDS: &[&str] =
    &["cjk", "notosansjp", "bizud", "biz-ud", "yugoth", "meiryo", "msgothic", "hiragino", "ipag", "ipaexg", "takao", "vlgothic", "droidsansfallback"];

/// Install the system CJK font as the last fallback of the Proportional and Monospace families.
/// Does nothing when no usable CJK font is found.
#[allow(dead_code)]
pub fn setup(ctx: &egui::Context) {
    install(ctx, find_cjk_font());
}

/// Install font bytes found earlier (see [`FontJob`]); `None` changes nothing.
/// SolidCraft's own families (its medium, semibold and bold faces) are always installed: its screen
/// asks for them, and a missing font family stops the whole app.
pub fn install(ctx: &egui::Context, bytes: Option<Vec<u8>>) {
    let base = solidcraft_ui_egui::fonts::base_font_definitions();
    ctx.set_fonts(match bytes {
        Some(bytes) => with_cjk_fallback(base, bytes),
        None => base,
    });
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(INSTALLED), true));
}

const INSTALLED: &str = "meltcad_fonts_installed";

/// Make sure SolidCraft's font families exist before its screen draws (a context nobody set fonts on,
/// as in tests, would otherwise stop at the first bold label).
pub fn ensure(ctx: &egui::Context) {
    if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new(INSTALLED)).unwrap_or(false)) {
        install(ctx, None);
    }
}

/// Looking for and reading the CJK font (a file of several MB) can take a while on a cold disk,
/// so it runs on its own thread from the first moment of start-up, while the window opens.
pub struct FontJob(Option<std::sync::mpsc::Receiver<Option<Vec<u8>>>>);

impl FontJob {
    pub fn start() -> FontJob {
        let (tx, rx) = std::sync::mpsc::channel();
        let started = std::thread::Builder::new().name("meltcad-font".into()).spawn(move || {
            let _ = tx.send(find_cjk_font());
        });
        FontJob(started.ok().map(|_| rx))
    }

    /// Poll without ever blocking the UI while a cold disk is searching for CJK fonts.
    /// Some(None) means search failed, and None means it is still running.
    pub fn poll(&mut self) -> Option<Option<Vec<u8>>> {
        let result = self.0.as_ref()?.try_recv();
        match result {
            Ok(bytes) => {
                self.0 = None;
                Some(bytes)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.0 = None;
                Some(None)
            }
        }
    }

    pub fn pending(&self) -> bool {
        self.0.is_some()
    }
}

/// `fonts` with `bytes` appended as the last fallback of the Proportional and Monospace families.
fn with_cjk_fallback(mut fonts: egui::FontDefinitions, bytes: Vec<u8>) -> egui::FontDefinitions {
    fonts.font_data.insert(CJK_FALLBACK.to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    let families: Vec<egui::FontFamily> = fonts.families.keys().cloned().collect();
    for family in families {
        let list = fonts.families.entry(family).or_default();
        list.retain(|entry| entry != CJK_FALLBACK);
        list.push(CJK_FALLBACK.to_owned());
    }
    fonts
}

/// Find a CJK font: the well-known paths first, then a bounded scan of the system font folders.
fn find_cjk_font() -> Option<Vec<u8>> {
    for path in FONT_CANDIDATES {
        if let Some(bytes) = try_read_font(Path::new(path)) {
            return Some(bytes);
        }
    }
    for root in scan_roots() {
        if let Some(bytes) = scan_dir_for_font(&root, SCAN_DEPTH, is_cjk_font_name) {
            return Some(bytes);
        }
    }
    None
}

/// System font folders to scan for a CJK font, for the platform this runs on.
fn scan_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "windows") {
        if let Some(windir) = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")) {
            roots.push(PathBuf::from(windir).join("Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/System/Library/Fonts"));
        roots.push(PathBuf::from("/Library/Fonts"));
    } else {
        roots.push(PathBuf::from("/usr/share/fonts"));
        roots.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join(".local/share/fonts"));
        }
    }
    roots
}

/// Read a font file and check that its bytes parse as a font. `None` for a missing, oversized,
/// unreadable or non-font file.
fn try_read_font(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FONT_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    ttf_parser::Face::parse(&bytes, 0).ok()?;
    Some(bytes)
}

/// Search `dir` and its subfolders, up to `depth` levels, for a font file whose lower-case name
/// passes `is_wanted`. Returns the first one that reads and parses.
fn scan_dir_for_font(dir: &Path, depth: usize, is_wanted: fn(&str) -> bool) -> Option<Vec<u8>> {
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let is_font = [".ttc", ".ttf", ".otf"].iter().any(|ext| name.ends_with(ext));
            if is_font
                && is_wanted(&name)
                && let Some(bytes) = try_read_font(&path)
            {
                return Some(bytes);
            }
        } else if path.is_dir() {
            subdirs.push(path);
        }
    }
    subdirs.iter().find_map(|sub| scan_dir_for_font(sub, depth - 1, is_wanted))
}

/// Whether a lower-case font file name looks like a CJK font.
fn is_cjk_font_name(file_name: &str) -> bool {
    CJK_FONT_KEYWORDS.iter().any(|keyword| file_name.contains(keyword))
}

#[cfg(test)]
mod job_tests {
    use super::*;

    #[test]
    fn polling_never_waits_and_consumes_exactly_one_result() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut job = FontJob(Some(rx));
        assert!(job.poll().is_none(), "no result yet");
        assert!(job.pending());
        tx.send(None).expect("send");
        assert!(matches!(job.poll(), Some(None)));
        assert!(!job.pending());
        assert!(job.poll().is_none());
    }
}
