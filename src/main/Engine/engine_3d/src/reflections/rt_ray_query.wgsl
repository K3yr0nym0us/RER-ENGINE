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
    material_count  : u32,
    _pad0           : f32,
    _pad1           : f32,
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
@group(0) @binding(12) var<storage, read> instance_materials : array<RtInstanceMaterial>;

@group(1) @binding(0) var t_probe_env : texture_cube_array<f32>;
@group(1) @binding(1) var s_probe_env : sampler;
@group(1) @binding(2) var<uniform> probe_meta : ReflProbeMeta;

@group(2) @binding(0) var t_shadow : texture_depth_2d;
@group(2) @binding(1) var s_shadow : sampler_comparison;
@group(2) @binding(2) var<uniform> rt_light : RtLightUniform;

fn world_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, view_depth_m, u.inv_view_proj, u.near_plane, u.far_plane);
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

fn base_color_at(uv : vec2<f32>) -> vec3<f32> {
    let clamped = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let fc = clamped * u.resolution - vec2<f32>(0.5);
    return textureLoad(t_base_color, vec2<i32>(fc), 0).rgb;
}

const RAY_FLAG_FORCE_OPAQUE : u32 = 0x1u;
const RAY_QUERY_INTERSECTION_NONE : u32 = 0u;

struct HwHit {
    found : bool,
    t : f32,
    instance_slot : u32,
}

fn trace_rt_hw(origin : vec3<f32>, dir : vec3<f32>, t_max : f32) -> HwHit {
    var out : HwHit;
    out.found = false;
    out.t = -1.0;
    out.instance_slot = 0u;
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
        return out;
    }
    out.found = true;
    out.t = hit.t;
    out.instance_slot = hit.instance_custom_data;
    return out;
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
    let src_ior = textureLoad(t_direct, px, 0).b * 10.0;
    let albedo = base_color_at(uv);
    if roughness > u.max_roughness {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let v = normalize(u.cam_pos.xyz - world_pos);
    var trace_dir = refl_fuzzy_mirror_dir_temporal(
        world_pos,
        u.cam_pos.xyz,
        n,
        roughness,
        u.frame_index,
        uv,
    );
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
    let hw_hit = trace_rt_hw(ray_origin, trace_dir, u.max_distance_m);
    if !hw_hit.found {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let hit_pos = ray_origin + trace_dir * hw_hit.t;
    let hit_normal = -normalize(trace_dir);
    let sample_uv = project_uv(hit_pos);
    let on_screen = sample_uv.x >= 0.0 && sample_uv.x <= 1.0 && sample_uv.y >= 0.0 && sample_uv.y <= 1.0;
    let spacing_px = refl_blur_spacing_px(roughness, hw_hit.t);

    var hit_mat = RtInstanceMaterial(vec4<f32>(1.0), vec4<f32>(0.5, 0.0, 0.0, 0.0));
    let has_mat = hw_hit.instance_slot < u.material_count;
    if has_mat {
        hit_mat = instance_materials[hw_hit.instance_slot];
    }
    let scene_col = refl_resolve_hit_radiance(
        hit_pos,
        hit_normal,
        trace_dir,
        sample_uv,
        on_screen,
        spacing_px,
        hit_mat,
        has_mat,
        probe_meta,
        t_probe_env,
        s_probe_env,
        rt_light,
        t_shadow,
        s_shadow,
        t_lit_scene,
        t_depth,
        t_direct,
        t_base_color,
        u.resolution,
        u.step_size,
        u.view_proj,
        u.near_plane,
        u.far_plane,
    );

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
