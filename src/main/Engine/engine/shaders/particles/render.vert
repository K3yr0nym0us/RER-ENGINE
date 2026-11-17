#version 460
// Billboard instanciado orientado a cámara o a velocidad. Cada instancia = 1 partícula;
// la geometría son 6 vértices de quad (TRIANGLE_LIST, sin index buffer) y `gl_InstanceIndex`
// indexa el storage buffer de partículas (binding 1/2 según `read_is_a`).
//
// Sin depth write: el pase corre con la depth del forward en READ_ONLY (soft particles
// F3 la muestreará aquí sin tocarla).

layout(location = 0) out vec4 v_color;
layout(location = 1) out vec2 v_uv;
layout(location = 2) out vec3 v_spin_size; // xy = ángulo/tamaño, z = rot_seed.z (estado)

struct Particle {
    vec4 pos_life;
    vec4 vel_age;
    vec4 rot_seed;
    vec4 color;
};

// Debe coincidir con `EmitterParams` de params.rs (160 bytes).
layout(set = 0, binding = 0) uniform Emitter {
    vec4 origin_radius;
    vec4 velocity_spread;
    vec4 size_life;
    vec4 color_start;
    vec4 color_end;
    vec4 phys;
    vec4 env;
    uvec4 misc;      // z = modo billboard: 0 cámara, 1 velocidad, 2 stretched
    vec4 volume;
    vec4 collide;    // z = size_end, w = stretch
} emitter;

layout(set = 0, binding = 1) readonly buffer PartA { Particle part_a[]; };
layout(set = 0, binding = 2) readonly buffer PartB { Particle part_b[]; };

layout(push_constant) uniform Push {
    mat4 view_proj;
    vec4 cam_right;  // xyz derecha de cámara (mundo)
    vec4 cam_up;     // xyz arriba de cámara (mundo)
    uvec4 misc;      // x = read_is_a, y = mezcla, z = soft, w reservado
    vec4 depth_params; // x near, y far, z suavidad (m), w reservado
} pc;

const vec2 CORNERS[6] = vec2[6](
    vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
    vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)
);

void main() {
    uint index = uint(gl_InstanceIndex);
    Particle p = pc.misc.x == 1u ? part_a[index] : part_b[index];

    float age = p.vel_age.w;
    float life = max(p.pos_life.w, 1e-3);
    float k = clamp(age / life, 0.0, 1.0);

    float size = p.rot_seed.y * mix(1.0, max(emitter.collide.z, 0.0), k);
    if (emitter.misc.x == 0u) {
        // Fuego: pico al nacer y luego el size_end encoge la llama.
        size *= mix(0.42, 1.0, smoothstep(0.0, 0.16, k));
    }
    if (emitter.misc.x == 5u && p.rot_seed.z >= 1.0) {
        // Grano en el suelo: el tamaño queda fijo (no encoge con size_end).
        size = p.rot_seed.y;
    }

    vec3 axis_r = pc.cam_right.xyz;
    vec3 axis_u = pc.cam_up.xyz;
    float stretch = 1.0;
    float width = 1.0;
    uint mode = emitter.misc.z;
    if (p.rot_seed.z >= 3.0) {
        // Agua tras el impacto: billboard a cámara (deja de ser una línea stretched).
        mode = 0u;
    }
    if (mode == 1u || mode == 2u) {
        vec3 vel = p.vel_age.xyz;
        float speed = length(vel);
        if (speed > 1e-3) {
            vec3 fwd = normalize(cross(pc.cam_up.xyz, pc.cam_right.xyz));
            axis_u = vel / speed;
            axis_r = cross(axis_u, fwd);
            if (length(axis_r) < 1e-4) {
                axis_r = pc.cam_right.xyz;
            } else {
                axis_r = normalize(axis_r);
            }
            if (mode == 2u) {
                float s = max(emitter.collide.w, 0.12);
                stretch = 1.0 + clamp(speed * s, 0.0, 6.5);
                width = 0.22;
            } else {
                stretch = 1.0 + clamp(speed * 0.09, 0.0, 2.5);
            }
        }
    }

    vec2 c = CORNERS[gl_VertexIndex];
    vec2 local = c;
    if (mode == 0u) {
        float angle = p.rot_seed.x;
        float ca = cos(angle);
        float sa = sin(angle);
        local = vec2(c.x * ca - c.y * sa, c.x * sa + c.y * ca);
    }

    vec3 world = p.pos_life.xyz
        + axis_r * (local.x * size * width)
        + axis_u * (local.y * size * stretch);

    v_uv = c;
    v_spin_size = p.rot_seed.xyz;
    float fade = 1.0;
    // Humo (mat 1): el alfa lo decide color_over_life (pow 2.5 + fade-in).
    if (emitter.misc.x != 1u && !(emitter.misc.x == 5u && p.rot_seed.z >= 1.0)) {
        fade = 1.0 - k * k;
    }
    if (emitter.misc.x == 4u && p.rot_seed.z >= 2.0 && p.rot_seed.z < 3.0) {
        fade *= (1.0 - k) * (1.0 - k);
    }
    v_color = vec4(p.color.rgb, p.color.a * fade);
    gl_Position = pc.view_proj * vec4(world, 1.0);
}
