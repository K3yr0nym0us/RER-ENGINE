// ── Uniforms ──────────────────────────────────────────────────────────────────
struct SceneUniforms {
    view_proj : mat4x4<f32>,
    model     : mat4x4<f32>,
    cam_pos   : vec4<f32>,   // xyz = posición cámara  |  w: 0=normal 1=selected 2=hover
}

@group(0) @binding(0)
var<uniform> u: SceneUniforms;

// ── Textura albedo (group 1) ──────────────────────────────────────────────────
@group(1) @binding(0) var t_albedo: texture_2d<f32>;
@group(1) @binding(1) var s_albedo: sampler;

// ── Vertex I/O ────────────────────────────────────────────────────────────────
struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) uv       : vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0) world_pos       : vec3<f32>,
    @location(1) world_normal    : vec3<f32>,
    @location(2) uv              : vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let world_pos4 = u.model * vec4<f32>(in.position, 1.0);
    var out: VertexOutput;
    out.clip_pos    = u.view_proj * world_pos4;
    out.world_pos   = world_pos4.xyz;
    out.world_normal = normalize((u.model * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv          = in.uv;
    return out;
}

// ── PBR helpers ───────────────────────────────────────────────────────────────

// Distribución GGX / Trowbridge-Reitz
fn distribution_ggx(n: vec3<f32>, h: vec3<f32>, roughness: f32) -> f32 {
    let a  = roughness * roughness;
    let a2 = a * a;
    let ndoth  = max(dot(n, h), 0.0);
    let ndoth2 = ndoth * ndoth;
    let denom  = ndoth2 * (a2 - 1.0) + 1.0;
    return a2 / (3.14159265 * denom * denom);
}

// Geometría Smith-Schlick-GGX
fn geometry_schlick(ndotv: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return ndotv / (ndotv * (1.0 - k) + k);
}

fn geometry_smith(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, roughness: f32) -> f32 {
    let ndotv = max(dot(n, v), 0.0);
    let ndotl = max(dot(n, l), 0.0);
    return geometry_schlick(ndotv, roughness) * geometry_schlick(ndotl, roughness);
}

// Fresnel Schlick
fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// ── Fragmento PBR ─────────────────────────────────────────────────────────────
// Material: metallic=0 (dieléctrico puro), roughness=0.5 (semimate)
// Luz direccional única (sol de día)
const LIGHT_DIR   : vec3<f32> = vec3<f32>(0.6,  1.0, 0.4);
const LIGHT_COLOR : vec3<f32> = vec3<f32>(3.0,  2.8, 2.5);   // luz blanca cálida
const METALLIC    : f32       = 0.0;
const ROUGHNESS   : f32       = 0.5;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let albedo_samp = textureSample(t_albedo, s_albedo, in.uv);
    let albedo      = pow(albedo_samp.rgb, vec3<f32>(2.2));  // sRGB → linear

    let n = normalize(in.world_normal);
    let v = normalize(u.cam_pos.xyz - in.world_pos);
    let l = normalize(LIGHT_DIR);
    let h = normalize(v + l);

    // F0: para dieléctrico ~0.04, para metales = albedo
    let f0      = mix(vec3<f32>(0.04), albedo, METALLIC);
    let ndotl   = max(dot(n, l), 0.0);
    let radiance = LIGHT_COLOR * ndotl;

    // Especular (Cook-Torrance)
    let ndf = distribution_ggx(n, h, ROUGHNESS);
    let g   = geometry_smith(n, v, l, ROUGHNESS);
    let f   = fresnel_schlick(max(dot(h, v), 0.0), f0);

    let kd    = (1.0 - f) * (1.0 - METALLIC);
    let spec  = (ndf * g * f) / max(4.0 * max(dot(n, v), 0.0) * ndotl, 0.001);

    let lo = (kd * albedo / 3.14159265 + spec) * radiance;

    // Ambiente IBL simplificado (AO = 1 por ahora)
    let ambient = vec3<f32>(0.03) * albedo;

    // Tone mapping Reinhard + corrección gamma
    var color = ambient + lo;
    color     = color / (color + vec3<f32>(1.0));

    // ── Rim glow: borde del objeto según estado de selección ─────────────────
    // rim_factor ≈ 0 en el centro, 1 en los bordes tangentes a la cámara
    let rim_factor = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    if u.cam_pos.w > 1.5 {
        // Hover: borde cian sutil
        color += vec3<f32>(0.15, 0.65, 0.90) * rim_factor * 1.3;
    } else if u.cam_pos.w > 0.5 {
        // Seleccionado: borde dorado brillante
        color += vec3<f32>(1.0, 0.80, 0.15) * rim_factor * 2.2;
    }

    color = pow(color, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(color, albedo_samp.a);
}
