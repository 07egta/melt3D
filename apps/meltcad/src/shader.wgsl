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
    extra: vec4<f32>,   // x = shell wall thickness (0 = solid, minus = open top), y z = profile rounding limit, winding, w = 1: chamfer instead of round
    gcol: vec4<f32>,    // gradient: second colour rgb, strength (0 = no gradient)
    gpar: vec4<f32>,    // gradient: direction (0 x, 1 y, 2 z; +3 = along the shape's own box), softness, from, to
    ripple: vec4<f32>,  // mode, amplitude, wavelength, number
    ripple2: vec4<f32>, // angle, edge fade, preserve, detail
    ripple_half: vec4<f32>, // local bounding half extents
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
    offs2: vec4<f32>,     // first vec4 of the outline points, the path nodes, 1 = something keeps only an overlap (nothing may be left out of the march)
    env: array<vec4<f32>, 11>,      // exposure/ambient/shadow/flags, backdrop+lights, 3 sky colours, 3 x (light dir+power, light colour)
};

@group(0) @binding(0) var<uniform> G: Globals;
@group(0) @binding(1) var DT: texture_2d<f32>;

// The scene list: 1024 vec4 per texture row.
fn D(i: i32) -> vec4<f32> { return textureLoad(DT, vec2<i32>(i & 1023, i >> 10), 0); }

// Layout (keep in step with gpu.rs):
//   group  (3 vec4): [first block, block count, op sign, blend] [first instance, instance count, first item (see cand), 0] [bounding sphere]
//   block  (2 vec4): [bounding sphere] [largest blend, 1 = never skip, first shape, shape count]
//   instance (4 vec4): three rows (x y z, offset) taking a world point back into the group's own space, then [world centre of the group's sphere, scale]
//   shape (19 vec4): [bounding sphere = position, radius] then the 18 vec4 of struct Shape
fn group_at(gi: i32) -> i32 { return i32(G.offs.x + 0.5) + gi * 3; }
fn block_at(bi: i32) -> i32 { return i32(G.offs.y + 0.5) + bi * 2; }
fn inst_at(i: i32) -> i32 { return i32(G.offs.z + 0.5) + i * 4; }
fn shape_rec(i: i32) -> i32 { return i32(G.offs.w + 0.5) + i * 19; }
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
    s.ripple = D(b + 15);
    s.ripple2 = D(b + 16);
    s.ripple_half = D(b + 17);
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

fn ripple_adjust(base: f32, q: vec3<f32>, s: Shape) -> f32 {
    let rr = s.ripple;
    let r2 = s.ripple2;
    if (rr.x < 0.5 || rr.y <= 0.0) { return base; }
    let amplitude = rr.y * s.pos.w;
    if (abs(base) > amplitude * 2.0) {
        if (base > 0.0 && r2.z < 0.5) { return base - amplitude; }
        return base;
    }
    let angle = r2.x * 0.017453292519943295;
    let sn = sin(angle);
    let cs = cos(angle);
    let u = q.x * cs + q.y * sn;
    let v = q.y * cs - q.x * sn;
    let radius = length(q.xy);
    let k = 6.283185307179586 / rr.z;
    let linear = rr.x > 1.5 && rr.x < 2.5;
    let random = rr.x > 2.5;
    let support = rr.w * rr.z * select(1.0, 0.5, linear);
    let distance = select(radius, abs(u), linear);
    let fade_width = max(support * r2.y, 0.001);
    let t = clamp((distance - (support - fade_width)) / fade_width, 0.0, 1.0);
    var envelope = 1.0 - t * t * (3.0 - 2.0 * t);
    let h = s.ripple_half.xyz;
    if (r2.z > 0.5) {
        let edge = max(abs(q.x) / max(h.x, 0.01), abs(q.y) / max(h.y, 0.01));
        let f = clamp((edge - 0.68) / 0.32, 0.0, 1.0);
        envelope *= 1.0 - f * f * (3.0 - 2.0 * f);
    }
    let core = select(radius, u, linear);
    var wave = sin(select(core * k, (0.84 * u + 0.34 * v) * k, random));
    var weight = 1.0;
    if (r2.w >= 2.0) {
        let a = select(core * k * 2.0 + 0.3, (-0.31 * u + 1.23 * v) * k + 0.8, random);
        wave += 0.15 * sin(a);
        weight += 0.15;
    }
    if (r2.w >= 3.0) {
        let a = select(core * k * 3.0 + 1.1, (1.37 * u - 0.71 * v) * k + 1.1, random);
        wave += 0.07 * sin(a);
        weight += 0.07;
    }
    if (r2.w >= 4.0) {
        let a = select(core * k * 4.0 + 2.0, (-1.73 * u - 0.48 * v) * k + 2.0, random);
        wave += 0.035 * sin(a);
        weight += 0.035;
    }
    wave /= weight;
    let delta = rr.y * envelope * select(-wave, (1.0 - wave) * 0.5, r2.z > 0.5);
    let edge_slope = select(0.0, 6.0 / max(min(h.x, h.y), 0.01), r2.z > 0.5);
    let safety = 1.0 + rr.y * (3.0 * k + 2.0 / fade_width + edge_slope);
    return (base + delta * s.pos.w) / safety;
}

// The factor `ripple_adjust` divides by where it bends the surface (Ripple::safety).
fn ripple_safety(s: Shape) -> f32 {
    let rr = s.ripple;
    let r2 = s.ripple2;
    let k = 6.283185307179586 / rr.z;
    let linear = rr.x > 1.5 && rr.x < 2.5;
    let support = rr.w * rr.z * select(1.0, 0.5, linear);
    let fade_width = max(support * r2.y, 0.001);
    let h = s.ripple_half.xyz;
    let edge_slope = select(0.0, 6.0 / max(min(h.x, h.y), 0.01), r2.z > 0.5);
    return 1.0 + rr.y * (3.0 * k + 2.0 / fade_width + edge_slope);
}

fn shape_dist(s: Shape, p: vec3<f32>) -> f32 {
    let d = p - s.pos.xyz;
    let q = vec3<f32>(dot(s.r0.xyz, d), dot(s.r1.xyz, d), dot(s.r2.xyz, d)) / s.scale.xyz;
    let kind = i32(s.scale.w + 0.5);
    let sz = s.size.xyz;
    let rd = s.size.w;
    let chamfer = s.extra.w > 0.5 && rd > 0.0;
    shell_k = 1.0;
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
        // Same signed inflation used by the CPU SDF and mesh export for Extrude.
        r -= clamp(s.params.x, -40.0, 40.0);
    }
    let fc = i32(s.r0.w + 0.5);
    if (fc > 0) { r = face_melt(s, q, r, fc); }
    let scaled = r * s.pos.w;
    var ripple_shorten = 1.0;
    if (s.ripple.x >= 0.5 && s.ripple.y > 0.0 && abs(scaled) <= s.ripple.y * s.pos.w * 2.0) {
        ripple_shorten = 1.0 / ripple_safety(s);
    }
    r = ripple_adjust(scaled, q, s);
    // Hollow walls of thickness `shell`, in the shape's own mm, through the same
    // distance-shortening the shape maths uses. (A curve hollows itself in curve_dist.)
    // A minus shell is the same wall with a round hole in the top (local +Z): `top_hole` in sdf.rs.
    if (s.extra.x != 0.0 && kind != 18) {
        let sh = abs(s.extra.x) * s.pos.w * shell_shortening(s, q, fc) * ripple_shorten;
        r = abs(r + sh * 0.5) - sh * 0.5;
        if (s.extra.x < 0.0) {
            let hole = max(length(q.xy) - min(s.ripple_half.x, s.ripple_half.y) * 0.35, -q.z) * s.pos.w;
            r = max(r, -hole);
        }
    }
    return r;
}

// A melt on one face (face_melt_distance in sdf.rs): swells or sinks that face, nothing else.
fn face_melt(s: Shape, q: vec3<f32>, d: f32, fc: i32) -> f32 {
    let ax = (fc - 1) / 2;
    var sg = 1.0;
    if (((fc - 1) % 2) == 1) { sg = -1.0; }
    let amount = s.r1.w;
    let reach = max(s.r2.w, 1.0);
    let t = sg * q[ax] - s.mat2.w;
    let k = 1.0 / reach;
    var w = clamp(1.0 - abs(t) * k, 0.0, 1.0);
    if (w <= 0.0) { return d; }
    w = w * w * (3.0 - 2.0 * w);
    let u = q[(ax + 1) % 3];
    let v = q[(ax + 2) % 3];
    let f = 6.2832 / (reach * 1.6);
    let wave = 0.5 + 0.5 * sin(u * f + sin(v * f * 1.3)) * cos(v * f * 0.8 + cos(u * f * 0.6));
    let slope = abs(amount) * (1.6 * k + 1.32 * f);
    return (d - amount * w * wave) / sqrt(1.0 + slope * slope);
}

// The factor `face_melt` divides by inside the face's reach (1 outside it).
fn face_melt_shortening(s: Shape, q: vec3<f32>, fc: i32) -> f32 {
    let ax = (fc - 1) / 2;
    var sg = 1.0;
    if (((fc - 1) % 2) == 1) { sg = -1.0; }
    let amount = s.r1.w;
    let reach = max(s.r2.w, 1.0);
    let t = sg * q[ax] - s.mat2.w;
    let k = 1.0 / reach;
    if (clamp(1.0 - abs(t) * k, 0.0, 1.0) <= 0.0) { return 1.0; }
    let f = 6.2832 / (reach * 1.6);
    let slope = abs(amount) * (1.6 * k + 1.32 * f);
    return 1.0 / sqrt(1.0 + slope * slope);
}

// How much shorter than a true distance the shape maths makes this point's distance
// (`Prepared::shell_shortening`): sweeps, lofts and wave-warped revolves scale it down so that
// marching cannot overshoot (they leave their factor in `shell_k`), and so do face melts; a shell
// on that field must use the same scale.
fn shell_shortening(s: Shape, q: vec3<f32>, fc: i32) -> f32 {
    var shorten = shell_k;
    if (fc > 0) { shorten = shorten * face_melt_shortening(s, q, fc); }
    return shorten;
}

// The surface colour of a shape at p: its colour, or the gradient (Material::gradient_weight).
fn mix_colour(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> {
    if (t <= 0.0) { return a; }
    if (t >= 1.0) { return b; }
    let al = pow(max(a, vec3<f32>(0.0)), vec3<f32>(2.2));
    let bl = pow(max(b, vec3<f32>(0.0)), vec3<f32>(2.2));
    return pow(max(mix(al, bl, t), vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}

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
    return mix_colour(s.color.rgb, s.gcol.rgb, k * s.gcol.w);
}

fn mix_hit(a: Hit, b: Hit, t: f32) -> Hit {
    // b at weight 0, a at weight 1
    var o: Hit;
    o.d = 0.0;
    o.col = mix_colour(b.col, a.col, t);
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

// Per-ray candidates, found once per pixel before the primary march (see `fs`).
//  * `cand`: one bit for every item (a group, or one instance of a pattern group) whose bounding
//    sphere comes near the ray.
//  * `cs`: the shapes of those items whose own sphere comes near the ray, in the order the march
//    melts them (group, instance, shape): the primary march then reads only these (`map_d_cand`).
//    More than MAXC of them (`ovf`): the march falls back to the groups' bits above.
// What is left out is far from the whole ray, so it can neither be hit nor bend the surface where
// the ray meets it (the blend leaves a shape alone beyond its reach, and a ray that never comes
// that close cannot be stopped by it either). Every other query (shadows, occlusion, glass) is
// about points off the ray and uses the full scan; the normal at the hit uses the bits.
const MAXC: i32 = 24;
var<private> cand: array<u32, 8>;
var<private> cs: array<u32, 24>;
var<private> ncs: i32;
var<private> ovf: bool;
var<private> use_cand: bool;

fn is_cand(i: i32) -> bool {
    if (!use_cand || i >= 256) { return true; }
    return ((cand[i >> 5] >> u32(i & 31)) & 1u) != 0u;
}

// How near the segment o + d * t, t in [t0, t1] comes to the sphere c, r (negative: it goes through), in the units of `d`.
fn seg_sphere(o: vec3<f32>, d: vec3<f32>, dd: f32, t0: f32, t1: f32, s: vec4<f32>) -> f32 {
    let tp = clamp(dot(s.xyz - o, d) / dd, t0, t1);
    return length(s.xyz - (o + d * tp)) - s.w;
}

// A curve (kind 18) is a thin tube inside a big sphere: is the ray (o + d * t, t in [t0, t1], in the
// space of the group, `scl` = its scale) farther than `margin` from every one of the spheres around
// its groups of segments (shader_curve.wgsl)?
fn curve_far(rec: i32, o: vec3<f32>, d: vec3<f32>, t0: f32, t1: f32, scl: f32, margin: f32) -> bool {
    let ext = D(rec + 12);
    let st = i32(ext.x + 0.5);
    let n = min(i32(ext.y + 0.5), 96);
    if (n < 2 || st < 2) { return false; }
    let sc = D(rec + 5);
    let pos = D(rec + 4);
    let r0 = D(rec + 1);
    let r1 = D(rec + 2);
    let r2 = D(rec + 3);
    // the ray in the curve's own space, where shape_dist measures (and then scales the distance by pos.w)
    let oo = o - pos.xyz;
    let q0 = vec3<f32>(dot(r0.xyz, oo), dot(r1.xyz, oo), dot(r2.xyz, oo)) / sc.xyz;
    let qd = vec3<f32>(dot(r0.xyz, d), dot(r1.xyz, d), dot(r2.xyz, d)) / sc.xyz;
    let dd = max(dot(qd, qd), 1e-12);
    let ng = (n - 1 + CURVE_GROUP - 1) / CURVE_GROUP;
    for (var g = 0; g < ng; g = g + 1) {
        let sp = path_pt(st + n + g);
        if (seg_sphere(q0, qd, dd, t0, t1, sp) * pos.w * scl < margin) { return false; }
    }
    return true;
}

// The items and shapes near the ray `ro + rd * t` for t in [t0, t1]; `slack` is added to every reach
// (the drip moves the point the shapes are measured at, and the selection outline looks a few pixels out).
fn find_candidates(ro: vec3<f32>, rd: vec3<f32>, t0: f32, t1: f32, slack: f32) {
    for (var w = 0; w < 8; w = w + 1) { cand[w] = 0u; }
    ncs = 0;
    ovf = false;
    let amount = G.melt.y;
    let rm = reach_mult();
    let ng = i32(G.counts.x + 0.5);
    for (var gi = 0; gi < ng; gi = gi + 1) {
        let gb = group_at(gi);
        let gr = D(gb);
        if (gr.z > -0.5 && gr.z < 0.5) { continue; }   // a stain group changes no distance
        let gn = D(gb + 1);
        let gs = D(gb + 2);
        let ni = i32(gn.y + 0.5);
        let fi = i32(gn.x + 0.5);
        let ib0 = i32(gn.z + 0.5);
        let grow = rm * gr.w * amount + slack;
        let nonleaf = nh_nonleaf(gn.w);
        let fb = i32(gr.x + 0.5);
        let nb = i32(gr.y + 0.5);
        for (var j = 0; j < max(ni, 1); j = j + 1) {
            var sphere = gs;
            var o = ro;
            var d = rd;
            var scl = 1.0;
            let ib = inst_at(fi + j);
            if (ni > 0) {
                let ic = D(ib + 3);
                scl = ic.w;
                sphere = vec4<f32>(ic.xyz, gs.w * scl);
            }
            let near = seg_sphere(ro, rd, 1.0, t0, t1, sphere);
            // (a group that keeps only the overlap changes the result everywhere: never left out)
            if (near >= grow && gr.z < 1.5 && !nonleaf) { continue; }
            let item = ib0 + j;
            if (item < 256) { cand[item >> 5] = cand[item >> 5] | (1u << u32(item & 31)); }
            // the ray in the group's own space (the instance's rows carry a world point back)
            if (ni > 0) {
                let r0 = D(ib);
                let r1 = D(ib + 1);
                let r2 = D(ib + 2);
                o = vec3<f32>(dot(r0.xyz, ro) + r0.w, dot(r1.xyz, ro) + r1.w, dot(r2.xyz, ro) + r2.w);
                d = vec3<f32>(dot(r0.xyz, rd), dot(r1.xyz, rd), dot(r2.xyz, rd));
            }
            let dd = max(dot(d, d), 1e-12);
            let key = (u32(gi) << 8u) | u32(j & 255);
            // a group with groups nested in it always opens, even when none of its own shapes is near
            if (nonleaf) {
                if (ncs < MAXC) {
                    cs[ncs] = (key << 9u) | 0x80000000u;
                    ncs = ncs + 1;
                } else {
                    ovf = true;
                }
            }
            for (var bi = 0; bi < nb; bi = bi + 1) {
                let bb = block_at(fb + bi);
                let b0 = D(bb);
                let b1 = D(bb + 1);
                if (b1.y < 0.5 && seg_sphere(o, d, dd, t0, t1, b0) * scl >= rm * b1.x * amount + slack) { continue; }
                let first = i32(b1.z + 0.5);
                let cnt = i32(b1.w + 0.5);
                for (var si = 0; si < cnt; si = si + 1) {
                    let rec = shape_rec(first + si);
                    let misc = D(rec + 7);
                    let op = misc.x;
                    if (op > -0.5 && op < 0.5) { continue; }   // a stain changes no distance
                    let reach = rm * misc.y * amount + slack;
                    if (op < 1.5 && seg_sphere(o, d, dd, t0, t1, D(rec)) * scl >= reach) { continue; }
                    if (op < 1.5 && i32(D(rec + 5).w + 0.5) == 18 && curve_far(rec, o, d, t0, t1, scl, reach)) { continue; }
                    if (ncs < MAXC) {
                        cs[ncs] = (key << 9u) | u32(first + si);
                        ncs = ncs + 1;
                    } else {
                        ovf = true;
                    }
                }
            }
        }
    }
}

// Nested groups: a group melts into the group it sits in, and that group (with everything nested in it)
// melts into the scene. The open groups are kept by depth (0..3); a group closes into its parent when the
// next group at its depth or higher comes. The group table carries the depth (and 0.5 if groups are nested
// below it) in its second row's last number.
var<private> nh_open: array<bool, 4>;
var<private> nh_have: array<bool, 4>;
var<private> nh_acc: array<Hit, 4>;
var<private> nh_op: array<f32, 4>;
var<private> nh_k: array<f32, 4>;
var<private> nh_root: Hit;
var<private> nh_rhave: bool;

fn nh_depth(gw: f32) -> i32 { return clamp(i32(gw + 0.01), 0, 3); }
fn nh_nonleaf(gw: f32) -> bool { return fract(gw + 0.01) > 0.25; }

fn nh_reset() {
    for (var l = 0; l < 4; l = l + 1) { nh_open[l] = false; nh_have[l] = false; }
    nh_rhave = false;
    nh_root = empty_hit();
}

fn nh_close(from_l: i32, mode: i32) {
    // (fixed-count loops only: some shader compilers cannot unroll a loop that counts to a variable)
    for (var i = 0; i < 4; i = i + 1) {
        let l = 3 - i;
        if (l >= from_l && nh_open[l]) {
            nh_open[l] = false;
            if (nh_have[l]) {
                var pl = -1;
                for (var j = 0; j < 3; j = j + 1) {
                    let c = l - 1 - j;
                    if (pl < 0 && c >= 0 && nh_open[c]) { pl = c; }
                }
                var pacc = nh_root;
                var phave = nh_rhave;
                if (pl >= 0) { pacc = nh_acc[pl]; phave = nh_have[pl]; }
                var res = nh_acc[l];
                if (phave) { res = combine(pacc, nh_acc[l], nh_op[l], nh_k[l], mode); }
                if (phave || nh_op[l] >= 0.5) {
                    if (pl >= 0) { nh_acc[pl] = res; nh_have[pl] = true; } else { nh_root = res; nh_rhave = true; }
                }
            }
        }
    }
}

// (distance, 1 if there is one): what a group at depth d melts into
fn nh_parent(d: i32) -> vec2<f32> {
    var pl = -1;
    for (var j = 0; j < 3; j = j + 1) {
        let c = d - 1 - j;
        if (pl < 0 && c >= 0 && nh_open[c]) { pl = c; }
    }
    if (pl < 0) { return vec2<f32>(nh_root.d, select(0.0, 1.0, nh_rhave)); }
    return vec2<f32>(nh_acc[pl].d, select(0.0, 1.0, nh_have[pl]));
}

// the same for the distance alone
var<private> nd_open: array<bool, 4>;
var<private> nd_have: array<bool, 4>;
var<private> nd_acc: array<f32, 4>;
var<private> nd_op: array<f32, 4>;
var<private> nd_k: array<f32, 4>;
var<private> nd_root: f32;
var<private> nd_rhave: bool;

fn nd_reset() {
    for (var l = 0; l < 4; l = l + 1) { nd_open[l] = false; nd_have[l] = false; }
    nd_rhave = false;
    nd_root = 1e6;
}

fn nd_fold(acc: f32, gv: f32, op: f32, k: f32, mode: i32) -> f32 {
    // (one blend for all three: add = smin, subtract = -smin(-a, b), keep the overlap = -smin(-a, -b))
    let na = op > 1.5 || op <= 0.0;
    let nb = op > 1.5;
    let r = smin_h(select(acc, -acc, na), select(gv, -gv, nb), k, mode).x;
    return select(r, -r, na);
}

fn nd_close(from_l: i32, mode: i32) {
    // (fixed-count loops only: some shader compilers cannot unroll a loop that counts to a variable)
    for (var i = 0; i < 4; i = i + 1) {
        let l = 3 - i;
        if (l >= from_l && nd_open[l]) {
            nd_open[l] = false;
            if (nd_have[l]) {
                var pl = -1;
                for (var j = 0; j < 3; j = j + 1) {
                    let c = l - 1 - j;
                    if (pl < 0 && c >= 0 && nd_open[c]) { pl = c; }
                }
                var pacc = nd_root;
                var phave = nd_rhave;
                if (pl >= 0) { pacc = nd_acc[pl]; phave = nd_have[pl]; }
                var res = nd_acc[l];
                if (phave) { res = nd_fold(pacc, nd_acc[l], nd_op[l], nd_k[l], mode); }
                if (phave || nd_op[l] >= 0.5) {
                    if (pl >= 0) { nd_acc[pl] = res; nd_have[pl] = true; } else { nd_root = res; nd_rhave = true; }
                }
            }
        }
    }
}

fn nd_parent(d: i32) -> vec2<f32> {
    var pl = -1;
    for (var j = 0; j < 3; j = j + 1) {
        let c = d - 1 - j;
        if (pl < 0 && c >= 0 && nd_open[c]) { pl = c; }
    }
    if (pl < 0) { return vec2<f32>(nd_root, select(0.0, 1.0, nd_rhave)); }
    return vec2<f32>(nd_acc[pl], select(0.0, 1.0, nd_have[pl]));
}

fn map(p0: vec3<f32>) -> Hit {
    let p = warp(p0);
    let mode = i32(G.melt.x + 0.5);
    let amount = G.melt.y;
    let rm = reach_mult();
    nh_reset();
    let ng = i32(G.counts.x + 0.5);
    for (var gi = 0; gi < ng; gi = gi + 1) {
        let gb = group_at(gi);
        let gr = D(gb);
        let gn = D(gb + 1);
        let gs = D(gb + 2);
        let ni = i32(gn.y + 0.5);
        let fi = i32(gn.x + 0.5);
        let gd = nh_depth(gn.w);
        let nonleaf = nh_nonleaf(gn.w);
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
            nh_close(gd, mode);
            let par = nh_parent(gd);
            if (!nonleaf && par.y > 0.5 && group_far(gr, sphere, p, par.x, amount, rm, mode)) { continue; }
            let q = to_group(ni, ib, p);
            var gx = group_hit(gr, q, amount, rm, mode);
            nh_open[gd] = true;
            nh_op[gd] = gr.z;
            nh_k[gd] = gr.w * amount;
            nh_have[gd] = gx.ok;
            if (gx.ok) {
                gx.h.d = gx.h.d * sc;
                nh_acc[gd] = gx.h;
            }
        }
    }
    nh_close(0, mode);
    var acc = nh_root;
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
    nd_reset();
    let ng = i32(G.counts.x + 0.5);
    for (var gi = 0; gi < ng; gi = gi + 1) {
        let gb = group_at(gi);
        let gr = D(gb);
        if (gr.z > -0.5 && gr.z < 0.5) { continue; }   // a stain group changes no distance
        let gn = D(gb + 1);
        let gs = D(gb + 2);
        let ni = i32(gn.y + 0.5);
        let fi = i32(gn.x + 0.5);
        let ib0 = i32(gn.z + 0.5);
        let gd = nh_depth(gn.w);
        let nonleaf = nh_nonleaf(gn.w);
        for (var j = 0; j < max(ni, 1); j = j + 1) {
            if (!nonleaf && !is_cand(ib0 + j)) { continue; }
            var sc = 1.0;
            var sphere = gs;
            let ib = inst_at(fi + j);
            if (ni > 0) {
                let ic = D(ib + 3);
                sc = ic.w;
                sphere = vec4<f32>(ic.xyz, gs.w * sc);
            }
            nd_close(gd, mode);
            let par = nd_parent(gd);
            if (!nonleaf && par.y > 0.5 && group_far(gr, sphere, p, par.x, amount, rm, mode)) { continue; }
            let q = to_group(ni, ib, p);
            let gv = group_dist(gr, q, amount, rm, mode);
            nd_open[gd] = true;
            nd_op[gd] = gr.z;
            nd_k[gd] = gr.w * amount;
            nd_have[gd] = gv.y > 0.5;
            nd_acc[gd] = gv.x * sc;
        }
    }
    nd_close(0, mode);
    var acc = nd_root;
    if (G.melt.z > 0.0) {
        let slope = G.melt.z * 1.32 * 0.12 * G.melt.w;
        acc = acc / sqrt(1.0 + slope * slope);
    }
    return acc;
}

// map_d for the primary march: only the shapes of `cs` (see find_candidates), in their order, with
// the same rules as map_d / group_dist.
fn map_d_cand(p0: vec3<f32>) -> f32 {
    let p = warp(p0);
    let mode = i32(G.melt.x + 0.5);
    let amount = G.melt.y;
    let rm = reach_mult();
    nd_reset();
    var key = -1;
    var skip = false;
    var q = p;
    var sc = 1.0;
    var gop = 0.0;
    var gk = 0.0;
    var gdep = 0;
    var g = 0.0;
    var gh = false;
    for (var e = 0; e <= ncs; e = e + 1) {
        var ent = 0xffffffffu;
        if (e < ncs) { ent = cs[e]; }
        let nkey = i32((ent & 0x7fffffffu) >> 9u);
        if (nkey != key) {
            // the group item before is finished: it opens (its own shapes are its first part; groups nested in it follow)
            if (key >= 0 && !skip) {
                nd_open[gdep] = true;
                nd_op[gdep] = gop;
                nd_k[gdep] = gk;
                nd_have[gdep] = gh;
                nd_acc[gdep] = g * sc;
            }
            if (e >= ncs) { break; }
            key = nkey;
            gh = false;
            g = 0.0;
            skip = false;
            let gi = nkey >> 8;
            let gb = group_at(gi);
            let gr = D(gb);
            let gn = D(gb + 1);
            let gs = D(gb + 2);
            let ni = i32(gn.y + 0.5);
            let ib = inst_at(i32(gn.x + 0.5) + (nkey & 255));
            gop = gr.z;
            gk = gr.w * amount;
            gdep = nh_depth(gn.w);
            var sphere = gs;
            sc = 1.0;
            if (ni > 0) {
                let ic = D(ib + 3);
                sc = ic.w;
                sphere = vec4<f32>(ic.xyz, gs.w * sc);
            }
            nd_close(gdep, mode);
            let par = nd_parent(gdep);
            skip = !nh_nonleaf(gn.w) && par.y > 0.5 && group_far(gr, sphere, p, par.x, amount, rm, mode);
            q = to_group(ni, ib, p);
        }
        if (skip) { continue; }
        if ((ent & 0x80000000u) != 0u) { continue; }   // (the mark of a group that has groups nested in it: it always opens)
        let si = i32(ent & 511u);
        let rec = shape_rec(si);
        let misc = D(rec + 7);
        let op = misc.x;
        let k = misc.y * amount;
        if (!gh) {
            if (op < 0.5) { continue; }
        } else {
            if (op > -0.5 && op < 0.5) { continue; }
            if (op < 1.5) {
                let bs = D(rec);
                let lb = length(q - bs.xyz) - bs.w;
                if (lb >= far_enough(select(-g, g, op > 0.0), k, rm, mode)) { continue; }
            }
        }
        let b = shape_dist(shape_at(si), q);
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
    nd_close(0, mode);
    var acc = nd_root;
    if (G.melt.z > 0.0) {
        let slope = G.melt.z * 1.32 * 0.12 * G.melt.w;
        acc = acc / sqrt(1.0 + slope * slope);
    }
    return acc;
}

fn dist_only(p: vec3<f32>) -> f32 { return map_d(p); }

// the four corners of the tetrahedron the normal is taken over
fn tetra_k(i: i32) -> vec3<f32> {
    if (i == 0) { return vec3<f32>(1.0, -1.0, -1.0); }
    if (i == 1) { return vec3<f32>(-1.0, -1.0, 1.0); }
    if (i == 2) { return vec3<f32>(-1.0, 1.0, -1.0); }
    return vec3<f32>(1.0, 1.0, 1.0);
}

// The surface normal from the distance field: four taps on a tetrahedron, or (`fast`, while the
// view moves) three forward differences from the distance `d0` already known at `p`. All the taps
// go through ONE call of the distance function inside a loop: a driver compiler inlines every
// call site of it, and the distance function is by far the biggest thing in the shader.
fn surface_normal(p: vec3<f32>, e: f32, fast: bool, d0: f32) -> vec3<f32> {
    var acc = vec3<f32>(0.0);
    let taps = select(4, 3, fast);
    for (var i = 0; i < taps; i = i + 1) {
        var k = tetra_k(i);
        if (fast) { k = vec3<f32>(select(0.0, 1.0, i == 0), select(0.0, 1.0, i == 1), select(0.0, 1.0, i == 2)); }
        let d = dist_only(p + k * e);
        if (fast) { acc[i] = d - d0; } else { acc = acc + k * d; }
    }
    if (fast) { return normalize(acc + vec3<f32>(0.0, 0.0, 1e-7)); }
    return normalize(acc);
}

fn normal_at(p: vec3<f32>, e: f32) -> vec3<f32> {
    return surface_normal(p, e, false, 0.0);
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
    // Only the shapes near the shadow ray are read (as for the primary march). The penumbra is
    // 10 h / t, so a shape farther than tend / 10 from the ray cannot darken any point of it.
    var list_ok = false;
    use_cand = false;
    if (G.offs2.z < 0.5) {
        find_candidates(ro, rd, t, tend, 1.0 + G.melt.z + 0.1 * tend);
        if (ncs == 0 && !ovf) { return 1.0; }
        use_cand = true;
        list_ok = !ovf;
    }
    let max_step = select(6.0, 14.0, nsteps < 20);
    for (var i = 0; i < nsteps; i = i + 1) {
        var h = 0.0;
        if (list_ok) {
            h = map_d_cand(ro + rd * t);
        } else {
            h = map_d(ro + rd * t);
        }
        res = min(res, 10.0 * h / t);
        t = t + clamp(h, 0.4, max_step);
        if (res < 0.001 || t > tend) { break; }
    }
    use_cand = false;
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

// Analysis view modes (G.hover.z: 0 normal, 1 zebra horizontal, 2 zebra vertical, 3 rings, 4 up / down).
// Same formulas as view_mode.rs. One call, at the end of fs; no loops.
fn view_mode_band(x: f32) -> f32 { return smoothstep(0.0, 0.06, fract(x)) - smoothstep(0.5, 0.56, fract(x)); }

fn view_mode_color(c: vec3<f32>, n: vec3<f32>, rd: vec3<f32>, on: bool) -> vec3<f32> {
    let m = i32(G.hover.z + 0.5);
    if (!on || m < 1 || m > 4) { return c; }
    let d = dot(rd, n);
    let r = rd - 2.0 * d * n;
    let ndv = clamp(-d, 0.0, 1.0);
    let x = select(dot(r, G.cam_up.xyz), dot(r, G.cam_right.xyz), m == 2);
    let zebra = mix(vec3<f32>(0.06, 0.06, 0.08), vec3<f32>(0.96, 0.96, 0.94), view_mode_band(x * 5.0));
    let rings = mix(vec3<f32>(0.10, 0.16, 0.34), vec3<f32>(0.88, 0.94, 1.0), view_mode_band(ndv * 9.0));
    let ud0 = select(vec3<f32>(0.85, 0.22, 0.20), vec3<f32>(0.22, 0.72, 0.34), n.z > 0.0) * (0.7 + 0.3 * ndv);
    let ud = mix(ud0, vec3<f32>(0.10, 0.30, 1.0), 1.0 - smoothstep(0.02, 0.05, abs(n.z)));
    var o = select(zebra, rings, m == 3);
    o = select(o, ud, m == 4);
    // (the colours are display values: a target that encodes sRGB itself wants them linear)
    return select(o, pow(o, vec3<f32>(2.2)), flag(1));
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

    // The screen-space widths of the grid lines. A derivative needs every pixel of its 2x2 block to
    // run the same code, so they are taken here, before anything branches on what the pixel
    // shows (inside the ground branches, pixels at the edge of a shape took different ways, and
    // DirectX's compiler warns about gradients in divergent flow).
    let gp0 = ro + rd * t_ground;
    // Grid spacing comes from Preferences (`env[4].w`); 10 mm is the default.
    let gs = max(G.env[4].w, 0.1);
    let gw1 = max(fwidth(gp0.xy / gs), vec2<f32>(1e-4));
    let gw2 = max(fwidth(gp0.xy / (gs * 10.0)), vec2<f32>(1e-4));
    let gwy = max(fwidth(gp0.y), 1e-4);
    let gwx = max(fwidth(gp0.x), 1e-4);

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
    // The modelling floor is a shadow receiver and grid overlay, never a ray-march clip plane.
    let t_cut = t_max;
    var last = false;
    // a little per-pixel jitter turns banding in the lamp glow into fine noise
    t = t + fract(sin(dot(in.pos.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453) * 1.5;
    // groups and pattern copies far from this ray are left out of the march (not with lamps: their glow reaches far)
    use_cand = false;
    var list_ok = false;
    if (run && !glow_scene && G.offs2.z < 0.5) {
        // (the selection outline looks a few pixels out from the ray)
        let slack = 1.0 + G.melt.z + 3.0 * select(t_cut * foot, foot, tanh <= 0.0);
        find_candidates(ro, rd, t, t_cut, slack);
        use_cand = true;
        list_ok = !ovf;
        // nothing near the ray: it cannot hit anything
        if (ncs == 0 && !ovf) { run = false; }
    }
    // Over-relaxed sphere tracing: steps a little longer than the distance, taken back when two
    // steps would have jumped over a surface. Lamps and a selected carver need small steps.
    // (a march that reads only what is near the ray takes longer steps than one that reads everything, so it is relaxed a little less)
    var relax = select(1.1, select(1.5, 1.3, list_ok), fast);
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
        } else if (list_ok) {
            d = map_d_cand(p);
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
            last = false;
            continue;
        }
        // hit when closer than a fraction of a pixel (never finer than the old fixed limit)
        if (d < max(0.004 * (1.0 + t * 0.002), pix * select(0.2, 0.5, fast))) { hit = true; break; }
        stp = step;
        prev_r = d;
        if (last) { break; }
        if (t_cut < t_max && t + step >= t_cut) {
            // no jump past the floor: the last point looked at is the floor itself
            step = max(t_cut - t, 0.0);
            stp = step;
            last = true;
        }
        t = t + step;
        if (t > t_max) { break; }
    }
    if (hit && !glow_scene) {
        use_cand = false;
        h = map(ro + rd * t);
    }

    // What is seen is the floor or a surface (never both). Either way the key light's shadow of
    // that point is wanted: it is found here, in one place (the code of a shadow is large).
    let above = ro.z >= 0.0;
    let ground_in_front = t_ground < 1e8 && (!hit || t_ground < t);
    let floor_vis = above && t_ground < 1e8 && !hit && floor_on;
    let surf_vis = hit;
    var n_hit = vec3<f32>(0.0, 0.0, 1.0);
    var sh_ro = vec3<f32>(0.0);
    var sh_steps = 0;
    if (floor_vis && floor_on && !fast) {
        sh_ro = ro + rd * t_ground + vec3<f32>(0.0, 0.0, 0.05);
        sh_steps = 32;
    } else if (surf_vis) {
        let p0 = ro + rd * t;
        // (three taps are enough while moving)
        n_hit = surface_normal(p0, select(0.02 + t * 0.0008, 0.03 + t * 0.0015, fast), fast, h.d);
        sh_ro = p0 + n_hit * 0.1;
        if (n_lights() > 0) { sh_steps = select(32, 4, fast); }
    }
    var shadow_raw = 1.0;
    if (sh_steps > 0) { shadow_raw = soft_shadow(sh_ro, key, 300.0, sh_steps); }
    // the same for the occlusion (full quality only), at one call site
    var ao_p = vec3<f32>(0.0);
    var ao_n = vec3<f32>(0.0, 0.0, 1.0);
    var ao_do = false;
    if (!fast) {
        if (floor_vis && floor_on) {
            ao_p = ro + rd * t_ground;
            ao_do = true;
        } else if (surf_vis) {
            ao_p = ro + rd * t;
            ao_n = n_hit;
            ao_do = true;
        }
    }
    var ao_raw = 1.0;
    if (ao_do) { ao_raw = ambient_occ(ao_p, ao_n); }

    // ground
    if (floor_vis) {
        let gp = ro + rd * t_ground;
        if (floor_on) {
            // contact shadow of the blob on the ground
            if (fast) {
                // while moving: one tap for a soft dark patch under the blob
                // (a floor point more than 6 mm clear of the scene's sphere gets full light: no need to measure)
                if (G.bound.w > 0.0 && length(gp - G.bound.xyz) < G.bound.w + 12.0) {
                    let ao = clamp(map_d(gp + vec3<f32>(0.0, 0.0, 6.0)) / 6.0, 0.0, 1.0);
                    col = col * mix(1.0, mix(0.6, 1.0, ao), shadow_k);
                }
            } else {
                col = col * mix(1.0, mix(0.55, 1.0, shadow_raw), shadow_k);
                col = col * mix(1.0, mix(0.6, 1.0, ao_raw), shadow_k);
            }
        }
    }

    if (surf_vis) {
        let p = ro + rd * t;
        let n = n_hit;
        let v = -rd;
        let rough = clamp(h.rough, 0.04, 1.0);
        let metal = h.metal;
        let alb = lin(h.col);
        let ndv = max(dot(n, v), 0.0);
        let fres = pow(1.0 - ndv, 5.0);
        let ao = ao_raw;
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
                sh = mix(1.0, shadow_raw, shadow_k);
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

    // The grid is an overlay on geometry behind its plane from either side; it never occludes.
    if (grid_on && ground_in_front) {
        let gp = ro + rd * t_ground;
        let a = abs(fract(gp.xy / gs - 0.5) - 0.5) / gw1;
        let line = 1.0 - min(min(a.x, a.y), 1.0);
        if (above) {
            // Keep the existing LOD handoff for the top-side grid.
            let f1 = 1.0 - smoothstep(0.12, 0.45, max(gw1.x, gw1.y));
            let f2 = 1.0 - smoothstep(0.12, 0.45, max(gw2.x, gw2.y));
            let major = abs(fract(gp.xy / (gs * 10.0) - 0.5) - 0.5) / gw2;
            let lm = 1.0 - min(min(major.x, major.y), 1.0);
            let axisx = 1.0 - min(abs(gp.y) / gwy, 1.0);
            let axisy = 1.0 - min(abs(gp.x) / gwx, 1.0);
            col = mix(col, col + vec3<f32>(0.03), line * 0.5 * f1);
            col = mix(col, col + vec3<f32>(0.05), lm * 0.6 * f2);
            col = mix(col, lin(vec3<f32>(0.85, 0.35, 0.35)), axisx * 0.55);
            col = mix(col, lin(vec3<f32>(0.42, 0.78, 0.42)), axisy * 0.55);
        } else {
            let fade = (1.0 - smoothstep(0.12, 0.45, max(gw1.x, gw1.y))) * 0.6;
            let axisx = 1.0 - min(abs(gp.y) / gwy, 1.0);
            let axisy = 1.0 - min(abs(gp.x) / gwx, 1.0);
            col = mix(col, col + vec3<f32>(0.04), line * 0.5 * fade);
            col = mix(col, lin(vec3<f32>(0.85, 0.35, 0.35)), axisx * 0.55 * 0.6);
            col = mix(col, lin(vec3<f32>(0.42, 0.78, 0.42)), axisy * 0.55 * 0.6);
        }
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

    return vec4<f32>(view_mode_color(encode(col), n_hit, rd, surf_vis), 1.0);
}
