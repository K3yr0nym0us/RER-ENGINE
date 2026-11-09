#version 460
#extension GL_EXT_ray_tracing : require
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_scalar_block_layout : require
#extension GL_EXT_shader_explicit_arithmetic_types_int64 : require

layout(location = 0) rayPayloadInEXT vec3 payload;
hitAttributeEXT vec2 attribs;

layout(set = 0, binding = 0) uniform Camera {
    mat4 inv_view_proj;
    vec4 cam_pos;
    vec4 resolution;
    vec4 light_dir;
} cam;

struct Material {
    vec4 albedo;
    vec4 pbr;
    vec4 probe;
};

layout(set = 0, binding = 3, scalar) readonly buffer Materials {
    Material mats[];
};

struct GeoRef {
    uint64_t vertex;
    uint64_t index;
    uint vertex_stride;
    uint index_stride;
};

layout(set = 0, binding = 6, scalar) readonly buffer Geos {
    GeoRef geos[];
};

layout(buffer_reference, scalar) readonly buffer Positions {
    float v[];
};
layout(buffer_reference, scalar) readonly buffer Indices {
    uint i[];
};

vec3 load_pos(Positions positions, uint index, uint stride) {
    uint base = index * (stride / 4u);
    return vec3(positions.v[base], positions.v[base + 1u], positions.v[base + 2u]);
}

void main() {
    uint slot = gl_InstanceCustomIndexEXT;
    GeoRef geo = geos[slot];
    Indices indices = Indices(geo.index);
    Positions positions = Positions(geo.vertex);
    uint prim = gl_PrimitiveID * 3u;
    vec3 v0 = load_pos(positions, indices.i[prim], geo.vertex_stride);
    vec3 v1 = load_pos(positions, indices.i[prim + 1u], geo.vertex_stride);
    vec3 v2 = load_pos(positions, indices.i[prim + 2u], geo.vertex_stride);
    vec3 n = normalize(cross(v1 - v0, v2 - v0));
    if (dot(n, gl_WorldRayDirectionEXT) > 0.0) {
        n = -n;
    }
    vec3 albedo = mats[slot].albedo.rgb;
    float ndotl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    float ambient = clamp(cam.light_dir.w, 0.0, 1.0);
    payload = albedo * (ambient + (1.0 - ambient) * ndotl);
}
