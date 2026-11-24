#version 450

// Umbral compartido con ssr.frag / reflect.rgen / trace.comp: por debajo, el reflejo lo da SSR.
const float REFL_MIRROR_ROUGHNESS_MAX = 0.04;

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D scene_tex;
layout(set = 0, binding = 1) uniform sampler2D reflection_tex;
layout(set = 0, binding = 2) uniform sampler2D ssr_tex;
layout(set = 0, binding = 3) uniform sampler2D normal_tex;
layout(set = 0, binding = 4) uniform sampler2D world_tex; // .w = metallic del G-buffer
// .a = flag reflectivo; en transparentes trae el Fresnel codificado por forward.frag.
layout(set = 0, binding = 5) uniform sampler2D albedo_tex;
// x = modo de reflejos: 0 = Off, 1 = SSR, 2 = RT. El modo viene del push, no se infiere de alfas.
layout(push_constant) uniform Push {
    vec4 params;
} pc;
layout(location = 0) out vec4 out_color;

void main() {
    // Muestreo en Y invertido: este pase debe conservar la orientación de la imagen de entrada
    // (ver `ssr.frag`). Solo `present.frag` aplica el volteo del frame.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    vec4 scene = texture(scene_tex, su);
    vec4 rt = texture(reflection_tex, su);
    vec4 ssr = texture(ssr_tex, su);
    float roughness = texture(normal_tex, su).w;
    float metallic = clamp(texture(world_tex, su).w, 0.0, 1.0);
    vec4 albedo_s = texture(albedo_tex, su);
    // albedo.a NO es opacidad: en transparentes codifica el Fresnel (0.55..1) calculado en
    // forward con la cámara real. La opacidad de salida es `scene.a`, como siempre.
    float refl_flag = albedo_s.a;
    float fresnel = refl_flag >= 0.5 ? clamp((refl_flag - 0.55) / 0.45, 0.0, 1.0) : 1.0;
    float fresnel_weight = mix(0.40, 1.0, fresnel);
    int mode = int(clamp(pc.params.x + 0.5, 0.0, 2.0));

    vec3 final_reflection = scene.rgb;
    float strength = 0.0;

    if (mode == 2) {
        // ── RT: fórmula histórica, calcada. No se toca nada de esta rama. ──
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
        float f0 = mix(0.10, 0.95, metallic);
        strength = f0 * confidence * (1.0 - roughness * 0.35);
        final_reflection = reflection * mix(vec3(1.0), scene.rgb, metallic);
    } else if (mode == 1) {
        // ── SSR: F0/metallic + Fresnel/roughness + confianza, SIN fuerza plana 0.90. ──
        // La imagen SSR limpia a 0 en miss: confianza 0 ⇒ strength 0 ⇒ se devuelve `scene`,
        // que en SSR conserva el spec directo y las sondas del forward. Nunca hay miss negro.
        vec3 reflection = ssr.rgb;
        float confidence = ssr.a;
        float f0 = mix(0.04, 1.0, metallic); // F0 físico (dieléctrico 0.04, metal = albedo).
        float gloss = 1.0 - roughness;        // El pulido modula el Fresnel, no una constante.
        strength = f0 * gloss * confidence;
        // El G-buffer/depth de SSR es opaco. Si el color de escena tiene cobertura transparente,
        // el reflejo del opaco subyacente se ve atenuado al atravesar vidrio/agua.
        strength *= clamp(scene.a, 0.0, 1.0);
        // Tinte metálico con el albedo del G-buffer, no con la escena ya iluminada.
        final_reflection = reflection * mix(vec3(1.0), albedo_s.rgb, metallic);
    }
    // Off (mode 0): strength = 0, la escena pasa intacta.

    strength *= fresnel_weight;
    out_color = vec4(mix(scene.rgb, final_reflection, clamp(strength, 0.0, 1.0)), scene.a);
}
