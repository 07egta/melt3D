// Chamfers (corner style 1 of Shape::corner): the corner is cut off with a flat plane, `round` mm
// along each side of a right-angled corner. Same formulas as corner.rs (the CPU copy); keep them in
// step. s.extra.w = 1 turns the chamfer on; s.size.w is the size of the cut.

const CN_S: f32 = 0.70710678;

// two things with signed distances a and b, whose gradients are at right angles, meeting in a
// corner that is cut off c along each side
fn cn_pair(a: f32, b: f32, c: f32) -> f32 {
    return max(max(a, b), (a + b + c) * CN_S);
}

fn cn_cut_box(q: vec3<f32>, b: vec3<f32>, c: f32) -> f32 {
    let d = abs(q) - b;
    let bev = max(max(d.x + d.y + c, d.x + d.z + c), d.y + d.z + c) * CN_S;
    return max(max(d.x, max(d.y, d.z)), bev);
}

// the bevel plane through the edge of two planes (signed distances da, db, unit normals with the
// dot product nn); it lies c * CN_S inside the sharp edge
fn cn_bevel(da: f32, db: f32, nn: f32, c: f32) -> f32 {
    return (da + db) / sqrt(max(2.0 + 2.0 * nn, 0.05)) + c * CN_S;
}

// cuts the corner `cur` (between prev and next) off a flat shape with distance d at p, c along each
// side but at most half of either side; only convex corners (orient +1 = counter-clockwise outline)
fn cn_cut_vertex(d: f32, p: vec2<f32>, prev: vec2<f32>, cur: vec2<f32>, next: vec2<f32>, orient: f32, c: f32) -> f32 {
    let t1 = unit2(cur - prev);
    let t2 = unit2(next - cur);
    if ((t1.x * t2.y - t1.y * t2.x) * orient <= 1e-4) { return d; }
    let leg = min(c, min(length(cur - prev), length(next - cur)) * 0.5);
    if (leg <= 1e-6) { return d; }
    let p1 = cur - t1 * leg;
    let p2 = cur + t2 * leg;
    let e = unit2(p2 - p1);
    let n = vec2<f32>(e.y * orient, -e.x * orient);
    let g = dot(p - p1, n);
    return max(d, min(g, leg - length(p - cur)));
}

// every convex corner of the outline G.poly[start .. start + n]; with `axis` (revolve profiles) a
// side on the Z axis is no surface: the corner there is cut as if the other side were mirrored
fn cn_cut_polygon(d0: f32, p: vec2<f32>, start: i32, n: i32, orient: f32, c: f32, axis: bool) -> f32 {
    if (n < 3 || c <= 0.0) { return d0; }
    var d = d0;
    for (var i = 0; i < n; i = i + 1) {
        let a = get_pt(start + (i + n - 1) % n);
        let b = get_pt(start + i);
        let e = get_pt(start + (i + 1) % n);
        var prev = a;
        var next = e;
        var skip = false;
        if (axis) {
            let on_a = a.x <= 1e-4;
            let on_b = b.x <= 1e-4;
            let on_e = e.x <= 1e-4;
            if (on_b && on_a && on_e) { skip = true; }
            else if (on_b && on_a) { prev = vec2<f32>(-e.x, e.y); }
            else if (on_b && on_e) { next = vec2<f32>(-a.x, a.y); }
        }
        if (!skip) { d = cn_cut_vertex(d, p, prev, b, next, orient, c); }
    }
    return d;
}
