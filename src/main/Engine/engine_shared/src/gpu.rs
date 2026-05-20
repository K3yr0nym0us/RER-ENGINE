//! Política GPU del motor: un solo backend por proceso (Vulkan hoy; DX12 futuro en Windows).
//!
//! Futuro DX12 (no activo aún):
//! - Cargo: `features = [..., "dx12"]` en wgpu (solo Windows).
//! - Variable de entorno `RER_GPU_BACKEND=dx12`.
//! - Sin fallback entre Vulkan y DX12 ni OpenGL.

use std::fmt;
use std::sync::Arc;

use winit::window::Window;

/// Backend gráfico activo del motor (exclusivo por sesión).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineGpuBackend {
    Vulkan,
    /// Reservado para Windows; requiere feature `dx12` en wgpu y build dedicado.
    #[cfg(target_os = "windows")]
    Dx12,
}

impl EngineGpuBackend {
    /// Backend solicitado al arranque. Hoy siempre Vulkan.
    pub fn active() -> Self {
        match std::env::var("RER_GPU_BACKEND")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "dx12" | "directx12" => {
                #[cfg(target_os = "windows")]
                {
                    log::warn!(
                        "RER_GPU_BACKEND=dx12 aún no está habilitado en esta build; usando Vulkan."
                    );
                }
                #[cfg(not(target_os = "windows"))]
                log::warn!("DirectX 12 solo está previsto en Windows; usando Vulkan.");
                Self::Vulkan
            }
            _ => Self::Vulkan,
        }
    }

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
    format!(
        "No se pudo iniciar el motor gráfico con {}.\n\n\
         {detail}\n\n\
         Sugerencias:\n\
         • Instala o actualiza los controladores de tu tarjeta gráfica (NVIDIA / AMD / Intel).\n\
         • En WSL2: usa WSLg (Windows 11), drivers GPU en Windows y en la distro \
         `sudo apt install mesa-vulkan-drivers` (o drivers NVIDIA para WSL).\n\
         • Comprueba Vulkan en terminal: `vulkaninfo` o `vkcube`.\n\
         • Reinicia el editor después de instalar drivers.",
        backend.label()
    )
}

/// Crea instancia wgpu, surface y adapter. Solo el backend de [`EngineGpuBackend::active`].
pub async fn init_gpu(
    window: Arc<Window>,
) -> Result<(wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter), GpuInitError> {
    let backend = EngineGpuBackend::active();
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
        .ok_or_else(|| {
            GpuInitError {
                message: user_facing_message(
                    backend,
                    "No se encontró ningún adaptador gráfico compatible con la superficie.",
                ),
            }
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
