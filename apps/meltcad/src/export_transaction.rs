//! Safe multi-file exports: stage every file, reserve a rollback copy of all
//! existing targets, and replace destinations only after preparation succeeds.
//! A failure midway through OBJ+MTL must never leave the old pair mismatched.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct Part {
    dest: PathBuf,
    stage: PathBuf,
    backup: Option<PathBuf>,
    changed: bool,
}

fn unique_file(dest: &Path, tag: &str) -> io::Result<(PathBuf, File)> {
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let name = dest.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output has no name"))?;
    for _ in 0..32 {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let mut file_name = OsString::from(".");
        file_name.push(name);
        file_name.push(format!(".melt3d-{tag}-{}-{nonce}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        let candidate = parent.join(file_name);
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        match opts.open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "could not reserve an output stage"))
}

fn parent(dest: &Path) -> &Path {
    dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."))
}

fn check_target(dest: &Path) -> Result<Option<fs::Metadata>, String> {
    if !parent(dest).is_dir() {
        return Err(format!("output directory does not exist: {}", parent(dest).display()));
    }
    match fs::symlink_metadata(dest) {
        Ok(meta) if meta.file_type().is_file() && !meta.file_type().is_symlink() => Ok(Some(meta)),
        Ok(_) => Err(format!("output is not a regular file: {}", dest.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot inspect {}: {e}", dest.display())),
    }
}

/// Write every output in a set, or preserve every existing destination if
/// cancellation or any write fails. This is a best-effort filesystem
/// transaction: a process crash during the rename phase may require manual
/// restoration from a retained .melt3d-backup file.
pub fn write_many(files: &[(PathBuf, Vec<u8>)], cancelled: impl Fn() -> bool) -> Result<(), String> {
    write_many_with(files, cancelled, |_| Ok(()))
}

fn write_many_with(
    files: &[(PathBuf, Vec<u8>)],
    cancelled: impl Fn() -> bool,
    after_write: impl Fn(usize) -> Result<(), String>,
) -> Result<(), String> {
    if files.is_empty() {
        return Err("no export files were generated".into());
    }
    let mut seen = HashSet::new();
    // All destination paths are checked before creating any file, not after
    // the first output has already overwritten an existing model.
    for (dest, _) in files {
        if !seen.insert(dest) {
            return Err(format!("duplicate export destination: {}", dest.display()));
        }
        check_target(dest)?;
    }

    let mut parts: Vec<Part> = Vec::with_capacity(files.len());
    let written = (|| {
        // Stage all new bytes and sync each stage.
        for (dest, bytes) in files {
            if cancelled() {
                return Err("Export cancelled".to_string());
            }
            let (stage, mut file) = unique_file(dest, "stage").map_err(|e| format!("cannot stage {}: {e}", dest.display()))?;
            parts.push(Part { dest: dest.clone(), stage, backup: None, changed: false });
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| format!("cannot stage {}: {e}", dest.display()))?;
        }

        // Preserve the old contents of *every* existing destination before
        // the first rename. Copying is streamed to keep memory bounded.
        for part in &mut parts {
            if let Some(meta) = check_target(&part.dest)? {
                let (backup, mut output) = unique_file(&part.dest, "backup")
                    .map_err(|e| format!("cannot reserve backup {}: {e}", part.dest.display()))?;
                part.backup = Some(backup.clone());
                let mut input = File::open(&part.dest).map_err(|e| format!("cannot back up {}: {e}", part.dest.display()))?;
                io::copy(&mut input, &mut output).and_then(|_| output.sync_all())
                    .map_err(|e| format!("cannot back up {}: {e}", part.dest.display()))?;
                fs::set_permissions(&backup, meta.permissions())
                    .map_err(|e| format!("cannot preserve permissions for {}: {e}", part.dest.display()))?;
            }
        }

        // Commit now that every stage and backup is durable.
        for (i, part) in parts.iter_mut().enumerate() {
            if cancelled() {
                return Err("Export cancelled".to_string());
            }
            if let Err(first) = fs::rename(&part.stage, &part.dest) {
                // Windows may refuse rename-over-existing. We can safely
                // remove the old file only because its full backup exists.
                if part.backup.is_none() || !part.dest.is_file() {
                    return Err(format!("cannot replace {}: {first}", part.dest.display()));
                }
                fs::remove_file(&part.dest).map_err(|e| format!("cannot replace {}: {e}", part.dest.display()))?;
                part.changed = true;
                fs::rename(&part.stage, &part.dest).map_err(|e| format!("cannot replace {}: {e}", part.dest.display()))?;
            }
            part.changed = true;
            after_write(i)?;
        }
        Ok(())
    })();

    if let Err(error) = written {
        let mut restore_failures = Vec::new();
        for part in parts.iter().rev().filter(|p| p.changed) {
            let restored = match &part.backup {
                Some(backup) => fs::rename(backup, &part.dest).or_else(|first| {
                    if part.dest.is_file() {
                        fs::remove_file(&part.dest)?;
                        fs::rename(backup, &part.dest)
                    } else {
                        Err(first)
                    }
                }),
                None => match fs::remove_file(&part.dest) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e),
                },
            };
            if let Err(e) = restored {
                restore_failures.push(format!("{}: {e}", part.dest.display()));
            }
        }
        for part in &parts {
            let _ = fs::remove_file(&part.stage);
            if restore_failures.is_empty() {
                if let Some(backup) = &part.backup {
                    let _ = fs::remove_file(backup);
                }
            }
        }
        if restore_failures.is_empty() {
            return Err(error);
        }
        return Err(format!("{error}; rollback failed for {}; backup files were kept", restore_failures.join("; ")));
    }

    for part in &parts {
        let _ = fs::remove_file(&part.stage);
        if let Some(backup) = &part.backup {
            let _ = fs::remove_file(backup);
        }
        #[cfg(unix)]
        {
            let _ = File::open(parent(&part.dest)).and_then(|dir| dir.sync_all());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "melt3d-export-transaction-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_second_destination_directory_leaves_existing_obj_unchanged() {
        let folder = directory();
        let obj = folder.join("model.obj");
        let mtl = folder.join("model.mtl");
        fs::write(&obj, b"old obj").unwrap();
        fs::create_dir(&mtl).unwrap();
        let files = vec![(obj.clone(), b"new obj".to_vec()), (mtl.clone(), b"new mtl".to_vec())];
        assert!(write_many(&files, || false).is_err());
        assert_eq!(fs::read(&obj).unwrap(), b"old obj");
        assert!(mtl.is_dir());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn simulated_failure_after_first_commit_restores_both_original_files() {
        let folder = directory();
        let obj = folder.join("model.obj");
        let mtl = folder.join("model.mtl");
        fs::write(&obj, b"old obj").unwrap();
        fs::write(&mtl, b"old mtl").unwrap();
        let files = vec![(obj.clone(), b"new obj".to_vec()), (mtl.clone(), b"new mtl".to_vec())];
        let result = write_many_with(&files, || false, |i| if i == 0 { Err("injected failure".into()) } else { Ok(()) });
        assert!(result.is_err());
        assert_eq!(fs::read(&obj).unwrap(), b"old obj");
        assert_eq!(fs::read(&mtl).unwrap(), b"old mtl");
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn both_outputs_are_written_or_none_on_cancel() {
        let folder = directory();
        let obj = folder.join("a.obj");
        let mtl = folder.join("a.mtl");
        let files = vec![(obj.clone(), b"v 0 0 0\n".to_vec()), (mtl.clone(), b"newmtl main\n".to_vec())];
        assert!(write_many(&files, || true).is_err());
        assert!(!obj.exists() && !mtl.exists());
        write_many(&files, || false).unwrap();
        assert_eq!(fs::read(&obj).unwrap(), b"v 0 0 0\n");
        assert_eq!(fs::read(&mtl).unwrap(), b"newmtl main\n");
        fs::remove_dir_all(folder).unwrap();
    }
}
