//! The viewport on the GPU: one full-screen triangle whose shader sphere-traces the blob scene
//! (`shader.wgsl`). The scene travels in a small uniform buffer (camera, light) plus a float
//! texture holding the scene list (groups, blocks, shapes, outlines, path nodes). The picture is rendered into an
//! offscreen texture only when its inputs changed (see `perf::FrameCache`); every egui frame just
//! copies that texture into the window.

use std::sync::Arc;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

use crate::camera::{Camera, TAN_HALF_FOV};
use crate::model::{MAX_GROUPS, MAX_SHAPES, Pick, Scene};
use crate::sdf::Prepared;

/// Per-frame numbers (camera, light, counts, where the lists start) travel in a small uniform
/// buffer: `GLOBAL_V4` vec4 (struct `Globals` in shader.wgsl). The scene itself is a long list of
/// vec4 in a float texture (`ROW` vec4 per row), read with `textureLoad`: there is no size limit
/// from the 64 KB uniform buffer, and it works on every backend (a storage buffer is not allowed
/// by the limits egui-wgpu asks for on OpenGL). The list is as long as the scene needs.
const V4_BOUND: usize = 7;
const V4_HOVER: usize = 8;
const V4_SELSPH: usize = 9;
const V4_OFFS: usize = 10;
const V4_OFFS2: usize = 11;
const V4_ENV: usize = 12;
const ENV_V4: usize = 11;
pub const GLOBAL_V4: usize = V4_ENV + ENV_V4;
/// Size of the uniform buffer in bytes.
pub const GLOBAL_BYTES: usize = GLOBAL_V4 * 16;
/// vec4 per texture row of the scene list.
pub const ROW: usize = 1024;
/// vec4 per group, per block of shapes, per shape (a bounding sphere and the 15 of `struct Shape`).
const G_STRIDE: usize = 3;
const B_STRIDE: usize = 2;
const S_STRIDE: usize = 16;
/// Outline points the lists hold (two points per vec4) and path nodes (one per vec4).
pub const POLY_PTS: usize = 65536;
pub const PATH_V4: usize = 32768;
/// The longest the scene list can ever be (vec4), well inside a 2048-row texture.
pub const MAX_POOL_V4: usize = MAX_GROUPS * G_STRIDE + MAX_SHAPES * (S_STRIDE + B_STRIDE) + POLY_PTS / 2 + PATH_V4 + 4096;
const _: () = assert!(MAX_POOL_V4 <= 2048 * ROW, "the scene list must fit a 2048 x 1024 texture");

fn put(u: &mut [f32], slot: usize, v: [f32; 4]) {
    if let Some(dst) = u.get_mut(slot * 4..slot * 4 + 4) {
        dst.copy_from_slice(&v);
    }
}

fn push(list: &mut Vec<f32>, v: [f32; 4]) {
    list.extend_from_slice(&v);
}

/// Everything the shader needs for one frame: `GLOBAL_V4` vec4 of globals, then the scene list.
pub fn pack(scene: &Scene, cam: &Camera, size: [f32; 2], selected: Option<Pick>, hover: Option<Pick>, srgb: bool) -> Vec<f32> {
    let mut u = vec![0.0_f32; GLOBAL_V4 * 4];
    let (right, up, fwd) = cam.basis();
    let eye = cam.eye();
    put(&mut u, 0, [eye[0], eye[1], eye[2], 0.0]);
    put(&mut u, 1, [right[0], right[1], right[2], 0.0]);
    put(&mut u, 2, [up[0], up[1], up[2], 0.0]);
    put(&mut u, 3, [fwd[0], fwd[1], fwd[2], if cam.ortho { 0.0 } else { TAN_HALF_FOV }]);
    put(&mut u, 4, [size[0], size[1], cam.half_height(), if srgb { 1.0 } else { 0.0 }]);
    let m = scene.melt;
    put(&mut u, 5, [m.mode.id(), m.amount, m.drip, m.ripple]);

    // the lists
    let mut groups: Vec<f32> = Vec::new();
    let mut blocks: Vec<f32> = Vec::new();
    let mut shapes: Vec<f32> = Vec::new();
    let mut poly: Vec<f32> = Vec::new();
    let mut path: Vec<f32> = Vec::new();
    let mut shape_n = 0usize;
    let mut group_n = 0usize;
    let mut block_n = 0usize;
    let mut sel_first = -1.0_f32;
    let mut sel_count = 0.0_f32;
    let mut hover_first = -1.0_f32;
    let mut hover_count = 0.0_f32;
    for (gi, g) in scene.groups.iter().enumerate() {
        if !g.visible {
            continue;
        }
        let first = shape_n;
        let mut members: Vec<&crate::model::Shape> = Vec::new();
        for (si, s) in g.shapes.iter().enumerate() {
            if !s.visible || shape_n >= MAX_SHAPES {
                continue;
            }
            pack_shape(s, &mut shapes, &mut poly, &mut path);
            members.push(s);
            if hover == Some(Pick::Shape(gi, si)) {
                hover_first = shape_n as f32;
                hover_count = 1.0;
            }
            if selected == Some(Pick::Shape(gi, si)) {
                sel_first = shape_n as f32;
                sel_count = 1.0;
            }
            shape_n += 1;
        }
        let count = shape_n - first;
        if selected == Some(Pick::Group(gi)) && count > 0 {
            sel_first = first as f32;
            sel_count = count as f32;
        }
        if count > 0 && group_n < MAX_GROUPS {
            let info = crate::accel::group_info(&members, &scene.melt);
            push(&mut groups, [block_n as f32, info.blocks.len() as f32, g.op.sign(), g.blend]);
            push(&mut groups, [0.0; 4]);
            push(&mut groups, [info.bound.c[0], info.bound.c[1], info.bound.c[2], info.bound.r]);
            for b in &info.blocks {
                push(&mut blocks, [b.bound.c[0], b.bound.c[1], b.bound.c[2], b.bound.r]);
                push(&mut blocks, [b.kmax, if b.skippable { 0.0 } else { 1.0 }, (first + b.first) as f32, b.count as f32]);
            }
            block_n += info.blocks.len();
            group_n += 1;
        }
    }
    // the sections follow each other: groups, blocks, (instances), shapes, outline points, path nodes
    let v4 = |l: &Vec<f32>| l.len() / 4;
    let block_base = v4(&groups);
    let inst_base = block_base + v4(&blocks);
    let shape_base = inst_base;
    while poly.len() % 4 != 0 {
        poly.push(0.0);
    }
    let poly_base = shape_base + v4(&shapes);
    let path_base = poly_base + v4(&poly);
    put(&mut u, 6, [group_n as f32, shape_n as f32, sel_first, sel_count]);
    put(&mut u, V4_OFFS, [0.0, block_base as f32, inst_base as f32, shape_base as f32]);
    put(&mut u, V4_OFFS2, [poly_base as f32, path_base as f32, 0.0, 0.0]);
    let (bc, br) = bounds(scene, selected);
    put(&mut u, V4_BOUND, [bc[0], bc[1], bc[2], br]);
    put(&mut u, V4_HOVER, [hover_first, hover_count, 0.0, 0.0]);
    put(&mut u, V4_SELSPH, crate::perf::sphere_of(scene, &crate::view::targets(scene, selected)));
    let e = &scene.env;
    let flags = (if e.grid { 1.0 } else { 0.0 }) + (if e.floor { 2.0 } else { 0.0 });
    put(&mut u, V4_ENV, [e.exposure, e.ambient, e.shadow, flags]);
    put(&mut u, V4_ENV + 1, [e.backdrop[0], e.backdrop[1], e.backdrop[2], e.lights.len() as f32]);
    put(&mut u, V4_ENV + 2, [e.sky_top[0], e.sky_top[1], e.sky_top[2], 0.0]);
    put(&mut u, V4_ENV + 3, [e.sky_horizon[0], e.sky_horizon[1], e.sky_horizon[2], 0.0]);
    put(&mut u, V4_ENV + 4, [e.sky_bottom[0], e.sky_bottom[1], e.sky_bottom[2], 0.0]);
    for (i, l) in e.lights.iter().enumerate().take(crate::model::MAX_LIGHTS) {
        let d = l.dir();
        put(&mut u, V4_ENV + 5 + i * 2, [d[0], d[1], d[2], l.power]);
        put(&mut u, V4_ENV + 6 + i * 2, [l.color[0], l.color[1], l.color[2], 0.0]);
    }
    u.reserve(groups.len() + blocks.len() + shapes.len() + poly.len() + path.len());
    u.extend_from_slice(&groups);
    u.extend_from_slice(&blocks);
    u.extend_from_slice(&shapes);
    u.extend_from_slice(&poly);
    u.extend_from_slice(&path);
    u
}

/// One shape's 16 vec4 (a bounding sphere, then `struct Shape`); its outline points and path nodes
/// go into the shared pools.
fn pack_shape(s: &crate::model::Shape, out: &mut Vec<f32>, poly: &mut Vec<f32>, path: &mut Vec<f32>) {
    let p = Prepared::new(s);
    // outline points, two per vec4: returns (first point, count)
    let pack_outline = |pts: &[[f32; 2]], poly: &mut Vec<f32>| -> (usize, usize) {
        let used = poly.len() / 2;
        if pts.is_empty() || used + pts.len() > POLY_PTS {
            return (0, 0);
        }
        for pt in pts {
            poly.extend_from_slice(pt);
        }
        (used, pts.len())
    };
    // (the prepared copy: a revolve profile is already kept on the +x side)
    let (start, count) = if s.kind.uses_outline() { pack_outline(&p.outline, poly) } else { (0, 0) };
    let (start2, count2) = if s.kind.uses_outline2() { pack_outline(&p.outline2, poly) } else { (0, 0) };
    let (pstart, pcount) = if s.kind == crate::model::Kind::Curve {
        pack_curve(s, path)
    } else if s.kind.uses_path() && path.len() / 4 + s.path.len() <= PATH_V4 {
        let st = path.len() / 4;
        for pt in &s.path {
            push(path, *pt);
        }
        (st, s.path.len())
    } else {
        (0, 0)
    };
    let radius = crate::perf::shape_radius(s);
    push(out, [s.pos[0], s.pos[1], s.pos[2], radius]);
    push(out, [p.inv[0][0], p.inv[0][1], p.inv[0][2], 0.0]);
    push(out, [p.inv[1][0], p.inv[1][1], p.inv[1][2], 0.0]);
    push(out, [p.inv[2][0], p.inv[2][1], p.inv[2][2], 0.0]);
    push(out, [s.pos[0], s.pos[1], s.pos[2], p.min_scale()]);
    push(out, [s.scale[0], s.scale[1], s.scale[2], s.kind.id()]);
    push(out, [s.size[0], s.size[1], s.size[2], p.round]);
    push(out, [s.op.sign(), s.blend, start as f32, count as f32]);
    let m = &s.material;
    push(out, [m.color[0], m.color[1], m.color[2], m.roughness]);
    push(out, [m.metallic, m.glow, m.glass, m.iridescence]);
    // the gradient: slot 13 = second colour + strength, slot 14 = direction (+ 3 when it follows
    // the shape's own box), softness, from, to; mat2.z = signed 1 / the gradient's length
    let inv_len = if m.has_gradient() { (if m.grad_flip { -0.5 } else { 0.5 }) / p.grad_half.max(1e-3) } else { 0.0 };
    push(out, [m.coat, m.translucency, inv_len, 0.0]);
    push(out, s.params);
    push(out, [pstart as f32, pcount as f32, start2 as f32, count2 as f32]);
    push(out, [p.shell, p.poly_limit, p.poly_orient, if p.chamfer && p.round > 0.0 { 1.0 } else { 0.0 }]);
    push(out, [m.color2[0], m.color2[1], m.color2[2], if m.has_gradient() { m.gradient } else { 0.0 }]);
    push(out, [f32::from(m.grad_axis.min(2)) + if m.grad_local { 3.0 } else { 0.0 }, m.grad_soft, m.grad_from, m.grad_to]);
}

/// A curve's smoothed points go into the path list behind two header entries (the low and high
/// corner of a box around the tube). Returns (first point, count); (0, 0) when there is no room.
fn pack_curve(s: &crate::model::Shape, path: &mut Vec<f32>) -> (usize, usize) {
    let used = path.len() / 4;
    let room = PATH_V4.saturating_sub(used + 2);
    let mut pts = crate::curve::build(&s.path, &s.params);
    if pts.len() > room {
        pts = crate::curve::build_max(&s.path, &s.params, room);
    }
    if pts.len() < 2 || pts.len() > room {
        return (0, 0);
    }
    let (lo, hi) = crate::curve::bounds(&pts);
    push(path, [lo[0], lo[1], lo[2], 0.0]);
    push(path, [hi[0], hi[1], hi[2], 0.0]);
    let st = used + 2;
    for pt in &pts {
        push(path, *pt);
    }
    (st, pts.len())
}

/// A sphere around everything the shader can draw (centre, radius; radius 0 when there is nothing):
/// rays that miss it skip the marching. It includes the melt's reach, the drips and the selection.
pub fn bounds(scene: &Scene, selected: Option<Pick>) -> ([f32; 3], f32) {
    let mut items: Vec<([f32; 3], f32)> = Vec::new();
    for (gi, g) in scene.groups.iter().enumerate() {
        if !g.visible {
            continue;
        }
        for (si, s) in g.shapes.iter().enumerate() {
            let is_sel = match selected {
                Some(Pick::Shape(a, b)) => a == gi && b == si,
                Some(Pick::Group(a)) => a == gi,
                None => false,
            };
            if !s.visible || (!is_sel && (s.op != crate::model::Op::Add || g.op != crate::model::Op::Add)) {
                continue;
            }
            // a lamp's glow reaches a good way out
            let halo = if s.material.glow > 0.0 { 30.0 } else { 0.0 };
            let reach = (s.blend + g.blend) * scene.melt.amount * 1.5 + scene.melt.drip + 2.0 + halo;
            items.push((s.pos, crate::view::radius_of(s) + reach));
        }
    }
    if items.is_empty() {
        return ([0.0; 3], 0.0);
    }
    let n = items.len() as f32;
    let c = items.iter().fold([0.0; 3], |a, (p, _)| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]);
    let r = items.iter().map(|(p, r)| crate::sdf::len(crate::sdf::sub(*p, c)) + r).fold(0.0_f32, f32::max);
    (c, r)
}

/// Set the shader's flag word (`G.view.w`): see `perf::view_flags`.
pub fn set_view_flags(data: &mut [f32], flags: f32) {
    if let Some(dst) = data.get_mut(4 * 4 + 3) {
        *dst = flags;
    }
}

/// The scene list as a float texture: `ROW` vec4 per row.
struct Pool {
    texture: wgpu::Texture,
    rows: u32,
    bind: wgpu::BindGroup,
}

struct Resources {
    /// Draws the scene (into `target`).
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    scene_layout: wgpu::BindGroupLayout,
    /// The scene list; made (and made taller) when a scene needs it.
    pool: Option<Pool>,
    /// Copies `target` into egui's picture.
    blit: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    format: wgpu::TextureFormat,
    /// The finished picture of the scene. It stays until the scene changes, so a repaint that
    /// is only about the panels costs one textured triangle, not a ray march.
    target: Option<Target>,
    cache: crate::perf::FrameCache,
}

struct Target {
    view: wgpu::TextureView,
    size: [u32; 2],
    /// Smooth copy (the picture is smaller than the viewport) and exact copy (same size).
    linear: wgpu::BindGroup,
    nearest: wgpu::BindGroup,
}

/// The shader is one program in several files: the core, the extra solids, the CAD features.
pub fn shader_source() -> String {
    [include_str!("shader.wgsl"), include_str!("shader_prims.wgsl"), include_str!("shader_cad.wgsl"), include_str!("shader_curve.wgsl"), include_str!("shader_corner.wgsl")].join("\n")
}

const BLIT_SOURCE: &str = include_str!("blit.wgsl");

/// Build the pipelines (this is the slow part of start-up: the driver compiles the shader).
fn build(device: &wgpu::Device, format: wgpu::TextureFormat) -> Resources {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("yuracad"),
        source: wgpu::ShaderSource::Wgsl(shader_source().into()),
    });
    crate::perf::mark("shader module created");
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("yuracad"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("yuracad"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("yuracad"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
    crate::perf::mark("scene pipeline created");
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("yuracad"),
        contents: &vec![0u8; GLOBAL_BYTES],
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // the copy of the finished picture into egui's frame
    let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("yuracad blit"), source: wgpu::ShaderSource::Wgsl(BLIT_SOURCE.into()) });
    let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("yuracad blit"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let blit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("yuracad blit"),
        bind_group_layouts: &[Some(&blit_layout)],
        immediate_size: 0,
    });
    let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("yuracad blit"),
        layout: Some(&blit_pl),
        vertex: wgpu::VertexState { module: &blit_module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &blit_module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });
    let sampler = |filter| {
        device.create_sampler(&wgpu::SamplerDescriptor { label: Some("yuracad"), mag_filter: filter, min_filter: filter, ..Default::default() })
    };
    Resources {
        pipeline,
        buffer,
        scene_layout: bgl,
        pool: None,
        blit,
        blit_layout,
        linear: sampler(wgpu::FilterMode::Linear),
        nearest: sampler(wgpu::FilterMode::Nearest),
        format,
        target: None,
        cache: crate::perf::FrameCache::default(),
    }
}

/// Start building the pipeline once, at start-up. The compile runs on its own thread so that the
/// window can show first; the viewport stays empty until it is done and then a repaint is asked for.
pub fn install(rs: &egui_wgpu::RenderState, ctx: &egui::Context) {
    let device = rs.device.clone();
    let format = rs.target_format;
    let renderer = rs.renderer.clone();
    let ctx = ctx.clone();
    let job = Arc::new(move || {
        let res = build(&device, format);
        renderer.write().callback_resources.insert(res);
        crate::perf::mark("gpu ready");
        ctx.request_repaint();
    });
    let job2 = job.clone();
    if std::thread::Builder::new().name("yuracad-gpu-init".into()).spawn(move || job2()).is_err() {
        job();
    }
}

impl Pool {
    /// A scene-list texture at least `rows` tall (rounded up to a power of two so it is rarely remade).
    fn new(device: &wgpu::Device, res_buffer: &wgpu::Buffer, layout: &wgpu::BindGroupLayout, rows: u32) -> Pool {
        let rows = rows.max(16).next_power_of_two().min(device.limits().max_texture_dimension_2d.max(16));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("yuracad scene list"),
            size: wgpu::Extent3d { width: ROW as u32, height: rows, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("yuracad"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: res_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
            ],
        });
        Pool { texture, rows, bind }
    }
}

impl Target {
    fn new(device: &wgpu::Device, res: &Resources, size: [u32; 2]) -> Target {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("yuracad picture"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: res.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let group = |sampler: &wgpu::Sampler| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuracad blit"),
                layout: &res.blit_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            })
        };
        Target { linear: group(&res.linear), nearest: group(&res.nearest), view, size }
    }
}

/// One frame of the viewport, as an egui paint callback.
pub struct Callback {
    pub data: Arc<Vec<f32>>,
    /// The size of the picture to draw, in pixels (the viewport's size times the quality scale).
    pub size: [u32; 2],
}

impl egui_wgpu::CallbackTrait for Callback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        let size = crate::perf::render_size(self.size, 1.0, device.limits().max_texture_dimension_2d);
        if res.target.as_ref().is_none_or(|t| t.size != size) {
            res.target = Some(Target::new(device, res, size));
            res.cache.invalidate();
        }
        // the scene list needs a texture tall enough
        let list = self.data.get(GLOBAL_V4 * 4..).unwrap_or(&[]);
        let used_rows = (list.len() / 4).div_ceil(ROW).max(1) as u32;
        if res.pool.as_ref().is_none_or(|p| p.rows < used_rows) {
            res.pool = Some(Pool::new(device, &res.buffer, &res.scene_layout, used_rows));
            res.cache.invalidate();
        }
        // nothing changed since the picture was drawn: keep it
        if !res.cache.needs_render(&self.data, size) {
            return Vec::new();
        }
        let (Some(target), Some(pool)) = (res.target.as_ref(), res.pool.as_ref()) else { return Vec::new() };
        let mut bytes = Vec::with_capacity(GLOBAL_BYTES);
        for v in self.data.iter().take(GLOBAL_V4 * 4) {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.resize(GLOBAL_BYTES, 0);
        queue.write_buffer(&res.buffer, 0, &bytes);
        // whole rows of the list (the last one padded with zeros)
        let rows = used_rows.min(pool.rows);
        let mut texels: Vec<u8> = Vec::with_capacity(rows as usize * ROW * 16);
        for v in list.iter().take(rows as usize * ROW * 4) {
            texels.extend_from_slice(&v.to_le_bytes());
        }
        texels.resize(rows as usize * ROW * 16, 0);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &pool.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &texels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(ROW as u32 * 16), rows_per_image: Some(rows) },
            wgpu::Extent3d { width: ROW as u32, height: rows, depth_or_array_layers: 1 },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("yuracad scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &pool.bind, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::perf::mark_once(&FIRST, "first scene picture queued");
        Vec::new()
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        let Some(target) = res.target.as_ref() else { return };
        let vp = info.viewport_in_pixels();
        if vp.width_px <= 0 || vp.height_px <= 0 {
            return;
        }
        pass.set_viewport(vp.left_px as f32, vp.top_px as f32, vp.width_px as f32, vp.height_px as f32, 0.0, 1.0);
        pass.set_pipeline(&res.blit);
        let exact = target.size == [vp.width_px as u32, vp.height_px as u32];
        pass.set_bind_group(0, if exact { &target.nearest } else { &target.linear }, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader for a packed frame: where each list starts.
    pub(super) struct Frame<'a> {
        pub u: &'a [f32],
    }

    impl<'a> Frame<'a> {
        pub fn new(u: &'a [f32]) -> Frame<'a> {
            Frame { u }
        }
        fn g(&self, slot: usize) -> [f32; 4] {
            [self.u[slot * 4], self.u[slot * 4 + 1], self.u[slot * 4 + 2], self.u[slot * 4 + 3]]
        }
        /// vec4 number `i` of the scene list.
        pub fn d(&self, i: usize) -> [f32; 4] {
            self.g(GLOBAL_V4 + i)
        }
        pub fn counts(&self) -> [f32; 4] {
            self.g(6)
        }
        pub fn groups_base(&self) -> usize {
            self.g(V4_OFFS)[0] as usize
        }
        pub fn blocks_base(&self) -> usize {
            self.g(V4_OFFS)[1] as usize
        }
        pub fn shapes_base(&self) -> usize {
            self.g(V4_OFFS)[3] as usize
        }
        pub fn poly_base(&self) -> usize {
            self.g(V4_OFFS2)[0] as usize
        }
        pub fn path_base(&self) -> usize {
            self.g(V4_OFFS2)[1] as usize
        }
        /// Slot `k` (0 = bounding sphere, 1.. = struct Shape) of shape `i`.
        pub fn shape(&self, i: usize, k: usize) -> [f32; 4] {
            self.d(self.shapes_base() + i * S_STRIDE + k)
        }
        pub fn list_len(&self) -> usize {
            self.u.len() / 4 - GLOBAL_V4
        }
    }

    #[test]
    fn the_default_scene_packs_without_overflow() {
        let u = pack(&Scene::default(), &Camera::default(), [800.0, 600.0], Some(Pick::Shape(0, 0)), None, true);
        let f = Frame::new(&u);
        assert_eq!(f.counts(), [1.0, 1.0, 0.0, 1.0], "one group, one shape, shape 0 selected");
        let e = V4_ENV * 4;
        assert!((u[e] - 1.0).abs() < 1e-6 && (u[e + 7] - 2.0).abs() < 1e-6, "exposure 1, two lights");
        // one group with one block holding the one shape; the shape's sphere is first in its record
        let g = f.d(f.groups_base());
        assert_eq!((g[0], g[1], g[2]), (0.0, 1.0, 1.0));
        let b = f.d(f.blocks_base() + 1);
        assert_eq!((b[2], b[3]), (0.0, 1.0));
        assert_eq!(f.shape(0, 0)[..3], [0.0, 0.0, 20.0]);
        assert_eq!(f.shape(0, 4)[..3], [0.0, 0.0, 20.0], "the position slot of struct Shape");
        assert_eq!(f.shape(0, 5)[3], 0.0, "a sphere is kind 0");
        assert!(f.list_len() < ROW, "a tiny scene is a tiny list");
    }

    #[test]
    fn a_big_scene_packs_all_its_shapes() {
        let mut sc = Scene::default();
        let base = sc.groups[0].shapes[0].clone();
        sc.groups[0].shapes.clear();
        for gi in 1..crate::model::MAX_GROUPS {
            sc.groups.push(crate::model::Group::new(format!("g{gi}")));
        }
        for i in 0..crate::model::MAX_SHAPES {
            let mut s = base.clone();
            s.pos = [i as f32 * 3.0, (i % 96) as f32 * 5.0, 10.0];
            sc.groups[i % crate::model::MAX_GROUPS].shapes.push(s);
        }
        assert_eq!(sc.shape_count(), crate::model::MAX_SHAPES);
        let u = pack(&sc, &Camera::default(), [800.0, 600.0], None, None, true);
        let f = Frame::new(&u);
        assert_eq!(f.counts()[0] as usize, crate::model::MAX_GROUPS);
        assert_eq!(f.counts()[1] as usize, crate::model::MAX_SHAPES);
        // every shape has its record, with its position, and every block points at real shapes
        for i in 0..crate::model::MAX_SHAPES {
            assert_eq!(f.shape(i, 0)[3] > 0.0, true, "shape {i} has a radius");
            assert_eq!(f.shape(i, 4)[0], f.shape(i, 0)[0]);
        }
        let mut covered = 0;
        for gi in 0..crate::model::MAX_GROUPS {
            let g = f.d(f.groups_base() + gi * G_STRIDE);
            for bi in 0..g[1] as usize {
                let b = f.d(f.blocks_base() + (g[0] as usize + bi) * B_STRIDE + 1);
                assert!(b[3] >= 1.0 && b[3] <= crate::accel::BLOCK as f32);
                assert_eq!(b[2] as usize, covered, "blocks follow each other through the shape list");
                covered += b[3] as usize;
            }
        }
        assert_eq!(covered, crate::model::MAX_SHAPES);
        assert!(f.list_len() <= MAX_POOL_V4);
    }

    #[test]
    fn hover_and_selection_name_packed_shapes() {
        let mut sc = Scene::default();
        let mut g2 = crate::model::Group::new("b");
        g2.shapes.push(sc.groups[0].shapes[0].clone());
        g2.shapes.push(sc.groups[0].shapes[0].clone());
        sc.groups.push(g2);
        let u = pack(&sc, &Camera::default(), [8.0, 6.0], Some(Pick::Group(1)), Some(Pick::Shape(1, 1)), true);
        let f = Frame::new(&u);
        assert_eq!(&f.counts()[2..], &[1.0, 2.0], "the second group is packed shapes 1 and 2");
        assert_eq!(&u[V4_HOVER * 4..V4_HOVER * 4 + 2], &[2.0, 1.0]);
    }
}

#[cfg(test)]
mod curve_tests {
    use super::tests::Frame;
    use super::*;
    use crate::model::{Group, Kind, Material, Shape};

    fn scene_with_curves(n: usize) -> Scene {
        let mut g = Group::new("g");
        for i in 0..n {
            let mut s = Shape::new(Kind::Curve, [i as f32 * 3.0, 0.0, 10.0], Material::default());
            s.path = (0..32).map(|k| [k as f32 * 2.0, (k * k) as f32 * 0.1, 0.0, 3.0]).collect();
            g.shapes.push(s);
        }
        Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new() }
    }

    #[test]
    fn a_curve_is_packed_as_a_box_then_its_smooth_points() {
        let scene = scene_with_curves(1);
        let u = pack(&scene, &Camera::default(), [800.0, 600.0], None, None, true);
        let f = Frame::new(&u);
        let ext = f.shape(0, 12);
        let (start, count) = (ext[0] as usize, ext[1] as usize);
        assert!((2..=crate::curve::MAX_POINTS).contains(&count) && start == 2);
        let at = |slot: usize| f.d(f.path_base() + slot);
        let (lo, hi) = (at(start - 2), at(start - 1));
        for k in 0..count {
            let p = at(start + k);
            for i in 0..3 {
                assert!(p[i] >= lo[i] && p[i] <= hi[i], "point {k} axis {i} outside the box");
            }
        }
        let want = crate::curve::build(&scene.groups[0].shapes[0].path, &scene.groups[0].shapes[0].params);
        assert_eq!(want.len(), count);
        assert_eq!(at(start), want[0]);
    }

    #[test]
    fn many_curves_fit_and_too_many_never_overflow_the_path_list() {
        let scene = scene_with_curves(crate::model::MAX_SHAPES);
        let u = pack(&scene, &Camera::default(), [800.0, 600.0], None, None, true);
        let f = Frame::new(&u);
        let mut used = 0;
        let mut drawn = 0;
        for i in 0..scene.shape_count() {
            let ext = f.shape(i, 12);
            let (start, count) = (ext[0] as usize, ext[1] as usize);
            if count > 0 {
                assert!(start + count <= PATH_V4, "curve {i} runs past the list");
                used = used.max(start + count);
                drawn += 1;
            }
        }
        assert!(used > 0 && drawn >= 100, "{drawn} curves fit");
        assert!(f.list_len() <= MAX_POOL_V4);
    }

    #[test]
    fn outlines_share_one_pool_and_never_overflow_it() {
        let mut g = Group::new("g");
        for i in 0..crate::model::MAX_SHAPES {
            let mut s = Shape::new(Kind::Loft, [i as f32, 0.0, 0.0], Material::default());
            s.outline = (0..crate::model::MAX_OUTLINE).map(|k| [k as f32, (k * k % 7) as f32]).collect();
            s.outline2 = s.outline.clone();
            g.shapes.push(s);
        }
        let scene = Scene { groups: vec![g], melt: Default::default(), env: Default::default(), saved: Vec::new() };
        let u = pack(&scene, &Camera::default(), [800.0, 600.0], None, None, true);
        let f = Frame::new(&u);
        let mut points = 0;
        for i in 0..scene.shape_count() {
            let (a, ca) = (f.shape(i, 7)[2] as usize, f.shape(i, 7)[3] as usize);
            let (b, cb) = (f.shape(i, 12)[2] as usize, f.shape(i, 12)[3] as usize);
            for (st, n) in [(a, ca), (b, cb)] {
                if n > 0 {
                    assert!(st + n <= POLY_PTS, "outline of shape {i} runs past the pool");
                    points = points.max(st + n);
                    // the first point is where the pool says
                    let v = f.d(f.poly_base() + st / 2);
                    let off = (st % 2) * 2;
                    assert_eq!([v[off], v[off + 1]], [0.0, 0.0], "point 0 of the profile is (0, 0)");
                }
            }
        }
        assert!(points > POLY_PTS / 2, "the pool is used well past the old 1024 points ({points})");
    }
}

#[cfg(test)]
mod shader_tests {
    use super::*;

    /// The shader must parse and validate, or the viewport shows nothing.
    #[test]
    fn the_shader_is_valid_wgsl() {
        let src = shader_source();
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        if let Err(e) = v.validate(&module) {
            panic!("{}", e.emit_to_string(&src));
        }
    }

    /// The uniform buffer must be exactly as big as the shader's `Globals` struct, or wgpu rejects
    /// the bind group (and the viewport never draws).
    #[test]
    fn the_shader_struct_is_as_big_as_the_uniform_buffer() {
        let src = shader_source();
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let size = module.types.iter().find_map(|(_, t)| match (&t.name, &t.inner) {
            (Some(name), naga::TypeInner::Struct { span, .. }) if name == "Globals" => Some(*span as usize),
            _ => None,
        });
        assert_eq!(size, Some(GLOBAL_BYTES));
    }

    /// The scene list is a float texture read with textureLoad: a binding every backend (also
    /// OpenGL, where egui-wgpu allows no storage buffers) can give, and `Globals` stays small.
    #[test]
    fn the_scene_list_is_a_texture_not_a_uniform_array() {
        let src = shader_source();
        assert!(src.contains("textureLoad(DT"), "scene data is read from the list texture");
        assert!(!src.contains("array<Shape"), "no shape array in the uniform");
        assert!(GLOBAL_BYTES <= 16384, "the globals fit the smallest uniform limit (WebGL2/GL: 16 KB), {GLOBAL_BYTES}");
        assert!(ROW.is_power_of_two() && src.contains("& 1023") && src.contains(">> 10"), "ROW matches the shader's row width");
    }

    #[test]
    fn the_blit_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(BLIT_SOURCE).unwrap_or_else(|e| panic!("{}", e.emit_to_string(BLIT_SOURCE)));
        let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
        if let Err(e) = v.validate(&module) {
            panic!("{}", e.emit_to_string(BLIT_SOURCE));
        }
    }

    #[test]
    fn the_limits_are_what_the_readme_says() {
        assert!(crate::model::MAX_SHAPES >= 512 && crate::model::MAX_GROUPS >= 96);
        assert!(MAX_POOL_V4 <= 2048 * ROW);
        assert!(POLY_PTS >= 16 * 1024 && PATH_V4 >= 16 * 1024, "much bigger outline and path pools");
    }
}
