#version 450

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D current_tex;
layout(set = 0, binding = 1) uniform sampler2D history_tex;
layout(set = 0, binding = 2) uniform sampler2D velocity_tex;
// Rechazo de historial de SSR: el `full` layout ya tiene 6 samplers; los bindings 3/4 se
// comparten con composite (3 = profundidad aquí, 4 = normal aquí) y deben estar escritos siempre.
layout(set = 0, binding = 3) uniform sampler2D depth_tex;
layout(set = 0, binding = 4) uniform sampler2D normal_tex;
// x = blend, y = modo/enabled (0 = off, 1 = RT/escena como antes, 2 = SSR), z = near, w = far.
layout(push_constant) uniform Push {
    vec4 params;
} pc;
layout(location = 0) out vec4 out_color;

float linear_depth(float d, float near, float far) {
    return (near * far) / max(far - d * (far - near), 1e-5);
}

vec3 decode_normal(vec3 encoded) {
    return normalize(encoded * 2.0 - 1.0);
}

void main() {
    // Ver `ssr.frag`: se muestrea en Y invertido para que este pase no añada un volteo extra.
    // La velocidad llega en el espacio del G-buffer, así que su componente Y también se invierte.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    vec3 current = texture(current_tex, su).rgb;
    int mode = int(clamp(pc.params.y + 0.5, 0.0, 2.0));
    if (mode == 0) {
        out_color = vec4(current, 1.0);
        return;
    }
    vec2 velocity = texture(velocity_tex, su).rg;
    velocity.y = -velocity.y;
    vec2 history_uv = su - velocity * 0.5;
    vec3 history = texture(history_tex, history_uv).rgb;
    vec3 near_min = current;
    vec3 near_max = current;
    vec2 texel = 1.0 / vec2(textureSize(current_tex, 0));
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec3 s = texture(current_tex, su + vec2(x, y) * texel).rgb;
            near_min = min(near_min, s);
            near_max = max(near_max, s);
        }
    }
    history = clamp(history, near_min, near_max);
    bool inside = history_uv.x >= 0.0 && history_uv.x <= 1.0 && history_uv.y >= 0.0 && history_uv.y <= 1.0;
    float blend = inside ? pc.params.x : 0.0;

    // ── SSR únicamente: rechazar historial por disoclusión / profundidad / normal. ──
    // RT (mode 1) NO entra aquí: conserva el blend y la salida históricos, sin leer depth/normal.
    if (mode == 2 && inside) {
        float near = pc.params.z;
        float far = pc.params.w;
        bool have_range = near > 0.0 && far > near;
        float cd = texture(depth_tex, su).r;
        float hd = texture(depth_tex, history_uv).r;
        if (have_range) {
            cd = linear_depth(cd, near, far);
            hd = linear_depth(hd, near, far);
        }
        vec3 cn = decode_normal(texture(normal_tex, su).xyz);
        vec3 hn = decode_normal(texture(normal_tex, history_uv).xyz);
        // 5 cm o 2% de la distancia actual: tolera la fluctuación del jitter sin perseguir
        // geometría que ha entrado en pantalla (disoclusión).
        float depth_threshold = max(0.05, 0.02 * cd);
        bool disocclusion = abs(hd - cd) > depth_threshold;
        bool normal_reject = dot(cn, hn) < 0.90;
        if (disocclusion || normal_reject) {
            blend = 0.0;
        }
    }

    out_color = vec4(mix(current, history, blend), 1.0);
}
