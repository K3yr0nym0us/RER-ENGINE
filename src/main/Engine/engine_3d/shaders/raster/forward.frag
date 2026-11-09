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
    mat4 light_view_proj[4];
    vec4 cascade_splits;
    vec4 cascade_extent;
    vec4 shadow_params;
    vec4 light_params;
    vec4 jitter;
    vec4 depth_plane;
    vec4 shadow_bias;
} u;

layout(set = 0, binding = 1) uniform sampler2DArrayShadow shadow_map;
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

/// Cascada que cubre un punto, por su distancia al ojo. Los cortes vienen en `cascade_splits`
/// (borde lejano de cada cascada) y las casillas inactivas valen FLT_MAX, así que el primer
/// `split` que encaja es siempre la cascada correcta. Devuelve -1 más allá del borde de sombras.
int shadow_cascade(float view_dist) {
    int count = int(u.shadow_params.w);
    if (count <= 0 || count > 4) {
        return -1;
    }
    if (view_dist > u.cascade_splits[count - 1]) {
        return -1;
    }
    for (int i = 0; i < 4; i++) {
        if (i < count && view_dist <= u.cascade_splits[i]) {
            return i;
        }
    }
    return count - 1;
}

float shadow_pcf(vec3 world, vec3 n) {
    if (u.light_color.w < 0.5) {
        return 1.0;
    }
    float view_dist = distance(world, u.cam_pos.xyz);
    int cascade = shadow_cascade(view_dist);
    if (cascade < 0) {
        return 1.0;
    }
    int count = int(u.shadow_params.w);
    float ndotl = max(dot(normalize(n), normalize(u.light_dir.xyz)), 0.0);
    float normal_scale = u.shadow_bias.x + u.shadow_bias.y * (1.0 - ndotl);
    vec3 biased = world + normalize(u.light_dir.xyz) * normal_scale;
    vec4 clip = u.light_view_proj[cascade] * vec4(biased, 1.0);
    vec3 ndc = clip.xyz / clip.w;
    vec2 uv = ndc.xy * 0.5 + 0.5;
    uv.y = 1.0 - uv.y;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return 1.0;
    }
    float slope = sqrt(max(1.0 - ndotl * ndotl, 0.0));
    float depth_ref = ndc.z - (u.shadow_bias.z + u.shadow_bias.w * slope);
    // El radio PCF se pide en UNIDADES DE MUNDO y se traduce a texeles de ESTE mapa. Con un radio
    // fijo en texeles, la cascada lejana (misma resolución, más mundo por texel) tendría una
    // penumbra durísima y el cambio de cascada se vería como un corte.
    float texel_world = 2.0 * max(u.cascade_extent[cascade], 0.001) * u.shadow_params.z;
    float radius = clamp(u.shadow_params.x / max(texel_world, 1e-5), 1.0, 4.0);
    float texel = u.shadow_params.z;
    // Con radio abierto conviene 5x5 taps: más penumbra sin más resolución de mapa.
    int taps = radius > 2.5 ? 2 : 1;
    float sum = 0.0;
    float used = 0.0;
    for (int y = -taps; y <= taps; y++) {
        for (int x = -taps; x <= taps; x++) {
            vec2 off = vec2(float(x), float(y)) * texel * radius;
            sum += texture(shadow_map, vec4(uv + off, depth_ref, float(cascade)));
            used += 1.0;
        }
    }
    float shadow = sum / used;
    // Costura entre cascadas: en la última franja de cada cascada (excepto la última) se desvanece
    // hacia "sin sombra" para que el salto de densidad de texel no se lea como una línea dura.
    if (cascade + 1 < count) {
        float edge = u.cascade_splits[cascade];
        float thickness = clamp(u.shadow_params.y, 0.0, 0.9);
        float k = smoothstep(edge * (1.0 - thickness), edge, view_dist);
        shadow = mix(shadow, 1.0, k);
    }
    return shadow;
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
    vec3 lit = albedo * u.light_color.rgb * (ambient + (1.0 - ambient) * ndotl * shadow) * u.light_params.x;
    vec3 v = normalize(u.cam_pos.xyz - v_world);
    float spec = pow(max(dot(n, normalize(l + v)), 0.0), mix(8.0, 64.0, 1.0 - roughness));
    if (u.depth_plane.z < 0.5) {
        lit += u.light_color.rgb * spec * (1.0 - roughness) * shadow * 0.35;
    }
    // IBL de sondas. `v_tex.z` es la ranura del cubemap array asignada a esta instancia
    // (-1 = fuera de todo volumen de influencia). El peso viene en `light_params.y` y solo es > 0
    // si hay sondas activas; sin sonda cercana se cae al color de cielo procedural, que es el que
    // ya mostraba el viewport, así el cambio de una sonda no deja la zona negra.
    if (u.light_params.y > 0.0) {
        float probe_index = v_tex.z;
        vec3 probe_dir = normalize(mix(n, reflect(-v, n), clamp(1.0 - roughness, 0.0, 1.0)));
        vec3 probe = probe_index >= 0.0
            ? texture(probe_cube, vec4(probe_dir, probe_index)).rgb
            : vec3(0.72, 0.86, 0.98);
        lit += albedo * probe * u.light_params.y;
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
