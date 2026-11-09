//! Ruta de presentación Vulkan (Ash) del motor 2D, en paralelo a la de wgpu.
//!
//! Es la fase 2 del plan de migración: contexto, superficie, swapchain, clear y present. Todavía
//! **no dibuja nada** (los pases de sprites, rejilla y HUD llegan en las fases 4 y 5). Se activa
//! con `RER_2D_VULKAN=1`; sin esa variable el motor sigue usando la ruta wgpu de siempre, así que
//! la lógica de negocio del 2D no cambia mientras el camino nuevo no esté completo.
//!
//! El contexto no reimplementa Vulkan: reutiliza `engine_shared::vulkan`, el mismo núcleo que usa
//! el 3D (instancia, device, swapchain, `FrameTarget`, barreras). Aquí solo está lo propio del 2D:
//! pedir **sin** trazado y el orden de layouts del swapchain.

use ash::vk;
use rer_engine_shared::vulkan::gpu::cmd_image_barrier;
use rer_engine_shared::vulkan::{FrameAcquire, SharedGpu, VulkanError, VulkanOptions};
use rer_engine_shared::wgpu_surface::SurfacePresentError;
use winit::window::Window;

/// ¿Debe usar el 2D la ruta Vulkan? Por defecto no: wgpu sigue mandando hasta la fase 6.
pub(crate) fn enabled_from_env() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = std::env::var("RER_2D_VULKAN")
            .map(|value| value.trim() == "1" || value.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if on {
            log::warn!(
                "[2d-vulkan] RER_2D_VULKAN activo: presentación por Ash (sin pases de dibujo \
                 todavía). La ruta wgpu queda inactiva en este proceso."
            );
        }
        on
    })
}

/// Contexto Ash del 2D. Clonable como el del 3D: cada recurso guarda su `Arc` al device.
#[derive(Clone)]
pub(crate) struct Vulkan2d {
    gpu: SharedGpu,
}

impl Vulkan2d {
    /// Crea instancia, device y swapchain sobre la ventana dada.
    ///
    /// `VulkanOptions::default()` es la clave de la separación: al no pedir `ray_tracing`, el
    /// núcleo omite las `VK_KHR_*` de trazado, no exige *buffer device address* y no registra los
    /// handles de RT. Un error aquí es duro (no hay fallback a wgpu en caliente): se propaga al
    /// arranque como cualquier otro fallo de GPU.
    pub(crate) fn new(window: &Window) -> Result<Self, VulkanError> {
        SharedGpu::new(window, &VulkanOptions::default()).map(|gpu| Self { gpu })
    }

    /// Recrea el swapchain tras un `resize` de la ventana.
    pub(crate) fn resize(&self, width: u32, height: u32) -> Result<(), VulkanError> {
        self.gpu.resize(width, height)
    }

    /// Un frame: limpiar la imagen del swapchain al color de fondo y presentarla.
    ///
    /// El orden de layouts es el mismo que usa el 3D, que es el que ya está validado:
    /// `UNDEFINED → COLOR_ATTACHMENT` antes del pase y `COLOR_ATTACHMENT → PRESENT_SRC` al final,
    /// dentro del mismo command buffer. `submit_and_present` espera la imagen en `PRESENT_SRC`.
    pub(crate) fn present_clear(&self, color: [f32; 4]) -> Result<(), SurfacePresentError> {
        let frame = match self.gpu.begin_frame() {
            Ok(frame) => frame,
            Err(FrameAcquire::Outdated) => return Err(SurfacePresentError::Reconfigure),
            Err(FrameAcquire::Skip) => return Err(SurfacePresentError::SkipFrame),
            // Sin más información recuperable: el bucle del motor trata Validation como "siguiente
            // frame", igual que hace la ruta wgpu hoy.
            Err(FrameAcquire::Fail(_)) => return Err(SurfacePresentError::Validation),
        };
        let device = self.gpu.device();
        let cmd = frame.command;
        let extent = self.gpu.extent();
        let color_image = frame.color_image;

        cmd_image_barrier(
            device,
            cmd,
            color_image,
            vk::ImageAspectFlags::COLOR,
            1,
            1,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags::empty(),
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        );
        unsafe {
            let attachment = vk::RenderingAttachmentInfo::default()
                .image_view(frame.color_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: color },
                });
            let attachments = [attachment];
            let info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent,
                })
                .layer_count(1)
                .color_attachments(&attachments);
            device.cmd_begin_rendering(cmd, &info);
            device.cmd_end_rendering(cmd);
        }
        cmd_image_barrier(
            device,
            cmd,
            color_image,
            vk::ImageAspectFlags::COLOR,
            1,
            1,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::empty(),
        );

        self.gpu
            .submit_and_present(&frame)
            .map_err(|error| match error {
                FrameAcquire::Outdated => SurfacePresentError::Reconfigure,
                FrameAcquire::Skip => SurfacePresentError::SkipFrame,
                FrameAcquire::Fail(_) => SurfacePresentError::Validation,
            })
    }
}
