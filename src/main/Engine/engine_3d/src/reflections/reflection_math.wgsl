// Shared depth / projection math for SSR, RT and debug passes.
// Must stay inverse-consistent with shader.wgsl `ndc_z_to_view_depth_m` / G-buffer export.

/// RTIOW hit interval lower bound (shadow acne / self-intersection).
const REFL_RAY_T_MIN : f32 = 0.001;

fn refl_uv_to_ndc_xy(uv : vec2<f32>) -> vec2<f32> {
    return vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
}

fn refl_ndc_xy_to_uv(ndc_xy : vec2<f32>) -> vec2<f32> {
    var uv = ndc_xy * 0.5 + vec2<f32>(0.5);
    uv.y = 1.0 - uv.y;
    return uv;
}

fn refl_view_depth_to_ndc_z_vk(view_depth_m : f32, near_plane : f32, far_plane : f32) -> f32 {
    return (far_plane - near_plane * far_plane / view_depth_m) / (far_plane - near_plane);
}

fn refl_vk_ndc_z_to_gl(ndc_z_vk : f32) -> f32 {
    return ndc_z_vk * 2.0 - 1.0;
}

fn refl_gl_ndc_z_to_vk(ndc_z_gl : f32) -> f32 {
    return ndc_z_gl * 0.5 + 0.5;
}

/// Inverse of `world_pos_from_depth`: clip.z/clip.w → Vulkan NDC z → linear view depth (m).
fn refl_view_depth_m_from_world(
    world : vec3<f32>,
    view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> f32 {
    let clip = view_proj * vec4<f32>(world, 1.0);
    if clip.w <= 0.0 {
        return -1.0;
    }
    let ndc_z_gl = clip.z / clip.w;
    let ndc_z_vk = refl_gl_ndc_z_to_vk(ndc_z_gl);
    return (near_plane * far_plane) / (far_plane - ndc_z_vk * (far_plane - near_plane));
}

fn refl_project_uv(world : vec3<f32>, view_proj : mat4x4<f32>) -> vec2<f32> {
    let clip = view_proj * vec4<f32>(world, 1.0);
    if clip.w <= 0.0 {
        return vec2<f32>(-1.0);
    }
    return refl_ndc_xy_to_uv(clip.xy / clip.w);
}

fn refl_world_pos_from_depth(
    uv : vec2<f32>,
    view_depth_m : f32,
    inv_view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> vec3<f32> {
    let ndc_z_vk = refl_view_depth_to_ndc_z_vk(view_depth_m, near_plane, far_plane);
    let ndc_z_gl = refl_vk_ndc_z_to_gl(ndc_z_vk);
    let world_h = inv_view_proj * vec4<f32>(refl_uv_to_ndc_xy(uv), ndc_z_gl, 1.0);
    return world_h.xyz / world_h.w;
}

/// RTIOW metal.rs: reflect(ray toward surface, outward normal).
fn refl_mirror_dir(world_pos : vec3<f32>, cam_pos : vec3<f32>, n : vec3<f32>) -> vec3<f32> {
    let incident = normalize(world_pos - cam_pos);
    let nn = normalize(n);
    return reflect(incident, nn);
}

/// Deterministic pseudo-random unit vector (RTIOW `random_in_unit_sphere` sin RNG global).
fn refl_random_unit_vector(seed : vec2<f32>) -> vec3<f32> {
    let h1 = fract(sin(dot(seed, vec2<f32>(127.1, 311.7))) * 43758.5453);
    let h2 = fract(sin(dot(seed + vec2<f32>(17.0, 31.0), vec2<f32>(269.5, 183.3))) * 43758.5453);
    let h3 = fract(sin(dot(seed + vec2<f32>(53.0, 97.0), vec2<f32>(419.2, 371.9))) * 43758.5453);
    let z = 1.0 - 2.0 * h1;
    let r = sqrt(max(1.0 - z * z, 0.0));
    let phi = 6.2831853 * h2;
    return vec3<f32>(r * cos(phi), r * sin(phi), z);
}

/// RTIOW Book 1 fuzzy metal: `normalize(reflect) + fuzz * random_unit_vector`.
fn refl_metal_fuzz_from_roughness(roughness : f32) -> f32 {
    return clamp(roughness, 0.0, 1.0) * 0.5;
}

fn refl_apply_fuzzy_metal(reflected : vec3<f32>, n : vec3<f32>, fuzz : f32, seed : vec2<f32>) -> vec3<f32> {
    let nn = normalize(n);
    var refl = normalize(reflected);
    if dot(refl, nn) <= 0.0 {
        return vec3<f32>(0.0);
    }
    if fuzz <= 1e-6 {
        return refl;
    }
    let scattered = normalize(refl + fuzz * refl_random_unit_vector(seed));
    if dot(scattered, nn) <= 0.0 {
        return vec3<f32>(0.0);
    }
    return scattered;
}

/// Dirección de traza metálica (SSR/RT): RTIOW fuzzy + rechazo grazing.
fn refl_fuzzy_mirror_dir(
    world_pos : vec3<f32>,
    cam_pos : vec3<f32>,
    n : vec3<f32>,
    roughness : f32,
    seed : vec2<f32>,
) -> vec3<f32> {
    let refl = refl_mirror_dir(world_pos, cam_pos, n);
    let fuzz = refl_metal_fuzz_from_roughness(roughness);
    return refl_apply_fuzzy_metal(refl, n, fuzz, seed);
}

/// Fuzzy con semilla blue-noise temporal (frame + UV).
fn refl_blue_noise_seed(frame_index : u32, uv : vec2<f32>) -> vec2<f32> {
    let f = f32(frame_index) * 0.6180339887;
    return vec2<f32>(uv.x + f * 0.173, uv.y + f * 0.271);
}

fn refl_fuzzy_mirror_dir_temporal(
    world_pos : vec3<f32>,
    cam_pos : vec3<f32>,
    n : vec3<f32>,
    roughness : f32,
    frame_index : u32,
    uv : vec2<f32>,
) -> vec3<f32> {
    return refl_fuzzy_mirror_dir(
        world_pos,
        cam_pos,
        n,
        roughness,
        refl_blue_noise_seed(frame_index, uv),
    );
}

/// Mip LOD del cubemap alineado al fuzz RTIOW (split-sum aproximado).
fn refl_env_cubemap_lod(roughness : f32) -> f32 {
    let r = clamp(roughness, 0.0, 1.0);
    let fuzz = refl_metal_fuzz_from_roughness(r);
    return clamp(r * r * 4.0 + fuzz * 2.0, 0.0, 4.0);
}

/// Blur kernel spacing (px): más rugosidad → kernel más ancho en SSR/composite.
fn refl_blur_spacing_px(roughness : f32, hit_dist_m : f32) -> f32 {
    let dist_term = min(hit_dist_m * 0.06, 0.5);
    let r = clamp(roughness, 0.0, 1.0);
    return clamp(r * r * 8.0 + dist_term, 0.0, 6.0);
}

/// F0 coherente con forward (`shader.wgsl`: albedo en metales, 0.04 en dieléctricos).
fn refl_metal_f0(albedo_rgb : vec3<f32>, metallic : f32) -> vec3<f32> {
    let dielectric = vec3<f32>(0.04);
    return mix(dielectric, albedo_rgb, clamp(metallic, 0.0, 1.0));
}

fn refl_fresnel_schlick_vec3(cos_theta : f32, f0 : vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

fn refl_fresnel_schlick_scalar(cos_theta : f32, f0 : f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - cos_theta, 5.0);
}

/// Máscara SSR/RT (alpha): RTIOW Fresnel + rugosidad, sin metallic_boost artístico.
fn refl_trace_strength(
    roughness : f32,
    metallic : f32,
    n : vec3<f32>,
    view_dir : vec3<f32>,
    albedo_rgb : vec3<f32>,
) -> f32 {
    let f0 = refl_metal_f0(albedo_rgb, metallic);
    let cos_theta = max(dot(normalize(n), normalize(view_dir)), 0.0);
    let fres = refl_fresnel_schlick_vec3(cos_theta, f0);
    let fres_w = dot(fres, vec3<f32>(0.2126, 0.7152, 0.0722));
    let rough_term = (1.0 - roughness) * (1.0 - roughness);
    return rough_term * fres_w;
}

fn refl_normal_bias(step_size : f32) -> f32 {
    return max(REFL_RAY_T_MIN, max(0.002, step_size * 0.05));
}

fn refl_depth_reject_m(step_size : f32) -> f32 {
    return max(0.06, step_size * 0.75);
}

fn refl_thickness_m(step_size : f32, roughness : f32) -> f32 {
    return max(step_size * 1.5, 0.04) + roughness * 0.06;
}

/// Tolerancia reproyección depth tras hit de triángulo (superficie real, no AABB).
fn refl_rt_hit_depth_reject_m(step_size : f32) -> f32 {
    return max(0.12, step_size * 1.25);
}

/// RTIOW metal.rs: color del rebote *= albedo del metal en el punto de reflexión.
fn refl_metal_attenuate(hit_rgb : vec3<f32>, albedo_rgb : vec3<f32>, metallic : f32) -> vec3<f32> {
    if metallic > 0.5 {
        return hit_rgb * albedo_rgb;
    }
    return hit_rgb;
}

const REFL_MAX_PROBES : u32 = 8u;

struct RtTri {
    v0 : vec4<f32>,
    v1 : vec4<f32>,
    v2 : vec4<f32>,
}

struct ReflProbeMeta {
    entries : array<vec4<f32>, 8>,
}

/// Entorno procedural (mismo gradiente que shader.wgsl fake_environment).
fn refl_fake_environment(refl_dir : vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(0.50, 0.57, 0.72);
    let horizon = vec3<f32>(0.82, 0.84, 0.88);
    let ground = vec3<f32>(0.26, 0.27, 0.29);
    let y = clamp(refl_dir.y, -1.0, 1.0);
    if y >= 0.0 {
        return mix(horizon, sky, pow(y, 0.45));
    }
    return mix(horizon, ground, pow(-y, 0.55));
}

fn refl_nearest_probe_layer(hit_pos : vec3<f32>, probe_meta : ReflProbeMeta) -> i32 {
    var best_i = -1;
    var best_d = 1e30;
    for (var i = 0u; i < REFL_MAX_PROBES; i++) {
        let e = probe_meta.entries[i];
        if e.w <= 0.0 {
            continue;
        }
        let d = distance(hit_pos, e.xyz);
        if d < best_d {
            best_d = d;
            best_i = i32(i);
        }
    }
    return best_i;
}

fn refl_sample_probe_at_hit(
    hit_pos : vec3<f32>,
    sample_dir : vec3<f32>,
    roughness : f32,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
) -> vec3<f32> {
    let dir_n = normalize(sample_dir);
    let layer_i = refl_nearest_probe_layer(hit_pos, probe_meta);
    if layer_i >= 0 {
        let lod = refl_env_cubemap_lod(roughness);
        return textureSampleLevel(t_probe, s_probe, dir_n, layer_i, lod).rgb;
    }
    return refl_fake_environment(dir_n);
}

fn refl_sample_probe_for_material(
    hit_pos : vec3<f32>,
    sample_dir : vec3<f32>,
    roughness : f32,
    mat : RtInstanceMaterial,
    has_material : bool,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
) -> vec3<f32> {
    let dir_n = normalize(sample_dir);
    var layer_i = -1;
    if has_material && mat.probe.x >= 0.0 {
        layer_i = i32(mat.probe.x);
    }
    if layer_i < 0 {
        layer_i = refl_nearest_probe_layer(hit_pos, probe_meta);
    }
    if layer_i >= 0 {
        let lod = refl_env_cubemap_lod(roughness);
        return textureSampleLevel(t_probe, s_probe, dir_n, layer_i, lod).rgb;
    }
    return refl_fake_environment(dir_n);
}

// ── RT Hit Lighting lite (Fase A/D/C) ────────────────────────────────────────

const RT_MAT_FLAG_DIELECTRIC : u32 = 1u;

struct RtInstanceMaterial {
    albedo : vec4<f32>,
    pbr : vec4<f32>,
    /// x = probe layer (-1 = nearest to hit), yzw unused
    probe : vec4<f32>,
}

struct RtLightUniform {
    light_dir : vec4<f32>,
    light_view_proj : mat4x4<f32>,
    light_params : vec4<f32>,
    shadow_bias : vec4<f32>,
    light_color : vec4<f32>,
    rt_flags : vec4<f32>,
}

fn refl_tri_normal(tri : RtTri) -> vec3<f32> {
    let e1 = tri.v1.xyz - tri.v0.xyz;
    let e2 = tri.v2.xyz - tri.v0.xyz;
    return normalize(cross(e1, e2));
}

fn refl_tri_instance_slot(tri : RtTri) -> u32 {
    return bitcast<u32>(tri.v0.w);
}

fn refl_rt_shadow_at(
    world_pos : vec3<f32>,
    world_normal : vec3<f32>,
    rt_light : RtLightUniform,
    t_shadow : texture_depth_2d,
    s_shadow : sampler_comparison,
) -> f32 {
    if rt_light.light_color.w <= 0.5 {
        return 1.0;
    }
    var l = rt_light.light_dir.xyz;
    if dot(l, l) < 1e-6 {
        l = vec3<f32>(0.45, 1.0, 0.35);
    }
    l = normalize(l);
    let n = normalize(world_normal);
    let ndotl = max(dot(n, l), 0.0);
    let normal_scale = rt_light.shadow_bias.x + rt_light.shadow_bias.y * (1.0 - ndotl);
    let biased_pos = world_pos + l * normal_scale;
    let clip = rt_light.light_view_proj * vec4<f32>(biased_pos, 1.0);
    let ndc = clip.xyz / clip.w;
    var uv = ndc.xy * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    let slope = sqrt(max(1.0 - ndotl * ndotl, 0.0));
    let depth_ref = ndc.z - (rt_light.shadow_bias.z + rt_light.shadow_bias.w * slope);
    let texel = rt_light.light_params.z;
    let radius = rt_light.light_params.w;
    var sum = 0.0;
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel * radius;
            sum += textureSampleCompareLevel(t_shadow, s_shadow, uv + off, depth_ref);
        }
    }
    return sum / 9.0;
}

fn refl_hit_lighting_lite(
    hit_pos : vec3<f32>,
    hit_normal : vec3<f32>,
    view_dir : vec3<f32>,
    mat : RtInstanceMaterial,
    has_material : bool,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
    rt_light : RtLightUniform,
    t_shadow : texture_depth_2d,
    s_shadow : sampler_comparison,
) -> vec3<f32> {
    _ = rt_light;
    _ = t_shadow;
    _ = s_shadow;
    if !has_material {
        return refl_sample_probe_at_hit(hit_pos, view_dir, 0.0, probe_meta, t_probe, s_probe);
    }
    let albedo = mat.albedo.xyz;
    let metallic = mat.pbr.y;
    let roughness = mat.pbr.x;
    let env = refl_sample_probe_for_material(
        hit_pos,
        view_dir,
        roughness,
        mat,
        has_material,
        probe_meta,
        t_probe,
        s_probe,
    );
    let n = normalize(hit_normal);
    let v = normalize(view_dir);
    let ndotv = max(dot(n, v), 0.0);
    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(vec3<f32>(1.0 - ndotv), vec3<f32>(5.0));
    let spec = env * fresnel;
    let diff = env * (1.0 - metallic) * albedo * (vec3<f32>(1.0) - fresnel);
    return spec + diff;
}

fn refl_refract_dir(incident : vec3<f32>, normal : vec3<f32>, eta : f32) -> vec3<f32> {
    let n = normalize(normal);
    let uv = normalize(incident);
    let cos_theta = min(dot(-uv, n), 1.0);
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let cannot_refract = eta * sin_theta > 1.0;
    if cannot_refract {
        return reflect(uv, n);
    }
    let r_out_perp = eta * (uv + cos_theta * n);
    let r_out_parallel = -sqrt(max(1.0 - dot(r_out_perp, r_out_perp), 0.0)) * n;
    return normalize(r_out_perp + r_out_parallel);
}

fn refl_dielectric_fresnel(cos_theta : f32, ref_idx : f32) -> f32 {
    var r0 = (1.0 - ref_idx) / (1.0 + ref_idx);
    r0 = r0 * r0;
    return r0 + (1.0 - r0) * pow(1.0 - cos_theta, 5.0);
}

fn refl_resolve_hit_radiance(
    hit_pos : vec3<f32>,
    hit_normal : vec3<f32>,
    refl_dir : vec3<f32>,
    sample_uv : vec2<f32>,
    on_screen : bool,
    spacing_px : f32,
    mat : RtInstanceMaterial,
    has_material : bool,
    probe_meta : ReflProbeMeta,
    t_probe : texture_cube_array<f32>,
    s_probe : sampler,
    rt_light : RtLightUniform,
    t_shadow : texture_depth_2d,
    s_shadow : sampler_comparison,
    t_lit_scene : texture_2d<f32>,
    t_depth : texture_2d<f32>,
    t_direct : texture_2d<f32>,
    t_base_color : texture_2d<f32>,
    resolution : vec2<f32>,
    step_size : f32,
    view_proj : mat4x4<f32>,
    near_plane : f32,
    far_plane : f32,
) -> vec3<f32> {
    if on_screen {
        let hit_px = vec2<i32>(sample_uv * resolution);
        let hit_depth_m = textureLoad(t_depth, hit_px, 0).r;
        let clip = view_proj * vec4<f32>(hit_pos, 1.0);
        let ray_depth_m = (near_plane * far_plane)
            / (far_plane - refl_gl_ndc_z_to_vk(clip.z / clip.w) * (far_plane - near_plane));
        if abs(ray_depth_m - hit_depth_m) <= refl_rt_hit_depth_reject_m(step_size) {
            let hit_metallic = textureLoad(t_direct, hit_px, 0).a;
            let hit_albedo = textureLoad(t_base_color, hit_px, 0).rgb;
            let lit = textureLoad(t_lit_scene, hit_px, 0).rgb;
            return refl_metal_attenuate(lit, hit_albedo, hit_metallic);
        }
    }
    return refl_hit_lighting_lite(
        hit_pos,
        hit_normal,
        normalize(refl_dir),
        mat,
        has_material,
        probe_meta,
        t_probe,
        s_probe,
        rt_light,
        t_shadow,
        s_shadow,
    );
}
