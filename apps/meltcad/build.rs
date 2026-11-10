//! Remove JSON formatting whitespace from the bundled model library at compile time; text inside
//! JSON strings, numbers, shapes and materials is never changed. The readable originals stay in
//! `apps/meltcad/library/` and are what people edit. `src/library.rs` embeds the copies written
//! to `OUT_DIR` (a test there checks they parse to exactly the same values as the originals).
//!
//! This script never panics: if a file cannot be compacted it is copied unchanged (with a cargo
//! warning), so a build is never worse than embedding the original file.
use std::{env, fs, path::Path, path::PathBuf};

/// Keep in step with `FILES` in `src/library.rs`.
const LIBRARIES: &[&str] = &[
    "people",
    "hands",
    "animals",
    "tools_a",
    "tools_b",
    "furniture",
    "food",
    "vehicles",
    "buildings",
    "nature",
    "print",
    "pip",
    "realistic",
    "toon",
    "town",
];

/// The bytes without whitespace outside JSON strings, or None when a string is left open
/// (then the caller embeds the original bytes).
fn remove_json_whitespace(raw: &[u8]) -> Option<Vec<u8>> {
    let mut compact = Vec::with_capacity(raw.len());
    let mut in_string = false;
    let mut escaped = false;
    for &b in raw {
        if in_string {
            compact.push(b);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if b == b'"' {
            in_string = true;
            compact.push(b);
        } else if !matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
            compact.push(b);
        }
    }
    if in_string || escaped { None } else { Some(compact) }
}

fn write_one(input: &Path, output: &Path) {
    let raw = match fs::read(input) {
        Ok(raw) => raw,
        Err(e) => {
            // include_str! then reports the missing file with its own clear error.
            println!("cargo:warning=could not read {}: {e}", input.display());
            return;
        }
    };
    let bytes = match remove_json_whitespace(&raw) {
        Some(compact) => compact,
        None => {
            println!("cargo:warning={} has an unterminated JSON string; embedding it unchanged", input.display());
            raw
        }
    };
    if let Err(e) = fs::write(output, bytes) {
        println!("cargo:warning=could not write {}: {e}", output.display());
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let (Some(manifest), Some(out)) = (env::var_os("CARGO_MANIFEST_DIR"), env::var_os("OUT_DIR")) else {
        println!("cargo:warning=CARGO_MANIFEST_DIR or OUT_DIR is not set; the model library cannot be prepared");
        return;
    };
    let (manifest, out) = (PathBuf::from(manifest), PathBuf::from(out));
    if let Err(e) = fs::create_dir_all(&out) {
        println!("cargo:warning=could not create {}: {e}", out.display());
        return;
    }
    for name in LIBRARIES {
        let file = format!("{name}.json");
        let input = manifest.join("library").join(&file);
        println!("cargo:rerun-if-changed={}", input.display());
        write_one(&input, &out.join(file));
    }
}
