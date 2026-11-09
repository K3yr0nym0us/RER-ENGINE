#version 450

// Port GLSL del `vs_main` de gizmo.wgsl. El color viene por vértice; el eje se deduce del índice
// de vértice (48 vértices por eje, igual que en el WGSL).
layout(location = 0) in vec3 position;
layout(location = 1) in vec4 color;

layout(set = 0, binding = 0) uniform GizmoU {
    mat4 view_proj;
    mat4 model;
    vec4 flags; // x = hovered_axis (-1=none), y = active_axis (-1=none)
} u;

layout(location = 0) out vec4 v_color;

void main() {
    gl_Position = u.view_proj * u.model * vec4(position, 1.0);

    int axis = gl_VertexIndex / 48;
    int h_axis = int(u.flags.x);
    int a_axis = int(u.flags.y);

    vec4 col = color;
    if (a_axis == axis) {
        // Activo (arrastrando): blanco brillante.
        col = vec4(min(col.rgb * 2.0 + 0.35, vec3(1.0)), 1.0);
    } else if (h_axis == axis) {
        // Hover: más brillante.
        col = vec4(min(col.rgb * 1.55 + 0.12, vec3(1.0)), 1.0);
    }
    v_color = col;
}
