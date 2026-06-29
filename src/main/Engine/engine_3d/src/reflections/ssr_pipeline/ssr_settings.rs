//! Parámetros de configuración SSR (Lettier).
//! Extraídos de `reflections::settings::ReflectionSettings` para que el
//! pipeline SSR sea autocontenido.

use crate::config_3d::reflection_graphics::ReflectionTier;

/// Lettier `resolution`: fracción de píxeles del rayo en pantalla (0–1).
pub fn ssr_coarse_resolution(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::Off => 0.0,
        ReflectionTier::Low => 0.40,
        ReflectionTier::Medium => 0.55,
        ReflectionTier::High => 0.75,
        ReflectionTier::Ultra => 1.0,
    }
}

/// Lettier `thickness`: tolerancia del test de profundidad (metros). Guía usa 0.5.
pub fn ssr_thickness_m(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::Off => 0.5,
        ReflectionTier::Low => 0.45,
        ReflectionTier::Medium => 0.40,
        ReflectionTier::High => 0.35,
        ReflectionTier::Ultra => 0.65,
    }
}

/// Lettier `steps`: iteraciones del paso de refinamiento binario.
pub fn ssr_binary_steps(tier: ReflectionTier) -> u32 {
    match tier {
        ReflectionTier::Off => 0,
        ReflectionTier::Low => 1,
        ReflectionTier::Medium => 3,
        ReflectionTier::High => 5,
        ReflectionTier::Ultra => 7,
    }
}

/// Tope de iteraciones del coarse pass (Lettier `int(delta)` acotado por tier).
pub fn ssr_coarse_max_iters(tier: ReflectionTier) -> u32 {
    match tier {
        ReflectionTier::Off => 0,
        ReflectionTier::Low => 12,
        ReflectionTier::Medium => 96,
        ReflectionTier::High => 256,
        ReflectionTier::Ultra => 512,
    }
}
