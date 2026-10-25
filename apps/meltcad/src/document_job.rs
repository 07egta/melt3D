//! Native scene documents are read/parsed and serialized/staged on worker threads.
//! Only a successful result is ever applied to the live scene on the UI thread.
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Instant;

use crate::model::Scene;

// Prevent an untrusted design file from allocating an unlimited JSON buffer and parsed copy.
// Saving uses the reader's limit too: a larger file here could never be opened again.
pub const MAX_DOCUMENT_BYTES: usize = crate::recovery::MAX_PROJECT_BYTES as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Open,
    Save,
}

#[allow(clippy::large_enum_variant)]
pub enum Completed {
    Opened { scene: Scene, path: PathBuf, notes: Vec<String> },
    Saved { path: PathBuf },
}

/// Status: 0 cancellable, 1 cancelled, 2 final file commit (cannot cancel).
pub struct Job {
    rx: Receiver<Result<Completed, String>>,
    state: Arc<AtomicU8>,
    progress: Arc<AtomicU32>,
    pub kind: Kind,
    pub base_revision: u64,
    started: Instant,
    cancelled_at: Option<Instant>,
}

fn cancelled(state: &AtomicU8) -> bool {
    state.load(Ordering::Acquire) == 1
}

fn check_cancel(state: &AtomicU8) -> Result<(), String> {
    if cancelled(state) { Err(crate::export_mesh::CANCELLED.into()) } else { Ok(()) }
}

/// Would the reader refuse a serialized document of this size? Save must refuse it first,
/// or it publishes a file the app cannot open again.
fn too_large_to_publish(bytes: usize) -> bool {
    bytes > MAX_DOCUMENT_BYTES
}

impl Job {
    fn spawn(
        ctx: &egui::Context,
        kind: Kind,
        base_revision: u64,
        task: impl FnOnce(&AtomicU8, &AtomicU32) -> Result<Completed, String> + Send + 'static,
    ) -> Result<Self, String> {
        let permit = crate::work_budget::try_acquire(ctx)?;
        let state = Arc::new(AtomicU8::new(0));
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, rx) = channel();
        let (s, p) = (state.clone(), progress.clone());
        std::thread::Builder::new()
            .name("yura-native-document".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| task(&s, &p)))
                    .unwrap_or_else(|_| Err("The file job stopped partway through".into()));
                drop(permit);
                let _ = tx.send(result);
            })
            .map_err(|e| format!("Could not start the file job: {e}"))?;
        Ok(Self { rx, state, progress, kind, base_revision, started: Instant::now(), cancelled_at: None })
    }

    pub fn open(ctx: &egui::Context, path: PathBuf, base_revision: u64) -> Result<Self, String> {
        Self::spawn(ctx, Kind::Open, base_revision, move |cancel, progress| {
            check_cancel(cancel)?;
            progress.store(100, Ordering::Relaxed);
            // The same size-limited read and damaged-file guard as every other open.
            let (scene, notes) = crate::recovery::read_project(&path)?;
            check_cancel(cancel)?;
            progress.store(1000, Ordering::Relaxed);
            Ok(Completed::Opened { scene, path, notes })
        })
    }

    pub fn save(ctx: &egui::Context, path: PathBuf, scene: Scene, base_revision: u64) -> Result<Self, String> {
        Self::spawn(ctx, Kind::Save, base_revision, move |state, progress| {
            check_cancel(state)?;
            let text = scene.try_to_json()?;
            if too_large_to_publish(text.len()) {
                return Err(format!("The design is too large (max {} MiB)", MAX_DOCUMENT_BYTES / (1024 * 1024)));
            }
            progress.store(450, Ordering::Relaxed);
            check_cancel(state)?;
            let files = vec![(path.clone(), text.into_bytes())];
            crate::atomic_files::write_files(
                &files,
                &|| cancelled(state),
                &|| state.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire).is_ok(),
                &|fraction| progress.store(450 + (fraction * 500.0) as u32, Ordering::Relaxed),
            )?;
            progress.store(1000, Ordering::Relaxed);
            Ok(Completed::Saved { path })
        })
    }

    pub fn poll(&self) -> Option<Result<Completed, String>> {
        match self.rx.try_recv() {
            Ok(_) if cancelled(&self.state) => Some(Err(crate::export_mesh::CANCELLED.into())),
            Ok(res) => Some(res),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("The file job stopped unexpectedly".into())),
        }
    }

    /// Stop while reading, parsing, serializing or staging. Final rename cannot be interrupted.
    pub fn cancel(&mut self) -> bool {
        if self.state.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            self.cancelled_at = Some(Instant::now());
            true
        } else {
            false
        }
    }
    pub fn can_cancel(&self) -> bool {
        self.state.load(Ordering::Acquire) == 0
    }
    pub fn is_committing(&self) -> bool {
        self.state.load(Ordering::Acquire) == 2
    }
    pub fn fraction(&self) -> f32 {
        (self.progress.load(Ordering::Relaxed) as f32 / 1000.0).clamp(0.0, 1.0)
    }
    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
    pub fn cancel_elapsed(&self) -> Option<f64> {
        self.cancelled_at.map(|t| t.elapsed().as_secs_f64())
    }

    pub fn phase_name(&self) -> &'static str {
        if self.is_committing() {
            return "Finishing the file (cannot be stopped, to protect the original)";
        }
        if !self.can_cancel() {
            return "Waiting for the stop to finish…";
        }
        match (self.kind, self.fraction() < 0.5) {
            (Kind::Open, true) => "Reading the file",
            (Kind::Open, false) => "Checking the shape data",
            (Kind::Save, true) => "Preparing the save data",
            (Kind::Save, false) => "Preparing the finished file",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind as ShapeKind;

    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("yura_docjob_{}_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed), name))
    }

    fn wait(job: &Job) -> Result<Completed, String> {
        for _ in 0..400 {
            if let Some(result) = job.poll() {
                return result;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err("timed out".into())
    }

    #[test]
    fn save_and_open_produce_a_complete_scene() {
        let ctx = egui::Context::default();
        let path = path("roundtrip.yura");
        let scene = Scene::default();
        let save = Job::save(&ctx, path.clone(), scene.clone(), 1).expect("start saving");
        assert!(matches!(wait(&save).expect("saved"), Completed::Saved { .. }));
        let open = Job::open(&ctx, path.clone(), 2).expect("start opening");
        let Completed::Opened { scene: loaded, .. } = wait(&open).expect("loaded") else { panic!("wrong output") };
        assert_eq!(loaded.shape_count(), scene.shape_count());
        assert_eq!(loaded.groups[0].shapes[0].kind, ShapeKind::Sphere);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_cancelled_native_load_never_returns_a_partial_scene() {
        let ctx = egui::Context::default();
        let path = path("cancel.yura");
        std::fs::write(&path, Scene::default().to_json()).expect("fixture");
        let mut job = Job::open(&ctx, path.clone(), 1).expect("start loading");
        assert!(job.cancel(), "open remains cancellable until the result is observed");
        let error = match wait(&job) {
            Err(e) => e,
            Ok(_) => panic!("a cancelled result must never replace the live scene"),
        };
        assert_eq!(error, crate::export_mesh::CANCELLED);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_never_publishes_more_than_the_reader_accepts() {
        // The reader and the recovery writer stop at 128 MiB, so a larger save limit would
        // publish a design this app can never open again.
        assert!(!too_large_to_publish(128 * 1024 * 1024));
        assert!(too_large_to_publish(128 * 1024 * 1024 + 1));
    }

    #[test]
    fn failed_save_does_not_replace_existing_scene() {
        let ctx = egui::Context::default();
        let path = path("missing").join("scene.yura");
        let job = Job::save(&ctx, path, Scene::default(), 1).expect("start worker");
        assert!(wait(&job).is_err());
    }
}
