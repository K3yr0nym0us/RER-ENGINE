struct BvhNode {
    bbox_min : vec4<f32>,
    bbox_max : vec4<f32>,
    left_or_tri_offset : u32,
    right_or_tri_count : u32,
    flags : u32,
    _pad : u32,
}

struct RtUniforms {
    inv_view_proj   : mat4x4<f32>,
    view_proj       : mat4x4<f32>,
    cam_pos         : vec4<f32>,
    resolution      : vec2<f32>,
    node_count      : u32,
    tri_count       : u32,
    max_distance_m  : f32,
    max_roughness   : f32,
    rt_blend        : f32,
    step_size       : f32,
    near_plane      : f32,
    far_plane       : f32,
    frame_index     : u32,
    material_count  : u32,
    gbuffer_scale   : f32,
    _pad            : f32,
}

const RT_TILE_SIZE : u32 = 16u;

@group(0) @binding(0) var<uniform> u : RtUniforms;
@group(0) @binding(11) var<storage, read> tile_list : array<u32>;
@group(0) @binding(12) var<storage, read> tile_count_buf : array<u32>;
@group(0) @binding(1) var<storage, read> bvh_nodes : array<BvhNode>;
@group(0) @binding(2) var<storage, read> bvh_tris : array<RtTri>;
@group(0) @binding(3) var reflection_out : texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var t_ssr : texture_2d<f32>;
@group(0) @binding(5) var t_depth : texture_2d<f32>;
@group(0) @binding(6) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(7) var t_lit_scene : texture_2d<f32>;
@group(0) @binding(8) var t_direct : texture_2d<f32>;
@group(0) @binding(9) var t_surface : texture_2d<f32>;
@group(0) @binding(10) var t_base_color : texture_2d<f32>;
@group(0) @binding(13) var<storage, read> instance_materials : array<RtInstanceMaterial>;

@group(1) @binding(0) var t_probe_env : texture_cube_array<f32>;
@group(1) @binding(1) var s_probe_env : sampler;
@group(1) @binding(2) var<uniform> probe_meta : ReflProbeMeta;

@group(2) @binding(0) var t_shadow : texture_depth_2d;
@group(2) @binding(1) var s_shadow : sampler_comparison;
@group(2) @binding(2) var<uniform> rt_light : RtLightUniform;

@group(3) @binding(0) var t_albedo_array : texture_2d_array<f32>;
@group(3) @binding(1) var s_albedo : sampler;

fn rt_resolve_hit(
    hit_pos : vec3<f32>,
    hit_normal : vec3<f32>,
    refl_dir : vec3<f32>,
    hit_tri : RtTri,
    has_tri : bool,
    sample_uv : vec2<f32>,
    on_screen : bool,
    spacing_px : f32,
    mat : RtInstanceMaterial,
    has_mat : bool,
    rt_occlusion : f32,
) -> vec3<f32> {
    let bary = select(vec3<f32>(1.0, 0.0, 0.0), refl_tri_barycentric(hit_tri, hit_pos), has_tri);
    return refl_resolve_hit_radiance(
        hit_pos,
        hit_normal,
        refl_dir,
        sample_uv,
        on_screen,
        spacing_px,
        mat,
        has_mat,
        has_tri,
        hit_tri,
        bary,
        probe_meta,
        t_probe_env,
        s_probe_env,
        t_albedo_array,
        s_albedo,
        rt_light,
        t_shadow,
        s_shadow,
        t_lit_scene,
        t_depth,
        t_normal_roughness,
        t_direct,
        t_base_color,
        u.resolution * u.gbuffer_scale,
        u.step_size,
        u.view_proj,
        u.near_plane,
        u.far_plane,
        rt_occlusion,
    );
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

fn normal_from_packed(packed: vec4<f32>) -> vec3<f32> {
    return decode_octahedral(packed.zw);
}

fn texel_px(uv : vec2<f32>) -> vec2<i32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let gb_res = u.resolution * u.gbuffer_scale;
    let fc = clamped * gb_res - vec2<f32>(0.5);
    return vec2<i32>(fc);
}

fn base_color_at(uv : vec2<f32>) -> vec3<f32> {
    return textureLoad(t_base_color, texel_px(uv), 0).rgb;
}

fn lit_scene_at(uv : vec2<f32>) -> vec3<f32> {
    return textureLoad(t_lit_scene, texel_px(uv), 0).rgb;
}

fn lit_scene_blurred(uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    let ref_depth_m = textureLoad(t_depth, texel_px(uv), 0).r;
    if ref_depth_m <= 0.0001 {
        return lit_scene_at(uv);
    }
    let depth_reject_m = refl_depth_reject_m(u.step_size);
    let texel = vec2<f32>(1.0) / (u.resolution * u.gbuffer_scale);
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var oy = -3; oy <= 3; oy++) {
        for (var ox = -3; ox <= 3; ox++) {
            let p = uv + vec2<f32>(f32(ox), f32(oy)) * spacing_px * texel;
            let px = texel_px(p);
            let tap_depth_m = textureLoad(t_depth, px, 0).r;
            if tap_depth_m <= 0.0001 {
                continue;
            }
            if abs(tap_depth_m - ref_depth_m) > depth_reject_m {
                continue;
            }
            acc += textureLoad(t_lit_scene, px, 0).rgb;
            wsum += 1.0;
        }
    }
    if wsum < 1.0 {
        return lit_scene_at(uv);
    }
    return acc / wsum;
}

fn ray_aabb(origin : vec3<f32>, dir : vec3<f32>, bmin : vec3<f32>, bmax : vec3<f32>, t_max : f32) -> f32 {
    var tmin = REFL_RAY_T_MIN;
    var tmax = t_max;
    for (var i = 0; i < 3; i++) {
        let o = origin[i];
        let d = dir[i];
        let mn = bmin[i];
        let mx = bmax[i];
        if abs(d) < 1e-6 {
            if o < mn || o > mx {
                return -1.0;
            }
        } else {
            var t1 = (mn - o) / d;
            var t2 = (mx - o) / d;
            if t1 > t2 {
                let tmp = t1;
                t1 = t2;
                t2 = tmp;
            }
            tmin = max(tmin, t1);
            tmax = min(tmax, t2);
            if tmax < tmin {
                return -1.0;
            }
        }
    }
    if tmin > t_max {
        return -1.0;
    }
    return tmin;
}

fn ray_tri_moller(origin : vec3<f32>, dir : vec3<f32>, v0 : vec3<f32>, v1 : vec3<f32>, v2 : vec3<f32>, t_max : f32) -> f32 {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let pvec = cross(dir, edge2);
    let det = dot(edge1, pvec);
    if abs(det) < 1e-8 {
        return -1.0;
    }
    let inv_det = 1.0 / det;
    let tvec = origin - v0;
    let u = dot(tvec, pvec) * inv_det;
    if u < 0.0 || u > 1.0 {
        return -1.0;
    }
    let qvec = cross(tvec, edge1);
    let v = dot(dir, qvec) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return -1.0;
    }
    let t = dot(edge2, qvec) * inv_det;
    if t < REFL_RAY_T_MIN || t > t_max {
        return -1.0;
    }
    // Rechazar back-faces (RTIOW: solo cara frontal hacia el rayo).
    let face_n = cross(edge1, edge2);
    if dot(face_n, dir) >= 0.0 {
        return -1.0;
    }
    return t;
}

struct BvhHit {
    found : bool,
    t : f32,
    tri_idx : u32,
}

fn bvh_trace(origin : vec3<f32>, dir : vec3<f32>, t_max : f32) -> BvhHit {
    var hit : BvhHit;
    hit.found = false;
    hit.t = t_max + 1.0;
    hit.tri_idx = 0u;
    if u.node_count == 0u || u.tri_count == 0u {
        return hit;
    }

    var stack : array<u32, 32>;
    var sp = 0u;
    stack[0] = 0u;
    sp = 1u;

    while sp > 0u {
        sp -= 1u;
        let node_idx = stack[sp];
        if node_idx >= u.node_count {
            continue;
        }
        let node = bvh_nodes[node_idx];
        let bmin = node.bbox_min.xyz;
        let bmax = node.bbox_max.xyz;
        let box_t = ray_aabb(origin, dir, bmin, bmax, hit.t);
        if box_t < 0.0 {
            continue;
        }
        if node.flags == 1u {
            let tri_start = node.left_or_tri_offset;
            let tri_count = node.right_or_tri_count;
            for (var ti = 0u; ti < tri_count; ti++) {
                let idx = tri_start + ti;
                if idx >= u.tri_count {
                    break;
                }
                let tri = bvh_tris[idx];
                let t = ray_tri_moller(origin, dir, tri.v0.xyz, tri.v1.xyz, tri.v2.xyz, hit.t);
                if t > 0.0 && t < hit.t {
                    hit.t = t;
                    hit.tri_idx = idx;
                    hit.found = true;
                }
            }
        } else {
            let left = node.left_or_tri_offset;
            let right = node.right_or_tri_count;
            if sp + 2u <= 32u {
                stack[sp] = right;
                sp += 1u;
                stack[sp] = left;
                sp += 1u;
            }
        }
    }
    return hit;
}

fn rt_shade_pixel_at(gid : vec2<u32>) {
    if gid.x >= u32(u.resolution.x) || gid.y >= u32(u.resolution.y) {
        return;
    }
    let px = vec2<i32>(i32(gid.x), i32(gid.y));
    let uv = (vec2<f32>(gid) + vec2<f32>(0.5)) / u.resolution;
    let ssr = textureLoad(t_ssr, px, 0);
    let gb_px = vec2<i32>(vec2<f32>(px) * u.gbuffer_scale);

    let view_depth_m = textureLoad(t_depth, gb_px, 0).r;
    if view_depth_m <= 0.0001 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let packed = textureLoad(t_normal_roughness, gb_px, 0);
    let n = normal_from_packed(packed);
    let roughness = textureLoad(t_surface, gb_px, 0).g;
    let metallic = textureLoad(t_direct, gb_px, 0).a;
    let src_ior = textureLoad(t_direct, gb_px, 0).b * 10.0;
    let albedo = base_color_at(uv);
    if roughness > u.max_roughness {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let v = normalize(u.cam_pos.xyz - world_pos);
    let seed = refl_blue_noise_seed(u.frame_index, uv);
    let T = refl_tangent(n);
    let B = refl_bitangent(n, T);
    let H_local = ggx_sample_ndf(roughness, seed);
    let H = normalize(H_local.x * T + H_local.y * B + H_local.z * n);
    var trace_dir = reflect(v, H);
    if rt_light.rt_flags.x > 0.5 && src_ior > 1.0 && metallic < 0.5 {
        let eta = 1.0 / src_ior;
        trace_dir = refl_refract_dir(normalize(world_pos - u.cam_pos.xyz), n, eta);
    }
    if dot(trace_dir, trace_dir) <= 1e-8 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let strength = refl_trace_strength(roughness, metallic, n, v, albedo);
    if strength < 0.005 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let normal_bias = refl_normal_bias(u.step_size);
    let ray_origin = world_pos + n * normal_bias;
    let bvh_hit = bvh_trace(ray_origin, trace_dir, u.max_distance_m);
    if !bvh_hit.found {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let hit_pos = ray_origin + trace_dir * bvh_hit.t;
    let tri = bvh_tris[bvh_hit.tri_idx];
    let hit_normal = refl_tri_normal(tri);
    let instance_slot = refl_tri_instance_slot(tri);
    let sample_uv = project_uv(hit_pos);
    let on_screen = sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0;

    let spacing_px = refl_blur_spacing_px(roughness, bvh_hit.t);
    var hit_mat = RtInstanceMaterial(vec4<f32>(1.0), vec4<f32>(0.5, 0.0, 0.0, 0.0), vec4<f32>(-1.0, 0.0, 0.0, 0.0));
    let has_mat = instance_slot < u.material_count;
    if has_mat {
        hit_mat = instance_materials[instance_slot];
    }
    var rt_shadow_occlusion = 1.0;
    if rt_light.rt_flags.w > 0.5 {
        let light_dir = normalize(rt_light.light_dir.xyz);
        let shadow_hit = bvh_trace(hit_pos + n * normal_bias, light_dir, u.max_distance_m);
        rt_shadow_occlusion = select(1.0, 0.0, shadow_hit.found);
    }

    let scene_col = rt_resolve_hit(
        hit_pos,
        hit_normal,
        trace_dir,
        tri,
        true,
        sample_uv,
        on_screen,
        spacing_px,
        hit_mat,
        has_mat,
        rt_shadow_occlusion,
    );

    var final_col = scene_col;
    // GGX importance sampling BRDF/PDF weight for primary reflection ray
    let is_refrac_primary = rt_light.rt_flags.x > 0.5 && src_ior > 1.0 && metallic < 0.5;
    if !is_refrac_primary {
        let ndotv_val = max(dot(n, v), 1e-8);
        let ndotl_val = max(dot(n, trace_dir), 1e-8);
        let ndoth_val = max(dot(n, H), 1e-8);
        let vdoth_val = max(dot(v, H), 1e-8);
        let f0 = refl_metal_f0(albedo, metallic);
        let w = ggx_reflection_weight(roughness, ndotv_val, ndotl_val, ndoth_val, vdoth_val, f0);
        final_col = refl_firefly_clamp(final_col * w, 50.0);
    }
    if rt_light.rt_flags.y > 0.5 && has_mat {
        let hit_metallic = hit_mat.pbr.y;
        let hit_rough = hit_mat.pbr.x;
        let hit_albedo = hit_mat.albedo.xyz;
        let hit_flags = bitcast<u32>(hit_mat.albedo.w);
        let hit_dielectric = (hit_flags & RT_MAT_FLAG_DIELECTRIC) != 0u;
        let sec_origin = hit_pos + hit_normal * normal_bias;
        if hit_metallic > 0.5 && hit_rough < 0.85 {
            let sec_seed = refl_blue_noise_seed(u.frame_index + 17u, sample_uv);
            let sec_T = refl_tangent(hit_normal);
            let sec_B = refl_bitangent(hit_normal, sec_T);
            let sec_H_local = ggx_sample_ndf(hit_rough, sec_seed);
            let sec_H = normalize(sec_H_local.x * sec_T + sec_H_local.y * sec_B + sec_H_local.z * hit_normal);
            let sec_incident = normalize(hit_pos - u.cam_pos.xyz);
            let sec_dir = reflect(sec_incident, sec_H);
            let sec_hit = bvh_trace(sec_origin, sec_dir, u.max_distance_m * 0.5);
            if sec_hit.found {
                let sec_pos = sec_origin + sec_dir * sec_hit.t;
                let sec_tri = bvh_tris[sec_hit.tri_idx];
                let sec_n = refl_tri_normal(sec_tri);
                let sec_slot = refl_tri_instance_slot(sec_tri);
                let sec_uv = project_uv(sec_pos);
                let sec_on = sec_uv.x >= 0.0 && sec_uv.x <= 1.0 && sec_uv.y >= 0.0 && sec_uv.y <= 1.0;
                var sec_mat = hit_mat;
                var sec_has = false;
                if sec_slot < u.material_count {
                    sec_mat = instance_materials[sec_slot];
                    sec_has = true;
                }
                var sec_shadow_occlusion = 1.0;
                if rt_light.rt_flags.w > 0.5 {
                    let light_dir = normalize(rt_light.light_dir.xyz);
                    let sec_shadow_hit = bvh_trace(sec_pos + sec_n * normal_bias, light_dir, u.max_distance_m);
                    sec_shadow_occlusion = select(1.0, 0.0, sec_shadow_hit.found);
                }
                let sec_col = rt_resolve_hit(
                    sec_pos,
                    sec_n,
                    sec_dir,
                    sec_tri,
                    true,
                    sec_uv,
                    sec_on,
                    spacing_px,
                    sec_mat,
                    sec_has,
                    sec_shadow_occlusion,
                );
                final_col = refl_firefly_clamp(hit_albedo * scene_col + sec_col * 0.35, 50.0);
            }
        } else if hit_dielectric && rt_light.rt_flags.x > 0.5 {
            let ior = max(hit_mat.pbr.z, 1.01);
            let eta = 1.0 / ior;
            let sec_dir = refl_refract_dir(-trace_dir, hit_normal, eta);
            if dot(sec_dir, sec_dir) > 1e-8 {
                let sec_hit = bvh_trace(sec_origin, sec_dir, u.max_distance_m * 0.5);
                if sec_hit.found {
                    let sec_pos = sec_origin + sec_dir * sec_hit.t;
                    let sec_tri = bvh_tris[sec_hit.tri_idx];
                    let sec_n = refl_tri_normal(sec_tri);
                    let sec_slot = refl_tri_instance_slot(sec_tri);
                    let sec_uv = project_uv(sec_pos);
                    let sec_on = sec_uv.x >= 0.0 && sec_uv.x <= 1.0 && sec_uv.y >= 0.0 && sec_uv.y <= 1.0;
                    var sec_mat = hit_mat;
                    var sec_has = false;
                    if sec_slot < u.material_count {
                        sec_mat = instance_materials[sec_slot];
                        sec_has = true;
                    }
                    var sec2_shadow_occlusion = 1.0;
                    if rt_light.rt_flags.w > 0.5 {
                        let light_dir = normalize(rt_light.light_dir.xyz);
                        let sec2_shadow_hit = bvh_trace(sec_pos + sec_n * normal_bias, light_dir, u.max_distance_m);
                        sec2_shadow_occlusion = select(1.0, 0.0, sec2_shadow_hit.found);
                    }
                    let sec_col = rt_resolve_hit(
                        sec_pos,
                        sec_n,
                        sec_dir,
                        sec_tri,
                        true,
                        sec_uv,
                        sec_on,
                        spacing_px,
                        sec_mat,
                        sec_has,
                        sec2_shadow_occlusion,
                    );
                    final_col = refl_firefly_clamp(mix(scene_col, sec_col, 0.45), 50.0);
                }
            }
        }

    }

    let rt_weight = (1.0 - ssr.a) * strength * u.rt_blend;
    if rt_weight < 0.01 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let out_rgb = mix(ssr.rgb, final_col, rt_weight);
    let out_a = max(ssr.a, rt_weight);
    textureStore(reflection_out, px, vec4<f32>(out_rgb, out_a));
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    rt_shade_pixel_at(gid.xy);
}

@compute @workgroup_size(8, 8)
fn cs_sparse_main(
    @builtin(workgroup_id) wg_id : vec3<u32>,
    @builtin(local_invocation_id) lid : vec3<u32>,
) {
    let tile_idx = wg_id.x;
    let active_tiles = tile_count_buf[0];
    if tile_idx >= active_tiles {
        return;
    }
    let tx = tile_list[tile_idx * 2u];
    let ty = tile_list[tile_idx * 2u + 1u];
    let pixel = vec2<u32>(tx * RT_TILE_SIZE + lid.x, ty * RT_TILE_SIZE + lid.y);
    rt_shade_pixel_at(pixel);
}
