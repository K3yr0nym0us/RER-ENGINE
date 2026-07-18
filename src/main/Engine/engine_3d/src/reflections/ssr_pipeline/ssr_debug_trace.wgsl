//! SSR debug — mismo path que `ssr.wgsl` (G-buffer world_pos + marcha SSR).

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

fn lit_scene_at(reflection_hit_uv : vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(t_scene);
    let px = texel_px(reflection_hit_uv, dims);
    return textureLoad(t_scene, px, 0).rgb;
}

/// Idéntico a `ssr_reflected_radiance` en `ssr.wgsl` (`t_lit_scene` → `t_scene` en debug).
fn ssr_reflected_radiance_at_hit(reflection_hit_uv : vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(t_scene, s_linear, reflection_hit_uv, 0.0).rgb;
}

/// |clip.z/w − prepass z| — moiré en corona si > ~0.01 (TAA / G-buffer vs reproyección).
fn ssr_proj_depth_delta_at(surface_uv : vec2<f32>) -> f32 {
    let depth_prepass = depth_at(surface_uv, textureDimensions(t_depth));
    if refl_depth_prepass_invalid(depth_prepass) {
        return -1.0;
    }
    let surface_pos_world = world_pos_at_uv(surface_uv, depth_prepass);
    let projected_ndc_z = ssr_world_to_ndc(surface_pos_world, u.view_proj).z;
    return abs(projected_ndc_z - depth_prepass);
}

fn ssr_debug_box_blur(uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
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
    eligible             : bool,
    exit_reason          : u32,
    /// Idéntico al primer argumento de `ssr_evaluate_trace` en `ssr.wgsl`.
    reflection_dir_world : vec3<f32>,
    ray_origin_ndc       : vec3<f32>,
}

/// Misma cadena que `ssr.wgsl` antes de `ssr_evaluate_trace`.
fn ssr_march_inputs_at(surface_uv : vec2<f32>) -> SsrMarchInputs {
    var out : SsrMarchInputs;
    out.eligible = false;
    out.exit_reason = 0u;
    out.reflection_dir_world = vec3<f32>(0.0);
    out.ray_origin_ndc = vec3<f32>(0.0);

    let depth_prepass = depth_at(surface_uv, textureDimensions(t_depth));
    if refl_depth_prepass_invalid(depth_prepass) {
        out.exit_reason = 1u;
        return out;
    }

    let surface_normal_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(surface_uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let surface_texel = texel_px(surface_uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surface_texel, 0).g;

    if roughness > u.max_roughness {
        out.exit_reason = 2u;
        return out;
    }

    let surface_pos_world = world_pos_at_uv(surface_uv, depth_prepass);
    out.reflection_dir_world = ssr_reflection_world(
        u.cam_pos.xyz,
        surface_pos_world,
        surface_normal_world,
    );
    out.ray_origin_ndc = ssr_ray_start_cs(surface_pos_world, depth_prepass, u.view_proj);
    out.eligible = true;
    return out;
}

struct SsrRayVis {
    eligible           : bool,
    found              : bool,
    surface_uv         : vec2<f32>,
    reflection_target_uv : vec2<f32>,
}

fn ssr_overlay_line_dist_px(uv : vec2<f32>, a : vec2<f32>, b : vec2<f32>, res : vec2<f32>) -> f32 {
    let p = uv * res;
    let p0 = a * res;
    let p1 = b * res;
    let ab = p1 - p0;
    let len2 = max(dot(ab, ab), 1e-6);
    let t = clamp(dot(p - p0, ab) / len2, 0.0, 1.0);
    let closest = p0 + ab * t;
    return length(p - closest);
}

/// Overlay de rayos: misma lógica que el SSR final (`ssr_march_inputs_at` + `ssr_evaluate_trace`).
fn ssr_ray_vis_at(surface_uv : vec2<f32>) -> SsrRayVis {
    var out : SsrRayVis;
    out.eligible = false;
    out.found = false;
    out.surface_uv = surface_uv;
    out.reflection_target_uv = surface_uv;

    let march_in = ssr_march_inputs_at(surface_uv);
    if !march_in.eligible {
        return out;
    }

    let march_hit = ssr_evaluate_trace(
        march_in.reflection_dir_world,
        march_in.ray_origin_ndc,
        1.0,
        u.view_proj,
        max(u.coarse_max_iters, 2u),
        u.binary_steps,
        u.thickness_m,
        u.near_plane,
    );

    out.eligible = true;
    out.found = march_hit.found;
    out.reflection_target_uv = select(
        march_hit.ray_march_end_uv,
        march_hit.reflection_hit_uv,
        march_hit.found,
    );
    return out;
}

struct SsrTraceDbg {
    eligible             : bool,
    found                : bool,
    march_hit            : bool,
    self_rejected        : bool,
    exit_reason          : u32,
    view_dir_world       : vec3<f32>,
    reflection_dir_world : vec3<f32>,
    reflection_hit_uv    : vec2<f32>,
    depth_delta          : f32,
    path_px              : f32,
    steps_frac           : f32,
    strength             : f32,
    roughness            : f32,
    march_dist_m         : f32,
}

fn trace_ssr_debug(surface_uv : vec2<f32>) -> SsrTraceDbg {
    var out : SsrTraceDbg;
    out.eligible = false;
    out.found = false;
    out.march_hit = false;
    out.self_rejected = false;
    out.exit_reason = 0u;
    out.reflection_hit_uv = vec2<f32>(-1.0);
    out.depth_delta = 0.0;
    out.path_px = 0.0;
    out.steps_frac = 0.0;
    out.strength = 0.0;
    out.roughness = 1.0;
    out.march_dist_m = 0.0;
    out.view_dir_world = vec3<f32>(0.0);
    out.reflection_dir_world = vec3<f32>(0.0);

    let march_in = ssr_march_inputs_at(surface_uv);
    if !march_in.eligible {
        out.exit_reason = march_in.exit_reason;
        return out;
    }

    let surface_normal_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(surface_uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let surface_texel = texel_px(surface_uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surface_texel, 0).g;
    out.roughness = roughness;

    let direct_texel = texel_px(surface_uv, textureDimensions(t_direct));
    let metallic = textureLoad(t_direct, direct_texel, 0).a;
    let albedo = textureLoad(t_base_color, surface_texel, 0).rgb;

    let surface_pos_world = world_pos_at_uv(
        surface_uv,
        depth_at(surface_uv, textureDimensions(t_depth)),
    );
    out.view_dir_world = ssr_view_dir_world(u.cam_pos.xyz, surface_pos_world);
    out.reflection_dir_world = march_in.reflection_dir_world;
    out.strength = refl_trace_strength(
        roughness,
        metallic,
        surface_normal_world,
        out.view_dir_world,
        albedo,
        0.0,
    );
    out.eligible = true;

    let march_hit = ssr_evaluate_trace(
        march_in.reflection_dir_world,
        march_in.ray_origin_ndc,
        1.0,
        u.view_proj,
        max(u.coarse_max_iters, 2u),
        u.binary_steps,
        u.thickness_m,
        u.near_plane,
    );

    if !march_hit.found {
        out.exit_reason = 4u;
        return out;
    }

    out.march_hit = true;
    out.reflection_hit_uv = march_hit.reflection_hit_uv;
    out.path_px = length((march_hit.reflection_hit_uv - surface_uv) * u.gb_resolution);

    out.found = true;
    out.exit_reason = 9u;
    return out;
}

fn compute_reflection_strength(uv : vec2<f32>) -> f32 {
    let surface_texel = texel_px(uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surface_texel, 0).g;
    if roughness > u.max_roughness {
        return 0.0;
    }
    let depth_prepass = depth_at(uv, textureDimensions(t_depth));
    if refl_depth_prepass_invalid(depth_prepass) {
        return 0.0;
    }
    let surface_normal_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let direct_texel = texel_px(uv, textureDimensions(t_direct));
    let metallic = textureLoad(t_direct, direct_texel, 0).a;
    let albedo = textureLoad(t_base_color, surface_texel, 0).rgb;
    let surface_pos_world = world_pos_at_uv(uv, depth_prepass);
    let view_dir_world = ssr_view_dir_world(u.cam_pos.xyz, surface_pos_world);
    return refl_trace_strength(
        roughness,
        metallic,
        surface_normal_world,
        view_dir_world,
        albedo,
        0.0,
    );
}
