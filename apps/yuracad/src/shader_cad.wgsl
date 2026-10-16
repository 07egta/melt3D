// CAD features, kinds 16, 17, 19: Revolve, Loft, Sweep (Curve, 18, is in shader_curve.wgsl).
// Profiles live in G.poly (misc.z start, misc.w count; loft top = ext.z start, ext.w count),
// path nodes in G.path (ext.x start, ext.y count; xyz + radius). Keep in step with cad.rs.

// polygon distance that ignores edges lying on the Z axis (x = 0); the sign counts all edges
fn polygon_off_axis_d(start: i32, n: i32, p: vec2<f32>) -> f32 {
    if (n < 3) { return 1e9; }
    var d = 1e18;
    var sgn = 1.0;
    var j = n - 1;
    for (var i = 0; i < n; i = i + 1) {
        let vi = get_pt(start + i);
        let vj = get_pt(start + j);
        let e = vj - vi;
        let w = p - vi;
        if (!(vi.x <= 1e-4 && vj.x <= 1e-4)) {
            let t = clamp(dot(w, e) / max(dot(e, e), 1e-12), 0.0, 1.0);
            let b = w - e * t;
            d = min(d, dot(b, b));
        }
        let c1 = p.y >= vi.y;
        let c2 = p.y < vj.y;
        let c3 = e.x * w.y > e.y * w.x;
        if ((c1 && c2 && c3) || (!c1 && !c2 && !c3)) { sgn = -sgn; }
        j = i;
    }
    return sgn * sqrt(d);
}

// signed distance to the wedge of angles 0..deg about Z, seen from above
fn wedge_d(p: vec2<f32>, deg: f32) -> f32 {
    let a = radians(deg);
    let e1 = vec2<f32>(cos(a), sin(a));
    var d0 = length(p);
    if (p.x > 0.0) { d0 = abs(p.y); }
    var d1 = length(p);
    if (dot(p, e1) > 0.0) { d1 = abs(e1.x * p.y - e1.y * p.x); }
    let d = min(d0, d1);
    let left = p.y >= 0.0;
    let right = e1.x * p.y - e1.y * p.x <= 0.0;
    var inside = left && right;
    if (deg > 180.0) { inside = left || right; }
    if (inside) { return -d; }
    return d;
}

// ---- rounding a profile (see cad.rs): corners are shrunk per sample from their neighbours ----

fn unit2(v: vec2<f32>) -> vec2<f32> {
    let l = length(v);
    if (l < 1e-9) { return vec2<f32>(0.0); }
    return v / l;
}

// the corner `cur` between `prev` and `next`, moved inwards by rr (orient +1 = counter-clockwise);
// with `axis` a side on the Z axis counts as the mirror image of the other side
fn inset_corner(prev: vec2<f32>, cur: vec2<f32>, next: vec2<f32>, orient: f32, rr: f32, axis: bool) -> vec2<f32> {
    let t1 = unit2(cur - prev);
    let t2 = unit2(next - cur);
    var n1 = vec2<f32>(-t1.y, t1.x) * orient;
    var n2 = vec2<f32>(-t2.y, t2.x) * orient;
    if (axis) {
        let on1 = prev.x <= 1e-4 && cur.x <= 1e-4;
        let on2 = cur.x <= 1e-4 && next.x <= 1e-4;
        if (on1 && !on2) { n1 = vec2<f32>(-n2.x, n2.y); }
        if (on2 && !on1) { n2 = vec2<f32>(-n1.x, n1.y); }
        if (on1 && on2) { n1 = vec2<f32>(0.0); n2 = vec2<f32>(0.0); }
    }
    let den = max(1.0 + dot(n1, n2), 0.1);
    return cur + (n1 + n2) * (rr / den);
}

fn polygon_inset_d(start: i32, n: i32, p: vec2<f32>, rr: f32, orient: f32, axis: bool) -> f32 {
    if (n < 3) { return 1e9; }
    var d = 1e18;
    var sgn = 1.0;
    var j = n - 1;
    var oj = get_pt(start + n - 1);
    var vj = inset_corner(get_pt(start + n - 2), oj, get_pt(start), orient, rr, axis);
    for (var i = 0; i < n; i = i + 1) {
        let oi = get_pt(start + i);
        let vi = inset_corner(get_pt(start + (i + n - 1) % n), oi, get_pt(start + (i + 1) % n), orient, rr, axis);
        let e = vj - vi;
        let w = p - vi;
        if (!(axis && oi.x <= 1e-4 && oj.x <= 1e-4)) {
            let t = clamp(dot(w, e) / max(dot(e, e), 1e-12), 0.0, 1.0);
            let b = w - e * t;
            d = min(d, dot(b, b));
        }
        let c1 = p.y >= vi.y;
        let c2 = p.y < vj.y;
        let c3 = e.x * w.y > e.y * w.x;
        if ((c1 && c2 && c3) || (!c1 && !c2 && !c3)) { sgn = -sgn; }
        j = i;
        oj = oi;
        vj = vi;
    }
    return sgn * sqrt(d);
}

// the rounding that really applies (round, in profile units, kept inside extra.y = the limit)
fn profile_round(s: Shape, scale: f32) -> f32 {
    return min(max(s.size.w, 0.0) / max(scale, 1e-3), s.extra.y);
}

fn revolve_dist(s: Shape, q: vec3<f32>) -> f32 {
    var d = 0.0;
    let rr = profile_round(s, 1.0);
    if (s.extra.w > 0.5 && s.size.w > 0.0) {
        d = cn_cut_polygon(polygon_off_axis_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), vec2<f32>(length(q.xy), q.z)), vec2<f32>(length(q.xy), q.z), i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), s.extra.z, s.size.w, true);
    } else if (rr > 0.0) {
        d = polygon_inset_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), vec2<f32>(length(q.xy), q.z), rr, s.extra.z, true) - rr;
    } else {
        d = polygon_off_axis_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), vec2<f32>(length(q.xy), q.z));
    }
    let ang = clamp(s.params.x, 1.0, 360.0);
    if (ang < 359.99) { d = max(d, wedge_d(q.xy, ang)); }
    return d;
}

fn loft_dist(s: Shape, q: vec3<f32>) -> f32 {
    let h = max(s.size.x, 1e-3);
    let d1 = polygon_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), q.xy);
    let d2 = polygon_d(i32(s.ext.z + 0.5), i32(s.ext.w + 0.5), q.xy);
    let t = clamp(q.z / h + 0.5, 0.0, 1.0);
    let rr = min(max(s.size.w, 0.0), h * 0.5);
    let w = vec2<f32>(d1 + (d2 - d1) * t + rr, abs(q.z) - h * 0.5 + rr);
    var d = min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - rr;
    if (s.extra.w > 0.5 && rr > 0.0) { d = cn_pair(d1 + (d2 - d1) * t, abs(q.z) - h * 0.5, rr); }
    return d / (1.0 + abs(d2 - d1) / h);
}

fn sweep_dist(s: Shape, q: vec3<f32>) -> f32 {
    let n = i32(s.ext.y + 0.5);
    let base = i32(s.ext.x + 0.5);
    if (n < 2 || s.misc.w < 2.5) { return 1e5; }
    var best = 3.4e38;
    var bi = 0;
    var bt = 0.0;
    for (var i = 0; i < n - 1; i = i + 1) {
        let a = path_pt(base + i).xyz;
        let b = path_pt(base + i + 1).xyz;
        let ab = b - a;
        let ap = q - a;
        let t = clamp(dot(ap, ab) / max(dot(ab, ab), 1e-12), 0.0, 1.0);
        let c = ap - ab * t;
        let d2 = dot(c, c);
        if (d2 < best) { best = d2; bi = i; bt = t; }
    }
    let pa = path_pt(base + bi);
    let pb = path_pt(base + bi + 1);
    let ab = pb.xyz - pa.xyz;
    var tn = vec3<f32>(0.0, 0.0, 1.0);
    if (length(ab) > 1e-12) { tn = ab / length(ab); }
    var up = vec3<f32>(0.0, 0.0, 1.0);
    if (abs(tn.z) > 0.99) { up = vec3<f32>(0.0, 1.0, 0.0); }
    let nrm = normalize(cross(up, tn));
    let bin = cross(tn, nrm);
    let c = pa.xyz + ab * bt;
    let r = max(abs(mix(pa.w, pb.w, bt)), 1e-3);
    let dd = q - c;
    let uv = vec2<f32>(dot(dd, nrm), dot(dd, bin)) / r;
    var d2 = polygon_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), uv) * r;
    let rr = profile_round(s, r);
    if (s.extra.w > 0.5 && s.size.w > 0.0) {
        d2 = cn_cut_polygon(d2 / r, uv, i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), s.extra.z, s.size.w / max(r, 1e-3), false) * r;
    } else if (rr > 0.0) {
        d2 = (polygon_inset_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), uv, rr, s.extra.z, false) - rr) * r;
    }
    var axial = -1e5;
    if (bi == 0 && bt <= 0.0) { axial = -dot(q - pa.xyz, tn); }
    if (bi == n - 2 && bt >= 1.0) { axial = max(axial, dot(q - pb.xyz, tn)); }
    let w = vec2<f32>(d2, axial);
    return (min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0)))) * 0.7;
}

fn cad_dist(s: Shape, q: vec3<f32>, kind: i32) -> f32 {
    if (kind == 16) { return revolve_dist(s, q); }
    if (kind == 17) { return loft_dist(s, q); }
    if (kind == 19) { return sweep_dist(s, q); }
    return length(q) - s.size.x;
}
