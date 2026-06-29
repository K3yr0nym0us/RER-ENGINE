// Lettier screen-space reflection (SSR).
// https://lettier.github.io/3d-game-shaders-for-beginners/screen-space-reflection.html
//
// Flujo:
//   position_view + normal_view → view_dir → reflection_dir
//   → ray_start / ray_end → proyección a píxeles → marcha (coarse + refine)
//   → UV reflejada → color escena × visibilidad × specular (miss = cero, sin fallback).

struct SsrUniforms {
    resolution        : vec2<f32>,
    gb_resolution     : vec2<f32>,
    inv_view_proj     : mat4x4<f32>,
    view_proj         : mat4x4<f32>,
    view              : mat4x4<f32>,
    inv_view          : mat4x4<f32>,
    near_plane        : f32,
    far_plane         : f32,
    max_distance_m    : f32,
    coarse_resolution : f32,
    thickness_m       : f32,
    max_roughness     : f32,
    binary_steps      : u32,
    coarse_max_iters  : u32,
    gbuffer_scale     : f32,
    _pad0             : f32,
    _pad1             : f32,
    _pad2             : f32,
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

fn decode_octahedral(enc : vec2<f32>) -> vec3<f32> {
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
    let uv_c = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let fc = uv_c * vec2<f32>(dims) - vec2<f32>(0.5);
    return vec2<i32>(fc);
}

fn scene_depth_nearest_at_uv(uv : vec2<f32>) -> f32 {
    return textureLoad(t_depth, texel_px(uv, textureDimensions(t_depth)), 0).r;
}

/// Bilinear manual (R32Float no es filterable en wgpu; no usar s_linear en t_depth).
fn scene_depth_bilinear_at_uv(uv : vec2<f32>) -> f32 {
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

    let d00 = textureLoad(t_depth, c00, 0).r;
    let d10 = textureLoad(t_depth, c10, 0).r;
    let d01 = textureLoad(t_depth, c01, 0).r;
    let d11 = textureLoad(t_depth, c11, 0).r;

    let d0 = mix(d00, d10, frac.x);
    let d1 = mix(d01, d11, frac.x);
    return mix(d0, d1, frac.y);
}

/// Híbrido nearest + bilinear (Bevy raymarch): en profundidad lineal (m) el mínimo
/// es el fragmento más cercano y reduce acne en bordes sin false-occlusion excesiva.
fn scene_depth_at_uv(uv : vec2<f32>) -> f32 {
    let nearest = scene_depth_nearest_at_uv(uv);
    let linear = scene_depth_bilinear_at_uv(uv);
    if nearest <= 1e-4 {
        return linear;
    }
    if linear <= 1e-4 {
        return nearest;
    }
    return min(nearest, linear);
}

fn world_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, view_depth_m, u.inv_view_proj, u.near_plane, u.far_plane);
}

fn lit_at_uv(uv : vec2<f32>) -> vec3<f32> {
    return textureLoad(t_lit_scene, texel_px(uv, textureDimensions(t_lit_scene)), 0).rgb;
}

/// Color de escena en UV de impacto (Lettier `colorTexture`: escena sin SSR).
fn ssr_reflected_radiance(hit_uv : vec2<f32>) -> vec3<f32> {
    return lit_at_uv(hit_uv);
}

/// Box blur Lettier sobre radiancia reflejada (sin reintroducir env en metales).
fn lettier_box_blur_radiance(uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    let texel = vec2<f32>(1.0) / u.gb_resolution;
    var acc = vec3<f32>(0.0);
    for (var oy = -3; oy <= 3; oy++) {
        for (var ox = -3; ox <= 3; ox++) {
            let p = uv + vec2<f32>(f32(ox), f32(oy)) * spacing_px * texel;
            acc += textureSampleLevel(t_lit_scene, s_linear, p, 0.0).rgb;
        }
    }
    return acc / 49.0;
}

struct LettierHit {
    found      : bool,
    hit_uv     : vec2<f32>,
    visibility : f32,
}

struct SsrRay {
    view_dir        : vec3<f32>,
    reflection_dir  : vec3<f32>,
    ray_start       : vec3<f32>,
    ray_end         : vec3<f32>,
    ray_end_depth   : f32,
}

/// Construye el rayo de reflexión en view space (Lettier + bias mínimo anti self-hit).
fn build_ssr_ray(position_view : vec3<f32>, normal_view : vec3<f32>, max_distance : f32) -> SsrRay {
    var ray : SsrRay;

    ray.view_dir = normalize(-position_view);
    ray.reflection_dir = lettier_reflection_dir(ray.view_dir, normal_view);

    let view_depth_m = lettier_view_depth_from_view_pos(position_view);
    let bias_m = min(ssr_view_normal_bias_m(view_depth_m) * 2.0, 0.1);
    ray.ray_start = position_view + normalize(normal_view) * bias_m;
    ray.ray_end = ssr_clip_ray_end_view(ray.ray_start, ray.reflection_dir, max_distance);
    ray.ray_end_depth = lettier_view_depth_from_view_pos(ray.ray_end);

    return ray;
}

/// Núcleo SSR: método Sugulee (clip-space direction + frustum-bounded max distance + sin aborts).
fn trace_lettier_ssr(
    surf_uv : vec2<f32>,
    position_view : vec3<f32>,
    normal_view : vec3<f32>,
    roughness : f32,
    n_world : vec3<f32>,
) -> LettierHit {
    var out : LettierHit;
    out.found = false;
    out.hit_uv = vec2<f32>(-1.0);
    out.visibility = 0.0;

    var ray = build_ssr_ray(position_view, normal_view, u.max_distance_m);
    let ray_start_depth = lettier_view_depth_from_view_pos(ray.ray_start);
    let tex_size = u.gb_resolution;

    // ── Segmento en UV: extremos proyectados (Bevy) en lugar del bound Sugulee ─
    let surf_depth_m = lettier_view_depth_from_view_pos(position_view);
    let ndc_z_vk = refl_view_depth_to_ndc_z_vk(surf_depth_m, u.near_plane, u.far_plane);
    let start_world = (u.inv_view * vec4<f32>(ray.ray_start, 1.0)).xyz;
    let start_uv = refl_project_uv(start_world, u.view_proj);
    let seg = ssr_uv_ray_segment(ray.ray_start, ray.reflection_dir, u.max_distance_m, u.view_proj, u.inv_view);
    let dp = seg.xy;
    let seg_len_uv = seg.z;

    if seg_len_uv <= 1e-6 {
        return out;
    }

    let rayPosTS = vec3<f32>(start_uv, ndc_z_vk);
    let endPosTS = rayPosTS + vec3<f32>(dp, 0.0);

    let startPx = rayPosTS.xy * tex_size;
    let endPx = endPosTS.xy * tex_size;
    let dpPx = endPx - startPx;
    let max_dist_px = max(abs(dpPx.x), abs(dpPx.y));

    if max_dist_px <= 0.0 {
        return out;
    }

    // Lettier `resolution`: fracción de pasos sobre el delta en píxeles (0–1).
    let coarse_iters = min(
        max(u32(max_dist_px * u.coarse_resolution), 1u),
        max(u.coarse_max_iters, 1u),
    );
    // Cubrir todo el segmento UV en `coarse_iters` pasos (Bevy: steps repartidos en [0,1]).
    let stepVec = dp / f32(coarse_iters);

    var fragPosTS = rayPosTS + vec3<f32>(stepVec, 0.0);

    let use_x = select(0.0, 1.0, abs(dpPx.x) >= abs(dpPx.y));
    var search0 = 0.0;
    var search1 = 0.0;
    var hit0 = 0u;
    var hit1 = 0u;
    let thickness = max(u.thickness_m, ssr_hit_thickness_m(ray_start_depth, roughness, ray.reflection_dir));

    // ── Coarse pass ──────────────────────────────────────────────────────
    for (var i = 0u; i < coarse_iters; i++) {
        if fragPosTS.x < 0.0 || fragPosTS.x > 1.0 || fragPosTS.y < 0.0 || fragPosTS.y > 1.0 {
            break;
        }

        let sample_uv = fragPosTS.xy;
        let scene_depth_m = scene_depth_at_uv(sample_uv);
        if scene_depth_m <= 1e-4 {
            fragPosTS += vec3<f32>(stepVec, 0.0);
            continue;
        }

        search1 = clamp(
            mix(
                (fragPosTS.y - rayPosTS.y) / max(dp.y, 1e-10),
                (fragPosTS.x - rayPosTS.x) / max(dp.x, 1e-10),
                use_x,
            ),
            0.0, 1.0,
        );
        let ray_depth_m = lettier_perspective_depth(ray_start_depth, ray.ray_end_depth, search1);
        let depth_delta = ray_depth_m - scene_depth_m;

        if depth_delta > 0.0 && depth_delta < thickness {
            hit0 = 1u;
            break;
        }
        search0 = search1;
        fragPosTS += vec3<f32>(stepVec, 0.0);
    }

    if hit0 == 0u {
        return out;
    }

    let refine_iters = u.binary_steps * hit0;
    var refine_l = search0;
    var refine_r = search1;

    // ── Binary refinement ────────────────────────────────────────────────
    for (var j = 0u; j < refine_iters; j++) {
        let test_t = (refine_l + refine_r) * 0.5;
        let testTS = mix(rayPosTS.xy, endPosTS.xy, test_t);
        if testTS.x < 0.0 || testTS.x > 1.0 || testTS.y < 0.0 || testTS.y > 1.0 {
            break;
        }

        let scene_depth_m = scene_depth_at_uv(testTS);
        if scene_depth_m <= 1e-4 {
            break;
        }

        let ray_depth_m = lettier_perspective_depth(ray_start_depth, ray.ray_end_depth, test_t);
        let depth_delta = ray_depth_m - scene_depth_m;

        if depth_delta > 0.0 && depth_delta < thickness {
            hit1 = 1u;
            refine_r = test_t;
        } else {
            refine_l = test_t;
        }
    }
    search1 = (refine_l + refine_r) * 0.5;

    let hit_uv = mix(rayPosTS.xy, endPosTS.xy, search1);

    let surf_depth_m2 = lettier_view_depth_from_view_pos(position_view);
    let scene_depth_m2 = scene_depth_at_uv(hit_uv);
    let surf_world = world_from_depth(surf_uv, surf_depth_m2);
    let hit_world = world_from_depth(hit_uv, scene_depth_m2);
    let n_hit_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(hit_uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    if ssr_reject_self_reflection(
        surf_world,
        hit_world,
        n_world,
        n_hit_world,
        surf_depth_m2,
        scene_depth_m2,
        surf_uv,
        hit_uv,
        tex_size,
    ) {
        return out;
    }

    if scene_depth_m2 <= 1e-4 {
        return out;
    }

    let ray_depth_m2 = lettier_perspective_depth(ray_start_depth, ray.ray_end_depth, search1);
    let depth_delta2 = ray_depth_m2 - scene_depth_m2;

    let position_to_view = (u.view * vec4<f32>(hit_world, 1.0)).xyz;
    let vis_fade_m = min(u.max_distance_m, 50.0);
    let vis = lettier_ssr_visibility(
        hit1 == 1u,
        hit0 == 1u,
        ray.view_dir,
        ray.reflection_dir,
        depth_delta2,
        thickness,
        position_view,
        position_to_view,
        vis_fade_m,
        hit_uv,
    );

    if vis <= 0.0 {
        return out;
    }

    out.found = true;
    out.hit_uv = hit_uv;
    out.visibility = vis;
    return out;
}

@fragment
fn fs_main(in : VsOut) -> SsrOut {
    var result : SsrOut;
    result.reflection = vec4<f32>(0.0);
    result.hit_uv = vec4<f32>(-1.0, -1.0, 0.0, 0.0);

    let view_depth_m = scene_depth_at_uv(in.uv);
    if view_depth_m <= 1e-4 {
        return result;
    }

    let n_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(in.uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let surf_px = texel_px(in.uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;
    if roughness > u.max_roughness {
        return result;
    }

    let metallic = textureLoad(t_direct, texel_px(in.uv, textureDimensions(t_direct)), 0).a;
    let albedo = textureLoad(t_base_color, surf_px, 0).rgb;

    let world_pos = world_from_depth(in.uv, view_depth_m);
    let position_view = (u.view * vec4<f32>(world_pos, 1.0)).xyz;
    let normal_view = normalize((u.view * vec4<f32>(n_world, 0.0)).xyz);

    let V_view = normalize(-position_view);
    let V_world = normalize((u.inv_view * vec4<f32>(V_view, 0.0)).xyz);
    let trace_w = refl_trace_strength(roughness, metallic, n_world, V_world, albedo);

    let hit = trace_lettier_ssr(in.uv, position_view, normal_view, roughness, n_world);
    if !hit.found {
        // Sin hit en pantalla → 0 (probes/RT aparte).
        return result;
    }

    // Lettier reflection-color: escena reflejada × visibilidad de marcha.
    let blur_spacing = lettier_reflection_roughness(roughness) * 4.0 + 1.0;
    let color_sharp = ssr_reflected_radiance(hit.hit_uv);
    let color_blur = lettier_box_blur_radiance(hit.hit_uv, blur_spacing);
    let reflected = mix(color_sharp, color_blur, lettier_reflection_roughness(roughness));

    // Bevy: indirect += ssr_rgb * brdf_weight * fade. Fresnel modula intensidad, no bloquea el trace.
    let vis = hit.visibility;
    let refl_rgb = reflected * trace_w * vis;
    let refl_lum = dot(refl_rgb, vec3<f32>(0.2126, 0.7152, 0.0722));

    result.reflection = vec4<f32>(refl_rgb, max(trace_w * vis, saturate(refl_lum * 3.0)));
    result.hit_uv = vec4<f32>(hit.hit_uv, vis, 1.0);
    return result;
}
