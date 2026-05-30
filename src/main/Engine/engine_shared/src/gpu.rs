//! Política GPU del motor: **Vulkan** exclusivo (2D y 3D; Windows y Linux).
//!
//! Sin OpenGL, sin `Backends::all()` ni otros backends wgpu.

use std::fmt;
use std::sync::Arc;

use winit::window::Window;

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
    pub fn wgpu_backends(self) -> wgpu::Backends {
        let _ = self;
        wgpu::Backends::VULKAN
    }

    pub fn label(self) -> &'static str {
        let _ = self;
        "Vulkan"
    }

    fn expected_wgpu_backend(self) -> wgpu::Backend {
        let _ = self;
        wgpu::Backend::Vulkan
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

fn user_facing_message(backend: EngineGpuBackend, detail: &str) -> String {
    let _ = backend;
    let hints = "\
         • Instala o actualiza los controladores de tu tarjeta gráfica (NVIDIA / AMD / Intel).\n\
         • En WSL2: usa WSLg (Windows 11), drivers GPU en Windows y en la distro \
         `sudo apt install mesa-vulkan-drivers` (o drivers NVIDIA para WSL).\n\
         • Comprueba Vulkan en terminal: `vulkaninfo` o `vkcube`.\n\
         • Reinicia el editor después de instalar drivers.";

    format!(
        "No se pudo iniciar el motor gráfico con {}.\n\n\
         {detail}\n\n\
         Sugerencias:\n{hints}",
        backend.label()
    )
}

/// Crea instancia wgpu, surface y adapter para el perfil indicado.
pub async fn init_gpu(
    window: Arc<Window>,
    profile: EngineGpuProfile,
) -> Result<(wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter), GpuInitError> {
    let backend = resolve_backend(profile);
    let backends = backend.wgpu_backends();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });

    let surface = instance.create_surface(window).map_err(|e| GpuInitError {
        message: user_facing_message(
            backend,
            &format!("No se pudo crear la superficie de presentación: {e}"),
        ),
    })?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .ok_or_else(|| GpuInitError {
            message: user_facing_message(
                backend,
                "No se encontró ningún adaptador gráfico compatible con la superficie.",
            ),
        })?;

    let info = adapter.get_info();
    if info.backend != backend.expected_wgpu_backend() {
        return Err(GpuInitError {
            message: user_facing_message(
                backend,
                &format!(
                    "El adaptador devolvió {:?} en lugar de {}.",
                    info.backend,
                    backend.label()
                ),
            ),
        });
    }

    log::info!("Motor GPU: {} — {}", backend.label(), info.name);
    Ok((instance, surface, adapter))
}

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
