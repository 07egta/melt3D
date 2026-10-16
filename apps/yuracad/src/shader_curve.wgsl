// Curve (kind 18): a tube through the points G.path[ext.x .. ext.x + ext.y] (xyz + radius), already
// smoothed by the CPU (curve.rs). Two header entries sit just before the points: G.path[ext.x - 2]
// and G.path[ext.x - 1] are the low and high corners of a box around the whole tube.
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

fn curve_dist(s: Shape, q: vec3<f32>) -> f32 {
    let st = i32(s.ext.x + 0.5);
    let n = min(i32(s.ext.y + 0.5), 96);
    if (n < 2 || st < 2) { return 1.0e4; }
    // far away: the distance to the box around the tube is a safe, cheap answer
    let lo = G.path[st - 2].xyz;
    let hi = G.path[st - 1].xyz;
    let ob = max(max(lo - q, q - hi), vec3<f32>(0.0));
    let db = length(ob);
    let reach = 2.0 + s.misc.y * G.melt.y * 6.0;
    if (db > reach) { return db; }
    var d = 1.0e6;
    var a = G.path[st];
    for (var i = 1; i < n; i = i + 1) {
        let b = G.path[st + i];
        d = min(d, round_cone(q, a.xyz, a.w, b.xyz, b.w));
        a = b;
    }
    // a shell hollows the tube first (shape_dist leaves curves alone), then flat ends cut it open
    let wall = s.extra.x / max(s.pos.w, 1.0e-6);
    if (wall > 0.0) { d = abs(d + wall * 0.5) - wall * 0.5; }
    if (s.params.z > 0.5 && s.params.x < 0.5) {
        let p0 = G.path[st];
        let p1 = G.path[st + 1];
        let pn = G.path[st + n - 1];
        let pm = G.path[st + n - 2];
        let t0 = normalize(p1.xyz - p0.xyz + vec3<f32>(1.0e-6, 0.0, 0.0));
        let t1 = normalize(pn.xyz - pm.xyz + vec3<f32>(1.0e-6, 0.0, 0.0));
        d = max(d, max(dot(q - p0.xyz, -t0), dot(q - pn.xyz, t1)));
    }
    return d;
}
