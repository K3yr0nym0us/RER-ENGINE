// SSR raymarch — profundidad prepass = GL NDC z.

struct SsrHybridRootFinder {
    linear_steps          : u32,
    bisection_steps       : u32,
    use_secant            : bool,
    linear_march_exponent : f32,
    jitter                : f32,
    min_t                 : f32,
    max_t                 : f32,
}

fn ssr_hybrid_root_finder_new(linear_steps : u32) -> SsrHybridRootFinder {
    var res : SsrHybridRootFinder;
    res.linear_steps = linear_steps;
    res.bisection_steps = 0u;
    res.use_secant = false;
    res.linear_march_exponent = 1.0;
    res.jitter = 1.0;
    res.min_t = 0.0;
    res.max_t = 1.0;
    return res;
}

struct SsrDistanceWithPenetration {
    distance    : f32,
    valid       : bool,
    penetration : f32,
}

struct SsrDepthRaymarchDistanceFn {
    depth_tex_size    : vec2<f32>,
    march_behind      : bool,
    depth_thickness   : f32,
    sloppy_march      : bool,
}

fn ssr_depth_raymarch_distance_evaluate(
    distance_fn : ptr<function, SsrDepthRaymarchDistanceFn>,
    ray_point_cs : vec3<f32>,
) -> SsrDistanceWithPenetration {
    let interp_uv = refl_ndc_xy_to_uv(ray_point_cs.xy);
    let ray_march_z = ssr_gl_ndc_z_to_march_z(ray_point_cs.z);
    let ray_depth = 1.0 / max(ray_march_z, 1e-6);

    let depth_linear_gl = ssr_march_depth_linear(interp_uv, (*distance_fn).depth_tex_size);
    let depth_nearest_gl = ssr_march_depth_nearest(interp_uv, (*distance_fn).depth_tex_size);
    let linear_depth = 1.0 / max(ssr_gl_ndc_z_to_march_z(depth_linear_gl), 1e-6);
    let unfiltered_depth = 1.0 / max(ssr_gl_ndc_z_to_march_z(depth_nearest_gl), 1e-6);

    var max_depth : f32;
    var min_depth : f32;
    if (*distance_fn).sloppy_march {
        max_depth = unfiltered_depth;
        min_depth = unfiltered_depth;
    } else {
        max_depth = max(linear_depth, unfiltered_depth);
        min_depth = min(linear_depth, unfiltered_depth);
    }

    let bias = 0.000002;
    var res : SsrDistanceWithPenetration;
    res.distance = max_depth * (1.0 + bias) - ray_depth;
    res.penetration = ray_depth - min_depth;
    if (*distance_fn).march_behind {
        res.valid = res.penetration < (*distance_fn).depth_thickness;
    } else {
        res.valid = true;
    }
    return res;
}

fn ssr_hybrid_root_finder_find_root(
    root_finder : ptr<function, SsrHybridRootFinder>,
    start : vec3<f32>,
    end : vec3<f32>,
    distance_fn : ptr<function, SsrDepthRaymarchDistanceFn>,
    hit_t : ptr<function, f32>,
    miss_t : ptr<function, f32>,
    hit_d : ptr<function, SsrDistanceWithPenetration>,
) -> bool {
    let dir = end - start;
    var min_t = (*root_finder).min_t;
    var max_t = (*root_finder).max_t;
    var min_d = SsrDistanceWithPenetration(0.0, false, 0.0);
    var max_d = SsrDistanceWithPenetration(0.0, false, 0.0);
    var intersected = false;

    if (*root_finder).linear_steps > 0u {
        let candidate_t = mix(
            min_t,
            max_t,
            pow(
                (*root_finder).jitter / f32((*root_finder).linear_steps),
                (*root_finder).linear_march_exponent,
            ),
        );
        let candidate = start + dir * candidate_t;
        let candidate_d = ssr_depth_raymarch_distance_evaluate(distance_fn, candidate);
        intersected = candidate_d.distance < 0.0 && candidate_d.valid;

        if intersected {
            max_t = candidate_t;
            max_d = candidate_d;
        } else {
            min_t = candidate_t;
            min_d = candidate_d;
            for (var step = 1u; step < (*root_finder).linear_steps; step += 1u) {
                let t = mix(
                    (*root_finder).min_t,
                    (*root_finder).max_t,
                    pow(
                        (f32(step) + (*root_finder).jitter) / f32((*root_finder).linear_steps),
                        (*root_finder).linear_march_exponent,
                    ),
                );
                let c = start + dir * t;
                let c_d = ssr_depth_raymarch_distance_evaluate(distance_fn, c);
                intersected = c_d.distance < 0.0 && c_d.valid;
                if intersected {
                    max_t = t;
                    max_d = c_d;
                    break;
                }
                min_t = t;
                min_d = c_d;
            }
        }
    }

    *miss_t = min_t;
    *hit_t = min_t;

    if intersected {
        for (var step = 0u; step < (*root_finder).bisection_steps; step += 1u) {
            let mid_t = (min_t + max_t) * 0.5;
            let candidate = start + dir * mid_t;
            let candidate_d = ssr_depth_raymarch_distance_evaluate(distance_fn, candidate);
            if candidate_d.distance < 0.0 && candidate_d.valid {
                max_t = mid_t;
                max_d = candidate_d;
            } else {
                min_t = mid_t;
                min_d = candidate_d;
            }
        }

        if (*root_finder).use_secant {
            let total_d = min_d.distance + -max_d.distance;
            let mid_t = mix(min_t, max_t, min_d.distance / total_d);
            let candidate = start + dir * mid_t;
            let candidate_d = ssr_depth_raymarch_distance_evaluate(distance_fn, candidate);
            if abs(candidate_d.distance) < min_d.distance * 0.9 && candidate_d.valid {
                *hit_t = mid_t;
                *hit_d = candidate_d;
            } else {
                *hit_t = max_t;
                *hit_d = max_d;
            }
            return true;
        }
        *hit_t = max_t;
        *hit_d = max_d;
        return true;
    }

    *hit_t = min_t;
    return false;
}

struct SsrDepthRayMarchResult {
    hit                  : bool,
    hit_t                : f32,
    hit_uv               : vec2<f32>,
    hit_penetration      : f32,
    hit_penetration_frac : f32,
}

struct SsrDepthRayMarch {
    linear_steps              : u32,
    linear_march_exponent     : f32,
    bisection_steps           : u32,
    use_secant                : bool,
    jitter                    : f32,
    ray_start_cs              : vec3<f32>,
    ray_end_cs                : vec3<f32>,
    march_behind_surfaces     : bool,
    use_sloppy_march          : bool,
    depth_thickness_linear_z  : f32,
    depth_tex_size            : vec2<f32>,
    near_plane                : f32,
}

fn ssr_depth_ray_march_new(depth_tex_size : vec2<f32>, near_plane : f32) -> SsrDepthRayMarch {
    var res : SsrDepthRayMarch;
    res.jitter = 1.0;
    res.linear_steps = 4u;
    res.bisection_steps = 0u;
    res.linear_march_exponent = 1.0;
    res.depth_tex_size = depth_tex_size;
    res.near_plane = near_plane;
    res.depth_thickness_linear_z = 1.0;
    res.march_behind_surfaces = false;
    res.use_sloppy_march = false;
    res.use_secant = false;
    return res;
}

fn ssr_depth_ray_march_from_cs(raymarch : ptr<function, SsrDepthRayMarch>, start_cs : vec3<f32>) {
    (*raymarch).ray_start_cs = start_cs;
}

fn ssr_depth_ray_march_to_cs_dir_impl(
    raymarch : ptr<function, SsrDepthRayMarch>,
    dir_cs : vec4<f32>,
    infinite : bool,
) {
    var end_cs = vec4<f32>((*raymarch).ray_start_cs, 1.0) + dir_cs;
    end_cs = end_cs / (select(-1.0, 1.0, end_cs.w >= 0.0) * max(abs(end_cs.w), 1e-10));

    var delta_cs = end_cs.xyz - (*raymarch).ray_start_cs;
    // Near clip solo XY; Z en GL NDC [-1,1] (no WebGPU 0..1).
    let near_edge = select(
        vec3<f32>(-1.0, -1.0, -1.0),
        vec3<f32>(1.0, 1.0, 1.0),
        delta_cs < vec3<f32>(0.0),
    );
    let dist_near = (near_edge - (*raymarch).ray_start_cs) / delta_cs;
    let max_dist_near = max(dist_near.x, dist_near.y);
    (*raymarch).ray_start_cs += delta_cs * max(0.0, max_dist_near);

    delta_cs = end_cs.xyz - (*raymarch).ray_start_cs;
    let far_edge = select(
        vec3<f32>(-1.0, -1.0, -1.0),
        vec3<f32>(1.0, 1.0, 1.0),
        delta_cs >= vec3<f32>(0.0),
    );
    let dist_far = (far_edge - (*raymarch).ray_start_cs) / delta_cs;
    let min_dist_far = min(min(dist_far.x, dist_far.y), dist_far.z);
    if infinite {
        delta_cs *= min_dist_far;
    } else {
        delta_cs *= min(min_dist_far, 1.0);
    }
    (*raymarch).ray_end_cs = (*raymarch).ray_start_cs + delta_cs;
}

fn ssr_depth_ray_march_to_ws_dir(raymarch : ptr<function, SsrDepthRayMarch>, world_dir : vec3<f32>, view_proj : mat4x4<f32>) {
    let dir_cs = ssr_direction_world_to_clip(world_dir, view_proj);
    ssr_depth_ray_march_to_cs_dir_impl(raymarch, dir_cs, true);
}

fn ssr_depth_ray_march_march(raymarch : ptr<function, SsrDepthRayMarch>) -> SsrDepthRayMarchResult {
    var res = SsrDepthRayMarchResult(false, 0.0, vec2<f32>(0.0), 0.0, 0.0);

    let ray_start_uv = refl_ndc_xy_to_uv((*raymarch).ray_start_cs.xy);
    let ray_end_uv = refl_ndc_xy_to_uv((*raymarch).ray_end_cs.xy);
    let ray_len_px = (ray_end_uv - ray_start_uv) * (*raymarch).depth_tex_size;

    let step_count = max(
        2,
        min(i32((*raymarch).linear_steps), i32(floor(length(ray_len_px)))),
    );

    let linear_z_to_scaled_linear_z = 1.0 / (*raymarch).near_plane;
    let depth_thickness = (*raymarch).depth_thickness_linear_z * linear_z_to_scaled_linear_z;

    var distance_fn : SsrDepthRaymarchDistanceFn;
    distance_fn.depth_tex_size = (*raymarch).depth_tex_size;
    distance_fn.march_behind = (*raymarch).march_behind_surfaces;
    distance_fn.depth_thickness = depth_thickness;
    distance_fn.sloppy_march = (*raymarch).use_sloppy_march;

    var hit = SsrDistanceWithPenetration(0.0, false, 0.0);
    var hit_t = 0.0;
    var miss_t = 0.0;
    var root_finder = ssr_hybrid_root_finder_new(u32(step_count));
    root_finder.bisection_steps = (*raymarch).bisection_steps;
    root_finder.use_secant = (*raymarch).use_secant;
    root_finder.linear_march_exponent = (*raymarch).linear_march_exponent;
    root_finder.jitter = (*raymarch).jitter;

    let intersected = ssr_hybrid_root_finder_find_root(
        &root_finder,
        (*raymarch).ray_start_cs,
        (*raymarch).ray_end_cs,
        &distance_fn,
        &hit_t,
        &miss_t,
        &hit,
    );

    res.hit_t = hit_t;
    if intersected && hit.penetration < depth_thickness && hit.distance < depth_thickness {
        res.hit = true;
        res.hit_uv = mix(ray_start_uv, ray_end_uv, res.hit_t);
        res.hit_penetration = hit.penetration / linear_z_to_scaled_linear_z;
        res.hit_penetration_frac = hit.penetration / depth_thickness;
        return res;
    }

    res.hit_t = miss_t;
    res.hit_uv = mix(ray_start_uv, ray_end_uv, res.hit_t);
    return res;
}

struct SsrMarchHit {
    found             : bool,
    reflection_hit_uv : vec2<f32>,
    /// Extremo del segmento de marcha (solo overlay debug; no altera el hit).
    ray_march_end_uv  : vec2<f32>,
}

/// Evalúa marcha SSR. `ray_origin_ndc` = NDC xyz del píxel (xy desde UV, z = prepass GL NDC).
fn ssr_evaluate_trace(
    reflection_dir_world : vec3<f32>,
    ray_origin_ndc       : vec3<f32>,
    jitter               : f32,
    view_proj            : mat4x4<f32>,
    linear_steps         : u32,
    bisection_steps      : u32,
    thickness_linear_z   : f32,
    near_plane           : f32,
) -> SsrMarchHit {
    var out : SsrMarchHit;
    out.found = false;
    out.reflection_hit_uv = vec2<f32>(-1.0);
    out.ray_march_end_uv = refl_ndc_xy_to_uv(ray_origin_ndc.xy);

    let depth_size = vec2<f32>(textureDimensions(t_depth));
    var raymarch = ssr_depth_ray_march_new(depth_size, near_plane);

    ssr_depth_ray_march_from_cs(&raymarch, ray_origin_ndc);
    ssr_depth_ray_march_to_ws_dir(&raymarch, normalize(reflection_dir_world), view_proj);

    raymarch.linear_steps = linear_steps;
    raymarch.bisection_steps = bisection_steps;
    raymarch.use_secant = bisection_steps > 0u;
    raymarch.depth_thickness_linear_z = thickness_linear_z;
    raymarch.jitter = jitter;
    raymarch.march_behind_surfaces = false;

    out.ray_march_end_uv = refl_ndc_xy_to_uv(raymarch.ray_end_cs.xy);

    let result = ssr_depth_ray_march_march(&raymarch);
    if result.hit {
        out.found = true;
        out.reflection_hit_uv = result.hit_uv;
    }
    return out;
}
