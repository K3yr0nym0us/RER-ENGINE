#version 450

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D current_tex;
layout(set = 0, binding = 1) uniform sampler2D history_tex;
layout(set = 0, binding = 2) uniform sampler2D velocity_tex;
layout(location = 0) out vec4 out_color;

void main() {
    vec4 current = texture(current_tex, v_uv);
    vec2 velocity = texture(velocity_tex, v_uv).rg;
    vec2 history_uv = v_uv - velocity * 0.5;
    vec4 history = texture(history_tex, history_uv);
    bool inside = history_uv.x >= 0.0 && history_uv.x <= 1.0 && history_uv.y >= 0.0 && history_uv.y <= 1.0;
    if (!inside || history.a <= 0.0 || current.a <= 0.0) {
        out_color = current;
        return;
    }
    out_color = mix(history, current, 0.15);
}
