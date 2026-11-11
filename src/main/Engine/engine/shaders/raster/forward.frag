#version 450

layout(location = 0) in vec3 v_world;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec2 v_uv;
layout(location = 3) in vec4 v_flag;
layout(location = 4) in vec4 v_tex;
layout(location = 5) in vec4 v_curr;
layout(location = 6) in vec4 v_prev;

layout(set = 0, binding = 0) uniform Scene {
    mat4 view_proj;
    mat4 view_proj_stable;
    mat4 prev_view_proj;
    mat4 inv_view_proj;
    vec4 cam_pos;
    vec4 light_dir;
    vec4 light_color;
    mat4 light_view_proj;
    vec4 light_params;
    vec4 jitter;
    vec4 depth_plane;
    vec4 shadow_bias;
} u;

layout(set = 0, binding = 1) uniform sampler2DShadow shadow_map;
layout(set = 1, binding = 0) uniform sampler2DArray albedo_tex;
layout(set = 3, binding = 0) uniform highp samplerCubeArray probe_cube;
layout(push_constant) uniform Push {
    uint joint_base;
    float pass_kind;
} pc;

layout(location = 0) out vec4 out_color;
layout(location = 1) out vec4 out_normal_rough;
layout(location = 2) out vec4 out_albedo;
layout(location = 3) out vec4 out_world;
layout(location = 4) out vec2 out_velocity;

float shadow_pcf(vec3 world, vec3 n) {
    if (u.light_color.w < 0.5) {
        return 1.0;
    }
    float ndotl = max(dot(normalize(n), normalize(u.light_dir.xyz)), 0.0);
    float normal_scale = u.shadow_bias.x + u.shadow_bias.y * (1.0 - ndotl);
    vec3 biased = world + normalize(u.light_dir.xyz) * normal_scale;
    vec4 clip = u.light_view_proj * vec4(biased, 1.0);
    vec3 ndc = clip.xyz / clip.w;
    vec2 uv = ndc.xy * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }
    float slope = sqrt(max(1.0 - ndotl * ndotl, 0.0));
    float depth_ref = ndc.z - (u.shadow_bias.z + u.shadow_bias.w * slope);
    float texel = u.light_params.z;
    float radius = max(u.light_params.w, 1.0);
    // El radio ya escala con la resolución del mapa (lo decide `scene_light_params`). Con mapas
    // grandes el radio supera 2.5 texeles y conviene abrir el kernel a 5x5: así Ultra aporta más
    // taps además de más resolución, en lugar de dibujar solo más fino.
    int taps = radius > 2.5 ? 2 : 1;
    float sum = 0.0;
    float used = 0.0;
    for (int y = -taps; y <= taps; y++) {
        for (int x = -taps; x <= taps; x++) {
            vec2 off = vec2(float(x), float(y)) * texel * radius;
            sum += texture(shadow_map, vec3(uv + off, depth_ref));
            used += 1.0;
        }
    }
    return sum / used;
}

void main() {
    vec3 n = normalize(v_normal);
    float layer = max(v_tex.x, 0.0);
    vec4 albedo_s = texture(albedo_tex, vec3(v_uv, layer));
    vec3 albedo = albedo_s.rgb;
    float roughness = v_flag.w < 0.0 ? 0.5 : v_flag.w;
    float metallic = v_tex.y;
    vec3 l = normalize(u.light_dir.xyz);
    float ndotl = max(dot(n, l), 0.0);
    float shadow = shadow_pcf(v_world, n);
    float ambient = clamp(u.light_dir.w, 0.0, 1.0);
    // `light_params.y` es la oscuridad de sombra (0 = sin oscurecer, 1 = negro).
    // En zona iluminada shadow=1 -> factor 1; en zona oculta shadow=0 -> 1 - darkness.
    float shadow_darkness = clamp(u.light_params.y, 0.0, 1.0);
    float shadow_factor = 1.0 - shadow_darkness * (1.0 - shadow);
    vec3 lit = albedo * u.light_color.rgb
        * (ambient + (1.0 - ambient) * ndotl * shadow_factor)
        * u.light_params.x;
    vec3 v = normalize(u.cam_pos.xyz - v_world);
    float spec = pow(max(dot(n, normalize(l + v)), 0.0), mix(8.0, 64.0, 1.0 - roughness));
    if (u.depth_plane.z < 0.5) {
        lit += u.light_color.rgb * spec * (1.0 - roughness) * shadow_factor * 0.35;
    }
    // IBL de sondas. `v_tex.z` es la ranura del cubemap array asignada a esta instancia
    // (-1 = fuera de todo volumen de influencia). El peso viaja en `jitter.z` y solo es > 0
    // si hay sondas activas; sin sonda cercana se cae al color de cielo procedural.
    if (u.jitter.z > 0.0) {
        float probe_index = v_tex.z;
        vec3 probe_dir = normalize(mix(n, reflect(-v, n), clamp(1.0 - roughness, 0.0, 1.0)));
        vec3 probe = probe_index >= 0.0
            ? texture(probe_cube, vec4(probe_dir, probe_index)).rgb
            : vec3(0.72, 0.86, 0.98);
        lit += albedo * probe * u.jitter.z;
    }
    float flag = v_flag.x;
    float rim = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    if (flag > 1.5) {
        lit = mix(lit, vec3(0.12, 0.60, 0.88), 0.38);
        lit += vec3(0.15, 0.65, 0.90) * rim;
    } else if (flag > 0.5) {
        lit = mix(lit, vec3(1.0, 0.75, 0.10), 0.38);
        lit += vec3(1.0, 0.80, 0.15) * rim;
    }
    float alpha = albedo_s.a * v_flag.y;
    if (alpha < 0.02) {
        discard;
    }
    if (pc.pass_kind < 0.5 && alpha < 0.98) {
        discard;
    }
    if (pc.pass_kind >= 0.5 && alpha >= 0.98) {
        discard;
    }
    out_color = vec4(lit, alpha);
    out_normal_rough = vec4(n * 0.5 + 0.5, roughness);
    out_albedo = vec4(albedo, 1.0);
    out_world = vec4(v_world, metallic);
    vec2 curr = v_curr.xy / v_curr.w;
    vec2 prev = v_prev.xy / v_prev.w;
    out_velocity = curr - prev;
}
