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

fn revolve_dist(s: Shape, q: vec3<f32>) -> f32 {
    var d = polygon_off_axis_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), vec2<f32>(length(q.xy), q.z));
    let ang = clamp(s.params.x, 1.0, 360.0);
    if (ang < 359.99) { d = max(d, wedge_d(q.xy, ang)); }
    return d;
}

fn loft_dist(s: Shape, q: vec3<f32>) -> f32 {
    let h = max(s.size.x, 1e-3);
    let d1 = polygon_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), q.xy);
    let d2 = polygon_d(i32(s.ext.z + 0.5), i32(s.ext.w + 0.5), q.xy);
    let t = clamp(q.z / h + 0.5, 0.0, 1.0);
    let w = vec2<f32>(d1 + (d2 - d1) * t, abs(q.z) - h * 0.5);
    let d = min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0)));
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
        let a = G.path[base + i].xyz;
        let b = G.path[base + i + 1].xyz;
        let ab = b - a;
        let ap = q - a;
        let t = clamp(dot(ap, ab) / max(dot(ab, ab), 1e-12), 0.0, 1.0);
        let c = ap - ab * t;
        let d2 = dot(c, c);
        if (d2 < best) { best = d2; bi = i; bt = t; }
    }
    let pa = G.path[base + bi];
    let pb = G.path[base + bi + 1];
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
    let d2 = polygon_d(i32(s.misc.z + 0.5), i32(s.misc.w + 0.5), vec2<f32>(dot(dd, nrm), dot(dd, bin)) / r) * r;
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
