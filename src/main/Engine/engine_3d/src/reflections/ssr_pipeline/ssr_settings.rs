//! Parámetros SSR alineados con Bevy `ssr_settings` (`bevy_pbr/src/ssr`).

use crate::config_3d::reflection_graphics::ReflectionTier;

/// Reservado (Bevy reparte pasos por longitud del rayo en píxeles).
pub fn ssr_coarse_resolution(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::Off => 0.0,
        ReflectionTier::Low => 0.40,
        ReflectionTier::Medium => 0.55,
        ReflectionTier::High => 0.75,
        ReflectionTier::Ultra => 1.0,
    }
}

/// Bevy `ssr_settings.thickness` (`depth_thickness_linear_z`; default 0.25).
pub fn ssr_thickness_m(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::Off => 0.25,
        ReflectionTier::Low => 0.25,
        ReflectionTier::Medium => 0.25,
        ReflectionTier::High => 0.30,
        ReflectionTier::Ultra => 0.30,
    }
}

/// Bevy `bisection_steps`.
pub fn ssr_binary_steps(tier: ReflectionTier) -> u32 {
    match tier {
        ReflectionTier::Off => 0,
        ReflectionTier::Low => 3,
        ReflectionTier::Medium => 5,
        ReflectionTier::High => 5,
        ReflectionTier::Ultra => 7,
    }
}

/// Bevy `linear_steps` (tope; el shader acota por longitud del rayo en píxeles).
pub fn ssr_coarse_max_iters(tier: ReflectionTier) -> u32 {
    match tier {
        ReflectionTier::Off => 0,
        ReflectionTier::Low => 32,
        ReflectionTier::Medium => 64,
        ReflectionTier::High => 128,
        ReflectionTier::Ultra => 256,
    }
}
