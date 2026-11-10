//! The scene shader must translate for every backend wgpu can run it on: HLSL (DirectX 12; shader
//! model 5.1 is what FXC gets, 6.0 what DXC gets), SPIR-V (Vulkan), MSL (Metal) and GLSL (OpenGL
//! ES 3.00 and desktop 3.30). A translation error here is a viewport that never appears on that
//! backend. The tests also look at what the output asks of the driver's compiler: the size of
//! the HLSL, how many loops it has, and constructs FXC is known to dislike (see `fxc_report`).
//!
//! Set `MELTCAD_DUMP_SHADERS=/some/dir` to write the translated shaders there.

use std::cell::RefCell;

use naga::back::{glsl, hlsl, msl, spv};
use naga::valid::{Capabilities, ValidationFlags, Validator};

// Warnings that naga writes through the `log` crate while translating on this thread.
thread_local! {
    static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) && r.target().starts_with("naga") {
            WARNINGS.with(|w| w.borrow_mut().push(format!("{} {}: {}", r.level(), r.target(), r.args())));
        }
    }
    fn flush(&self) {}
}

fn capture_warnings() {
    static LOGGER: Capture = Capture;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Warn);
}

/// Parse and validate the scene shader.
pub fn module() -> (naga::Module, naga::valid::ModuleInfo) {
    let src = crate::gpu::shader_source();
    let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
    let info = Validator::new(ValidationFlags::all(), Capabilities::all()).validate(&module).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
    (module, info)
}

pub fn hlsl_text(shader_model: hlsl::ShaderModel) -> String {
    let (m, info) = module();
    let options = hlsl::Options { shader_model, ..Default::default() };
    let pipeline = hlsl::PipelineOptions { entry_point: None };
    let mut out = String::new();
    hlsl::Writer::new(&mut out, &options, &pipeline).write(&m, &info, None).unwrap_or_else(|e| panic!("hlsl {shader_model:?}: {e}"));
    out
}

fn dump(name: &str, text: &str) {
    if let Some(dir) = std::env::var_os("MELTCAD_DUMP_SHADERS") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(name), text);
    }
}

fn no_warnings(what: &str) {
    let w = WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()));
    assert!(w.is_empty(), "naga warned while writing {what}:\n{}", w.join("\n"));
}

/// What the HLSL asks of FXC.
#[derive(Debug, Default, PartialEq)]
pub struct FxcReport {
    pub bytes: usize,
    pub lines: usize,
    pub loops: usize,
    /// `while(true)` style loops (the bounded ones naga makes are `while(true)` with a counter too:
    /// what matters is the count of loops that have no counter at all).
    pub unbounded_loops: usize,
    pub texture_loads: usize,
    pub derivatives: usize,
    pub functions: usize,
}

pub fn fxc_report(hlsl: &str) -> FxcReport {
    let mut r = FxcReport { bytes: hlsl.len(), lines: hlsl.lines().count(), ..Default::default() };
    // a loop counts as bounded when naga's bound counter (`loop_bound`) is declared right before it
    let lines: Vec<&str> = hlsl.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_start();
        if t.starts_with("while(true)") || t.starts_with("while (true)") || t.starts_with("for(") || t.starts_with("for (") {
            r.loops += 1;
            let after = lines.get(i + 1..(i + 3).min(lines.len())).map(|s| s.join("\n")).unwrap_or_default();
            if !after.contains("loop_bound") && !t.starts_with("for") {
                r.unbounded_loops += 1;
            }
        }
        r.texture_loads += l.matches(".Load(").count();
        r.derivatives += l.matches("fwidth(").count() + l.matches("ddx(").count() + l.matches("ddy(").count();
    }
    r.functions = hlsl
        .lines()
        .filter(|l| {
            !l.starts_with(' ')
                && !l.starts_with('\t')
                && l.contains('(')
                && l.trim_end().ends_with(')')
                && !l.starts_with("//")
                && !l.starts_with('#')
        })
        .count();
    r
}

#[test]
fn the_shader_translates_to_hlsl_5_1_and_6_0() {
    capture_warnings();
    for (name, sm) in [("scene_sm5_1.hlsl", hlsl::ShaderModel::V5_1), ("scene_sm6_0.hlsl", hlsl::ShaderModel::V6_0)] {
        let text = hlsl_text(sm);
        no_warnings(name);
        assert!(text.contains("vs(") && text.contains("fs("), "both entry points are written");
        assert!(text.len() > 10_000);
        let r = fxc_report(&text);
        eprintln!("{name}: {r:?}");
        dump(name, &text);
    }
}

#[test]
fn the_shader_translates_to_spirv() {
    capture_warnings();
    let (m, info) = module();
    let words = spv::write_vec(&m, &info, &spv::Options::default(), None).unwrap_or_else(|e| panic!("spir-v: {e}"));
    no_warnings("spir-v");
    assert_eq!(words.first(), Some(&0x0723_0203), "SPIR-V magic number");
    eprintln!("SPIR-V: {} words", words.len());
}

#[test]
fn the_shader_translates_to_msl() {
    capture_warnings();
    let (m, info) = module();
    let (text, _) = msl::write_string(&m, &info, &msl::Options::default(), &msl::PipelineOptions::default()).unwrap_or_else(|e| panic!("msl: {e}"));
    no_warnings("msl");
    assert!(text.contains("vertex") && text.contains("fragment"));
    eprintln!("MSL: {} bytes", text.len());
    dump("scene.metal", &text);
}

fn glsl_text(version: glsl::Version, stage: naga::ShaderStage, entry: &str) -> String {
    let (m, info) = module();
    let options = glsl::Options { version, ..Default::default() };
    let pipeline = glsl::PipelineOptions { shader_stage: stage, entry_point: entry.to_string(), multiview: None };
    let mut out = String::new();
    glsl::Writer::new(&mut out, &m, &info, &options, &pipeline, Default::default())
        .and_then(|mut w| w.write())
        .unwrap_or_else(|e| panic!("glsl {version:?} {entry}: {e}"));
    out
}

#[test]
fn the_shader_translates_to_glsl_es_300_and_desktop_330() {
    capture_warnings();
    for (tag, version) in [("es300", glsl::Version::new_gles(300)), ("gl330", glsl::Version::Desktop(330))] {
        for (stage, entry) in [(naga::ShaderStage::Vertex, "vs"), (naga::ShaderStage::Fragment, "fs")] {
            let text = glsl_text(version, stage, entry);
            no_warnings(&format!("glsl {tag} {entry}"));
            assert!(text.starts_with("#version"), "{tag} {entry}");
            eprintln!("GLSL {tag} {entry}: {} bytes", text.len());
            dump(&format!("scene_{tag}_{entry}.glsl"), &text);
        }
    }
}

#[test]
fn the_blit_shader_translates_everywhere_too() {
    capture_warnings();
    let src = include_str!("blit.wgsl");
    let m = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
    let info = Validator::new(ValidationFlags::all(), Capabilities::all()).validate(&m).expect("valid");
    let mut out = String::new();
    hlsl::Writer::new(&mut out, &hlsl::Options::default(), &hlsl::PipelineOptions { entry_point: None }).write(&m, &info, None).expect("hlsl");
    spv::write_vec(&m, &info, &spv::Options::default(), None).expect("spv");
    msl::write_string(&m, &info, &msl::Options::default(), &msl::PipelineOptions::default()).expect("msl");
    no_warnings("blit");
}

/// Derivatives (`fwidth`) are only well defined where the four pixels of a block run the same
/// code. DirectX's compiler warns (and may mis-compile) when they sit in flow that depends on what
/// the pixel shows, so they are taken at the top level of `fs`, before any branch.
#[test]
fn derivatives_are_taken_in_uniform_flow_only() {
    let wgsl = crate::gpu::shader_source();
    let first_march = wgsl.find("for (var i = 0; i < max_it").expect("the march loop");
    let last_fwidth = wgsl.rfind("fwidth(").expect("fwidth is used");
    assert!(last_fwidth < first_march, "every fwidth comes before the march loop's first branch");
    let text = hlsl_text(hlsl::ShaderModel::V5_1);
    let fs_at = text.find("float4 fs(").expect("fs");
    let fs = &text[fs_at..];
    let loop_at = fs.find("while(true)").expect("a loop in fs");
    for (i, l) in fs.lines().enumerate().filter(|(_, l)| l.contains("fwidth(")) {
        let indent = l.len() - l.trim_start().len();
        assert_eq!(indent, 4, "line {i} of fs: a derivative is not at the top level: {l}");
    }
    let first_fwidth = fs.find("fwidth(").expect("fwidth in fs");
    assert!(first_fwidth < loop_at, "the derivatives come before the first loop");
}

#[test]
fn the_report_counts_what_it_should() {
    let sample = "float4 f(float2 p) {\n    uint loop_bound = 0u;\n    while(true) {\n        if (loop_bound >= 4u) { break; }\n    }\n    while(true) {\n    }\n    for(int i = 0; i < 3; i++) {}\n    float4 c = t.Load(int3(0,0,0));\n    float2 w = fwidth(p);\n    return c;\n}\n";
    let r = fxc_report(sample);
    assert_eq!((r.loops, r.unbounded_loops, r.texture_loads, r.derivatives), (3, 1, 1, 1), "{r:?}");
}

/// How big a driver compiler that inlines every call (FXC and LLVM both do) sees each function:
/// its own expressions plus those of everything it calls, once per call site.
pub mod inline_cost {
    use naga::{Block, Function, Handle, Statement};
    use std::collections::HashMap;

    fn calls_in(block: &Block, out: &mut Vec<Handle<Function>>) {
        for st in block.iter() {
            match st {
                Statement::Call { function, .. } => out.push(*function),
                Statement::Block(b) => calls_in(b, out),
                Statement::If { accept, reject, .. } => {
                    calls_in(accept, out);
                    calls_in(reject, out);
                }
                Statement::Switch { cases, .. } => cases.iter().for_each(|c| calls_in(&c.body, out)),
                Statement::Loop { body, continuing, .. } => {
                    calls_in(body, out);
                    calls_in(continuing, out);
                }
                _ => {}
            }
        }
    }

    /// (name, own expressions, call sites of other functions, expanded expressions) per function,
    /// and for the entry point `fs` the number of times each function is called after inlining.
    pub struct Table {
        pub rows: Vec<(String, usize, usize, usize)>,
        pub fs_expanded: usize,
        /// How many copies of each function the inlined `fs` contains.
        pub copies_in_fs: HashMap<String, usize>,
    }

    pub fn table(module: &naga::Module) -> Table {
        let mut expanded: HashMap<Handle<Function>, usize> = HashMap::new();
        let mut own: HashMap<Handle<Function>, (usize, Vec<Handle<Function>>)> = HashMap::new();
        for (h, f) in module.functions.iter() {
            let mut calls = Vec::new();
            calls_in(&f.body, &mut calls);
            own.insert(h, (f.expressions.len(), calls));
        }
        // functions are stored callee-first, so one pass in order is enough
        for (h, _) in module.functions.iter() {
            let Some((n, calls)) = own.get(&h) else { continue };
            let total = n + calls.iter().map(|c| expanded.get(c).copied().unwrap_or(0)).sum::<usize>();
            expanded.insert(h, total);
        }
        let name = |h: Handle<Function>| module.functions[h].name.clone().unwrap_or_default();
        let mut rows: Vec<_> = module
            .functions
            .iter()
            .map(|(h, f)| {
                let (n, calls) = own.get(&h).cloned().unwrap_or_default();
                (f.name.clone().unwrap_or_default(), n, calls.len(), expanded.get(&h).copied().unwrap_or(0))
            })
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.3));
        // copies of each function inside the inlined entry point
        let mut copies_in_fs: HashMap<String, usize> = HashMap::new();
        let mut fs_expanded = 0;
        if let Some(ep) = module.entry_points.iter().find(|e| e.name == "fs") {
            let mut direct = Vec::new();
            calls_in(&ep.function.body, &mut direct);
            fs_expanded = ep.function.expressions.len() + direct.iter().map(|c| expanded.get(c).copied().unwrap_or(0)).sum::<usize>();
            fn walk(
                h: Handle<Function>,
                own: &HashMap<Handle<Function>, (usize, Vec<Handle<Function>>)>,
                mult: usize,
                out: &mut HashMap<String, usize>,
                name: &dyn Fn(Handle<Function>) -> String,
            ) {
                *out.entry(name(h)).or_default() += mult;
                if let Some((_, calls)) = own.get(&h) {
                    for c in calls {
                        walk(*c, own, mult, out, name);
                    }
                }
            }
            for c in direct {
                walk(c, &own, 1, &mut copies_in_fs, &name);
            }
        }
        Table { rows, fs_expanded, copies_in_fs }
    }
}

#[test]
fn inlining_cost_report() {
    let (m, _) = module();
    let t = inline_cost::table(&m);
    eprintln!("fs expanded (expressions after inlining everything): {}", t.fs_expanded);
    for (name, own, calls, exp) in t.rows.iter().take(14) {
        eprintln!(
            "  {name:<16} own {own:>5}  call sites {calls:>3}  expanded {exp:>8}  copies in fs {:>3}",
            t.copies_in_fs.get(name).copied().unwrap_or(0)
        );
    }
    // The distance function is most of the shader and every call site is compiled again by the
    // driver (software Vulkan: 190 s with 23 copies, 75 s with 11). A new call site must be a
    // deliberate decision: raise these numbers only knowingly.
    let copies = |n: &str| t.copies_in_fs.get(n).copied().unwrap_or(0);
    assert!(copies("map_d") <= 11, "map_d is inlined {} times in fs", copies("map_d"));
    assert!(copies("map") <= 3, "map is inlined {} times in fs", copies("map"));
    assert!(copies("shape_dist") <= 15, "shape_dist is inlined {} times in fs", copies("shape_dist"));
    // Linear-light material blending adds 535 scalar expressions to the former 152k budget.
    // Distance-function/map call limits above stay unchanged; permit only this measured increase
    // (the parametric vase profile adds about 900 more, the rounded-square tube section about 3,200, the ripple modifier about 3,800,
    // the shell shortening through a private variable measured 164,754 before the pattern merge;
    // the Twist modifier, a branch-free untwist in each shape_dist copy plus its two gated loads in
    // shape_at, was estimated (not measured) at about 900 more, hence the extra 1,000; measured
    // in CI on the Twist preset branch it is 166,253 (the limit leaves a 47-expression margin).
    assert!(t.fs_expanded <= 166_300, "fs is {} expressions after inlining", t.fs_expanded);
}
