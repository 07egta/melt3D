// Extra solids, kinds 6..15. `q` is the point in the shape's own space (already divided by scale).
// s.size.xyz = size, s.size.w = rounding, s.params = extra numbers.
// Same formulas as prims.rs (the CPU copy); keep them in step. All shapes are centred on the origin, Z up.

const PM_PI: f32 = 3.14159265;
const PM_SQRT3_2: f32 = 0.8660254;
const PM_TET_S: f32 = 0.9428090;

fn pm_mod(a: f32, m: f32) -> f32 { return a - m * floor(a / m); }
fn pm_sgn(x: f32) -> f32 { if (x < 0.0) { return -1.0; } return 1.0; }
fn pm_count(v: f32, lo: f32, hi: f32) -> f32 { return max(lo, min(hi, floor(v + 0.5))); }
fn pm_d2(v: vec3<f32>) -> f32 { return dot(v, v); }

fn pm_ud_triangle(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> f32 {
    let ba = b - a; let pa = p - a;
    let cb = c - b; let pb = p - b;
    let ac = a - c; let pc = p - c;
    let nor = cross(ba, ac);
    let inside = pm_sgn(dot(cross(ba, nor), pa)) + pm_sgn(dot(cross(cb, nor), pb)) + pm_sgn(dot(cross(ac, nor), pc)) >= 2.0;
    var r = 0.0;
    if (inside) {
        let h = dot(nor, pa);
        r = h * h / max(pm_d2(nor), 1e-12);
    } else {
        let s0 = pm_d2(ba * clamp(dot(ba, pa) / max(pm_d2(ba), 1e-12), 0.0, 1.0) - pa);
        let s1 = pm_d2(cb * clamp(dot(cb, pb) / max(pm_d2(cb), 1e-12), 0.0, 1.0) - pb);
        let s2 = pm_d2(ac * clamp(dot(ac, pc) / max(pm_d2(ac), 1e-12), 0.0, 1.0) - pc);
        r = min(s0, min(s1, s2));
    }
    return sqrt(r);
}

fn pm_sd_triangle(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0; let e1 = p2 - p1; let e2 = p0 - p2;
    let v0 = p - p0; let v1 = p - p1; let v2 = p - p2;
    let q0 = v0 - e0 * clamp(dot(v0, e0) / max(dot(e0, e0), 1e-12), 0.0, 1.0);
    let q1 = v1 - e1 * clamp(dot(v1, e1) / max(dot(e1, e1), 1e-12), 0.0, 1.0);
    let q2 = v2 - e2 * clamp(dot(v2, e2) / max(dot(e2, e2), 1e-12), 0.0, 1.0);
    let s = pm_sgn(e0.x * e2.y - e0.y * e2.x);
    let c0 = vec2<f32>(dot(q0, q0), s * (v0.x * e0.y - v0.y * e0.x));
    let c1 = vec2<f32>(dot(q1, q1), s * (v1.x * e1.y - v1.y * e1.x));
    let c2 = vec2<f32>(dot(q2, q2), s * (v2.x * e2.y - v2.y * e2.x));
    let m = min(min(c0, c1), c2);
    return -sqrt(m.x) * pm_sgn(m.y);
}

// (distance along the face normal, distance along the face >= 0) in the first sector of a regular n-gon.
fn pm_fold_poly(x: f32, y: f32, n: f32) -> vec2<f32> {
    let an = PM_PI / n;
    let bn = pm_mod(atan2(y, x), 2.0 * an) - an;
    let rho = length(vec2<f32>(x, y));
    return vec2<f32>(rho * cos(bn), rho * abs(sin(bn)));
}

fn pm_poly_n(x: f32, y: f32, r: f32, n: f32) -> f32 {
    let an = PM_PI / n;
    let f = pm_fold_poly(x, y, n);
    let px = f.x - r * cos(an);
    let ky = r * sin(an);
    let py = f.y - ky + min(max(ky - f.y, 0.0), ky);
    return length(vec2<f32>(px, py)) * pm_sgn(px);
}

fn pm_extrude(d2: f32, z: f32, hh: f32, rr: f32) -> f32 {
    let w0 = d2;
    let w1 = abs(z) - hh + rr;
    return min(max(w0, w1), 0.0) + length(vec2<f32>(max(w0, 0.0), max(w1, 0.0))) - rr;
}

fn pm_octa(q: vec3<f32>, s: f32) -> f32 {
    let p = abs(q);
    let m = p.x + p.y + p.z - s;
    var r = p;
    if (3.0 * p.x < m) {
        r = p;
    } else if (3.0 * p.y < m) {
        r = vec3<f32>(p.y, p.z, p.x);
    } else if (3.0 * p.z < m) {
        r = vec3<f32>(p.z, p.x, p.y);
    } else {
        return m * 0.57735026;
    }
    let k = min(max(0.5 * (r.z - r.y + s), 0.0), s);
    return length(vec3<f32>(r.x, r.y - s + k, r.z - k));
}

fn pm_tetra(q: vec3<f32>, rc: f32) -> f32 {
    let off = rc / 3.0;
    let dn = vec2<f32>(PM_TET_S * 0.5, PM_TET_S * PM_SQRT3_2);
    let dpl = max(max(-q.z - off, dn.x * q.x + dn.y * q.y + q.z / 3.0 - off),
                  max(-PM_TET_S * q.x + q.z / 3.0 - off, dn.x * q.x - dn.y * q.y + q.z / 3.0 - off));
    if (dpl <= 0.0) { return dpl; }
    let rho = rc * PM_TET_S;
    let t = vec3<f32>(0.0, 0.0, rc);
    let b0 = vec3<f32>(rho, 0.0, -off);
    let b1 = vec3<f32>(-0.5 * rho, PM_SQRT3_2 * rho, -off);
    let b2 = vec3<f32>(-0.5 * rho, -PM_SQRT3_2 * rho, -off);
    return min(min(pm_ud_triangle(q, t, b0, b1), pm_ud_triangle(q, t, b1, b2)),
               min(pm_ud_triangle(q, t, b2, b0), pm_ud_triangle(q, b0, b1, b2)));
}

fn pm_pyramid(q: vec3<f32>, n: f32, a: f32, zb: f32, zt: f32) -> f32 {
    let an = PM_PI / n;
    let f = pm_fold_poly(q.x, q.y, n);
    let z = q.z;
    let l = max(length(vec2<f32>(zt - zb, a)), 1e-6);
    let din = max(((zt - zb) * f.x + a * z - a * zt) / l, zb - z);
    if (din <= 0.0) { return din; }
    let e = a * tan(an);
    let tri = pm_ud_triangle(vec3<f32>(f.x, f.y, z), vec3<f32>(0.0, 0.0, zt), vec3<f32>(a, e, zb), vec3<f32>(a, -e, zb));
    let base = length(vec3<f32>(max(f.x - a, 0.0), max(f.y - e, 0.0), z - zb));
    return min(tri, base);
}

fn pm_star2(x: f32, y: f32, r: f32, n: f32, ratio: f32) -> f32 {
    let an = PM_PI / n;
    let bn = abs(pm_mod(atan2(y, x) + an, 2.0 * an) - an);
    let rho = length(vec2<f32>(x, y));
    let px = rho * cos(bn);
    let py = rho * sin(bn);
    let e = vec2<f32>(ratio * r * cos(an) - r, ratio * r * sin(an));
    let t = clamp(((px - r) * e.x + py * e.y) / max(dot(e, e), 1e-12), 0.0, 1.0);
    let d = length(vec2<f32>(px - r - e.x * t, py - e.y * t));
    if (e.x * py - e.y * (px - r) > 0.0) { return -d; }
    return d;
}

fn prim_dist(s: Shape, q: vec3<f32>, kind: i32) -> f32 {
    let sz = s.size.xyz;
    let rnd = max(s.size.w, 0.0);
    if (kind == 6 || kind == 7) {
        var n = 3.0;
        if (kind == 7) { n = pm_count(s.params.x, 3.0, 32.0); }
        let hh = sz.y * 0.5;
        let ca = cos(PM_PI / n);
        let rr = min(min(rnd, hh), sz.x * ca * 0.98);
        return pm_extrude(pm_poly_n(q.x, q.y, sz.x - rr / ca, n), q.z, hh, rr);
    } else if (kind == 8) {
        let rr = min(rnd, sz.x * 0.3);
        return pm_octa(q, sz.x - rr * 1.7320508) - rr;
    } else if (kind == 9) {
        let rr = min(rnd, sz.x * 0.3);
        return pm_tetra(q, sz.x - 3.0 * rr) - rr;
    } else if (kind == 10) {
        let n = pm_count(s.params.x, 3.0, 16.0);
        let a = sz.x * cos(PM_PI / n);
        let h = sz.y;
        let l = max(length(vec2<f32>(h, a)), 1e-6);
        let rr = min(rnd, a * h / (l + a) * 0.98);
        let hh = h * 0.5;
        return pm_pyramid(q, n, a - rr * (l + a) / max(h, 1e-6), -hh + rr, hh - rr * l / max(a, 1e-6)) - rr;
    } else if (kind == 11) {
        let hl = sz.y * 0.5;
        return length(vec3<f32>(q.x, q.y, q.z - max(-hl, min(hl, q.z)))) - sz.x;
    } else if (kind == 12) {
        let r = sz.x;
        let rr = min(rnd, r * 0.5);
        let rad = r - rr;
        let zp = q.z + r * 0.5 - rr;
        let rho = length(q.xy);
        if (zp >= 0.0) { return max(length(vec2<f32>(rho, zp)) - rad, -zp) - rr; }
        if (rho <= rad) { return -zp - rr; }
        return length(vec2<f32>(rho - rad, zp)) - rr;
    } else if (kind == 13) {
        let t = min(max(sz.y, 0.0), sz.x);
        let hw = t * 0.5;
        let hh = sz.z * 0.5;
        let rr = min(min(rnd, hw), hh);
        let rho = length(q.xy);
        var dx = abs(rho - (sz.x - hw)) - hw + rr;
        if (t >= sz.x) { dx = rho - sz.x + rr; }
        let d = vec2<f32>(dx, abs(q.z) - hh + rr);
        return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0))) - rr;
    } else if (kind == 14) {
        let w = sz.x * 0.5;
        let dd = sz.y * 0.5;
        let h = sz.z * 0.5;
        let rin = max((sz.x + sz.z - length(vec2<f32>(sz.x, sz.z))) * 0.5, 1e-6);
        let rr = min(min(rnd, dd), rin * 0.98);
        let c = vec2<f32>(-w + rin, -h + rin);
        let k = (rin - rr) / rin;
        let d2 = pm_sd_triangle(vec2<f32>(q.x, q.z), c + (vec2<f32>(-w, -h) - c) * k, c + (vec2<f32>(w, -h) - c) * k, c + (vec2<f32>(-w, h) - c) * k);
        let w0 = d2;
        let w1 = abs(q.y) - dd + rr;
        return min(max(w0, w1), 0.0) + length(vec2<f32>(max(w0, 0.0), max(w1, 0.0))) - rr;
    } else if (kind == 15) {
        let n = pm_count(s.params.x, 3.0, 16.0);
        let ratio = max(0.1, min(0.9, s.params.y));
        let hh = sz.y * 0.5;
        let an = PM_PI / n;
        let e = vec2<f32>(ratio * sz.x * cos(an) - sz.x, ratio * sz.x * sin(an));
        let el = max(length(e), 1e-6);
        let rr = min(min(rnd, hh), e.y * sz.x / el * 0.5);
        let c2 = (e.y * sz.x - rr * el) / el;
        let r2 = c2 * el / max(e.y, 1e-6);
        let w2 = c2 * el / max(e.y * cos(an) - e.x * sin(an), 1e-6);
        return pm_extrude(pm_star2(q.x, q.y, r2, n, w2 / max(r2, 1e-6)), q.z, hh, rr);
    }
    return length(q) - sz.x;
}
