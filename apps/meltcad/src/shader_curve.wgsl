// Curve (kind 18): a tube through the points path_pt(ext.x .. ext.x + ext.y) (xyz + radius), already
// smoothed by the CPU (curve.rs). Two header entries sit just before the points: path_pt(ext.x - 2)
// and path_pt(ext.x - 1) are the low and high corners of a box around the whole tube. The low
// corner's w is the largest local join reach, so skipped segment groups remain exact; the high
// corner's w is the reach at the closure of a loop.
// params.y is unused here; params.z = 1 cuts the two ends flat; params.x = 1 is a closed loop.
// extra.x (shell) hollows the tube here; a flat-ended hollow tube is open at both ends.

// Distance to the round cone from a (radius ra) to b (radius rb).
fn round_cone(p: vec3<f32>, a: vec3<f32>, ra: f32, b: vec3<f32>, rb: f32) -> f32 {
    let ba = b - a;
    let l2 = dot(ba, ba);
    let rr = ra - rb;
    let a2 = l2 - rr * rr;
    let pa = p - a;
    if (l2 < 1.0e-4 || a2 < 1.0e-4) {
        if (ra >= rb) { return length(pa) - ra; }
        return length(p - b) - rb;
    }
    let il2 = 1.0 / l2;
    let y = dot(pa, ba);
    let z = y - l2;
    let w = pa * l2 - ba * y;
    let x2 = dot(w, w);
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    var sr = 1.0;
    if (rr < 0.0) { sr = -1.0; }
    var sy = 1.0;
    if (y < 0.0) { sy = -1.0; }
    var sz = 1.0;
    if (z < 0.0) { sz = -1.0; }
    let k = sr * rr * rr * x2;
    if (sz * a2 * z2 > k) { return sqrt(x2 + z2) * il2 - rb; }
    if (sy * a2 * y2 < k) { return sqrt(x2 + y2) * il2 - ra; }
    return (sqrt(x2 * a2 * il2) + y * rr) * il2 - ra;
}

// Same bend-and-radius reach as curve::joint_blend on the CPU.
fn curve_joint_blend(a: vec4<f32>, b: vec4<f32>, c: vec4<f32>) -> f32 {
    let ab = b.xyz - a.xyz;
    let bc = c.xyz - b.xyz;
    let la = length(ab);
    let lb = length(bc);
    if (la < 1.0e-4 || lb < 1.0e-4) { return 0.0; }
    let turn = sqrt(max(1.0 - clamp(dot(ab, bc) / (la * lb), -1.0, 1.0), 0.0));
    return min(min(min(a.w, b.w), c.w) * 0.5 * turn, min(la, lb) * 0.5);
}

// One local smooth union, never accumulated across the whole chain.
fn curve_smin(a: f32, b: f32, k: f32) -> f32 {
    if (k <= 1.0e-6) { return min(a, b); }
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return b + (a - b) * h - k * h * (1.0 - h);
}

// Segments per bounding sphere (curve::GROUP_SEGMENTS). The spheres follow the points in the path
// list: path_pt(ext.x + ext.y + g) is the sphere (centre, radius with the tube's thickness) of the
// segments g * 8 .. g * 8 + 7.
const CURVE_GROUP: i32 = 8;

fn curve_dist(s: Shape, q: vec3<f32>) -> f32 {
    let st = i32(s.ext.x + 0.5);
    let n = min(i32(s.ext.y + 0.5), 96);
    if (n < 2 || st < 2) { return 1.0e4; }
    // far away: the distance to the box around the tube is a safe, cheap answer
    let lo = path_pt(st - 2).xyz;
    let hi = path_pt(st - 1).xyz;
    let max_join = path_pt(st - 2).w;
    let ob = max(max(lo - q, q - hi), vec3<f32>(0.0));
    let db = length(ob);
    let reach = 2.0 + s.misc.y * G.melt.y * 6.0;
    if (db > reach) { return db; }
    // The nearest piece is found among the groups of segments, not among all of them: the spheres
    // bound the distance from both sides (every group has a tube point inside its sphere, and lies
    // inside it), so only the groups that can hold something nearer than what is known are measured.
    let ng = (n - 1 + CURVE_GROUP - 1) / CURVE_GROUP;
    let gb = st + n;
    var ub = 1.0e6;
    var lbmin = 1.0e6;
    for (var g = 0; g < ng; g = g + 1) {
        let sp = path_pt(gb + g);
        let dc = length(q - sp.xyz);
        ub = min(ub, dc + sp.w);
        lbmin = min(lbmin, dc - sp.w);
    }
    // (a lower bound, as the box above is: nothing of the tube is nearer)
    if (lbmin > reach) { return max(db, lbmin); }
    var d = 1.0e6;
    var first_d = 1.0e6;
    var last_d = 1.0e6;
    var prev_d = 1.0e6;
    var prev_i = -2;
    for (var g = 0; g < ng; g = g + 1) {
        let sp = path_pt(gb + g);
        if (length(q - sp.xyz) - sp.w > min(d, ub) + max_join) { continue; }
        let i0 = g * CURVE_GROUP;
        var a = path_pt(st + i0);
        for (var k = 1; k <= CURVE_GROUP; k = k + 1) {
            let i = i0 + k;
            if (i >= n) { break; }
            let b = path_pt(st + i);
            let one = round_cone(q, a.xyz, a.w, b.xyz, b.w);
            d = min(d, one);
            if (i == 1) { first_d = one; }
            if (prev_i == i - 2) {
                let before = path_pt(st + i - 2);
                d = min(d, curve_smin(prev_d, one, curve_joint_blend(before, a, b)));
            }
            prev_d = one;
            prev_i = i - 1;
            if (i == n - 1) { last_d = one; }
            a = b;
        }
    }
    if (s.params.x > 0.5 && first_d < 1.0e5 && last_d < 1.0e5) {
        d = min(d, curve_smin(last_d, first_d, path_pt(st - 1).w));
    }
    // a shell hollows the tube first (shape_dist leaves curves alone), then flat ends cut it open
    let wall = s.extra.x / max(s.pos.w, 1.0e-6);
    if (wall > 0.0) { d = abs(d + wall * 0.5) - wall * 0.5; }
    if (s.params.z > 0.5 && s.params.x < 0.5) {
        let p0 = path_pt(st);
        let p1 = path_pt(st + 1);
        let pn = path_pt(st + n - 1);
        let pm = path_pt(st + n - 2);
        let t0 = normalize(p1.xyz - p0.xyz + vec3<f32>(1.0e-6, 0.0, 0.0));
        let t1 = normalize(pn.xyz - pm.xyz + vec3<f32>(1.0e-6, 0.0, 0.0));
        d = max(d, max(dot(q - p0.xyz, -t0), dot(q - pn.xyz, t1)));
    }
    return d;
}
