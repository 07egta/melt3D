//! The log file `meltcad-log.txt`: where the graphics start-up goes wrong is written here, so that
//! a person who sees an empty viewport can send us the file. It lives next to the program when
//! that folder can be written, and in the temp folder otherwise.
//!
//! It records the wgpu version, the backend and adapter, the limits that matter, which texture
//! formats the driver can bind, when the shader compile started and ended, every error text, and
//! every panic (a panic hook appends them).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

pub const FILE_NAME: &str = "meltcad-log.txt";
/// The wgpu this build uses (a test compares it with Cargo.lock).
pub const WGPU_VERSION: &str = "30.0.1";
/// Nothing more is written once the file is this big (a loop of driver errors must not fill the disk).
pub const MAX_BYTES: u64 = 512 * 1024;

/// One log file.
pub struct LogFile {
    path: PathBuf,
    start: Instant,
}

impl LogFile {
    pub fn new(path: PathBuf) -> LogFile {
        LogFile { path, start: Instant::now() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Empty the file (a new start).
    pub fn reset(&self) -> std::io::Result<()> {
        std::fs::write(&self.path, b"")
    }

    /// Add `text` (one or more lines), each stamped with the seconds since this program started.
    pub fn append(&self, text: &str) -> std::io::Result<()> {
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if size > MAX_BYTES {
            return Ok(());
        }
        let stamp = format!("[{:8.3}s] ", self.start.elapsed().as_secs_f64());
        let pad = " ".repeat(stamp.chars().count());
        let mut out = String::with_capacity(text.len() + 32);
        for (i, line) in text.lines().enumerate() {
            out.push_str(if i == 0 { &stamp } else { &pad });
            out.push_str(line);
            out.push('\n');
        }
        if out.is_empty() {
            out.push_str(&stamp);
            out.push('\n');
        }
        if size + out.len() as u64 > MAX_BYTES {
            out.push_str("(the log is full: nothing more is written)\n");
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        f.write_all(out.as_bytes())
    }

    /// The last `n` lines of the file.
    pub fn tail(&self, n: usize) -> String {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let from = lines.len().saturating_sub(n);
        lines.get(from..).unwrap_or(&[]).join("\n")
    }
}

/// Where the log goes: next to the program if a file can be made there, else the temp folder.
pub fn choose_path(exe_dir: Option<&Path>, temp: &Path) -> PathBuf {
    if let Some(dir) = exe_dir {
        let candidate = dir.join(FILE_NAME);
        if std::fs::OpenOptions::new().create(true).append(true).open(&candidate).is_ok() {
            return candidate;
        }
    }
    temp.join(FILE_NAME)
}

static GLOBAL: OnceLock<LogFile> = OnceLock::new();

/// The program's log.
pub fn global() -> &'static LogFile {
    GLOBAL.get_or_init(|| {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        LogFile::new(choose_path(exe_dir.as_deref(), &std::env::temp_dir()))
    })
}

pub fn path() -> PathBuf {
    global().path().to_path_buf()
}

/// Append a line to the program's log (errors writing it are ignored: the log must never hurt).
pub fn log(text: &str) {
    let _ = global().append(text);
}

/// Start the log of this run. A restart with another backend (`appending`) keeps what the first
/// run wrote; a fresh start begins with an empty file.
pub fn init(appending: bool) {
    let g = global();
    if !appending {
        let _ = g.reset();
    }
    log(&format!("==== MELTCAD {} starts ({})", env!("CARGO_PKG_VERSION"), utc_text(unix_seconds())));
    log(&format!("os {} {}, wgpu {}", std::env::consts::OS, std::env::consts::ARCH, WGPU_VERSION));
    log(&format!("log file {}", g.path().display()));
    for key in ["MELTCAD_BACKEND", "MELTCAD_BACKEND_TRIED", "WGPU_BACKEND", "WGPU_DX12_COMPILER", "WGPU_POWER_PREF"] {
        if let Some(v) = std::env::var_os(key) {
            log(&format!("env {key}={}", v.to_string_lossy()));
        }
    }
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "2026-10-16 09:30:05 UTC" from Unix seconds.
pub fn utc_text(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// What a panic looks like in the log.
pub fn panic_report(thread: &str, location: &str, message: &str, backtrace: &str) -> String {
    let mut out = format!("PANIC in thread '{thread}' at {location}: {message}");
    for line in backtrace.lines().take(40) {
        out.push_str("\n    ");
        out.push_str(line.trim_end());
    }
    out
}

/// Every panic is appended to the log before the usual message goes to stderr.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_else(|| "(unknown place)".into());
        let message = crate::gpu_status::panic_text(info.payload());
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        log(&panic_report(thread.name().unwrap_or("(unnamed)"), &location, &message, &backtrace));
        previous(info);
    }));
}

struct FileLogger;

/// Which `log` records go into the file: warnings and errors from anywhere (wgpu and eframe say
/// why a graphics device could not be made through them), and the notes of our own graphics code.
pub fn wanted(level: log::Level, target: &str) -> bool {
    level <= log::Level::Warn || (level <= log::Level::Info && (target.starts_with("egui_wgpu") || target.starts_with("meltcad") || target.starts_with("eframe")))
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        wanted(metadata.level(), metadata.target())
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            log(&format!("{} {}: {}", record.level(), record.target(), record.args()));
        }
    }

    fn flush(&self) {}
}

/// Send the `log` crate's warnings (and our own notes) to the file. Does nothing if another
/// logger is already set.
pub fn install_logger() {
    static LOGGER: FileLogger = FileLogger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// The lines that describe an adapter: what it is, the limits the shader depends on, and which
/// of the texture formats the scene list uses it can bind.
pub fn adapter_report(
    info: &egui_wgpu::wgpu::AdapterInfo,
    limits: &egui_wgpu::wgpu::Limits,
    downlevel: &egui_wgpu::wgpu::DownlevelCapabilities,
    formats: &[(egui_wgpu::wgpu::TextureFormat, egui_wgpu::wgpu::TextureFormatFeatures)],
) -> Vec<String> {
    use egui_wgpu::wgpu::TextureUsages;
    let mut out = vec![
        format!("backend {:?}", info.backend),
        format!("adapter '{}' type {:?} vendor 0x{:04x} device 0x{:04x}", info.name, info.device_type, info.vendor, info.device),
        format!("driver '{}' info '{}'", info.driver, info.driver_info),
        format!(
            "limits: texture2d {} | uniform buffer {} bytes | bind groups {} | sampled textures/stage {} | color attachments {} | vertex/fragment inter-stage {}",
            limits.max_texture_dimension_2d,
            limits.max_uniform_buffer_binding_size,
            limits.max_bind_groups,
            limits.max_sampled_textures_per_shader_stage,
            limits.max_color_attachments,
            limits.max_inter_stage_shader_variables,
        ),
        format!("downlevel: shader model {:?}, flags {:?}", downlevel.shader_model, downlevel.flags),
    ];
    for (format, f) in formats {
        out.push(format!(
            "format {:?}: texture binding {}, copy dst {}, filterable {}, flags {:?}",
            format,
            f.allowed_usages.contains(TextureUsages::TEXTURE_BINDING),
            f.allowed_usages.contains(TextureUsages::COPY_DST),
            f.flags.contains(egui_wgpu::wgpu::TextureFormatFeatureFlags::FILTERABLE),
            f.flags,
        ));
    }
    out
}

/// Write an adapter's description to the log.
pub fn log_adapter(adapter: &egui_wgpu::wgpu::Adapter) {
    use egui_wgpu::wgpu::TextureFormat;
    let formats: Vec<_> = [TextureFormat::Rgba32Float, TextureFormat::Rgba16Float].into_iter().map(|f| (f, adapter.get_texture_format_features(f))).collect();
    for line in adapter_report(&adapter.get_info(), &adapter.limits(), &adapter.get_downlevel_capabilities(), &formats) {
        log(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("meltcad-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("temp dir");
        d
    }

    #[test]
    fn lines_are_stamped_and_appended() {
        let d = temp_dir("append");
        let log = LogFile::new(d.join(FILE_NAME));
        log.append("first").expect("write");
        log.append("second\nthird").expect("write");
        let text = std::fs::read_to_string(log.path()).expect("read");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with('[') && lines[0].contains("s] first"), "{text}");
        assert!(lines[1].ends_with("second") && lines[2].ends_with("third"), "{text}");
        assert_eq!(lines[1].find("second"), lines[2].find("third"), "continuation lines are aligned");
        assert_eq!(log.tail(2), format!("{}\n{}", lines[1], lines[2]));
        assert_eq!(log.tail(100).lines().count(), 3);
        log.reset().expect("reset");
        assert_eq!(std::fs::read_to_string(log.path()).expect("read"), "");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_full_log_stops_growing() {
        let d = temp_dir("full");
        let log = LogFile::new(d.join(FILE_NAME));
        let chunk = "x".repeat(1000);
        for _ in 0..(MAX_BYTES / 1000 + 20) {
            log.append(&chunk).expect("write");
        }
        let size = std::fs::metadata(log.path()).expect("meta").len();
        assert!(size <= MAX_BYTES + 2000, "{size}");
        log.append("more").expect("write");
        assert_eq!(std::fs::metadata(log.path()).expect("meta").len(), size, "nothing is added to a full log");
        assert!(log.tail(1).contains("log is full") || log.tail(2).contains("log is full"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_log_goes_next_to_the_program_when_it_can_and_to_temp_when_not() {
        let exe = temp_dir("exe");
        let tmp = temp_dir("tmp");
        assert_eq!(choose_path(Some(&exe), &tmp), exe.join(FILE_NAME));
        let missing = exe.join("no/such/folder");
        assert_eq!(choose_path(Some(&missing), &tmp), tmp.join(FILE_NAME));
        assert_eq!(choose_path(None, &tmp), tmp.join(FILE_NAME));
        let _ = std::fs::remove_dir_all(exe);
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn writing_to_an_unwritable_place_is_an_error_not_a_panic() {
        let log = LogFile::new(PathBuf::from("/definitely/not/there/meltcad-log.txt"));
        assert!(log.append("x").is_err());
        assert_eq!(log.tail(5), "");
    }

    #[test]
    fn dates_are_right() {
        assert_eq!(utc_text(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc_text(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc_text(1_781_000_000), "2026-06-09 10:13:20 UTC");
        assert_eq!(utc_text(1_709_251_199), "2024-02-29 23:59:59 UTC");
    }

    #[test]
    fn a_panic_report_names_thread_place_and_message() {
        let r = panic_report("meltcad-gpu-init", "src/gpu.rs:10", "boom", "0: a\n1: b");
        assert!(r.starts_with("PANIC in thread 'meltcad-gpu-init' at src/gpu.rs:10: boom"), "{r}");
        assert!(r.contains("\n    0: a") && r.contains("\n    1: b"));
        let long = (0..100).map(|i| format!("frame {i}")).collect::<Vec<_>>().join("\n");
        assert_eq!(panic_report("t", "p", "m", &long).lines().count(), 41, "long backtraces are cut");
    }

    #[test]
    fn the_logger_keeps_warnings_and_our_notes_only() {
        use log::Level::*;
        assert!(wanted(Error, "wgpu_hal::dx12"));
        assert!(wanted(Warn, "anything"));
        assert!(wanted(Info, "meltcad::gpu"));
        assert!(wanted(Info, "egui_wgpu"));
        assert!(!wanted(Info, "wgpu_hal::vulkan"));
        assert!(!wanted(Debug, "meltcad"));
    }

    #[test]
    fn the_adapter_report_has_the_facts_we_need() {
        use egui_wgpu::wgpu;
        let mut info = wgpu::AdapterInfo::new(wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Dx12);
        info.name = "Test GPU".into();
        info.vendor = 0x10de;
        info.device = 0x2684;
        info.driver = "NVIDIA".into();
        info.driver_info = "551.23".into();
        let features = wgpu::TextureFormatFeatures { allowed_usages: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, flags: wgpu::TextureFormatFeatureFlags::empty() };
        let lines = adapter_report(&info, &wgpu::Limits::default(), &wgpu::DownlevelCapabilities::default(), &[(wgpu::TextureFormat::Rgba32Float, features)]);
        let all = lines.join("\n");
        assert!(all.contains("backend Dx12"), "{all}");
        assert!(all.contains("'Test GPU'") && all.contains("DiscreteGpu") && all.contains("0x10de"), "{all}");
        assert!(all.contains("driver 'NVIDIA' info '551.23'"), "{all}");
        assert!(all.contains("texture2d 8192") && all.contains("uniform buffer 65536"), "{all}");
        assert!(all.contains("format Rgba32Float: texture binding true, copy dst true, filterable false"), "{all}");
    }

    #[test]
    fn the_wgpu_version_in_the_log_is_the_one_in_cargo_lock() {
        let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")).expect("Cargo.lock");
        let needle = format!("name = \"wgpu\"\nversion = \"{WGPU_VERSION}\"");
        assert!(lock.contains(&needle), "update WGPU_VERSION in gpu_log.rs");
    }
}
