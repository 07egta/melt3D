//! Keep model JSON readable in git while embedding only exact lexical content.
//! A generated metadata catalog lets the UI search without deserializing models.
use std::fmt::Write as _;
use std::{env, fs, io, path::Path};

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

fn compact_json(raw: &[u8]) -> io::Result<Vec<u8>> {
    let mut compact = Vec::with_capacity(raw.len());
    let mut in_string = false;
    let mut escaped = false;
    for &byte in raw {
        if in_string {
            compact.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
            compact.push(byte);
        } else if !matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
            compact.push(byte);
        }
    }
    if in_string {
        return Err(invalid("unterminated JSON string"));
    }
    Ok(compact)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn text_field<'a>(value: &'a serde_json::Value, name: &str) -> io::Result<&'a str> {
    value.get(name).and_then(serde_json::Value::as_str).ok_or_else(|| invalid(format!("missing or invalid {name}")))
}

fn tags(value: &serde_json::Value) -> io::Result<Vec<&str>> {
    let Some(tags) = value.get("tags") else { return Ok(Vec::new()) };
    let tags = tags.as_array().ok_or_else(|| invalid("tags must be an array"))?;
    tags.iter().map(|tag| tag.as_str().ok_or_else(|| invalid("tags must be strings"))).collect()
}

fn write_if_changed(path: &Path, content: &[u8]) -> io::Result<()> {
    if fs::read(path).is_ok_and(|old| old == content) {
        return Ok(());
    }
    fs::write(path, content)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::path::PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let out = std::path::PathBuf::from(env::var("OUT_DIR")?);
    let mut files = format!("const FILES: [&str; {}] = [\n", LIBRARIES.len());
    let mut catalog = format!("const CATALOG: [CategoryIndex; {}] = [\n", LIBRARIES.len());
    for library in LIBRARIES {
        let filename = format!("{library}.json");
        let path = manifest.join("library").join(&filename);
        println!("cargo:rerun-if-changed={}", path.display());
        let raw = fs::read(&path)?;
        // Validate and inspect names/tags only. Never serialize this Value back:
        // numeric spelling, escapes and every shape/material token stay exact.
        let metadata: serde_json::Value = serde_json::from_slice(&raw)?;
        let category_name = text_field(&metadata, "category")?;
        write_if_changed(&out.join(&filename), &compact_json(&raw)?)?;
        writeln!(files, "include_str!(concat!(env!(\"OUT_DIR\"), \"/{filename}\")),")?;
        writeln!(catalog, "CategoryIndex {{ name: {category_name:?}, items: &[")?;
        if let Some(items) = metadata.get("items") {
            for item in items.as_array().ok_or_else(|| invalid("items must be an array"))? {
                let name = text_field(item, "name")?;
                let tags = tags(item)?;
                let lower_name = name.to_lowercase();
                let lower_tags: Vec<String> = tags.iter().map(|tag| tag.to_lowercase()).collect();
                writeln!(catalog, "ItemIndex {{ name: {name:?}, tags: &{tags:?}, lower_name: {lower_name:?}, lower_tags: &{lower_tags:?} }},")?;
            }
        }
        writeln!(catalog, "] }},")?;
    }
    files.push_str("];\n");
    catalog.push_str("];\n");
    write_if_changed(&out.join("library_catalog.rs"), format!("{files}\n{catalog}").as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_preserves_literal_tokens_and_string_whitespace() {
        let input = br#" { "number" : 1.234567890123456789e-017 , "negative" : -0.00,
            "text" : " keep  spaces\tand\nnewlines ", "escaped" : "a\"b\\c" } "#;
        let expected = br#"{"number":1.234567890123456789e-017,"negative":-0.00,"text":" keep  spaces\tand\nnewlines ","escaped":"a\"b\\c"}"#;
        assert_eq!(compact_json(input).expect("valid strings"), expected);
        assert!(compact_json(br#"{"text":"unfinished"#).is_err());
    }
}
