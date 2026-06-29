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

fn scene_depth_at_uv(uv : vec2<f32>) -> f32 {
    return textureLoad(t_depth, texel_px(uv, textureDimensions(t_depth)), 0).r;
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
    ray.ray_end = ray.ray_start + ray.reflection_dir * max_distance;
    ray.ray_end_depth = lettier_view_depth_from_view_pos(ray.ray_end);

    return ray;
}

/// Núcleo SSR Lettier: marcha en espacio de pantalla + refinamiento binario.
fn trace_lettier_ssr(
    surf_uv : vec2<f32>,
    position_view : vec3<f32>,
    normal_view : vec3<f32>,
    roughness : f32,
) -> LettierHit {
    var out : LettierHit;
    out.found = false;
    out.hit_uv = vec2<f32>(-1.0);
    out.visibility = 0.0;

    var ray = build_ssr_ray(position_view, normal_view, u.max_distance_m);
    let ray_start_depth = lettier_view_depth_from_view_pos(ray.ray_start);

    // ── Proyección del rayo a píxeles ────────────────────────────────────
    let tex_size = u.gb_resolution;
    var start_frag = lettier_view_to_frag_px(ray.ray_start, u.inv_view, u.view_proj, tex_size);
    var end_frag = lettier_view_to_frag_px(ray.ray_end, u.inv_view, u.view_proj, tex_size);
    if start_frag.x < 0.0 {
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
            return out;
        }
    }

    let delta_x = end_frag.x - start_frag.x;
    let delta_y = end_frag.y - start_frag.y;
    let use_x = select(0.0, 1.0, abs(delta_x) >= abs(delta_y));
    let delta = mix(abs(delta_y), abs(delta_x), use_x) * clamp(u.coarse_resolution, 0.0, 1.0);
    let line_span = max(delta, 1.0);
    // Lettier: int(delta) iteraciones, 1 paso por unidad de línea (resolución ya en `delta`).
    let coarse_iters = min(u32(line_span), max(u.coarse_max_iters, 1u));
    let increment = vec2<f32>(delta_x, delta_y) / f32(coarse_iters);

    var frag = start_frag;
    var search0 = 0.0;
    var search1 = 0.0;
    var hit0 = 0u;
    var hit1 = 0u;
    let thickness = max(u.thickness_m, ssr_hit_thickness_m(ray_start_depth, roughness, ray.reflection_dir));

    // ── Paso 1: coarse pass ──────────────────────────────────────────────
    for (var i = 0u; i < coarse_iters; i++) {
        frag += increment;
        let sample_uv = frag / tex_size;
        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            break;
        }

        let scene_depth_m = scene_depth_at_uv(sample_uv);
        if scene_depth_m <= 1e-4 {
            continue;
        }

        search1 = clamp(lettier_line_search_t(frag, start_frag, delta_x, delta_y, use_x), 0.0, 1.0);
        let ray_depth_m = lettier_perspective_depth(ray_start_depth, ray.ray_end_depth, search1);
        let depth_delta = ray_depth_m - scene_depth_m;

        if depth_delta > 0.0 && depth_delta < thickness {
            hit0 = 1u;
            break;
        }
        search0 = search1;
    }

    if hit0 == 0u {
        return out;
    }

    let refine_iters = u.binary_steps * hit0;
    var refine_l = search0;
    var refine_r = search1;

    // ── Paso 2: refinement pass (binary search simétrico) ────────────────
    for (var j = 0u; j < refine_iters; j++) {
        let test_t = (refine_l + refine_r) * 0.5;
        frag = mix(start_frag, end_frag, test_t);
        let sample_uv = frag / tex_size;
        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            break;
        }

        let scene_depth_m = scene_depth_at_uv(sample_uv);
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

    frag = mix(start_frag, end_frag, search1);
    let hit_uv = frag / tex_size;

    let surf_depth_m = lettier_view_depth_from_view_pos(position_view);
    let scene_depth_m = scene_depth_at_uv(hit_uv);
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

    let ray_depth_m = lettier_perspective_depth(ray_start_depth, ray.ray_end_depth, search1);
    let depth_delta = ray_depth_m - scene_depth_m;

    // Lettier `positionTo`: posición de escena en el UV de impacto (no el punto del rayo).
    let hit_world = world_from_depth(hit_uv, scene_depth_m);
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
    let NdotV = max(dot(normal_view, V_view), 0.0);
    let specular_amount = lettier_specular_amount(metallic, roughness, albedo, NdotV);
    if specular_amount <= 0.0 {
        return result;
    }

    let hit = trace_lettier_ssr(in.uv, position_view, normal_view, roughness);
    if !hit.found {
        // Lettier: sin hit en pantalla → reflection-color = 0 (probes/RT aparte).
        return result;
    }

    // Lettier reflection-color: mix(0, sceneColor, visibility) → premultiplicado en RGB.
    let blur_spacing = lettier_reflection_roughness(roughness) * 4.0 + 1.0;
    let color_sharp = ssr_reflected_radiance(hit.hit_uv);
    let color_blur = lettier_box_blur_radiance(hit.hit_uv, blur_spacing);
    let reflected = mix(color_sharp, color_blur, lettier_reflection_roughness(roughness));

    // Lettier reflection.frag: mix(color, colorBlur, roughness) * specularAmount
    let vis = hit.visibility;
    let refl_rgb = reflected * specular_amount * vis;

    result.reflection = vec4<f32>(refl_rgb, specular_amount * vis);
    result.hit_uv = vec4<f32>(hit.hit_uv, vis, 1.0);
    return result;
}
