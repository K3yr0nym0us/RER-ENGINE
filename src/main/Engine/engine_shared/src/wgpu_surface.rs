//! Adquisición de textura de swapchain (wgpu 29+: `CurrentSurfaceTexture`).

use winit::dpi::PhysicalSize;

/// Limita el tamaño físico de la superficie a lo que admite el adaptador (evita panic en `configure`).
pub fn clamp_surface_physical_size(
    device: &wgpu::Device,
    size: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
    let max = device.limits().max_texture_dimension_2d.max(1);
    PhysicalSize::new(size.width.clamp(1, max), size.height.clamp(1, max))
}

/// Resultado al pedir el frame actual de la superficie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePresentError {
    /// Reconfigurar la superficie (p. ej. `resize` + `configure`).
    Reconfigure,
    /// Saltar este frame (timeout, ocluida, etc.).
    SkipFrame,
    /// Error de validación capturado por wgpu.
    Validation,
}

/// Obtiene la textura de presentación o indica cómo proceder.
pub fn acquire_surface_texture(
    surface: &wgpu::Surface,
) -> Result<wgpu::SurfaceTexture, SurfacePresentError> {
    match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(texture)
        | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Ok(texture),
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            Err(SurfacePresentError::SkipFrame)
        }
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            Err(SurfacePresentError::Reconfigure)
        }
        wgpu::CurrentSurfaceTexture::Validation => Err(SurfacePresentError::Validation),
    }
}
