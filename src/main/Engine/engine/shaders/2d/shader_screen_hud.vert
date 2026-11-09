#version 450

// Port GLSL del `vs_screen_hud` de shader_screen_hud.wgsl. Es el mismo layout de vértice e
// instancia que el shader de escena; el bloque uniforme es el completo del 2D
// (`ScreenHudSceneUniforms`, 416 B) porque comparte el binding con la ruta wgpu.
layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec2 uv;
layout(location = 3) in vec4 model0;
layout(location = 4) in vec4 model1;
layout(location = 5) in vec4 model2;
layout(location = 6) in vec4 model3;
layout(location = 7) in vec4 flag_pad;
layout(location = 8) in vec4 uv_rect;

layout(set = 0, binding = 0) uniform Scene {
    mat4 view_proj;
    mat4 view_proj_stable;
    mat4 prev_view_proj;
    mat4 inv_view_proj;
    vec4 cam_pos;
    vec4 light_dir;
    vec4 light_color;
    mat4 light_view_proj;
    vec4 light_params;
    vec4 jitter;
    vec4 shadow_bias;
} u;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out float v_alpha_mul;

void main() {
    mat4 model = mat4(model0, model1, model2, model3);
    gl_Position = u.view_proj * model * vec4(position, 1.0);
    v_uv = uv_rect.xy + uv * (uv_rect.zw - uv_rect.xy);
    v_alpha_mul = flag_pad.y;
}
