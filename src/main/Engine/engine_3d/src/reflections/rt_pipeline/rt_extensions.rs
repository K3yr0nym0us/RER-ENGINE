//! Extensiones post-MVP RT v2 (libro 2/3, pipelines completos, skinned dinámico).

#![allow(dead_code)]

use crate::config_3d::reflection_graphics::{ReflectionSettings, ReflectionTier};

/// Segundo rebote especular — `max_bounces >= 2` (tier Ultra).
pub fn rt_second_bounce_enabled(settings: &ReflectionSettings) -> bool {
    settings.max_bounces() >= 2
}

/// BLAS/TLAS hardware para mallas skinned (refit por frame).
pub fn skinned_tlas_supported() -> bool {
    true
}

/// Migración futura de ray query inline a `@ray_generation` / `traceRay`.
pub const RT_PIPELINE_V2_REFERENCE: &str = "https://github.com/gfx-rs/wgpu/pull/9450";

/// Gate de producto: pipeline RT completo con `@ray_generation` (Fase E).
/// Permanece desactivado hasta wgpu PR #9450 estable; ver `rt_pipeline/`.
pub fn rt_generation_pipeline_enabled() -> bool {
    false
}

/// Semilla blue-noise determinista por píxel+frame para fuzzy RTIOW convergente vía temporal.
pub fn blue_noise_seed(frame_index: u32, uv: [f32; 2]) -> [f32; 2] {
    let f = frame_index as f32 * 0.618_034;
    [uv[0] + f * 0.173, uv[1] + f * 0.271]
}

/// WGSL equivalente (documentado en reflection_math.wgsl).
pub const BLUE_NOISE_WGSL: &str = r"
fn refl_blue_noise_seed(frame_index : u32, uv : vec2<f32>) -> vec2<f32> {
    let f = f32(frame_index) * 0.6180339887;
    return vec2<f32>(uv.x + f * 0.173, uv.y + f * 0.271);
}
";

/// Dieléctricos (refracción RTIOW) en SSR/RT — tier Ultra.
pub fn dielectric_rt_enabled(settings: &ReflectionSettings) -> bool {
    settings.tier == ReflectionTier::Ultra
}

/// Indirecto difuso acotado (SSIL) — tier Ultra.
pub fn rt_diffuse_gi_enabled(settings: &ReflectionSettings) -> bool {
    settings.tier == ReflectionTier::Ultra
}

/// Sombras RT — tier High y Ultra.
pub fn rt_shadows_enabled(settings: &ReflectionSettings) -> bool {
    settings.rt_shadow_rays()
}

/// Suelo RT single-sided (evita hits duplicados en rayos rasos).
pub fn rt_ground_single_sided_enabled() -> bool {
    true
}

/// Cuando `rt_generation_pipeline_enabled()` sea true, usar módulo shader dedicado.
pub const RT_PIPELINE_MIGRATION_NOTE: &str =
    "reflections/rt_pipeline/ — migrar desde rt_ray_query.wgsl tras wgpu PR #9450";

/// Cuando `dielectric_rt_enabled()` sea true, extender reflection_math con refract RTIOW.
pub const DIELECTRIC_MIGRATION_NOTE: &str =
    "reflection_math.wgsl: refl_refract_dir + máscara dieléctrica en SSR/RT";

/// Cuando `rt_ground_single_sided_enabled()` sea true, usar mesh RT de una cara o CULL_BACK_FACING.
pub const GROUND_RT_MIGRATION_NOTE: &str =
    "mesh_3d::create_ground_plane RT variant o RayDesc cull en rt_bvh/rt_ray_query";
