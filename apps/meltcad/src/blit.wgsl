// Copies the finished scene picture into egui's frame (one triangle).
//
// There are two pictures: A, the cheap one drawn while things move (any share of the texture,
// `a.xy` = the used share along x and y, `a.zw` = one texel), and B, the sharp one that is drawn a
// band of rows at a time while the scene is still. Rows of B above `c.x` (a share of the height)
// are finished and shown; the rest shows A. When a picture is smaller than the window it is
// scaled up with a bilinear read and a light contrast-adaptive sharpen (`c.y`, 0 = none).
@group(0) @binding(0) var tex_a: texture_2d<f32>;
@group(0) @binding(1) var tex_b: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct Params {
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
};
@group(0) @binding(3) var<uniform> P: Params;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    var o: VOut;
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

// a point of the used part of a picture, kept half a texel inside its edge
fn at(uv: vec2<f32>, share: vec2<f32>, texel: vec2<f32>) -> vec2<f32> {
    return clamp(uv * share, texel * 0.5, share - texel * 0.5);
}

fn read_a(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(tex_a, samp, at(uv, P.a.xy, P.a.zw), 0.0).rgb;
}

fn read_b(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(tex_b, samp, at(uv, P.b.xy, P.b.zw), 0.0).rgb;
}

fn read_pic(from_b: bool, uv: vec2<f32>) -> vec3<f32> {
    if (from_b) { return read_b(uv); }
    return read_a(uv);
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let from_b = in.uv.y < P.c.x;
    var share = P.a.xy;
    var texel = P.a.zw;
    if (from_b) {
        share = P.b.xy;
        texel = P.b.zw;
    }
    var c = read_pic(from_b, in.uv);
    // sharpen only a picture that is being scaled up
    if (P.c.y > 0.0 && (share.x < 0.999 || share.y < 0.999)) {
        // one source texel away, in uv units of the window
        let d = texel / share;
        let n = read_pic(from_b, in.uv + vec2<f32>(0.0, -d.y));
        let s = read_pic(from_b, in.uv + vec2<f32>(0.0, d.y));
        let w = read_pic(from_b, in.uv + vec2<f32>(-d.x, 0.0));
        let e = read_pic(from_b, in.uv + vec2<f32>(d.x, 0.0));
        let mn = min(c, min(min(n, s), min(w, e)));
        let mx = max(c, max(max(n, s), max(w, e)));
        // contrast adaptive: flat areas and strong edges are left alone, soft detail is lifted
        let amp = sqrt(clamp(min(mn, vec3<f32>(1.0) - mx) / max(mx, vec3<f32>(1e-4)), vec3<f32>(0.0), vec3<f32>(1.0)));
        let k = -amp * mix(0.125, 0.2, clamp(P.c.y, 0.0, 1.0));
        c = clamp((c + (n + s + w + e) * k) / (vec3<f32>(1.0) + 4.0 * k), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(c, 1.0);
}
