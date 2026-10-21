//! Remove JSON formatting whitespace at compile time; never change text inside
//! JSON strings, numerical literals, shapes, materials or runtime library APIs.
//! The originals remain readable and editable in apps/meltcad/library/.
use std::{env, fs, io, path::PathBuf};

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

fn remove_json_whitespace(raw: &[u8]) -> io::Result<Vec<u8>> {
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
    if in_string || escaped {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "unterminated JSON string"));
    }
    Ok(compact)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::create_dir_all(&out)?;
    for name in LIBRARIES {
        let file = format!("{name}.json");
        let input = manifest.join("library").join(&file);
        println!("cargo:rerun-if-changed={}", input.display());
        let raw = fs::read(&input)?;
        fs::write(out.join(file), remove_json_whitespace(&raw)?)?;
    }
    Ok(())
}
