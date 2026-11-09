#version 450

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D current_tex;
layout(set = 0, binding = 1) uniform sampler2D history_tex;
layout(set = 0, binding = 2) uniform sampler2D velocity_tex;
layout(push_constant) uniform Push { float blend; float enabled; } pc;
layout(location = 0) out vec4 out_color;

void main() {
    // Ver `ssr.frag`: se muestrea en Y invertido para que este pase no añada un volteo extra.
    // La velocidad llega en el espacio del G-buffer, así que su componente Y también se invierte.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    vec3 current = texture(current_tex, su).rgb;
    if (pc.enabled < 0.5) {
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
    float blend = inside ? pc.blend : 1.0;
    out_color = vec4(mix(history, current, blend), 1.0);
}
