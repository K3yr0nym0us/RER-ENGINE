#version 450

layout(location = 0) in vec2 v_uv;
layout(location = 1) in float v_alpha;
layout(set = 0, binding = 0) uniform sampler2D atlas;
layout(location = 0) out vec4 out_color;

void main() {
    vec4 tex = texture(atlas, v_uv);
    float alpha = tex.a * v_alpha;
    if (alpha < 0.02) {
        discard;
    }
    out_color = vec4(tex.rgb, alpha);
}
