#version 450

layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec2 uv;
layout(location = 3) in vec4 model0;
layout(location = 4) in vec4 model1;
layout(location = 5) in vec4 model2;
layout(location = 6) in vec4 model3;
layout(location = 7) in vec4 flag_pad;
layout(location = 8) in vec4 tex_layer_pad;

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

layout(location = 0) out vec3 v_world;
layout(location = 1) out vec3 v_normal;
layout(location = 2) out vec2 v_uv;
layout(location = 3) out vec4 v_flag;
layout(location = 4) out vec4 v_tex;
layout(location = 5) out vec4 v_curr;
layout(location = 6) out vec4 v_prev;

void main() {
    mat4 model = mat4(model0, model1, model2, model3);
    vec4 world = model * vec4(position, 1.0);
    vec3 n = mat3(model) * normal;
    v_world = world.xyz;
    v_normal = n;
    v_uv = uv;
    v_flag = flag_pad;
    v_tex = tex_layer_pad;
    vec4 curr = u.view_proj * world;
    vec4 prev = u.prev_view_proj * world;
    v_curr = curr;
    v_prev = prev;
    gl_Position = curr;
}
