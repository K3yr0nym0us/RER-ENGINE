//! Política GPU del motor: **Vulkan** exclusivo (2D y 3D; Windows).
//!
//! El backend real lo crea `rer_engine_core::vulkan` (Ash). Este módulo solo expone la etiqueta
//! común y el error de arranque que comparten los binarios.

use std::fmt;

/// Perfil del binario que solicita GPU (2D vs 3D).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineGpuProfile {
    TwoD,
    ThreeD,
}

/// Backend gráfico activo del motor (exclusivo por sesión).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineGpuBackend {
    Vulkan,
}

impl EngineGpuBackend {
    pub fn label(self) -> &'static str {
        let _ = self;
        "Vulkan"
    }
}

/// Resuelve el backend según perfil de motor (siempre Vulkan; sin variables de entorno).
pub fn resolve_backend(_profile: EngineGpuProfile) -> EngineGpuBackend {
    EngineGpuBackend::Vulkan
}

#[derive(Debug, Clone)]
pub struct GpuInitError {
    pub message: String,
}

impl fmt::Display for GpuInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GpuInitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_d_always_vulkan() {
        assert_eq!(
            resolve_backend(EngineGpuProfile::TwoD),
            EngineGpuBackend::Vulkan
        );
    }

    #[test]
    fn three_d_always_vulkan() {
        assert_eq!(
            resolve_backend(EngineGpuProfile::ThreeD),
            EngineGpuBackend::Vulkan
        );
    }
}
