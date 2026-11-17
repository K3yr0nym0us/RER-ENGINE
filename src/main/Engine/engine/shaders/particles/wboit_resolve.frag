#version 460
// Resolve de WBOIT (F3): compone acumulación + revealage sobre el HDR de escena.
//
//     final = scene · revealage + avg_color
//
// donde avg_color = accum.rgb / clamp(accum.a, 1e-5, ...). El resultado se escribe en
// una imagen dedicada y `copy_color` la lleva al HDR (muestrear y escribir `scene` en
// el mismo pase sería un feedback loop ilegal).

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D accum_tex;
layout(set = 0, binding = 1) uniform sampler2D reveal_tex;
layout(set = 0, binding = 2) uniform sampler2D scene_tex;

void main() {
    // Convención del motor (ver present.frag): el viewport tiene la altura negativa, así
    // que un pase fullscreen que muestrea `v_uv` a secas ESCRIBE ESPEJADO en Y — invertía
    // la pantalla entera al activar la niebla. Se muestrea en `1.0 - v_uv.y` para que el
    // resolve sea una copia neutra en la misma orientación que `scene`.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    vec4 acc = texture(accum_tex, su);
    float reveal = texture(reveal_tex, su).r;
    vec3 scene = texture(scene_tex, su).rgb;

    vec3 avg = acc.a > 0.0 ? acc.rgb / max(acc.a, 1e-5) : vec3(0.0);
    // El revealage es el producto Π(1 - alpha): 1.0 = sin transparencia acumulada.
    vec3 composited = scene * reveal + avg;
    out_color = vec4(composited, 1.0);
}
