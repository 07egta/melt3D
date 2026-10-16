// Imported meshes, kind 20: a baked signed distance volume (sdf_volume.rs). All the volumes of the
// scene live one after another in one half-float texture (binding 2), four grid nodes to a texel
// (r, g, b, a), 2048 texels to a row. There is no filtering: the trilinear blend is done here with
// eight textureLoad calls, so it works on every backend (OpenGL included).
// Per shape: size.xyz = number of nodes along x, y, z; params.xyz = position of node (0, 0, 0) in the
// shape's own space; params.w = cell size; ext.x = number of the volume's first node in the texture.
// Keep in step with Volume::distance (sdf_volume.rs) and gpu.rs (VOL_W).

@group(0) @binding(2) var VT: texture_2d<f32>;

fn vol_node(n: i32) -> f32 {
    let t = n >> 2;
    let v = textureLoad(VT, vec2<i32>(t & 2047, t >> 11), 0);
    return v[n & 3];
}

fn mesh_dist(s: Shape, q: vec3<f32>) -> f32 {
    // a mesh shape without a volume (or one that did not fit the texture) is empty
    if (s.size.x < 0.5) { return 1e4; }
    let dims = max(s.size.xyz, vec3<f32>(1.0));
    let cell = max(s.params.w, 1e-6);
    let top = dims - vec3<f32>(1.0);
    let g = (q - s.params.xyz) / cell;
    let c = clamp(g, vec3<f32>(0.0), top);
    // outside the grid: the distance to the box of nodes plus the value at its nearest point
    let excess = length((g - c) * cell);
    let b = floor(c);
    let f = c - b;
    let i0 = vec3<i32>(min(b, top));
    let i1 = min(i0 + vec3<i32>(1), vec3<i32>(top));
    let nx = i32(dims.x + 0.5);
    let ny = i32(dims.y + 0.5);
    let base = i32(s.ext.x + 0.5);
    let row0 = base + nx * (i0.y + ny * i0.z);
    let row1 = base + nx * (i1.y + ny * i0.z);
    let row2 = base + nx * (i0.y + ny * i1.z);
    let row3 = base + nx * (i1.y + ny * i1.z);
    let c00 = mix(vol_node(row0 + i0.x), vol_node(row0 + i1.x), f.x);
    let c10 = mix(vol_node(row1 + i0.x), vol_node(row1 + i1.x), f.x);
    let c01 = mix(vol_node(row2 + i0.x), vol_node(row2 + i1.x), f.x);
    let c11 = mix(vol_node(row3 + i0.x), vol_node(row3 + i1.x), f.x);
    return mix(mix(c00, c10, f.y), mix(c01, c11, f.y), f.z) + excess;
}
