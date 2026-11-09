#version 450

// Quad del fondo dentro del pase forward multisampleado.
//
// Con MSAA no se puede copiar el fondo 1× a la escena 2× (`vkCmdCopyImage` exige el mismo número
// de muestras), así que se dibuja un triángulo full-screen. Escribe las CINCO salidas del G-buffer
// aunque solo importa el color: si una ubicación de salida no se escribe, el adjunto correspondiente
// no queda garantizado, y normal/álbedo/mundo/velocidad se muestrean después en SSR, TAA y reflejos.
// El cielo no es superficie: normal, mundo y velocidad se dejan en el valor con el que se limpia el
// pase (0) y el álbedo queda a negro con alfa 0, que es lo que ya produce el CLEAR.

layout(location = 0) in vec2 v_uv;

layout(set = 0, binding = 0) uniform sampler2D source_tex;

layout(location = 0) out vec4 out_color;
layout(location = 1) out vec4 out_normal_rough;
layout(location = 2) out vec4 out_albedo;
layout(location = 3) out vec4 out_world;
layout(location = 4) out vec2 out_velocity;

void main() {
    // Muestreo invertido en Y: con el viewport del motor de altura negativa, leer en `1.0 - v_uv.y`
    // es lo que hace de este triángulo una copia 1:1 en memoria (misma regla que `present.frag`).
    out_color = vec4(texture(source_tex, vec2(v_uv.x, 1.0 - v_uv.y)).rgb, 1.0);
    out_normal_rough = vec4(0.0);
    out_albedo = vec4(0.0);
    out_world = vec4(0.0);
    out_velocity = vec2(0.0);
}
