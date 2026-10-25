//! Pictures requested through `--control` (`render`, `anim.render`).
//!
//! The window thread copies the scene and starts a worker; the worker draws with the CPU and
//! writes the complete file beside the destination, then renames it into place. A long animation
//! therefore never stops the window from drawing its own frames, and a cancelled or failed
//! picture leaves the destination exactly as it was.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use serde_json::{Value, json};

use crate::model::Scene;

/// What to draw: one four-view sheet, or the whole animation as a GIF.
pub enum Work {
    Sheet { size: usize },
    Animation { size: usize, gpu: bool },
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub struct Job {
    rx: Receiver<Result<Value, String>>,
    cancel: Arc<AtomicBool>,
    /// The control request's answer, sent only after the file is complete.
    reply: Option<Sender<Result<Value, String>>>,
}

impl Job {
    /// The finished picture (once). The waiting control client is answered here, on
    /// the window thread, so the answer never races the file's rename.
    pub fn poll(&mut self) -> Option<Result<Value, String>> {
        let result = match self.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("The picture job stopped unexpectedly".to_owned()),
        };
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(result.clone());
        }
        Some(result)
    }

    /// Ask the worker not to publish its file. A picture already renamed into place is complete.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // Closing the window (or starting over) must not leave a half-written output.
        self.cancel();
    }
}

/// Start drawing `scene` on a worker. `reply` is answered when the complete file exists.
pub fn start(ctx: &egui::Context, scene: Scene, path: PathBuf, work: Work, reply: Sender<Result<Value, String>>) -> Result<Job, String> {
    // A picture is a large allocation too: share the one-at-a-time memory guard.
    let permit = crate::work_budget::try_acquire(ctx)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("yura-control-picture".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw(&scene, &path, work, &worker_cancel)))
                .unwrap_or_else(|_| Err("The picture job stopped partway through".to_owned()));
            drop(permit);
            let _ = tx.send(result);
        })
        .map_err(|e| format!("Could not start the picture job: {e}"))?;
    Ok(Job { rx, cancel, reply: Some(reply) })
}

fn draw(scene: &Scene, path: &Path, work: Work, cancel: &AtomicBool) -> Result<Value, String> {
    #[cfg(test)]
    test_gate::wait(path);
    let (bytes, answer) = match work {
        Work::Sheet { size } => {
            let (w, rgba) = crate::cpu_render::sheet(scene, size);
            let bytes = crate::png::encode_rgba(w as u32, w as u32, &rgba)?;
            (bytes, json!({"png": path.display().to_string(), "pixels": w}))
        }
        Work::Animation { size, gpu } => {
            let bytes = crate::cpu_render::animation_gif(scene, None, size, gpu, |_, _| !cancel.load(Ordering::Acquire))?;
            (bytes, json!({"gif": path.display().to_string()}))
        }
    };
    publish(path, &bytes, cancel)?;
    Ok(answer)
}

/// Write the complete image to a sibling temp file, sync it, then rename it atomically over
/// the destination. Nothing is published if the job was stopped or the write failed.
fn publish(path: &Path, bytes: &[u8], cancel: &AtomicBool) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("The picture came out empty".into());
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Stopped".into());
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| "The output location has no file name".to_owned())?;
    let mut file = None;
    let mut temp = PathBuf::new();
    for _ in 0..16 {
        let mut temp_name = std::ffi::OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(".{}-{}.tmp", std::process::id(), NEXT_TEMP.fetch_add(1, Ordering::Relaxed)));
        temp = parent.join(temp_name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(opened) => {
                file = Some(opened);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot write {}: {e}", path.display())),
        }
    }
    let Some(mut file) = file else {
        return Err(format!("Cannot create a work file for {}", path.display()));
    };
    let written = (|| -> std::io::Result<()> {
        for chunk in bytes.chunks(256 * 1024) {
            if cancel.load(Ordering::Acquire) {
                return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "stopped"));
            }
            file.write_all(chunk)?;
        }
        file.sync_all()
    })();
    drop(file);
    let error = match written {
        Ok(()) if cancel.load(Ordering::Acquire) => Some("Stopped".to_owned()),
        Ok(()) => None,
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => Some("Stopped".to_owned()),
        Err(e) => Some(format!("Cannot write {}: {e}", path.display())),
    };
    if let Some(error) = error {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("Cannot finish {}: {e}", path.display()));
    }
    Ok(())
}

/// Test-only gate: hold one picture (keyed by its destination) before drawing, so a test can
/// observe that the window keeps running while the worker is busy.
#[cfg(test)]
pub mod test_gate {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, Sender, channel};

    static HOLD: Mutex<Vec<(PathBuf, Receiver<()>)>> = Mutex::new(Vec::new());

    /// Hold the next picture written to `path`. Send on the returned sender to release it.
    pub fn hold(path: &Path) -> Sender<()> {
        let (tx, rx) = channel();
        if let Ok(mut hold) = HOLD.lock() {
            hold.push((path.to_path_buf(), rx));
        }
        tx
    }

    pub(super) fn wait(path: &Path) {
        let held = HOLD.lock().ok().and_then(|mut hold| {
            let at = hold.iter().position(|(held_path, _)| held_path == path)?;
            Some(hold.remove(at).1)
        });
        if let Some(rx) = held {
            // A test that never releases must fail with a clear message, not deadlock the suite.
            if rx.recv_timeout(std::time::Duration::from_secs(20)).is_err() {
                panic!("the picture test gate was not released");
            }
        }
    }
}
