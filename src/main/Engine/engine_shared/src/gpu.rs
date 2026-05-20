//! Política GPU del motor: un solo backend por proceso.
//!
//! - Motor 2D: siempre Vulkan.
//! - Motor 3D en Linux: Vulkan.
//! - Motor 3D en Windows: DirectX 12 (fijo; no usa variables de entorno).
//! - Sin fallback entre Vulkan y DX12 ni OpenGL.

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
    #[cfg(target_os = "windows")]
    Dx12,
}

impl EngineGpuBackend {
    pub fn wgpu_backends(self) -> wgpu::Backends {
        match self {
            Self::Vulkan => wgpu::Backends::VULKAN,
            #[cfg(target_os = "windows")]
            Self::Dx12 => wgpu::Backends::DX12,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vulkan => "Vulkan",
            #[cfg(target_os = "windows")]
            Self::Dx12 => "DirectX 12",
        }
    }

    fn expected_wgpu_backend(self) -> wgpu::Backend {
        match self {
            Self::Vulkan => wgpu::Backend::Vulkan,
            #[cfg(target_os = "windows")]
            Self::Dx12 => wgpu::Backend::Dx12,
        }
    }
}

/// Resuelve el backend según perfil de motor (sin variables de entorno).
pub fn resolve_backend(profile: EngineGpuProfile) -> EngineGpuBackend {
    match profile {
        EngineGpuProfile::TwoD => EngineGpuBackend::Vulkan,
        EngineGpuProfile::ThreeD => {
            #[cfg(target_os = "windows")]
            {
                EngineGpuBackend::Dx12
            }
            #[cfg(not(target_os = "windows"))]
            {
                EngineGpuBackend::Vulkan
            }
        }
    }
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
    let hints = match backend {
        EngineGpuBackend::Vulkan => "\
         • Instala o actualiza los controladores de tu tarjeta gráfica (NVIDIA / AMD / Intel).\n\
         • En WSL2: usa WSLg (Windows 11), drivers GPU en Windows y en la distro \
         `sudo apt install mesa-vulkan-drivers` (o drivers NVIDIA para WSL).\n\
         • Comprueba Vulkan en terminal: `vulkaninfo` o `vkcube`.\n\
         • Reinicia el editor después de instalar drivers.",
        #[cfg(target_os = "windows")]
        EngineGpuBackend::Dx12 => "\
         • Instala o actualiza los controladores de tu tarjeta gráfica (NVIDIA / AMD / Intel).\n\
         • En Windows: asegúrate de tener DirectX 12 y el runtime actualizado (Windows Update).\n\
         • Reinicia el editor después de instalar drivers.",
    };

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
    fn three_d_default_by_platform() {
        #[cfg(target_os = "windows")]
        assert_eq!(
            resolve_backend(EngineGpuProfile::ThreeD),
            EngineGpuBackend::Dx12
        );
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            resolve_backend(EngineGpuProfile::ThreeD),
            EngineGpuBackend::Vulkan
        );
    }
}
