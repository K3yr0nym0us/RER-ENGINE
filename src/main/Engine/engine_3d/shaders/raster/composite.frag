#version 450

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D scene_tex;
layout(set = 0, binding = 1) uniform sampler2D reflection_tex;
layout(set = 0, binding = 2) uniform sampler2D ssr_tex;
layout(set = 0, binding = 3) uniform sampler2D normal_tex;
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
    if (roughness <= 0.04) {
        reflection = ssr.rgb;
        confidence = ssr.a;
    }
    float strength = (1.0 - roughness) * confidence;
    out_color = vec4(mix(scene.rgb, reflection, clamp(strength, 0.0, 1.0)), scene.a);
}
