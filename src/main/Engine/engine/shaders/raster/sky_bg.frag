#version 450

// Cielo procedural a pantalla completa cuando RT/BVH están apagados.
//
// Mismo `sky_color` y mismo empaquetado de `Camera` que `primary.rgen` / `trace.comp`,
// para que SSR/MSAA/copy_color vean el mismo `targets.background`. El triángulo
// `fullscreen.vert` cubre el extent SSAA; no se suben nodos ni triángulos del BVH.

layout(location = 0) in vec2 v_uv;

layout(set = 0, binding = 0) uniform Camera {
    mat4 inv_view_proj;
    vec4 cam_pos;
    vec4 resolution;
    vec4 light_dir;
    vec4 sky_center_radius; // xyz = centro de la esfera, w = radio
    vec4 sky_bottom;        // rgb = azul muy oscuro, casi negro (cara inferior)
    vec4 sky_top;           // rgb = azul (cara superior), w = punto de azul pleno
    vec4 sky_sun;           // xyz = dirección al sol, w = intensidad del brillo
    vec4 sky_sun_color;     // rgb = azul claro del brillo
    vec4 rt_params;         // parámetros del tier RTX (no usados por el fondo)
} cam;

layout(location = 0) out vec4 out_color;

vec3 sky_color(vec3 origin, vec3 dir) {
    float radius = max(cam.sky_center_radius.w, 0.001);
    vec3 oc = origin - cam.sky_center_radius.xyz;
    float b = dot(oc, dir);
    float c = dot(oc, oc) - radius * radius;
    float h = b * b - c;
    float t = h > 0.0 ? -b + sqrt(h) : -b;
    vec3 p = origin + dir * max(t, 0.0);
    float f = clamp((p.y - cam.sky_center_radius.y) / radius * 0.5 + 0.5, 0.0, 1.0);
    float k = clamp(f / max(cam.sky_top.w, 0.001), 0.0, 1.0);
    k = k * k * (3.0 - 2.0 * k);
    vec3 base = mix(cam.sky_bottom.rgb, cam.sky_top.rgb, k);
    base *= mix(0.12, 1.0, clamp(cam.sky_sun_color.w, 0.0, 1.0));
    float glow = 0.0;
    vec3 to_sun = cam.sky_sun.xyz - origin;
    float sun_len = length(to_sun);
    if (sun_len > 0.001 && cam.sky_sun.w > 0.0) {
        float alignment = max(dot(dir, to_sun / sun_len), 0.0);
        glow = pow(alignment, 6.0) * cam.sky_sun.w;
    }
    return mix(base, cam.sky_sun_color.rgb, clamp(glow, 0.0, 1.0));
}

void main() {
    // `v_uv` viene del triángulo full-screen con viewport de altura negativa:
    // Y=0 es el borde inferior. Invertir Y iguala el UV de `primary.rgen`
    // (`LaunchID.y = 0` arriba).
    vec2 uv = vec2(v_uv.x, 1.0 - v_uv.y);
    vec2 ndc = vec2(uv.x * 2.0 - 1.0, (1.0 - uv.y) * 2.0 - 1.0);
    vec4 target = cam.inv_view_proj * vec4(ndc, 1.0, 1.0);
    vec3 dir = normalize(target.xyz / target.w - cam.cam_pos.xyz);
    out_color = vec4(sky_color(cam.cam_pos.xyz, dir), 1.0);
}
