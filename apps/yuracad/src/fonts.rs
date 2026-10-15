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
pub fn setup(ctx: &egui::Context) {
    let Some(bytes) = find_cjk_font() else {
        return;
    };
    ctx.set_fonts(with_cjk_fallback(egui::FontDefinitions::default(), bytes));
}

/// `fonts` with `bytes` appended as the last fallback of the Proportional and Monospace families.
fn with_cjk_fallback(mut fonts: egui::FontDefinitions, bytes: Vec<u8>) -> egui::FontDefinitions {
    fonts.font_data.insert(CJK_FALLBACK.to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
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
