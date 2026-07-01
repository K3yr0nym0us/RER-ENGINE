//! Parámetros SSR por tier de reflejos.

use crate::config_3d::reflection_graphics::ReflectionTier;

/// Fracción de píxeles del rayo en pantalla; alineado con `ReflectionTier::reflection_screen_fraction`.
pub fn ssr_coarse_resolution(tier: ReflectionTier) -> f32 {
    tier.reflection_screen_fraction()
}

/// Tolerancia del test de profundidad en marcha SSR (metros).
pub fn ssr_thickness_m(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::Off => 0.25,
        ReflectionTier::Low => 0.25,
        ReflectionTier::Medium => 0.25,
        ReflectionTier::High => 0.30,
        ReflectionTier::Ultra => 0.30,
    }
}

/// Iteraciones del refinamiento binario SSR.
pub fn ssr_binary_steps(tier: ReflectionTier) -> u32 {
    match tier {
        ReflectionTier::Off => 0,
        ReflectionTier::Low => 3,
        ReflectionTier::Medium => 5,
        ReflectionTier::High => 5,
        ReflectionTier::Ultra => 7,
    }
}
