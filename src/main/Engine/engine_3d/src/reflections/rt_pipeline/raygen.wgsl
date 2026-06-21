// Stub: migración a pipeline RT clásico (@ray_generation) tras wgpu PR #9450.
// Referencia: https://github.com/jdonald/rust-raytracing (raygen.rgen + closesthit.rchit + miss.rmiss)
//
// Cuando rt_extensions::rt_generation_pipeline_enabled() sea true:
//   1. traceRay contra TLAS desde raygen
//   2. closest-hit: material por instancia + refl_resolve_hit_radiance
//   3. miss: probe / fake env
//
// Hoy el path activo es compute inline en rt_ray_query.wgsl (wgpu_ray_query).

enable wgpu_ray_query;

// @ray_generation — placeholder (no enlazado mientras el gate esté desactivado)
