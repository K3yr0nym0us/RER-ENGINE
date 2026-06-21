struct RtUniforms {
    inv_view_proj   : mat4x4<f32>,
    view_proj       : mat4x4<f32>,
    cam_pos         : vec4<f32>,
    resolution      : vec2<f32>,
    max_distance_m  : f32,
    max_roughness   : f32,
    rt_blend        : f32,
    step_size       : f32,
    near_plane      : f32,
    far_plane       : f32,
    frame_index     : u32,
    _pad0           : u32,
    _pad1           : f32,
    _pad2           : f32,
}

const RT_TILE_SIZE : u32 = 16u;

@group(0) @binding(0) var<uniform> u : RtUniforms;
@group(0) @binding(1) var tlas : acceleration_structure;
@group(0) @binding(2) var reflection_out : texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var t_ssr : texture_2d<f32>;
@group(0) @binding(4) var t_depth : texture_2d<f32>;
@group(0) @binding(5) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(6) var t_lit_scene : texture_2d<f32>;
@group(0) @binding(7) var t_direct : texture_2d<f32>;
@group(0) @binding(8) var t_surface : texture_2d<f32>;
@group(0) @binding(9) var t_base_color : texture_2d<f32>;
@group(0) @binding(10) var<storage, read> tile_list : array<u32>;
@group(0) @binding(11) var<storage, read> tile_count_buf : array<u32>;

@group(1) @binding(0) var t_probe_env : texture_cube_array<f32>;
@group(1) @binding(1) var s_probe_env : sampler;
@group(1) @binding(2) var<uniform> probe_meta : ReflProbeMeta;

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
    let fc = clamped * u.resolution - vec2<f32>(0.5);
    return vec2<i32>(fc);
}

fn base_color_at(uv : vec2<f32>) -> vec3<f32> {
    return textureLoad(t_base_color, texel_px(uv), 0).rgb;
}

fn lit_scene_blurred(uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    let ref_depth_m = textureLoad(t_depth, vec2<i32>(uv * u.resolution), 0).r;
    if ref_depth_m <= 0.0001 {
        return textureLoad(t_lit_scene, vec2<i32>(uv * u.resolution), 0).rgb;
    }
    let depth_reject_m = refl_depth_reject_m(u.step_size);
    let texel = vec2<f32>(1.0) / u.resolution;
    var acc = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var oy = -3; oy <= 3; oy++) {
        for (var ox = -3; ox <= 3; ox++) {
            let p = uv + vec2<f32>(f32(ox), f32(oy)) * spacing_px * texel;
            let px = vec2<i32>(p * u.resolution);
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
        return textureLoad(t_lit_scene, vec2<i32>(uv * u.resolution), 0).rgb;
    }
    return acc / wsum;
}

const RAY_FLAG_FORCE_OPAQUE : u32 = 0x1u;
const RAY_QUERY_INTERSECTION_NONE : u32 = 0u;

fn trace_rt_hw(origin : vec3<f32>, dir : vec3<f32>, t_max : f32) -> f32 {
    var rq : ray_query;
    let desc = RayDesc(
        RAY_FLAG_FORCE_OPAQUE,
        0xFFu,
        REFL_RAY_T_MIN,
        t_max,
        origin,
        dir,
    );
    rayQueryInitialize(&rq, tlas, desc);
    loop {
        if !rayQueryProceed(&rq) {
            break;
        }
    }
    let hit = rayQueryGetCommittedIntersection(&rq);
    if hit.kind == RAY_QUERY_INTERSECTION_NONE {
        return -1.0;
    }
    return hit.t;
}

fn rt_shade_pixel_at(gid : vec2<u32>) {
    if gid.x >= u32(u.resolution.x) || gid.y >= u32(u.resolution.y) {
        return;
    }
    let px = vec2<i32>(i32(gid.x), i32(gid.y));
    let uv = (vec2<f32>(gid) + vec2<f32>(0.5)) / u.resolution;
    let ssr = textureLoad(t_ssr, px, 0);

    let view_depth_m = textureLoad(t_depth, px, 0).r;
    if view_depth_m <= 0.0001 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let packed = textureLoad(t_normal_roughness, px, 0);
    let n = normal_from_packed(packed);
    let roughness = textureLoad(t_surface, px, 0).g;
    let metallic = textureLoad(t_direct, px, 0).a;
    let albedo = base_color_at(uv);
    if roughness > u.max_roughness {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let v = normalize(u.cam_pos.xyz - world_pos);
    let refl_dir = refl_fuzzy_mirror_dir_temporal(
        world_pos,
        u.cam_pos.xyz,
        n,
        roughness,
        u.frame_index,
        uv,
    );
    if dot(refl_dir, refl_dir) <= 1e-8 {
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
    let closest = trace_rt_hw(ray_origin, refl_dir, u.max_distance_m);
    if closest < 0.0 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let hit_pos = ray_origin + refl_dir * closest;
    let sample_uv = project_uv(hit_pos);
    let on_screen = sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0;

    let spacing_px = refl_blur_spacing_px(roughness, closest);
    var scene_col = vec3<f32>(0.0);
    var valid_hit = false;

    if on_screen {
        let hit_px = vec2<i32>(sample_uv * u.resolution);
        let hit_depth_m = textureLoad(t_depth, hit_px, 0).r;
        let ray_depth_m = view_depth_m_from_world(hit_pos);
        if abs(ray_depth_m - hit_depth_m) <= refl_rt_hit_depth_reject_m(u.step_size) {
            let hit_metallic = textureLoad(t_direct, hit_px, 0).a;
            let hit_albedo = textureLoad(t_base_color, hit_px, 0).rgb;
            scene_col = refl_metal_attenuate(
                lit_scene_blurred(sample_uv, spacing_px),
                hit_albedo,
                hit_metallic,
            );
            valid_hit = true;
        }
    }

    if !valid_hit {
        scene_col = refl_sample_probe_at_hit(
            hit_pos,
            refl_dir,
            roughness,
            probe_meta,
            t_probe_env,
            s_probe_env,
        );
    }

    let rt_weight = (1.0 - ssr.a) * strength * u.rt_blend;
    if rt_weight < 0.01 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let out_rgb = mix(ssr.rgb, scene_col, rt_weight);
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
