#version 450

// Port GLSL del `fs_screen_hud` de shader_screen_hud.wgsl. Exclusivo del pase de imágenes HUD en
// espacio de pantalla: no reutilizar para la escena.
layout(location = 0) in vec2 v_uv;
layout(location = 1) in float v_alpha_mul;

layout(set = 1, binding = 0) uniform sampler2D t_hud;

layout(location = 0) out vec4 out_color;

void main() {
    vec4 c = texture(t_hud, v_uv);
    out_color = vec4(c.rgb, c.a * v_alpha_mul);
}
