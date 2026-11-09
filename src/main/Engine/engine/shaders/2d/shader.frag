#version 450

// Port GLSL de `evaluate_scene_color` + `fs_main` de shader.wgsl. `fs_overlay` era idéntico a
// `fs_main` en el WGSL, así que ambos pipelines (main / main-2d / overlay) comparten este mismo
// fragmento y solo cambian el estado del pipeline.
layout(location = 0) in vec3 v_world_pos;
layout(location = 1) in vec3 v_world_normal;
layout(location = 2) in vec2 v_uv;
layout(location = 3) in float v_flag;
layout(location = 4) in float v_alpha_mul;
layout(location = 5) in float v_render_kind;
layout(location = 6) in vec2 v_uv_center;

layout(set = 0, binding = 0) uniform Scene {
    mat4 view_proj;
    vec4 cam_pos;
} u;

layout(set = 1, binding = 0) uniform sampler2D t_albedo;

layout(location = 0) out vec4 out_color;

const vec3 LIGHT_DIR = vec3(0.6, 1.0, 0.4);
const vec3 LIGHT_COLOR = vec3(3.0, 2.8, 2.5); // conservado del WGSL; la rama 2D no lo usa
const float METALLIC = 0.0;
const float ROUGHNESS = 0.5;

void main() {
    vec4 albedo_samp = texture(t_albedo, v_uv);
    if (v_render_kind >= 0.5) {
        albedo_samp = texture(t_albedo, v_uv_center);
    }
    vec3 albedo = albedo_samp.rgb; // color directo, sin gamma ni PBR

    vec3 n = normalize(v_world_normal);
    vec3 v = normalize(u.cam_pos.xyz - v_world_pos);
    vec3 l = normalize(LIGHT_DIR);

    vec3 color = albedo;
    if (v_render_kind < 0.5) {
        // Iluminación Lambert simple para entidades normales.
        float ndotl = max(dot(n, l), 0.0);
        color = albedo * (0.4 + 0.6 * ndotl); // 40% ambient + 60% diffuse
    }

    // Rim glow + tinte plano según estado de selección/hover.
    float rim_factor = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    if (v_flag > 1.5) {
        // Hover: tinte cian.
        color = mix(color, vec3(0.12, 0.60, 0.88), 0.38);
        color += vec3(0.15, 0.65, 0.90) * rim_factor * 1.3;
    } else if (v_flag > 0.5) {
        // Seleccionado: tinte dorado.
        color = mix(color, vec3(1.0, 0.75, 0.10), 0.38);
        color += vec3(1.0, 0.80, 0.15) * rim_factor * 2.2;
    }

    float out_alpha = albedo_samp.a * v_alpha_mul;
    if (v_render_kind >= 0.5) {
        out_alpha = v_alpha_mul;
    }

    out_color = vec4(color, out_alpha);
}
