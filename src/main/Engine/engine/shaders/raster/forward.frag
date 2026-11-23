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
    vec4 aura_color; // rgb = color del aura de interacción
} u;

layout(set = 0, binding = 1) uniform sampler2DShadow shadow_map;

// Sombras volumétricas: el humo/niebla se agrupa en una textura 3D de transmitancia,
// T = exp(-sigma * integral de densidad hacia el sol), calculada en el pase volumétrico.
// Una lectura por fragmento proyecta la sombra del medio sobre esta superficie.
layout(set = 0, binding = 2) uniform sampler3D volume_light;
layout(set = 0, binding = 3) uniform VolParams {
    vec4 center;        // xyz = centro del volumen (mundo)
    vec4 half_extent;   // xyz = semiextensión (m)
    vec4 resolution;
    vec4 light_dir;
    vec4 params;        // w = intensidad de la proyección sobre superficies
    vec4 history_center;
    vec4 history_meta;
} vol;

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
    float slope_n = abs(dot(normalize(n), ldir));
    // El bias debe escalar con el texel del shadow map, no ser metros fijos:
    // con 1024 px sobre todo el mundo un texel son ~0.119 m, pero el bias de
    // los modelos era 0.0004 m (~0.003 texel). Cada superficie (ndotl distinto)
    // oscilaba al cruzar el umbral del comparador, a su propio ritmo. Escalar
    // con u.jitter.w (metros/texel) lo hace estable y se autoajusta al tier:
    // mas resolucion = texel menor = menos desplazamiento = salto menos visible.
    float texel_m = max(u.jitter.w, 0.05);
    float normal_scale = u.shadow_bias.x + u.shadow_bias.y * (1.0 - ndotl)
        + texel_m * (1.5 + 2.0 * (1.0 - slope_n));
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
    // Muestrear CENTROS de texel, no offsets fraccionarios. Con radio 0.85/1.7/2.4
    // los taps caian entre texeles y el sampler de comparacion LINEAL interpelaba
    // distinto en cada frame mientras el sol movia el UV -> temblor (mas rapido
    // cuanto mas tier, porque hay mas cruces de centro por segundo).
    int reach = radius > 2.5 ? 2 : 1;
    float sum = 0.0;
    float used = 0.0;
    for (int y = -reach; y <= reach; y++) {
        for (int x = -reach; x <= reach; x++) {
            vec2 off = vec2(float(x), float(y)) * texel;
            // textureLod (variante Level) en vez de texture(): el early-return de arriba
            // crea control flow no uniforme y texture() usaria derivados implicitos (UB).
            // Bevy hace lo mismo con textureSampleCompareLevel. El shadow map no tiene mips.
            sum += textureLod(shadow_map, vec3(uv + off, depth_ref), 0.0);
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
    // Atlas 16×16 tiles de 64 px en una capa 1024². `v_tex.y` es el índice
    // local (0..255); inset de 0.5 px para no sangrar al tile vecino.
    // uv.x >= 1.5: interior de cueva (mesher suma 2.0). La boca y el túnel
    // visto desde fuera se quedan en [0, 1] y usan el splat de superficie.
    bool cave_rock = v_uv.x >= 1.5;
    vec2 tile_uv = cave_rock ? vec2(v_uv.x - 2.0, v_uv.y) : v_uv;
    tile_uv = clamp(tile_uv, 0.0, 1.0);
    float tiles = 16.0;
    float tile = floor(v_tex.y + 0.5);
    float tx = mod(tile, tiles);
    float tz = floor(tile / tiles);
    vec2 splat_uv = (vec2(tx, tz) + (tile_uv * 63.0 + 0.5) / 64.0) / tiles;
    float splat_layer = max(v_tex.x, 0.0);
    vec4 w = textureLod(albedo_tex, vec3(splat_uv, splat_layer), 0.0);
    // El uniforme manda. `v_tex.w` se queda en 0 cuando el IOR del terreno pisa
    // la capa, y base 0 es el checker del editor: solo Grass (base+0) se ve a cuadros.
    float base = floor(u.depth_plane.w + 0.5);
    if (base < 0.5) {
        base = floor(v_tex.w + 0.5);
    }
    // Capa 0 = checker del editor. Si el uniforme o el IOR se quedan en 0,
    // Grass no puede usar esa capa: se salta a 1 (aislamiento verde).
    if (base < 0.5) {
        base = 1.0;
    }
    vec2 tuv = v_world.xz * TERRAIN_TEXTURE_SCALE;
    vec3 grass = terrain_layer(tuv, base + 0.0, vec3(0.31, 0.52, 0.24));
    vec3 dirt = terrain_layer(tuv, base + 1.0, vec3(0.47, 0.33, 0.18));
    vec3 clay = terrain_layer(tuv, base + 2.0, vec3(0.62, 0.40, 0.26));
    vec3 sand = terrain_layer(tuv, base + 3.0, vec3(0.77, 0.68, 0.45));
    vec3 stone = terrain_layer(tuv, base + 4.0, vec3(0.48, 0.46, 0.44));
    vec3 snow = terrain_layer(tuv, base + 5.0, vec3(0.90, 0.92, 0.94));
    vec3 mud = terrain_layer(tuv, base + 6.0, vec3(0.10, 0.08, 0.06));
    if (cave_rock) {
        return stone;
    }

    float slope = clamp(1.0 - n.y, 0.0, 1.0);
    float dirt_k = smoothstep(0.12, 0.38, slope);
    vec2 clay_p = v_world.xz * 0.04;
    float clay_n = value_noise(clay_p) * 0.65 + value_noise(clay_p * 2.7 + 17.0) * 0.35;
    float clay_k = (1.0 - dirt_k) * smoothstep(0.60, 0.78, clay_n);
    vec3 ground = mix(mix(grass, clay, clay_k), dirt, dirt_k);
    float covered = w.r + w.g + w.b + w.a;
    float mud_w = max(1.0 - covered, 0.0);
    float sum = max(covered + mud_w, 0.001);
    vec3 rgb = (ground * w.r + stone * w.g + sand * w.b + snow * w.a + mud * mud_w) / sum;
    return rgb;
}

void main() {
    // Primera persona: recorta la geometria dentro del radio alrededor del ojo.
    // u.cam_pos.w = 0 fuera de play, asi que el recorte solo actua en FP.
    // Oculta el interior del rostro del Player sin depender de su malla.
    if (u.cam_pos.w > 0.0 && distance(v_world, u.cam_pos.xyz) < u.cam_pos.w) {
        discard;
    }
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

    // Proyección del medio sobre esta superficie: muestreamos la transmitancia en el punto
    // del mundo. T baja donde el humo/niebla intercepta la luz solar antes de llegar al suelo.
    if (u.light_color.w > 0.5 && vol.params.w > 0.0) {
        vec3 vu = (v_world - vol.center.xyz) / (2.0 * vol.half_extent.xyz) + 0.5;
        if (all(greaterThanEqual(vu, vec3(0.0))) && all(lessThanEqual(vu, vec3(1.0)))) {
            shadow_factor *= mix(1.0, clamp(texture(volume_light, vu).r, 0.0, 1.0), vol.params.w);
        }
    }

    vec3 lit = albedo * u.light_color.rgb
        * (ambient + (1.0 - ambient) * ndotl * shadow_factor)
        * u.light_params.x;
    vec3 v = normalize(u.cam_pos.xyz - v_world);
    float spec = pow(max(dot(n, normalize(l + v)), 0.0), mix(8.0, 64.0, 1.0 - roughness));
    if (u.depth_plane.z < 0.5) {
        lit += u.light_color.rgb * spec * (1.0 - roughness) * shadow_factor * 0.35;
    }
    // IBL especular de sondas: no es una luz. `v_tex.z` es la ranura (-1 = fuera
    // del volumen → no se aplica). El peso va en `jitter.z`.
    // Con SSR/RT (`depth_plane.z`) el composite ya pinta la banda reflectiva
    // (roughness ≤ 0.35). Superponer el cubemap duplicaba el reflejo y, por la
    // convención Y del cubemap, salía de cabeza.
    if (!terrain && u.jitter.z > 0.0) {
        float probe_index = v_tex.z;
        bool ssr_owns_spec = u.depth_plane.z >= 0.5 && roughness <= 0.35;
        if (probe_index >= 0.0 && !ssr_owns_spec) {
            vec3 probe_dir = normalize(mix(n, reflect(-v, n), clamp(1.0 - roughness, 0.0, 1.0)));
            vec3 env = texture(probe_cube, vec4(probe_dir, probe_index)).rgb;
            float ndotv = max(dot(n, v), 0.0);
            float f0 = mix(0.04, 1.0, metallic);
            float fresnel = mix(f0, 1.0, pow(1.0 - ndotv, 5.0));
            float gloss = clamp(1.0 - roughness, 0.0, 1.0);
            vec3 tint = mix(vec3(1.0), albedo, metallic);
            lit += env * tint * fresnel * gloss * u.jitter.z;
        }
    }
    float flag = v_flag.x;
    float rim = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    if (flag > 7.5) {
        // Aura de interacción: color libre elegido en el tooltip.
        vec3 aura = u.aura_color.rgb;
        lit = mix(lit, aura, 0.38);
        lit += aura * rim;
    } else if (flag > 1.5) {
        // flag 2.0 = azul del hover del editor.
        vec3 aura = vec3(0.12, 0.60, 0.88);
        lit = mix(lit, aura, 0.38);
        lit += aura * rim;
    } else if (flag > 0.5) {
        lit = mix(lit, vec3(1.0, 0.75, 0.10), 0.38);
        lit += vec3(1.0, 0.80, 0.15) * rim;
    }
    float alpha = albedo_s.a * v_flag.y;
    float fresnel = pow(1.0 - max(dot(n, v), 0.0), 5.0);
    if (alpha < 0.02) {
        discard;
    }
    if (pc.pass_kind < 0.5 && alpha < 0.98) {
        discard;
    }
    if (pc.pass_kind >= 0.5 && alpha >= 0.98) {
        discard;
    }
    // Fresnel en el pase transparente: el centro se ve a través y los bordes se vuelven
    // opacos, que es lo que da el aspecto de cristal. Va tras el discard para no alterar
    // la selección de pase (opaco vs. blend).
    if (pc.pass_kind >= 0.5) {
        alpha = mix(alpha, min(1.0, alpha + 0.80), fresnel);
    }
    out_color = vec4(lit, alpha);
    out_normal_rough = vec4(n * 0.5 + 0.5, roughness);
    // w = flag de material reflectivo: SurfacePbr con roughness baja. `v_flag.w` es la
    // roughness (>= 0) o -1 si no hay PBR. El suelo tiene SurfacePbr de 0.50 y NO debe
    // reflejar; las esferas de material (chrome/acero/plástico/vidrio/agua) van de 0.0 a 0.2.
    float reflective = (v_flag.w >= 0.0 && v_flag.w <= 0.35 && !terrain) ? 1.0 : 0.0;
    // En superficies transparentes reflectivas, albedo.a codifica el Fresnel en 0.55..1
    // para que composite module la fuerza del reflejo. El gate >= 0.5 de SSR/RT sigue pasando.
    if (reflective > 0.5 && alpha < 0.98) {
        reflective = mix(0.55, 1.0, fresnel);
    }
    out_albedo = vec4(albedo, reflective);
    out_world = vec4(v_world, metallic);
    vec2 curr = v_curr.xy / v_curr.w;
    vec2 prev = v_prev.xy / v_prev.w;
    out_velocity = curr - prev;
}
