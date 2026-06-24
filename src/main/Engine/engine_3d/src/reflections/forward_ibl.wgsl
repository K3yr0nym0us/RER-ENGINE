// Forward-pass IBL metálico (capa A). Incluido vía shader_loader antes del cuerpo del shader de escena.
// Lógica idéntica a la que vivía inline en shader.wgsl / shader_skinned.wgsl (sin cambiar comportamiento).

struct ForwardMetalLighting {
    ambient: vec3<f32>,
    direct: vec3<f32>,
}

/// Muestreo de cubemap para `fs_main` (tier Low+).
/// Devuelve `vec4(rgb, mode)`:
/// - `mode = 0`: sin probe / sin contribución
/// - `mode = 1`: metal — reemplazo IBL en `evaluate_scene`
/// - `mode = 2`: dieléctrico — sumar especular IBL sobre diffuse
fn forward_sample_probe_env(
    t_probe: texture_cube_array<f32>,
    s_probe: sampler,
    world_pos: vec3<f32>,
    cam_pos: vec3<f32>,
    world_normal: vec3<f32>,
    surface_metallic: f32,
    surface_roughness: f32,
    albedo: vec3<f32>,
    probe_layer: i32,
    probe_entries: array<vec4<f32>, 8>,
) -> vec4<f32> {
    if probe_layer < 0 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let n = normalize(world_normal);
    let v = normalize(cam_pos - world_pos);
    let sample_dir = refl_cubemap_sample_dir(world_pos, cam_pos, n, probe_layer, probe_entries);
    let lod = refl_env_cubemap_lod(surface_roughness);
    let env_rgb = textureSampleLevel(t_probe, s_probe, sample_dir, probe_layer, lod).rgb;
    if surface_metallic > 0.5 {
        return vec4<f32>(env_rgb, 1.0);
    }
    let f0 = refl_metal_f0(albedo, surface_metallic);
    let cos_theta = max(dot(n, v), 0.0);
    let fres = refl_fresnel_schlick_vec3(cos_theta, f0);
    let fres_w = dot(fres, vec3<f32>(0.2126, 0.7152, 0.0722));
    let rough_term = (1.0 - surface_roughness) * (1.0 - surface_roughness);
    let spec = fres_w * rough_term;
    if spec < 0.01 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return vec4<f32>(env_rgb * spec, 2.0);
}

/// Alias metálico (compat tests / lectura).
fn forward_sample_metallic_env(
    t_probe: texture_cube_array<f32>,
    s_probe: sampler,
    world_pos: vec3<f32>,
    cam_pos: vec3<f32>,
    world_normal: vec3<f32>,
    surface_metallic: f32,
    surface_roughness: f32,
    probe_layer: i32,
    probe_entries: array<vec4<f32>, 8>,
) -> vec4<f32> {
    return forward_sample_probe_env(
        t_probe,
        s_probe,
        world_pos,
        cam_pos,
        world_normal,
        surface_metallic,
        surface_roughness,
        vec3<f32>(0.55),
        probe_layer,
        probe_entries,
    );
}

/// Entorno procedural cromado (mismo gradiente que `shader.wgsl` antes de la extracción).
fn forward_fake_environment(refl_dir: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(0.50, 0.57, 0.72);
    let horizon = vec3<f32>(0.82, 0.84, 0.88);
    let ground = vec3<f32>(0.26, 0.27, 0.29);
    let y = clamp(refl_dir.y, -1.0, 1.0);
    if y >= 0.0 {
        return mix(horizon, sky, pow(y, 0.45));
    }
    return mix(horizon, ground, pow(-y, 0.55));
}

fn forward_fresnel_schlick_metal(f0: vec3<f32>, cos_theta: f32) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

/// Bloque PBR metálico de `evaluate_scene` (procedural + cubemap opcional).
fn forward_evaluate_metallic_pbr(
    albedo: vec3<f32>,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    surface_roughness: f32,
    uv: vec2<f32>,
    cam_pos: vec3<f32>,
    light_dir: vec3<f32>,
    light_color: vec3<f32>,
    light_intensity: f32,
    ndotl: f32,
    env_override: vec3<f32>,
    has_env_override: bool,
) -> ForwardMetalLighting {
    let n = normalize(world_normal);
    let v = normalize(cam_pos - world_pos);
    let cos_theta = max(dot(n, v), 0.0);
    let f0 = albedo;
    let fres = forward_fresnel_schlick_metal(f0, cos_theta);
    let refl_proc = refl_fuzzy_mirror_dir(
        world_pos,
        cam_pos,
        n,
        surface_roughness,
        uv,
    );
    let env_proc = forward_fake_environment(
        select(reflect(-v, n), refl_proc, dot(refl_proc, refl_proc) > 1e-8),
    );
    let proc_lum = dot(env_proc, vec3<f32>(0.2126, 0.7152, 0.0722));
    let blur_w = surface_roughness * surface_roughness;
    let env_proc_eff = mix(env_proc, vec3<f32>(proc_lum), blur_w);
    let env_eff = select(env_proc_eff, env_override, has_env_override);

    let h = normalize(light_dir + v);
    let spec_power = mix(512.0, 16.0, surface_roughness);
    let spec = pow(max(dot(n, h), 0.0), spec_power);
    var out: ForwardMetalLighting;
    out.ambient = env_eff * fres;
    out.direct = light_color * spec * light_intensity * ndotl * (1.0 - surface_roughness * 0.6);
    return out;
}

/// Variante skinned: gradiente procedural distinto (conservado del shader_skinned original).
fn forward_fake_environment_skinned(refl_dir: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(0.72, 0.75, 0.80);
    let horizon = vec3<f32>(0.42, 0.43, 0.45);
    let ground = vec3<f32>(0.12, 0.12, 0.13);
    let t = clamp(refl_dir.y * 0.5 + 0.5, 0.0, 1.0);
    if t > 0.5 {
        return mix(horizon, sky, (t - 0.5) * 2.0);
    }
    return mix(ground, horizon, t * 2.0);
}

fn forward_evaluate_metallic_pbr_skinned(
    albedo: vec3<f32>,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    surface_roughness: f32,
    uv: vec2<f32>,
    cam_pos: vec3<f32>,
    light_dir: vec3<f32>,
    light_color: vec3<f32>,
    light_intensity: f32,
    ndotl: f32,
    env_override: vec3<f32>,
    has_env_override: bool,
) -> ForwardMetalLighting {
    let n = normalize(world_normal);
    let v = normalize(cam_pos - world_pos);
    let cos_theta = max(dot(n, v), 0.0);
    let f0 = albedo;
    let fres = forward_fresnel_schlick_metal(f0, cos_theta);
    let refl_fuzzy = refl_fuzzy_mirror_dir(
        world_pos,
        cam_pos,
        n,
        surface_roughness,
        uv,
    );
    let env_proc = forward_fake_environment_skinned(
        select(reflect(-v, n), refl_fuzzy, dot(refl_fuzzy, refl_fuzzy) > 1e-8),
    );
    let proc_lum = dot(env_proc, vec3<f32>(0.2126, 0.7152, 0.0722));
    let blur_w = surface_roughness * surface_roughness;
    let env_proc_eff = mix(env_proc, vec3<f32>(proc_lum), blur_w);
    let env_eff = select(env_proc_eff, env_override, has_env_override);

    let h = normalize(light_dir + v);
    let spec_power = mix(512.0, 16.0, surface_roughness);
    let spec = pow(max(dot(n, h), 0.0), spec_power);
    var out: ForwardMetalLighting;
    out.ambient = env_eff * fres;
    out.direct = light_color * spec * light_intensity * ndotl * (1.0 - surface_roughness * 0.6);
    return out;
}
