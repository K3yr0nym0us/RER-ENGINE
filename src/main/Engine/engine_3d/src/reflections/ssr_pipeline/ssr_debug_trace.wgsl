//! SSR debug trace — extraído de `debug.wgsl`.
//! Contraparte de `ssr.wgsl` (misma marcha Lettier) para visualización debug.

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

fn view_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    let world = world_pos_from_depth(uv, view_depth_m);
    let view_h = u.view * vec4<f32>(world, 1.0);
    return view_h.xyz;
}

fn depth_at(uv : vec2<f32>, dims : vec2<u32>) -> f32 {
    let px = texel_px(uv, dims);
    return textureLoad(t_depth, px, 0).r;
}

fn lit_scene_at(hit_uv : vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(t_scene);
    let px = texel_px(hit_uv, dims);
    return textureLoad(t_scene, px, 0).rgb;
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

struct SsrTraceDbg {
    eligible     : bool,
    found        : bool,
    exit_reason  : u32,
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

struct DbgSsrRay {
    view_dir        : vec3<f32>,
    reflection_dir  : vec3<f32>,
    ray_start       : vec3<f32>,
    ray_end         : vec3<f32>,
    ray_end_depth   : f32,
}

fn dbg_build_ssr_ray(position_view : vec3<f32>, normal_view : vec3<f32>) -> DbgSsrRay {
    var ray : DbgSsrRay;
    ray.view_dir = normalize(-position_view);
    ray.reflection_dir = lettier_reflection_dir(ray.view_dir, normal_view);
    let view_depth_m = lettier_view_depth_from_view_pos(position_view);
    let bias_m = min(ssr_view_normal_bias_m(view_depth_m) * 2.0, 0.1);
    ray.ray_start = position_view + normalize(normal_view) * bias_m;
    ray.ray_end = ray.ray_start + ray.reflection_dir * u.max_distance_m;
    ray.ray_end_depth = lettier_view_depth_from_view_pos(ray.ray_end);
    return ray;
}

fn trace_ssr_debug(surf_uv : vec2<f32>) -> SsrTraceDbg {
    return trace_ssr_lettier_debug(surf_uv);
}

fn trace_ssr_lettier_debug(surf_uv : vec2<f32>) -> SsrTraceDbg {
    var out : SsrTraceDbg;
    out.eligible = false;
    out.found = false;
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

    let depth_dims = textureDimensions(t_depth);
    let view_depth_m = depth_at(surf_uv, depth_dims);
    if view_depth_m <= 1e-4 {
        return out;
    }

    let n_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(surf_uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
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
    let position_view = (u.view * vec4<f32>(world_pos, 1.0)).xyz;
    let normal_view = normalize((u.view * vec4<f32>(n_world, 0.0)).xyz);

    out.view_dir = normalize(u.cam_pos.xyz - world_pos);
    var ray = dbg_build_ssr_ray(position_view, normal_view);
    out.refl_dir = normalize((u.inv_view * vec4<f32>(ray.reflection_dir, 0.0)).xyz);

    let V_view = normalize(-position_view);
    let NdotV = max(dot(normal_view, V_view), 0.0);
    let specular_amount = lettier_specular_amount(metallic, roughness, albedo, NdotV);
    out.strength = specular_amount;
    if specular_amount <= 0.0 {
        return out;
    }
    out.eligible = true;

    let march_start_depth = lettier_view_depth_from_view_pos(ray.ray_start);
    let tex_size = u.gb_resolution;
    var start_frag = lettier_view_to_frag_px(ray.ray_start, u.inv_view, u.view_proj, tex_size);
    var end_frag = lettier_view_to_frag_px(ray.ray_end, u.inv_view, u.view_proj, tex_size);
    if start_frag.x < 0.0 {
        out.exit_reason = 1u;
        return out;
    }
    if end_frag.x < 0.0 {
        let dir = ray.ray_end - ray.ray_start;
        let t_near = (-u.near_plane - ray.ray_start.z) / max(dir.z, 1e-6);
        if t_near > 0.0 && t_near < 1.0 {
            let clamped_end = ray.ray_start + dir * t_near * 0.999;
            ray.ray_end = clamped_end;
            end_frag = lettier_view_to_frag_px(ray.ray_end, u.inv_view, u.view_proj, tex_size);
        }
        if end_frag.x < 0.0 {
            out.exit_reason = 1u;
            return out;
        }
    }

    let delta_x = end_frag.x - start_frag.x;
    let delta_y = end_frag.y - start_frag.y;
    let use_x = select(0.0, 1.0, abs(delta_x) >= abs(delta_y));
    let delta = mix(abs(delta_y), abs(delta_x), use_x) * clamp(u.coarse_resolution, 0.0, 1.0);
    let line_span = max(delta, 1.0);
    let coarse_iters = min(u32(line_span), max(u.coarse_max_iters, 1u));
    let increment = vec2<f32>(delta_x, delta_y) / f32(coarse_iters);

    var frag = start_frag;
    var search0 = 0.0;
    var search1 = 0.0;
    var hit0 = 0u;
    var hit1 = 0u;
    let thickness = max(u.thickness_m, ssr_hit_thickness_m(march_start_depth, roughness, ray.reflection_dir));

    for (var i = 0u; i < coarse_iters; i++) {
        frag += increment;
        let sample_uv = frag / tex_size;
        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            out.exit_reason = 1u;
            break;
        }

        let scene_depth_m = depth_at(sample_uv, depth_dims);
        if scene_depth_m <= 1e-4 {
            continue;
        }

        search1 = clamp(lettier_line_search_t(frag, start_frag, delta_x, delta_y, use_x), 0.0, 1.0);
        let ray_depth_m = lettier_perspective_depth(march_start_depth, ray.ray_end_depth, search1);
        let depth_delta = ray_depth_m - scene_depth_m;

        if depth_delta > 0.0 && depth_delta < thickness {
            hit0 = 1u;
            break;
        }
        search0 = search1;
    }

    if hit0 == 0u {
        out.exit_reason = 2u;
        return out;
    }

    var refine_l = search0;
    var refine_r = search1;
    let refine_iters = u.binary_steps * hit0;
    for (var j = 0u; j < refine_iters; j++) {
        let test_t = (refine_l + refine_r) * 0.5;
        frag = mix(start_frag, end_frag, test_t);
        let sample_uv = frag / tex_size;
        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            break;
        }

        let scene_depth_m = depth_at(sample_uv, depth_dims);
        if scene_depth_m <= 1e-4 {
            break;
        }

        let ray_depth_m = lettier_perspective_depth(march_start_depth, ray.ray_end_depth, test_t);
        let depth_delta = ray_depth_m - scene_depth_m;

        if depth_delta > 0.0 && depth_delta < thickness {
            hit1 = 1u;
            refine_r = test_t;
        } else {
            refine_l = test_t;
        }
    }

    search1 = (refine_l + refine_r) * 0.5;
    frag = mix(start_frag, end_frag, search1);
    let hit_uv = frag / tex_size;

    let surf_depth_m = lettier_view_depth_from_view_pos(position_view);
    let scene_depth_m = depth_at(hit_uv, depth_dims);
    if ssr_reject_self_hit(surf_depth_m, scene_depth_m) {
        return out;
    }

    if scene_depth_m <= 1e-4 {
        return out;
    }

    let hit_px_dist = length((hit_uv - surf_uv) * tex_size);
    if hit_px_dist < 1.5 {
        return out;
    }

    let ray_depth_m = lettier_perspective_depth(march_start_depth, ray.ray_end_depth, search1);
    let depth_delta = ray_depth_m - scene_depth_m;

    let hit_world = world_pos_from_depth(hit_uv, scene_depth_m);
    let position_to_view = (u.view * vec4<f32>(hit_world, 1.0)).xyz;
    let vis_fade_m = min(u.max_distance_m, 50.0);
    let vis = lettier_ssr_visibility(
        hit1 == 1u,
        hit0 == 1u,
        ray.view_dir,
        ray.reflection_dir,
        depth_delta,
        thickness,
        position_view,
        position_to_view,
        vis_fade_m,
        hit_uv,
    );

    out.steps_frac = search1;
    out.path_px = length((hit_uv - surf_uv) * u.resolution);
    out.depth_delta = depth_delta;
    out.march_dist_m = length(position_to_view - ray.ray_start);

    if vis <= 0.0 {
        return out;
    }

    out.found = true;
    out.exit_reason = 3u;
    out.hit_uv = hit_uv;
    return out;
}

fn compute_reflection_strength(uv : vec2<f32>) -> f32 {
    let depth_dims = textureDimensions(t_depth);
    let view_depth_m = depth_at(uv, depth_dims);
    if view_depth_m <= 1e-4 {
        return 0.0;
    }

    let surf_px = texel_px(uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;
    if roughness > u.max_roughness {
        return 0.0;
    }

    let dir_px = texel_px(uv, textureDimensions(t_direct));
    let metallic = textureLoad(t_direct, dir_px, 0).a;
    let albedo = textureLoad(t_base_color, surf_px, 0).rgb;

    let n_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let V_world = normalize(u.cam_pos.xyz - world_pos);
    let NdotV = max(dot(n_world, V_world), 0.0);

    return lettier_specular_amount(metallic, roughness, albedo, NdotV);
}
