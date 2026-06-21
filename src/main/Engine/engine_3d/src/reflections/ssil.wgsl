struct SsilUniforms {
    inv_view_proj : mat4x4<f32>,
    view_proj     : mat4x4<f32>,
    cam_pos       : vec4<f32>,
    resolution    : vec2<f32>,
    near_plane    : f32,
    far_plane     : f32,
    sample_radius : f32,
    strength      : f32,
    depth_reject_m : f32,
    gbuffer_scale : f32,
}

@group(0) @binding(0) var<uniform> u : SsilUniforms;
@group(0) @binding(1) var t_depth : texture_2d<f32>;
@group(0) @binding(2) var t_normal_roughness : texture_2d<f32>;
@group(0) @binding(3) var t_lit_scene : texture_2d<f32>;
@group(0) @binding(4) var t_direct : texture_2d<f32>;
@group(0) @binding(5) var t_surface : texture_2d<f32>;
@group(0) @binding(6) var ssil_out : texture_storage_2d<rgba8unorm, write>;

// Poisson fijo en pantalla (sin rotación temporal — evita "fantasma" girando).
const SSIL_SAMPLES : array<vec2<f32>, 8> = array<vec2<f32>, 8>(
    vec2<f32>(0.18, 0.04),
    vec2<f32>(-0.14, 0.16),
    vec2<f32>(-0.08, -0.18),
    vec2<f32>(0.22, -0.12),
    vec2<f32>(-0.20, -0.06),
    vec2<f32>(0.06, 0.22),
    vec2<f32>(0.12, -0.20),
    vec2<f32>(-0.22, 0.10),
);

fn world_pos_from_depth(uv : vec2<f32>, view_depth_m : f32) -> vec3<f32> {
    return refl_world_pos_from_depth(uv, view_depth_m, u.inv_view_proj, u.near_plane, u.far_plane);
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

fn normal_from_packed(packed : vec4<f32>) -> vec3<f32> {
    return decode_octahedral(packed.zw);
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid : vec3<u32>) {
    if gid.x >= u32(u.resolution.x) || gid.y >= u32(u.resolution.y) {
        return;
    }
    let px = vec2<i32>(i32(gid.x), i32(gid.y));
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / u.resolution;
    let gb_px = vec2<i32>(vec2<f32>(px) * u.gbuffer_scale);
    let view_depth_m = textureLoad(t_depth, gb_px, 0).r;
    if view_depth_m <= 0.0001 {
        textureStore(ssil_out, px, vec4<f32>(0.0));
        return;
    }

    let roughness = textureLoad(t_surface, gb_px, 0).g;
    let metallic = textureLoad(t_direct, gb_px, 0).a;
    if metallic > 0.5 || roughness < 0.35 {
        textureStore(ssil_out, px, vec4<f32>(0.0));
        return;
    }

    let packed = textureLoad(t_normal_roughness, gb_px, 0);
    let n = normal_from_packed(packed);
    let world_pos = world_pos_from_depth(uv, view_depth_m);
    let base_lit = textureLoad(t_lit_scene, gb_px, 0).rgb;

    var accum = vec3<f32>(0.0);
    var weight_sum = 0.0;
    let texel = vec2<f32>(1.0) / u.resolution;
    let radius = u.sample_radius * texel;

    for (var i = 0; i < 8; i++) {
        let offset = SSIL_SAMPLES[i] * radius;
        let sample_uv = uv + offset;
        if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
            continue;
        }
        let sample_px = vec2<i32>(
            i32(sample_uv.x * u.resolution.x * u.gbuffer_scale),
            i32(sample_uv.y * u.resolution.y * u.gbuffer_scale),
        );
        let sample_depth = textureLoad(t_depth, sample_px, 0).r;
        if sample_depth <= 0.0001 {
            continue;
        }
        if abs(sample_depth - view_depth_m) > u.depth_reject_m {
            continue;
        }
        let sample_world = world_pos_from_depth(sample_uv, sample_depth);
        let to_sample = sample_world - world_pos;
        let dist = length(to_sample);
        if dist < 0.05 || dist > 3.0 {
            continue;
        }
        let sample_n = normal_from_packed(textureLoad(t_normal_roughness, sample_px, 0));
        let dir = normalize(to_sample);
        let ndotl = max(dot(n, dir), 0.0);
        let facing = max(dot(sample_n, -dir), 0.0);
        if ndotl < 0.05 || facing < 0.05 {
            continue;
        }
        let sample_lit = textureLoad(t_lit_scene, sample_px, 0).rgb;
        let w = ndotl * facing / (1.0 + dist * 0.5);
        accum += sample_lit * w;
        weight_sum += w;
    }

    var indirect = vec3<f32>(0.0);
    if weight_sum > 1e-4 {
        let neighbor = accum / weight_sum;
        indirect = max(neighbor - base_lit, vec3<f32>(0.0)) * u.strength;
    }
    textureStore(ssil_out, px, vec4<f32>(indirect, 1.0));
}
