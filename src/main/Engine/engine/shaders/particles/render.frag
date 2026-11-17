#version 460
// Billboards con soft particles (F3): el alpha se desvanece contra la depth del
// forward cuando la partícula se acerca a una superficie opaca. Muestra la depth en
// binding 6 (layout READ_ONLY del pase). Modo de mezcla por push constant.
//
// Escritura directa en el HDR de escena (fuego aditivo, lluvia/nieve/arena alpha).
// El humo, la niebla y el agua van por el acumulador WBOIT (`wboit.frag`).

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec2 v_uv;
layout(location = 2) in vec3 v_spin_size;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 6) uniform sampler2D depth_tex;

layout(push_constant) uniform Push {
    mat4 view_proj;
    vec4 cam_right;
    vec4 cam_up;
    uvec4 misc;        // x = read_is_a, y = 0 aditivo / 1 alpha, z = soft, w = forma
    vec4 depth_params; // x near, y far, z suavidad (m), w reservado
} pc;

// Convierte un depth [0,1] a distancia en espacio de vista (positiva).
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

// Volutas de humo/polvo: el gaussiano solo produce una esfera; el FBM rota con
// `rot_seed.x` y abre huecos para que se lea como nube, no como disco.
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

// Grano de arena: cuadrado irregular con degradado, no un círculo geométrico.
float grain_shape(vec2 uv, float seed) {
    float ca = cos(seed);
    float sa = sin(seed);
    vec2 r = vec2(uv.x * ca - uv.y * sa, uv.x * sa + uv.y * ca);
    vec2 g = abs(r);
    float box = mix(max(g.x, g.y), g.x + g.y, 0.22);
    float n = hash21(vec2(seed, seed * 1.37));
    float edge = 0.52 + n * 0.16;
    float grain = 1.0 - smoothstep(edge, edge + 0.28, box);
    grain *= mix(0.55, 1.0, 1.0 - clamp(box * 0.85, 0.0, 1.0));
    return grain;
}

void main() {
    uint mat = pc.misc[3];
    vec3 rgb;
    float alpha;
    if (mat == 0u) {
        // Llama: óvalo vertical, núcleo caliente desplazado hacia arriba, borde irregular.
        vec2 q = vec2(v_uv.x * 1.45, v_uv.y * 0.72 - 0.18);
        float n = fract(sin(dot(v_uv, vec2(12.9898, 78.233))) * 43758.5453);
        float flame = length(q) + n * 0.1;
        if (flame > 1.0) {
            discard;
        }
        float shape = smoothstep(1.0, 0.12, flame);
        float core = smoothstep(0.38, 0.0, length(q + vec2(0.0, 0.12)));
        rgb = v_color.rgb * (shape + core * 0.7);
        alpha = v_color.a * shape;
    } else if (mat == 5u) {
        // Arena: granos finos irregulares; el polvo (tamaño > 4 cm) usa volutas de humo.
        float shape = v_spin_size.y > 0.04
            ? volute_shape(v_uv, v_spin_size.x)
            : grain_shape(v_uv, v_spin_size.x);
        if (shape <= 0.001) {
            discard;
        }
        rgb = v_color.rgb * shape;
        alpha = v_color.a * shape;
    } else if (mat == 3u || mat == 6u) {
        // Gota/chorro: más estrecha en X (el stretch del vértice alarga Y).
        vec2 drop = vec2(v_uv.x * 1.7, v_uv.y * 0.65);
        float dd = length(drop);
        if (dd > 1.0) {
            discard;
        }
        float shape = smoothstep(1.0, 0.2, dd);
        rgb = v_color.rgb * shape;
        alpha = v_color.a * shape;
    } else if (mat == 7u) {
        // Cubo de agua: blob redondo con borde brillante (aproximación de Fresnel —
        // el reflejo del agua vive en el borde de la gota/charco, no en el centro).
        float d = length(v_uv);
        if (d > 1.0) {
            discard;
        }
        float shape = smoothstep(1.0, 0.15, d);
        float rim = smoothstep(0.5, 0.9, d) * shape;
        rgb = v_color.rgb * (shape + rim * 0.55);
        alpha = v_color.a * shape;
    } else if (mat == 1u) {
        float shape = volute_shape(v_uv, v_spin_size.x);
        if (shape <= 0.001) {
            discard;
        }
        rgb = v_color.rgb * shape;
        alpha = v_color.a * shape;
    } else {
        // Nube (niebla / nieve): falloff gausiano sin núcleo.
        float d = length(v_uv);
        if (d > 1.0) {
            discard;
        }
        float shape = smoothstep(1.0, 0.0, d);
        shape *= shape;
        rgb = v_color.rgb * shape;
        alpha = v_color.a * shape;
    }

    // Soft particles: distancia entre la superficie opaca y el fragmento, en metros.
    if (pc.misc[2] == 1u) {
        vec2 uv = gl_FragCoord.xy / vec2(textureSize(depth_tex, 0));
        float scene_depth = texture(depth_tex, uv).x;
        float near = pc.depth_params.x;
        float far = pc.depth_params.y;
        float scene_z = linear_depth(scene_depth, near, far);
        float frag_z = linear_depth(gl_FragCoord.z, near, far);
        float fade = clamp((scene_z - frag_z) / max(pc.depth_params.z, 1e-3), 0.0, 1.0);
        alpha *= fade;
        if (alpha <= 0.0) {
            discard;
        }
    }

    if (pc.misc[1] == 1u) {
        // Alpha estándar: color directo, el factor SRC_ALPHA lo aplica el blend.
        out_color = vec4(rgb, alpha);
    } else {
        // Aditivo: el RGB ya lleva el alpha doblado (premultiplicado).
        out_color = vec4(rgb * alpha, alpha);
    }
}
