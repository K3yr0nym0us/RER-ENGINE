#version 450

layout(location = 0) in vec3 position;
layout(location = 1) in vec4 color;

layout(set = 0, binding = 0) uniform GizmoU {
    mat4 view_proj;
    mat4 model;
    vec4 flags;      // x = eje resaltado, y = eje activo (-1 = ninguno)
    vec4 axis_start; // inicio de cada flecha en unidades del mesh (antes de escala)
} u;

layout(location = 0) out vec4 v_color;

// El color del vértice codifica el eje (X rojo, Y verde, Z azul), igual que
// `gizmo_axis_from_color` en el hit-testing del editor. Debe coincidir con
// los colores de `build_axes` (gizmo.rs).
int gizmo_axis_from_color(vec4 col) {
    if (col.r > col.g + 0.05 && col.r > col.b + 0.05) return 0;
    if (col.g > col.r + 0.05 && col.g > col.b + 0.05) return 1;
    return 2;
}

void main() {
    int axis = gizmo_axis_from_color(color);

    // Desplazar la flecha hacia fuera de la selección. Sin esto, el shader
    // pinta desde el centro mientras `pick_translate_gizmo_axis` testea el
    // segmento desplazado: el click cae en la entidad y sale movimiento libre.
    vec3 pos = position;
    if (axis == 0) {
        pos.x += u.axis_start.x;
    } else if (axis == 1) {
        pos.y += u.axis_start.y;
    } else {
        pos.z += u.axis_start.z;
    }

    vec4 shaded = color;
    int hovered_axis = int(u.flags.x);
    int active_axis = int(u.flags.y);
    if (active_axis == axis) {
        shaded = vec4(min(color.rgb * 2.0 + vec3(0.35), vec3(1.0)), 1.0);
    } else if (hovered_axis == axis) {
        shaded = vec4(min(color.rgb * 1.55 + vec3(0.12), vec3(1.0)), 1.0);
    }

    v_color = shaded;
    gl_Position = u.view_proj * u.model * vec4(pos, 1.0);
}
