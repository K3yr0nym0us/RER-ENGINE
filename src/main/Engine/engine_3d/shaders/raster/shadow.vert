#version 450

layout(location = 0) in vec3 position;
layout(location = 3) in vec4 model0;
layout(location = 4) in vec4 model1;
layout(location = 5) in vec4 model2;
layout(location = 6) in vec4 model3;

layout(set = 0, binding = 0) uniform Scene {
    mat4 view_proj;
    mat4 view_proj_stable;
    mat4 prev_view_proj;
    mat4 inv_view_proj;
    vec4 cam_pos;
    vec4 light_dir;
    vec4 light_color;
    mat4 light_view_proj[4];
    vec4 cascade_splits;
    vec4 cascade_extent;
    vec4 shadow_params;
    vec4 light_params;
    vec4 jitter;
    vec4 depth_plane;
    vec4 shadow_bias;
} u;

void main() {
    mat4 model = mat4(model0, model1, model2, model3);
    // El pase de sombras reescribe esta ranura con la matriz de la cascada que está dibujando
    // (cmd_update_buffer por cascada), así que el shader no necesita saber el índice.
    gl_Position = u.light_view_proj[0] * model * vec4(position, 1.0);
}
