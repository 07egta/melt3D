//! What the graphics path is doing, shared between the thread that compiles the shader and the
//! window (which reads it every frame): not started, compiling, ready, or failed with a reason.
//! Nothing here needs a graphics card, so the state machine is tested without one.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// After this many errors from the driver while drawing, the graphics path counts as failed and
/// the window falls back to the simple view.
pub const RUNTIME_ERROR_LIMIT: u32 = 3;
/// ...but only when they come this soon after the path became ready: a path that is broken from
/// the start. A stray error much later (a resize, say) is only noted.
pub const RUNTIME_ERROR_WINDOW: Duration = Duration::from_secs(30);

/// Where the viewport's graphics path stands.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Nothing was started (the person chose the simple view, or there is no graphics device).
    Off,
    /// The shader is being compiled by the driver.
    Compiling { since: Instant },
    /// The pipelines exist; `secs` is how long the compile took.
    Ready { secs: f64 },
    /// It cannot work (yet): the reason, in words a person can send to us.
    Failed { message: String },
}

/// Which adapter the graphics path runs on (for the card and for the log).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdapterInfo {
    pub backend: String,
    pub name: String,
    pub driver: String,
    pub device_type: String,
}

impl AdapterInfo {
    pub fn from_wgpu(info: &egui_wgpu::wgpu::AdapterInfo) -> AdapterInfo {
        let driver = if info.driver_info.is_empty() { info.driver.clone() } else { format!("{} {}", info.driver, info.driver_info) };
        AdapterInfo {
            backend: format!("{:?}", info.backend),
            name: info.name.clone(),
            driver: driver.trim().to_string(),
            device_type: format!("{:?}", info.device_type),
        }
    }

    /// True when nothing is known (no adapter yet).
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.backend.is_empty() && self.name.is_empty()
    }

    /// "Vulkan / NVIDIA GeForce ..." for cards and menus.
    pub fn short(&self) -> String {
        match (self.backend.is_empty(), self.name.is_empty()) {
            (true, true) => "(unknown)".to_string(),
            (false, true) => self.backend.clone(),
            (true, false) => self.name.clone(),
            (false, false) => format!("{} / {}", self.backend, self.name),
        }
    }
}

#[derive(Debug)]
struct Inner {
    state: State,
    info: AdapterInfo,
    /// Bumped by every `begin`; a result that carries an older number is ignored.
    generation: u32,
    runtime_errors: u32,
    last_error: Option<String>,
    ready_at: Option<Instant>,
}

/// The shared status. Cloning shares it.
#[derive(Clone, Debug)]
pub struct GpuStatus(Arc<Mutex<Inner>>);

/// A copy of the status, taken once per frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub state: State,
    pub info: AdapterInfo,
    pub runtime_errors: u32,
    pub last_error: Option<String>,
}

impl Default for GpuStatus {
    fn default() -> Self {
        GpuStatus::new()
    }
}

impl GpuStatus {
    /// Nothing started yet.
    pub fn new() -> GpuStatus {
        GpuStatus(Arc::new(Mutex::new(Inner {
            state: State::Off,
            info: AdapterInfo::default(),
            generation: 0,
            runtime_errors: 0,
            last_error: None,
            ready_at: None,
        })))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // (a thread that died holding the lock must not take the window down with it)
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn snapshot(&self) -> Snapshot {
        let g = self.lock();
        Snapshot { state: g.state.clone(), info: g.info.clone(), runtime_errors: g.runtime_errors, last_error: g.last_error.clone() }
    }

    pub fn set_info(&self, info: AdapterInfo) {
        self.lock().info = info;
    }

    /// The compile starts: returns the number to give `finish_ok` / `finish_err`.
    pub fn begin(&self, now: Instant) -> u32 {
        let mut g = self.lock();
        g.generation = g.generation.wrapping_add(1);
        g.state = State::Compiling { since: now };
        g.runtime_errors = 0;
        g.last_error = None;
        g.ready_at = None;
        g.generation
    }

    /// The compile finished. Ignored when a newer `begin` has happened since.
    pub fn finish_ok(&self, generation: u32, secs: f64) -> bool {
        self.finish_ok_at(generation, secs, Instant::now())
    }

    pub fn finish_ok_at(&self, generation: u32, secs: f64, now: Instant) -> bool {
        let mut g = self.lock();
        if g.generation != generation {
            return false;
        }
        g.state = State::Ready { secs };
        g.ready_at = Some(now);
        true
    }

    /// The compile failed. Ignored when a newer `begin` has happened since.
    pub fn finish_err(&self, generation: u32, message: impl Into<String>) -> bool {
        let mut g = self.lock();
        if g.generation != generation {
            return false;
        }
        g.state = State::Failed { message: message.into() };
        true
    }

    /// The driver reported an error while drawing (or at any time). After
    /// `RUNTIME_ERROR_LIMIT` of them in a ready path it counts as failed. Returns true when this
    /// call made it fail.
    pub fn runtime_error(&self, message: impl Into<String>) -> bool {
        self.runtime_error_at(message, Instant::now())
    }

    pub fn runtime_error_at(&self, message: impl Into<String>, now: Instant) -> bool {
        let mut g = self.lock();
        let early = g.ready_at.is_none_or(|r| now.saturating_duration_since(r) <= RUNTIME_ERROR_WINDOW);
        if early {
            g.runtime_errors = g.runtime_errors.saturating_add(1);
        }
        let message = message.into();
        g.last_error = Some(message.clone());
        if early && matches!(g.state, State::Ready { .. }) && g.runtime_errors >= RUNTIME_ERROR_LIMIT {
            g.state = State::Failed { message: format!("Error while drawing, {} times: {}", g.runtime_errors, message) };
            return true;
        }
        false
    }

    /// A drawing error caught on the spot (an error scope around our own resources): the path
    /// is failed at once.
    pub fn fail_now(&self, message: impl Into<String>) {
        let mut g = self.lock();
        let message = message.into();
        g.last_error = Some(message.clone());
        g.state = State::Failed { message };
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.lock().state, State::Ready { .. })
    }
}

impl Snapshot {
    pub fn is_ready(&self) -> bool {
        matches!(self.state, State::Ready { .. })
    }

    /// How long the compile has been running (None when it is not).
    pub fn compiling_for(&self, now: Instant) -> Option<Duration> {
        match self.state {
            State::Compiling { since } => Some(now.saturating_duration_since(since)),
            _ => None,
        }
    }
}

/// "12 s" / "1 min 05 s" for the waiting card.
pub fn elapsed_text(secs: u64) -> String {
    if secs < 60 { format!("{secs} s") } else { format!("{} min {:02} s", secs / 60, secs % 60) }
}

/// The line of the waiting card.
pub fn compiling_text(secs: u64) -> String {
    format!("Preparing graphics… the first run can take a while (elapsed {})", elapsed_text(secs))
}

/// Seconds of waiting after which the card also offers the ways out (simple view, other backend).
pub const OFFER_WAYS_OUT_AFTER_SECS: u64 = 20;

/// A wgpu error in one line of text: where it happened, what kind, and the driver's words.
pub fn describe_wgpu_error(stage: &str, err: &egui_wgpu::wgpu::Error) -> String {
    use egui_wgpu::wgpu::Error;
    let (kind, text) = match err {
        Error::OutOfMemory { .. } => ("Out of memory (OutOfMemory)", err.to_string()),
        Error::Validation { description, .. } => ("Validation error (Validation)", description.clone()),
        Error::Internal { description, .. } => ("Internal error (Internal)", description.clone()),
    };
    let mut out = format!("{stage}: {kind}: {}", text.trim());
    // the lower-level causes often carry the compiler's own message
    let mut source = std::error::Error::source(err);
    let mut depth = 0;
    while let Some(s) = source {
        let line = s.to_string();
        if !line.trim().is_empty() && !out.contains(line.trim()) {
            out.push_str("\n  caused by: ");
            out.push_str(line.trim());
        }
        source = s.source();
        depth += 1;
        if depth >= 8 {
            break;
        }
    }
    out
}

/// The text of a caught panic payload.
pub fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(no message)".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_new_status_is_off_and_not_ready() {
        let s = GpuStatus::new();
        assert_eq!(s.snapshot().state, State::Off);
        assert!(!s.is_ready());
        assert!(s.snapshot().info.is_empty());
    }

    #[test]
    fn compiling_then_ready() {
        let s = GpuStatus::new();
        let start = t0();
        let g = s.begin(start);
        assert!(matches!(s.snapshot().state, State::Compiling { .. }));
        assert_eq!(s.snapshot().compiling_for(start + Duration::from_secs(12)), Some(Duration::from_secs(12)));
        assert!(s.finish_ok(g, 3.5));
        assert_eq!(s.snapshot().state, State::Ready { secs: 3.5 });
        assert!(s.is_ready());
        assert_eq!(s.snapshot().compiling_for(start), None);
    }

    #[test]
    fn compiling_then_failed_keeps_the_message() {
        let s = GpuStatus::new();
        let g = s.begin(t0());
        assert!(s.finish_err(g, "shader: X3500 unable to unroll loop"));
        assert_eq!(s.snapshot().state, State::Failed { message: "shader: X3500 unable to unroll loop".into() });
        assert!(!s.is_ready());
    }

    #[test]
    fn a_retry_ignores_the_result_of_the_old_attempt() {
        let s = GpuStatus::new();
        let old = s.begin(t0());
        let new = s.begin(t0());
        assert_ne!(old, new);
        assert!(!s.finish_err(old, "late failure of the first try"));
        assert!(matches!(s.snapshot().state, State::Compiling { .. }), "the new attempt is still running");
        assert!(s.finish_ok(new, 1.0));
        assert!(!s.finish_ok(old, 9.0), "an old result never overwrites a newer one");
        assert_eq!(s.snapshot().state, State::Ready { secs: 1.0 });
    }

    #[test]
    fn begin_clears_the_errors_of_the_last_try() {
        let s = GpuStatus::new();
        let g = s.begin(t0());
        s.finish_ok(g, 1.0);
        s.runtime_error("a");
        assert_eq!(s.snapshot().runtime_errors, 1);
        s.begin(t0());
        let snap = s.snapshot();
        assert_eq!((snap.runtime_errors, snap.last_error), (0, None));
    }

    #[test]
    fn a_few_drawing_errors_turn_a_ready_path_into_a_failed_one() {
        let s = GpuStatus::new();
        let g = s.begin(t0());
        s.finish_ok(g, 1.0);
        for i in 1..RUNTIME_ERROR_LIMIT {
            assert!(!s.runtime_error(format!("e{i}")));
            assert!(s.is_ready(), "{i} errors are tolerated");
        }
        assert!(s.runtime_error("last"));
        match s.snapshot().state {
            State::Failed { message } => assert!(message.contains("last"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_stray_error_long_after_start_is_only_noted() {
        let s = GpuStatus::new();
        let start = t0();
        let g = s.begin(start);
        s.finish_ok_at(g, 1.0, start);
        let late = start + RUNTIME_ERROR_WINDOW + Duration::from_secs(1);
        for _ in 0..10 {
            assert!(!s.runtime_error_at("resize noise", late));
        }
        assert!(s.is_ready(), "a long-running good path is not thrown away");
        assert_eq!(s.snapshot().last_error.as_deref(), Some("resize noise"));
        assert_eq!(s.snapshot().runtime_errors, 0);
    }

    #[test]
    fn errors_while_compiling_do_not_decide_anything() {
        let s = GpuStatus::new();
        s.begin(t0());
        for _ in 0..10 {
            assert!(!s.runtime_error("noise"));
        }
        assert!(matches!(s.snapshot().state, State::Compiling { .. }));
        assert_eq!(s.snapshot().last_error.as_deref(), Some("noise"));
    }

    #[test]
    fn fail_now_fails_at_once() {
        let s = GpuStatus::new();
        let g = s.begin(t0());
        s.finish_ok(g, 1.0);
        s.fail_now("pool texture: out of memory");
        assert_eq!(s.snapshot().state, State::Failed { message: "pool texture: out of memory".into() });
    }

    #[test]
    fn a_status_survives_a_thread_that_panicked_holding_it() {
        let s = GpuStatus::new();
        let s2 = s.clone();
        let _ = std::thread::spawn(move || {
            let _guard = s2.0.lock();
            panic!("on purpose");
        })
        .join();
        let g = s.begin(t0());
        assert!(s.finish_ok(g, 2.0));
        assert!(s.is_ready());
    }

    #[test]
    fn the_waiting_text_counts_the_seconds() {
        assert_eq!(compiling_text(12), "Preparing graphics… the first run can take a while (elapsed 12 s)");
        assert_eq!(elapsed_text(0), "0 s");
        assert_eq!(elapsed_text(59), "59 s");
        assert_eq!(elapsed_text(60), "1 min 00 s");
        assert_eq!(elapsed_text(245), "4 min 05 s");
    }

    #[test]
    fn adapter_info_reads_well() {
        let a = AdapterInfo { backend: "Vulkan".into(), name: "Test GPU".into(), driver: "d 1".into(), device_type: "DiscreteGpu".into() };
        assert_eq!(a.short(), "Vulkan / Test GPU");
        assert_eq!(AdapterInfo { backend: "Gl".into(), ..Default::default() }.short(), "Gl");
        assert_eq!(AdapterInfo::default().short(), "(unknown)");
        let mut w = egui_wgpu::wgpu::AdapterInfo::new(egui_wgpu::wgpu::DeviceType::Cpu, egui_wgpu::wgpu::Backend::Vulkan);
        w.name = "W".into();
        w.vendor = 1;
        w.device = 2;
        w.driver = "llvmpipe".into();
        w.driver_info = "LLVM 19".into();
        let i = AdapterInfo::from_wgpu(&w);
        assert_eq!((i.backend.as_str(), i.name.as_str(), i.driver.as_str(), i.device_type.as_str()), ("Vulkan", "W", "llvmpipe LLVM 19", "Cpu"));
    }

    #[test]
    fn wgpu_errors_read_with_their_stage_and_cause() {
        use egui_wgpu::wgpu::Error;
        let v = Error::Validation { source: "inner cause".into(), description: "Shader error: bad".into() };
        let t = describe_wgpu_error("Shader", &v);
        assert!(t.starts_with("Shader: Validation error (Validation): Shader error: bad"), "{t}");
        assert!(t.contains("caused by: inner cause"), "{t}");
        let m = Error::OutOfMemory { source: "no room".into() };
        let t = describe_wgpu_error("Texture", &m);
        assert!(t.contains("OutOfMemory") && t.contains("Out of Memory"), "{t}");
        let i = Error::Internal { source: "x".into(), description: "driver said no".into() };
        assert!(describe_wgpu_error("pipeline", &i).contains("driver said no"));
    }

    #[test]
    fn panic_payloads_become_text() {
        let a = std::panic::catch_unwind(|| {
            let _ = std::hint::black_box(1);
            std::panic::panic_any("static message");
        })
        .unwrap_err();
        assert_eq!(panic_text(&*a), "static message");
        let b = std::panic::catch_unwind(|| std::panic::panic_any(String::from("owned"))).unwrap_err();
        assert_eq!(panic_text(&*b), "owned");
        let c = std::panic::catch_unwind(|| std::panic::panic_any(5_u32)).unwrap_err();
        assert_eq!(panic_text(&*c), "(no message)");
    }
}
