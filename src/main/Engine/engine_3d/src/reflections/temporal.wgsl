struct TemporalUniforms {
    resolution     : vec2<f32>,
    blend          : f32,
    enabled        : f32,
    depth_reject_m : f32,
    gbuffer_scale  : f32,
}

@group(0) @binding(0) var<uniform> u : TemporalUniforms;
@group(0) @binding(1) var t_curr : texture_2d<f32>;
@group(0) @binding(2) var t_history : texture_2d<f32>;
@group(0) @binding(3) var t_velocity : texture_2d<f32>;
@group(0) @binding(4) var s_linear : sampler;
@group(0) @binding(5) var t_depth : texture_2d<f32>;
@group(0) @binding(6) var t_hit_uv_curr : texture_2d<f32>;
@group(0) @binding(7) var t_hit_uv_history : texture_2d<f32>;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv          : vec2<f32>,
}

struct TemporalOut {
    @location(0) reflection : vec4<f32>,
    @location(1) hit_uv     : vec4<f32>,
}

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

fn clip_axis(aabb_min : f32, aabb_max : f32, p : f32, q : f32) -> f32 {
    var r = q - p;
    let rmax = aabb_max - p;
    let rmin = aabb_min - p;
    let eps = 0.00001;
    if r > rmax + eps { r = rmax; }
    if r < rmin - eps { r = rmin; }
    return p + r;
}

fn clip_vec4(aabb_min : vec4<f32>, aabb_max : vec4<f32>, anchor : vec4<f32>, q : vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        clip_axis(aabb_min.r, aabb_max.r, anchor.r, q.r),
        clip_axis(aabb_min.g, aabb_max.g, anchor.g, q.g),
        clip_axis(aabb_min.b, aabb_max.b, anchor.b, q.b),
        clip_axis(aabb_min.a, aabb_max.a, anchor.a, q.a),
    );
}

const VARIANCE_BOX : f32 = 1.20;
const RPC_9 : f32 = 1.0 / 9.0;

fn depth_at(uv : vec2<f32>) -> f32 {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let gb_res = u.resolution * u.gbuffer_scale;
    let fc = clamped * gb_res - vec2<f32>(0.5);
    let px = vec2<i32>(fc);
    return textureLoad(t_depth, px, 0).r;
}

fn uv_in_bounds(uv : vec2<f32>) -> bool {
    return uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0;
}

fn temporal_blend(curr : vec4<f32>, hist_raw : vec4<f32>, uv : vec2<f32>) -> vec4<f32> {
    let texel = vec2<f32>(1.0) / u.resolution;
    var sum  = vec4<f32>(0.0);
    var sum2 = vec4<f32>(0.0);
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel;
            let c = textureSample(t_curr, s_linear, uv + off);
            sum  += c;
            sum2 += c * c;
        }
    }
    let avg = sum * RPC_9;
    let dev = sqrt(max(sum2 * RPC_9 - avg * avg, vec4<f32>(0.0)));
    let cmin = avg - dev * VARIANCE_BOX;
    let cmax = avg + dev * VARIANCE_BOX;
    let hist = clip_vec4(cmin, cmax, curr, hist_raw);
    let b = clamp(u.blend, 0.0, 0.95);
    return mix(curr, hist, b);
}

@fragment
fn fs_main(in : VsOut) -> TemporalOut {
    let curr = textureSample(t_curr, s_linear, in.uv);
    let hit_uv_curr = textureSample(t_hit_uv_curr, s_linear, in.uv).xy;

    var out : TemporalOut;
    out.hit_uv = vec4<f32>(hit_uv_curr, 0.0, 1.0);

    // Sin traza SSR este frame: no arrastrar historial (evita grano blanco en metales).
    if curr.a < 0.01 {
        out.reflection = curr;
        return out;
    }

    if u.enabled < 0.5 {
        out.reflection = curr;
        return out;
    }

    let vel = textureSample(t_velocity, s_linear, in.uv).xy;
    let prev_surface_uv = in.uv - vel;
    let hit_curr = curr.a > 0.02;

    if hit_curr {
        var hit_uv_hist = vec2<f32>(0.0);
        if uv_in_bounds(prev_surface_uv) {
            hit_uv_hist = textureSample(t_hit_uv_history, s_linear, prev_surface_uv).xy;
        }
        let hit_uv_vel = hit_uv_curr - hit_uv_hist;
        let prev_uv_refl = in.uv - hit_uv_vel;

        if uv_in_bounds(prev_uv_refl) {
            let depth_curr = depth_at(in.uv);
            let depth_prev = depth_at(prev_surface_uv);
            let depth_reject = abs(depth_curr - depth_prev) > u.depth_reject_m;
            if !depth_reject {
                let hist_raw = textureSample(t_history, s_linear, prev_uv_refl);
                out.reflection = temporal_blend(curr, hist_raw, in.uv);
                return out;
            }
        }
        out.reflection = curr;
        return out;
    }

    if !uv_in_bounds(prev_surface_uv) {
        out.reflection = curr;
        return out;
    }

    let depth_curr = depth_at(in.uv);
    let depth_prev = depth_at(prev_surface_uv);
    let depth_reject = abs(depth_curr - depth_prev) > u.depth_reject_m;

    let hist_raw = textureSample(t_history, s_linear, prev_surface_uv);
    let hit_prev = hist_raw.a > 0.02;
    if depth_reject || hit_prev {
        out.reflection = curr;
        return out;
    }

    out.reflection = temporal_blend(curr, hist_raw, in.uv);
    return out;
}
