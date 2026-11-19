#version 460
#extension GL_EXT_ray_tracing : require

layout(location = 0) rayPayloadInEXT vec3 payload;

void main() {
    // Los raygen dejan el payload con el color del cielo antes de trazar; no lo sobrescribimos.
    // Así el degradado de la esfera se conserva tanto en el pase primario como en reflejos.
}
