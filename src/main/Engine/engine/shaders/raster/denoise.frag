#version 450

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D color_tex;
layout(set = 0, binding = 1) uniform sampler2D depth_tex;
layout(set = 0, binding = 2) uniform sampler2D normal_tex;
layout(push_constant) uniform Push { float step_px; } pc;
layout(location = 0) out vec4 out_color;

void main() {
    // Ver `ssr.frag`: muestreo en Y invertido para conservar la orientación de la entrada.
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    vec2 texel = 1.0 / vec2(textureSize(color_tex, 0));
    vec4 center = texture(color_tex, su);
    if (center.a <= 0.0) {
        out_color = center;
        return;
    }
    float depth = texture(depth_tex, su).r;
    vec3 normal = texture(normal_tex, su).xyz;
    vec4 sum = center;
    float weight = 1.0;
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            if (x == 0 && y == 0) {
                continue;
            }
            vec2 uv = su + vec2(x, y) * texel * pc.step_px;
            vec4 sample_c = texture(color_tex, uv);
            float d = texture(depth_tex, uv).r;
            vec3 n = texture(normal_tex, uv).xyz;
            float w = exp(-abs(d - depth) * 80.0) * pow(max(dot(n, normal), 0.0), 16.0) * sample_c.a;
            sum += sample_c * w;
            weight += w;
        }
    }
    out_color = sum / weight;
}
