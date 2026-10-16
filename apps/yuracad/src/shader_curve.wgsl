// Curve (kind 18): a tube through the path nodes G.path[ext.x .. ext.x + ext.y] (xyz + radius).
fn curve_dist(s: Shape, q: vec3<f32>) -> f32 {
    return length(q) - s.size.x;
}
