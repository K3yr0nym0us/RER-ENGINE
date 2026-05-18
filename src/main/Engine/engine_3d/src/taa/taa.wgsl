struct TaaUniforms {
    resolution     : vec2<f32>,
    blend          : f32,
    enabled        : f32,
    zoom_stability : f32,
    _pad0          : f32,
    _pad1          : f32,
    _pad2          : f32,
}

@group(0) @binding(0) var<uniform> u : TaaUniforms;
@group(0) @binding(1) var t_curr   : texture_2d<f32>;
@group(0) @binding(2) var s_curr   : sampler;
@group(0) @binding(3) var t_hist   : texture_2d<f32>;
@group(0) @binding(4) var s_hist   : sampler;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv          : vec2<f32>,
}

const RPC_9 : f32 = 1.0 / 9.0;
const VARIANCE_BOX : f32 = 1.15;

@vertex
fn vs_main(@builtin(vertex_index) vi : u32) -> VsOut {
    var p : vec2<f32>;
    switch vi {
        case 0u: { p = vec2<f32>(-1.0, -1.0); }
        case 1u: { p = vec2<f32>( 3.0, -1.0); }
        default: { p = vec2<f32>(-1.0,  3.0); }
    }
    var out : VsOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv  = p * 0.5 + vec2<f32>(0.5, 0.5);
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

fn clip_aabb(aabb_min : f32, aabb_max : f32, p : f32, q : f32) -> f32 {
    var r = q - p;
    let rmax = aabb_max - p;
    let rmin = aabb_min - p;
    let eps = 0.00001;
    if r > rmax + eps { r = rmax; }
    if r < rmin - eps { r = rmin; }
    return p + r;
}

// Bilinear manual (R32 no filtrable) para suavizar antes del TAA.
fn sample_mask_bilinear(tex: texture_2d<f32>, uv: vec2<f32>) -> f32 {
    let res = u.resolution;
    let p = uv * res - vec2<f32>(0.5);
    let i0 = vec2<i32>(floor(p));
    let f = fract(p);
    let s00 = textureLoad(tex, i0, 0).r;
    let s10 = textureLoad(tex, i0 + vec2<i32>(1, 0), 0).r;
    let s01 = textureLoad(tex, i0 + vec2<i32>(0, 1), 0).r;
    let s11 = textureLoad(tex, i0 + vec2<i32>(1, 1), 0).r;
    return mix(mix(s00, s10, f.x), mix(s01, s11, f.x), f.y);
}

fn gather_neighborhood(uv : vec2<f32>, texel : vec2<f32>) -> vec3<f32> {
    var sum = 0.0;
    var sum2 = 0.0;
    var center = 0.0;
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel;
            let s = sample_mask_bilinear(t_curr, uv + off);
            if ox == 0 && oy == 0 {
                center = s;
            }
            sum += s;
            sum2 += s * s;
        }
    }
    let avg = sum * RPC_9;
    let dev = sqrt(max(sum2 * RPC_9 - avg * avg, 0.0));
    let color_min = avg - dev * VARIANCE_BOX;
    let color_max = avg + dev * VARIANCE_BOX;
    let anchor = clamp(avg, color_min, color_max);
    var hist = sample_mask_bilinear(t_hist, uv);
    hist = clip_aabb(color_min, color_max, anchor, hist);
    return vec3<f32>(center, hist, dev);
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    if u.enabled < 0.5 {
        let s = sample_mask_bilinear(t_curr, uv);
        return vec4<f32>(s, 0.0, 0.0, 0.0);
    }

    let texel = vec2<f32>(1.0) / u.resolution;
    let n = gather_neighborhood(uv, texel);
    let curr = n.x;
    var hist = n.y;

    let reactive = 1.0 - min(abs(curr - hist) * 6.0, 1.0);
    let far = 1.0 - clamp(u.zoom_stability, 0.0, 1.0);
    let base_blend = mix(u.blend, 0.92, far * 0.5);
    // En penumbra (gradiente alto) priorizar frame actual → menos estelas repetidas.
    let penumbra = smoothstep(0.006, 0.045, n.z);
    var blend_w = base_blend * reactive * (1.0 - penumbra * 0.92);
    blend_w = clamp(blend_w, 0.0, 0.88);

    let out_s = mix(curr, hist, blend_w);
    return vec4<f32>(out_s, 0.0, 0.0, 0.0);
}
