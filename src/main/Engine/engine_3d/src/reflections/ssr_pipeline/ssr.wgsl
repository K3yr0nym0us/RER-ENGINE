// Screen-space reflections — Bevy `bevy_pbr/src/ssr` + `ssr/raymarch.wgsl`.
// https://github.com/bevyengine/bevy/tree/main/crates/bevy_pbr/src/ssr

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
@group(0) @binding(9) var t_ambient : texture_2d<f32>;
@group(0) @binding(10) var t_world_pos : texture_2d<f32>;

struct SsrProbeMeta {
    entries : array<vec4<f32>, 8>,
}

@group(1) @binding(0) var t_probe_env : texture_cube_array<f32>;
@group(1) @binding(1) var s_probe_env : sampler;
@group(1) @binding(2) var<uniform> probe_meta : SsrProbeMeta;

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

fn scene_depth_at_uv(uv : vec2<f32>) -> f32 {
    return scene_depth_nearest_at_uv(uv);
}

// Bevy `depth_sample_nearest` / `depth_sample_linear` (prepass = GL NDC z).
fn ssr_march_depth_nearest(uv : vec2<f32>, tex_size : vec2<f32>) -> f32 {
    _ = tex_size;
    return scene_depth_nearest_at_uv(uv);
}

fn ssr_march_depth_linear(uv : vec2<f32>, tex_size : vec2<f32>) -> f32 {
    _ = tex_size;
    return scene_depth_bilinear_at_uv(uv);
}

fn world_pos_bilinear_at_uv(uv : vec2<f32>) -> vec3<f32> {
    let dims = textureDimensions(t_world_pos);
    let tex_size = vec2<f32>(dims);
    let coord = uv * tex_size - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coord));
    let frac = coord - vec2<f32>(base);

    let max_coord = vec2<i32>(i32(dims.x) - 1, i32(dims.y) - 1);
    let c00 = clamp(base, vec2<i32>(0), max_coord);
    let c10 = clamp(base + vec2<i32>(1, 0), vec2<i32>(0), max_coord);
    let c01 = clamp(base + vec2<i32>(0, 1), vec2<i32>(0), max_coord);
    let c11 = clamp(base + vec2<i32>(1, 1), vec2<i32>(0), max_coord);

    let p00 = textureLoad(t_world_pos, c00, 0).xyz;
    let p10 = textureLoad(t_world_pos, c10, 0).xyz;
    let p01 = textureLoad(t_world_pos, c01, 0).xyz;
    let p11 = textureLoad(t_world_pos, c11, 0).xyz;
    let p0 = mix(p00, p10, frac.x);
    let p1 = mix(p01, p11, frac.x);
    return mix(p0, p1, frac.y);
}

fn world_pos_at_uv(uv : vec2<f32>, depth_prepass : f32) -> vec3<f32> {
    // Rgba16Float no es filterable en wgpu: bilinear manual con textureLoad.
    let wp = world_pos_bilinear_at_uv(uv);
    if dot(wp, wp) > 1e-8 {
        return wp;
    }
    let depth = scene_depth_bilinear_at_uv(uv);
    return refl_world_pos_from_depth(uv, depth, u.inv_view_proj, u.near_plane, u.far_plane);
}

/// Posición coherente con prepass depth + sesgo hacia afuera para reflect().
fn ssr_reflect_surface(
    uv : vec2<f32>,
    depth_prepass : f32,
    n_world : vec3<f32>,
) -> vec3<f32> {
    return ssr_reflect_surface_at_uv(
        uv,
        depth_prepass,
        n_world,
        u.inv_view_proj,
        u.near_plane,
        u.far_plane,
    );
}

fn ssr_reflected_radiance(hit_uv : vec2<f32>) -> vec3<f32> {
    // Bevy `color_texture`: lit-composite lineal; peso Fresnel/metal va en alpha/miss, no aquí.
    return textureSampleLevel(t_lit_scene, s_linear, hit_uv, 0.0).rgb;
}

/// Peso especular SSR: en metales el F0 oscuro no debe anular el trazo (Bevy usa BRDF/pdf).
fn ssr_specular_weight(trace_w : f32, roughness : f32, metallic : f32) -> f32 {
    if metallic > 0.5 {
        let r = clamp(roughness, 0.0, 1.0);
        let glossy = (1.0 - r) * (1.0 - r);
        return max(trace_w, glossy * 0.8);
    }
    return trace_w;
}

/// Fallback en miss SSR: cubemap probe (Bevy env map) o procedural si no hay probe.
fn ssr_environment_fallback_radiance(
    world_pos : vec3<f32>,
    cam_world : vec3<f32>,
    n_world : vec3<f32>,
    R_world : vec3<f32>,
    albedo : vec3<f32>,
    metallic : f32,
    roughness : f32,
    spec_w : f32,
) -> vec3<f32> {
    let layer_i = refl_nearest_probe_layer_entries(world_pos, probe_meta.entries);
    var env : vec3<f32>;
    if layer_i >= 0 {
        let sample_dir = refl_cubemap_sample_dir(
            world_pos,
            cam_world,
            n_world,
            layer_i,
            probe_meta.entries,
        );
        let lod = refl_env_cubemap_lod(roughness);
        env = textureSampleLevel(t_probe_env, s_probe_env, sample_dir, layer_i, lod).rgb;
    } else {
        env = refl_fake_environment(normalize(R_world));
    }
    var rgb = refl_metal_attenuate(env, albedo, metallic);
    let lum = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    rgb = mix(vec3<f32>(lum), rgb, 1.0 - roughness * 0.4);
    return rgb * spec_w;
}

fn ssr_box_blur_radiance(uv : vec2<f32>, spacing_px : f32) -> vec3<f32> {
    let texel = vec2<f32>(1.0) / u.gb_resolution;
    var acc = vec3<f32>(0.0);
    for (var oy = -3; oy <= 3; oy++) {
        for (var ox = -3; ox <= 3; ox++) {
            let p = uv + vec2<f32>(f32(ox), f32(oy)) * spacing_px * texel;
            acc += ssr_reflected_radiance(p);
        }
    }
    return acc / 49.0;
}

@fragment
fn fs_main(in : VsOut) -> SsrOut {
    var result : SsrOut;
    result.reflection = vec4<f32>(0.0);
    result.hit_uv = vec4<f32>(-1.0, -1.0, 0.0, 0.0);

    let depth_prepass = scene_depth_at_uv(in.uv);
    if refl_depth_prepass_invalid(depth_prepass) {
        return result;
    }

    let surf_px = texel_px(in.uv, textureDimensions(t_surface));
    let roughness = textureLoad(t_surface, surf_px, 0).g;
    if roughness > u.max_roughness {
        return result;
    }

    let direct_px = texel_px(in.uv, textureDimensions(t_direct));
    let direct_s = textureLoad(t_direct, direct_px, 0);
    let metallic = direct_s.a;
    let ior = select(0.0, direct_s.b * 10.0, direct_s.b > 0.01);
    let albedo = textureLoad(t_base_color, surf_px, 0).rgb;

    let n_world = decode_octahedral(
        textureLoad(t_normal_roughness, texel_px(in.uv, textureDimensions(t_normal_roughness)), 0).zw,
    );
    let world_pos = world_pos_at_uv(in.uv, depth_prepass);
    let cam_world = (u.inv_view * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;

    let V_world = ssr_bevy_view_dir_world(cam_world, world_pos);
    let trace_w = refl_trace_strength(roughness, metallic, n_world, V_world, albedo, ior);

    // P desde G-buffer; start_cs.z = prepass (no clip.z/w — distinto de @builtin(position).z en wgpu).
    let R_world = ssr_bevy_reflection_world(cam_world, world_pos, n_world);
    let start_cs = ssr_ray_start_cs(world_pos, depth_prepass, u.view_proj);

    let spec_w = ssr_specular_weight(trace_w, roughness, metallic);

    var hit = ssr_evaluate_bevy(
        R_world,
        start_cs,
        1.0,
        u.view_proj,
        max(u.coarse_max_iters, 2u),
        u.binary_steps,
        u.thickness_m,
        u.near_plane,
    );

    var reflected : vec3<f32>;
    if hit.found {
        let blur_spacing = roughness * roughness * 4.0 + 1.0;
        let color_sharp = ssr_reflected_radiance(hit.hit_uv);
        let color_blur = ssr_box_blur_radiance(hit.hit_uv, blur_spacing);
        reflected = mix(color_sharp, color_blur, clamp(roughness, 0.0, 1.0));
    } else {
        reflected = ssr_environment_fallback_radiance(
            world_pos,
            cam_world,
            n_world,
            R_world,
            albedo,
            metallic,
            roughness,
            spec_w,
        );
    }

    // spec_w solo en miss (fallback); hits Bevy no multiplican radiance por Fresnel.
    let refl_rgb = select(reflected * spec_w, reflected, hit.found);
    let refl_lum = dot(refl_rgb, vec3<f32>(0.2126, 0.7152, 0.0722));

    result.reflection = vec4<f32>(refl_rgb, max(spec_w, saturate(refl_lum * 2.0)));
    if hit.found {
        result.hit_uv = vec4<f32>(hit.hit_uv, 1.0, 1.0);
    }
    return result;
}
