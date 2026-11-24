#version 450

// Vertex de SOLO sombras para mallas skinneas. No se comparte con el forward: el pase de
// sombras empuja la VP de la cascada (0..64) y la base de la paleta (64..80) en un bloque de
// 80 B, mientras que el forward usa un push de 8 B. Un mismo shader no puede declarar los dos
// bloques porque el rango declarado debe caber en el layout del pipeline que lo usa.

layout(location = 0) in vec3 position;
layout(location = 1) in vec3 normal;
layout(location = 2) in vec2 uv;
layout(location = 3) in uvec4 joints;
layout(location = 4) in vec4 weights;
layout(location = 5) in vec4 model0;
layout(location = 6) in vec4 model1;
layout(location = 7) in vec4 model2;
layout(location = 8) in vec4 model3;
layout(location = 9) in vec4 flag_pad;
layout(location = 10) in vec4 tex_layer_pad;

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
    vec4 depth_plane;
    vec4 shadow_bias;
} u;

layout(set = 2, binding = 0) readonly buffer Joints {
    mat4 palette[];
};

layout(push_constant) uniform CascadePush {
    mat4 view_proj;   // 0..64: matriz luz·proyección de la cascada
    vec4 joint_base;  // 64..80: x = base de la paleta del caster
} pc;

layout(location = 0) out vec3 v_world;
layout(location = 1) out vec3 v_normal;
layout(location = 2) out vec2 v_uv;
layout(location = 3) out vec4 v_flag;
layout(location = 4) flat out vec4 v_tex;
layout(location = 5) out vec4 v_curr;
layout(location = 6) out vec4 v_prev;

void main() {
    uint base = uint(pc.joint_base.x);
    mat4 skin = palette[base + joints.x] * weights.x
        + palette[base + joints.y] * weights.y
        + palette[base + joints.z] * weights.z
        + palette[base + joints.w] * weights.w;
    mat4 model = mat4(model0, model1, model2, model3);
    vec4 world = model * skin * vec4(position, 1.0);
    v_world = world.xyz;
    v_normal = mat3(model) * mat3(skin) * normal;
    v_uv = uv;
    v_flag = flag_pad;
    v_tex = tex_layer_pad;
    v_curr = u.view_proj_stable * world;
    v_prev = u.prev_view_proj * world;
    gl_Position = pc.view_proj * world;
}
