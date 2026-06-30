struct SceneTaaUniforms {
    resolution     : vec2<f32>,
    blend          : f32,
    enabled        : f32,
    zoom_stability : f32,
    _pad_jitter    : f32,
    jitter         : vec2<f32>,
    disocclusion   : f32,
    near_plane     : f32,
    far_plane      : f32,
    _pad_vec2      : f32,
    _pad_align     : vec2<f32>,
    _pad_mat4      : vec2<f32>,
    inv_view_proj  : mat4x4<f32>,
    prev_view_proj : mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> u : SceneTaaUniforms;
@group(0) @binding(1) var t_curr    : texture_2d<f32>;
@group(0) @binding(2) var s_curr     : sampler;
@group(0) @binding(3) var t_hist     : texture_2d<f32>;
@group(0) @binding(4) var s_hist     : sampler;
@group(0) @binding(5) var t_depth    : texture_2d<f32>;
@group(0) @binding(6) var s_depth    : sampler;
@group(0) @binding(7) var t_velocity : texture_2d<f32>;
@group(0) @binding(8) var s_velocity : sampler;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv          : vec2<f32>,
}

const RPC_9 : f32 = 1.0 / 9.0;
const VARIANCE_BOX : f32 = 1.35;
const SCENE_MAX_BLEND : f32 = 0.86;

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

fn prepass_to_view_depth_m(ndc_z_gl : f32) -> f32 {
    let ndc_z_vk = ndc_z_gl * 0.5 + 0.5;
    return (u.near_plane * u.far_plane) / (u.far_plane - ndc_z_vk * (u.far_plane - u.near_plane));
}

fn depth_at_m(uv : vec2<f32>) -> f32 {
    return prepass_to_view_depth_m(textureSample(t_depth, s_depth, uv).r);
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
    let velocity = textureSample(t_velocity, s_velocity, uv).xy;
    let uv_hist = uv - velocity;

    let depth_curr = depth_at_m(uv);
    let depth_hist = depth_at_m(uv_hist);
    let disoccluded = abs(depth_curr - depth_hist) > u.disocclusion;

    // Reconstruye el AABB local con el vecindario 3x3 alrededor del píxel actual
    // (clip_history hace el muestreo + clamp; aplicamos su clamp también al history
    // reproyectado para rechazar ghosts que cayeron fuera del rango actual).
    let clipped = clip_history(uv, texel);
    var sum = vec3<f32>(0.0);
    var sum2 = vec3<f32>(0.0);
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel;
            let c = textureSample(t_curr, s_curr, uv + off).rgb;
            sum += c;
            sum2 += c * c;
        }
    }
    let avg_n = sum * RPC_9;
    let dev_n = sqrt(max(sum2 * RPC_9 - avg_n * avg_n, vec3<f32>(0.0)));
    let cmin = avg_n - dev_n * VARIANCE_BOX;
    let cmax = avg_n + dev_n * VARIANCE_BOX;
    var hist : vec3<f32>;
    if disoccluded {
        hist = curr_samp.rgb;
    } else {
        let raw = textureSample(t_hist, s_hist, uv_hist).rgb;
        hist = clip_color(cmin, cmax, curr_samp.rgb, raw);
    }

    let edge_dev = clipped.a;
    let curr = curr_samp.rgb;

    let reactive = 1.0 - min(length(curr - hist) * 2.2, 1.0);
    let vel_w = 1.0 - min(length(velocity) * 6.0, 0.72);
    let far = 1.0 - clamp(u.zoom_stability, 0.0, 1.0);
    let base_blend = mix(u.blend, u.blend * 0.82, far * 0.22);
    let edge_color = smoothstep(0.003, 0.055, edge_dev);
    let edge_sobel = smoothstep(0.012, 0.095, sobel_edge(uv, texel));
    let depth_edge = smoothstep(0.05, 1.0, abs(depth_curr - depth_hist));
    let edge = max(max(edge_color, edge_sobel), depth_edge);
    // Más history en siluetas aliased (sube anti-aliasing en bordes de objetos).
    var blend_w = base_blend * reactive * vel_w + edge * 0.48;
    if disoccluded {
        blend_w = min(blend_w, 0.22);
    }
    blend_w = clamp(blend_w, 0.0, SCENE_MAX_BLEND);

    let out_rgb = mix(curr, hist, blend_w);
    return vec4<f32>(out_rgb, curr_samp.a);
}
