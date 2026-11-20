#version 460
#extension GL_EXT_ray_tracing : require

// Un único incoming payload, compartido con los raygen.
struct RayPayload {
    vec4 color;
    vec4 hit;
};
layout(location = 0) rayPayloadInEXT RayPayload payload;

void main() {
    // Los raygen dejan el color del cielo en payload.color; no lo sobrescribimos.
    // distancia 0 = miss, para que los rebotes sepan que no hay más geometría.
    payload.hit = vec4(0.0);
}
