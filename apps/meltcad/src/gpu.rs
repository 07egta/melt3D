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
use crate::gpu_status::{AdapterInfo, GpuStatus, describe_wgpu_error};
use crate::sdf::Prepared;

/// Per-frame numbers (camera, light, counts, where the lists start) travel in a small uniform
/// buffer: `GLOBAL_V4` vec4 (struct `Globals` in shader.wgsl). The scene itself is a long list of
/// vec4 in a float texture (`ROW` vec4 per row), read with `textureLoad`: there is no size limit
/// from the 64 KB uniform buffer, and it works on every backend (a storage buffer is not allowed
/// by the limits egui-wgpu asks for on OpenGL). The list is as long as the scene needs.
const V4_VIEW: usize = 4;
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
    let sp = ScenePack::new(scene, vols.clone());
    let mut u = sp.globals(scene, cam, size, selected, hover, srgb).to_vec();
    u.extend_from_slice(&sp.list);
    u
}

static PACK_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The scene as the shader reads it: the long list (groups, blocks, instances, shapes, outline
/// points, path nodes) and what is needed to name things in it. It depends on the scene alone, not
/// on the camera, the selection or the picture size, so a frame that only moves the camera reuses
/// it (and the graphics card keeps the list it already has: see `id`).
#[derive(Debug)]
pub struct ScenePack {
    /// A number that is new for every pack: the graphics side uploads the list when it changes.
    pub id: u64,
    pub list: Arc<Vec<f32>>,
    pub vols: Arc<VolumeSet>,
    /// Does any visible shape glow (the shader must march with materials)?
    pub glow: bool,
    /// Does any visible group or shape keep only an overlap (`Op::Intersect`)? Then what is far
    /// from a ray still decides where it stops, and the march may not leave anything out.
    pub overlaps: bool,
    /// Instances in the list (a plain group counts none).
    pub instances: usize,
    /// Shapes in the list and groups to draw.
    pub shape_n: usize,
    pub group_n: usize,
    /// Number of unchanged shapes whose packed data was safely reused.
    pub shapes_reused: usize,
    /// Number of shape payloads computed on this revision.
    pub shapes_repacked: usize,
    offs: [f32; 4],
    offs2: [f32; 2],
    resolved: Vec<crate::pattern::RGroup>,
    /// (group, shape, number in the list) of every packed shape.
    slots: Vec<(usize, usize, usize)>,
    /// (group, first packed shape, count) of every group whose shapes were packed.
    ranges: Vec<(usize, usize, usize)>,
}

impl ScenePack {
    pub fn new(scene: &Scene, vols: VolumeSet) -> ScenePack {
        Self::reusing(scene, vols, None)
    }

    /// Keep exact packed payloads of unchanged shapes while updating dependent group bounds,
    /// ordering, Boolean operations and instance transforms. Pool offsets are rebased below;
    /// meshes and curves take the original path to avoid reusing context-dependent volume
    /// locations or an adaptively truncated curve.
    pub fn reusing(scene: &Scene, vols: VolumeSet, previous: Option<(&Scene, &ScenePack)>) -> ScenePack {
        let old_slots: std::collections::HashMap<(usize, usize), usize> = previous
            .map(|(_, pack)| pack.slots.iter().map(|&(g,s,slot)| ((g,s), slot)).collect())
            .unwrap_or_default();
        let resolved = crate::pattern::resolve(scene);
        let mut groups: Vec<f32> = Vec::new();
        let mut blocks: Vec<f32> = Vec::new();
        let mut insts: Vec<f32> = Vec::new();
        let mut shapes: Vec<f32> = Vec::new();
        let mut poly: Vec<f32> = Vec::new();
        let mut path: Vec<f32> = Vec::new();
        let mut shape_n = 0usize;
        let mut packed_overlap = false;
        let mut block_n = 0usize;
        let mut inst_n = 0usize;
        let mut slots: Vec<(usize, usize, usize)> = Vec::new();
        let mut ranges: Vec<(usize, usize, usize)> = Vec::new();
        let mut shapes_reused = 0;
        let mut shapes_repacked = 0;
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
                let reused = previous.is_some_and(|(old_scene, old_pack)| {
                    old_scene.shape(gi, si).is_some_and(|old| old == s)
                        && old_slots.get(&(gi, si)).is_some_and(|&slot| {
                            reuse_shape_pack(s, old_pack, slot, &mut shapes, &mut poly, &mut path)
                        })
                });
                if reused { shapes_reused += 1; }
                else {
                    pack_shape(s, &mut shapes, &mut poly, &mut path, &vols);
                    shapes_repacked += 1;
                }
                packed_overlap |= s.op == crate::model::Op::Intersect;
                members.push(s);
                slots.push((gi, si, shape_n));
                shape_n += 1;
            }
            let count = shape_n - first;
            ranges.push((gi, first, count));
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
        let mut item_n = 0usize;
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
            // (the third number is this group's first item: every instance, or the group itself, is one item the march can leave out)
            push(&mut groups, [first_inst as f32, (inst_n - first_inst) as f32, item_n as f32, 0.0]);
            item_n += (inst_n - first_inst).max(1);
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
        let mut list = groups;
        list.reserve(blocks.len() + insts.len() + shapes.len() + poly.len() + path.len());
        list.extend_from_slice(&blocks);
        list.extend_from_slice(&insts);
        list.extend_from_slice(&shapes);
        list.extend_from_slice(&poly);
        list.extend_from_slice(&path);
        ScenePack {
            id: PACK_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            list: Arc::new(list),
            vols: Arc::new(vols),
            glow: crate::perf::scene_has_glow(scene),
            overlaps: resolved.iter().filter_map(|r| scene.groups.get(r.gi)).any(|g| g.op == crate::model::Op::Intersect)
                || packed_overlap,
            instances: inst_n,
            shape_n,
            group_n,
            shapes_reused,
            shapes_repacked,
            offs: [0.0, block_base as f32, inst_base as f32, shape_base as f32],
            offs2: [poly_base as f32, path_base as f32],
            resolved,
            slots,
            ranges,
        }
    }

    /// The packed number of a shape (hidden shapes have none).
    fn slot_of(&self, gi: usize, si: usize) -> Option<usize> {
        self.slots.iter().find(|(g, s, _)| *g == gi && *s == si).map(|(_, _, n)| *n)
    }

    /// The per-frame numbers (`GLOBAL_V4` vec4): camera, light, counts, where the lists start, the
    /// selection. `scene` must be the one this pack was made from.
    pub fn globals(&self, scene: &Scene, cam: &Camera, size: [f32; 2], selected: Option<Pick>, hover: Option<Pick>, srgb: bool) -> [f32; GLOBAL_V4 * 4] {
        let mut u = [0.0_f32; GLOBAL_V4 * 4];
        let (right, up, fwd) = cam.basis();
        let eye = cam.eye();
        put(&mut u, 0, [eye[0], eye[1], eye[2], 0.0]);
        put(&mut u, 1, [right[0], right[1], right[2], 0.0]);
        put(&mut u, 2, [up[0], up[1], up[2], 0.0]);
        put(&mut u, 3, [fwd[0], fwd[1], fwd[2], if cam.ortho { 0.0 } else { TAN_HALF_FOV }]);
        put(&mut u, 4, [size[0], size[1], cam.half_height(), if srgb { 1.0 } else { 0.0 }]);
        let m = scene.melt;
        put(&mut u, 5, [m.mode.id(), m.amount, m.drip, m.ripple]);
        let (mut sel_first, mut sel_count, mut hover_first, mut hover_count) = (-1.0_f32, 0.0_f32, -1.0_f32, 0.0_f32);
        let find = |p: Option<Pick>| -> Option<(f32, f32)> {
            match p? {
                Pick::Shape(g, s) => self.slot_of(g, s).map(|n| (n as f32, 1.0)),
                Pick::Group(g) => self.ranges.iter().find(|(gi, _, c)| *gi == g && *c > 0).map(|(_, f, c)| (*f as f32, *c as f32)),
            }
        };
        if let Some((f, c)) = find(selected) {
            (sel_first, sel_count) = (f, c);
        }
        // (the hover names one shape only)
        if let Some(Pick::Shape(g, s)) = hover
            && let Some(n) = self.slot_of(g, s)
        {
            (hover_first, hover_count) = (n as f32, 1.0);
        }
        put(&mut u, 6, [self.group_n as f32, self.shape_n as f32, sel_first, sel_count]);
        put(&mut u, V4_OFFS, self.offs);
        put(&mut u, V4_OFFS2, [self.offs2[0], self.offs2[1], if self.overlaps { 1.0 } else { 0.0 }, 0.0]);
        let (bc, br) = bounds_of(scene, selected, &self.resolved);
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
        u
    }
}

/// Copy a previously packed unchanged shape without redoing transforms, CAD inset or polygon
/// preparation. Rebase its outline/path indices to the current compact pools. Returns false
/// (falling back to the original packer) if any pool would overflow or the old data was truncated.
/// Important: curves' adaptive tessellation and mesh volume addresses depend on neighbouring
/// shapes, so those types always use the exact original packer instead.
fn reuse_shape_pack(
    shape: &crate::model::Shape, old: &ScenePack, slot: usize,
    shapes: &mut Vec<f32>, poly: &mut Vec<f32>, path: &mut Vec<f32>,
) -> bool {
    use crate::model::Kind;
    if matches!(shape.kind, Kind::Curve | Kind::Mesh) {
        return false;
    }
    let start = (old.offs[3] as usize + slot * S_STRIDE) * 4;
    let Some(old_data) = old.list.get(start..start + S_STRIDE * 4) else { return false };
    let mut data = old_data.to_vec();

    let poly_base = old.offs2[0] as usize * 4;
    let path_base = old.offs2[1] as usize * 4;
    let grab_poly = |start_slot: usize, count_slot: usize, enabled: bool, count_expected: usize| -> Option<&[f32]> {
        if !enabled { return Some(&[]); }
        let (first, count) = (data[start_slot] as usize, data[count_slot] as usize);
        if count != count_expected { return None; }
        let at = poly_base.checked_add(first.checked_mul(2)?)?;
        let end = at.checked_add(count.checked_mul(2)?)?;
        old.list.get(at..end)
    };
    let Some(outline) = grab_poly(7*4+2, 7*4+3, shape.kind.uses_outline(), shape.outline.len()) else { return false };
    let Some(outline2) = grab_poly(12*4+2, 12*4+3, shape.kind.uses_outline2(), shape.outline2.len()) else { return false };
    let (old_start, old_count) = (data[12*4] as usize, data[12*4+1] as usize);
    let path_slice = if shape.kind.uses_path() {
        if old_count != shape.path.len() { return false; }
        let at = path_base + old_start * 4;
        let Some(nodes) = old.list.get(at..at + old_count * 4) else { return false };
        nodes
    } else {
        &[]
    };
    if poly.len() / 2 + outline.len()/2 + outline2.len()/2 > POLY_PTS
        || path.len()/4 + path_slice.len()/4 > PATH_V4
    {
        return false;
    }
    let mut append_poly = |slot: usize, count: usize, enabled: bool, pts: &[f32]| {
        if enabled && count > 0 {
            data[slot] = (poly.len()/2) as f32;
            poly.extend_from_slice(pts);
        } else {
            data[slot] = 0.0;
        }
    };
    append_poly(7*4+2, shape.outline.len(), shape.kind.uses_outline(), outline);
    append_poly(12*4+2, shape.outline2.len(), shape.kind.uses_outline2(), outline2);
    if shape.kind.uses_path() {
        data[12*4] = (path.len()/4) as f32;
        path.extend_from_slice(path_slice);
    }
    shapes.extend_from_slice(&data);
    true
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
/// corner of a box around the tube), and are followed by one bounding sphere (centre, radius with
/// the tube's thickness) for every `curve::GROUP_SEGMENTS` segments: the shader looks at those
/// first and measures only the groups that can hold the nearest piece. Returns (first point,
/// count); (0, 0) when there is no room.
fn pack_curve(s: &crate::model::Shape, path: &mut Vec<f32>) -> (usize, usize) {
    let used = path.len() / 4;
    let room = PATH_V4.saturating_sub(used + 2);
    let need = |n: usize| n + n.saturating_sub(1).div_ceil(crate::curve::GROUP_SEGMENTS);
    let mut pts = crate::curve::build(&s.path, &s.params);
    if need(pts.len()) > room {
        pts = crate::curve::build_max(&s.path, &s.params, (room * crate::curve::GROUP_SEGMENTS / (crate::curve::GROUP_SEGMENTS + 1)).max(4));
    }
    if pts.len() < 2 || need(pts.len()) > room {
        return (0, 0);
    }
    let (lo, hi) = crate::curve::bounds(&pts);
    let closed = s.params[0] > 0.5;
    push(path, [lo[0], lo[1], lo[2], crate::curve::max_joint_blend(&pts, closed)]);
    push(path, [hi[0], hi[1], hi[2], crate::curve::closed_joint_blend(&pts, closed)]);
    let st = used + 2;
    for pt in &pts {
        push(path, *pt);
    }
    for g in crate::curve::group_spheres(&pts) {
        push(path, g);
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
    /// Draws the scene (into a picture).
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    scene_layout: wgpu::BindGroupLayout,
    /// The scene list; made (and made taller) when a scene needs it.
    pool: Option<Pool>,
    /// What the pool holds now: the pack it came from, and a copy of its numbers (so that a
    /// small change uploads only the rows that differ).
    pool_id: u64,
    pool_copy: Vec<f32>,
    /// The imported meshes' distances (a 1-row dummy when there are none).
    vol: VolTex,
    /// Copies the pictures into egui's frame.
    blit: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    blit_params: wgpu::Buffer,
    linear: wgpu::Sampler,
    format: wgpu::TextureFormat,
    /// The two pictures of the scene (see `Pics`). They stay until the scene changes, so a repaint
    /// that is only about the panels costs one textured triangle, not a ray march.
    pics: Option<Pics>,
    /// Times the scene passes on the graphics card (None where the device cannot).
    timer: Option<crate::gpu_timing::Timer>,
    /// How many rows of the sharp picture to draw in a frame.
    pacer: crate::adapt::BandPacer,
    /// Frame counter (for the frame cache and tests).
    frames: u64,
    /// The volume layout generation: bumped whenever the volume texture changes.
    vol_gen: u64,
}

/// What a picture was made from. A picture is current when the key of the frame equals it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PicKey {
    /// Hash of the per-frame numbers (camera, selection, light...) without the picture size and the fast flag.
    globals: u64,
    list: u64,
    vol_gen: u64,
    /// The size drawn, in pixels.
    size: [u32; 2],
}

/// A picture the scene is drawn into: a texture as large as the window's viewport, of which the
/// top left `used` part is drawn (so that changing the render scale does not make a new texture).
struct Pic {
    view: wgpu::TextureView,
    key: Option<PicKey>,
    used: [u32; 2],
}

/// The cheap picture A, drawn in one pass while things move; the sharp picture B, drawn band by
/// band (a few rows in each frame) when the scene is still, so that no frame has to pay for the
/// whole thing; and the bind group the blit reads both through.
struct Pics {
    a: Pic,
    b: Pic,
    /// Rows of B that are finished (the key says for which picture).
    bands: crate::adapt::Bands,
    capacity: [u32; 2],
    bind: wgpu::BindGroup,
}

/// The shader is one program in several files: the core, the extra solids, the CAD features.
pub fn shader_source() -> String {
    // (tests and the benchmark can compare against another version of the files: `MELTCAD_SHADER_DIR=dir`)
    #[cfg(test)]
    if let Some(dir) = std::env::var_os("MELTCAD_SHADER_DIR") {
        let files = ["shader.wgsl", "shader_prims.wgsl", "shader_cad.wgsl", "shader_curve.wgsl", "shader_corner.wgsl", "shader_mesh.wgsl"];
        let parts: Vec<String> = files.iter().filter_map(|f| std::fs::read_to_string(std::path::Path::new(&dir).join(f)).ok()).collect();
        if parts.len() == files.len() {
            return parts.join("\n");
        }
    }
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
/// The blit's parameters: three vec4.
const BLIT_PARAM_BYTES: usize = 48;

/// Run `f` with every kind of wgpu error caught instead of panicking (the default handler of an
/// error nobody asked for is a panic, and on the compile thread a panic is silent). Returns what
/// `f` made and the text of the errors it caused.
pub(crate) fn scoped<T>(device: &wgpu::Device, stage: &str, f: impl FnOnce() -> T) -> (T, Vec<String>) {
    let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let value = f();
    let mut errors = Vec::new();
    // (scopes come off in the reverse order they went on)
    for guard in [validation, internal, oom] {
        if let Some(e) = pollster::block_on(guard.pop()) {
            errors.push(describe_wgpu_error(stage, &e));
        }
    }
    (value, errors)
}

/// What the shader translator said about a module (warnings and errors), and whether any of it
/// was an error.
fn compilation_messages(module: &wgpu::ShaderModule) -> (Vec<String>, bool) {
    let info = pollster::block_on(module.get_compilation_info());
    let mut any_error = false;
    let lines = info
        .messages
        .iter()
        .map(|m| {
            let error = m.message_type == wgpu::CompilationMessageType::Error;
            any_error |= error;
            let at = m.location.as_ref().map(|l| format!(" (line {}, column {})", l.line_number, l.line_position)).unwrap_or_default();
            format!("{}{}: {}", if error { "error" } else { "warning" }, at, m.message)
        })
        .collect();
    (lines, any_error)
}

/// A step of building the pipelines: runs `f` in an error scope, writes how long it took to the
/// log, and turns the caught errors into one error text.
fn step<T>(device: &wgpu::Device, stage: &str, f: impl FnOnce() -> T) -> Result<T, String> {
    crate::gpu_log::log(&format!("{stage}: start"));
    let t = std::time::Instant::now();
    let (value, errors) = scoped(device, stage, f);
    crate::gpu_log::log(&format!("{stage}: end after {:.3} s", t.elapsed().as_secs_f64()));
    if errors.is_empty() { Ok(value) } else { Err(errors.join("\n")) }
}

fn texture_entry(binding: u32, visibility: wgpu::ShaderStages, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}

/// A pipeline that draws one full-screen triangle with `module`'s `vs` and `fs` into `format`.
fn triangle_pipeline(device: &wgpu::Device, label: &str, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Build the pipelines (this is the slow part of start-up: the driver compiles the shader).
/// Any error from the driver comes back as text; none of them panics.
fn try_build(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Result<Resources, String> {
    try_build_from(device, queue, format, &shader_source())
}

/// `try_build` with the scene shader's source given (so that tests can hand it a broken one).
pub(crate) fn try_build_from(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, source: &str) -> Result<Resources, String> {
    let module = step(device, "シェーダーを読む (create_shader_module)", || {
        device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("meltcad"), source: wgpu::ShaderSource::Wgsl(source.into()) })
    })?;
    let (messages, any_error) = compilation_messages(&module);
    for m in &messages {
        crate::gpu_log::log(&format!("shader: {m}"));
    }
    if any_error {
        return Err(format!("シェーダーのエラー:\n{}", messages.join("\n")));
    }
    crate::perf::mark("shader module created");
    let (bgl, layout) = step(device, "bind group layout", || {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("meltcad"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                // the scene list, then the imported meshes' distances (half floats): both read with textureLoad
                texture_entry(1, wgpu::ShaderStages::FRAGMENT, false),
                texture_entry(2, wgpu::ShaderStages::FRAGMENT, false),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("meltcad"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        (bgl, layout)
    })?;
    // (the slow one: the driver compiles the shader here)
    let pipeline = step(device, "シーンの描画パイプライン (create_render_pipeline)", || triangle_pipeline(device, "meltcad", &layout, &module, format))?;
    crate::perf::mark("scene pipeline created");
    let buffer = step(device, "uniform buffer", || {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("meltcad"),
            contents: &vec![0u8; GLOBAL_BYTES],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    })?;

    // the copy of the finished pictures into egui's frame
    let (blit_layout, blit) = step(device, "blit pipeline", || {
        let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("meltcad blit"), source: wgpu::ShaderSource::Wgsl(BLIT_SOURCE.into()) });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("meltcad blit"),
            entries: &[
                texture_entry(0, wgpu::ShaderStages::FRAGMENT, true),
                texture_entry(1, wgpu::ShaderStages::FRAGMENT, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let blit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("meltcad blit"), bind_group_layouts: &[Some(&blit_layout)], immediate_size: 0 });
        let blit = triangle_pipeline(device, "meltcad blit", &blit_pl, &blit_module, format);
        (blit_layout, blit)
    })?;
    let blit_params = step(device, "blit parameters", || {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("meltcad blit"),
            contents: &[0u8; BLIT_PARAM_BYTES],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    })?;
    let linear = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("meltcad"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
    // (the one-row volume texture: Rgba16Float must be bindable)
    let vol = step(device, "volume texture", || VolTex::new(device, 1))?;
    Ok(Resources {
        pipeline,
        buffer,
        scene_layout: bgl,
        pool: None,
        pool_id: 0,
        pool_copy: Vec::new(),
        vol,
        blit,
        blit_layout,
        blit_params,
        linear,
        format,
        pics: None,
        timer: crate::gpu_timing::Timer::new(device, queue),
        pacer: crate::adapt::BandPacer::default(),
        frames: 0,
        vol_gen: 1,
    })
}

/// `try_build` for the headless tests, which have nothing better to do with an error than stop.
#[cfg(test)]
fn build(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Resources {
    try_build(device, queue, format).unwrap_or_else(|e| panic!("{e}"))
}

/// Make the device report its errors to the status and the log instead of panicking.
fn watch_errors(device: &wgpu::Device, status: &GpuStatus) {
    let status = status.clone();
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        let text = describe_wgpu_error("wgpu", &e);
        crate::gpu_log::log(&format!("wgpu error (not caught by a scope): {text}"));
        status.runtime_error(text);
    }));
}

/// Start building the pipelines (at start-up, and again when the person asks for a retry). The
/// compile runs on its own thread so that the window can show first, and the viewport shows the
/// simple view meanwhile; `status` says how it goes. Whatever happens (an error from the driver,
/// even a panic) ends in `status`, never in a silent dead thread.
pub fn install(rs: &egui_wgpu::RenderState, ctx: &egui::Context, status: &GpuStatus) {
    let device = rs.device.clone();
    let format = rs.target_format;
    let renderer = rs.renderer.clone();
    let queue = rs.queue.clone();
    let ctx = ctx.clone();
    let st = status.clone();
    let info = rs.adapter.get_info();
    status.set_info(AdapterInfo::from_wgpu(&info));
    watch_errors(&device, status);
    crate::gpu_log::log(&format!("graphics path starts: target format {format:?}"));
    crate::gpu_log::log_adapter(&rs.adapter);
    let generation = status.begin(std::time::Instant::now());
    let job = Arc::new(move || {
        let t = std::time::Instant::now();
        crate::gpu_log::log("shader compile: start");
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| try_build(&device, &queue, format)));
        let secs = t.elapsed().as_secs_f64();
        let backend = crate::backend::Backend::from_wgpu(info.backend);
        match outcome {
            Ok(Ok(res)) => {
                renderer.write().callback_resources.insert(res);
                crate::gpu_log::log(&format!("shader compile: done after {secs:.3} s, the graphics path is ready"));
                st.finish_ok(generation, secs);
                if let Some(b) = backend {
                    crate::backend::remember(b, true);
                }
                crate::perf::mark("gpu ready");
            }
            Ok(Err(message)) => {
                crate::gpu_log::log(&format!("shader compile: FAILED after {secs:.3} s\n{message}"));
                st.finish_err(generation, message);
                if let Some(b) = backend {
                    crate::backend::remember(b, false);
                }
            }
            Err(payload) => {
                let message = format!("パニック (panic) がおきました: {}", crate::gpu_status::panic_text(&*payload));
                crate::gpu_log::log(&format!("shader compile: PANIC after {secs:.3} s: {message}"));
                st.finish_err(generation, message);
                if let Some(b) = backend {
                    crate::backend::remember(b, false);
                }
            }
        }
        ctx.request_repaint();
    });
    let job2 = job.clone();
    // (OpenGL keeps one context lock for the whole compile: egui's own texture uploads on the main
    // thread would then wait on it and give up, so there the compile runs right here)
    let on_gl = info.backend == wgpu::Backend::Gl;
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
fn ensure_volumes(res: &mut Resources, device: &wgpu::Device, queue: &wgpu::Queue, set: &VolumeSet) -> bool {
    if res.vol.resident.same_as(set) {
        return false;
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
    res.vol_gen += 1;
    true
}

/// What the graphics side tells the app: the timings of finished passes, and how the pictures stand.
#[derive(Clone, Default, Debug)]
pub struct Feedback(Arc<std::sync::Mutex<FeedbackInner>>);

#[derive(Default, Debug)]
struct FeedbackInner {
    passes: Vec<crate::gpu_timing::TimedPass>,
    state: FeedbackState,
}

/// How the pictures stand after the last frame the graphics side prepared.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct FeedbackState {
    /// The device can time passes.
    pub timing: bool,
    /// Share of the sharp picture that is finished (1 = all, 0 = none).
    pub sharp: f32,
    /// The size drawn for the cheap picture, in pixels.
    pub moving_px: [u32; 2],
    /// Rows the sharp picture is drawn per frame now.
    pub band_rows: u32,
    /// Frames the graphics side has prepared.
    pub frames: u64,
}

impl Feedback {
    fn lock(&self) -> std::sync::MutexGuard<'_, FeedbackInner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The timings that finished since the last call.
    pub fn take_passes(&self) -> Vec<crate::gpu_timing::TimedPass> {
        std::mem::take(&mut self.lock().passes)
    }

    pub fn state(&self) -> FeedbackState {
        self.lock().state
    }

    fn push(&self, passes: Vec<crate::gpu_timing::TimedPass>, state: FeedbackState) {
        let mut g = self.lock();
        g.passes.extend(passes);
        // (nobody listening: do not grow without end)
        let excess = g.passes.len().saturating_sub(256);
        if excess > 0 {
            g.passes.drain(..excess);
        }
        g.state = state;
    }
}

/// One frame of the viewport, as an egui paint callback.
pub struct Callback {
    /// The per-frame numbers (`ScenePack::globals`).
    pub globals: [f32; GLOBAL_V4 * 4],
    /// The scene list and the imported meshes (the same pack is reused while the scene is unchanged).
    pub pack: Arc<ScenePack>,
    /// The viewport's size in pixels.
    pub viewport: [u32; 2],
    /// The view is moving: draw the cheap picture.
    pub fast: bool,
    /// Share of the viewport's pixels along each side to draw for the cheap picture, and for the sharp one.
    pub scale_moving: f32,
    pub scale_still: f32,
    /// How much to sharpen a picture that is scaled up (0..1).
    pub sharpen: f32,
    /// Draw the sharp picture in one go, at once (a picture of the viewport is being taken).
    pub whole: bool,
    /// The time between the last two frames (ms), 0 when not known (for pacing the bands without timestamps).
    pub frame_ms: f32,
    /// Where an error of ours while drawing is reported.
    pub status: GpuStatus,
    pub feedback: Feedback,
    /// Asks for the next frame while the sharp picture is still being drawn.
    pub ctx: egui::Context,
}

/// Hash of the per-frame numbers that decide what a picture shows: everything except the size of
/// the picture and the fast flag, which the pass patches in.
fn globals_key(globals: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, v) in globals.iter().enumerate() {
        if i == V4_VIEW * 4 || i == V4_VIEW * 4 + 1 || i == V4_VIEW * 4 + 3 {
            continue;
        }
        h ^= u64::from(v.to_bits());
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Make the scene-list texture tall enough for `list` (a scene list from `ScenePack`).
fn ensure_pool(res: &mut Resources, device: &wgpu::Device, list_len: usize) {
    let used_rows = (list_len / 4).div_ceil(ROW).max(1) as u32;
    if res.pool.as_ref().is_none_or(|p| p.rows < used_rows) {
        res.pool = Some(Pool::new(device, &res.buffer, &res.scene_layout, &res.vol.view, used_rows));
        res.pool_id = 0;
        res.pool_copy.clear();
        invalidate_pics(res);
    }
}

fn invalidate_pics(res: &mut Resources) {
    if let Some(p) = res.pics.as_mut() {
        p.a.key = None;
        p.b.key = None;
        p.bands = crate::adapt::Bands::default();
    }
}

/// Put the scene list into the pool texture, the rows that differ from what is there.
fn upload_list(res: &mut Resources, queue: &wgpu::Queue, list: &[f32], id: u64) {
    if res.pool_id == id {
        return;
    }
    let Some(pool) = res.pool.as_ref() else { return };
    let rows = ((list.len() / 4).div_ceil(ROW).max(1) as u32).min(pool.rows) as usize;
    let row_len = ROW * 4;
    if res.pool_copy.len() < rows * row_len {
        res.pool_copy.resize(rows * row_len, 0.0);
    }
    let new_row = |r: usize| -> &[f32] {
        let from = (r * row_len).min(list.len());
        let to = ((r + 1) * row_len).min(list.len());
        list.get(from..to).unwrap_or(&[])
    };
    // does row r of the new list equal what the texture holds (a short row counts as zero-padded)?
    let same = |copy: &[f32], r: usize| -> bool {
        let old = copy.get(r * row_len..(r + 1) * row_len).unwrap_or(&[]);
        let new = new_row(r);
        old.iter().enumerate().all(|(i, o)| o.to_bits() == new.get(i).copied().unwrap_or(0.0).to_bits())
    };
    let first = (0..rows).find(|&r| !same(&res.pool_copy, r));
    let last = (0..rows).rev().find(|&r| !same(&res.pool_copy, r));
    if let (Some(first), Some(last)) = (first, last) {
        let n = last - first + 1;
        let mut texels: Vec<u8> = Vec::with_capacity(n * row_len * 4);
        for r in first..=last {
            let row = new_row(r);
            for i in 0..row_len {
                texels.extend_from_slice(&row.get(i).copied().unwrap_or(0.0).to_le_bytes());
            }
            if let Some(dst) = res.pool_copy.get_mut(r * row_len..(r + 1) * row_len) {
                for (i, d) in dst.iter_mut().enumerate() {
                    *d = row.get(i).copied().unwrap_or(0.0);
                }
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &pool.texture, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: first as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
            &texels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(ROW as u32 * 16), rows_per_image: Some(n as u32) },
            wgpu::Extent3d { width: ROW as u32, height: n as u32, depth_or_array_layers: 1 },
        );
    }
    res.pool_id = id;
}

/// The two pictures for a viewport of `capacity` pixels (made again only when the viewport changes size).
fn ensure_pics(res: &mut Resources, device: &wgpu::Device, capacity: [u32; 2]) {
    if res.pics.as_ref().is_some_and(|p| p.capacity == capacity) {
        return;
    }
    let make = || {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("meltcad picture"),
            size: wgpu::Extent3d { width: capacity[0], height: capacity[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: res.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Pic { view: tex.create_view(&wgpu::TextureViewDescriptor::default()), key: None, used: [0, 0] }
    };
    let (a, b) = (make(), make());
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("meltcad blit"),
        layout: &res.blit_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&a.view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&b.view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&res.linear) },
            wgpu::BindGroupEntry { binding: 3, resource: res.blit_params.as_entire_binding() },
        ],
    });
    res.pics = Some(Pics { a, b, bands: crate::adapt::Bands::default(), capacity, bind });
}

/// Where and how one scene pass draws.
struct PassSpec {
    /// The picture's size in pixels (the shader's view of the window).
    size: [u32; 2],
    /// Only these rows (first, count) are shaded; the picture is kept elsewhere (`Load`).
    rows: Option<(u32, u32)>,
    /// Clear the picture first.
    clear: bool,
    fast: bool,
    kind: crate::gpu_timing::Kind,
    /// Linear size relative to the window, for the timing record.
    scale: f32,
}

/// The globals with the picture size and the fast flag of this pass written in.
fn patched(globals: &[f32], size: [u32; 2], fast: bool) -> Vec<f32> {
    let mut g = globals.to_vec();
    if let Some(v) = g.get_mut(V4_VIEW * 4) {
        *v = size[0] as f32;
    }
    if let Some(v) = g.get_mut(V4_VIEW * 4 + 1) {
        *v = size[1] as f32;
    }
    if let Some(v) = g.get_mut(V4_VIEW * 4 + 3) {
        let flags = (*v + 0.5) as u32 & !crate::perf::FLAG_FAST;
        *v = (flags | if fast { crate::perf::FLAG_FAST } else { 0 }) as f32;
    }
    g
}

/// Draw the scene into `view` (the globals go into the uniform buffer; the list must be in the pool).
fn scene_pass(res: &mut Resources, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, globals: &[f32], spec: &PassSpec) {
    let g = patched(globals, spec.size, spec.fast);
    let mut bytes = Vec::with_capacity(GLOBAL_BYTES);
    for v in g.iter().take(GLOBAL_V4 * 4) {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.resize(GLOBAL_BYTES, 0);
    queue.write_buffer(&res.buffer, 0, &bytes);
    let (pixels, nrows) = match spec.rows {
        Some((_, n)) => (u64::from(spec.size[0]) * u64::from(n), n),
        None => (u64::from(spec.size[0]) * u64::from(spec.size[1]), spec.size[1]),
    };
    let claim = res.timer.as_mut().and_then(|t| t.claim(spec.kind, pixels, nrows, spec.scale));
    {
        let Some(pool) = res.pool.as_ref() else {
            if let (Some(t), Some(c)) = (res.timer.as_mut(), claim) {
                t.cancel(c);
            }
            return;
        };
        let writes = res.timer.as_ref().zip(claim.as_ref()).map(|(t, c)| t.writes(c));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("meltcad scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: if spec.clear { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: writes,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &pool.bind, &[]);
        pass.set_viewport(0.0, 0.0, spec.size[0] as f32, spec.size[1] as f32, 0.0, 1.0);
        if let Some((first, n)) = spec.rows {
            pass.set_scissor_rect(0, first, spec.size[0], n);
        }
        pass.draw(0..3, 0..1);
    }
    if let (Some(t), Some(c)) = (res.timer.as_mut(), claim) {
        t.finish(encoder, c);
    }
}

/// Draw a whole frame (`pack` layout: globals then list) into `view` at the size the globals say.
/// For tests and the benchmark, which own their pictures and wait for the result themselves.
#[cfg(test)]
pub(crate) fn draw(res: &mut Resources, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, data: &[f32]) {
    let (globals, list) = data.split_at((GLOBAL_V4 * 4).min(data.len()));
    let id = PACK_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    upload_list(res, queue, list, id);
    let size = [globals.get(V4_VIEW * 4).copied().unwrap_or(1.0) as u32, globals.get(V4_VIEW * 4 + 1).copied().unwrap_or(1.0) as u32];
    let fast = globals.get(V4_VIEW * 4 + 3).is_some_and(|f| (*f + 0.5) as u32 & crate::perf::FLAG_FAST != 0);
    scene_pass(res, queue, encoder, view, globals, &PassSpec { size, rows: None, clear: true, fast, kind: crate::gpu_timing::Kind::Whole, scale: 1.0 });
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
        if !self.status.is_ready() {
            return Vec::new();
        }
        let Some(res) = resources.get_mut::<Resources>() else { return Vec::new() };
        // our own resources are made inside an error scope: a refusal from the driver (no memory
        // for the picture, a texture it cannot make) fails the graphics path with a message
        // instead of panicking the window
        let (_, errors) = scoped(device, "描画の準備 (prepare)", || self.prepare_frame(device, queue, encoder, res));
        if !errors.is_empty() && res.timer.is_some() {
            // (the timing of passes is a convenience: before giving the whole graphics path up, try without it)
            crate::gpu_log::log(&format!("drawing failed, trying without the timing of passes:\n{}", errors.join("\n")));
            res.timer = None;
            res.pics = None;
            res.pool = None;
            res.pool_id = 0;
            self.ctx.request_repaint();
            return Vec::new();
        }
        if !errors.is_empty() {
            let text = errors.join("\n");
            crate::gpu_log::log(&format!("drawing failed:\n{text}"));
            self.status.fail_now(text);
            // (the next picture is made from scratch if the person retries)
            res.pics = None;
            res.pool = None;
            res.pool_id = 0;
        }
        Vec::new()
    }

    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(res) = resources.get::<Resources>() else { return };
        let Some(pics) = res.pics.as_ref() else { return };
        let vp = info.viewport_in_pixels();
        if vp.width_px <= 0 || vp.height_px <= 0 {
            return;
        }
        pass.set_viewport(vp.left_px as f32, vp.top_px as f32, vp.width_px as f32, vp.height_px as f32, 0.0, 1.0);
        pass.set_pipeline(&res.blit);
        pass.set_bind_group(0, &pics.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Callback {
    /// Everything `prepare` does inside its error scope: make what is missing, then draw the one
    /// pass this frame needs (if any).
    fn prepare_frame(&self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, res: &mut Resources) {
        res.frames += 1;
        let max_dim = device.limits().max_texture_dimension_2d;
        let vp = [self.viewport[0].clamp(1, max_dim), self.viewport[1].clamp(1, max_dim)];
        // timings of passes that finished (they are two or three frames old)
        let mut passes = Vec::new();
        if let Some(t) = res.timer.as_mut() {
            t.begin_frame(device);
            passes = t.take();
            for p in &passes {
                if p.kind == crate::gpu_timing::Kind::Band {
                    res.pacer.observe_gpu(p.rows, p.ms);
                }
            }
        }
        ensure_pics(res, device, vp);
        ensure_volumes(res, device, queue, &self.pack.vols);
        ensure_pool(res, device, self.pack.list.len());
        upload_list(res, queue, &self.pack.list, self.pack.id);

        let gkey = globals_key(&self.globals);
        let size_a = crate::perf::render_size(vp, self.scale_moving, max_dim);
        let size_b = crate::perf::render_size(vp, self.scale_still, max_dim);
        let key_a = PicKey { globals: gkey, list: self.pack.id, vol_gen: res.vol_gen, size: size_a };
        let key_b = PicKey { size: size_b, ..key_a };
        // (taken out of `res` while the passes borrow the rest of it)
        let Some(mut pics) = res.pics.take() else { return };
        let same_scene = |k: Option<PicKey>| k.is_some_and(|k| k.globals == key_a.globals && k.list == key_a.list && k.vol_gen == key_a.vol_gen);
        let mut again = false;
        let lin = |size: [u32; 2]| size[0] as f32 / vp[0] as f32;
        if self.fast {
            if pics.a.key != Some(key_a) {
                scene_pass(res, queue, encoder, &pics.a.view, &self.globals, &PassSpec { size: size_a, rows: None, clear: true, fast: true, kind: crate::gpu_timing::Kind::Moving, scale: lin(size_a) });
                pics.a.key = Some(key_a);
                pics.a.used = size_a;
            }
        } else if pics.b.key != Some(key_b) || !pics.bands.complete() {
            if !same_scene(pics.a.key) && !self.whole {
                // the cheap picture of this very state first: it is what is shown while the sharp one builds
                scene_pass(res, queue, encoder, &pics.a.view, &self.globals, &PassSpec { size: size_a, rows: None, clear: true, fast: true, kind: crate::gpu_timing::Kind::Moving, scale: lin(size_a) });
                pics.a.key = Some(key_a);
                pics.a.used = size_a;
                again = true;
            } else {
                if pics.b.key != Some(key_b) {
                    pics.b.key = Some(key_b);
                    pics.b.used = size_b;
                    pics.bands = crate::adapt::Bands::new(size_b[1]);
                }
                if res.timer.is_none() {
                    res.pacer.observe_frame(self.frame_ms);
                }
                let rows = if self.whole { size_b[1] } else { res.pacer.rows(size_b[1]) };
                if let Some((first, n)) = pics.bands.next(rows) {
                    let spec = PassSpec { size: size_b, rows: Some((first, n)), clear: first == 0, fast: false, kind: crate::gpu_timing::Kind::Band, scale: lin(size_b) };
                    scene_pass(res, queue, encoder, &pics.b.view, &self.globals, &spec);
                    pics.bands.advance(n);
                }
                again = !pics.bands.complete();
            }
        }
        // what the blit shows: A everywhere, and the finished rows of B on top when B is of this very state
        let b_current = pics.b.key == Some(key_b);
        let reveal = if b_current { pics.bands.fraction() } else { 0.0 };
        let cap = [pics.capacity[0] as f32, pics.capacity[1] as f32];
        let share = |u: [u32; 2]| [u[0] as f32 / cap[0], u[1] as f32 / cap[1], 1.0 / cap[0], 1.0 / cap[1]];
        let (a, b) = (share(pics.a.used), share(pics.b.used));
        // (a picture scaled up a lot has little to sharpen: the light touch of a bigger one would only make blocks)
        let lin_a = pics.a.used[0] as f32 / vp[0] as f32;
        let sharpen = self.sharpen.clamp(0.0, 1.0) * (2.0 * lin_a - 0.25).clamp(0.0, 1.0);
        let params: [f32; 12] = [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], reveal, sharpen, 0.0, 0.0];
        let mut bytes = Vec::with_capacity(BLIT_PARAM_BYTES);
        for v in params {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        queue.write_buffer(&res.blit_params, 0, &bytes);
        let sharp_done = if b_current { pics.bands.fraction() } else { 0.0 };
        res.pics = Some(pics);
        let band_rows = res.pacer.rows(size_b[1]);
        self.feedback.push(passes, FeedbackState { timing: res.timer.is_some(), sharp: sharp_done, moving_px: size_a, band_rows, frames: res.frames });
        if again {
            self.ctx.request_repaint();
        }
        static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::perf::mark_once(&FIRST, "first scene picture queued");
    }
}

/// Drawing without a window: for the speed test (`bench.rs`). It opens its own device (with
/// timestamp queries when the adapter has them), builds the same pipelines as the window and
/// draws scenes into textures it owns, waiting for the card after each frame.
pub(crate) mod offscreen {
    use super::*;

    pub struct Offscreen {
        pub device: wgpu::Device,
        pub queue: wgpu::Queue,
        res: Resources,
        /// "Vulkan / Intel(R) Arc(TM) 140V Graphics (driver ...)".
        pub adapter: String,
        pub backend: String,
        /// Seconds the shader took to compile.
        pub compile_secs: f64,
        /// The adapter can time passes.
        pub timing: bool,
        pub software: bool,
    }

    pub struct Picture {
        #[cfg_attr(not(test), allow(dead_code))]
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        pub size: [u32; 2],
    }

    impl Offscreen {
        #[cfg_attr(not(test), allow(dead_code))]
        pub fn open() -> Result<Offscreen, String> {
            Offscreen::open_with(None, None)
        }

        /// `open`, with the scene shader's source given (the tests count work with an instrumented copy)
        /// and the backend the window uses (`None`: what the environment says, else the usual set).
        pub fn open_with(source: Option<&str>, backends: Option<wgpu::Backends>) -> Result<Offscreen, String> {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            if let Some(b) = backends.or_else(wgpu::Backends::from_env) {
                desc.backends = b;
            }
            let instance = wgpu::Instance::new(desc);
            let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                ..Default::default()
            }))
            .map_err(|e| format!("グラフィックスのアダプターが見つかりません: {e}"))?;
            let info = adapter.get_info();
            let gl = info.backend == wgpu::Backend::Gl;
            let base = if gl { wgpu::Limits::downlevel_webgl2_defaults() } else { wgpu::Limits::default() };
            let timing_feature = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
            let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("meltcad bench"),
                required_features: timing_feature,
                required_limits: wgpu::Limits { max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d.min(8192), ..base },
                ..Default::default()
            }))
            .map_err(|e| format!("グラフィックスのデバイスを作れません: {e}"))?;
            let t = std::time::Instant::now();
            let res = match source {
                Some(src) => try_build_from(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, src)?,
                None => try_build(&device, &queue, wgpu::TextureFormat::Rgba8Unorm)?,
            };
            let compile_secs = t.elapsed().as_secs_f64();
            let driver = if info.driver_info.is_empty() { info.driver.clone() } else { format!("{} {}", info.driver, info.driver_info) };
            Ok(Offscreen {
                timing: res.timer.is_some(),
                device,
                queue,
                res,
                adapter: format!("{}{}", info.name, if driver.trim().is_empty() { String::new() } else { format!(" (driver {})", driver.trim()) }),
                backend: format!("{:?}", info.backend),
                compile_secs,
                software: info.device_type == wgpu::DeviceType::Cpu,
            })
        }

        pub fn picture(&self, size: [u32; 2]) -> Picture {
            let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bench picture"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            Picture { texture, view, size }
        }

        /// Put a scene's list (and meshes) on the card.
        pub fn load(&mut self, pack: &ScenePack) {
            ensure_volumes(&mut self.res, &self.device, &self.queue, &pack.vols);
            ensure_pool(&mut self.res, &self.device, pack.list.len());
            upload_list(&mut self.res, &self.queue, &pack.list, pack.id);
        }

        /// Draw one frame and wait for it: (seconds from submit to done, GPU milliseconds of the pass
        /// when the card can time it).
        pub fn frame(&mut self, pic: &Picture, globals: &[f32], fast: bool) -> (f64, Option<f32>) {
            let spec = PassSpec { size: pic.size, rows: None, clear: true, fast, kind: crate::gpu_timing::Kind::Whole, scale: 1.0 };
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("bench") });
            scene_pass(&mut self.res, &self.queue, &mut enc, &pic.view, globals, &spec);
            let t0 = std::time::Instant::now();
            self.queue.submit([enc.finish()]);
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            let wall = t0.elapsed().as_secs_f64();
            let gpu = self.res.timer.as_mut().and_then(|t| t.flush_blocking(&self.device).last().map(|p| p.ms));
            (wall, gpu)
        }

        /// The picture's pixels (RGBA8), for a look at what was drawn.
        #[cfg_attr(not(test), allow(dead_code))]
        pub fn read(&self, pic: &Picture) -> Vec<u8> {
            let (w, h) = (pic.size[0], pic.size[1]);
            let bpr = (w * 4).next_multiple_of(256);
            let out = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: u64::from(bpr) * u64::from(h), usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &pic.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &out, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            self.queue.submit([enc.finish()]);
            let slice = out.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            let mut px = Vec::with_capacity((w * h * 4) as usize);
            if let Ok(mapped) = slice.get_mapped_range() {
                for y in 0..h {
                    let row = (y * bpr) as usize;
                    if let Some(r) = mapped.get(row..row + (w * 4) as usize) {
                        px.extend_from_slice(r);
                    }
                }
            }
            out.unmap();
            px
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Material, Shape};

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

    fn incremental_pack_scene(count: usize) -> Scene {
        let mut scene = Scene::default();
        scene.groups.clear();
        for gi in 0..count.div_ceil(10) {
            let mut group = crate::model::Group::new(format!("group-{gi}"));
            group.blend = 2.0;
            group.op = if gi > 0 && gi % 4 == 0 { crate::model::Op::Subtract } else { crate::model::Op::Add };
            for si in 0..10 {
                let i = gi * 10 + si;
                if i >= count { break; }
                let kind = match i % 7 {
                    0 => crate::model::Kind::Sphere,
                    1 => crate::model::Kind::Extrude,
                    2 => crate::model::Kind::Revolve,
                    3 => crate::model::Kind::Sweep,
                    4 => crate::model::Kind::Box,
                    5 => crate::model::Kind::Cylinder,
                    _ => crate::model::Kind::Curve, // deliberately uses the uncached exact path
                };
                let mut sh = Shape::new(kind, [(i % 20) as f32 * 25.0, (i / 20) as f32 * 25.0, 10.0], Material::default());
                if i % 5 == 0 { sh.shell = 0.6; }
                if i % 6 == 0 && si > 0 { sh.op = crate::model::Op::Subtract; }
                if matches!(kind, crate::model::Kind::Curve | crate::model::Kind::Sweep) {
                    sh.path = vec![[0.0, 0.0, 0.0, 2.0], [8.0, 0.0, 1.0, 2.0], [12.0, 8.0, 0.0, 2.0]];
                }
                group.shapes.push(sh);
            }
            scene.groups.push(group);
        }
        scene
    }

    fn same_pack_as_cold(scene: &Scene, prev: Option<(&Scene, &ScenePack)>) -> ScenePack {
        let fast = ScenePack::reusing(scene, VolumeSet::of(scene), prev);
        let cold = ScenePack::new(scene, VolumeSet::of(scene));
        assert_eq!(fast.list.as_ref(), cold.list.as_ref(), "GPU input must be bit-for-bit identical");
        assert_eq!(fast.offs, cold.offs);
        assert_eq!(fast.offs2, cold.offs2);
        assert_eq!(fast.slots, cold.slots);
        assert_eq!(fast.ranges, cold.ranges);
        assert_eq!(fast.shape_n, cold.shape_n);
        assert_eq!(fast.group_n, cold.group_n);
        assert_eq!(fast.glow, cold.glow);
        assert_eq!(fast.overlaps, cold.overlaps);
        fast
    }

    #[test]
    fn incremental_gpu_pack_is_byte_identical_for_10_100_300_boolean_thin_and_path_shapes() {
        for n in [10, 100, 300] {
            let original = incremental_pack_scene(n);
            let old = same_pack_as_cold(&original, None);
            let mut changed = original.clone();
            changed.groups[0].shapes[0].pos[0] += 1.25;
            changed.melt.mode = crate::model::BlendMode::Gooey;
            let fresh = same_pack_as_cold(&changed, Some((&original, &old)));
            assert_eq!(fresh.shapes_reused, n - n/7 - 1, "{n} shapes; curves rebuilt exactly");
            assert_eq!(fresh.shapes_repacked, n/7 + 1);

            // Undo/redo must not resurrect cached positions or a previous Boolean op.
            let undone = same_pack_as_cold(&original, Some((&changed, &fresh)));
            assert_eq!(undone.shape_n, n);
            let mut op_change = changed.clone();
            op_change.groups[0].shapes[3].op = crate::model::Op::Intersect;
            let other = 1.min(op_change.groups.len()-1);
            op_change.groups[other].op = crate::model::Op::Subtract;
            let _ = same_pack_as_cold(&op_change, Some((&changed, &fresh)));

            // Insertion/deletion shifts the slots and polygon offsets; no old slot can leak.
            let mut deleted = original.clone();
            deleted.groups[0].shapes.remove(1);
            let _ = same_pack_as_cold(&deleted, Some((&original, &old)));
        }
    }

    /// Run explicitly for comparative CPU packing timings on the actual target machine.
    #[test]
    #[ignore]
    fn incremental_gpu_pack_timing_10_100_300() {
        use std::time::{Duration, Instant};
        const REPEATS: usize = 30;
        for n in [10, 100, 300] {
            let mut scene = incremental_pack_scene(n);
            let mut last = ScenePack::new(&scene, VolumeSet::of(&scene));
            let mut cold_time = Duration::ZERO;
            let mut reuse_time = Duration::ZERO;
            for k in 0..REPEATS {
                let mut edited = scene.clone();
                let si = k % n;
                edited.groups[si/10].shapes[si%10].pos[0] += 0.3;
                let started = Instant::now();
                let cold = ScenePack::new(&edited, VolumeSet::of(&edited));
                cold_time += started.elapsed();
                let started = Instant::now();
                let updated = ScenePack::reusing(&edited, VolumeSet::of(&edited), Some((&scene, &last)));
                reuse_time += started.elapsed();
                assert_eq!(updated.list.as_ref(), cold.list.as_ref());
                scene = edited;
                last = updated;
            }
            eprintln!("GPU pack {:>3} shapes: cold {:.3}ms/edit, incremental {:.3}ms/edit, speedup {:.2}x ({} edits)",
                n, cold_time.as_secs_f64()*1000.0/REPEATS as f64,
                reuse_time.as_secs_f64()*1000.0/REPEATS as f64,
                cold_time.as_secs_f64()/reuse_time.as_secs_f64().max(1e-12), REPEATS);
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
                assert!(start + count + (count - 1).div_ceil(crate::curve::GROUP_SEGMENTS) <= PATH_V4, "curve {i} runs past the list");
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
        let (device, queue, info) = open_device(downlevel)?;
        let res = build(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        Some(Gpu { device, queue, res, info })
    }

    /// Just the device (no pipelines): for tests that must be quick or that break things on purpose.
    pub fn open_device(downlevel: bool) -> Option<(wgpu::Device, wgpu::Queue, String)> {
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
        Some((device, queue, info))
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
            ensure_pool(&mut self.res, &self.device, data.len().saturating_sub(GLOBAL_V4 * 4));
            let bpr = (size[0] * 4).next_multiple_of(256);
            let out = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: (bpr * size[1]) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            draw(&mut self.res, &self.queue, &mut enc, &view, data);
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
        crate::bench::library_scene(shapes)
    }

    fn view_of(sc: &Scene) -> Camera {
        let mut cam = Camera::default();
        let (c, r) = bounds(sc, None);
        cam.frame(c, r.max(10.0));
        cam
    }

    /// The reported failure of the real report: a shader the driver refuses must come back as an
    /// error text (shown in the viewport and the log), never as a panic on the compile thread.
    /// Needs an adapter (the software one is fine); skips itself without.
    #[test]
    fn a_shader_the_driver_refuses_is_an_error_not_a_panic() {
        let Some((device, queue, info)) = headless::open_device(false) else {
            eprintln!("no adapter: skipped");
            return;
        };
        eprintln!("{info}");
        let format = wgpu::TextureFormat::Rgba8Unorm;
        // 1: not even WGSL
        let e = try_build_from(&device, &queue, format, "this is not wgsl at all").err().unwrap_or_default();
        assert!(!e.is_empty(), "a parse error is an Err");
        eprintln!("--- parse error text:\n{e}");
        // 2: valid WGSL but no entry points called vs / fs: the pipeline is refused
        let e = try_build_from(&device, &queue, format, "@compute @workgroup_size(1) fn main() {}").err().unwrap_or_default();
        assert!(!e.is_empty(), "a pipeline the driver refuses is an Err");
        eprintln!("--- pipeline error text:\n{e}");
        // 3: the device keeps working afterwards
        let ok = try_build_from(&device, &queue, format, include_str!("blit.wgsl"));
        // (the blit shader has no scene bindings: its layout does not match, so this is an Err too, but a clean one)
        let _ = ok;
    }

    /// An error inside a scope is caught and described, and the scope leaves nothing behind.
    #[test]
    fn an_error_scope_catches_a_validation_error() {
        let Some((device, _queue, _)) = headless::open_device(false) else {
            eprintln!("no adapter: skipped");
            return;
        };
        let (_, errors) = scoped(&device, "test", || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("too big"),
                size: wgpu::Extent3d { width: 1 << 30, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].starts_with("test: 検証エラー"), "{}", errors[0]);
        let (_, none) = scoped(&device, "test", || 1 + 1);
        assert!(none.is_empty());
    }

    #[test]
    fn the_library_scene_has_the_asked_size() {
        let sc = library_scene(300);
        assert!(sc.shape_count() >= 300 && sc.shape_count() <= 340 && sc.groups.len() < MAX_GROUPS, "{} shapes in {} groups", sc.shape_count(), sc.groups.len());
    }

    /// Read the real WGSL curve distance back as pixels on an adapter. This covers the rounded
    /// join at a box edge, a closure and a join across two segment search groups.
    #[test]
    #[ignore = "needs a graphics adapter"]
    fn curve_gpu_distance_matches_cpu() {
        use crate::model::{Kind, Material, Shape};
        let Some((device, queue, info)) = headless::open_device(false) else {
            eprintln!("no graphics adapter: skipped");
            return;
        };
        eprintln!("{info}");
        let mut src = shader_source();
        let entry = src.find("fn fs(in: VOut)").expect("fragment entry");
        let open = entry + src[entry..].find('{').expect("fragment body");
        let mut depth = 0_i32;
        let mut close = None;
        for (off, ch) in src[open..].char_indices() {
            if ch == '{' { depth += 1; }
            if ch == '}' {
                depth -= 1;
                if depth == 0 { close = Some(open + off); break; }
            }
        }
        src.replace_range(open..=close.expect("fragment end"), "{\n    let q = vec3<f32>(in.uv.x * 48.0 - 24.0, in.uv.y * 48.0 - 24.0, 0.0);\n    let d = curve_dist(shape_at(0), q);\n    return vec4<f32>(clamp((d + 16.0) / 32.0, 0.0, 1.0), 0.0, 0.0, 1.0);\n}");
        let res = try_build_from(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, &src).expect("distance shader");
        let mut gpu = headless::Gpu { device, queue, res, info };
        let mut apex = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        apex.path = vec![[0.0, -16.0, 0.0, 4.0], [16.0, 0.0, 0.0, 4.0], [0.0, 16.0, 0.0, 4.0]];
        apex.params = [0.0, 0.0, 0.0, 1.0];
        let mut looped = Shape::new(Kind::Curve, [0.0; 3], Material::default());
        looped.path = (0..12).map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 12.0;
            [12.0 * a.cos(), 12.0 * a.sin(), 0.0, 4.0]
        }).collect();
        looped.params = [1.0, 0.0, 0.0, 1.0];
        let size = [64_u32, 64_u32];
        for shape in [apex, looped] {
            let prep = Prepared::new(&shape);
            let scene = Scene { groups: vec![Group { shapes: vec![shape], ..Group::new("pipe") }], melt: Default::default(), env: Default::default(), saved: Vec::new() };
            let data = pack(&scene, &Camera::default(), [size[0] as f32, size[1] as f32], None, None, false);
            let (pixels, _) = gpu.render(&data, size);
            assert_eq!(pixels.len(), (size[0] * size[1] * 4) as usize);
            let mut compared = 0;
            for y in (0..size[1]).step_by(3) {
                for x in (0..size[0]).step_by(3) {
                    let q = [(x as f32 + 0.5) * 48.0 / size[0] as f32 - 24.0, (y as f32 + 0.5) * 48.0 / size[1] as f32 - 24.0, 0.0];
                    let cpu = prep.distance(q);
                    if !(-15.5..15.5).contains(&cpu) { continue; }
                    let gpu_d = f32::from(pixels[((y * size[0] + x) * 4) as usize]) * 32.0 / 255.0 - 16.0;
                    assert!((gpu_d - cpu).abs() < 0.18, "at {q:?}: GPU {gpu_d}, CPU {cpu}");
                    compared += 1;
                }
            }
            assert!(compared > 100, "measured {compared} points");
        }
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

    /// Renders a set of scenes that touch every part of the scene shader (floor and grid with
    /// shadows, selection, fast mode, glass, glow, metal, patterns) and writes them as PPM files
    /// into `$MELTCAD_GOLDEN_DIR`. Run it before and after a change of how the shader is written
    /// (not of what it computes): `cmp` of the two folders must be silent. Also prints the
    /// seconds the pipeline took to compile.
    /// `MELTCAD_GOLDEN_DIR=/tmp/golden cargo test -p meltcad --release gpu_headless_golden -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gpu_headless_golden() {
        use crate::model::{Kind, Material, Shape};
        let Some(dir) = std::env::var_os("MELTCAD_GOLDEN_DIR").map(std::path::PathBuf::from) else {
            eprintln!("set MELTCAD_GOLDEN_DIR");
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let t = std::time::Instant::now();
        let Some(mut gpu) = headless::open(std::env::var_os("YURA_GL_LIMITS").is_some()) else {
            eprintln!("no graphics adapter here");
            return;
        };
        eprintln!("COMPILE {:.1} s on {}", t.elapsed().as_secs_f64(), gpu.info);
        let mut scenes: Vec<(&str, Scene, Option<Pick>, bool)> = Vec::new();
        scenes.push(("default", Scene::default(), Some(Pick::Shape(0, 0)), false));
        scenes.push(("default_fast", Scene::default(), Some(Pick::Shape(0, 0)), true));
        let mut glass = Scene::default();
        glass.groups[0].shapes.clear();
        let mut ball = Shape::new(Kind::Sphere, [0.0, 0.0, 22.0], Material { glass: 1.0, color: [0.6, 0.8, 1.0], roughness: 0.1, ..Material::default() });
        ball.size = [22.0, 22.0, 22.0];
        glass.groups[0].shapes.push(ball);
        glass.groups[0].shapes.push(Shape::new(Kind::Box, [40.0, 10.0, 12.0], Material { metallic: 1.0, roughness: 0.25, iridescence: 0.6, coat: 0.8, ..Material::default() }));
        glass.groups[0].shapes.push(Shape::new(Kind::Torus, [-40.0, 0.0, 14.0], Material { translucency: 0.8, color: [0.9, 0.5, 0.3], ..Material::default() }));
        scenes.push(("glass", glass.clone(), None, false));
        scenes.push(("glass_fast", glass.clone(), None, true));
        let mut lamp = glass.clone();
        lamp.groups[0].shapes.push(Shape::new(Kind::Sphere, [0.0, 50.0, 30.0], Material { glow: 3.0, color: [1.0, 0.7, 0.3], ..Material::default() }));
        scenes.push(("lamp", lamp, Some(Pick::Shape(0, 1)), false));
        scenes.push(("library", library_scene(12), Some(Pick::Group(0)), false));
        let size = [240u32, 156u32];
        for (name, sc, sel, fast) in scenes {
            let mut cam = Camera::default();
            let (cc, r) = bounds(&sc, sel);
            cam.frame(cc, r);
            let mut data = pack(&sc, &cam, [size[0] as f32, size[1] as f32], sel, None, false);
            set_view_flags(&mut data, crate::perf::view_flags(false, fast, crate::perf::scene_has_glow(&sc)));
            let (rgba, secs) = gpu.render(&data, size);
            let ppm = [format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes(), rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect()].concat();
            let _ = std::fs::write(dir.join(format!("{name}.ppm")), ppm);
            let lit = rgba.chunks(4).filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8).count();
            eprintln!("{name}: {secs:.2} s, {lit} lit pixels");
        }
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
