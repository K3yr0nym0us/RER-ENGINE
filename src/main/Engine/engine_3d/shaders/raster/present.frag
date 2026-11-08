#version 450

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D source_tex;
layout(location = 0) out vec4 out_color;

void main() {
    // El viewport del motor tiene la altura negativa (para que las matrices NDC estilo wgpu del
    // HUD/gizmos y la proyección `perspective_rh` del port sigan valiendo). Con ese viewport, un
    // triángulo full-screen que muestree `v_uv` escribe espejado en Y: la fila 0 de la imagen de
    // origen terminaba en la última fila del swapchain.
    //
    // El G-buffer ya se guarda upright (fila 0 = arriba del mundo), así que TODOS los pases
    // fullscreen, incluido este, debe muestrear en `1.0 - v_uv.y` para ser copias neutras. Antes
    // sólo `present` volteaba y el resultado dependía de cuántos pases hubiera en la cadena: por
    // eso activar reflejos invertía la pantalla entera.
    out_color = vec4(texture(source_tex, vec2(v_uv.x, 1.0 - v_uv.y)).rgb, 1.0);
}
