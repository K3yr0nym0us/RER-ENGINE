#version 450

// Port GLSL del `vs_main` de shader.wgsl. El layout de vértice e instancia tiene que coincidir
// byte a byte con `mesh::Vertex` (32 B) y `mesh::InstanceData` (96 B); las pruebas de
// `src/vulkan/layout.rs` fijan esos offsets.
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
    vec4 cam_pos;
} u;

layout(location = 0) out vec3 v_world_pos;
layout(location = 1) out vec3 v_world_normal;
layout(location = 2) out vec2 v_uv;
layout(location = 3) out float v_flag;
layout(location = 4) out float v_alpha_mul;
layout(location = 5) out float v_render_kind;
layout(location = 6) out vec2 v_uv_center;

void main() {
    mat4 model = mat4(model0, model1, model2, model3);
    vec4 world = model * vec4(position, 1.0);
    gl_Position = u.view_proj * world;
    v_world_pos = world.xyz;
    v_world_normal = normalize((model * vec4(normal, 0.0)).xyz);
    // Remap UV 0..1 a la sub-región del atlas: uv_rect.xy = min, uv_rect.zw = max.
    v_uv = uv_rect.xy + uv * (uv_rect.zw - uv_rect.xy);
    v_flag = flag_pad.x;
    v_alpha_mul = flag_pad.y;
    v_render_kind = flag_pad.z;
    v_uv_center = (uv_rect.xy + uv_rect.zw) * 0.5;
}
