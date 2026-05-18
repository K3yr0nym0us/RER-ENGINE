// ── Uniforms compartidos por frame (group 0) ──────────────────────────────────
// Solo contiene datos que son iguales para TODOS los sprites del frame:
// la view_proj de la cámara y la posición de la cámara para iluminación.
// El model matrix y el flag de selección viajan por instancia (InstanceInput).
struct SceneUniforms {
    view_proj : mat4x4<f32>,
    cam_pos   : vec4<f32>,   // xyz = posición cámara, w = sin uso
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

// ── Instance I/O (paso de datos por instancia) ────────────────────────────────
// model: matrix columna-major. flag_pad.x: 0=normal 1=seleccionado 2=hover
// flag_pad.y: multiplicador de alpha por instancia (default 1.0)
// flag_pad.z: 0=normal 1=collider 2=trigger (color plano, sin difuminado)
// uv_rect: sub-región del texture atlas [u_min, v_min, u_max, v_max]
struct InstanceInput {
    @location(3) model_0  : vec4<f32>,
    @location(4) model_1  : vec4<f32>,
    @location(5) model_2  : vec4<f32>,
    @location(6) model_3  : vec4<f32>,
    @location(7) flag_pad : vec4<f32>,
    @location(8) uv_rect  : vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos  : vec4<f32>,
    @location(0) world_pos       : vec3<f32>,
    @location(1) world_normal    : vec3<f32>,
    @location(2) uv              : vec2<f32>,
    @location(3) flag            : f32,
    @location(4) alpha_mul       : f32,
    @location(5) render_kind     : f32,
    @location(6) uv_center       : vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput, inst: InstanceInput) -> VertexOutput {
    let model      = mat4x4<f32>(inst.model_0, inst.model_1, inst.model_2, inst.model_3);
    let world_pos4 = model * vec4<f32>(in.position, 1.0);
    var out: VertexOutput;
    out.clip_pos    = u.view_proj * world_pos4;
    out.world_pos   = world_pos4.xyz;
    out.world_normal = normalize((model * vec4<f32>(in.normal, 0.0)).xyz);
    // Remap UV 0→1 a la sub-región del atlas: uv_rect.xy = min, uv_rect.zw = max
    out.uv          = inst.uv_rect.xy + in.uv * (inst.uv_rect.zw - inst.uv_rect.xy);
    out.flag        = inst.flag_pad.x;
    out.alpha_mul   = inst.flag_pad.y;
    out.render_kind = inst.flag_pad.z;
    out.uv_center   = (inst.uv_rect.xy + inst.uv_rect.zw) * 0.5;
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

fn evaluate_scene_color(in: VertexOutput) -> vec4<f32> {
    var albedo_samp = textureSample(t_albedo, s_albedo, in.uv);
    if in.render_kind >= 0.5 {
        albedo_samp = textureSample(t_albedo, s_albedo, in.uv_center);
    }
    let albedo      = albedo_samp.rgb;   // usar color directo sin gamma ni PBR

    let n = normalize(in.world_normal);
    let v = normalize(u.cam_pos.xyz - in.world_pos);
    let l = normalize(LIGHT_DIR);

    var color = albedo;
    if in.render_kind < 0.5 {
        // Iluminación Lambert simple para entidades normales.
        let ndotl = max(dot(n, l), 0.0);
        color = albedo * (0.4 + 0.6 * ndotl);  // 40% ambient + 60% diffuse
    }

    // ── Rim glow + flat tint según estado de selección/hover ─────────────────
    // rim_factor ≈ 0 en el centro, 1 en los bordes tangentes a la cámara.
    // En quads 2D (normal=[0,0,1] frente a cámara) rim_factor≈0, por eso
    // añadimos también un flat mix para que el cambio sea siempre visible.
    let rim_factor = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    if in.flag > 1.5 {
        // Hover: tint cian
        color = mix(color, vec3<f32>(0.12, 0.60, 0.88), 0.38);
        color += vec3<f32>(0.15, 0.65, 0.90) * rim_factor * 1.3;
    } else if in.flag > 0.5 {
        // Seleccionado: tint dorado
        color = mix(color, vec3<f32>(1.0, 0.75, 0.10), 0.38);
        color += vec3<f32>(1.0, 0.80, 0.15) * rim_factor * 2.2;
    }

    var out_alpha = albedo_samp.a * in.alpha_mul;
    if in.render_kind >= 0.5 {
        out_alpha = in.alpha_mul;
    }

    return vec4<f32>(color, out_alpha);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return evaluate_scene_color(in);
}

@fragment
fn fs_overlay(in: VertexOutput) -> @location(0) vec4<f32> {
    return evaluate_scene_color(in);
}
