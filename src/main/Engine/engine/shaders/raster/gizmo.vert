#version 450

layout(location = 0) in vec3 position;
layout(location = 1) in vec4 color;

layout(set = 0, binding = 0) uniform GizmoU {
    mat4 view_proj;
    mat4 model;
    vec4 flags;
    vec4 axis_start;
} u;

layout(location = 0) out vec4 v_color;

void main() {
    v_color = color;
    gl_Position = u.view_proj * u.model * vec4(position, 1.0);
}
