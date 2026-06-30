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
/// - `mode = 2`: dieléctrico reflectante (SurfacePbr + rugosidad trazable) — solo especular IBL
fn forward_probe_surface_eligible(inst_roughness: f32, surface_roughness: f32) -> bool {
    // Misma política que SSR: opt-in por SurfacePbr (inst >= 0) y rugosidad baja.
    return inst_roughness >= 0.0 && surface_roughness <= 0.70;
}

/// Bevy: cubemap probe = indirecto de baja frecuencia; SSR = detalle en pantalla.
/// Si el píxel es trazable por SSR, el cubemap se aplica en el miss SSR, no en forward.
fn forward_defer_probe_to_ssr(
    ssr_active: bool,
    inst_roughness: f32,
    surface_roughness: f32,
    surface_metallic: f32,
) -> bool {
    if !ssr_active {
        return false;
    }
    if surface_metallic > 0.5 {
        return true;
    }
    return forward_probe_surface_eligible(inst_roughness, surface_roughness);
}

/// Atenúa el dominante azul del cubemap/procedural en dieléctricos claros (vidrio).
fn forward_tint_dielectric_env(env_rgb: vec3<f32>, albedo: vec3<f32>) -> vec3<f32> {
    let albedo_lum = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
    if albedo_lum < 0.55 {
        return env_rgb;
    }
    let env_lum = dot(env_rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let neutral_tint = vec3<f32>(env_lum) * (albedo / max(albedo_lum, 1e-4));
    return mix(env_rgb, neutral_tint, 0.72);
}

fn forward_sample_probe_env(
    t_probe: texture_cube_array<f32>,
    s_probe: sampler,
    world_pos: vec3<f32>,
    cam_pos: vec3<f32>,
    world_normal: vec3<f32>,
    surface_metallic: f32,
    inst_roughness: f32,
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
    if !forward_probe_surface_eligible(inst_roughness, surface_roughness) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let trace_w = refl_trace_strength(surface_roughness, surface_metallic, n, v, albedo, 0.0);
    if trace_w < 0.01 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let tinted_env = forward_tint_dielectric_env(env_rgb, albedo);
    return vec4<f32>(tinted_env * trace_w, 2.0);
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
        surface_roughness,
        vec3<f32>(0.55),
        probe_layer,
        probe_entries,
    );
}

/// Entorno procedural cromado (mismo celeste uniforme que el cielo esférico de límites).
fn forward_fake_environment(_refl_dir: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(0.72, 0.86, 0.98);
}

fn forward_fresnel_schlick_metal(f0: vec3<f32>, cos_theta: f32) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

/// Evita IBL metálico blanco puro en convexidades (esferas): cap por tinte F0 de la alfombra plana.
fn forward_clamp_metal_reflection(amb: vec3<f32>, albedo: vec3<f32>) -> vec3<f32> {
    let amb_lum = dot(amb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let ref_lum = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
    let cap = ref_lum * 1.35 + 0.14;
    if amb_lum > cap {
        return amb * (cap / max(amb_lum, 1e-4));
    }
    return amb;
}

/// Terminador solar en metales (sin diffuse Lambert): half-Lambert sobre la reflexión ambiente.
fn forward_metal_sun_shade(ndotl: f32) -> f32 {
    let half_lambert = ndotl * 0.5 + 0.5;
    return 0.14 + 0.86 * half_lambert;
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
    defer_specular_to_ssr: bool,
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
    let albedo_lum = max(max(albedo.r, albedo.g), albedo.b);
    let sun_term = forward_metal_sun_shade(ndotl);
    var out: ForwardMetalLighting;
    let metal_amb = forward_clamp_metal_reflection(env_eff * fres * sun_term, albedo);
    // Reservar headroom para SSR: metales claros (steel) y rugosos tenían IBL tan alto
    // que el composite no dejaba ver el reflejo en pantalla (chrome oscuro no sufría).
    let spec_reserve = (1.0 - surface_roughness) * (1.0 - surface_roughness);
    let albedo_lum_pbr = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
    let bright_suppress = mix(1.0, 0.32, smoothstep(0.12, 0.48, albedo_lum_pbr));
    var ibl_headroom = mix(0.1, 1.0, spec_reserve) * bright_suppress;
    if defer_specular_to_ssr {
        // Metales claros: reservar specular para SSR. Metales oscuros (chrome): conservar tinte IBL base.
        let dark_metal = smoothstep(0.16, 0.05, albedo_lum_pbr);
        ibl_headroom = mix(
            ibl_headroom * 0.06 * bright_suppress,
            ibl_headroom * mix(0.38, 0.24, spec_reserve),
            dark_metal,
        );
    }
    out.ambient = metal_amb * ibl_headroom;
    out.direct = light_color * spec * light_intensity * ndotl * (1.0 - surface_roughness * 0.6)
        * (0.25 + 0.75 * albedo_lum);
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
    defer_specular_to_ssr: bool,
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
    let albedo_lum = max(max(albedo.r, albedo.g), albedo.b);
    let sun_term = forward_metal_sun_shade(ndotl);
    var out: ForwardMetalLighting;
    let metal_amb = forward_clamp_metal_reflection(env_eff * fres * sun_term, albedo);
    let spec_reserve = (1.0 - surface_roughness) * (1.0 - surface_roughness);
    let albedo_lum_pbr = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
    let bright_suppress = mix(1.0, 0.32, smoothstep(0.12, 0.48, albedo_lum_pbr));
    var ibl_headroom = mix(0.1, 1.0, spec_reserve) * bright_suppress;
    if defer_specular_to_ssr {
        let dark_metal = smoothstep(0.16, 0.05, albedo_lum_pbr);
        ibl_headroom = mix(
            ibl_headroom * 0.06 * bright_suppress,
            ibl_headroom * mix(0.38, 0.24, spec_reserve),
            dark_metal,
        );
    }
    out.ambient = metal_amb * ibl_headroom;
    out.direct = light_color * spec * light_intensity * ndotl * (1.0 - surface_roughness * 0.6)
        * (0.25 + 0.75 * albedo_lum);
    return out;
}
