#version 450

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(set = 0, binding = 0) uniform sampler2D atlas;
layout(location = 0) out vec4 out_color;

void main() {
    vec4 tex = texture(atlas, v_uv);
    out_color = vec4(tex.rgb * v_color.rgb, tex.a * v_color.a);
}
