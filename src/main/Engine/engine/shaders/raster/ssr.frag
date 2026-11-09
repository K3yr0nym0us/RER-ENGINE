#version 450

// Mismos umbrales que el trazado (RT/BVH) y que composite, para que no queden bandas sin nadie
// que las resuelva. Ver docs/Reflections_SSR.yaml.
const float REFL_MIRROR_ROUGHNESS_MAX = 0.04;
const float REFL_MAX_ROUGHNESS = 0.70;

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D depth_tex;
layout(set = 0, binding = 1) uniform sampler2D normal_tex;
layout(set = 0, binding = 2) uniform sampler2D color_tex;
layout(location = 0) out vec4 out_color;

void main() {
    // Cada pase fullscreen con el viewport volteado (altura negativa) invierte la imagen.
    // Se muestrea en coordenadas invertidas de Y para que este pase sea neutro en orientación
    // y solo `present.frag` haga el único volteo del frame (independiente de cuántos pases
    // haya antes: sin esto, activar reflejos añadía un volteo extra y la pantalla se invertía).
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    float depth = texture(depth_tex, su).r;
    if (depth >= 0.9999) {
        out_color = vec4(0.0);
        return;
    }
    vec4 nr = texture(normal_tex, su);
    // Hasta REFL_MAX_ROUGHNESS; los espejos (<= REFL_MIRROR_ROUGHNESS_MAX) TAMBIEN se marchan aqui,
    // porque `composite.frag` delega en SSR justo esa banda. Antes SSR rechazaba <= 0.04 y > 0.45,
    // asi que cromo/agua/vidro no recibian ningun reflejo y entre 0.45 y 0.65 no habia ni SSR ni RT.
    if (nr.w > REFL_MAX_ROUGHNESS) {
        out_color = vec4(0.0);
        return;
    }
    vec2 n = nr.xy * 2.0 - 1.0;
    vec2 dir = vec2(n.x, -n.y) * (1.0 - nr.w);
    vec3 color = vec3(0.0);
    float confidence = 0.0;
    for (int i = 2; i <= 16; i++) {
        vec2 uv = su + dir * (0.012 * float(i));
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            break;
        }
        float hit = texture(depth_tex, uv).r;
        if (hit < depth - 0.0005) {
            color = texture(color_tex, uv).rgb;
            confidence = 1.0 - nr.w;
            break;
        }
    }
    out_color = vec4(color, confidence);
}
