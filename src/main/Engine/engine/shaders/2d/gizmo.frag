#version 450

// Port GLSL del `fs_main` de gizmo.wgsl: el color ya viene resuelto por el vértice.
layout(location = 0) in vec4 v_color;
layout(location = 0) out vec4 out_color;

void main() {
    out_color = v_color;
}
