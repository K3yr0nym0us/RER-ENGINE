//! SSR debug — mismo path que `ssr.wgsl` (G-buffer world_pos + marcha Bevy).

fn uv_to_ndc_xy(uv : vec2<f32>) -> vec2<f32> {
    return refl_uv_to_ndc_xy(uv);
}

fn ndc_xy_to_uv(ndc_xy : vec2<f32>) -> vec2<f32> {
    return refl_ndc_xy_to_uv(ndc_xy);
}

fn texel_px(uv : vec2<f32>, dims : vec2<u32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let fc = clamped * vec2<f32>(dims) - vec2<f32>(0.5);
    return vec2<i32>(fc);
}

fn world_pos_bilinear_at_uv(uv : vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(t_world_pos);
    let tex_size = vec2<f32>(dims);
    let coord = uv * tex_size - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coord));
    let frac = coord - vec2<f32>(base);

    let max_coord = vec2<i32>(i32(dims.x) - 1, i32(dims.y) - 1);
    let c00 = clamp(base, vec2<i32>(0), max_coord);
    let c10 = clamp(base + vec2<i32>(1, 0), vec2<i32>(0), max_coord);
    let c01 = clamp(base + vec2<i32>(0, 1), vec2<i32>(0), max_coord);
    let c11 = clamp(base + vec2<i32>(1, 1), vec2<i32>(0), max_coord);

    let p00 = textureLoad(t_world_pos, c00, 0).xyz;
    let p10 = textureLoad(t_world_pos, c10, 0).xyz;
    let p01 = textureLoad(t_world_pos, c01, 0).xyz;
    let p11 = textureLoad(t_world_pos, c11, 0).xyz;
    let p0 = mix(p00, p10, frac.x);
    let p1 = mix(p01, p11, frac.x);
    return mix(p0, p1, frac.y);
}

fn world_pos_at_uv(uv : vec2<f32>, depth_prepass : f32) -> vec3<f32> {
    let wp = world_pos_bilinear_at_uv(uv);
    if dot(wp, wp) > 1e-8 {
        return wp;
    }
    return world_pos_from_depth(uv, depth_prepass);
}

fn world_pos_from_depth(uv : vec2<f32>, depth_prepass : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, depth_prepass, u.inv_view_proj, u.near_plane, u.far_plane);
}

fn view_pos_from_depth(uv : vec2<f32>, depth_prepass : f32) -> vec3<f32> {
    let world = world_pos_from_depth(uv, depth_prepass);
    return (u.view * vec4<f32>(world, 1.0)).xyz;
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

fn depth_at(uv : vec2<f32>, dims : vec2<u32>) -> f32 {
    let px = texel_px(uv, dims);
    return textureLoad(t_depth, px, 0).r;
}

fn depth_nearest_at(uv : vec2<f32>) -> f32 {
    return depth_at(uv, textureDimensions(t_depth));
}

fn depth_bilinear_at(uv : vec2<f32>) -> f32 {
    let dims = textureDimensions(t_depth);
    let tex_size = vec2<f32>(dims);
    let coord = uv * tex_size - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coord));
    let frac = coord - vec2<f32>(base);
    let max_coord = vec2<i32>(i32(dims.x) - 1, i32(dims.y) - 1);
    let c00 = clamp(base, vec2<i32>(0), max_coord);
    let c10 = clamp(base + vec2<i32>(1, 0), vec2<i32>(0), max_coord);
    let c01 = clamp(base + vec2<i32>(0, 1), vec2<i32>(0), max_coord);
    let c11 = clamp(base + vec2<i32>(1, 1), vec2<i32>(0), max_coord);
    let d0 = mix(textureLoad(t_depth, c00, 0).r, textureLoad(t_depth, c10, 0).r, frac.x);
    let d1 = mix(textureLoad(t_depth, c01, 0).r, textureLoad(t_depth, c11, 0).r, frac.x);
    return mix(d0, d1, frac.y);
}

fn ssr_march_depth_nearest(uv : vec2<f32>, tex_size : vec2<f32>) -> f32 {
    _ = tex_size;
    return depth_nearest_at(uv);
}

fn ssr_march_depth_linear(uv : vec2<f32>, tex_size : vec2<f32>) -> f32 {
    _ = tex_size;
    return depth_bilinear_at(uv);
}

fn lit_scene_at(hit_uv : vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(t_scene);
    let px = texel_px(hit_uv, dims);
    return textureLoad(t_scene, px, 0).rgb;
}

/// Idéntico a `ssr_reflected_radiance` en `ssr.wgsl` (`t_lit_scene` → `t_scene` en debug).
fn ssr_reflected_radiance_at_hit(hit_uv : vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(t_scene, s_linear, hit_uv, 0.0).rgb;
}

/// |clip.z/w − prepass z| — moiré en corona si > ~0.01 (TAA / G-buffer vs reproyección).
fn ssr_proj_depth_delta_at(surf_uv : vec2<f32>) -> f32 {
    let depth_prepass = depth_at(surf_uv, textureDimensions(t_depth));
    if refl_depth_prepass_invalid(depth_prepass) {
        return -1.0;
    }
    let world_pos = world_pos_at_uv(surf_uv, depth_prepass);
    let proj_z = ssr_world_to_ndc(world_pos, u.view_proj).z;
    return abs(proj_z - depth_prepass);
}

fn lettier_debug_box_blur(uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    let texel = vec2<f32>(1.0) / u.gb_resolution;
    var acc = vec3<f32>(0.0);
    for (var oy = -3; oy <= 3; oy++) {
        for (var ox = -3; ox <= 3; ox++) {
            let p = uv + vec2<f32>(f32(ox), f32(oy)) * spacing_px * texel;
            acc += lit_scene_at(p);
        }
    }
    return acc / 49.0;
}

struct SsrMarchInputs {
    eligible    : bool,
    exit_reason : u32,
    /// Idéntico al primer argumento de `ssr_evaluate_bevy` en `ssr.wgsl`.
    R_world     : vec3<f32>,
    start_cs    : vec3<f32>,
}

/// Misma cadena que `ssr.wgsl` antes de `ssr_evaluate_bevy` (P, N, R, start_cs).
fn ssr_march_inputs_at(surf_uv : vec2<f32>) -> SsrMarchInputs {
    var out : SsrMarchInputs;
    out.eligible = false;
    out.exit_reason = 0u;
    out.R_world = vec3<f32>(0.0);
    out.start_cs = vec3<f32>(0.0);

    let depth_prepass = depth_at(surf_uv, textureDimensions(t_depth));
    if refl_depth_prepass_invalid(depth_prepass) {
        out.exit_reason = 1u;
        return out;
    }

    let n_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(surf_uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let surf_px = texel_px(surf_uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;

    if roughness > u.max_roughness {
        out.exit_reason = 2u;
        return out;
    }

    let world_pos = world_pos_at_uv(surf_uv, depth_prepass);
    out.R_world = ssr_bevy_reflection_world(u.cam_pos.xyz, world_pos, n_world);
    out.start_cs = ssr_ray_start_cs(world_pos, depth_prepass, u.view_proj);
    out.eligible = true;
    return out;
}

struct SsrTraceDbg {
    eligible      : bool,
    found         : bool,
    march_hit     : bool,
    self_rejected : bool,
    exit_reason   : u32,
    view_dir      : vec3<f32>,
    refl_dir      : vec3<f32>,
    hit_uv        : vec2<f32>,
    depth_delta   : f32,
    path_px       : f32,
    steps_frac    : f32,
    strength      : f32,
    roughness     : f32,
    march_dist_m  : f32,
}

fn trace_ssr_debug(surf_uv : vec2<f32>) -> SsrTraceDbg {
    var out : SsrTraceDbg;
    out.eligible = false;
    out.found = false;
    out.march_hit = false;
    out.self_rejected = false;
    out.exit_reason = 0u;
    out.hit_uv = vec2<f32>(-1.0);
    out.depth_delta = 0.0;
    out.path_px = 0.0;
    out.steps_frac = 0.0;
    out.strength = 0.0;
    out.roughness = 1.0;
    out.march_dist_m = 0.0;
    out.view_dir = vec3<f32>(0.0);
    out.refl_dir = vec3<f32>(0.0);

    let march_in = ssr_march_inputs_at(surf_uv);
    if !march_in.eligible {
        out.exit_reason = march_in.exit_reason;
        return out;
    }

    let n_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(surf_uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let surf_px = texel_px(surf_uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;
    out.roughness = roughness;

    let dir_px = texel_px(surf_uv, textureDimensions(t_direct));
    let metallic = textureLoad(t_direct, dir_px, 0).a;
    let albedo = textureLoad(t_base_color, surf_px, 0).rgb;

    let world_pos = world_pos_at_uv(surf_uv, depth_at(surf_uv, textureDimensions(t_depth)));
    out.view_dir = ssr_bevy_view_dir_world(u.cam_pos.xyz, world_pos);
    out.refl_dir = march_in.R_world;
    out.strength = refl_trace_strength(roughness, metallic, n_world, out.view_dir, albedo, 0.0);
    out.eligible = true;

    let hit = ssr_evaluate_bevy(
        march_in.R_world,
        march_in.start_cs,
        1.0,
        u.view_proj,
        max(u.coarse_max_iters, 2u),
        u.binary_steps,
        u.thickness_m,
        u.near_plane,
    );

    if !hit.found {
        out.exit_reason = 4u;
        return out;
    }

    out.march_hit = true;
    out.hit_uv = hit.hit_uv;
    out.path_px = length((hit.hit_uv - surf_uv) * u.gb_resolution);

    out.found = true;
    out.exit_reason = 9u;
    return out;
}

fn compute_reflection_strength(uv : vec2<f32>) -> f32 {
    let surf_px = texel_px(uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;
    if roughness > u.max_roughness {
        return 0.0;
    }
    let depth_prepass = depth_at(uv, textureDimensions(t_depth));
    if refl_depth_prepass_invalid(depth_prepass) {
        return 0.0;
    }
    let n_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let dir_px = texel_px(uv, textureDimensions(t_direct));
    let metallic = textureLoad(t_direct, dir_px, 0).a;
    let albedo = textureLoad(t_base_color, surf_px, 0).rgb;
    let world_pos = world_pos_at_uv(uv, depth_prepass);
    let V_world = ssr_bevy_view_dir_world(u.cam_pos.xyz, world_pos);
    return refl_trace_strength(roughness, metallic, n_world, V_world, albedo, 0.0);
}
