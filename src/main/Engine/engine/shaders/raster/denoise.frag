#version 450

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D color_tex;
layout(set = 0, binding = 1) uniform sampler2D depth_tex;
layout(set = 0, binding = 2) uniform sampler2D normal_tex;
// x = step_px, y = near, z = far, w = modo (1 = SSR, 2 = RT).
layout(push_constant) uniform Push {
    vec4 params;
} pc;
layout(location = 0) out vec4 out_color;

// Solo SSR: profundidad de pantalla no lineal [0,1] -> metros en espacio de vista.
// near/far vienen del push; sin rango válido se usa la profundidad cruda (mismo comportamiento
// que el filtro histórico).
float linear_depth(float d, float near, float far) {
    return (near * far) / max(far - d * (far - near), 1e-5);
}

vec3 decode_normal(vec3 encoded) {
    return normalize(encoded * 2.0 - 1.0);
}

void main() {
    // Ver `ssr.frag`: muestreo en Y invertido para conservar la orientación de la entrada.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    vec2 texel = 1.0 / vec2(textureSize(color_tex, 0));
    vec4 center = texture(color_tex, su);
    if (center.a <= 0.0) {
        out_color = center;
        return;
    }
    vec4 sum = center;
    float weight = 1.0;

    // RT: filtro histórico CALCADO (normales codificadas, depth no lineal, exp*80).
    // Cualquier modo que no sea SSR explícito conserva este camino.
    if (int(pc.params.w + 0.5) != 1) {
        float depth = texture(depth_tex, su).r;
        vec3 normal = texture(normal_tex, su).xyz;
        for (int y = -1; y <= 1; y++) {
            for (int x = -1; x <= 1; x++) {
                if (x == 0 && y == 0) {
                    continue;
                }
                vec2 uv = su + vec2(x, y) * texel * pc.params.x;
                vec4 sample_c = texture(color_tex, uv);
                float d = texture(depth_tex, uv).r;
                vec3 n = texture(normal_tex, uv).xyz;
                float w = exp(-abs(d - depth) * 80.0) * pow(max(dot(n, normal), 0.0), 16.0) * sample_c.a;
                sum += sample_c * w;
                weight += w;
            }
        }
        out_color = sum / weight;
        return;
    }

    // SSR: decodificar/normalizar normales y linealizar profundidad ANTES del dot y del diff.
    float near = pc.params.y;
    float far = pc.params.z;
    bool have_range = near > 0.0 && far > near;
    float center_depth = texture(depth_tex, su).r;
    float depth_lin = have_range ? linear_depth(center_depth, near, far) : center_depth;
    vec3 normal = decode_normal(texture(normal_tex, su).xyz);
    // Color y confianza se acumulan por separado: la confianza no debe teñir el RGB del reflejo.
    vec3 rgb_sum = center.rgb;
    float conf_sum = center.a;
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            if (x == 0 && y == 0) {
                continue;
            }
            vec2 uv = su + vec2(x, y) * texel * pc.params.x;
            vec4 sample_c = texture(color_tex, uv);
            float d = texture(depth_tex, uv).r;
            float d_lin = have_range ? linear_depth(d, near, far) : d;
            vec3 n = decode_normal(texture(normal_tex, uv).xyz);
            // Escala de 0.125 m: conserva el borde de objetos finos y evita sangrar entre
            // planos alejados; el grosor de impacto del rayo sigue siendo otro parámetro.
            float w = exp(-abs(d_lin - depth_lin) * 8.0) * pow(max(dot(n, normal), 0.0), 16.0);
            rgb_sum += sample_c.rgb * w;
            conf_sum += sample_c.a * w;
            weight += w;
        }
    }
    out_color = vec4(rgb_sum / weight, conf_sum / weight);
}
