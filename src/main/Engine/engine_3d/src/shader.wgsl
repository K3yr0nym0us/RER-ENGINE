struct SceneUniforms {
    view_proj         : mat4x4<f32>,
    view_proj_stable  : mat4x4<f32>,
    prev_view_proj    : mat4x4<f32>,
    inv_view_proj   : mat4x4<f32>,
    cam_pos         : vec4<f32>,
    light_dir       : vec4<f32>,
    light_color     : vec4<f32>,
    light_view_proj : mat4x4<f32>,
    light_params    : vec4<f32>,
    jitter          : vec4<f32>,
    depth_plane     : vec4<f32>,
    shadow_bias     : vec4<f32>,
}

@group(0) @binding(0)
var<uniform> u: SceneUniforms;

@group(0) @binding(1)
var t_shadow: texture_depth_2d;

@group(0) @binding(2)
var s_shadow: sampler_comparison;

@group(1) @binding(0) var t_albedo: texture_2d_array<f32>;
@group(1) @binding(1) var s_albedo: sampler;

// Reflection probes (grupo 2): cubemap array con el entorno capturado por cada esfera/probe
// (suelo, vecinas y jugador en 360°). Solo lo usa `fs_main`; `fs_overlay` (pase de captura y
// ghost) NO lo referencia, por eso su pipeline no necesita declarar el grupo 2.
@group(2) @binding(0) var t_probe_env: texture_cube_array<f32>;
@group(2) @binding(1) var s_probe_env: sampler;

struct ProbeMeta {
    center_radius : array<vec4<f32>, 8>,
}

@group(2) @binding(2) var<uniform> probe_meta : ProbeMeta;

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
}

struct InstanceInput {
    @location(3) model_0  : vec4<f32>,
    @location(4) model_1  : vec4<f32>,
    @location(5) model_2  : vec4<f32>,
    @location(6) model_3  : vec4<f32>,
    @location(7) flag_pad : vec4<f32>,
    @location(8) tex_layer_pad : vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0) world_pos       : vec3<f32>,
    @location(1) world_normal    : vec3<f32>,
    @location(2) uv              : vec2<f32>,
    @location(3) flag            : f32,
    @location(4) alpha_mul       : f32,
    @location(5) render_kind     : f32,
    @location(6) tex_layer       : u32,
    @location(7) prev_clip_pos     : vec4<f32>,
    @location(8) curr_stable_clip  : vec4<f32>,
    @location(9) surface_roughness  : f32,
    @location(10) surface_metallic  : f32,
    /// Índice de capa del probe en el cube array (-1 = sin probe → entorno procedural).
    @location(11) probe_index       : f32,
}

fn scene_light_dir_norm() -> vec3<f32> {
    var l = u.light_dir.xyz;
    if dot(l, l) < 1e-6 {
        l = vec3<f32>(0.45, 1.0, 0.35);
    }
    return normalize(l);
}

fn scene_shadow(world_pos: vec3<f32>, world_normal: vec3<f32>) -> f32 {
    let l = scene_light_dir_norm();
    let n = normalize(world_normal);
    let ndotl = max(dot(n, l), 0.0);
    // Hacia la luz: menos peter-panning en suelo que empujar por la normal del receptor.
    let normal_scale = u.shadow_bias.x + u.shadow_bias.y * (1.0 - ndotl);
    let biased_pos = world_pos + l * normal_scale;

    let clip = u.light_view_proj * vec4<f32>(biased_pos, 1.0);
    let ndc = clip.xyz / clip.w;
    var uv = ndc.xy * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 1.0;
    }
    let slope = sqrt(max(1.0 - ndotl * ndotl, 0.0));
    let depth_ref = ndc.z - (u.shadow_bias.z + u.shadow_bias.w * slope);
    let texel = u.light_params.z;
    let radius = u.light_params.w;
    var sum = 0.0;
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let off = vec2<f32>(f32(ox), f32(oy)) * texel * radius;
            sum += textureSampleCompareLevel(t_shadow, s_shadow, uv + off, depth_ref);
        }
    }
    return sum / 9.0;
}

fn apply_selection_rim(color: vec3<f32>, flag: f32, world_pos: vec3<f32>, world_normal: vec3<f32>) -> vec3<f32> {
    let v = normalize(u.cam_pos.xyz - world_pos);
    let n = normalize(world_normal);
    let rim_factor = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    var out_c = color;
    if flag > 1.5 {
        out_c = mix(out_c, vec3<f32>(0.12, 0.60, 0.88), 0.38);
        out_c += vec3<f32>(0.15, 0.65, 0.90) * rim_factor * 1.3;
    } else if flag > 0.5 {
        out_c = mix(out_c, vec3<f32>(1.0, 0.75, 0.10), 0.38);
        out_c += vec3<f32>(1.0, 0.80, 0.15) * rim_factor * 2.2;
    }
    return out_c;
}

@vertex
fn vs_shadow(in: VertexInput, inst: InstanceInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let world = model * vec4<f32>(in.position, 1.0);
    return u.light_view_proj * world;
}

@vertex
fn vs_main(in: VertexInput, inst: InstanceInput) -> VertexOutput {
    let model      = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let world_pos4 = model * vec4<f32>(in.position, 1.0);
    var out: VertexOutput;
    let world_pos = world_pos4.xyz;
    out.clip_pos     = u.view_proj * world_pos4;
    out.world_pos    = world_pos;
    out.world_normal = normalize((model * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv           = in.uv;
    out.flag         = inst.flag_pad.x;
    out.alpha_mul    = inst.flag_pad.y;
    out.render_kind  = inst.flag_pad.z;
    out.tex_layer    = u32(inst.tex_layer_pad.x);
    out.surface_roughness = inst.flag_pad.w;
    out.surface_metallic = inst.tex_layer_pad.y;
    out.probe_index = inst.tex_layer_pad.z;
    let prev_h = u.prev_view_proj * vec4<f32>(world_pos, 1.0);
    out.prev_clip_pos = prev_h;
    out.curr_stable_clip = u.view_proj_stable * world_pos4;
    return out;
}

struct SceneFragOut {
    @location(0) ambient : vec4<f32>,
    @location(1) shadow  : vec4<f32>,
    @location(2) direct  : vec4<f32>,
    @location(3) depth   : vec4<f32>,
    @location(4) velocity_normal : vec4<f32>,
}

fn encode_octahedral(n: vec3<f32>) -> vec2<f32> {
    let inv_sum = 1.0 / (abs(n.x) + abs(n.y) + abs(n.z));
    var p = n.xy * inv_sum;
    if n.z < 0.0 {
        let ox = p.x;
        let oy = p.y;
        p.x = (1.0 - abs(oy)) * sign(ox);
        p.y = (1.0 - abs(ox)) * sign(oy);
    }
    return p * 0.5 + vec2<f32>(0.5);
}

fn pack_velocity_normal(velocity: vec2<f32>, n: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(velocity, encode_octahedral(normalize(n)));
}

/// PBR-friendly: por defecto, las superficies son MATE (rugosidad alta) → sin reflejos.
/// El SSR/RT solo afecta materiales con `SurfacePbr` explícito (rugosidad baja, metallic).
/// Coherente con Unreal: el reflejo es opt-in por material, no aplicado a todo.
fn resolve_surface_roughness(inst_roughness: f32) -> f32 {
    return select(0.9, inst_roughness, inst_roughness >= 0.0);
}

fn pack_depth_export(view_depth_m: f32) -> vec4<f32> {
    return vec4<f32>(view_depth_m, 0.0, 0.0, 0.0);
}

/// `ndc_z` = `@builtin(position).z` en fragmento: profundidad Vulkan [0,1] ya dividida (near=0, far=1).
/// NO usar `position.z / position.w`: en WGSL `.w` es `1/clip_w`, y esa división devuelve `clip_z`
/// (clip space OpenGL de glam, negativo delante de la cámara), no NDC.
fn ndc_z_to_view_depth_m(ndc_z: f32) -> f32 {
    let n = u.depth_plane.x;
    let f = u.depth_plane.y;
    return (n * f) / (f - ndc_z * (f - n));
}

/// Reflejo de entorno aproximado para el metal (IBL fake; NO es el cielo del mundo, solo
/// se ve DENTRO del metal). Gradiente neutro con contraste claro→oscuro según la dirección
/// de reflexión: da al metal su aspecto pulido cuando el SSR no tiene geometría que reflejar.
/// El SSR sustituye este fallback donde sí golpea el suelo/personaje.
fn fake_environment(refl_dir: vec3<f32>) -> vec3<f32> {
    // Entorno cromado: cielo, una BANDA DE HORIZONTE clara (la "línea" brillante típica de
    // un metal pulido que da el look cromo en toda la circunferencia) y un suelo gris medio
    // que aproxima el reflejo del piso donde el SSR no llega. Esto evita que el borde de la
    // esfera se vea liso/plano: ahora toda la superficie lee como reflectante.
    let sky     = vec3<f32>(0.50, 0.57, 0.72);  // arriba (cielo/techo)
    let horizon = vec3<f32>(0.82, 0.84, 0.88);  // banda clara de horizonte (luz rasante)
    let ground  = vec3<f32>(0.26, 0.27, 0.29);  // abajo (suelo reflejado)
    let y = clamp(refl_dir.y, -1.0, 1.0);
    if y >= 0.0 {
        // Horizonte → cielo (curva para concentrar el brillo cerca del ecuador).
        return mix(horizon, sky, pow(y, 0.45));
    }
    // Horizonte → suelo.
    return mix(horizon, ground, pow(-y, 0.55));
}

fn fresnel_schlick_metal(f0: vec3<f32>, cos_theta: f32) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}

/// Mip del cubemap según rugosidad perceptual (rough² × max_mip; coherente con fuzzy RTIOW y SSR).
fn env_cubemap_lod(roughness: f32) -> f32 {
    let r = clamp(roughness, 0.0, 1.0);
    return r * r * 4.0;
}

/// RTIOW fuzzy metal (Book 1): espejo + desvío hacia normal si rough > 0.5; absorb si dot <= 0.
fn apply_fuzzy_reflection(refl: vec3<f32>, n: vec3<f32>, roughness: f32) -> vec3<f32> {
    let nn = normalize(n);
    var dir = refl;
    if dot(dir, nn) <= 0.0 {
        return vec3<f32>(0.0);
    }
    let r = clamp(roughness, 0.0, 1.0);
    if r > 0.5 {
        dir = normalize(mix(dir, nn, r * r));
        if dot(dir, nn) <= 0.0 {
            return vec3<f32>(0.0);
        }
    }
    return dir;
}

/// Dirección de muestreo del cubemap: reflexión geométrica (RTIOW). `probe_idx` solo elige capa.
fn cubemap_sample_dir(world_pos: vec3<f32>, n: vec3<f32>, probe_idx: i32) -> vec3<f32> {
    _ = probe_idx;
    let incident = normalize(world_pos - u.cam_pos.xyz);
    return reflect(incident, normalize(n));
}

fn evaluate_scene(in: VertexOutput, env_override: vec3<f32>, has_env_override: bool) -> SceneFragOut {
    // Export coherente con SSR legacy: clip_pos.z (interpolado) como ndc_z Vulkan.
    let view_depth_m = ndc_z_to_view_depth_m(in.clip_pos.z);
    let curr_ndc = in.curr_stable_clip.xy / in.curr_stable_clip.w;
    let prev_ndc = in.prev_clip_pos.xy / in.prev_clip_pos.w;
    let velocity = (curr_ndc - prev_ndc) * vec2<f32>(0.5, 0.5);

    let layer = i32(in.tex_layer);
    let surface_roughness = resolve_surface_roughness(in.surface_roughness);
    let surface_metallic = in.surface_metallic;

    if in.render_kind >= 2.5 {
        let hud = textureSample(t_albedo, s_albedo, in.uv, layer);
        let c = hud.rgb;
        return SceneFragOut(
            vec4<f32>(c, hud.a * in.alpha_mul),
            vec4<f32>(1.0, 1.0, 0.0, 0.0),  // HUD: sin reflejos (rugosidad 1.0)
            vec4<f32>(0.0, 0.0, 0.0, 0.0),
            pack_depth_export(view_depth_m),
            pack_velocity_normal(velocity, vec3<f32>(0.0, 0.0, 1.0)),
        );
    }

    var shadow = 1.0;
    var albedo_samp = textureSample(t_albedo, s_albedo, in.uv, layer);
    if in.render_kind >= 0.5 {
        albedo_samp = textureSample(t_albedo, s_albedo, vec2<f32>(0.5, 0.5), layer);
    }
    let albedo = albedo_samp.rgb;

    var amb = albedo;
    var dir = vec3<f32>(0.0);
    let skip_shadow_receive = in.render_kind >= 0.25 && in.render_kind < 0.5;
    if in.render_kind < 0.5 {
        let n = normalize(in.world_normal);
        let l = scene_light_dir_norm();
        let ndotl = max(dot(n, l), 0.0);
        let ambient = u.light_dir.w;
        let lc = u.light_color.xyz;
        let intensity = u.light_params.x;
        amb = albedo * ambient * lc * intensity;
        dir = albedo * (1.0 - ambient) * ndotl * lc * intensity;
        if u.light_color.w > 0.5 && !skip_shadow_receive {
            shadow = scene_shadow(in.world_pos, n);
        }
    }

    let lit = apply_selection_rim(amb + dir, in.flag, in.world_pos, in.world_normal);
    let base = amb + dir;
    let rim_add = lit - base;
    amb = amb + rim_add * 0.5;
    dir = dir + rim_add * 0.5;

    var out_alpha = albedo_samp.a * in.alpha_mul;
    if in.render_kind >= 0.5 {
        out_alpha = in.alpha_mul;
    } else if in.alpha_mul < 0.99 {
        out_alpha = albedo_samp.a * in.alpha_mul;
    } else {
        out_alpha = 1.0;
    }

    let n = normalize(in.world_normal);

    // PBR metálico: en metales NO hay diffuse Lambertiano. Lo que da el "color" es la
    // reflexión del entorno tintada por F0 = albedo. Sin IBL real, se aproxima con un
    // entorno procedural (gradiente cielo→suelo) que da el look cromado base; la composite
    // SSR/RT añade reflejos reales encima cuando los hay. Mismo enfoque que Unreal usa
    // como fallback cuando la captura de entorno falla (forums.unrealengine.com).
    if surface_metallic > 0.5 {
        let v = normalize(u.cam_pos.xyz - in.world_pos);
        let l = scene_light_dir_norm();
        let ndotl = max(dot(n, l), 0.0);
        let cos_theta = max(dot(n, v), 0.0);
        let lc = u.light_color.xyz;
        let intensity = u.light_params.x;
        let f0 = albedo;
        let fres = fresnel_schlick_metal(f0, cos_theta);
        // En un metal, la apariencia ES el entorno reflejado tintado por su F0 (color de la
        // textura). Sin IBL real se aproxima con el gradiente; el SSR sustituye encima donde
        // hay geometría on-screen. Rugosidad alta aplana el reflejo hacia el promedio.
        // Con probe: cubemap real del entorno (suelo/vecinas/jugador en 360°). Sin él:
        // gradiente procedural cromado. El SSR/RT sustituye encima donde hay geometría on-screen.
        let incident = normalize(in.world_pos - u.cam_pos.xyz);
        let refl_proc = apply_fuzzy_reflection(reflect(incident, n), n, surface_roughness);
        let env_proc = fake_environment(
            select(reflect(-v, n), refl_proc, dot(refl_proc, refl_proc) > 1e-8),
        );
        // Tier Off (procedural): difuminar hacia luminancia local — aplana líneas/contrastes
        // sin oscurecer el F0 de la esfera (evita confundir rugosidad con color base).
        let proc_lum = dot(env_proc, vec3<f32>(0.2126, 0.7152, 0.0722));
        let blur_w = surface_roughness * surface_roughness;
        let env_proc_eff = mix(env_proc, vec3<f32>(proc_lum), blur_w);
        // Cubemap (tier Low+): env_override ya viene prefiltrado por mip en fs_main.
        let env_eff = select(env_proc_eff, env_override, has_env_override);
        amb = env_eff * fres;

        // Highlight especular de la luz direccional (glint del sol sobre el metal).
        let h = normalize(l + v);
        let spec_power = mix(512.0, 16.0, surface_roughness);
        let spec = pow(max(dot(n, h), 0.0), spec_power);
        dir = lc * spec * intensity * ndotl * (1.0 - surface_roughness * 0.6);
    }

    return SceneFragOut(
        vec4<f32>(amb, out_alpha),
        // surface (Rg16Float): .r = shadow, .g = roughness.
        vec4<f32>(shadow, surface_roughness, 0.0, 0.0),
        // direct: .rgb = luz directa, .a = metallic (canal libre; lit_composite usa amb.a).
        vec4<f32>(dir, surface_metallic),
        pack_depth_export(view_depth_m),
        pack_velocity_normal(velocity, n),
    );
}

fn sample_surface_albedo(in: VertexOutput) -> vec3<f32> {
    let layer = i32(in.tex_layer);
    var albedo_samp = textureSample(t_albedo, s_albedo, in.uv, layer);
    if in.render_kind >= 0.5 {
        albedo_samp = textureSample(t_albedo, s_albedo, vec2<f32>(0.5, 0.5), layer);
    }
    return albedo_samp.rgb;
}

/// Pase aparte (1 MRT): límite wgpu 32 B/muestra impide 6.º target en el pass principal.
@fragment
fn fs_export_base_color(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.render_kind >= 2.5 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(sample_surface_albedo(in), 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> SceneFragOut {
    // El cubemap del probe es la reflexión COMPLETA del entorno (captura 360° desde el
    // centro de la esfera: suelo, vecinas, jugador aunque esté detrás de la cámara FPS).
    // SSR/RT solo añaden detalle nítido de geometría on-screen encima (capa secundaria).
    let n = normalize(in.world_normal);
    let layer_i = i32(in.probe_index);
    let rough = resolve_surface_roughness(in.surface_roughness);
    let sample_dir = cubemap_sample_dir(in.world_pos, n, layer_i);
    let layer = max(layer_i, 0);
    let lod = env_cubemap_lod(rough);
    var env_cube = vec3<f32>(0.0);
    var has_override = false;
    // Rugosidad del cubemap = mip LOD; fuzzy de dirección solo en SSR/RT (reflection_math.wgsl).
    if in.surface_metallic > 0.5 && in.probe_index >= 0.0 {
        env_cube = textureSampleLevel(t_probe_env, s_probe_env, sample_dir, layer, lod).rgb;
        has_override = true;
    }
    return evaluate_scene(in, env_cube, has_override);
}

@fragment
fn fs_overlay(in: VertexOutput) -> @location(0) vec4<f32> {
    // Captura IBL: entorno difuso/reflexión base, sin glint especular del sol (evita manchas
    // blancas circulares al reflejar otras esferas metálicas en el cubemap).
    let out = evaluate_scene(in, vec3<f32>(0.0), false);
    var rgb = out.ambient.rgb;
    if in.surface_metallic <= 0.5 {
        rgb += out.direct.rgb;
    }
    return vec4<f32>(rgb, out.ambient.a);
}
