const RT_TILE_SIZE : u32 = 16u;
const MAX_RT_TILES : u32 = 4096u;

struct MaskUniforms {
    resolution    : vec2<f32>,
    max_roughness : f32,
    threshold     : f32,
    tiles_x       : u32,
    tiles_y       : u32,
    gbuffer_scale : f32,
    _pad          : f32,
}

@group(0) @binding(0) var<uniform> u : MaskUniforms;
@group(0) @binding(1) var<storage, read_write> tile_count : atomic<u32>;
@group(0) @binding(2) var<storage, read_write> tile_list : array<u32>;
@group(0) @binding(3) var t_ssr : texture_2d<f32>;
@group(0) @binding(4) var t_depth : texture_2d<f32>;
@group(0) @binding(5) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(6) var t_surface : texture_2d<f32>;
@group(0) @binding(7) var t_direct : texture_2d<f32>;

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

@compute @workgroup_size(1, 1, 1)
fn cs_build_mask(@builtin(global_invocation_id) gid : vec3<u32>) {
    let tx = gid.x;
    let ty = gid.y;
    if tx >= u.tiles_x || ty >= u.tiles_y {
        return;
    }

    var need_rt = false;
    let base_x = tx * RT_TILE_SIZE;
    let base_y = ty * RT_TILE_SIZE;

    for (var oy = 0u; oy < RT_TILE_SIZE; oy += 4u) {
        for (var ox = 0u; ox < RT_TILE_SIZE; ox += 4u) {
            let px = vec2<i32>(
                i32(min(base_x + ox, u32(u.resolution.x) - 1u)),
                i32(min(base_y + oy, u32(u.resolution.y) - 1u)),
            );
            let gb_px = vec2<i32>(vec2<f32>(px) * u.gbuffer_scale);
            let ssr = textureLoad(t_ssr, px, 0);
            if (1.0 - ssr.a) * ssr.a < 0.0001 && ssr.a > 0.95 {
                continue;
            }
            let view_depth_m = textureLoad(t_depth, gb_px, 0).r;
            if view_depth_m <= 0.0001 {
                continue;
            }
            let packed = textureLoad(t_normal_roughness, gb_px, 0);
            let n = normal_from_packed(packed);
            let roughness = textureLoad(t_surface, gb_px, 0).g;
            let metallic = textureLoad(t_direct, gb_px, 0).a;
            if roughness > u.max_roughness || metallic < 0.05 {
                continue;
            }
            let strength = (1.0 - roughness) * (1.0 - roughness) * metallic;
            let rt_weight = (1.0 - ssr.a) * strength;
            if rt_weight > u.threshold {
                need_rt = true;
                break;
            }
        }
        if need_rt {
            break;
        }
    }

    if !need_rt {
        return;
    }

    let idx = atomicAdd(&tile_count, 1u);
    if idx >= MAX_RT_TILES {
        return;
    }
    tile_list[idx * 2u] = tx;
    tile_list[idx * 2u + 1u] = ty;
}
