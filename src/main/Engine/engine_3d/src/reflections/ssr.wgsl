struct SsrUniforms {
    inv_view_proj     : mat4x4<f32>,
    view_proj         : mat4x4<f32>,
    cam_pos           : vec4<f32>,
    resolution        : vec2<f32>,
    max_steps         : u32,
    max_distance_m    : f32,
    max_roughness     : f32,
    step_size         : f32,
    near_plane        : f32,
    far_plane         : f32,
    clear_color       : vec4<f32>,
    ssr_blur_enabled  : f32,
    _pad              : f32,
    _struct_pad       : vec2<f32>,
}

@group(0) @binding(0) var<uniform> u : SsrUniforms;
@group(0) @binding(1) var t_depth : texture_2d<f32>;
@group(0) @binding(2) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(3) var t_lit_scene : texture_2d<f32>;
@group(0) @binding(4) var s_linear : sampler;
@group(0) @binding(5) var s_nearest : sampler;
@group(0) @binding(6) var t_direct : texture_2d<f32>;
@group(0) @binding(7) var t_surface : texture_2d<f32>;
@group(0) @binding(8) var t_base_color : texture_2d<f32>;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv          : vec2<f32>,
}

struct SsrOut {
    @location(0) reflection : vec4<f32>,
    @location(1) hit_uv       : vec4<f32>,
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

fn view_depth_m_from_world(world : vec3<f32>) -> f32 {
    return refl_view_depth_m_from_world(world, u.view_proj, u.near_plane, u.far_plane);
}

fn project_uv(world : vec3<f32>) -> vec2<f32> {
    return refl_project_uv(world, u.view_proj);
}

fn world_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, view_depth_m, u.inv_view_proj, u.near_plane, u.far_plane);
}

fn decode_octahedral(enc: vec2<f32>) -> vec3<f32> {
    var p = enc * 2.0 - vec2<f32>(1.0);
    var n = vec3<f32>(p.x, p.y, 1.0 - abs(p.x) - abs(p.y));
    if n.z < 0.0 {
        let ox = n.x;
        let oy = n.y;
        n.x = (1.0 - abs(oy)) * sign(ox);
        n.y = (1.0 - abs(ox)) * sign(oy);
    }
    return normalize(n);
}

fn normal_from_packed(packed: vec4<f32>) -> vec3<f32> {
    return decode_octahedral(packed.zw);
}

fn texel_px(uv : vec2<f32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let fc = clamped * u.resolution - vec2<f32>(0.5);
    return vec2<i32>(fc);
}

fn base_color_at(uv : vec2<f32>) -> vec3<f32> {
    let px = texel_px(uv);
    return textureLoad(t_base_color, px, 0).rgb;
}

fn lit_scene_at(hit_uv : vec2<f32>) -> vec3<f32> {
    let px = texel_px(hit_uv);
    return textureLoad(t_lit_scene, px, 0).rgb;
}

fn lit_scene_blurred(hit_uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    if u.ssr_blur_enabled < 0.5 {
        return lit_scene_at(hit_uv);
    }
    let ref_depth_m = depth_at(hit_uv);
    if ref_depth_m <= 0.0001 {
        return lit_scene_at(hit_uv);
    }
    let depth_reject_m = refl_depth_reject_m(u.step_size);
    let texel = vec2<f32>(1.0) / u.resolution;
    var acc = vec3<f32>(0.0);
    var w_sum = 0.0;
    for (var oy = -3; oy <= 3; oy++) {
        for (var ox = -3; ox <= 3; ox++) {
            let p = hit_uv + vec2<f32>(f32(ox), f32(oy)) * spacing_px * texel;
            let px = texel_px(p);
            let tap_depth_m = textureLoad(t_depth, px, 0).r;
            if tap_depth_m <= 0.0001 {
                continue;
            }
            if abs(tap_depth_m - ref_depth_m) > depth_reject_m {
                continue;
            }
            acc += textureLoad(t_lit_scene, px, 0).rgb;
            w_sum += 1.0;
        }
    }
    if w_sum < 1.0 {
        return lit_scene_at(hit_uv);
    }
    return acc / w_sum;
}

/// Color de rebote SSR: en metales, quitar direct (glint) para no duplicar hotspots blancos.
fn reflection_hit_color(hit_uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    let lit = lit_scene_blurred(hit_uv, spacing_px);
    let hit_px = texel_px(hit_uv);
    let hit_direct = textureLoad(t_direct, hit_px, 0);
    if hit_direct.a > 0.5 {
        return max(lit - hit_direct.rgb, vec3<f32>(0.0));
    }
    return lit;
}

fn depth_at(uv : vec2<f32>) -> f32 {
    let px = texel_px(uv);
    return textureLoad(t_depth, px, 0).r;
}

struct SsrHit {
    found : bool,
    hit_uv : vec2<f32>,
    strength : f32,
    roughness : f32,
    ray_depth : f32,
    hit_depth : f32,
    march_dist_m : f32,
}

fn trace_ssr(
    surf_uv : vec2<f32>,
    world_pos : vec3<f32>,
    n : vec3<f32>,
    roughness : f32,
    metallic : f32,
    albedo : vec3<f32>,
) -> SsrHit {
    var miss : SsrHit;
    miss.found = false;
    miss.hit_uv = vec2<f32>(-1.0);
    miss.strength = 0.0;
    miss.roughness = roughness;
    miss.ray_depth = 0.0;
    miss.hit_depth = 0.0;
    miss.march_dist_m = 0.0;

    let view_dir = normalize(u.cam_pos.xyz - world_pos);
    let refl_dir = refl_mirror_dir(world_pos, u.cam_pos.xyz, n);

    if dot(refl_dir, n) <= 0.0 {
        return miss;
    }

    let strength = refl_trace_strength(roughness, metallic, n, view_dir, albedo);
    if strength < 0.005 {
        return miss;
    }

    let normal_bias = refl_normal_bias(u.step_size);
    let march_start = world_pos + n * normal_bias + refl_dir * u.step_size;
    let march_end = world_pos + n * normal_bias + refl_dir * u.max_distance_m;

    let uv_start = project_uv(march_start);
    let uv_end = project_uv(march_end);
    if uv_start.x < 0.0 || uv_end.x < 0.0 {
        return miss;
    }

    let delta_uv = uv_end - uv_start;
    let px_len = length(delta_uv * u.resolution);
    let step_count = max(1u, min(u.max_steps, u32(ceil(px_len))));

    var dist_prev = u.step_size;
    var hit_uv = vec2<f32>(-1.0);

    for (var i = 0u; i < step_count; i++) {
        let t = f32(i + 1u) / f32(step_count);
        let dist = mix(u.step_size, u.max_distance_m, t);
        let march_world = world_pos + n * normal_bias + refl_dir * dist;
        let sample_uv = project_uv(march_world);

        if sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0 {
            let sample_depth_m = depth_at(sample_uv);
            if sample_depth_m > 0.0001 {
                let ray_depth_m = view_depth_m_from_world(march_world);
                let diff = ray_depth_m - sample_depth_m;
                let thick = refl_thickness_m(u.step_size, roughness);
                if diff > 0.0 && diff < thick {
                    var dist_lo = dist_prev;
                    var dist_hi = dist;
                    for (var k = 0u; k < 4u; k++) {
                        let dist_mid = (dist_lo + dist_hi) * 0.5;
                        let mid_world = world_pos + n * normal_bias + refl_dir * dist_mid;
                        let uvm = project_uv(mid_world);
                        let sd_m = depth_at(uvm);
                        let mid_depth = view_depth_m_from_world(mid_world);
                        if mid_depth - sd_m > 0.0 {
                            dist_hi = dist_mid;
                            hit_uv = uvm;
                        } else {
                            dist_lo = dist_mid;
                        }
                    }
                    if hit_uv.x < 0.0 {
                        hit_uv = sample_uv;
                    }
                    let hit_depth_m = depth_at(hit_uv);
                    var hit : SsrHit;
                    hit.found = true;
                    hit.hit_uv = hit_uv;
                    hit.strength = strength;
                    hit.roughness = roughness;
                    hit.ray_depth = view_depth_m_from_world(
                        world_pos + n * normal_bias + refl_dir * dist_hi,
                    );
                    hit.hit_depth = hit_depth_m;
                    hit.march_dist_m = dist_hi;
                    return hit;
                }
            }
        }
        dist_prev = dist;
    }

    return miss;
}

fn empty_ssr_out() -> SsrOut {
    var out : SsrOut;
    out.reflection = vec4<f32>(0.0);
    out.hit_uv = vec4<f32>(0.0);
    return out;
}

@fragment
fn fs_main(in : VsOut) -> SsrOut {
    let view_depth_m = depth_at(in.uv);
    if view_depth_m <= 0.0001 {
        return empty_ssr_out();
    }

    let px = texel_px(in.uv);
    let packed = textureLoad(t_normal_roughness, px, 0);
    let n = normal_from_packed(packed);
    let roughness = textureLoad(t_surface, px, 0).g;
    let metallic = textureLoad(t_direct, px, 0).a;
    let albedo = base_color_at(in.uv);

    if roughness > u.max_roughness {
        return empty_ssr_out();
    }

    let world_pos = world_pos_from_depth(in.uv, view_depth_m);
    let hit = trace_ssr(in.uv, world_pos, n, roughness, metallic, albedo);
    if !hit.found {
        return empty_ssr_out();
    }

    let spacing_px = refl_blur_spacing_px(hit.roughness, hit.march_dist_m);
    let hit_color = refl_metal_attenuate(
        reflection_hit_color(hit.hit_uv, spacing_px),
        albedo,
        metallic,
    );

    var out : SsrOut;
    out.reflection = vec4<f32>(hit_color, hit.strength);
    out.hit_uv = vec4<f32>(hit.hit_uv, 0.0, 1.0);
    return out;
}

/// Readback CPU en (0.5, 0.5). Hit: RG=hit_uv, BA=ray/hit depth (m).
/// Miss: R=código negativo, G=roughness, B=metallic, A=view_depth o strength.
///   -1 cielo/sin geometría  -2 rugosidad > max  -3 fuerza/dir  -4 marcha SSR
fn ssr_diagnostic_at(surf_uv : vec2<f32>) -> vec4<f32> {
    let view_depth_m = depth_at(surf_uv);
    if view_depth_m <= 0.0001 {
        return vec4<f32>(-1.0, 0.0, 0.0, 0.0);
    }

    let px = texel_px(surf_uv);
    let packed = textureLoad(t_normal_roughness, px, 0);
    let n = normal_from_packed(packed);
    let roughness = textureLoad(t_surface, px, 0).g;
    let metallic = textureLoad(t_direct, px, 0).a;
    let albedo = base_color_at(surf_uv);

    if roughness > u.max_roughness {
        return vec4<f32>(-2.0, roughness, metallic, view_depth_m);
    }

    let world_pos = world_pos_from_depth(surf_uv, view_depth_m);
    let view_dir = normalize(u.cam_pos.xyz - world_pos);
    let refl_dir = refl_mirror_dir(world_pos, u.cam_pos.xyz, n);
    let strength = refl_trace_strength(roughness, metallic, n, view_dir, albedo);
    if dot(refl_dir, n) <= 0.0 {
        return vec4<f32>(-3.0, roughness, metallic, strength);
    }
    if strength < 0.005 {
        return vec4<f32>(-5.0, roughness, metallic, strength);
    }

    let hit = trace_ssr(surf_uv, world_pos, n, roughness, metallic, albedo);
    if !hit.found {
        return vec4<f32>(-4.0, roughness, metallic, strength);
    }
    return vec4<f32>(hit.hit_uv.x, hit.hit_uv.y, hit.ray_depth, hit.hit_depth);
}

@fragment
fn fs_log(_in : VsOut) -> @location(0) vec4<f32> {
    return ssr_diagnostic_at(vec2<f32>(0.5, 0.5));
}
