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
/// vec4 per group, per block of shapes, per instance, per shape (a bounding sphere and the 15 of `struct Shape`).
const G_STRIDE: usize = 3;
const B_STRIDE: usize = 2;
const I_STRIDE: usize = 4;
const S_STRIDE: usize = 16;
/// Outline points the lists hold (two points per vec4) and path nodes (one per vec4).
pub const POLY_PTS: usize = 65536;
pub const PATH_V4: usize = 32768;
/// The longest the scene list can ever be (vec4), well inside a 2048-row texture.
pub const MAX_POOL_V4: usize = MAX_GROUPS * G_STRIDE + MAX_SHAPES * (S_STRIDE + B_STRIDE) + (crate::pattern::MAX_INSTANCES + MAX_GROUPS) * I_STRIDE + POLY_PTS / 2 + PATH_V4 + 4096;
const _: () = assert!(MAX_POOL_V4 <= 2048 * ROW, "the scene list must fit a 2048 x 1024 texture");

/// The volume texture: every imported mesh's distances, half floats, four grid nodes to a texel
/// (`VOL_W` texels to a row, at most `VOL_H` rows), read with `textureLoad` in `shader_mesh.wgsl`.
pub const VOL_W: usize = 2048;
pub const VOL_H: usize = 2048;

/// The imported meshes of a scene as they sit in the volume texture: one after another, each one's
/// first node a multiple of 4. Shapes that share a volume share its place.
#[derive(Clone, Default, Debug)]
pub struct VolumeSet {
    /// (the volume, the number of its first node in the texture)
    pub items: Vec<(crate::mesh::SharedVolume, usize)>,
    /// Nodes in use, including the padding between volumes.
    pub total: usize,
}

impl VolumeSet {
    /// The volumes of all the mesh shapes of the scene (hidden ones too, so that hiding a shape
    /// does not move anything). A volume that no longer fits the texture is left out: its shapes
    /// draw nothing.
    pub fn of(scene: &Scene) -> VolumeSet {
        let mut set = VolumeSet::default();
        for sh in scene.groups.iter().flat_map(|g| g.shapes.iter()) {
            let (crate::model::Kind::Mesh, Some(v)) = (sh.kind, &sh.volume) else { continue };
            if set.items.iter().any(|(w, _)| w.key() == v.key()) {
                continue;
            }
            let base = set.total.next_multiple_of(4);
            if v.nodes() == 0 || base + v.nodes() > VOL_W * VOL_H * 4 {
                continue;
            }
            set.items.push((v.clone(), base));
            set.total = base + v.nodes();
        }
        set
    }
    /// Where a volume starts in the texture (None = not in the texture).
    pub fn base_of(&self, v: &crate::mesh::SharedVolume) -> Option<usize> {
        self.items.iter().find(|(w, _)| w.key() == v.key()).map(|(_, b)| *b)
    }
    /// Texture rows this set needs.
    pub fn rows(&self) -> u32 {
        (self.total.div_ceil(4).div_ceil(VOL_W)).max(1) as u32
    }
    /// Is it the same layout of the same volumes?
    pub fn same_as(&self, other: &VolumeSet) -> bool {
        self.items.len() == other.items.len() && self.items.iter().zip(&other.items).all(|(a, b)| a.0 == b.0 && a.1 == b.1)
    }
}

fn put(u: &mut [f32], slot: usize, v: [f32; 4]) {
    if let Some(dst) = u.get_mut(slot * 4..slot * 4 + 4) {
        dst.copy_from_slice(&v);
    }
}

fn push(list: &mut Vec<f32>, v: [f32; 4]) {
    list.extend_from_slice(&v);
}

/// Everything the shader needs for one frame: `GLOBAL_V4` vec4 of globals, then the scene list.
#[cfg_attr(not(test), allow(dead_code))]
pub fn pack(scene: &Scene, cam: &Camera, size: [f32; 2], selected: Option<Pick>, hover: Option<Pick>, srgb: bool) -> Vec<f32> {
    pack_with(scene, cam, size, selected, hover, srgb, &VolumeSet::of(scene))
}

/// `pack`, with the volumes' places in the volume texture given (`VolumeSet::of(scene)`).
pub fn pack_with(scene: &Scene, cam: &Camera, size: [f32; 2], selected: Option<Pick>, hover: Option<Pick>, srgb: bool, vols: &VolumeSet) -> Vec<f32> {
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
    let resolved = crate::pattern::resolve(scene);
    let mut groups: Vec<f32> = Vec::new();
    let mut blocks: Vec<f32> = Vec::new();
    let mut insts: Vec<f32> = Vec::new();
    let mut shapes: Vec<f32> = Vec::new();
    let mut poly: Vec<f32> = Vec::new();
    let mut path: Vec<f32> = Vec::new();
    let mut shape_n = 0usize;
    let mut block_n = 0usize;
    let mut inst_n = 0usize;
    let mut sel_first = -1.0_f32;
    let mut sel_count = 0.0_f32;
    let mut hover_first = -1.0_f32;
    let mut hover_count = 0.0_f32;
    // every group whose shapes are drawn (itself, or as the source of a linked copy) is packed once
    let mut srcs: Vec<usize> = resolved.iter().map(|r| r.src).collect();
    srcs.sort_unstable();
    srcs.dedup();
    /// Where a source group's blocks went: its first block, the number of blocks, its sphere.
    struct Packed {
        first_block: usize,
        n_blocks: usize,
        sphere: crate::accel::Bound,
    }
    let mut packed: std::collections::HashMap<usize, Packed> = std::collections::HashMap::new();
    for gi in srcs {
        let Some(g) = scene.groups.get(gi) else { continue };
        let first = shape_n;
        let mut members: Vec<&crate::model::Shape> = Vec::new();
        for (si, s) in g.shapes.iter().enumerate() {
            if !s.visible || shape_n >= MAX_SHAPES {
                continue;
            }
            pack_shape(s, &mut shapes, &mut poly, &mut path, vols);
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
        if count == 0 {
            continue;
        }
        let info = crate::accel::group_info(&members, &scene.melt);
        for b in &info.blocks {
            push(&mut blocks, [b.bound.c[0], b.bound.c[1], b.bound.c[2], b.bound.r]);
            push(&mut blocks, [b.kmax, if b.skippable { 0.0 } else { 1.0 }, (first + b.first) as f32, b.count as f32]);
        }
        packed.insert(gi, Packed { first_block: block_n, n_blocks: info.blocks.len(), sphere: info.bound });
        block_n += info.blocks.len();
    }
    let mut group_n = 0usize;
    for rg in &resolved {
        let (Some(g), Some(p)) = (scene.groups.get(rg.gi), packed.get(&rg.src)) else { continue };
        if group_n >= MAX_GROUPS {
            break;
        }
        let first_inst = inst_n;
        for i in &rg.insts {
            if inst_n >= crate::pattern::MAX_INSTANCES + MAX_GROUPS {
                break;
            }
            let rows = i.back_rows();
            for r in rows {
                push(&mut insts, r);
            }
            let c = i.bound(p.sphere).c;
            push(&mut insts, [c[0], c[1], c[2], i.s]);
            inst_n += 1;
        }
        push(&mut groups, [p.first_block as f32, p.n_blocks as f32, g.op.sign(), g.blend]);
        push(&mut groups, [first_inst as f32, (inst_n - first_inst) as f32, 0.0, 0.0]);
        push(&mut groups, [p.sphere.c[0], p.sphere.c[1], p.sphere.c[2], p.sphere.r]);
        group_n += 1;
    }
    // the sections follow each other: groups, blocks, instances, shapes, outline points, path nodes
    let v4 = |l: &Vec<f32>| l.len() / 4;
    let block_base = v4(&groups);
    let inst_base = block_base + v4(&blocks);
    let shape_base = inst_base + v4(&insts);
    while poly.len() % 4 != 0 {
        poly.push(0.0);
    }
    let poly_base = shape_base + v4(&shapes);
    let path_base = poly_base + v4(&poly);
    put(&mut u, 6, [group_n as f32, shape_n as f32, sel_first, sel_count]);
    put(&mut u, V4_OFFS, [0.0, block_base as f32, inst_base as f32, shape_base as f32]);
    put(&mut u, V4_OFFS2, [poly_base as f32, path_base as f32, 0.0, 0.0]);
    let (bc, br) = bounds_of(scene, selected, &resolved);
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
    u.reserve(groups.len() + blocks.len() + insts.len() + shapes.len() + poly.len() + path.len());
    u.extend_from_slice(&groups);
    u.extend_from_slice(&blocks);
    u.extend_from_slice(&insts);
    u.extend_from_slice(&shapes);
    u.extend_from_slice(&poly);
    u.extend_from_slice(&path);
    u
}

/// One shape's 16 vec4 (a bounding sphere, then `struct Shape`); its outline points and path nodes
/// go into the shared pools.
fn pack_shape(s: &crate::model::Shape, out: &mut Vec<f32>, poly: &mut Vec<f32>, path: &mut Vec<f32>, vols: &VolumeSet) {
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
    // a mesh: size = nodes along x, y, z (0 = nothing to draw), params = place of node 0 and the cell, ext.x = first node
    let (mut size4, mut params4, mut ext4) = ([s.size[0], s.size[1], s.size[2], p.round], s.params, [pstart as f32, pcount as f32, start2 as f32, count2 as f32]);
    if s.kind == crate::model::Kind::Mesh {
        let found = s.volume.as_ref().and_then(|v| vols.base_of(v).map(|b| (v, b)));
        (size4, params4, ext4) = match found {
            Some((v, base)) => ([v.dims[0] as f32, v.dims[1] as f32, v.dims[2] as f32, 0.0], [v.origin[0], v.origin[1], v.origin[2], v.cell], [base as f32, 0.0, 0.0, 0.0]),
            None => ([0.0; 4], [0.0; 4], [0.0; 4]),
        };
    }
    push(out, [s.pos[0], s.pos[1], s.pos[2], radius]);
    push(out, [p.inv[0][0], p.inv[0][1], p.inv[0][2], 0.0]);
    push(out, [p.inv[1][0], p.inv[1][1], p.inv[1][2], 0.0]);
    push(out, [p.inv[2][0], p.inv[2][1], p.inv[2][2], 0.0]);
    push(out, [s.pos[0], s.pos[1], s.pos[2], p.min_scale()]);
    push(out, [s.scale[0], s.scale[1], s.scale[2], s.kind.id()]);
    push(out, size4);
    push(out, [s.op.sign(), s.blend, start as f32, count as f32]);
    let m = &s.material;
    push(out, [m.color[0], m.color[1], m.color[2], m.roughness]);
    push(out, [m.metallic, m.glow, m.glass, m.iridescence]);
    // the gradient: slot 13 = second colour + strength, slot 14 = direction (+ 3 when it follows
    // the shape's own box), softness, from, to; mat2.z = signed 1 / the gradient's length
    let inv_len = if m.has_gradient() { (if m.grad_flip { -0.5 } else { 0.5 }) / p.grad_half.max(1e-3) } else { 0.0 };
    push(out, [m.coat, m.translucency, inv_len, 0.0]);
    push(out, params4);
    push(out, ext4);
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
    bounds_of(scene, selected, &crate::pattern::resolve(scene))
}

/// `bounds` for groups that are already resolved (`pattern::resolve`).
fn bounds_of(scene: &Scene, selected: Option<Pick>, resolved: &[crate::pattern::RGroup]) -> ([f32; 3], f32) {
    use crate::accel::{Bound, enclose};
    let mut items: Vec<Bound> = Vec::new();
    for rg in resolved {
        let (Some(g), Some(src)) = (scene.groups.get(rg.gi), scene.groups.get(rg.src)) else { continue };
        let mut parts: Vec<Bound> = Vec::new();
        for (si, s) in src.shapes.iter().enumerate() {
            let is_sel = match selected {
                Some(Pick::Shape(a, b)) => !rg.linked && a == rg.gi && b == si,
                Some(Pick::Group(a)) => a == rg.gi,
                None => false,
            };
            if !s.visible || (!is_sel && (s.op != crate::model::Op::Add || g.op != crate::model::Op::Add)) {
                continue;
            }
            // a lamp's glow reaches a good way out
            let halo = if s.material.glow > 0.0 { 30.0 } else { 0.0 };
            let reach = (s.blend + g.blend) * scene.melt.amount * 1.5 + scene.melt.drip + 2.0 + halo;
            parts.push(Bound { c: s.pos, r: crate::view::radius_of(s) + reach });
        }
        if parts.is_empty() {
            continue;
        }
        let base = enclose(&parts);
        if rg.insts.is_empty() {
            items.push(base);
        } else {
            items.extend(rg.insts.iter().map(|i| i.bound(base)));
        }
    }
    if items.is_empty() {
        return ([0.0; 3], 0.0);
    }
    let n = items.len() as f32;
    let c = items.iter().fold([0.0; 3], |a, b| [a[0] + b.c[0] / n, a[1] + b.c[1] / n, a[2] + b.c[2] / n]);
    let r = items.iter().map(|b| crate::sdf::len(crate::sdf::sub(b.c, c)) + b.r).fold(0.0_f32, f32::max);
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
    view: wgpu::TextureView,
    rows: u32,
    bind: wgpu::BindGroup,
}

/// The volume texture and what is in it.
struct VolTex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    rows: u32,
    /// What was uploaded (and so is in the texture now).
    resident: VolumeSet,
}

pub(crate) struct Resources {
    /// Draws the scene (into `target`).
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    scene_layout: wgpu::BindGroupLayout,
    /// The scene list; made (and made taller) when a scene needs it.
    pool: Option<Pool>,
    /// The imported meshes' distances (a 1-row dummy when there are none).
    vol: VolTex,
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
    [
        include_str!("shader.wgsl"),
        include_str!("shader_prims.wgsl"),
        include_str!("shader_cad.wgsl"),
        include_str!("shader_curve.wgsl"),
        include_str!("shader_corner.wgsl"),
        include_str!("shader_mesh.wgsl"),
    ].join("\n")
}

const BLIT_SOURCE: &str = include_str!("blit.wgsl");

/// Build the pipelines (this is the slow part of start-up: the driver compiles the shader).
fn build(device: &wgpu::Device, format: wgpu::TextureFormat) -> Resources {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("meltcad"),
        source: wgpu::ShaderSource::Wgsl(shader_source().into()),
    });
    crate::perf::mark("shader module created");
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("meltcad"),
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
            // the imported meshes' distances (half floats, read with textureLoad)
            wgpu::BindGroupLayoutEntry {
                binding: 2,
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
        label: Some("meltcad"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("meltcad"),
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
        label: Some("meltcad"),
        contents: &vec![0u8; GLOBAL_BYTES],
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // the copy of the finished picture into egui's frame
    let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("meltcad blit"), source: wgpu::ShaderSource::Wgsl(BLIT_SOURCE.into()) });
    let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("meltcad blit"),
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
        label: Some("meltcad blit"),
        bind_group_layouts: &[Some(&blit_layout)],
        immediate_size: 0,
    });
    let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("meltcad blit"),
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
        device.create_sampler(&wgpu::SamplerDescriptor { label: Some("meltcad"), mag_filter: filter, min_filter: filter, ..Default::default() })
    };
    Resources {
        pipeline,
        buffer,
        scene_layout: bgl,
        pool: None,
        vol: VolTex::new(device, 1),
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
    // (OpenGL keeps one context lock for the whole compile: egui's own texture uploads on the main
    // thread would then wait on it and give up, so there the compile runs right here)
    let on_gl = rs.adapter.get_info().backend == wgpu::Backend::Gl;
    if on_gl || std::thread::Builder::new().name("meltcad-gpu-init".into()).spawn(move || job2()).is_err() {
        job();
    }
}

impl Pool {
    /// A scene-list texture at least `rows` tall (rounded up to a power of two so it is rarely remade).
    fn new(device: &wgpu::Device, res_buffer: &wgpu::Buffer, layout: &wgpu::BindGroupLayout, vol: &wgpu::TextureView, rows: u32) -> Pool {
        let rows = rows.max(16).next_power_of_two().min(device.limits().max_texture_dimension_2d.max(16));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("meltcad scene list"),
            size: wgpu::Extent3d { width: ROW as u32, height: rows, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = Pool::bind(device, res_buffer, layout, &view, vol);
        Pool { texture, view, rows, bind }
    }

    fn bind(device: &wgpu::Device, res_buffer: &wgpu::Buffer, layout: &wgpu::BindGroupLayout, list: &wgpu::TextureView, vol: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("meltcad"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: res_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(list) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(vol) },
            ],
        })
    }
}

impl VolTex {
    /// A volume texture `rows` rows tall (empty).
    fn new(device: &wgpu::Device, rows: u32) -> VolTex {
        let rows = rows.clamp(1, VOL_H as u32);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("meltcad volumes"),
            size: wgpu::Extent3d { width: VOL_W as u32, height: rows, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        VolTex { texture, view, rows, resident: VolumeSet::default() }
    }

    /// Write `bytes` (little-endian half floats, 8 bytes to a texel) starting at texel `first`.
    fn write_texels(&self, queue: &wgpu::Queue, first: usize, bytes: &[u8]) {
        let total = bytes.len() / 8;
        let mut done = 0usize;
        while done < total {
            let t = first + done;
            let (x, y) = (t % VOL_W, t / VOL_W);
            let n = (VOL_W - x).min(total - done);
            // (a run to the end of the row, then whole rows, in one write when it can be)
            let rows_whole = if x == 0 { (total - done) / VOL_W } else { 0 };
            let (w, h) = if rows_whole > 0 { (VOL_W, rows_whole) } else { (n, 1) };
            if y as u32 + h as u32 > self.rows {
                return;
            }
            let part = bytes.get(done * 8..(done + w * h) * 8).unwrap_or(&[]);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x: x as u32, y: y as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
                part,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w as u32 * 8), rows_per_image: Some(h as u32) },
                wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
            );
            done += w * h;
        }
    }
}

/// Make the volume texture hold exactly `set`: only volumes that are new or moved are uploaded
/// (a frame that only moves a mesh uploads nothing). The picture must be drawn again afterwards.
fn ensure_volumes(res: &mut Resources, device: &wgpu::Device, queue: &wgpu::Queue, set: &VolumeSet) {
    if res.vol.resident.same_as(set) {
        return;
    }
    let rows = set.rows().next_power_of_two().min(VOL_H as u32);
    let fresh = rows != res.vol.rows;
    if fresh {
        res.vol = VolTex::new(device, rows);
        if let Some(p) = res.pool.as_mut() {
            p.bind = Pool::bind(device, &res.buffer, &res.scene_layout, &p.view, &res.vol.view);
        }
    }
    for (v, base) in &set.items {
        let there = !fresh && res.vol.resident.items.iter().any(|(w, b)| w == v && b == base);
        if there {
            continue;
        }
        let mut bytes: Vec<u8> = Vec::with_capacity(v.nodes().next_multiple_of(4) * 2);
        for h in v.to_f16_bits() {
            bytes.extend_from_slice(&h.to_le_bytes());
        }
        bytes.resize(v.nodes().next_multiple_of(4) * 2, 0);
        res.vol.write_texels(queue, base / 4, &bytes);
    }
    res.vol.resident = set.clone();
    res.cache.invalidate();
}

impl Target {
    fn new(device: &wgpu::Device, res: &Resources, size: [u32; 2]) -> Target {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("meltcad picture"),
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
                label: Some("meltcad blit"),
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
    /// The imported meshes the frame refers to (`VolumeSet::of`, as given to `pack_with`).
    pub volumes: Arc<VolumeSet>,
    /// The size of the picture to draw, in pixels (the viewport's size times the quality scale).
    pub size: [u32; 2],
}

/// Make the scene-list texture tall enough for `data` (a frame from `pack`).
fn ensure_pool(res: &mut Resources, device: &wgpu::Device, data: &[f32]) {
    let list_len = data.len().saturating_sub(GLOBAL_V4 * 4);
    let used_rows = (list_len / 4).div_ceil(ROW).max(1) as u32;
    if res.pool.as_ref().is_none_or(|p| p.rows < used_rows) {
        res.pool = Some(Pool::new(device, &res.buffer, &res.scene_layout, &res.vol.view, used_rows));
        res.cache.invalidate();
    }
}

/// Upload a frame (globals into the uniform buffer, the scene list into the texture) and draw it
/// into `view`. `ensure_pool` must have been called for `data`.
fn draw(res: &Resources, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, data: &[f32]) {
    let Some(pool) = res.pool.as_ref() else { return };
    let mut bytes = Vec::with_capacity(GLOBAL_BYTES);
    for v in data.iter().take(GLOBAL_V4 * 4) {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.resize(GLOBAL_BYTES, 0);
    queue.write_buffer(&res.buffer, 0, &bytes);
    // whole rows of the list (the last one padded with zeros)
    let list = data.get(GLOBAL_V4 * 4..).unwrap_or(&[]);
    let rows = ((list.len() / 4).div_ceil(ROW).max(1) as u32).min(pool.rows);
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
        label: Some("meltcad scene"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
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
        ensure_volumes(res, device, queue, &self.volumes);
        ensure_pool(res, device, &self.data);
        // nothing changed since the picture was drawn: keep it
        if !res.cache.needs_render(&self.data, size) {
            return Vec::new();
        }
        let Some(target) = res.target.as_ref() else { return Vec::new() };
        draw(res, queue, encoder, &target.view, &self.data);
        static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::perf::mark_once(&FIRST, "first scene picture queued");
        crate::perf::mark_interval("scene picture queued");
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
        /// The k-th start in the globals' offsets: 0 groups, 1 blocks, 2 instances, 3 shapes.
        pub fn g_offs(&self, k: usize) -> usize {
            self.g(V4_OFFS)[k] as usize
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
mod instance_tests {
    use super::tests::Frame;
    use super::*;
    use crate::model::{Group, Kind, Material, Shape};
    use crate::pattern::{Pattern, Xform};

    fn ball(x: f32) -> Shape {
        Shape::new(Kind::Sphere, [x, 0.0, 0.0], Material::default())
    }

    fn instances_of(f: &Frame, group: usize) -> Vec<[[f32; 4]; 4]> {
        let g1 = f.d(f.groups_base() + group * G_STRIDE + 1);
        let inst_base = f.g_offs(2);
        (0..g1[1] as usize).map(|j| [0, 1, 2, 3].map(|k| f.d(inst_base + (g1[0] as usize + j) * I_STRIDE + k))).collect()
    }

    #[test]
    fn a_pattern_group_packs_its_instances_not_more_shapes() {
        let mut sc = Scene::default();
        sc.groups[0].shapes[0].pos = [10.0, 0.0, 0.0];
        sc.groups[0].pattern = Some(Pattern::Rect { counts: [5, 4, 3], spacing: [30.0, 30.0, 30.0] });
        let u = pack(&sc, &Camera::default(), [800.0, 600.0], None, None, true);
        let f = Frame::new(&u);
        assert_eq!(f.counts()[1], 1.0, "still one shape in the list");
        let inst = instances_of(&f, 0);
        assert_eq!(inst.len(), 60);
        // the rows carry a world point back into the group: instance 7 is moved by (60, 30, 0)
        let (i, w) = (&inst[7], [70.0, 30.0, 0.0]);
        let q = [0, 1, 2].map(|k| i[k][0] * w[0] + i[k][1] * w[1] + i[k][2] * w[2] + i[k][3]);
        assert!((q[0] - 10.0).abs() < 1e-4 && q[1].abs() < 1e-4 && q[2].abs() < 1e-4, "{q:?}");
        assert_eq!(i[3][3], 1.0, "scale 1");
        // the sphere around the group, moved by the instance, is in the record's last vec4
        assert!((i[3][0] - 70.0).abs() < 20.0 && (i[3][1] - 30.0).abs() < 20.0);
        // a plain group has none
        let plain = pack(&Scene::default(), &Camera::default(), [8.0, 6.0], None, None, true);
        assert!(instances_of(&Frame::new(&plain), 0).is_empty());
        assert!(f.list_len() < ROW * 3);
    }

    #[test]
    fn a_linked_copy_points_at_the_sources_blocks() {
        let mut sc = Scene::default();
        let uid = sc.ensure_group_uid(0).unwrap_or(0);
        let mut copy = Group::new("copy");
        copy.instance_of = Some(uid);
        copy.xform = Xform { pos: [0.0, 50.0, 0.0], rot: [0.0; 3], scale: 2.0 };
        sc.groups.push(copy);
        let u = pack(&sc, &Camera::default(), [800.0, 600.0], Some(Pick::Group(1)), None, true);
        let f = Frame::new(&u);
        assert_eq!(f.counts()[0], 2.0, "two groups to draw");
        assert_eq!(f.counts()[1], 1.0, "one shape in the list");
        let (a, b) = (f.d(f.groups_base()), f.d(f.groups_base() + G_STRIDE));
        assert_eq!((a[0], a[1]), (b[0], b[1]), "both use the same blocks");
        assert_eq!(instances_of(&f, 1).len(), 1);
        assert_eq!(instances_of(&f, 1)[0][3][3], 2.0);
        assert_eq!(f.counts()[3], 0.0, "a linked copy has no shapes to outline");
        // a hidden source still draws its copies
        sc.groups[0].visible = false;
        let u = pack(&sc, &Camera::default(), [800.0, 600.0], None, None, true);
        let f = Frame::new(&u);
        assert_eq!((f.counts()[0], f.counts()[1]), (1.0, 1.0));
        // a copy of a group that is gone draws nothing
        sc.groups.remove(0);
        let u = pack(&sc, &Camera::default(), [800.0, 600.0], None, None, true);
        assert_eq!(Frame::new(&u).counts()[0], 0.0);
    }

    #[test]
    fn a_scene_full_of_patterns_stays_inside_the_list_and_the_limits() {
        let mut sc = Scene::default();
        sc.groups.clear();
        for g in 0..crate::model::MAX_GROUPS {
            let mut gr = Group::new(format!("g{g}"));
            for k in 0..5 {
                gr.shapes.push(ball(k as f32 * 7.0));
            }
            gr.pattern = Some(Pattern::Rect { counts: [20, 20, 20], spacing: [30.0, 30.0, 30.0] });
            sc.groups.push(gr);
        }
        let u = pack(&sc, &Camera::default(), [800.0, 600.0], None, None, true);
        let f = Frame::new(&u);
        let total: usize = (0..crate::model::MAX_GROUPS).map(|g| instances_of(&f, g).len()).sum();
        assert!(total >= crate::pattern::MAX_INSTANCES && total <= crate::pattern::MAX_INSTANCES + crate::model::MAX_GROUPS, "{total} instances");
        assert!(f.list_len() <= MAX_POOL_V4, "{} <= {MAX_POOL_V4}", f.list_len());
        // and the CPU field copes with the same scene
        let field = crate::sdf::Field::new(&sc);
        assert!(field.distance([0.0, 0.0, 0.0]).is_finite());
    }

    #[test]
    fn the_bounding_sphere_holds_every_instance() {
        let mut sc = Scene::default();
        sc.groups[0].pattern = Some(Pattern::Rect { counts: [3, 1, 1], spacing: [500.0, 0.0, 0.0] });
        let (c, r) = bounds(&sc, None);
        for k in 0..3 {
            let p = [500.0 * k as f32, 0.0, 20.0];
            assert!(crate::sdf::len(crate::sdf::sub(p, c)) + 20.0 <= r + 1e-2, "instance {k} is outside the sphere");
        }
        let mut one = sc.clone();
        one.groups[0].pattern = None;
        assert!(r > bounds(&one, None).1 + 400.0);
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

/// About `shapes` shapes of library models on a grid (for timing tests elsewhere).
#[cfg(test)]
pub(crate) fn library_scene_for_tests(shapes: usize) -> Scene {
    headless_tests::library_scene(shapes)
}

/// A real GPU (or the software one) without a window: renders scenes into a texture and reads
/// them back. Ignored by default (it needs an adapter): `cargo test -p meltcad --release gpu_headless -- --ignored --nocapture`.
#[cfg(test)]
pub(crate) mod headless {
    use super::*;

    pub struct Gpu {
        pub device: wgpu::Device,
        pub queue: wgpu::Queue,
        pub res: Resources,
        pub info: String,
    }

    /// `downlevel`: ask for the limits egui-wgpu asks for on OpenGL (no storage buffers, 16 KB uniforms).
    pub fn open(downlevel: bool) -> Option<Gpu> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        // (`WGPU_BACKEND=gl` runs the tests on OpenGL, the way the window does on machines without Vulkan)
        if let Some(b) = wgpu::Backends::from_env() {
            desc.backends = b;
        }
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, force_fallback_adapter: false, compatible_surface: None, ..Default::default() })).ok()?;
        let base = if downlevel { wgpu::Limits::downlevel_webgl2_defaults() } else { wgpu::Limits::default() };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("headless"),
            required_limits: wgpu::Limits { max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d.min(8192), ..base },
            ..Default::default()
        }))
        .ok()?;
        let info = format!("{:?}", adapter.get_info());
        let res = build(&device, wgpu::TextureFormat::Rgba8Unorm);
        Some(Gpu { device, queue, res, info })
    }

    impl Gpu {
        /// Render `data` (from `pack`) at `size`; returns RGBA8 pixels and the seconds the draw took.
        pub fn render(&mut self, data: &[f32], size: [u32; 2]) -> (Vec<u8>, f64) {
            self.render_with(data, &VolumeSet::default(), size)
        }

        /// `render`, with the imported meshes of the frame (`VolumeSet::of`, as given to `pack_with`).
        pub fn render_with(&mut self, data: &[f32], vols: &VolumeSet, size: [u32; 2]) -> (Vec<u8>, f64) {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("headless target"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            ensure_volumes(&mut self.res, &self.device, &self.queue, vols);
            ensure_pool(&mut self.res, &self.device, data);
            let bpr = (size[0] * 4).next_multiple_of(256);
            let out = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: (bpr * size[1]) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            draw(&self.res, &self.queue, &mut enc, &view, data);
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &out, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(size[1]) } },
                wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            );
            let t0 = std::time::Instant::now();
            self.queue.submit([enc.finish()]);
            let slice = out.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            let secs = t0.elapsed().as_secs_f64();
            let Ok(mapped) = slice.get_mapped_range() else { return (Vec::new(), secs) };
            let mut px = Vec::with_capacity((size[0] * size[1] * 4) as usize);
            for y in 0..size[1] {
                let row = (y * bpr) as usize;
                px.extend_from_slice(&mapped[row..row + (size[0] * 4) as usize]);
            }
            drop(mapped);
            out.unmap();
            (px, secs)
        }
    }
}

#[cfg(test)]
mod headless_tests {
    use super::headless;
    use super::*;
    use crate::model::Group;

    /// About `shapes` shapes: library models (cycling through every category) on a grid.
    pub(crate) fn library_scene(shapes: usize) -> Scene {
        let items: Vec<&crate::library::Item> = crate::library::all().iter().flat_map(|c| c.items.iter()).collect();
        let mut sc = Scene::default();
        sc.groups.clear();
        let mut i = 0usize;
        while sc.shape_count() < shapes && sc.groups.len() < MAX_GROUPS {
            let it = items[(i * 7) % items.len()];
            if !it.fits(sc.groups.len(), sc.shape_count()) {
                break;
            }
            let side = ((shapes as f32 / 12.0).sqrt().ceil() as usize).max(1);
            let at = [((i % side) as f32 - side as f32 / 2.0) * 90.0, ((i / side) as f32 - side as f32 / 2.0) * 90.0];
            let mut g: Group = it.placed(at);
            g.name = format!("{} {i}", it.name);
            sc.groups.push(g);
            i += 1;
        }
        sc
    }

    fn view_of(sc: &Scene) -> Camera {
        let mut cam = Camera::default();
        let (c, r) = bounds(sc, None);
        cam.frame(c, r.max(10.0));
        cam
    }

    #[test]
    fn the_library_scene_has_the_asked_size() {
        let sc = library_scene(300);
        assert!(sc.shape_count() >= 300 && sc.shape_count() <= 340 && sc.groups.len() < MAX_GROUPS, "{} shapes in {} groups", sc.shape_count(), sc.groups.len());
    }

    /// The picture the shader makes and the CPU field agree about what is there: the silhouette of
    /// a scene with a mirror, a ring, a grid and a linked copy, as drawn by the (software) GPU, is
    /// the same as the silhouette found by the CPU's ray marching.
    /// `cargo test -p meltcad --release gpu_headless_matches_cpu -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gpu_headless_matches_cpu() {
        use crate::model::{Kind, Material, Shape};
        use crate::pattern::{Pattern, Xform};
        let Some(mut gpu) = headless::open(std::env::var_os("YURA_GL_LIMITS").is_some()) else {
            eprintln!("no graphics adapter here");
            return;
        };
        let mut sc = Scene::default();
        sc.env.grid = false;
        sc.env.floor = false;
        sc.groups.clear();
        let mut g0 = Group::new("boxes");
        let mut b = Shape::new(Kind::Box, [30.0, 60.0, 15.0], Material::default());
        b.size = [25.0, 25.0, 25.0];
        g0.shapes.push(b);
        g0.pattern = Some(Pattern::Mirror { axes: [true, true, false], centre: false });
        sc.groups.push(g0);
        let mut g1 = Group::new("ring");
        let mut c = Shape::new(Kind::Cylinder, [50.0, -20.0, 0.0], Material::default());
        c.size = [8.0, 30.0, 1.0];
        g1.shapes.push(c);
        g1.pattern = Some(Pattern::Circular { count: 5, angle: 360.0, axis: 2, centre: [0.0; 3], rotate: true });
        sc.groups.push(g1);
        let mut g2 = Group::new("balls");
        g2.shapes.push(Shape::new(Kind::Sphere, [-20.0, -20.0, 12.0], Material::default()));
        g2.pattern = Some(Pattern::Rect { counts: [2, 1, 2], spacing: [25.0, 0.0, 30.0] });
        let uid = sc.ensure_group_uid(0).unwrap_or(0);
        sc.groups.push(g2);
        let mut copy = Group::new("copy");
        copy.instance_of = Some(uid);
        copy.xform = Xform { pos: [0.0, 0.0, 70.0], rot: [0.0, 0.0, 45.0], scale: 1.5 };
        sc.groups.push(copy);
        let size = [360u32, 234u32];
        let mut cam = Camera::default();
        let (cc, r) = bounds(&sc, None);
        cam.frame(cc, r);
        let draw = |scene: &Scene, gpu: &mut headless::Gpu| {
            let mut data = pack(scene, &cam, [size[0] as f32, size[1] as f32], None, None, false);
            set_view_flags(&mut data, crate::perf::view_flags(false, false, false));
            gpu.render(&data, size)
        };
        let (shown, secs) = draw(&sc, &mut gpu);
        let (empty, _) = draw(&Scene { groups: Vec::new(), ..sc.clone() }, &mut gpu);
        let field = crate::sdf::Field::new(&sc);
        let (mut gpu_only, mut cpu_only, mut both) = (0, 0, 0);
        for y in 0..size[1] {
            for x in 0..size[0] {
                let i = ((y * size[0] + x) * 4) as usize;
                let g = shown[i..i + 3] != empty[i..i + 3];
                let nx = (x as f32 + 0.5) / size[0] as f32 * 2.0 - 1.0;
                let ny = 1.0 - (y as f32 + 0.5) / size[1] as f32 * 2.0;
                let (o, d) = cam.ray([nx, ny], size[0] as f32 / size[1] as f32);
                let c = field.ray(o, d, 4000.0).is_some();
                match (g, c) {
                    (true, true) => both += 1,
                    (true, false) => gpu_only += 1,
                    (false, true) => cpu_only += 1,
                    _ => {}
                }
            }
        }
        eprintln!("{secs:.2} s, silhouette: {both} in both, {gpu_only} only the GPU, {cpu_only} only the CPU");
        let _ = std::fs::write("/tmp/yt_scale/gpu_patterns.ppm", [format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes(), shown.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect()].concat());
        assert!(both > 1500, "the scene covers a good part of the picture ({both})");
        // (the two march with different steps and hit limits, so a thin rim of edge pixels differs)
        assert!((gpu_only + cpu_only) * 100 < both * 6, "the silhouettes differ by more than 6%");
    }

    /// An imported mesh as the (software) GPU draws it, against the CPU field: our own STL of a small
    /// model is read back, baked and put in a scene beside a sphere that melts into it and a
    /// cylinder that carves it. The silhouettes of the two renderers must agree, and the mesh must
    /// be there (it covers a good part of the picture).
    /// `cargo test -p meltcad --release gpu_headless_mesh_matches_cpu -- --ignored --nocapture`
    /// (OpenGL like the window: `WGPU_BACKEND=gl YURA_GL_LIMITS=1 xvfb-run ...`)
    #[test]
    #[ignore]
    fn gpu_headless_mesh_matches_cpu() {
        use crate::model::{Kind, Material, Op, Shape};
        let Some(mut gpu) = headless::open(std::env::var_os("YURA_GL_LIMITS").is_some()) else {
            eprintln!("no graphics adapter here");
            return;
        };
        eprintln!("adapter: {}", gpu.info);
        let imp = crate::import_mesh::import_bytes(&crate::mesh_import::tests::sample_stl(), "model.stl", &Default::default()).unwrap();
        let st = crate::mesh_import::Settings { target_mm: 50.0, up: crate::mesh_import::Up::Z, floor: true, centre: true, res: 64 };
        let baked = crate::mesh_import::bake(&imp.mesh, imp.format, &st, &crate::sdf_volume::BakeControl::new()).unwrap();
        let mut sc = Scene::default();
        sc.env.grid = false;
        sc.env.floor = false;
        sc.groups.clear();
        let mut g = Group::new("m");
        let mut mesh = crate::mesh_import::shape_of(&baked, "m", "model.stl", [0.0, 0.0, baked.size[2] * 0.5], Material::default());
        mesh.blend = 6.0;
        mesh.rot = [0.0, 0.0, 30.0];
        g.shapes.push(mesh.clone());
        let mut ball = Shape::new(Kind::Sphere, [0.0, -22.0, 22.0], Material::default());
        ball.size[0] = 10.0;
        ball.blend = 6.0;
        g.shapes.push(ball);
        let mut cut = Shape::new(Kind::Cylinder, [8.0, 0.0, 10.0], Material::default());
        cut.size = [4.0, 60.0, 1.0];
        cut.rot = [90.0, 0.0, 0.0];
        cut.op = Op::Subtract;
        cut.blend = 2.0;
        g.shapes.push(cut);
        sc.groups.push(g);
        // a second copy of the same mesh in its own group shares the volume (one upload)
        let mut copy = Group::new("copy");
        let mut m2 = mesh.clone();
        m2.pos = [60.0, 10.0, baked.size[2] * 0.5];
        m2.scale = [0.7; 3];
        copy.shapes.push(m2);
        sc.groups.push(copy);
        let vols = VolumeSet::of(&sc);
        assert_eq!(vols.items.len(), 1, "both shapes use one volume");
        let size = [360u32, 234u32];
        let mut cam = Camera::default();
        let (cc, r) = bounds(&sc, None);
        cam.frame(cc, r);
        let draw = |scene: &Scene, gpu: &mut headless::Gpu| {
            let vols = VolumeSet::of(scene);
            let mut data = pack_with(scene, &cam, [size[0] as f32, size[1] as f32], None, None, false, &vols);
            set_view_flags(&mut data, crate::perf::view_flags(false, false, false));
            gpu.render_with(&data, &vols, size)
        };
        let (shown, secs) = draw(&sc, &mut gpu);
        let (empty, _) = draw(&Scene { groups: Vec::new(), ..sc.clone() }, &mut gpu);
        let field = crate::sdf::Field::new(&sc);
        let (mut gpu_only, mut cpu_only, mut both) = (0, 0, 0);
        for y in 0..size[1] {
            for x in 0..size[0] {
                let i = ((y * size[0] + x) * 4) as usize;
                let g = shown[i..i + 3] != empty[i..i + 3];
                let nx = (x as f32 + 0.5) / size[0] as f32 * 2.0 - 1.0;
                let ny = 1.0 - (y as f32 + 0.5) / size[1] as f32 * 2.0;
                let (o, d) = cam.ray([nx, ny], size[0] as f32 / size[1] as f32);
                let c = field.ray(o, d, 4000.0).is_some();
                match (g, c) {
                    (true, true) => both += 1,
                    (true, false) => gpu_only += 1,
                    (false, true) => cpu_only += 1,
                    _ => {}
                }
            }
        }
        eprintln!("{secs:.2} s, mesh silhouette: {both} in both, {gpu_only} only the GPU, {cpu_only} only the CPU");
        if let Some(dir) = std::env::var_os("YURA_OUT") {
            let _ = std::fs::write(std::path::Path::new(&dir).join("mesh_scene.meltcad"), sc.to_json());
            let _ = std::fs::write(std::path::Path::new(&dir).join("gpu_mesh.ppm"), [format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes(), shown.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect()].concat());
        }
        assert!(both > 1500, "the mesh covers a good part of the picture ({both})");
        assert!((gpu_only + cpu_only) * 100 < both * 6, "the silhouettes differ by more than 6%");
        // moving a mesh uploads nothing new, and replacing the volume draws it again
        let (shown2, _) = draw(&sc, &mut gpu);
        assert!(gpu.res.vol.resident.same_as(&VolumeSet::of(&sc)));
        assert!(shown2 == shown, "drawing the scene again after the volume was dropped and uploaded again gives the same picture");
    }

    /// Writes scenes for the real window: `/tmp/yt_scale/patterns.meltcad` (every kind of pattern and a
    /// linked copy) and `big300.meltcad` (about 300 library shapes).
    /// `cargo test -p meltcad --release dump_pattern_scenes -- --ignored`
    #[test]
    #[ignore]
    fn dump_pattern_scenes() {
        use crate::model::{Kind, Material, Shape};
        use crate::pattern::{PathMode, Pattern, Source, Xform};
        let mut sc = Scene::default();
        sc.groups.clear();
        let mut boxes = Group::new("ミラーの箱");
        let mut b = Shape::new(Kind::Box, [30.0, 60.0, 15.0], Material::plain([0.9, 0.5, 0.4], 0.4, 0.0));
        b.size = [25.0, 25.0, 25.0];
        boxes.shapes.push(b);
        boxes.pattern = Some(Pattern::Mirror { axes: [true, true, false], centre: false });
        sc.groups.push(boxes);
        let mut ring = Group::new("まわりの柱");
        let mut c = Shape::new(Kind::Cylinder, [60.0, -20.0, 0.0], Material::plain([0.4, 0.6, 0.9], 0.4, 0.0));
        c.size = [7.0, 30.0, 1.0];
        ring.shapes.push(c);
        ring.pattern = Some(Pattern::Circular { count: 7, angle: 360.0, axis: 2, centre: [0.0; 3], rotate: true });
        sc.groups.push(ring);
        let mut grid = Group::new("ならぶ玉");
        grid.shapes.push(Shape::new(Kind::Sphere, [-100.0, -100.0, 10.0], Material::plain([0.5, 0.85, 0.5], 0.3, 0.0)));
        grid.shapes[0].size[0] = 8.0;
        grid.pattern = Some(Pattern::Rect { counts: [4, 3, 1], spacing: [24.0, 24.0, 0.0] });
        sc.groups.push(grid);
        let mut line = Group::new("線");
        let mut sw = Shape::new(Kind::Sweep, [0.0; 3], Material::default());
        sw.path = vec![[-120.0, 90.0, 0.0, 1.0], [-40.0, 130.0, 20.0, 1.0], [60.0, 90.0, 0.0, 1.0], [140.0, 130.0, 20.0, 1.0]];
        sw.visible = false;
        line.shapes.push(sw);
        sc.groups.push(line);
        let uid = sc.ensure_shape_uid(3, 0).unwrap_or(0);
        let mut cones = Group::new("線の上の円すい");
        let mut cone = Shape::new(Kind::Cone, [0.0, 0.0, 0.0], Material::plain([0.95, 0.8, 0.3], 0.4, 0.0));
        cone.size = [6.0, 18.0, 1.0];
        cones.shapes.push(cone);
        cones.pattern = Some(Pattern::Path { source: uid, count: 9, mode: PathMode::Even, step: 20.0, align: true });
        sc.groups.push(cones);
        let mut pts = Group::new("点ごとの星");
        let mut star = Shape::new(Kind::Star, [0.0, 0.0, 0.0], Material::plain([0.9, 0.4, 0.7], 0.4, 0.0));
        star.size = [7.0, 6.0, 1.0];
        pts.shapes.push(star);
        pts.pattern = Some(Pattern::Geometry { source: Source::Shape(uid), orient: true });
        sc.groups.push(pts);
        let gid = sc.ensure_group_uid(0).unwrap_or(0);
        let mut copy = Group::new("リンクコピー");
        copy.instance_of = Some(gid);
        copy.xform = Xform { pos: [0.0, 0.0, 70.0], rot: [0.0, 0.0, 45.0], scale: 1.5 };
        sc.groups.push(copy);
        let _ = std::fs::create_dir_all("/tmp/yt_scale");
        let _ = std::fs::write("/tmp/yt_scale/patterns.meltcad", sc.to_json());
        let big = library_scene(300);
        let _ = std::fs::write("/tmp/yt_scale/big300.meltcad", big.to_json());
    }

    /// `cargo test -p meltcad --release gpu_headless_bench -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gpu_headless_bench() {
        let downlevel = std::env::var_os("YURA_GL_LIMITS").is_some();
        let Some(mut gpu) = headless::open(downlevel) else {
            eprintln!("no graphics adapter here");
            return;
        };
        eprintln!("adapter: {} (downlevel limits: {downlevel})", gpu.info);
        let size = [480u32, 300u32];
        for n in [12usize, 100, 300, 500] {
            let sc = library_scene(n);
            let cam = view_of(&sc);
            for fast in [false, true] {
                let mut data = pack(&sc, &cam, [size[0] as f32, size[1] as f32], None, None, false);
                set_view_flags(&mut data, crate::perf::view_flags(false, fast, false));
                let (_, first) = gpu.render(&data, size);
                let (px, secs) = gpu.render(&data, size);
                let lit = px.chunks(4).filter(|p| p[0] != px[0] || p[1] != px[1]).count();
                eprintln!(
                    "{:4} shapes in {:3} groups, {} : {:7.1} ms per {}x{} frame (first {:.0} ms), list {} vec4, {} pixels differ from the corner",
                    sc.shape_count(),
                    sc.groups.len(),
                    if fast { "fast" } else { "full" },
                    secs * 1000.0,
                    size[0],
                    size[1],
                    first * 1000.0,
                    data.len() / 4 - GLOBAL_V4,
                    lit
                );
            }
        }
        // one model, 250 live instances (a grid): no shape slots, 250 instances in the list
        let mut sc = library_scene(12);
        sc.groups.truncate(1);
        sc.groups[0].pattern = Some(crate::pattern::Pattern::Rect { counts: [25, 10, 1], spacing: [60.0, 60.0, 0.0] });
        let cam = view_of(&sc);
        for fast in [false, true] {
            let mut data = pack(&sc, &cam, [size[0] as f32, size[1] as f32], None, None, false);
            set_view_flags(&mut data, crate::perf::view_flags(false, fast, false));
            let (_, first) = gpu.render(&data, size);
            let (_, secs) = gpu.render(&data, size);
            eprintln!(
                "{:4} shapes x 250 instances, {} : {:7.1} ms per {}x{} frame (first {:.0} ms), list {} vec4",
                sc.shape_count(),
                if fast { "fast" } else { "full" },
                secs * 1000.0,
                size[0],
                size[1],
                first * 1000.0,
                data.len() / 4 - GLOBAL_V4
            );
        }
    }
}
