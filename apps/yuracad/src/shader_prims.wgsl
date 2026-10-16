// Extra solids, kinds 6..15. `q` is the point in the shape's own space (already divided by scale).
// s.size.xyz = size, s.size.w = rounding, s.params = extra numbers.
fn prim_dist(s: Shape, q: vec3<f32>, kind: i32) -> f32 {
    return length(q) - s.size.x;
}
