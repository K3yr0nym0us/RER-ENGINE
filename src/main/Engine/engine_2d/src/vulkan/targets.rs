//! Objetivos fuera de pantalla de la ruta 2D: color + profundidad con layouts rastreados.
//!
//! Son el destino del pase de escena (sprites/tiles) que después se compone sobre el swapchain.
//! El ciclo de layouts vive aquí y usa `vk::ImageLayout` como fuente única de verdad: nunca un
//! booleano "ya se muestreó", que es el fallo que produce cuadros negros.

use ash::vk;
use rer_engine_shared::vulkan::gpu::cmd_image_barrier;
use rer_engine_shared::vulkan::{GpuImage, SharedGpu, VulkanError};

/// Formato de profundidad del 2D: el mismo que el 3D (D32 con aspect DEPTH).
pub(crate) const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
/// Formato del color fuera de pantalla. UNORM a propósito: el pase de escena escribe valores
/// lineales, y el blit los copia a un swapchain sRGB que hace la codificación una sola vez.
pub(crate) const COLOR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

/// La imagen de color se muestrea (el blit lee su resultado), así que necesita `SAMPLED` además de
/// `COLOR_ATTACHMENT`; `TRANSFER_DST` para poder subirle píxeles si hiciera falta.
///
/// Son funciones, no constantes: en ash 0.38 la combinación de banderas usa `BitOr`, que no es
/// `const fn`, así que un `const` con `|` no compila.
fn color_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::COLOR_ATTACHMENT
        | vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::TRANSFER_DST
}

/// Profundidad de objetivo fuera de pantalla: adjunto y muestreable.
fn depth_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED
}

/// Barrera de layout con registro del layout real.
fn transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
    current: &mut vk::ImageLayout,
    want: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
) {
    if *current == want {
        return;
    }
    cmd_image_barrier(
        device, cmd, image, aspect, 1, 1, *current, want, src_stage, dst_stage, src_access,
        dst_access,
    );
    *current = want;
}

/// Color + profundidad fuera de pantalla, al tamaño del viewport.
pub(crate) struct SceneTargets {
    pub(crate) color: GpuImage,
    pub(crate) depth: GpuImage,
    pub(crate) extent: vk::Extent2D,
    /// Layouts reales, fuente única de verdad para las barreras.
    color_layout: vk::ImageLayout,
    depth_layout: vk::ImageLayout,
}

impl SceneTargets {
    pub(crate) fn new(gpu: &SharedGpu, extent: vk::Extent2D) -> Result<Self, VulkanError> {
        let color = gpu.create_image(
            extent,
            COLOR_FORMAT,
            color_usage(),
            vk::ImageAspectFlags::COLOR,
            "2d-scene",
        )?;
        let depth = gpu.create_image(
            extent,
            DEPTH_FORMAT,
            depth_usage(),
            vk::ImageAspectFlags::DEPTH,
            "2d-scene-depth",
        )?;
        Ok(Self {
            color,
            depth,
            extent,
            // `transition_initial` del núcleo compartido deja color en GENERAL y profundidad en
            // ATTACHMENT: se registra lo que realmente quedó, no lo que se supone.
            color_layout: vk::ImageLayout::GENERAL,
            depth_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        })
    }

    /// Reconstruye las imágenes al nuevo tamaño. El llamador debe haber esperado a la GPU: las
    /// imágenes viejas se liberan en el `Drop` y podrían estar en vuelo.
    pub(crate) fn resize(
        &mut self,
        gpu: &SharedGpu,
        extent: vk::Extent2D,
    ) -> Result<(), VulkanError> {
        if extent.width == 0 || extent.height == 0 {
            return Ok(());
        }
        if extent.width == self.extent.width && extent.height == self.extent.height {
            return Ok(());
        }
        self.color = gpu.create_image(
            extent,
            COLOR_FORMAT,
            color_usage(),
            vk::ImageAspectFlags::COLOR,
            "2d-scene",
        )?;
        self.depth = gpu.create_image(
            extent,
            DEPTH_FORMAT,
            depth_usage(),
            vk::ImageAspectFlags::DEPTH,
            "2d-scene-depth",
        )?;
        self.extent = extent;
        self.color_layout = vk::ImageLayout::GENERAL;
        self.depth_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        Ok(())
    }

    /// Deja color y profundidad en layout de adjunto antes de abrir el pase de escena.
    pub(crate) fn begin(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        transition(
            device,
            cmd,
            self.color.image,
            vk::ImageAspectFlags::COLOR,
            &mut self.color_layout,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags::empty(),
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        );
        transition(
            device,
            cmd,
            self.depth.image,
            vk::ImageAspectFlags::DEPTH,
            &mut self.depth_layout,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );
    }

    /// Cierra el pase: el color queda en GENERAL para que el blit lo muestree y la profundidad en
    /// READ_ONLY, el layout desde el que también la lee el 3D.
    pub(crate) fn end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        transition(
            device,
            cmd,
            self.color.image,
            vk::ImageAspectFlags::COLOR,
            &mut self.color_layout,
            vk::ImageLayout::GENERAL,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::SHADER_READ,
        );
        transition(
            device,
            cmd,
            self.depth.image,
            vk::ImageAspectFlags::DEPTH,
            &mut self.depth_layout,
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        );
    }
}
