// MELTCAD viewport: sphere-traces the blob scene. The formulas match sdf.rs.

struct Shape {
    r0: vec4<f32>,      // inverse rotation row 0
    r1: vec4<f32>,
    r2: vec4<f32>,
    pos: vec4<f32>,     // xyz position, w = smallest scale
    scale: vec4<f32>,   // xyz scale, w = kind
    size: vec4<f32>,    // size 0..2, w = rounding
    misc: vec4<f32>,    // x = op sign, y = blend, z = outline start (point index), w = outline count
    color: vec4<f32>,   // rgb, roughness
    mat: vec4<f32>,     // metallic, glow, glass, iridescence
    mat2: vec4<f32>,    // x = clear coat, y = translucency, z = signed 1 / gradient length
    params: vec4<f32>,  // kind-specific extra numbers
    ext: vec4<f32>,     // x = path start, y = path count, z = outline2 start, w = outline2 count
    extra: vec4<f32>,   // x = shell wall thickness (0 = solid), y z = profile rounding limit, winding, w = 1: chamfer instead of round
    gcol: vec4<f32>,    // gradient: second colour rgb, strength (0 = no gradient)
    gpar: vec4<f32>,    // gradient: direction (0 x, 1 y, 2 z; +3 = along the shape's own box), softness, from, to
};

// Everything per frame and small is in the uniform buffer; the scene itself (groups, blocks,
// shapes, outlines, path nodes) is one long list of vec4 in a float texture read with textureLoad,
// so the number of shapes is not limited by the 64 KB uniform size and it works on every backend.
struct Globals {
    cam_pos: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    cam_fwd: vec4<f32>,   // w = tan(fov / 2); 0 = orthographic
    view: vec4<f32>,      // width px, height px, ortho half height, flags: 1 = srgb target, 2 = fast mode, 4 = a lamp glows
    melt: vec4<f32>,      // mode, amount, drip, ripple
    counts: vec4<f32>,    // groups, shapes, selected first shape (-1 none), selected count
    bound: vec4<f32>,     // centre, radius of a sphere around everything (0 = nothing)
    hover: vec4<f32>,     // first hovered shape (-1 none), count
    selsph: vec4<f32>,    // bounding sphere of the selected shapes (centre, radius)
    offs: vec4<f32>,      // first vec4 of the group table, the block table, the instance table, the shapes
    offs2: vec4<f32>,     // first vec4 of the outline points, the path nodes
    env: array<vec4<f32>, 11>,      // exposure/ambient/shadow/flags, backdrop+lights, 3 sky colours, 3 x (light dir+power, light colour)
};

@group(0) @binding(0) var<uniform> G: Globals;
@group(0) @binding(1) var DT: texture_2d<f32>;

// The scene list: 1024 vec4 per texture row.
fn D(i: i32) -> vec4<f32> { return textureLoad(DT, vec2<i32>(i & 1023, i >> 10), 0); }

// Layout (keep in step with gpu.rs):
//   group  (3 vec4): [first block, block count, op sign, blend] [first instance, instance count, 0, 0] [bounding sphere]
//   block  (2 vec4): [bounding sphere] [largest blend, 1 = never skip, first shape, shape count]
//   instance (4 vec4): three rows (x y z, offset) taking a world point back into the group's own space, then [world centre of the group's sphere, scale]
//   shape (16 vec4): [bounding sphere = position, radius] then the 15 vec4 of struct Shape
fn group_at(gi: i32) -> i32 { return i32(G.offs.x + 0.5) + gi * 3; }
fn block_at(bi: i32) -> i32 { return i32(G.offs.y + 0.5) + bi * 2; }
fn inst_at(i: i32) -> i32 { return i32(G.offs.z + 0.5) + i * 4; }
fn shape_rec(i: i32) -> i32 { return i32(G.offs.w + 0.5) + i * 16; }
fn path_pt(i: i32) -> vec4<f32> { return D(i32(G.offs2.y + 0.5) + i); }

fn shape_at(i: i32) -> Shape {
    let b = shape_rec(i) + 1;
    var s: Shape;
    s.r0 = D(b);
    s.r1 = D(b + 1);
    s.r2 = D(b + 2);
    s.pos = D(b + 3);
    s.scale = D(b + 4);
    s.size = D(b + 5);
    s.misc = D(b + 6);
    s.color = D(b + 7);
    s.mat = D(b + 8);
    s.mat2 = D(b + 9);
    s.params = D(b + 10);
    s.ext = D(b + 11);
    s.extra = D(b + 12);
    s.gcol = D(b + 13);
    s.gpar = D(b + 14);
    return s;
}

fn flag(bit: i32) -> bool { return (i32(G.view.w + 0.5) & bit) != 0; }
fn is_fast() -> bool { return flag(2); }

// How far away a shape must be (at least) for a blend with value `a` to ignore it: beyond that
// the soft minimum returns `a` exactly. Round blends also feel any shape closer than their radius.
fn far_enough(a: f32, k: f32, rm: f32, mode: i32) -> f32 {
    var need = rm * k + a + 0.02;
    if (mode == 1) { need = max(need, rm * k + 0.02); }
    return need;
}

// a stain leaves the surface alone beyond this
fn paint_far(k: f32) -> f32 { return 0.5 * max(k * 0.35, 0.3) + 0.02; }

// a whole block of shapes is skipped beyond this (adds, carves and stains alike)
fn block_far(g: f32, kmax: f32, rm: f32) -> f32 {
    return max(rm * kmax + abs(g) + 0.02, paint_far(kmax));
}

// how far a blend reaches, in units of the blend size k (the soft minimum is exact beyond it)
fn reach_mult() -> f32 {
    let mode = i32(G.melt.x + 0.5);
    if (mode == 0) { return 1.0; }
    if (mode == 1) { return 1.3; }
    return 6.0;
}

struct Hit {
    d: f32,
    col: vec3<f32>,
    rough: f32,
    metal: f32,
    glow: f32,
    glass: f32,
    irid: f32,
    coat: f32,
    trans: f32,
    id: f32,
};

fn smin_h(a: f32, b: f32, k: f32, mode: i32) -> vec2<f32> {
    // returns (distance, how much of a is kept)
    if (k <= 1e-6) {
        if (a < b) { return vec2<f32>(a, 1.0); }
        return vec2<f32>(b, 0.0);
    }
    if (mode == 0) {
        let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
        return vec2<f32>(mix(b, a, h) - k * h * (1.0 - h), h);
    }
    if (mode == 1) {
        let r = k * 1.3;
        let u = vec2<f32>(max(r - a, 0.0), max(r - b, 0.0));
        let d = max(r, min(a, b)) - length(u);
        return vec2<f32>(d, clamp(0.5 + 0.5 * (b - a) / r, 0.0, 1.0));
    }
    let kk = k * 6.0;
    let h = max(kk - abs(a - b), 0.0) / kk;
    let m = h * h * h * 0.5;
    let s = m * kk / 3.0;
    if (a < b) { return vec2<f32>(a - s, 1.0 - m * 0.5); }
    return vec2<f32>(b - s, m * 0.5);
}

fn polygon_d(start: i32, n: i32, p: vec2<f32>) -> f32 {
    if (n < 3) { return 1e9; }
    let v0 = get_pt(start);
    var d = dot(p - v0, p - v0);
    var sgn = 1.0;
    var j = n - 1;
    for (var i = 0; i < n; i = i + 1) {
        let vi = get_pt(start + i);
        let vj = get_pt(start + j);
        let e = vj - vi;
        let w = p - vi;
        let t = clamp(dot(w, e) / max(dot(e, e), 1e-12), 0.0, 1.0);
        let b = w - e * t;
        d = min(d, dot(b, b));
        let c1 = p.y >= vi.y;
        let c2 = p.y < vj.y;
        let c3 = e.x * w.y > e.y * w.x;
        if ((c1 && c2 && c3) || (!c1 && !c2 && !c3)) { sgn = -sgn; }
        j = i;
    }
    return sgn * sqrt(d);
}

fn get_pt(i: i32) -> vec2<f32> {
    let v = D(i32(G.offs2.x + 0.5) + i / 2);
    if ((u32(i) & 1u) == 0u) { return v.xy; }
    return v.zw;
}

fn shape_dist(s: Shape, p: vec3<f32>) -> f32 {
    let d = p - s.pos.xyz;
    let q = vec3<f32>(dot(s.r0.xyz, d), dot(s.r1.xyz, d), dot(s.r2.xyz, d)) / s.scale.xyz;
    let kind = i32(s.scale.w + 0.5);
    let sz = s.size.xyz;
    let rd = s.size.w;
    let chamfer = s.extra.w > 0.5 && rd > 0.0;
    var r = 0.0;
    if (kind == 0) {
        r = length(q) - sz.x;
    } else if (kind == 1) {
        let b = sz * 0.5;
        let rr = min(rd, min(b.x, min(b.y, b.z)));
        let dd = abs(q) - b + vec3<f32>(rr);
        r = length(max(dd, vec3<f32>(0.0))) + min(max(dd.x, max(dd.y, dd.z)), 0.0) - rr;
        if (chamfer) { r = cn_cut_box(q, b, rr); }
    } else if (kind == 2) {
        let rr = min(rd, min(sz.x, sz.y * 0.5));
        let dd = vec2<f32>(length(q.xy) - sz.x + rr, abs(q.z) - sz.y * 0.5 + rr);
        r = min(max(dd.x, dd.y), 0.0) + length(max(dd, vec2<f32>(0.0))) - rr;
        if (chamfer) { r = cn_pair(length(q.xy) - sz.x, abs(q.z) - sz.y * 0.5, rr); }
    } else if (kind == 3) {
        let a = length(q.xy) - sz.x;
        r = length(vec2<f32>(a, q.z)) - sz.y;
    } else if (kind == 4 && chamfer) {
        // a cone with its base rim and its tip cut off
        let hh = sz.y * 0.5;
        let pc = vec2<f32>(length(q.xy), q.z);
        let ca = vec2<f32>(-sz.x, -hh);
        let cb = vec2<f32>(sz.x, -hh);
        let ct = vec2<f32>(0.0, hh);
        var dc = pm_sd_triangle(pc, ca, cb, ct);
        dc = cn_cut_vertex(dc, pc, ca, cb, ct, 1.0, rd);
        r = cn_cut_vertex(dc, pc, cb, ct, ca, 1.0, rd);
    } else if (kind == 4 && rd > 0.0) {
        // a rounded cone: its profile triangle shrunk about the incentre, then grown again
        let hh = sz.y * 0.5;
        let slant = length(vec2<f32>(sz.x, sz.y));
        let rin = max(sz.x * sz.y / max(sz.x + slant, 1e-6), 1e-6);
        let rr = min(rd, rin * 0.98);
        let k = (rin - rr) / rin;
        let c = vec2<f32>(0.0, -hh + rin);
        r = pm_sd_triangle(vec2<f32>(length(q.xy), q.z), c + (vec2<f32>(-sz.x, -hh) - c) * k, c + (vec2<f32>(sz.x, -hh) - c) * k, c + (vec2<f32>(0.0, hh) - c) * k) - rr;
    } else if (kind == 4) {
        let h = sz.y * 0.5;
        let r1 = sz.x;
        let r2 = 0.0;
        let qx = length(q.xy);
        let qy = q.z;
        let k1 = vec2<f32>(r2, h);
        let k2 = vec2<f32>(r2 - r1, 2.0 * h);
        var lim = r2;
        if (qy < 0.0) { lim = r1; }
        let ca = vec2<f32>(qx - min(qx, lim), abs(qy) - h);
        let t = clamp(dot(k1 - vec2<f32>(qx, qy), k2) / max(dot(k2, k2), 1e-9), 0.0, 1.0);
        let cb = vec2<f32>(qx, qy) - k1 + k2 * t;
        var sg = 1.0;
        if (cb.x < 0.0 && ca.y < 0.0) { sg = -1.0; }
        r = sg * sqrt(min(dot(ca, ca), dot(cb, cb)));
    } else if (kind == 18) {
        r = curve_dist(s, q);
    } else if (kind == 20) {
        r = mesh_dist(s, q);
    } else if (kind >= 16) {
        r = cad_dist(s, q, kind);
    } else if (kind >= 6) {
        r = prim_dist(s, q, kind);
    } else {
        let st = i32(s.misc.z + 0.5);
        let cnt = i32(s.misc.w + 0.5);
        let hh = sz.x * 0.5;
        var d2 = polygon_d(st, cnt, q.xy);
        if (chamfer) {
            d2 = cn_cut_polygon(d2, q.xy, st, cnt, s.extra.z, rd, false);
            r = cn_pair(d2, abs(q.z) - hh, min(rd, hh));
        } else {
            // every convex corner of the outline and the top and bottom edges are rounded
            var rr = 0.0;
            if (rd > 0.0) { rr = min(min(rd, hh), s.extra.y); }
            if (rr > 0.0) { d2 = polygon_inset_d(st, cnt, q.xy, rr, s.extra.z, false); }
            let w = vec2<f32>(d2, abs(q.z) - hh + rr);
            r = min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - rr;
        }
    }
    r = r * s.pos.w;
    // hollow walls of thickness `shell`
    let sh = s.extra.x;
    if (sh > 0.0 && kind != 18) { r = abs(r + sh * 0.5) - sh * 0.5; }
    return r;
}

// The surface colour of a shape at p: its colour, or the gradient (Material::gradient_weight).
fn shape_color(s: Shape, p: vec3<f32>) -> vec3<f32> {
    if (s.gcol.w <= 0.001) { return s.color.rgb; }
    let code = i32(s.gpar.x + 0.5);
    let ax = code % 3;
    let d = p - s.pos.xyz;
    var c = d[ax];
    if (code >= 3) {
        let q = vec3<f32>(dot(s.r0.xyz, d), dot(s.r1.xyz, d), dot(s.r2.xyz, d)) / s.scale.xyz;
        c = q[ax];
    }
    let t = clamp(c * s.mat2.z + 0.5, 0.0, 1.0);
    let u = clamp((t - s.gpar.z) / max(s.gpar.w - s.gpar.z, 0.02), 0.0, 1.0);
    let w = max(s.gpar.y * 0.5, 0.001);
    let k = smoothstep(0.5 - w, 0.5 + w, u);
    return mix(s.color.rgb, s.gcol.rgb, k * s.gcol.w);
}

fn mix_hit(a: Hit, b: Hit, t: f32) -> Hit {
    // b at weight 0, a at weight 1
    var o: Hit;
    o.d = 0.0;
    o.col = mix(b.col, a.col, t);
    o.rough = mix(b.rough, a.rough, t);
    o.metal = mix(b.metal, a.metal, t);
    o.glow = mix(b.glow, a.glow, t);
    o.glass = mix(b.glass, a.glass, t);
    o.irid = mix(b.irid, a.irid, t);
    o.coat = mix(b.coat, a.coat, t);
    o.trans = mix(b.trans, a.trans, t);
    o.id = select(b.id, a.id, t >= 0.5);
    return o;
}

// op: +1 add, -1 carve, 0 paint (colour only), 2 keep only the overlap
fn combine(a: Hit, b: Hit, op: f32, k: f32, mode: i32) -> Hit {
    if (op > 1.5) {
        // smax(a, b) = -smin(-a, -b); the surface belongs to whichever is farther out
        let r = smin_h(-a.d, -b.d, k, mode);
        var o = mix_hit(a, b, r.y);
        o.d = -r.x;
        return o;
    }
    if (op > 0.5) {
        let r = smin_h(a.d, b.d, k, mode);
        var o = mix_hit(a, b, r.y);
        o.d = r.x;
        return o;
    }
    if (op < -0.5) {
        let r = smin_h(-a.d, b.d, k, mode);
        let h = r.y;
        // where the cut is, the surface takes the colour of the cutter
        var o = mix_hit(a, b, 1.0 - clamp(1.0 - h, 0.0, 1.0) * (1.0 - h));
        o.rough = a.rough;
        o.metal = a.metal;
        o.glow = a.glow;
        o.glass = a.glass;
        o.irid = a.irid;
        o.coat = a.coat;
        o.trans = a.trans;
        o.d = -r.x;
        o.id = select(b.id, a.id, h > 0.5);
        return o;
    }
    // a stain: the shape of `a` stays; inside `b` the surface takes b's material
    let soft = max(k * 0.35, 0.3);
    let w = clamp(0.5 - b.d / soft, 0.0, 1.0);
    var o = mix_hit(b, a, w);
    o.d = a.d;
    o.id = a.id;
    return o;
}

fn warp(p: vec3<f32>) -> vec3<f32> {
    let drip = G.melt.z;
    if (drip <= 0.0) { return p; }
    let f = 0.12 * G.melt.w;
    let w = 0.5 + 0.5 * sin(p.x * f + sin(p.y * f * 1.3)) * cos(p.y * f * 0.8 + cos(p.x * f * 0.6));
    return vec3<f32>(p.x, p.y, p.z + drip * w);
}

// The world point `p` carried back into the group's own space by instance record `ib` (`ni` = 0: no instance).
fn to_group(ni: i32, ib: i32, p: vec3<f32>) -> vec3<f32> {
    if (ni <= 0) { return p; }
    let r0 = D(ib);
    let r1 = D(ib + 1);
    let r2 = D(ib + 2);
    return vec3<f32>(dot(r0.xyz, p) + r0.w, dot(r1.xyz, p) + r1.w, dot(r2.xyz, p) + r2.w);
}

fn empty_hit() -> Hit {
    var acc: Hit;
    acc.d = 1e6;
    acc.col = vec3<f32>(0.8);
    acc.rough = 0.5;
    acc.metal = 0.0;
    acc.glow = 0.0;
    acc.glass = 0.0;
    acc.irid = 0.0;
    acc.coat = 0.0;
    acc.trans = 0.0;
    acc.id = -1.0;
    return acc;
}

struct GroupHit {
    h: Hit,
    ok: bool,
};

// The shapes of one group, melted together in order (with materials). `ok` is false when the group
// has no shape to start from.
fn group_hit(gr: vec4<f32>, p: vec3<f32>, amount: f32, rm: f32, mode: i32) -> GroupHit {
    var out: GroupHit;
    out.ok = false;
    let fb = i32(gr.x + 0.5);
    let nb = i32(gr.y + 0.5);
    var g: Hit;
    var gh = false;
    for (var bi = 0; bi < nb; bi = bi + 1) {
        let bb = block_at(fb + bi);
        let b1 = D(bb + 1);
        if (gh && nb > 1 && b1.y < 0.5) {
            // a block that is far away changes nothing
            let b0 = D(bb);
            if (length(p - b0.xyz) - b0.w >= block_far(g.d, b1.x * amount, rm)) { continue; }
        }
        let first = i32(b1.z + 0.5);
        let cnt = i32(b1.w + 0.5);
        for (var si = 0; si < cnt; si = si + 1) {
            let rec = shape_rec(first + si);
            if (gh) {
                // far from this shape the blend leaves the result as it is: skip its distance
                let misc = D(rec + 7);
                let op = misc.x;
                let k = misc.y * amount;
                let bs = D(rec);
                let lb = length(p - bs.xyz) - bs.w;
                if (op > 1.5) {
                    // an overlap shape changes the result everywhere
                } else if (op > 0.5) {
                    if (lb >= far_enough(g.d, k, rm, mode)) { continue; }
                } else if (op < -0.5) {
                    if (lb >= far_enough(-g.d, k, rm, mode)) { continue; }
                } else if (lb >= paint_far(k)) {
                    continue;
                }
            }
            let s = shape_at(first + si);
            var h: Hit;
            h.d = shape_dist(s, p);
            h.col = shape_color(s, p);
            h.rough = s.color.w;
            h.metal = s.mat.x;
            h.glow = s.mat.y;
            h.glass = s.mat.z;
            h.irid = s.mat.w;
            h.coat = s.mat2.x;
            h.trans = s.mat2.y;
            h.id = f32(first + si);
            if (!gh) {
                if (s.misc.x < 0.5) { continue; }
                g = h;
                gh = true;
            } else {
                g = combine(g, h, s.misc.x, s.misc.y * amount, mode);
            }
        }
    }
    out.h = g;
    out.ok = gh;
    return out;
}

// Can group `gr` (whose sphere is `gs`) be left out, given what the groups before it came to?
fn group_far(gr: vec4<f32>, gs: vec4<f32>, p: vec3<f32>, acc_d: f32, amount: f32, rm: f32, mode: i32) -> bool {
    let k = gr.w * amount;
    let lb = length(p - gs.xyz) - gs.w;
    if (gr.z > 1.5) { return false; }
    if (gr.z > 0.5) { return lb >= far_enough(acc_d, k, rm, mode); }
    if (gr.z < -0.5) { return lb >= far_enough(-acc_d, k, rm, mode); }
    return lb >= paint_far(k);
}

fn map(p0: vec3<f32>) -> Hit {
    let p = warp(p0);
    let mode = i32(G.melt.x + 0.5);
    let amount = G.melt.y;
    let rm = reach_mult();
    var acc = empty_hit();
    var have = false;
    let ng = i32(G.counts.x + 0.5);
    for (var gi = 0; gi < ng; gi = gi + 1) {
        let gb = group_at(gi);
        let gr = D(gb);
        let gn = D(gb + 1);
        let gs = D(gb + 2);
        let ni = i32(gn.y + 0.5);
        let fi = i32(gn.x + 0.5);
        // every instance of the group melts into the result in turn (none: the group once, as it is)
        for (var j = 0; j < max(ni, 1); j = j + 1) {
            var sc = 1.0;
            var sphere = gs;
            let ib = inst_at(fi + j);
            if (ni > 0) {
                let ic = D(ib + 3);
                sc = ic.w;
                sphere = vec4<f32>(ic.xyz, gs.w * sc);
            }
            if (have && group_far(gr, sphere, p, acc.d, amount, rm, mode)) { continue; }
            let q = to_group(ni, ib, p);
            var gx = group_hit(gr, q, amount, rm, mode);
            if (!gx.ok) { continue; }
            gx.h.d = gx.h.d * sc;
            if (!have) {
                if (gr.z < 0.5) { continue; }
                acc = gx.h;
                have = true;
            } else {
                acc = combine(acc, gx.h, gr.z, gr.w * amount, mode);
            }
        }
    }
    if (G.melt.z > 0.0) {
        let slope = G.melt.z * 1.32 * 0.12 * G.melt.w;
        acc.d = acc.d / sqrt(1.0 + slope * slope);
    }
    return acc;
}

// The distance of one group alone: x = distance, y = 1 when the group has a shape to start from.
fn group_dist(gr: vec4<f32>, p: vec3<f32>, amount: f32, rm: f32, mode: i32) -> vec2<f32> {
    let fb = i32(gr.x + 0.5);
    let nb = i32(gr.y + 0.5);
    var g = 0.0;
    var gh = false;
    for (var bi = 0; bi < nb; bi = bi + 1) {
        let bb = block_at(fb + bi);
        let b1 = D(bb + 1);
        if (gh && nb > 1 && b1.y < 0.5) {
            let b0 = D(bb);
            if (length(p - b0.xyz) - b0.w >= block_far(g, b1.x * amount, rm)) { continue; }
        }
        let first = i32(b1.z + 0.5);
        let cnt = i32(b1.w + 0.5);
        for (var si = 0; si < cnt; si = si + 1) {
            let rec = shape_rec(first + si);
            let misc = D(rec + 7);
            let op = misc.x;
            let k = misc.y * amount;
            if (!gh) {
                if (op < 0.5) { continue; }
            } else {
                if (op > -0.5 && op < 0.5) { continue; }
                if (op < 1.5) {
                    let bs = D(rec);
                    let lb = length(p - bs.xyz) - bs.w;
                    if (lb >= far_enough(select(-g, g, op > 0.0), k, rm, mode)) { continue; }
                }
            }
            // (one copy of shape_dist keeps the shader small: the driver compiles every call site)
            let b = shape_dist(shape_at(first + si), p);
            if (!gh) {
                g = b;
                gh = true;
            } else if (op > 1.5) {
                g = -smin_h(-g, -b, k, mode).x;
            } else if (op > 0.0) {
                g = smin_h(g, b, k, mode).x;
            } else {
                g = -smin_h(-g, b, k, mode).x;
            }
        }
    }
    return vec2<f32>(g, select(0.0, 1.0, gh));
}

// The distance alone (no materials): what marching, shadows and normals need. It follows map()
// step for step, but skips the shapes that cannot matter and never mixes materials.
fn map_d(p0: vec3<f32>) -> f32 {
    let p = warp(p0);
    let mode = i32(G.melt.x + 0.5);
    let amount = G.melt.y;
    let rm = reach_mult();
    var acc = 1e6;
    var have = false;
    let ng = i32(G.counts.x + 0.5);
    for (var gi = 0; gi < ng; gi = gi + 1) {
        let gb = group_at(gi);
        let gr = D(gb);
        if (gr.z > -0.5 && gr.z < 0.5) { continue; }   // a stain group changes no distance
        let gn = D(gb + 1);
        let gs = D(gb + 2);
        let ni = i32(gn.y + 0.5);
        let fi = i32(gn.x + 0.5);
        for (var j = 0; j < max(ni, 1); j = j + 1) {
            var sc = 1.0;
            var sphere = gs;
            let ib = inst_at(fi + j);
            if (ni > 0) {
                let ic = D(ib + 3);
                sc = ic.w;
                sphere = vec4<f32>(ic.xyz, gs.w * sc);
            }
            if (have && group_far(gr, sphere, p, acc, amount, rm, mode)) { continue; }
            let q = to_group(ni, ib, p);
            let gd = group_dist(gr, q, amount, rm, mode);
            if (gd.y < 0.5) { continue; }
            let gv = gd.x * sc;
            if (!have) {
                if (gr.z < 0.5) { continue; }
                acc = gv;
                have = true;
            } else if (gr.z > 1.5) {
                acc = -smin_h(-acc, -gv, gr.w * amount, mode).x;
            } else if (gr.z > 0.0) {
                acc = smin_h(acc, gv, gr.w * amount, mode).x;
            } else {
                acc = -smin_h(-acc, gv, gr.w * amount, mode).x;
            }
        }
    }
    if (G.melt.z > 0.0) {
        let slope = G.melt.z * 1.32 * 0.12 * G.melt.w;
        acc = acc / sqrt(1.0 + slope * slope);
    }
    return acc;
}

fn dist_only(p: vec3<f32>) -> f32 { return map_d(p); }

fn normal_at(p: vec3<f32>, e: f32) -> vec3<f32> {
    let k = vec2<f32>(1.0, -1.0);
    return normalize(
        k.xyy * dist_only(p + k.xyy * e) + k.yyx * dist_only(p + k.yyx * e) +
        k.yxy * dist_only(p + k.yxy * e) + k.xxx * dist_only(p + k.xxx * e));
}

// distance to the selected shapes only (for the outline)
fn sel_dist(p0: vec3<f32>) -> f32 {
    let p = warp(p0);
    let first = i32(G.counts.z + 0.5);
    let cnt = i32(G.counts.w + 0.5);
    var d = 1e6;
    for (var i = 0; i < cnt; i = i + 1) {
        d = min(d, shape_dist(shape_at(first + i), p));
    }
    return d;
}

// a lower bound of the distance to the selected shapes (a sphere around them)
fn sel_lb(p0: vec3<f32>) -> f32 {
    return length(warp(p0) - G.selsph.xyz) - G.selsph.w;
}

fn soft_shadow(ro: vec3<f32>, rd: vec3<f32>, maxt: f32, nsteps: i32) -> f32 {
    var res = 1.0;
    var t = 0.6;
    var tend = maxt;
    // a ray that passes far from the scene throws no shadow: only march inside the bounding sphere
    if (G.bound.w <= 0.0) { return 1.0; }
    let oc = ro - G.bound.xyz;
    let big = G.bound.w + 0.1 * (length(oc) + G.bound.w) + 1.0;
    let b = dot(oc, rd);
    let disc = b * b - (dot(oc, oc) - big * big);
    if (disc <= 0.0) { return 1.0; }
    let sq = sqrt(disc);
    if (-b + sq < t) { return 1.0; }
    t = max(t, -b - sq);
    tend = min(maxt, -b + sq);
    let max_step = select(6.0, 14.0, nsteps < 20);
    for (var i = 0; i < nsteps; i = i + 1) {
        let h = map_d(ro + rd * t);
        res = min(res, 10.0 * h / t);
        t = t + clamp(h, 0.4, max_step);
        if (res < 0.001 || t > tend) { break; }
    }
    return clamp(res, 0.0, 1.0);
}

fn ambient_occ(p: vec3<f32>, n: vec3<f32>) -> f32 {
    // far from the scene nothing is close enough to darken the point
    if (G.bound.w <= 0.0 || length(p - G.bound.xyz) > G.bound.w + 28.0) { return 1.0; }
    var occ = 0.0;
    var sca = 1.0;
    for (var i = 0; i < 5; i = i + 1) {
        let h = 0.8 + 3.2 * f32(i);
        let d = dist_only(p + n * h);
        occ = occ + (h - d) * sca;
        sca = sca * 0.8;
    }
    return clamp(1.0 - 0.06 * occ, 0.0, 1.0);
}

fn lin(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn n_lights() -> i32 { return i32(G.env[1].w + 0.5); }
fn light_dir(i: i32) -> vec3<f32> { return G.env[5 + 2 * i].xyz; }
fn light_col(i: i32) -> vec3<f32> { return G.env[6 + 2 * i].xyz * G.env[5 + 2 * i].w; }

fn key_dir() -> vec3<f32> {
    if (n_lights() > 0) { return light_dir(0); }
    return vec3<f32>(0.0, 0.0, 1.0);
}

// what a shiny surface sees: the sky colours, plus a soft window where each light is
fn sky(rd: vec3<f32>, rough: f32) -> vec3<f32> {
    let t = clamp(rd.z * 0.5 + 0.5, 0.0, 1.0);
    var c = mix(G.env[4].xyz, G.env[3].xyz, smoothstep(0.0, 0.5, t));
    c = mix(c, G.env[2].xyz, smoothstep(0.5, 1.0, t));
    let nl = n_lights();
    for (var i = 0; i < nl; i = i + 1) {
        c = c + light_col(i) * pow(max(dot(rd, light_dir(i)), 0.0), mix(60.0, 6.0, rough)) * 0.8;
    }
    return c;
}

// What shiny things reflect: the sky, plus the bright strips and horizon of a photo studio, and
// the floor below. Rougher surfaces see a blurrier, flatter version.
fn env_reflect(rd: vec3<f32>, rough: f32) -> vec3<f32> {
    var c = sky(rd, rough);
    let up = rd.z;
    let lum = dot(G.env[2].xyz + G.env[3].xyz, vec3<f32>(0.33)) * 0.5;
    let strip = smoothstep(0.30, 0.50, up) * (1.0 - smoothstep(0.60, 0.80, up));
    c = c + vec3<f32>(1.0) * strip * lum * 2.0 * (1.0 - rough);
    c = c + G.env[3].xyz * exp(-abs(up) * 12.0) * 0.7 * (1.0 - rough);
    let floor_c = lin(G.env[1].xyz) * 0.9;
    c = mix(c, floor_c, smoothstep(0.02, -0.12, up) * 0.85);
    let avg = (G.env[2].xyz + G.env[3].xyz + G.env[4].xyz) * 0.33;
    return mix(c, avg, rough * 0.75);
}

fn background(uv: vec2<f32>) -> vec3<f32> {
    // the backdrop colour, a little lighter in the middle
    let c = vec2<f32>(uv.x - 0.5, uv.y - 0.45);
    let v = clamp(1.0 - dot(c, c) * 1.1, 0.0, 1.0);
    return lin(clamp(G.env[1].xyz * mix(0.87, 1.22, v), vec3<f32>(0.0), vec3<f32>(1.0)));
}

// what is seen far behind glass, in direction rd
fn far_view(rd: vec3<f32>) -> vec3<f32> {
    let bd = lin(G.env[1].xyz);
    return mix(bd * 1.05, sky(rd, 0.7) * 0.8 + bd * 0.3, clamp(0.1 + 0.55 * smoothstep(-0.1, 0.7, rd.z), 0.0, 1.0));
}

fn encode(c0: vec3<f32>) -> vec3<f32> {
    // a gentle filmic curve keeps colours rich instead of washing them out
    let c = c0 * 0.78 * G.env[0].x;
    let m = (c * (2.51 * c + vec3<f32>(0.03))) / (c * (2.43 * c + vec3<f32>(0.59)) + vec3<f32>(0.14));
    let x = clamp(m, vec3<f32>(0.0), vec3<f32>(1.0));
    if (flag(1)) { return x; }
    return pow(x, vec3<f32>(1.0 / 2.2));
}

// a quick lit look at whatever is seen through glass (no shadows, no glass)
fn shade_simple(p: vec3<f32>, rd: vec3<f32>, h: Hit) -> vec3<f32> {
    let n = normal_at(p, 0.04);
    let alb = lin(h.col);
    let nl = n_lights();
    var d = sky(n, 1.0) * 0.5 * G.env[0].y;
    for (var i = 0; i < nl; i = i + 1) {
        d = d + light_col(i) * max(dot(n, light_dir(i)), 0.0);
    }
    var c = alb * (1.0 - h.metal) * d + env_reflect(reflect(rd, n), h.rough) * mix(0.04, 0.9, h.metal) * alb;
    c = c + alb * h.glow * 1.4;
    return c;
}

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    var o: VOut;
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let aspect = G.view.x / max(G.view.y, 1.0);
    let ndc = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    var ro: vec3<f32>;
    var rd: vec3<f32>;
    let tanh = G.cam_fwd.w;
    if (tanh > 0.0) {
        ro = G.cam_pos.xyz;
        rd = normalize(G.cam_fwd.xyz + G.cam_right.xyz * ndc.x * aspect * tanh + G.cam_up.xyz * ndc.y * tanh);
    } else {
        rd = G.cam_fwd.xyz;
        ro = G.cam_pos.xyz + G.cam_right.xyz * ndc.x * aspect * G.view.z + G.cam_up.xyz * ndc.y * G.view.z;
    }
    var col = background(in.uv);
    let px = 2.0 / max(G.view.y, 1.0) * select(tanh, G.view.z, tanh <= 0.0);
    let flags = i32(G.env[0].w + 0.5);
    let grid_on = (flags & 1) != 0;
    let floor_on = (flags & 2) != 0;
    let shadow_k = G.env[0].z;
    let key = key_dir();

    // the ground grid (z = 0)
    var t_ground = 1e9;
    if (abs(rd.z) > 1e-4) {
        let tg = -ro.z / rd.z;
        if (tg > 0.0) { t_ground = tg; }
    }

    var t = 0.0;
    var hit = false;
    var h: Hit;
    let has_sel = G.counts.w > 0.5;
    let sel_carving = has_sel && D(shape_rec(i32(G.counts.z + 0.5)) + 7).x < 0.5;
    var sel_abs = 1e9;
    var sel_hit = false;
    let foot = px;
    var halo = vec3<f32>(0.0);
    let fast = is_fast();
    let glow_scene = flag(4) && !fast;
    // rays that miss the scene's bounding sphere skip the marching
    var run = false;
    var t_max = 2000.0;
    let oc = ro - G.bound.xyz;
    let bq = dot(oc, rd);
    let disc = bq * bq - (dot(oc, oc) - G.bound.w * G.bound.w);
    if (G.bound.w > 0.0 && disc > 0.0) {
        let sq = sqrt(disc);
        if (-bq + sq > 0.0) {
            run = true;
            t = max(-bq - sq, 0.0);
            t_max = -bq + sq;
        }
    }
    // a little per-pixel jitter turns banding in the lamp glow into fine noise
    t = t + fract(sin(dot(in.pos.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453) * 1.5;
    // Over-relaxed sphere tracing: steps a little longer than the distance, taken back when two
    // steps would have jumped over a surface. Lamps and a selected carver need small steps.
    var relax = select(1.1, 1.5, fast);
    if (glow_scene || (has_sel && sel_carving)) { relax = 0.9; }
    var max_it = select(128, 48, fast);
    if (glow_scene) { max_it = 200; }
    var prev_r = 0.0;
    var stp = 0.0;
    for (var i = 0; i < max_it; i = i + 1) {
        if (!run) { break; }
        let p = ro + rd * t;
        var d = 0.0;
        if (glow_scene) {
            h = map(p);
            d = h.d;
            if (h.glow > 0.0) {
                // a soft glow in the air around lamps
                halo = halo + lin(h.col) * h.glow * exp(-max(h.d, 0.0) * 0.15) * max(h.d, 0.05) * 0.9 * 0.009;
            }
        } else {
            d = map_d(p);
        }
        let pix = select(t * foot, foot, tanh <= 0.0);
        var step = d * relax;
        if (glow_scene && h.glow > 0.0) {
            // small steps near lamps keep their glow smooth (no rings)
            step = min(step, max(3.0, d * 0.25));
        }
        if (has_sel) {
            // the outline only needs the selected shapes where the ray passes close to them
            let lbs = sel_lb(p);
            if (lbs < 2.5 * max(pix, 0.02) || (sel_carving && lbs < step / 0.8)) {
                let sd = sel_dist(p);
                sel_abs = min(sel_abs, abs(sd) / max(pix, 0.02));
                if (sd < 0.02) { sel_hit = true; }
                if (sel_carving) { step = min(step, max(abs(sd) * 0.8, 0.15)); }
            }
        }
        if (relax > 1.0 && (d < 0.0 || stp > prev_r + d)) {
            // the last long step may have jumped over a surface: go back and walk carefully
            t = t - stp + prev_r;
            stp = 0.0;
            prev_r = 0.0;
            relax = 0.9;
            continue;
        }
        // hit when closer than a fraction of a pixel (never finer than the old fixed limit)
        if (d < max(0.004 * (1.0 + t * 0.002), pix * select(0.2, 0.5, fast))) { hit = true; break; }
        stp = step;
        prev_r = d;
        t = t + step;
        if (t > t_max) { break; }
    }
    if (hit && !glow_scene) { h = map(ro + rd * t); }

    // ground
    let above = ro.z >= 0.0;
    if (above && t_ground < 1e8 && (!hit || t_ground < t) && (grid_on || floor_on)) {
        let gp = ro + rd * t_ground;
        let fade = clamp(1.0 - length(gp.xy) / 600.0, 0.0, 1.0);
        if (grid_on) {
            let gs = 10.0;
            let a = abs(fract(gp.xy / gs - 0.5) - 0.5) / max(fwidth(gp.xy / gs), vec2<f32>(1e-4));
            let line = 1.0 - min(min(a.x, a.y), 1.0);
            let major = abs(fract(gp.xy / (gs * 10.0) - 0.5) - 0.5) / max(fwidth(gp.xy / (gs * 10.0)), vec2<f32>(1e-4));
            let lm = 1.0 - min(min(major.x, major.y), 1.0);
            let axisx = 1.0 - min(abs(gp.y) / max(fwidth(gp.y), 1e-4), 1.0);
            let axisy = 1.0 - min(abs(gp.x) / max(fwidth(gp.x), 1e-4), 1.0);
            col = mix(col, col + vec3<f32>(0.03), line * 0.5 * fade);
            col = mix(col, col + vec3<f32>(0.05), lm * 0.6 * fade);
            col = mix(col, lin(vec3<f32>(0.85, 0.35, 0.35)), axisx * 0.55 * fade);
            col = mix(col, lin(vec3<f32>(0.42, 0.78, 0.42)), axisy * 0.55 * fade);
        }
        if (floor_on) {
            // contact shadow of the blob on the ground
            if (fast) {
                // while moving: one tap for a soft dark patch under the blob
                let ao = clamp(map_d(gp + vec3<f32>(0.0, 0.0, 6.0)) / 6.0, 0.0, 1.0);
                col = col * mix(1.0, mix(0.6, 1.0, ao), shadow_k);
            } else {
                let sh = soft_shadow(gp + vec3<f32>(0.0, 0.0, 0.05), key, 300.0, 32);
                col = col * mix(1.0, mix(0.55, 1.0, sh), shadow_k);
                let ao = ambient_occ(gp, vec3<f32>(0.0, 0.0, 1.0));
                col = col * mix(1.0, mix(0.6, 1.0, ao), shadow_k);
            }
        }
    }

    if (hit && (t_ground > t || !(grid_on || floor_on))) {
        let p = ro + rd * t;
        var n = vec3<f32>(0.0, 0.0, 1.0);
        if (fast) {
            // three taps are enough while moving
            let e = 0.03 + t * 0.0015;
            n = normalize(vec3<f32>(map_d(p + vec3<f32>(e, 0.0, 0.0)) - h.d, map_d(p + vec3<f32>(0.0, e, 0.0)) - h.d, map_d(p + vec3<f32>(0.0, 0.0, e)) - h.d) + vec3<f32>(0.0, 0.0, 1e-7));
        } else {
            n = normal_at(p, 0.02 + t * 0.0008);
        }
        let v = -rd;
        let rough = clamp(h.rough, 0.04, 1.0);
        let metal = h.metal;
        let alb = lin(h.col);
        let ndv = max(dot(n, v), 0.0);
        let fres = pow(1.0 - ndv, 5.0);
        var ao = 1.0;
        if (!fast) { ao = ambient_occ(p, n); }
        let f0 = mix(vec3<f32>(0.04), alb, metal);
        var F = f0 + (vec3<f32>(1.0) - f0) * fres;
        // the rainbow sheen of soap and beetle shells
        let film = 0.5 + 0.5 * cos(6.2832 * (vec3<f32>(0.0, 0.33, 0.67) + ndv * 1.35 + p.z * 0.006 + p.x * 0.004));
        let irid = h.irid;
        var diff_col = alb * (1.0 - metal);
        diff_col = mix(diff_col, mix(diff_col, vec3<f32>(0.85), 0.35) * film * 1.35, irid * 0.7);
        let refl_tint = mix(vec3<f32>(1.0), film * 1.8, irid);
        let nl = n_lights();
        let spec_pow = mix(300.0, 8.0, rough);
        var thin = 0.0;
        if (h.trans > 0.01) {
            // thin parts glow more than thick ones
            thin = 0.6;
            if (!fast) { thin = 1.0 - clamp(-dist_only(p - n * 5.0) / 5.0, 0.0, 1.0) * 0.8; }
        }
        var lit = vec3<f32>(0.0);
        var spec = vec3<f32>(0.0);
        for (var i = 0; i < nl; i = i + 1) {
            let L = light_dir(i);
            let lc = light_col(i);
            var sh = 1.0;
            if (i == 0) {
                sh = mix(1.0, soft_shadow(p + n * 0.1, L, 300.0, select(32, 10, fast)), shadow_k);
                sh = mix(sh, 1.0, h.glass * 0.6);
            }
            let ndl = max(dot(n, L), 0.0);
            // translucent things wrap the light around their edges
            let wrap = h.trans * 0.55;
            let ndl_w = clamp((dot(n, L) + wrap) / (1.0 + wrap), 0.0, 1.0);
            let hv = normalize(L + v);
            let nh = max(dot(n, hv), 0.0);
            let lobe = pow(nh, spec_pow) * (spec_pow + 8.0) / 60.0;
            var coat_lobe = 0.0;
            if (h.coat > 0.001) { coat_lobe = pow(nh, 420.0) * (420.0 + 8.0) / 60.0; }
            lit = lit + lc * ndl_w * sh;
            if (h.trans > 0.01) {
                // light shining through thin parts, seen from the other side
                let back = pow(max(dot(rd, L), 0.0), 2.5);
                lit = lit + lc * alb * back * thin * h.trans * 1.1;
            }
            spec = spec + lc * ndl * sh * (F * lobe * 0.9 * refl_tint + vec3<f32>(0.04 + 0.96 * fres) * coat_lobe * h.coat * 0.9);
        }
        let amb = sky(n, 1.0) * 0.55 * ao * G.env[0].y;
        var c = diff_col * (amb + lit);
        c = c + spec;
        let rr = reflect(rd, n);
        let refl = env_reflect(rr, rough) * mix(0.5, 1.0, ao);
        c = c + refl * F * refl_tint * mix(0.15, 1.0, metal) * (1.0 - rough * 0.65);
        // clear coat: a second, glossy reflection on top
        if (h.coat > 0.001) {
            c = c + env_reflect(rr, 0.05) * (0.04 + 0.96 * fres) * h.coat * 0.55 * mix(0.5, 1.0, ao);
        }
        // a thin clean rim
        c = c + vec3<f32>(0.05) * fres * ao;

        // glass: bend the view ray through the body, and see what is behind it
        if (h.glass > 0.01 && fast) {
            // while moving: no bending, just the tinted view past it and the reflection
            let Fg = 0.04 + 0.96 * fres;
            let tint = pow(max(alb, vec3<f32>(0.001)), vec3<f32>(0.45));
            let gcol = far_view(rd) * tint * (1.0 - Fg) + env_reflect(rr, rough) * Fg * refl_tint;
            c = mix(c, gcol, h.glass);
        } else if (h.glass > 0.01) {
            let eta = 1.0 / 1.45;
            var rdi = refract(rd, n, eta);
            if (dot(rdi, rdi) < 1e-6) { rdi = reflect(rd, n); }
            var q = p - n * 0.08;
            var tin = 0.0;
            for (var k = 0; k < 40; k = k + 1) {
                let di = dist_only(q);
                if (di > -0.01) { break; }
                let st = max(-di * 0.9, 0.05);
                q = q + rdi * st;
                tin = tin + st;
            }
            let ne = normal_at(q, 0.04);
            var rdo = refract(rdi, -ne, 1.45);
            if (dot(rdo, rdo) < 1e-6) { rdo = reflect(rdi, -ne); }
            var seen = far_view(rdo);
            var q2 = q + rdo * 0.2;
            var t2 = 0.0;
            var floor_t = 1e9;
            if (rdo.z < -1e-3 && q.z > 0.0) { floor_t = -q.z / rdo.z; }
            for (var k = 0; k < 48; k = k + 1) {
                let hp = q2 + rdo * t2;
                let h2 = map(hp);
                if (t2 > floor_t) {
                    seen = lin(G.env[1].xyz) * 1.08;
                    break;
                }
                if (h2.d < 0.02) {
                    seen = shade_simple(hp, rdo, h2);
                    break;
                }
                t2 = t2 + max(h2.d * 0.9, 0.03);
                if (t2 > 300.0) { break; }
            }
            let tint = pow(max(alb, vec3<f32>(0.001)), vec3<f32>(0.3 + tin * 0.035));
            // frosted glass scatters what is behind it
            let frost = smoothstep(0.12, 0.7, rough);
            seen = mix(seen, mix(far_view(vec3<f32>(0.0, 0.0, 0.3)), seen, 0.35), frost);
            let Fg = 0.04 + 0.96 * fres;
            var gcol = seen * tint * (1.0 - Fg) + env_reflect(rr, rough) * Fg * refl_tint;
            for (var i = 0; i < nl; i = i + 1) {
                let L = light_dir(i);
                let nh = max(dot(n, normalize(L + v)), 0.0);
                let lobe = pow(nh, spec_pow * 2.0) * (spec_pow * 2.0 + 8.0) / 60.0;
                gcol = gcol + light_col(i) * lobe * 0.9 * max(dot(n, L), 0.0) * (0.04 + 0.96 * fres);
            }
            gcol = gcol + irid * film * 0.25 * (0.3 + fres);
            c = mix(c, gcol, h.glass);
        }

        // lamps
        c = c + alb * h.glow * 1.4;
        // the shape under the mouse lights up a little
        if (G.hover.y > 0.5 && abs(h.id - G.hover.x) < 0.5) {
            c = c * 1.1 + vec3<f32>(0.018, 0.016, 0.04) + vec3<f32>(0.04, 0.035, 0.09) * fres;
        }
        col = c;
    }

    // seen from below the ground is see-through: only its lines are drawn
    if (!above && t_ground < 1e8 && (!hit || t_ground < t) && grid_on) {
        let gp = ro + rd * t_ground;
        let fade = clamp(1.0 - length(gp.xy) / 600.0, 0.0, 1.0) * 0.6;
        let gs = 10.0;
        let a = abs(fract(gp.xy / gs - 0.5) - 0.5) / max(fwidth(gp.xy / gs), vec2<f32>(1e-4));
        let line = 1.0 - min(min(a.x, a.y), 1.0);
        let axisx = 1.0 - min(abs(gp.y) / max(fwidth(gp.y), 1e-4), 1.0);
        let axisy = 1.0 - min(abs(gp.x) / max(fwidth(gp.x), 1e-4), 1.0);
        col = mix(col, col + vec3<f32>(0.04), line * 0.5 * fade);
        col = mix(col, lin(vec3<f32>(0.85, 0.35, 0.35)), axisx * 0.55 * fade);
        col = mix(col, lin(vec3<f32>(0.42, 0.78, 0.42)), axisy * 0.55 * fade);
    }

    col = col + halo;

    // selection outline (and a faint tint inside a shape that carves)
    if (has_sel) {
        let carving = sel_carving;
        let ring = clamp(1.0 - sel_abs / 2.0, 0.0, 1.0) * 0.9;
        var a = 0.0;
        if (carving) {
            a = ring + select(0.0, 0.16, sel_hit);
        } else if (!sel_hit) {
            a = ring;
        }
        col = mix(col, vec3<f32>(0.32, 0.24, 0.95), clamp(a, 0.0, 1.0));
    }

    return vec4<f32>(encode(col), 1.0);
}
