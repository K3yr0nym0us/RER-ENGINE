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

/// Lettier `thickness`: tolerancia del test de profundidad (metros).
/// Bevy usa 0.25 en espacio lineal-z; ~0.25 m es un buen punto de partida en metros.
pub fn ssr_thickness_m(tier: ReflectionTier) -> f32 {
    match tier {
        ReflectionTier::Off => 0.25,
        ReflectionTier::Low => 0.25,
        ReflectionTier::Medium => 0.30,
        ReflectionTier::High => 0.35,
        ReflectionTier::Ultra => 0.40,
    }
}

/// Lettier `steps` / Bevy `bisection_steps`: refinamiento binario tras el coarse pass.
pub fn ssr_binary_steps(tier: ReflectionTier) -> u32 {
    match tier {
        ReflectionTier::Off => 0,
        ReflectionTier::Low => 3,
        ReflectionTier::Medium => 5,
        ReflectionTier::High => 5,
        ReflectionTier::Ultra => 7,
    }
}

/// Tope de iteraciones del coarse pass. Con `stepVec = dp / coarse_iters` cada paso
/// recorre una fracción del rayo completo (estilo Bevy `linear_steps`).
pub fn ssr_coarse_max_iters(tier: ReflectionTier) -> u32 {
    match tier {
        ReflectionTier::Off => 0,
        ReflectionTier::Low => 32,
        ReflectionTier::Medium => 64,
        ReflectionTier::High => 128,
        ReflectionTier::Ultra => 256,
    }
}
