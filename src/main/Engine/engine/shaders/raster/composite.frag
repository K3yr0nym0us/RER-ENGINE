#version 450

// Umbral compartido con ssr.frag / reflect.rgen / trace.comp: por debajo, el reflejo lo da SSR.
const float REFL_MIRROR_ROUGHNESS_MAX = 0.04;

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D scene_tex;
layout(set = 0, binding = 1) uniform sampler2D reflection_tex;
layout(set = 0, binding = 2) uniform sampler2D ssr_tex;
layout(set = 0, binding = 3) uniform sampler2D normal_tex;
layout(set = 0, binding = 4) uniform sampler2D world_tex; // .w = metallic del G-buffer
layout(location = 0) out vec4 out_color;

void main() {
    // Muestreo en Y invertido: este pase debe conservar la orientación de la imagen de entrada
    // (ver `ssr.frag`). Solo `present.frag` aplica el volteo del frame.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    vec4 scene = texture(scene_tex, su);
    vec4 rt = texture(reflection_tex, su);
    vec4 ssr = texture(ssr_tex, su);
    float roughness = texture(normal_tex, su).w;
    vec3 reflection = rt.a > 0.01 ? rt.rgb : ssr.rgb;
    float confidence = rt.a > 0.01 ? rt.a : ssr.a;
    // Espejos los da SSR, pero solo si el pase SSR se ejecutó (en modo RT la imagen está limpia).
    if (roughness <= REFL_MIRROR_ROUGHNESS_MAX && ssr.a > 0.01) {
        reflection = ssr.rgb;
        confidence = ssr.a;
    }
    // F0 físico: dieléctricos reflejan poco en incidencia normal (~0.04, aquí 0.10 para
    // que se vea el reflejo sin tapar el color); metales reflejan mucho pero con el
    // color propio de la superficie. Así plástico/vidrio/agua conservan su textura y
    // cromo/acero siguen siendo metálicos sin parecer un espejo neutro.
    float metallic = clamp(texture(world_tex, su).w, 0.0, 1.0);
    // En modo RT la imagen SSR se limpia a 0; en modo SSR se limpia la trazada. Eso permite
    // dar a cada sistema su peso: el SSR ya lleva `1 - roughness` en su confianza y necesita
    // un peso alto, mientras que el RT usa F0 fisico para no tapar el color del material.
    bool ssr_mode = ssr.a > 0.01 && rt.a <= 0.01;
    float strength;
    vec3 final_reflection;
    if (ssr_mode) {
        strength = (1.0 - roughness) * confidence * 0.90;
        final_reflection = reflection;
    } else {
        float f0 = mix(0.10, 0.95, metallic);
        strength = f0 * confidence * (1.0 - roughness * 0.35);
        final_reflection = reflection * mix(vec3(1.0), scene.rgb, metallic);
    }
    out_color = vec4(mix(scene.rgb, final_reflection, clamp(strength, 0.0, 1.0)), scene.a);
}
