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
    mat: vec4<f32>,     // x = metallic
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
};

@group(0) @binding(0) var<uniform> G: Globals;

struct Hit {
    d: f32,
    col: vec3<f32>,
    rough: f32,
    metal: f32,
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

fn combine(a: Hit, b: Hit, sub: bool, k: f32, mode: i32) -> Hit {
    var o: Hit;
    if (!sub) {
        let r = smin_h(a.d, b.d, k, mode);
        let h = r.y;
        o.d = r.x;
        o.col = mix(b.col, a.col, h);
        o.rough = mix(b.rough, a.rough, h);
        o.metal = mix(b.metal, a.metal, h);
        o.id = select(b.id, a.id, h >= 0.5);
    } else {
        let r = smin_h(-a.d, b.d, k, mode);
        let h = r.y;
        o.d = -r.x;
        o.col = mix(b.col, a.col, 1.0 - clamp(1.0 - h, 0.0, 1.0) * (1.0 - h));
        o.rough = a.rough;
        o.metal = a.metal;
        o.id = select(a.id, b.id, h > 0.5);
    }
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
            h.id = f32(first + si);
            if (!gh) {
                if (s.misc.x < 0.0) { continue; }
                g = h;
                gh = true;
            } else {
                g = combine(g, h, s.misc.x < 0.0, s.misc.y * amount, mode);
            }
        }
        if (!gh) { continue; }
        if (!have) {
            if (gr.z < 0.0) { continue; }
            acc = g;
            have = true;
        } else {
            acc = combine(acc, g, gr.z < 0.0, gr.w * amount, mode);
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

fn sky(rd: vec3<f32>, rough: f32) -> vec3<f32> {
    let t = clamp(rd.z * 0.5 + 0.5, 0.0, 1.0);
    let top = vec3<f32>(0.62, 0.66, 0.74);
    let hor = vec3<f32>(0.46, 0.46, 0.48);
    let bot = vec3<f32>(0.22, 0.22, 0.23);
    var c = mix(bot, hor, smoothstep(0.0, 0.5, t));
    c = mix(c, top, smoothstep(0.5, 1.0, t));
    // a soft window light
    let l = normalize(vec3<f32>(0.5, -0.6, 0.8));
    c = c + vec3<f32>(1.0, 0.97, 0.9) * pow(max(dot(rd, l), 0.0), mix(60.0, 6.0, rough)) * 0.9;
    return c;
}

fn lin(c: vec3<f32>) -> vec3<f32> { return pow(c, vec3<f32>(2.2)); }

fn background(uv: vec2<f32>) -> vec3<f32> {
    // soft neutral grey, a little lighter in the middle
    let c = vec2<f32>(uv.x - 0.5, uv.y - 0.45);
    let v = clamp(1.0 - dot(c, c) * 1.1, 0.0, 1.0);
    return lin(mix(vec3<f32>(0.235, 0.235, 0.245), vec3<f32>(0.33, 0.33, 0.345), v));
}

fn encode(c0: vec3<f32>) -> vec3<f32> {
    // a gentle filmic curve keeps colours rich instead of washing them out
    let c = c0 * 0.78;
    let m = (c * (2.51 * c + vec3<f32>(0.03))) / (c * (2.43 * c + vec3<f32>(0.59)) + vec3<f32>(0.14));
    let x = clamp(m, vec3<f32>(0.0), vec3<f32>(1.0));
    if (G.view.w > 0.5) { return x; }
    return pow(x, vec3<f32>(1.0 / 2.2));
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
    let sel_carving = has_sel && G.shapes[i32(G.counts.z + 0.5)].misc.x < 0.0;
    var sel_abs = 1e9;
    var sel_hit = false;
    let foot = select(px, px, tanh <= 0.0);
    for (var i = 0; i < 200; i = i + 1) {
        let p = ro + rd * t;
        h = map(p);
        if (has_sel) {
            let sd = sel_dist(p);
            let pix = select(t * foot, foot, tanh <= 0.0);
            sel_abs = min(sel_abs, abs(sd) / max(pix, 0.02));
            if (sd < 0.02) { sel_hit = true; }
        }
        if (h.d < 0.004 * (1.0 + t * 0.002)) { hit = true; break; }
        var step = h.d * 0.9;
        if (has_sel && sel_carving) { step = min(step, max(abs(sel_dist(p)) * 0.8, 0.15)); }
        t = t + step;
        if (t > 2000.0) { break; }
    }

    // ground
    let above = ro.z >= 0.0;
    if (above && t_ground < 1e8 && (!hit || t_ground < t)) {
        let gp = ro + rd * t_ground;
        let fade = clamp(1.0 - length(gp.xy) / 600.0, 0.0, 1.0);
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
        // contact shadow of the blob on the ground
        let l = normalize(vec3<f32>(0.5, -0.6, 0.8));
        let sh = soft_shadow(gp + vec3<f32>(0.0, 0.0, 0.05), l, 300.0);
        col = col * mix(0.55, 1.0, sh);
        let ao = ambient_occ(gp, vec3<f32>(0.0, 0.0, 1.0));
        col = col * mix(0.6, 1.0, ao);
    }

    if (hit && (t_ground > t)) {
        let p = ro + rd * t;
        let n = normal_at(p, 0.02 + t * 0.0008);
        let v = -rd;
        let l = normalize(vec3<f32>(0.5, -0.6, 0.8));
        let l2 = normalize(vec3<f32>(-0.7, 0.4, 0.35));
        let rough = clamp(h.rough, 0.04, 1.0);
        let metal = h.metal;
        let alb = lin(h.col);
        let sh = soft_shadow(p + n * 0.1, l, 300.0);
        let ao = ambient_occ(p, n);
        let ndl = max(dot(n, l), 0.0);
        let ndl2 = max(dot(n, l2), 0.0);
        let hv = normalize(l + v);
        let spec_pow = mix(300.0, 8.0, rough);
        let spec = pow(max(dot(n, hv), 0.0), spec_pow) * (spec_pow + 8.0) / 60.0;
        let fres = pow(1.0 - max(dot(n, v), 0.0), 5.0);
        let f0 = mix(vec3<f32>(0.04), alb, metal);
        let F = f0 + (vec3<f32>(1.0) - f0) * fres;
        let diff_col = alb * (1.0 - metal);
        let amb = sky(n, 1.0) * 0.55 * ao;
        var c = diff_col * (amb + vec3<f32>(1.0, 0.96, 0.9) * ndl * sh * 1.15 + vec3<f32>(0.55, 0.62, 0.75) * ndl2 * 0.35);
        c = c + F * spec * ndl * sh * 0.9;
        let rr = reflect(rd, n);
        let refl = sky(rr, rough) * mix(0.5, 1.0, ao);
        c = c + refl * F * mix(0.15, 1.0, metal) * (1.0 - rough * 0.65);
        // a thin clean rim
        c = c + vec3<f32>(0.05) * fres * ao;
        col = c;
    }

    // seen from below the ground is see-through: only its lines are drawn
    if (!above && t_ground < 1e8 && (!hit || t_ground < t)) {
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
