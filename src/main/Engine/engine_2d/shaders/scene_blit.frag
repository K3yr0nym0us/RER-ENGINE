#version 450

// Port GLSL del `fs_main` de scene_blit.wgsl.
//
// El viewport del motor tiene altura negativa (misma convención que el 3D). Con ese viewport un
// triángulo full-screen que muestree `v_uv` tal cual escribe la imagen espejada en Y: por eso se
// muestrea `1.0 - v_uv.y`, exactamente igual que el `present` del 3D. El escenario del 2D ya se
// guarda upright, así que este blit tiene que ser una copia neutra.
layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D t_color;
layout(location = 0) out vec4 out_color;

void main() {
    out_color = texture(t_color, vec2(v_uv.x, 1.0 - v_uv.y));
}
