#version 460
// WBOIT (F3): pase de acumulación para humo/niebla/agua, sin ordenar partículas en CPU.
//
// Dos adjuntos (McGuire & Bavoil 2013, variante de adrien-ben/vulkan-examples-rs):
// - accum (RGBA16F): `rgb = color * w`, `a = w` con blending aditivo ONE/ONE.
// - reveal (R16F): `r = 1 - alpha` con blending multiplicativo (dst *= 1 - src.r).
// El resolve (`wboit_resolve.frag`) combina con el HDR de escena.
//
// weight = alpha · (1 - gl_FragCoord.z)²: premia las capas cercanas a cámara, estable
// para billboards de humo/niebla en el rango de profundidad del mundo.

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec2 v_uv;
layout(location = 2) in vec3 v_spin_size;
layout(location = 0) out vec4 accum;
layout(location = 1) out float reveal;

layout(set = 0, binding = 6) uniform sampler2D depth_tex;

layout(push_constant) uniform Push {
    mat4 view_proj;
    vec4 cam_right;
    vec4 cam_up;
    uvec4 misc;        // x = read_is_a, y = mezcla, z = soft, w = forma (1 = nube)
    vec4 depth_params; // x near, y far, z suavidad (m)
} pc;

float linear_depth(float d01, float near, float far) {
    float z_ndc = d01 * 2.0 - 1.0;
    return 2.0 * near * far / (far + near - z_ndc * (far - near));
}

float hash21(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise21(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm21(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * noise21(p);
        p = p * 2.03 + vec2(1.7, 9.2);
        a *= 0.5;
    }
    return v;
}

float volute_shape(vec2 uv, float seed) {
    float d = length(uv);
    if (d > 1.0) {
        return 0.0;
    }
    float ca = cos(seed);
    float sa = sin(seed);
    vec2 r = vec2(uv.x * ca - uv.y * sa, uv.x * sa + uv.y * ca);
    vec2 q = r * 2.4 + vec2(seed * 1.7, seed * 0.9);
    q += vec2(sin(r.y * 3.1 + seed), cos(r.x * 2.7 - seed)) * 0.35;
    float n = fbm21(q);
    float envelope = smoothstep(1.0, 0.12, d);
    envelope *= envelope;
    float wisps = smoothstep(0.22, 0.78, n);
    return envelope * mix(0.12, 1.0, wisps) * (0.45 + 0.55 * n);
}

void main() {
    uint mat = pc.misc[3];
    vec3 rgb;
    float alpha;
    if (mat == 6u) {
        vec2 drop = vec2(v_uv.x * 1.7, v_uv.y * 0.65);
        float dd = length(drop);
        if (dd > 1.0) {
            discard;
        }
        float shape = smoothstep(1.0, 0.2, dd);
        rgb = v_color.rgb * shape;
        alpha = v_color.a * shape;
    } else if (mat == 1u) {
        float shape = volute_shape(v_uv, v_spin_size.x);
        if (shape <= 0.001) {
            discard;
        }
        rgb = v_color.rgb * shape;
        alpha = v_color.a * shape;
    } else {
        // Nube (niebla): falloff gausiano sin núcleo.
        float d = length(v_uv);
        if (d > 1.0) {
            discard;
        }
        float shape = smoothstep(1.0, 0.0, d);
        shape *= shape;
        rgb = v_color.rgb * shape;
        alpha = v_color.a * shape;
    }

    // Soft particles: mismo desvanecimiento que el pase directo.
    // El splash de agua (z >= 3) salta cerca del suelo: la suavidad de 0.4 m
    // lo borrarían por completo.
    if (pc.misc[2] == 1u && v_spin_size.z < 3.0) {
        vec2 uv = gl_FragCoord.xy / vec2(textureSize(depth_tex, 0));
        float scene_depth = texture(depth_tex, uv).x;
        float scene_z = linear_depth(scene_depth, pc.depth_params.x, pc.depth_params.y);
        float frag_z = linear_depth(gl_FragCoord.z, pc.depth_params.x, pc.depth_params.y);
        alpha *= clamp((scene_z - frag_z) / max(pc.depth_params.z, 1e-3), 0.0, 1.0);
        if (alpha <= 0.0) {
            discard;
        }
    }

    float w = alpha * pow(max(1.0 - gl_FragCoord.z, 1e-3), 2.0);
    // El blending del adjunto es ONE/ONE: se suman los pesos.
    accum = vec4(rgb * w, w);
    // El blending de reveal es multiplicativo sobre el canal R: dst *= (1 - src.r).
    // El shader emite alpha y el factor ONE_MINUS_SRC_COLOR deja Π(1 - alpha).
    reveal = alpha;
}
