struct RtUniforms {
    inv_view_proj   : mat4x4<f32>,
    view_proj       : mat4x4<f32>,
    cam_pos         : vec4<f32>,
    resolution      : vec2<f32>,
    instance_count  : u32,
    max_distance_m  : f32,
    max_roughness   : f32,
    rt_blend        : f32,
    step_size       : f32,
    near_plane      : f32,
    far_plane       : f32,
}

struct StaticInstance {
    min : vec4<f32>,
    max : vec4<f32>,
}

@group(0) @binding(0) var<uniform> u : RtUniforms;
@group(0) @binding(1) var<storage, read> instances : array<StaticInstance>;
@group(0) @binding(2) var reflection_out : texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(8) var t_ssr : texture_2d<f32>;
@group(0) @binding(3) var t_depth : texture_2d<f32>;
@group(0) @binding(4) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(5) var t_lit_scene : texture_2d<f32>;
@group(0) @binding(6) var t_direct : texture_2d<f32>;
@group(0) @binding(7) var s_nearest : sampler;
@group(0) @binding(9) var t_surface : texture_2d<f32>;
@group(0) @binding(10) var t_base_color : texture_2d<f32>;

fn world_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, view_depth_m, u.inv_view_proj, u.near_plane, u.far_plane);
}

fn view_depth_m_from_world(world : vec3<f32>) -> f32 {
    return refl_view_depth_m_from_world(world, u.view_proj, u.near_plane, u.far_plane);
}

fn project_uv(world : vec3<f32>) -> vec2<f32> {
    return refl_project_uv(world, u.view_proj);
}

fn ray_aabb(origin : vec3<f32>, dir : vec3<f32>, bmin : vec3<f32>, bmax : vec3<f32>) -> f32 {
    var tmin = -1e20;
    var tmax = 1e20;
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
    if tmin > 0.0 {
        return tmin;
    }
    return -1.0;
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
    let px = texel_px(uv);
    return textureLoad(t_base_color, px, 0).rgb;
}

fn lit_scene_at(uv : vec2<f32>) -> vec3<f32> {
    let px = vec2<i32>(uv * u.resolution);
    return textureLoad(t_lit_scene, px, 0).rgb;
}

fn lit_scene_blurred(uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    let ref_depth_m = textureLoad(t_depth, vec2<i32>(uv * u.resolution), 0).r;
    if ref_depth_m <= 0.0001 {
        return lit_scene_at(uv);
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
        return lit_scene_at(uv);
    }
    return acc / wsum;
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    if gid.x >= u32(u.resolution.x) || gid.y >= u32(u.resolution.y) {
        return;
    }
    let px = vec2<i32>(i32(gid.x), i32(gid.y));
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / u.resolution;

    let ssr = textureLoad(t_ssr, px, 0);

    let view_depth_m = textureLoad(t_depth, px, 0).r;
    if view_depth_m <= 0.0001 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let packed = textureLoad(t_normal_roughness, px, 0);
    let n = normal_from_packed(packed);
    let roughness = textureLoad(t_surface, px, 0).g;
    let metallic  = textureLoad(t_direct, px, 0).a;
    let albedo = base_color_at(uv);
    if roughness > u.max_roughness {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let v = normalize(u.cam_pos.xyz - world_pos);
    let refl_dir = refl_fuzzy_mirror_dir(world_pos, u.cam_pos.xyz, n, roughness);
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

    var closest = u.max_distance_m + 1.0;
    var hit = false;
    let count = min(u.instance_count, arrayLength(&instances));
    for (var i = 0u; i < count; i++) {
        let inst = instances[i];
        let t = ray_aabb(ray_origin, refl_dir, inst.min.xyz, inst.max.xyz);
        if t > 0.001 && t < closest {
            closest = t;
            hit = true;
        }
    }
    if !hit {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let hit_pos = ray_origin + refl_dir * closest;
    let sample_uv = project_uv(hit_pos);
    if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let hit_px = vec2<i32>(sample_uv * u.resolution);
    let hit_depth_m = textureLoad(t_depth, hit_px, 0).r;
    let ray_depth_m = view_depth_m_from_world(hit_pos);
    if abs(ray_depth_m - hit_depth_m) > refl_rt_hit_depth_reject_m(u.step_size) {
        textureStore(reflection_out, px, ssr);
        return;
    }

    let spacing_px = refl_blur_spacing_px(roughness, closest);
    let scene_col = refl_metal_attenuate(
        lit_scene_blurred(sample_uv, spacing_px),
        albedo,
        metallic,
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
