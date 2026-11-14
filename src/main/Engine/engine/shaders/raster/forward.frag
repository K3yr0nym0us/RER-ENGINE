#version 450

layout(location = 0) in vec3 v_world;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec2 v_uv;
layout(location = 3) in vec4 v_flag;
layout(location = 4) flat in vec4 v_tex;
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
    // x = near, y = far, z = reflejos, w = capa GPU de Grass.
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

float shadow_pcf(vec3 world, vec3 n, bool terrain) {
    if (u.light_color.w < 0.5) {
        return 1.0;
    }
    vec3 ldir = normalize(u.light_dir.xyz);
    float ndotl = max(dot(normalize(n), ldir), 0.0);
    float normal_scale = u.shadow_bias.x + u.shadow_bias.y * (1.0 - ndotl);
    // El mapa cubre el mundo entero (~1 m/texel). El bias de los modelos (0.4 mm)
    // no basta en el terreno: las tapas se auto-sombreaban. Unos texeles hacia
    // el sol quitan el acné y dejan la sombra de la montaña en el suelo.
    if (terrain) {
        float texel_m = max(u.jitter.w, 0.05);
        float slope_n = abs(dot(normalize(n), ldir));
        normal_scale += texel_m * (2.0 + 3.0 * (1.0 - slope_n));
    }
    vec3 biased = world + ldir * normal_scale;
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

// Un tile de material cubre 12.5 m (igual que la referencia). La cadena de mips
// del array elige sola el nivel según cuántos píxeles ocupa el tile: de lejos
// no se muestra el 1024.
const float TERRAIN_TEXTURE_SCALE = 0.08;

// Las fotos (sobre todo Stone) ya vienen oscuras. `pow(sRGB, 2.2)` las deja
// negras y la nieve, con el IBL del cielo, revienta en cian. Mismo camino que
// los modelos: muestreo UNORM y un color plano para que el sol se lea.
//
// `textureGrad` con derivadas solo de UV: si el LOD ve el índice de capa, Grass
// (primera foto, junto al checker) se mezcla con la capa 0 y se ve a cuadros.
vec3 sample_array_layer(vec2 uv, float layer) {
    float L = floor(layer + 0.5);
    return textureGrad(albedo_tex, vec3(uv, L), dFdx(uv), dFdy(uv)).rgb;
}

vec3 terrain_layer(vec2 tuv, float layer, vec3 tint) {
    return mix(tint, sample_array_layer(tuv, layer), 0.62);
}

float hash12(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

// Ruido de valor interpolado: sin la interpolación cada celda es un cuadro.
float value_noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 s = f * f * (3.0 - 2.0 * f);
    float a = hash12(i);
    float b = hash12(i + vec2(1.0, 0.0));
    float c = hash12(i + vec2(0.0, 1.0));
    float d = hash12(i + vec2(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

vec3 terrain_materials(vec3 n) {
    vec4 w = texture(albedo_tex, vec3(v_uv, max(v_tex.x, 0.0)));
    // El uniforme manda. `v_tex.w` se queda en 0 cuando el IOR del terreno pisa
    // la capa, y base 0 es el checker del editor: solo Grass (base+0) se ve a cuadros.
    float base = floor(u.depth_plane.w + 0.5);
    if (base < 0.5) {
        base = floor(v_tex.w + 0.5);
    }
    vec2 tuv = v_world.xz * TERRAIN_TEXTURE_SCALE;
    vec3 grass = terrain_layer(tuv, base + 0.0, vec3(0.31, 0.52, 0.24));
    vec3 dirt = terrain_layer(tuv, base + 1.0, vec3(0.47, 0.33, 0.18));
    vec3 clay = terrain_layer(tuv, base + 2.0, vec3(0.62, 0.40, 0.26));
    vec3 sand = terrain_layer(tuv, base + 3.0, vec3(0.77, 0.68, 0.45));
    vec3 stone = terrain_layer(tuv, base + 4.0, vec3(0.48, 0.46, 0.44));
    vec3 snow = terrain_layer(tuv, base + 5.0, vec3(0.90, 0.92, 0.94));

    float slope = clamp(1.0 - n.y, 0.0, 1.0);
    float dirt_k = smoothstep(0.12, 0.38, slope);
    vec2 clay_p = v_world.xz * 0.04;
    float clay_n = value_noise(clay_p) * 0.65 + value_noise(clay_p * 2.7 + 17.0) * 0.35;
    float clay_k = (1.0 - dirt_k) * smoothstep(0.60, 0.78, clay_n);
    vec3 ground = mix(mix(grass, clay, clay_k), dirt, dirt_k);
    float sum = max(w.r + w.g + w.b + w.a, 0.001);
    vec3 rgb = (ground * w.r + stone * w.g + sand * w.b + snow * w.a) / sum;
    return rgb;
}

void main() {
    vec3 n = normalize(v_normal);
    bool terrain = v_flag.z > 3.5 && v_flag.z < 4.5;
    vec4 albedo_s;
    vec3 albedo;
    if (terrain) {
        // Cara vista desde dentro (aleros, bordes recortados): la normal se
        // orienta hacia la cámara antes de medir la pendiente y la luz.
        vec3 to_eye = normalize(u.cam_pos.xyz - v_world);
        if (dot(n, to_eye) < 0.0) {
            n = -n;
        }
        albedo = terrain_materials(n);
        albedo_s = vec4(albedo, 1.0);
    } else {
        float layer = max(v_tex.x, 0.0);
        albedo_s = texture(albedo_tex, vec3(v_uv, layer));
        albedo = albedo_s.rgb;
    }
    float roughness = v_flag.w < 0.0 ? 0.5 : v_flag.w;
    float metallic = v_tex.y;
    vec3 l = normalize(u.light_dir.xyz);
    float ndotl = max(dot(n, l), 0.0);
    float shadow = shadow_pcf(v_world, n, terrain);
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
    if (!terrain && u.jitter.z > 0.0) {
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
