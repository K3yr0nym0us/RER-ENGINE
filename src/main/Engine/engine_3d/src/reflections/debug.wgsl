struct DebugUniforms {
    mode              : u32,
    max_roughness     : f32,
    near_plane        : f32,
    far_plane         : f32,
    cam_pos           : vec4<f32>,
    inv_view_proj     : mat4x4<f32>,
    view_proj         : mat4x4<f32>,
    view              : mat4x4<f32>,
    resolution        : vec2<f32>,
    max_steps         : u32,
    max_distance_m    : f32,
    step_size         : f32,
    ssr_blur_enabled  : f32,
    _struct_pad       : vec2<f32>,
}

@group(0) @binding(0) var<uniform> u : DebugUniforms;
@group(0) @binding(1) var t_scene : texture_2d<f32>;
@group(0) @binding(2) var t_depth : texture_2d<f32>;
@group(0) @binding(3) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(4) var t_reflection : texture_2d<f32>;
@group(0) @binding(5) var s_linear : sampler;
@group(0) @binding(6) var s_nearest : sampler;
@group(0) @binding(7) var t_surface : texture_2d<f32>;
@group(0) @binding(8) var t_direct : texture_2d<f32>;
@group(0) @binding(9) var t_base_color : texture_2d<f32>;

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0) uv          : vec2<f32>,
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

fn texel_px(uv : vec2<f32>, dims : vec2<u32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let fc = clamped * vec2<f32>(dims) - vec2<f32>(0.5);
    return vec2<i32>(fc);
}

fn uv_to_ndc_xy(uv : vec2<f32>) -> vec2<f32> {
    return refl_uv_to_ndc_xy(uv);
}

fn ndc_xy_to_uv(ndc_xy : vec2<f32>) -> vec2<f32> {
    return refl_ndc_xy_to_uv(ndc_xy);
}

fn world_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, view_depth_m, u.inv_view_proj, u.near_plane, u.far_plane);
}

fn view_depth_m_from_world(world : vec3<f32>) -> f32 {
    return refl_view_depth_m_from_world(world, u.view_proj, u.near_plane, u.far_plane);
}

fn project_uv(world : vec3<f32>) -> vec2<f32> {
    return refl_project_uv(world, u.view_proj);
}

fn thickness_m(base : f32, roughness : f32) -> f32 {
    return refl_thickness_m(base, roughness);
}

fn lit_scene_at(hit_uv : vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(t_scene);
    let px = texel_px(hit_uv, dims);
    return textureLoad(t_scene, px, 0).rgb;
}

fn lit_scene_blurred(hit_uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    if u.ssr_blur_enabled < 0.5 {
        return lit_scene_at(hit_uv);
    }
    let depth_dims = textureDimensions(t_depth);
    let ref_depth_m = depth_at(hit_uv, depth_dims);
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
            let px = texel_px(p, depth_dims);
            let tap_depth_m = textureLoad(t_depth, px, 0).r;
            if tap_depth_m <= 0.0001 {
                continue;
            }
            if abs(tap_depth_m - ref_depth_m) > depth_reject_m {
                continue;
            }
            acc += textureLoad(t_scene, px, 0).rgb;
            w_sum += 1.0;
        }
    }
    if w_sum < 1.0 {
        return lit_scene_at(hit_uv);
    }
    return acc / w_sum;
}

struct SsrTraceDbg {
    eligible     : bool,
    found        : bool,
    view_dir     : vec3<f32>,
    refl_dir     : vec3<f32>,
    hit_uv       : vec2<f32>,
    depth_delta  : f32,
    path_px      : f32,
    steps_frac   : f32,
    strength     : f32,
    roughness    : f32,
    march_dist_m : f32,
}

fn trace_ssr_debug(surf_uv : vec2<f32>) -> SsrTraceDbg {
    return trace_ssr_screen_debug(surf_uv);
}

/// Marcha screen-space idéntica a `ssr.wgsl` (producción).
fn trace_ssr_screen_debug(surf_uv : vec2<f32>) -> SsrTraceDbg {
    var out : SsrTraceDbg;
    out.eligible = false;
    out.found = false;
    out.hit_uv = vec2<f32>(-1.0);
    out.depth_delta = 0.0;
    out.path_px = 0.0;
    out.steps_frac = 0.0;
    out.strength = 0.0;
    out.roughness = 1.0;
    out.march_dist_m = 0.0;
    out.view_dir = vec3<f32>(0.0);
    out.refl_dir = vec3<f32>(0.0);

    let depth_dims = textureDimensions(t_depth);
    let view_depth_m = depth_at(surf_uv, depth_dims);
    if view_depth_m <= 0.0001 {
        return out;
    }

    let packed = textureSample(t_normal_roughness, s_linear, surf_uv);
    let n = decode_octahedral(packed.zw);
    let surf_px = texel_px(surf_uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;
    let dir_px = texel_px(surf_uv, textureDimensions(t_direct));
    let metallic = textureLoad(t_direct, dir_px, 0).a;
    let albedo = textureLoad(t_base_color, surf_px, 0).rgb;
    out.roughness = roughness;

    if roughness > u.max_roughness {
        return out;
    }

    let world_pos = world_pos_from_depth(surf_uv, view_depth_m);
    let view_dir = normalize(u.cam_pos.xyz - world_pos);
    let refl_dir = refl_fuzzy_mirror_dir(world_pos, u.cam_pos.xyz, n, roughness, surf_uv);
    out.view_dir = view_dir;
    out.refl_dir = refl_dir;

    let strength = refl_trace_strength(roughness, metallic, n, view_dir, albedo);
    out.strength = strength;
    if strength < 0.005 {
        return out;
    }
    out.eligible = true;

    if dot(refl_dir, n) <= 0.0 {
        return out;
    }

    let normal_bias = refl_normal_bias(u.step_size);
    let march_start = world_pos + n * normal_bias + refl_dir * u.step_size;
    let march_end = world_pos + n * normal_bias + refl_dir * u.max_distance_m;

    let uv_start = project_uv(march_start);
    let uv_end = project_uv(march_end);
    if uv_start.x < 0.0 || uv_end.x < 0.0 {
        return out;
    }

    let delta_uv = uv_end - uv_start;
    let px_len = length(delta_uv * u.resolution);
    let step_count = max(1u, min(u.max_steps, u32(ceil(px_len))));

    var dist_prev = u.step_size;
    var best_delta = 0.0;

    for (var i = 0u; i < step_count; i++) {
        out.steps_frac = (f32(i) + 1.0) / f32(max(step_count, 1u));
        let t = f32(i + 1u) / f32(step_count);
        let dist = mix(u.step_size, u.max_distance_m, t);
        let march_world = world_pos + n * normal_bias + refl_dir * dist;
        let sample_uv = project_uv(march_world);
        out.path_px = length((sample_uv - surf_uv) * u.resolution);

        if sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0 {
            let sample_px = texel_px(sample_uv, depth_dims);
            let sample_depth_m = textureLoad(t_depth, sample_px, 0).r;
            if sample_depth_m > 0.0001 {
                let ray_depth_m = view_depth_m_from_world(march_world);
                let diff = ray_depth_m - sample_depth_m;
                best_delta = max(best_delta, abs(diff));
                let thick = thickness_m(max(u.step_size * 1.5, 0.04), roughness);
                if diff > 0.0 && diff < thick {
                    var dist_lo = dist_prev;
                    var dist_hi = dist;
                    var hit_uv = vec2<f32>(-1.0);
                    for (var k = 0u; k < 4u; k++) {
                        let dist_mid = (dist_lo + dist_hi) * 0.5;
                        let mid_world = world_pos + n * normal_bias + refl_dir * dist_mid;
                        let uvm = project_uv(mid_world);
                        let sd_px = texel_px(uvm, depth_dims);
                        let sd_m = textureLoad(t_depth, sd_px, 0).r;
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
                    out.found = true;
                    out.hit_uv = hit_uv;
                    out.depth_delta = ray_depth_m - sample_depth_m;
                    out.march_dist_m = dist_hi;
                    return out;
                }
            }
        }
        dist_prev = dist;
    }

    out.depth_delta = best_delta;
    out.hit_uv = uv_end;
    return out;
}

fn view_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    let world = world_pos_from_depth(uv, view_depth_m);
    let view_h = u.view * vec4<f32>(world, 1.0);
    return view_h.xyz;
}

fn depth_at(uv : vec2<f32>, dims : vec2<u32>) -> f32 {
    let px = texel_px(uv, dims);
    return textureLoad(t_depth, px, 0).r;
}

fn compute_reflection_strength(uv : vec2<f32>) -> f32 {
    let depth_dims = textureDimensions(t_depth);
    let view_depth_m = depth_at(uv, depth_dims);
    if view_depth_m <= 0.0001 {
        return 0.0;
    }

    let surf_px = texel_px(uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;
    let dir_px = texel_px(uv, textureDimensions(t_direct));
    let metallic = textureLoad(t_direct, dir_px, 0).a;
    let albedo = textureLoad(t_base_color, surf_px, 0).rgb;

    if roughness > u.max_roughness {
        return 0.0;
    }

    let packed = textureSample(t_normal_roughness, s_linear, uv);
    let n = decode_octahedral(packed.zw);

    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let v = normalize(u.cam_pos.xyz - world_pos);
    let strength = refl_trace_strength(roughness, metallic, n, v, albedo);
    if strength < 0.005 {
        return 0.0;
    }
    return strength;
}

@fragment
fn fs_main(in : VsOut) -> @location(0) vec4<f32> {
    switch u.mode {
        case 1u: {
            let packed = textureSample(t_normal_roughness, s_linear, in.uv);
            let n = decode_octahedral(packed.zw);
            return vec4<f32>(n * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 2u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            let vis = clamp(d_m / 50.0, 0.0, 1.0);
            return vec4<f32>(vec3<f32>(vis), 1.0);
        }
        case 3u: {
            let r = textureSample(t_reflection, s_linear, in.uv);
            let hit = step(0.05, r.a);
            return vec4<f32>(vec3<f32>(hit), 1.0);
        }
        case 4u: {
            let strength = compute_reflection_strength(in.uv);
            return vec4<f32>(vec3<f32>(strength), 1.0);
        }
        case 5u: {
            return vec4<f32>(textureSample(t_scene, s_linear, in.uv).rgb, 1.0);
        }
        case 6u: {
            let r = textureSample(t_reflection, s_linear, in.uv);
            return vec4<f32>(r.rgb, 1.0);
        }
        case 7u: {
            let px = texel_px(in.uv, textureDimensions(t_surface));
            let roughness = textureLoad(t_surface, px, 0).g;
            return vec4<f32>(vec3<f32>(roughness), 1.0);
        }
        case 8u: {
            let px = texel_px(in.uv, textureDimensions(t_direct));
            let metallic = textureLoad(t_direct, px, 0).a;
            return vec4<f32>(vec3<f32>(metallic), 1.0);
        }
        case 9u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let wp = world_pos_from_depth(in.uv, d_m);
            let rel = wp - u.cam_pos.xyz;
            return vec4<f32>(fract(rel * 0.08 + vec3<f32>(0.5)), 1.0);
        }
        case 10u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let ndc_z_vk = refl_view_depth_to_ndc_z_vk(d_m, u.near_plane, u.far_plane);
            let ndc_z_gl = refl_vk_ndc_z_to_gl(ndc_z_vk);
            let ndc = vec3<f32>(uv_to_ndc_xy(in.uv), ndc_z_gl);
            return vec4<f32>(ndc * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 11u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let vp = view_pos_from_depth(in.uv, d_m);
            return vec4<f32>(fract(vp * 0.05 + vec3<f32>(0.5)), 1.0);
        }
        case 12u: {
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let wp = world_pos_from_depth(in.uv, d_m);
            let clip = u.view_proj * vec4<f32>(wp, 1.0);
            let ndc = clip.xyz / clip.w;
            let reproj_uv = ndc_xy_to_uv(ndc.xy);
            // RG = UV reproyectada; B = error |Δuv|×20 (negro = coherente).
            let err = length(reproj_uv - in.uv);
            return vec4<f32>(reproj_uv, clamp(err * 20.0, 0.0, 1.0), 1.0);
        }
        case 13u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.view_dir * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 14u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.refl_dir * 0.5 + vec3<f32>(0.5), 1.0);
        }
        case 15u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            // R = longitud del recorrido UV (px/200), G = progreso del march, B = hit.
            let path_vis = clamp(t.path_px / 200.0, 0.0, 1.0);
            let hit_vis = select(0.0, 1.0, t.found);
            return vec4<f32>(path_vis, t.steps_frac, hit_vis, 1.0);
        }
        case 16u: {
            let t = trace_ssr_debug(in.uv);
            if !t.eligible {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let vis = clamp(abs(t.depth_delta) * 10.0, 0.0, 1.0);
            return vec4<f32>(vec3<f32>(vis), 1.0);
        }
        case 17u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 18u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(lit_scene_at(t.hit_uv), 1.0);
        }
        case 19u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let spacing_px = refl_blur_spacing_px(t.roughness, t.march_dist_m);
            return vec4<f32>(lit_scene_blurred(t.hit_uv, spacing_px), 1.0);
        }
        case 20u: {
            let r = textureSample(t_reflection, s_linear, in.uv);
            return vec4<f32>(r.rgb, 1.0);
        }
        case 21u: {
            let px = texel_px(in.uv, textureDimensions(t_base_color));
            return vec4<f32>(textureLoad(t_base_color, px, 0).rgb, 1.0);
        }
        case 22u: {
            let strength = compute_reflection_strength(in.uv);
            return vec4<f32>(vec3<f32>(strength), 1.0);
        }
        case 23u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            // Deprecado: world/screen eran idénticos; alias de hit_uv (case 17).
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 24u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 25u: {
            let t = trace_ssr_debug(in.uv);
            if !t.found {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            return vec4<f32>(t.hit_uv, 0.0, 1.0);
        }
        case 26u: {
            let base = textureSample(t_scene, s_linear, in.uv);
            let refl = textureSample(t_reflection, s_linear, in.uv);
            let k = clamp(refl.a, 0.0, 1.0);
            let detail = max(refl.rgb - base.rgb, vec3<f32>(0.0));
            return vec4<f32>(base.rgb + detail * k, 1.0);
        }
        case 27u: {
            return vec4<f32>(textureSample(t_reflection, s_linear, in.uv).rgb, 1.0);
        }
        case 28u: {
            // Vista aproximada por posición world (sin probe_meta en este pass).
            let d_m = depth_at(in.uv, textureDimensions(t_depth));
            if d_m <= 0.0001 {
                return vec4<f32>(0.0, 0.0, 0.0, 1.0);
            }
            let wp = world_pos_from_depth(in.uv, d_m);
            let slot = f32(i32(fract(dot(wp, vec3<f32>(12.9898, 78.233, 45.164))) * 8.0));
            let hue = slot / 8.0;
            return vec4<f32>(fract(vec3<f32>(hue, hue * 0.7, hue * 0.35) + vec3<f32>(0.2)), 1.0);
        }
        default: {
            return textureSample(t_scene, s_linear, in.uv);
        }
    }
}
