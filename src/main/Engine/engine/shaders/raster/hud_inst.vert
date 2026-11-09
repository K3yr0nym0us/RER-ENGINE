#version 450

layout(location = 0) in vec3 position;
layout(location = 2) in vec2 uv;
layout(location = 3) in vec4 model0;
layout(location = 4) in vec4 model1;
layout(location = 5) in vec4 model2;
layout(location = 6) in vec4 model3;
layout(location = 7) in vec4 flag_pad;
layout(location = 8) in vec4 tex_layer_pad;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out float v_alpha;

void main() {
    mat4 model = mat4(model0, model1, model2, model3);
    gl_Position = model * vec4(position, 1.0);
    v_uv = mix(tex_layer_pad.xy, tex_layer_pad.zw, uv);
    v_alpha = flag_pad.y;
}
