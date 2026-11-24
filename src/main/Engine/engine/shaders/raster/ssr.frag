#version 450

// SSR tradicional: reconstruye la superficie desde depth + inv_view_proj, refleja el vector de
// vista y marcha el rayo en unidades de mundo. La roughness solo limita soporte/confianza; nunca
// acorta ni desvía la dirección del rayo.
const int MAX_MARCH_STEPS = 128;
const int MAX_BINARY_STEPS = 8;

layout(location = 0) in vec2 v_uv;
layout(set = 0, binding = 0) uniform sampler2D depth_tex;
layout(set = 0, binding = 1) uniform sampler2D normal_tex;
layout(set = 0, binding = 2) uniform sampler2D color_tex;
// albedo.a = flag de material reflectivo preparado por forward.frag (solo gate, no opacidad).
layout(set = 0, binding = 4) uniform sampler2D albedo_tex;
// Prefijo byte a byte de SceneUniforms: view_proj@0, inv_view_proj@192, cam_pos@256.
layout(set = 0, binding = 3) uniform SceneCamera {
    mat4 view_proj;
    mat4 view_proj_stable;
    mat4 prev_view_proj;
    mat4 inv_view_proj;
    vec4 cam_pos;
} camera;

layout(location = 0) out vec4 out_color;

// 64 bytes, espejo de SsrPush en present.rs:
// a = (march steps, binary steps, max distance [m], min thickness [m])
// b = (adaptive thickness, max thickness [m], roughness support, edge fade [UV])
// c = (near, far, origin bias [m], scene width)
// d = (scene height, aspect, 0, 0)
layout(push_constant) uniform SsrParams {
    vec4 a;
    vec4 b;
    vec4 c;
    vec4 d;
} pc;

float linear_depth(float depth) {
    float near = max(pc.c.x, 0.0001);
    float far = max(pc.c.y, near + 0.0001);
    return near * far / max(far - depth * (far - near), 0.0001);
}

vec2 project_to_uv(vec3 world, out float view_depth) {
    vec4 clip = camera.view_proj * vec4(world, 1.0);
    view_depth = clip.w;
    if (abs(clip.w) < 0.000001) {
        return vec2(-1.0);
    }
    vec2 uv = clip.xy / clip.w * 0.5 + 0.5;
    // El viewport negativo del fullscreen conserva una única convención: su muestrea arriba-abajo.
    uv.y = 1.0 - uv.y;
    return uv;
}

float max_thickness_at(float ray_distance) {
    // Tolerancia geométrica final. En adaptativo crece desde un mínimo seguro en primer plano
    // hasta el máximo del preset; sin adaptación usa siempre el máximo.
    float min_validation = max(2.0 * max(pc.a.w, 0.0001), 0.05);
    float max_validation = max(pc.b.y, min_validation);
    float grow = pc.b.x > 0.5 ? clamp(ray_distance / max(pc.a.z, 0.0001), 0.0, 1.0) : 1.0;
    return mix(min_validation, max_validation, grow);
}

// Cruce de profundidad monótono para marcha y búsqueda binaria: una vez el rayo pasa detrás de
// la superficie, sigue detrás. El grosor máximo NO participa aquí; evitaría el overshoot. UV
// fuera de pantalla y cielo se tratan como miss; en una silueta ese borde puede limitar la
// precisión de la búsqueda, pero no reintroduce el miss sistemático por grosor.
bool ray_crosses_scene(vec3 origin, vec3 direction, float ray_distance, float min_thickness) {
    vec3 world = origin + direction * ray_distance;
    float ray_view_depth;
    vec2 uv = project_to_uv(world, ray_view_depth);
    if (ray_view_depth <= pc.c.x || any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
        return false;
    }
    float scene_depth = textureLod(depth_tex, uv, 0).r;
    if (scene_depth >= 0.99999) {
        return false;
    }
    float delta = linear_depth(scene_depth) - ray_view_depth;
    return delta <= -min_thickness;
}

void main() {
    vec2 su = vec2(v_uv.x, 1.0 - v_uv.y);
    if (textureLod(albedo_tex, su, 0).a < 0.5) {
        out_color = vec4(0.0);
        return;
    }
    float depth = textureLod(depth_tex, su, 0).r;
    if (depth >= 0.99999) {
        out_color = vec4(0.0);
        return;
    }

    vec4 normal_rough = textureLod(normal_tex, su, 0);
    float roughness = clamp(normal_rough.a, 0.0, 1.0);
    if (roughness > pc.b.z) {
        out_color = vec4(0.0);
        return;
    }
    vec3 normal = normalize(normal_rough.xyz * 2.0 - 1.0);

    // La proyección Vulkan usa depth 0..1. Con el viewport negativo, la fila superior de la imagen
    // corresponde a NDC Y=+1; de ahí el volteo únicamente en Y.
    vec3 ndc = vec3(su.x * 2.0 - 1.0, 1.0 - su.y * 2.0, depth);
    vec4 world_h = camera.inv_view_proj * vec4(ndc, 1.0);
    if (abs(world_h.w) < 0.000001) {
        out_color = vec4(0.0);
        return;
    }
    vec3 world = world_h.xyz / world_h.w;
    vec3 view_dir = normalize(camera.cam_pos.xyz - world);
    if (dot(normal, view_dir) <= 0.0) {
        out_color = vec4(0.0);
        return;
    }

    // reflect() puro: la roughness no modifica esta dirección.
    vec3 ray_dir = normalize(reflect(-view_dir, normal));
    vec3 ray_origin = world + normal * max(pc.c.z, 0.0);
    float min_thickness = max(pc.a.w, 0.0001);
    float max_distance = max(pc.a.z, 0.001);
    int march_steps = clamp(int(pc.a.x), 1, MAX_MARCH_STEPS);
    int binary_steps = clamp(int(pc.a.y), 0, MAX_BINARY_STEPS);
    float step_length = max_distance / float(march_steps);

    // Si el origen ya está detrás de la superficie, lo sería también t=0: descartar evita que la
    // búsqueda converja a un auto-impacto sin información espacial útil.
    if (ray_crosses_scene(ray_origin, ray_dir, 0.0, min_thickness)) {
        out_color = vec4(0.0);
        return;
    }

    float lo = 0.0;
    float hi = 0.0;
    bool hit = false;
    for (int i = 0; i < MAX_MARCH_STEPS; i++) {
        if (i >= march_steps) {
            break;
        }
        float t = float(i + 1) * step_length;
        if (ray_crosses_scene(ray_origin, ray_dir, t, min_thickness)) {
            hi = t;
            hit = true;
            break;
        }
        lo = t;
    }
    if (!hit) {
        out_color = vec4(0.0);
        return;
    }

    // Mismo cruce monótono que la marcha; lo es un miss conocido y hi un hit conocido.
    for (int i = 0; i < MAX_BINARY_STEPS; i++) {
        if (i >= binary_steps) {
            break;
        }
        float mid = 0.5 * (lo + hi);
        if (ray_crosses_scene(ray_origin, ray_dir, mid, min_thickness)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    // Validación geométrica final, separada del cruce monótono. Reconstruye la superficie de la
    // escena en el UV impactado y exige que coincida espacialmente con el punto del rayo refinado.
    vec3 hit_world = ray_origin + ray_dir * hi;
    float hit_view_depth;
    vec2 hit_uv = project_to_uv(hit_world, hit_view_depth);
    if (hit_view_depth <= pc.c.x
        || any(lessThan(hit_uv, vec2(0.0)))
        || any(greaterThan(hit_uv, vec2(1.0)))) {
        out_color = vec4(0.0);
        return;
    }
    float hit_scene_depth = textureLod(depth_tex, hit_uv, 0).r;
    if (hit_scene_depth >= 0.99999) {
        out_color = vec4(0.0);
        return;
    }
    vec3 hit_ndc = vec3(hit_uv.x * 2.0 - 1.0, 1.0 - hit_uv.y * 2.0, hit_scene_depth);
    vec4 scene_world_h = camera.inv_view_proj * vec4(hit_ndc, 1.0);
    if (abs(scene_world_h.w) < 0.000001) {
        out_color = vec4(0.0);
        return;
    }
    vec3 scene_world = scene_world_h.xyz / scene_world_h.w;
    if (length(scene_world - hit_world) > max_thickness_at(hi)) {
        out_color = vec4(0.0);
        return;
    }
    vec2 pixel_delta = (hit_uv - su) * pc.c.zw;
    if (length(pixel_delta) < 0.75) {
        out_color = vec4(0.0);
        return;
    }

    // El color sale directamente de la escena; albedo.a no se usa como opacidad ni como color.
    vec3 scene_color = textureLod(color_tex, hit_uv, 0).rgb;
    vec2 edge = min(hit_uv, vec2(1.0) - hit_uv);
    float edge_fade = smoothstep(0.0, max(pc.b.w, 0.001), min(edge.x, edge.y));
    // Fundido de recorrido: evita que el reflejo aparezca/desaparezca de golpe al alcanzar
    // max_distance o el límite de pasos.
    float distance_fade = 1.0 - smoothstep(max_distance * 0.75, max_distance, hi);
    float confidence = clamp((1.0 - roughness) * edge_fade * distance_fade, 0.0, 1.0);
    out_color = vec4(scene_color, confidence);
}
