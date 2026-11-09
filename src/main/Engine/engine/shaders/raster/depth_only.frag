#version 450

// Pase de solo-profundidad (sin adjuntos de color).
//
// Con MSAA el G-buffer se dibuja contra imágenes multisampleadas y se resuelve sobre las 1×; la
// profundidad multisampleada se descarta, así que `targets.depth` se rellena volviendo a rasterizar
// la geometría opaca. Este fragment shader NO escribe colores: solo repite el `discard` del alfa
// cortado de `forward.frag`, para que la profundidad coincida con la imagen resuelta. Sin él, las
// hojas/cristales con alfa escribirían profundidad donde el color no pinta nada.

layout(location = 2) in vec2 v_uv;
layout(location = 4) in vec4 v_tex;

layout(set = 1, binding = 0) uniform sampler2DArray albedo_tex;

layout(push_constant) uniform Push {
    uint joint_base;
    float pass_kind;
} pc;

void main() {
    vec4 albedo_s = texture(albedo_tex, vec3(v_uv, max(v_tex.x, 0.0)));
    if (albedo_s.a < 0.02) {
        discard;
    }
    // Mismo umbral de corte que el pase opaco de `forward.frag`.
    if (pc.pass_kind < 0.5 && albedo_s.a < 0.98) {
        discard;
    }
}
