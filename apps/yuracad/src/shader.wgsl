// YURACAD viewport: sphere-traces the blob scene. The formulas match sdf.rs.

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
    mat2: vec4<f32>,    // x = clear coat
};

struct Globals {
    cam_pos: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    cam_fwd: vec4<f32>,   // w = tan(fov / 2); 0 = orthographic
    view: vec4<f32>,      // width px, height px, ortho half height, srgb target (1 / 0)
    melt: vec4<f32>,      // mode, amount, drip, ripple
    counts: vec4<f32>,    // groups, shapes, selected first shape (-1 none), selected count
    groups: array<vec4<f32>, 24>,   // first shape, count, op sign, blend
    shapes: array<Shape, 64>,
    poly: array<vec4<f32>, 128>,    // outline points, two per vec4 (x y x y)
    bound: vec4<f32>,               // centre, radius of a sphere around everything (0 = nothing)
    hover: vec4<f32>,               // first hovered shape (-1 none), count
    env: array<vec4<f32>, 11>,      // exposure/ambient/shadow/flags, backdrop+lights, 3 sky colours, 3 x (light dir+power, light colour)
};

@group(0) @binding(0) var<uniform> G: Globals;

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
    let v = G.poly[u32(i) / 2u];
    if ((u32(i) & 1u) == 0u) { return v.xy; }
    return v.zw;
}

fn shape_dist(s: Shape, p: vec3<f32>) -> f32 {
    let d = p - s.pos.xyz;
    let q = vec3<f32>(dot(s.r0.xyz, d), dot(s.r1.xyz, d), dot(s.r2.xyz, d)) / s.scale.xyz;
    let kind = i32(s.scale.w + 0.5);
    let sz = s.size.xyz;
    let rd = s.size.w;
    var r = 0.0;
    if (kind == 0) {
        r = length(q) - sz.x;
    } else if (kind == 1) {
        let b = sz * 0.5;
        let rr = min(rd, min(b.x, min(b.y, b.z)));
        let dd = abs(q) - b + vec3<f32>(rr);
        r = length(max(dd, vec3<f32>(0.0))) + min(max(dd.x, max(dd.y, dd.z)), 0.0) - rr;
    } else if (kind == 2) {
        let rr = min(rd, min(sz.x, sz.y * 0.5));
        let dd = vec2<f32>(length(q.xy) - sz.x + rr, abs(q.z) - sz.y * 0.5 + rr);
        r = min(max(dd.x, dd.y), 0.0) + length(max(dd, vec2<f32>(0.0))) - rr;
    } else if (kind == 3) {
        let a = length(q.xy) - sz.x;
        r = length(vec2<f32>(a, q.z)) - sz.y;
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
    } else {
        let d2 = polygon_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), q.xy);
        let rr = min(rd, sz.x * 0.5);
        let w = vec2<f32>(d2 + rr, abs(q.z) - sz.x * 0.5 + rr);
        r = min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - rr;
    }
    return r * s.pos.w;
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

// op: +1 add, -1 carve, 0 paint (colour only)
fn combine(a: Hit, b: Hit, op: f32, k: f32, mode: i32) -> Hit {
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
        o.id = select(a.id, b.id, h > 0.5);
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

fn map(p0: vec3<f32>) -> Hit {
    let p = warp(p0);
    let mode = i32(G.melt.x + 0.5);
    let amount = G.melt.y;
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
    var have = false;
    let ng = i32(G.counts.x + 0.5);
    for (var gi = 0; gi < ng; gi = gi + 1) {
        let gr = G.groups[gi];
        let first = i32(gr.x + 0.5);
        let cnt = i32(gr.y + 0.5);
        var g: Hit;
        var gh = false;
        for (var si = 0; si < cnt; si = si + 1) {
            let s = G.shapes[first + si];
            var h: Hit;
            h.d = shape_dist(s, p);
            h.col = s.color.rgb;
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
        if (!gh) { continue; }
        if (!have) {
            if (gr.z < 0.5) { continue; }
            acc = g;
            have = true;
        } else {
            acc = combine(acc, g, gr.z, gr.w * amount, mode);
        }
    }
    if (G.melt.z > 0.0) {
        let slope = G.melt.z * 1.32 * 0.12 * G.melt.w;
        acc.d = acc.d / sqrt(1.0 + slope * slope);
    }
    return acc;
}

fn dist_only(p: vec3<f32>) -> f32 { return map(p).d; }

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
        d = min(d, shape_dist(G.shapes[first + i], p));
    }
    return d;
}

fn soft_shadow(ro: vec3<f32>, rd: vec3<f32>, maxt: f32) -> f32 {
    var res = 1.0;
    var t = 0.6;
    for (var i = 0; i < 40; i = i + 1) {
        let h = dist_only(ro + rd * t);
        res = min(res, 10.0 * h / t);
        t = t + clamp(h, 0.4, 6.0);
        if (res < 0.001 || t > maxt) { break; }
    }
    return clamp(res, 0.0, 1.0);
}

fn ambient_occ(p: vec3<f32>, n: vec3<f32>) -> f32 {
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
    if (G.view.w > 0.5) { return x; }
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
    let sel_carving = has_sel && G.shapes[i32(G.counts.z + 0.5)].misc.x < 0.5;
    var sel_abs = 1e9;
    var sel_hit = false;
    let foot = px;
    var halo = vec3<f32>(0.0);
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
    for (var i = 0; i < 200; i = i + 1) {
        if (!run) { break; }
        let p = ro + rd * t;
        h = map(p);
        if (has_sel) {
            let sd = sel_dist(p);
            let pix = select(t * foot, foot, tanh <= 0.0);
            sel_abs = min(sel_abs, abs(sd) / max(pix, 0.02));
            if (sd < 0.02) { sel_hit = true; }
        }
        if (h.glow > 0.0) {
            // a soft glow in the air around lamps
            halo = halo + lin(h.col) * h.glow * exp(-max(h.d, 0.0) * 0.15) * max(h.d, 0.05) * 0.9 * 0.009;
        }
        if (h.d < 0.004 * (1.0 + t * 0.002)) { hit = true; break; }
        var step = h.d * 0.9;
        if (h.glow > 0.0) {
            // small steps near lamps keep their glow smooth (no rings)
            step = min(step, max(3.0, h.d * 0.25));
        }
        if (has_sel && sel_carving) { step = min(step, max(abs(sel_dist(p)) * 0.8, 0.15)); }
        t = t + step;
        if (t > t_max) { break; }
    }

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
            let sh = soft_shadow(gp + vec3<f32>(0.0, 0.0, 0.05), key, 300.0);
            col = col * mix(1.0, mix(0.55, 1.0, sh), shadow_k);
            let ao = ambient_occ(gp, vec3<f32>(0.0, 0.0, 1.0));
            col = col * mix(1.0, mix(0.6, 1.0, ao), shadow_k);
        }
    }

    if (hit && (t_ground > t || !(grid_on || floor_on))) {
        let p = ro + rd * t;
        let n = normal_at(p, 0.02 + t * 0.0008);
        let v = -rd;
        let rough = clamp(h.rough, 0.04, 1.0);
        let metal = h.metal;
        let alb = lin(h.col);
        let ndv = max(dot(n, v), 0.0);
        let fres = pow(1.0 - ndv, 5.0);
        let ao = ambient_occ(p, n);
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
            thin = 1.0 - clamp(-dist_only(p - n * 5.0) / 5.0, 0.0, 1.0) * 0.8;
        }
        var lit = vec3<f32>(0.0);
        var spec = vec3<f32>(0.0);
        for (var i = 0; i < nl; i = i + 1) {
            let L = light_dir(i);
            let lc = light_col(i);
            var sh = 1.0;
            if (i == 0) {
                sh = mix(1.0, soft_shadow(p + n * 0.1, L, 300.0), shadow_k);
                sh = mix(sh, 1.0, h.glass * 0.6);
            }
            let ndl = max(dot(n, L), 0.0);
            // translucent things wrap the light around their edges
            let wrap = h.trans * 0.55;
            let ndl_w = clamp((dot(n, L) + wrap) / (1.0 + wrap), 0.0, 1.0);
            let hv = normalize(L + v);
            let nh = max(dot(n, hv), 0.0);
            let lobe = pow(nh, spec_pow) * (spec_pow + 8.0) / 60.0;
            let coat_lobe = pow(nh, 420.0) * (420.0 + 8.0) / 60.0;
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
        c = c + env_reflect(rr, 0.05) * (0.04 + 0.96 * fres) * h.coat * 0.55 * mix(0.5, 1.0, ao);
        // a thin clean rim
        c = c + vec3<f32>(0.05) * fres * ao;

        // glass: bend the view ray through the body, and see what is behind it
        if (h.glass > 0.01) {
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
