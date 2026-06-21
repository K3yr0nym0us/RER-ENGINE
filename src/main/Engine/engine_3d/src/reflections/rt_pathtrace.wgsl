struct PathUniforms {
    inv_view_proj : mat4x4<f32>,
    view_proj     : mat4x4<f32>,
    cam_pos       : vec4<f32>,
    resolution    : vec2<f32>,
    node_count    : u32,
    tri_count     : u32,
    max_distance_m : f32,
    near_plane    : f32,
    far_plane     : f32,
    frame_index   : u32,
    max_bounces   : u32,
    material_count : u32,
    _pad0 : u32,
    _pad1 : u32,
}

struct BvhNode {
    bbox_min : vec4<f32>,
    bbox_max : vec4<f32>,
    left_or_tri_offset : u32,
    right_or_tri_count : u32,
    flags : u32,
    _pad : u32,
}

struct BvhHit {
    found : bool,
    t : f32,
    tri_idx : u32,
}

@group(0) @binding(0) var<uniform> u : PathUniforms;
@group(0) @binding(1) var<storage, read> bvh_nodes : array<BvhNode>;
@group(0) @binding(2) var<storage, read> bvh_tris : array<RtTri>;
@group(0) @binding(3) var path_out : texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var t_depth : texture_2d<f32>;
@group(0) @binding(5) var t_surface : texture_2d<f32>;
@group(0) @binding(6) var t_direct : texture_2d<f32>;
@group(0) @binding(7) var t_base_color : texture_2d<f32>;
@group(0) @binding(8) var<storage, read> instance_materials : array<RtInstanceMaterial>;

@group(1) @binding(0) var t_probe_env : texture_cube_array<f32>;
@group(1) @binding(1) var s_probe_env : sampler;
@group(1) @binding(2) var<uniform> probe_meta : ReflProbeMeta;

@group(2) @binding(0) var t_shadow : texture_depth_2d;
@group(2) @binding(1) var s_shadow : sampler_comparison;
@group(2) @binding(2) var<uniform> rt_light : RtLightUniform;

fn world_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, view_depth_m, u.inv_view_proj, u.near_plane, u.far_plane);
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
    let u_coord = dot(tvec, pvec) * inv_det;
    if u_coord < 0.0 || u_coord > 1.0 {
        return -1.0;
    }
    let qvec = cross(tvec, edge1);
    let v_coord = dot(dir, qvec) * inv_det;
    if v_coord < 0.0 || u_coord + v_coord > 1.0 {
        return -1.0;
    }
    let t = dot(edge2, qvec) * inv_det;
    if t < REFL_RAY_T_MIN || t > t_max {
        return -1.0;
    }
    return t;
}

fn bvh_trace(origin : vec3<f32>, dir : vec3<f32>, max_t : f32) -> BvhHit {
    var hit : BvhHit;
    hit.found = false;
    hit.t = max_t;
    hit.tri_idx = 0u;
    if u.node_count == 0u {
        return hit;
    }
    var stack : array<u32, 32>;
    var sp = 0u;
    stack[sp] = 0u;
    sp += 1u;
    loop {
        if sp == 0u {
            break;
        }
        sp -= 1u;
        let node_idx = stack[sp];
        if node_idx >= u.node_count {
            continue;
        }
        let node = bvh_nodes[node_idx];
        if ray_aabb(origin, dir, node.bbox_min.xyz, node.bbox_max.xyz, hit.t) < 0.0 {
            continue;
        }
        if node.flags == 1u {
            let tri_start = node.left_or_tri_offset;
            let tri_count = node.right_or_tri_count;
            for (var ti = 0u; ti < tri_count; ti++) {
                let idx = tri_start + ti;
                if idx >= u.tri_count {
                    continue;
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
            if sp + 2u <= 32u {
                stack[sp] = node.right_or_tri_count;
                sp += 1u;
                stack[sp] = node.left_or_tri_offset;
                sp += 1u;
            }
        }
    }
    return hit;
}

fn path_sky(dir : vec3<f32>) -> vec3<f32> {
    let t = 0.5 * (dir.y + 1.0);
    return mix(vec3<f32>(0.05, 0.06, 0.08), vec3<f32>(0.45, 0.55, 0.72), t);
}

fn path_trace_ray(origin : vec3<f32>, dir : vec3<f32>, seed : vec2<f32>) -> vec3<f32> {
    if u.max_bounces == 0u {
        return path_sky(normalize(dir));
    }

    var ray_origin = origin;
    var ray_dir = normalize(dir);
    var cur_seed = seed;
    var accumulated = vec3<f32>(0.0);
    var weight = vec3<f32>(1.0);
    let light = normalize(rt_light.light_dir.xyz);

    for (var depth = 0u; depth < u.max_bounces; depth++) {
        let hit = bvh_trace(ray_origin, ray_dir, u.max_distance_m);
        if !hit.found {
            return accumulated + weight * path_sky(ray_dir);
        }

        let tri = bvh_tris[hit.tri_idx];
        let hit_pos = ray_origin + ray_dir * hit.t;
        let n = refl_tri_normal(tri);
        let slot = refl_tri_instance_slot(tri);
        var albedo = vec3<f32>(0.7);
        var roughness = 0.5;
        var metallic = 0.0;
        if slot < u.material_count {
            let mat = instance_materials[slot];
            albedo = mat.albedo.xyz;
            roughness = mat.pbr.x;
            metallic = mat.pbr.y;
        }

        let shadow = refl_rt_shadow_at(hit_pos, n, rt_light, t_shadow, s_shadow);
        let ndotl = max(dot(n, light), 0.0);
        let direct = albedo * ndotl * shadow * rt_light.light_color.xyz * rt_light.light_params.x;
        accumulated += weight * direct;

        if metallic > 0.5 {
            weight = weight * albedo;
            ray_dir = refl_fuzzy_mirror_dir_temporal(
                hit_pos,
                u.cam_pos.xyz,
                n,
                roughness,
                u.frame_index + depth,
                cur_seed,
            );
        } else {
            weight = weight * albedo * 0.45;
            ray_dir = normalize(n + vec3<f32>(cur_seed.x - 0.5, cur_seed.y - 0.5, 0.5 - cur_seed.x));
        }
        ray_origin = hit_pos + n * 0.02;
        cur_seed = cur_seed + vec2<f32>(0.11, 0.23);
    }

    return accumulated + weight * path_sky(ray_dir);
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    if gid.x >= u32(u.resolution.x) || gid.y >= u32(u.resolution.y) {
        return;
    }
    let px = vec2<i32>(i32(gid.x), i32(gid.y));
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / u.resolution;
    let view_depth_m = textureLoad(t_depth, px, 0).r;
    if view_depth_m <= 0.0001 {
        textureStore(path_out, px, vec4<f32>(0.0));
        return;
    }
    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let dir = normalize(world_pos - u.cam_pos.xyz);
    let seed = refl_blue_noise_seed(u.frame_index, uv);
    let col = path_trace_ray(world_pos + dir * 0.01, -dir, seed);
    textureStore(path_out, px, vec4<f32>(col, 1.0));
}
