#version 450

// SSR con preset por tier. Los prólogos siguen siendo compartidos con composite/reflect:
// los espejos (roughness <= 0.04) los resuelve este pase, por eso no se descartan aquí.
const float REFL_MIRROR_ROUGHNESS_MAX = 0.04;

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D depth_tex;
layout(set = 0, binding = 1) uniform sampler2D normal_tex;
layout(set = 0, binding = 2) uniform sampler2D color_tex;
// albedo.a = flag de material reflectivo calculado en forward.frag (mismo gate que el RT).
layout(set = 0, binding = 5) uniform sampler2D albedo_tex;
layout(location = 0) out vec4 out_color;

// 32 bytes. a = (march steps, binary steps, max distance, thickness)
//              b = (adaptive thickness, roughness support, edge fade, libre)
layout(push_constant) uniform SsrParams {
    vec4 a;
    vec4 b;
} pc;

void main() {
    // Un único volteo al presentar: este pase es neutro en orientación.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    // Mismo filtro de material que el RT: solo superficies con SurfacePbr de baja roughness.
    if (texture(albedo_tex, su).a < 0.5) {
        out_color = vec4(0.0);
        return;
    }
    float depth = texture(depth_tex, su).r;
    if (depth >= 0.9999) {
        out_color = vec4(0.0);
        return;
    }
    vec4 nr = texture(normal_tex, su);
    if (nr.w > pc.b.y) {
        out_color = vec4(0.0);
        return;
    }
    vec2 n = nr.xy * 2.0 - 1.0;
    vec2 dir = vec2(n.x, -n.y) * (1.0 - nr.w);

    int march = clamp(int(pc.a.x), 1, 128);
    int refine = clamp(int(pc.a.y), 0, 8);
    float max_dist = max(pc.a.z, 0.001);
    float step_len = max_dist / float(march);
    float thick_base = max(pc.a.w, 0.0001);

    vec3 color = vec3(0.0);
    float confidence = 0.0;
    float t = 0.0;
    for (int i = 0; i < 128; i++) {
        if (i >= march) {
            break;
        }
        t += step_len;
        vec2 uv = su + dir * t;
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            break;
        }
        float hit_depth = texture(depth_tex, uv).r;
        // La profundidad es no lineal (near 0.1 / far 1000): un impacto real a 1 m ya da
        // diff ~= 0.05. Un limite superior con `thickness` descartaba casi todos los hits.
        // Se usa como separacion MINIMA (adaptativa) y se acepta cualquier impacto delante.
        float min_sep = max(0.0005, thick_base * 0.25);
        if (pc.b.x > 0.5) {
            min_sep *= mix(1.0, 2.0, clamp(t / max_dist, 0.0, 1.0));
        }
        if (depth - hit_depth <= min_sep) {
            continue;
        }

        // Refinar el impacto con búsqueda binaria entre el último paso vacío y el actual.
        float lo = t - step_len;
        float hi = t;
        for (int b = 0; b < 8; b++) {
            if (b >= refine) {
                break;
            }
            float mid = 0.5 * (lo + hi);
            vec2 muv = su + dir * mid;
            if (texture(depth_tex, muv).r < depth) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        vec2 hit_uv = su + dir * hi;
        vec4 hit_albedo = texture(albedo_tex, hit_uv);
        vec3 scene_col = texture(color_tex, hit_uv).rgb;
        // Los transparentes llegan a `scene` diluidos con el fondo (alpha bajo), así que su
        // reflejo sería casi invisible. En su lugar usamos su albedo del G-buffer, sin mezclar.
        // albedo.a == 1 en opacos; en transparentes codifica el Fresnel en 0.55..1.
        color = hit_albedo.a < 0.999 ? mix(scene_col, hit_albedo.rgb, 0.85) : scene_col;
        vec2 edge = min(hit_uv, vec2(1.0) - hit_uv);
        float fade = smoothstep(0.0, max(pc.b.z, 0.001), min(edge.x, edge.y));
        confidence = (1.0 - nr.w) * fade;
        break;
    }
    out_color = vec4(color, confidence);
}
