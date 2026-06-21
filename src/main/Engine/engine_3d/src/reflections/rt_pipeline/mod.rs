//! Migración a pipeline RT completo (`@ray_generation` / closest-hit / miss).
//!
//! Bloqueado hasta wgpu PR #9450 estable. Ver [`super::rt_extensions::rt_generation_pipeline_enabled`].
//!
//! Referencia: [jdonald/rust-raytracing](https://github.com/jdonald/rust-raytracing)

#![allow(dead_code)]

pub const MIGRATION_NOTE: &str = super::rt_extensions::RT_PIPELINE_MIGRATION_NOTE;

/// Shader stub (no compilado en pipeline de producción).
pub const RAYGEN_WGSL: &str = include_str!("raygen.wgsl");

/// Pasos de migración cuando wgpu exponga `VK_KHR_ray_tracing_pipeline` vía wgpu.
pub const MIGRATION_CHECKLIST: &[&str] = &[
    "Crear RayTracingPipelineLayout (TLAS + G-buffer + probes + shadow)",
    "Compilar raygen.wgsl, closesthit.wgsl, miss.wgsl",
    "Dispatch traceRays en lugar de compute sparse",
    "Mantener rt_bvh.wgsl como fallback sin ray query",
    "Activar rt_generation_pipeline_enabled() cuando wgpu_rt_pipeline_ready()",
];

/// Path de producción estable hasta migración a `@ray_generation`.
pub const PRODUCTION_PATH: &str = "compute_ray_query";

/// Criterio para activar pipeline clásico: upstream wgpu + flag explícito en rt_extensions.
pub fn production_uses_compute_ray_query() -> bool {
    !wgpu_rt_pipeline_ready()
}

/// Indica si el pipeline clásico puede activarse (upstream wgpu).
pub fn wgpu_rt_pipeline_ready() -> bool {
    super::rt_extensions::rt_generation_pipeline_enabled()
}
