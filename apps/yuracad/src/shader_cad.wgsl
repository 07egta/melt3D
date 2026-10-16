// CAD features, kinds 16..: Revolve, Loft, Curve, Sweep.
// Profiles live in G.poly (misc.z start, misc.w count; loft top = ext.z start, ext.w count),
// path nodes in G.path (ext.x start, ext.y count; xyz + radius).
fn cad_dist(s: Shape, q: vec3<f32>, kind: i32) -> f32 {
    return length(q) - s.size.x;
}
