//! Stage complete output files on the same filesystem before changing any user-visible file.
//! No partial export is published on cancellation or an I/O error. In-place replacements on
//! Windows use a backup/rollback because std::fs::rename cannot replace a destination there.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

fn sibling(path: &Path, kind: &str) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "output".into());
    path.with_file_name(format!(".{name}.yura-{kind}-{}-{}", std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed)))
}

struct Staged {
    destination: PathBuf,
    temp: PathBuf,
    backup: Option<PathBuf>,
    installed: bool,
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temp);
    }
}

fn rollback(entries: &mut [Staged]) -> String {
    let mut errors = Vec::new();
    for entry in entries.iter_mut().rev() {
        if entry.installed {
            if let Err(e) = std::fs::remove_file(&entry.destination) {
                errors.push(format!("{}: {e}", entry.destination.display()));
            }
            entry.installed = false;
        }
        if let Some(backup) = entry.backup.take() {
            if let Err(e) = std::fs::rename(&backup, &entry.destination) {
                errors.push(format!("{} の復旧に失敗: {e}（バックアップ: {}）", entry.destination.display(), backup.display()));
            }
        }
    }
    errors.join("; ")
}

/// Write all outputs in a worker. `begin_commit` reserves the uncancellable final rename
/// window atomically; its caller should refuse cancel requests after it returns true.
pub fn write_files(
    files: &[(PathBuf, Vec<u8>)],
    cancelled: &dyn Fn() -> bool,
    begin_commit: &dyn Fn() -> bool,
    on_progress: &dyn Fn(f32),
) -> Result<u64, String> {
    if files.is_empty() { return Err("書き出すファイルがありません".into()); }
    let mut entries: Vec<Staged> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let total: u64 = files.iter().map(|(_, data)| data.len() as u64).sum();
    let mut written = 0_u64;
    for (destination, data) in files {
        if !seen.insert(destination.clone()) { return Err(format!("出力先が重複しています: {}", destination.display())); }
        if cancelled() { return Err(crate::export_mesh::CANCELLED.into()); }
        let temp = sibling(destination, "staged");
        let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&temp)
            .map_err(|e| format!("作業ファイルを作れません {}: {e}", temp.display()))?;
        entries.push(Staged { destination: destination.clone(), temp, backup: None, installed: false });
        for chunk in data.chunks(256 * 1024) {
            if cancelled() { return Err(crate::export_mesh::CANCELLED.into()); }
            file.write_all(chunk).map_err(|e| format!("書き込みに失敗: {e}"))?;
            written += chunk.len() as u64;
            on_progress(written as f32 / total.max(1) as f32);
        }
        file.sync_all().map_err(|e| format!("ファイルの確定準備に失敗: {e}"))?;
    }
    if cancelled() || !begin_commit() {
        return Err(crate::export_mesh::CANCELLED.into());
    }
    // From here to the last rename, the caller must show why cancellation is unavailable.
    for index in 0..entries.len() {
        let result = (|| -> Result<(), String> {
            let entry = &mut entries[index];
            match std::fs::symlink_metadata(&entry.destination) {
                Ok(m) => {
                    if !m.is_file() { return Err(format!("出力先はファイルではありません: {}", entry.destination.display())); }
                    let backup = sibling(&entry.destination, "backup");
                    std::fs::rename(&entry.destination, &backup).map_err(|e| format!("古いファイルを保護できません: {e}"))?;
                    entry.backup = Some(backup);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("出力先を調べられません: {e}")),
            }
            std::fs::rename(&entry.temp, &entry.destination).map_err(|e| format!("出力ファイルを確定できません: {e}"))?;
            entry.installed = true;
            Ok(())
        })();
        if let Err(reason) = result {
            let restore = rollback(&mut entries);
            return Err(if restore.is_empty() { reason } else { format!("{reason}; {restore}") });
        }
    }
    for entry in &mut entries {
        if let Some(backup) = entry.backup.take() {
            // If cleanup fails the new output is still valid; leave the backup for manual recovery.
            let _ = std::fs::remove_file(backup);
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> PathBuf {
        std::env::temp_dir().join(format!("yura_atomic_{}_{}", std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed)))
    }

    #[test]
    fn cancel_keeps_existing_file_unchanged_and_removes_temp() {
        let dir = base();
        std::fs::create_dir_all(&dir).expect("test directory");
        let path = dir.join("original.obj");
        std::fs::write(&path, b"original").expect("old data");
        let r = write_files(&[(path.clone(), vec![1; 1024])], &|| false, &|| false, &|_| {});
        assert_eq!(r.unwrap_err(), crate::export_mesh::CANCELLED);
        assert_eq!(std::fs::read(&path).expect("unmodified"), b"original");
        assert_eq!(std::fs::read_dir(&dir).expect("dir").count(), 1, "no leaked stage");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failing_multifile_commit_rolls_back_the_first_file() {
        let dir = base();
        std::fs::create_dir_all(&dir).expect("test directory");
        let primary = dir.join("model.obj");
        let invalid = dir.join("model.mtl");
        std::fs::write(&primary, b"old").expect("original");
        std::fs::create_dir_all(&invalid).expect("invalid destination directory");
        let res = write_files(&[(primary.clone(), b"new".to_vec()), (invalid.clone(), b"newmtl".to_vec())], &|| false, &|| true, &|_| {});
        assert!(res.is_err(), "a directory is not a file");
        assert_eq!(std::fs::read(&primary).expect("restored original"), b"old");
        assert!(invalid.is_dir());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn successful_replacement_contains_only_complete_files() {
        let dir = base();
        std::fs::create_dir_all(&dir).expect("test directory");
        let path = dir.join("solid.stl");
        std::fs::write(&path, b"old").expect("original");
        let bytes = write_files(&[(path.clone(), vec![7; 1000])], &|| false, &|| true, &|_| {}).expect("commit");
        assert_eq!(bytes, 1000);
        assert_eq!(std::fs::read(&path).expect("new file"), vec![7; 1000]);
        assert_eq!(std::fs::read_dir(&dir).expect("dir").count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
