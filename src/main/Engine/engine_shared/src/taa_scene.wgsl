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
const VARIANCE_BOX : f32 = 1.35;
const SCENE_MAX_BLEND : f32 = 0.52;

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

fn clip_color(aabb_min : vec3<f32>, aabb_max : vec3<f32>, anchor : vec3<f32>, q : vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        clip_aabb(aabb_min.r, aabb_max.r, anchor.r, q.r),
        clip_aabb(aabb_min.g, aabb_max.g, anchor.g, q.g),
        clip_aabb(aabb_min.b, aabb_max.b, anchor.b, q.b),
    );
}

fn luminance(c : vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn lum_at(uv : vec2<f32>) -> f32 {
    return luminance(textureSample(t_curr, s_curr, uv).rgb);
}

/// Sobel sobre luminancia (compatible GLSL; sin muestrear depth).
fn sobel_edge(uv : vec2<f32>, texel : vec2<f32>) -> f32 {
    let tl = lum_at(uv + vec2<f32>(-texel.x, -texel.y));
    let t  = lum_at(uv + vec2<f32>(0.0, -texel.y));
    let tr = lum_at(uv + vec2<f32>(texel.x, -texel.y));
    let l  = lum_at(uv + vec2<f32>(-texel.x, 0.0));
    let r  = lum_at(uv + vec2<f32>(texel.x, 0.0));
    let bl = lum_at(uv + vec2<f32>(-texel.x, texel.y));
    let b  = lum_at(uv + vec2<f32>(0.0, texel.y));
    let br = lum_at(uv + vec2<f32>(texel.x, texel.y));
    let gx = -tl - 2.0 * l - bl + tr + 2.0 * r + br;
    let gy = -tl - 2.0 * t - tr + bl + 2.0 * b + br;
    return sqrt(gx * gx + gy * gy);
}

fn clip_history(uv : vec2<f32>, texel : vec2<f32>) -> vec4<f32> {
    var sum = vec3<f32>(0.0);
    var sum2 = vec3<f32>(0.0);
    var sum_l = 0.0;
    var sum2_l = 0.0;
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel;
            let c = textureSample(t_curr, s_curr, uv + off).rgb;
            let l = luminance(c);
            sum += c;
            sum2 += c * c;
            sum_l += l;
            sum2_l += l * l;
        }
    }
    let avg = sum * RPC_9;
    let dev = sqrt(max(sum2 * RPC_9 - avg * avg, vec3<f32>(0.0)));
    let avg_l = sum_l * RPC_9;
    let dev_l = sqrt(max(sum2_l * RPC_9 - avg_l * avg_l, 0.0));
    let cmin = avg - dev * VARIANCE_BOX;
    let cmax = avg + dev * VARIANCE_BOX;
    let anchor = clamp(avg, cmin, cmax);
    var hist = textureSample(t_hist, s_hist, uv).rgb;
    hist = clip_color(cmin, cmax, anchor, hist);
    return vec4<f32>(hist, dev_l);
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let curr_samp = textureSample(t_curr, s_curr, uv);
    if u.enabled < 0.5 {
        return curr_samp;
    }

    let texel = vec2<f32>(1.0) / u.resolution;
    let clipped = clip_history(uv, texel);
    let hist = clipped.rgb;
    let edge_dev = clipped.a;
    let curr = curr_samp.rgb;

    let reactive = 1.0 - min(length(curr - hist) * 3.5, 1.0);
    let far = 1.0 - clamp(u.zoom_stability, 0.0, 1.0);
    let base_blend = mix(u.blend, u.blend * 0.7, far * 0.35);
    let edge_color = smoothstep(0.004, 0.065, edge_dev);
    let edge_sobel = smoothstep(0.018, 0.12, sobel_edge(uv, texel));
    let edge = max(edge_color, edge_sobel);
    var blend_w = base_blend * reactive + edge * 0.35;
    blend_w = clamp(blend_w, 0.0, SCENE_MAX_BLEND);

    let out_rgb = mix(curr, hist, blend_w);
    return vec4<f32>(out_rgb, curr_samp.a);
}
