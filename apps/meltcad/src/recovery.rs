//! Crash recovery and atomic project writes for the MELTCAD (YuraCAD) desktop app.
//! Recovery snapshots are NOT project saves: restoring always opens an unsaved copy.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::model::Scene;

pub const INTERVAL: Duration = Duration::from_secs(30);
/// The most a project file may hold when it is read *or* written. Saving must never publish a
/// file this app refuses to open again (recovery and Open share this bound).
pub const MAX_PROJECT_BYTES: u64 = 128 * 1024 * 1024;
const FORMAT_VERSION: u32 = 1;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn unique_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{}-{nanos}-{}", std::process::id(), NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

pub fn default_directory() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("YuraCAD")
            .join("recovery")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir)
            .join("yuracad")
            .join("recovery")
    }
}

fn size_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "The file is over the 128 MiB safety limit")
}

fn read_limited(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_PROJECT_BYTES {
        return Err(size_error());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PROJECT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(size_error());
    }
    Ok(bytes)
}

fn parse_scene(text: &str) -> Result<(Scene, Vec<String>), String> {
    // A damaged design must never unwind through the event loop.
    std::panic::catch_unwind(|| Scene::from_json_notes(text)).map_err(|_| "Reading the damaged design data was stopped".to_owned())?
}

pub fn read_project(path: &Path) -> Result<(Scene, Vec<String>), String> {
    let bytes = read_limited(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    parse_scene(&text)
}

/// Do not truncate the existing file. Use a unique temporary file in the same
/// directory, sync it, then replace the destination by rename. A failed write
/// or replace leaves the destination's previous bytes untouched.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with(path, bytes, |_| Ok(()))
}

fn atomic_write_with<F: FnOnce(&Path) -> io::Result<()>>(path: &Path, bytes: &[u8], before_rename: F) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err(size_error());
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "The save location has no file name"))?;
    for _ in 0..16 {
        let mut tmp_name = OsString::from(".");
        tmp_name.push(name);
        tmp_name.push(format!(".{}.tmp", unique_id()));
        let tmp = parent.join(tmp_name);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            before_rename(&tmp)?;
            fs::rename(&tmp, path)?;
            // Best effort: Windows cannot sync a directory using File::open.
            #[cfg(unix)]
            {
                let _ = File::open(parent).and_then(|dir| dir.sync_all());
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        return result;
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "Could not create a temp file"))
}

pub fn save_project(path: &Path, scene: &Scene) -> Result<(), String> {
    let content = scene.try_to_json()?;
    atomic_write(path, content.as_bytes()).map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    source: Option<String>,
    checksum: u64,
    payload: String,
}

/// FNV-1a detects incidental corruption; this is not an authenticity signature.
fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn read_snapshot(path: &Path) -> Result<(Scene, Vec<String>, Option<String>), String> {
    let bytes = read_limited(path).map_err(|e| e.to_string())?;
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|e| format!("Recovery file format is invalid: {e}"))?;
    if snapshot.version != FORMAT_VERSION {
        return Err(format!("Unsupported recovery data format: {}", snapshot.version));
    }
    if checksum(snapshot.payload.as_bytes()) != snapshot.checksum {
        return Err("Recovery data checksum does not match (may be damaged)".into());
    }
    let (scene, notes) = parse_scene(&snapshot.payload)?;
    Ok((scene, notes, snapshot.source))
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Is the snapshot at `path` the live work of another running copy? Its lock file is then locked.
/// A lock nobody holds belongs to a crashed copy: it is cleaned away here.
fn in_use(path: &Path) -> bool {
    let lock_path = path.with_extension("lock");
    let Ok(file) = fs::OpenOptions::new().write(true).open(&lock_path) else { return false };
    match file.try_lock() {
        Ok(()) => {
            drop(file);
            let _ = fs::remove_file(&lock_path);
            false
        }
        Err(fs::TryLockError::WouldBlock) => true,
        Err(_) => false,
    }
}

/// A unique snapshot per running process. Old snapshots are not removed until
/// the user explicitly restores/discards them, or the current work is saved.
pub struct Store {
    root: PathBuf,
    own: PathBuf,
    /// Held open (and locked) for as long as this process runs: another launch sees the snapshot
    /// beside it as work in use, not as work left by a crash. The system lets go if we crash.
    lock: Option<fs::File>,
    pending: Vec<PathBuf>,
    restored_from: Option<PathBuf>,
    last_snapshot_rev: Option<u64>,
    next_due: Instant,
}

impl Store {
    pub fn new(root: PathBuf, now: Instant) -> Store {
        let id = unique_id();
        let own = root.join(format!("session-{id}.yrecovery"));
        let lock = fs::create_dir_all(&root)
            .and_then(|()| fs::OpenOptions::new().create(true).write(true).truncate(false).open(own.with_extension("lock")))
            .ok()
            .filter(|f| f.try_lock().is_ok());
        let mut pending = Vec::new();
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "yrecovery") && path != own && !in_use(&path) {
                    pending.push(path);
                }
            }
        }
        pending.sort_by_key(|path| fs::metadata(path).and_then(|m| m.modified()).ok());
        pending.reverse();
        Store { root, own, lock, pending, restored_from: None, last_snapshot_rev: None, next_due: now + INTERVAL }
    }

    pub fn pending_path(&self) -> Option<&Path> {
        self.pending.first().map(PathBuf::as_path)
    }

    /// Ignore this launch but keep the only surviving copy for a later launch. Every old
    /// snapshot is deferred together: postponing one crashed session must not pop the next
    /// one up on the next frame.
    pub fn postpone(&mut self) {
        self.pending.clear();
    }

    /// Explicitly discard just the selected old recovery. Other sessions are untouched.
    pub fn discard_pending(&mut self) -> io::Result<()> {
        if let Some(path) = self.pending.first() {
            remove_if_present(path)?;
            self.pending.remove(0);
        }
        Ok(())
    }

    /// Accept only after successful validation, and keep the old snapshot until
    /// the recovered work has been explicitly saved or discarded.
    pub fn recover_pending(&mut self) -> Result<(Scene, Vec<String>, Option<String>), String> {
        let Some(path) = self.pending.first() else {
            return Err("Recovery file not found".into());
        };
        let restored = read_snapshot(path)?;
        self.restored_from = Some(path.clone());
        self.pending.remove(0);
        Ok(restored)
    }

    /// Returns true only if a new recovery was written. The clock is injected
    /// so interval behavior can be tested without sleeps.
    pub fn checkpoint(&mut self, scene: &Scene, source: Option<&Path>, changed: bool, rev: u64, now: Instant) -> Result<bool, String> {
        if now < self.next_due {
            return Ok(false);
        }
        self.next_due = now + INTERVAL;
        if !changed || self.last_snapshot_rev == Some(rev) {
            return Ok(false);
        }
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let payload = scene.try_to_json()?;
        let snapshot = Snapshot {
            version: FORMAT_VERSION,
            source: source.map(|p| p.to_string_lossy().into_owned()),
            checksum: checksum(payload.as_bytes()),
            payload,
        };
        let bytes = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?;
        atomic_write(&self.own, &bytes).map_err(|e| e.to_string())?;
        self.last_snapshot_rev = Some(rev);
        Ok(true)
    }

    /// A successful manual Save or an explicitly discarded/opened document.
    pub fn clear_active(&mut self) -> io::Result<()> {
        remove_if_present(&self.own)?;
        if let Some(path) = &self.restored_from {
            remove_if_present(path)?;
        }
        self.restored_from = None;
        self.last_snapshot_rev = None;
        self.next_due = Instant::now() + INTERVAL;
        Ok(())
    }

    pub fn document_saved(&mut self, rev: u64) -> io::Result<()> {
        self.clear_active()?;
        self.last_snapshot_rev = Some(rev);
        Ok(())
    }

    /// Called only after an approved, normal window exit. A crash does not
    /// execute this cleanup; never use Drop to delete a recovery.
    pub fn normal_exit(&mut self) -> io::Result<()> {
        self.clear_active()?;
        if self.lock.take().is_some() {
            remove_if_present(&self.own.with_extension("lock"))?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn own_path(&self) -> &Path {
        &self.own
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("yuracad_recovery_test_{}", unique_id()))
    }

    fn edited() -> Scene {
        let mut scene = Scene::default();
        scene.groups.push(crate::model::Group::new("Added group"));
        scene
    }

    #[test]
    fn only_changed_revisions_are_checkpointed_on_the_interval() {
        let root = temp_root();
        let now = Instant::now();
        let mut store = Store::new(root.clone(), now);
        let scene = edited();
        assert!(!store.checkpoint(&scene, None, true, 2, now).expect("not due"));
        assert!(!store.checkpoint(&scene, None, false, 2, now + INTERVAL).expect("clean"));
        assert!(store.checkpoint(&scene, None, true, 2, now + INTERVAL * 2).expect("modified"));
        assert!(!store.checkpoint(&scene, None, true, 2, now + INTERVAL * 3).expect("same revision"));
        assert!(store.checkpoint(&scene, None, true, 3, now + INTERVAL * 4).expect("new revision"));
        store.normal_exit().expect("cleanup");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pending_crashed_recovery_does_not_block_the_current_sessions_snapshot() {
        let root = temp_root();
        let now = Instant::now();
        let mut crashed = Store::new(root.clone(), now);
        assert!(crashed.checkpoint(&edited(), None, true, 2, now + INTERVAL).expect("old snapshot"));
        let old_path = crashed.own_path().to_path_buf();
        let old_bytes = fs::read(&old_path).expect("old bytes");
        drop(crashed);

        let mut current = Store::new(root.clone(), now);
        assert_eq!(current.pending_path(), Some(old_path.as_path()));
        let mut current_scene = edited();
        current_scene.groups.push(crate::model::Group::new("New session"));
        assert!(current.checkpoint(&current_scene, None, true, 3, now + INTERVAL).expect("new snapshot"));
        let current_path = current.own_path().to_path_buf();
        assert_ne!(old_path, current_path, "each session owns a separate recovery file");
        assert_eq!(fs::read(&old_path).expect("old file kept"), old_bytes);
        assert_eq!(read_snapshot(&current_path).expect("current snapshot").0, current_scene);

        drop(current); // Both sessions ended without normal_exit: both must remain recoverable.
        let restarted = Store::new(root.clone(), now);
        assert_eq!(restarted.pending.len(), 2);
        assert!(restarted.pending.contains(&old_path));
        assert!(restarted.pending.contains(&current_path));
        drop(restarted);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normal_exit_clears_only_our_snapshot() {
        let root = temp_root();
        let now = Instant::now();
        let mut store = Store::new(root.clone(), now);
        store.checkpoint(&edited(), None, true, 2, now + INTERVAL).expect("checkpoint");
        assert!(store.own_path().exists());
        store.normal_exit().expect("normal exit");
        assert!(!store.own_path().exists());
        assert!(Store::new(root.clone(), now).pending_path().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unclean_exit_is_recoverable_without_overwriting_original() {
        let root = temp_root();
        let original = root.join("original.meltcad");
        fs::create_dir_all(&root).expect("create root");
        let initial = Scene::default();
        save_project(&original, &initial).expect("save initial project");
        let original_bytes = fs::read(&original).expect("original bytes");
        let now = Instant::now();
        let mut crashed = Store::new(root.clone(), now);
        assert!(crashed.checkpoint(&edited(), Some(&original), true, 2, now + INTERVAL).expect("checkpoint"));
        drop(crashed); // Simulate a crash: normal_exit is NOT called.
        let mut restarted = Store::new(root.clone(), now);
        let (scene, _, source) = restarted.recover_pending().expect("restore");
        assert_eq!(scene, edited());
        assert!(source.is_some());
        assert_eq!(fs::read(&original).expect("original remains"), original_bytes);
        restarted.normal_exit().expect("recovered session ended");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_failure_keeps_previous_project_and_cleans_temporary_file() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        let original = root.join("original.meltcad");
        fs::write(&original, b"old project").expect("seed");
        let err = atomic_write_with(&original, b"new project", |_| Err(io::Error::other("simulated failure before rename"))).expect_err("failure");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&original).expect("read"), b"old project");
        assert_eq!(fs::read_dir(&root).expect("list").count(), 1, "failed temp file is deleted");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn later_defers_every_crashed_snapshot_for_this_launch() {
        let root = temp_root();
        let now = Instant::now();
        // two sessions crashed: each left its own snapshot with different work
        for extra in 0..2 {
            let mut crashed = Store::new(root.clone(), now);
            let mut scene = edited();
            scene.groups.push(crate::model::Group::new(format!("Crashed {extra}")));
            assert!(crashed.checkpoint(&scene, None, true, 2, now + INTERVAL).expect("snapshot"));
            drop(crashed);
        }
        let mut restarted = Store::new(root.clone(), now);
        let offered = restarted.pending.clone();
        assert!(offered.len() >= 2, "both crashed sessions are offered: {}", offered.len());
        restarted.postpone();
        assert!(restarted.pending_path().is_none(), "Later means this launch: the next crashed session must not ask again");
        for path in &offered {
            assert!(path.exists(), "postponing keeps the work for a later launch: {}", path.display());
        }
        drop(restarted);
        assert!(Store::new(root.clone(), now).pending_path().is_some(), "a later launch is asked again");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancel_recovery_keeps_snapshot_and_does_not_touch_project() {
        let root = temp_root();
        let now = Instant::now();
        let mut crash = Store::new(root.clone(), now);
        crash.checkpoint(&edited(), None, true, 2, now + INTERVAL).expect("write");
        let snapshot_path = crash.own_path().to_path_buf();
        drop(crash);
        let mut restarted = Store::new(root.clone(), now);
        assert_eq!(restarted.pending_path(), Some(snapshot_path.as_path()));
        restarted.postpone();
        assert!(restarted.pending_path().is_none());
        assert!(snapshot_path.exists(), "postponement is non-destructive");
        assert!(Store::new(root.clone(), now).pending_path().is_some(), "can try next launch");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_or_corrupted_recovery_is_an_error_not_a_panic() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("root");
        let bad = root.join("session-bad.yrecovery");
        fs::write(&bad, b"not JSON").expect("write bad file");
        assert!(read_snapshot(&bad).is_err());
        let scene = edited();
        let now = Instant::now();
        let mut store = Store::new(root.clone(), now);
        store.postpone();
        store.checkpoint(&scene, None, true, 2, now + INTERVAL).expect("checkpoint");
        let valid = store.own_path().to_path_buf();
        let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&valid).expect("read")).expect("parse");
        json["checksum"] = serde_json::json!(0);
        fs::write(&valid, serde_json::to_vec(&json).expect("encode")).expect("tamper");
        assert!(read_snapshot(&valid).is_err(), "checksum mismatch is detected");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_running_copys_snapshot_is_not_offered_and_a_crashed_ones_is() {
        let root = temp_root();
        let now = Instant::now();
        let mut running = Store::new(root.clone(), now);
        assert!(running.checkpoint(&edited(), None, true, 2, now + INTERVAL).expect("checkpoint"));
        // a second launch while the first still runs: nothing to restore, nothing it could delete
        assert!(Store::new(root.clone(), now).pending_path().is_none());
        // the first one goes away without cleaning up (a crash): its work is now offered
        drop(running);
        let second = Store::new(root.clone(), now);
        assert!(second.pending_path().is_some());
        let _ = fs::remove_dir_all(root);
    }
}
