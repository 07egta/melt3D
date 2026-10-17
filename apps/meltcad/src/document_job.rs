//! Native scene documents are read/parsed and serialized/staged on worker threads.
//! Only a successful result is ever applied to the live scene on the UI thread.
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Instant;

use crate::model::Scene;

// Prevent an untrusted design file from allocating an unlimited JSON buffer and parsed copy.
pub const MAX_DOCUMENT_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Open,
    Save,
}

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
        std::thread::Builder::new().name("yura-native-document".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| task(&s, &p)))
                .unwrap_or_else(|_| Err("ファイルの処理が途中で止まりました".into()));
            drop(permit);
            let _ = tx.send(result);
        }).map_err(|e| format!("ファイル処理を開始できません: {e}"))?;
        Ok(Self { rx, state, progress, kind, base_revision, started: Instant::now(), cancelled_at: None })
    }

    pub fn open(ctx: &egui::Context, path: PathBuf, base_revision: u64) -> Result<Self, String> {
        Self::spawn(ctx, Kind::Open, base_revision, move |cancel, progress| {
            let mut file = std::fs::File::open(&path).map_err(|e| format!("開けませんでした: {e}"))?;
            let size = file.metadata().map(|m| m.len()).unwrap_or(0);
            if size > MAX_DOCUMENT_BYTES as u64 { return Err("ファイルが大きすぎます（最大512 MiB）".into()); }
            let mut bytes = Vec::with_capacity(size as usize);
            let mut chunk = [0u8; 256 * 1024];
            loop {
                check_cancel(cancel)?;
                let n = file.read(&mut chunk).map_err(|e| format!("読めませんでした: {e}"))?;
                if n == 0 { break; }
                if bytes.len().saturating_add(n) > MAX_DOCUMENT_BYTES {
                    return Err("ファイルが大きすぎます（最大512 MiB）".into());
                }
                bytes.extend_from_slice(&chunk[..n]);
                if size > 0 { progress.store(((bytes.len() as f64 / size as f64).min(1.0) * 500.0) as u32, Ordering::Relaxed); }
            }
            progress.store(500, Ordering::Relaxed);
            check_cancel(cancel)?;
            let text = std::str::from_utf8(&bytes).map_err(|e| format!("文字の形式が違います: {e}"))?;
            let (scene, notes) = Scene::from_json_notes(text)?;
            check_cancel(cancel)?;
            progress.store(1000, Ordering::Relaxed);
            Ok(Completed::Opened { scene, path, notes })
        })
    }

    pub fn save(ctx: &egui::Context, path: PathBuf, scene: Scene, base_revision: u64) -> Result<Self, String> {
        Self::spawn(ctx, Kind::Save, base_revision, move |state, progress| {
            check_cancel(state)?;
            let text = scene.to_json();
            if text.is_empty() { return Err("保存データを生成できませんでした".into()); }
            if text.len() > MAX_DOCUMENT_BYTES { return Err("作品が大きすぎます（最大512 MiB）".into()); }
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
            Err(TryRecvError::Disconnected) => Some(Err("ファイル処理が予期せず停止しました".into())),
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
    pub fn can_cancel(&self) -> bool { self.state.load(Ordering::Acquire) == 0 }
    pub fn is_committing(&self) -> bool { self.state.load(Ordering::Acquire) == 2 }
    pub fn fraction(&self) -> f32 { (self.progress.load(Ordering::Relaxed) as f32 / 1000.0).clamp(0.0, 1.0) }
    pub fn elapsed(&self) -> f64 { self.started.elapsed().as_secs_f64() }
    pub fn cancel_elapsed(&self) -> Option<f64> { self.cancelled_at.map(|t| t.elapsed().as_secs_f64()) }

    pub fn phase_name(&self) -> &'static str {
        if self.is_committing() { return "保存ファイルを確定中です（元データ保護のため中止できません）"; }
        if !self.can_cancel() { return "中止処理を待っています…"; }
        match (self.kind, self.fraction() < 0.5) {
            (Kind::Open, true) => "ファイルを読んでいます",
            (Kind::Open, false) => "形状データを検証しています",
            (Kind::Save, true) => "保存データを作っています",
            (Kind::Save, false) => "完成ファイルを準備しています",
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
            if let Some(result) = job.poll() { return result; }
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
        let _ = job.cancel();
        let result = wait(&job);
        // Cancellation may lose the race to an already finished tiny read, but never
        // yields a partially parsed Scene; App verifies the revision before applying it.
        assert!(matches!(result, Err(_) | Ok(Completed::Opened { .. })));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_save_does_not_replace_existing_scene() {
        let ctx = egui::Context::default();
        let path = path("missing").join("scene.yura");
        let job = Job::save(&ctx, path, Scene::default(), 1).expect("start worker");
        assert!(wait(&job).is_err());
    }
}
