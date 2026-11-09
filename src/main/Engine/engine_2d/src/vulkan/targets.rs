//! Recursos Vulkan de la ruta 2D (fase 3 del plan): objetivos fuera de pantalla y una textura
//! muestreable de prueba.
//!
//! Son los mismos recursos que van a necesitar los pases de sprites y del HUD cuando se enciendan
//! en la fase 5. Se crean aquí, y se validan aquí, porque el ciclo de adjunto de una imagen
//! *fuera* del swapchain es justo donde se rompen los layouts: el 3D lo pagó con cuadros negros en
//! el cielo y el CSM con capas sin pintar.

use ash::vk;
use rer_engine_shared::vulkan::gpu::cmd_image_barrier;
use rer_engine_shared::vulkan::{GpuImage, SharedGpu, VulkanError};

/// Formato de profundidad del 2D: el mismo que el 3D (D32 con aspect DEPTH).
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
/// La textura del scene target se muestrea (los pases de composición leen su resultado), así que
/// necesita `SAMPLED` además de `COLOR_ATTACHMENT`; `TRANSFER_DST` para poder subirle píxeles.
///
/// Son funciones, no constantes: en ash 0.38 la combinación de banderas usa `BitOr`, que no es
/// `const fn`, así que un `const` con `|` no compila.
fn color_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::COLOR_ATTACHMENT
        | vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::TRANSFER_DST
}

/// Profundidad de objetivo fuera de pantalla: adjunto y, a partir de la fase 5, muestreable.
fn depth_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED
}

/// Barrera de layout con registro del layout real.
///
/// Copia la política del 3D: sale por early-return si el layout ya coincide (una barrera
/// `X → X` es ilegal salvo los casos "general") y anota el nuevo. Nunca se guarda un booleano
/// "ya fue muestreada": eso fue el bug de los cuadros negros.
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
    color: GpuImage,
    depth: GpuImage,
    extent: vk::Extent2D,
    /// Layouts reales, fuente única de verdad para las barreras.
    color_layout: vk::ImageLayout,
    depth_layout: vk::ImageLayout,
}

impl SceneTargets {
    pub(crate) fn new(gpu: &SharedGpu, extent: vk::Extent2D) -> Result<Self, VulkanError> {
        let color = gpu.create_image(
            extent,
            vk::Format::R8G8B8A8_UNORM,
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
            // ATTACHMENT: se registra lo que realmente quedó, no lo que se suppose.
            color_layout: vk::ImageLayout::GENERAL,
            depth_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        })
    }

    /// Reconstruye las imágenes al nuevo tamaño. La vieja se libera con su `Drop` (destruye view e
    /// imagen tras `device_wait_idle` del frame, así que no puede estar referenciada en vuelo).
    pub(crate) fn resize(
        &mut self,
        gpu: &SharedGpu,
        extent: vk::Extent2D,
    ) -> Result<(), VulkanError> {
        if extent.width == self.extent.width
            && extent.height == self.extent.height
            && extent.width > 0
            && extent.height > 0
        {
            return Ok(());
        }
        if extent.width == 0 || extent.height == 0 {
            // Ventana minimizada: mantener las imágenes actuales, un swapchain de 0 px no es legal.
            return Ok(());
        }
        self.color = gpu.create_image(
            extent,
            vk::Format::R8G8B8A8_UNORM,
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

    /// Abre el pase fuera de pantalla: deja color y profundidad en layout de adjunto y devuelve las
    /// vistas que el `RenderingInfo` debe usar.
    fn begin(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
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

    /// Cierra el pase: el color vuelve a GENERAL (así lo podrá muestrear la composición) y la
    /// profundidad a READ_ONLY, que es el layout desde el que el 3D también la lee.
    fn end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
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

    /// Un pase de limpieza fuera de pantalla.
    ///
    /// No dibuja geometría todavía (llega con los pipelines de la fase 5), pero sí ejercita lo que
    /// puede fallar en silencio: adjuntos de color y profundidad sobre imágenes creadas a mano, el
    /// `load_op CLEAR`, y los dos ciclos de layout. Con la capa de validación activa, un layout mal
    /// declarado aquí aparece como VUID y no como ruido en pantalla.
    pub(crate) fn clear_pass(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        color: [f32; 4],
    ) {
        self.begin(device, cmd);
        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.color.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: color },
                });
            let depth_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.depth.view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                });
            let colors = [color_attachment];
            let info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent: self.extent,
                })
                .layer_count(1)
                .color_attachments(&colors)
                .depth_attachment(&depth_attachment);
            device.cmd_begin_rendering(cmd, &info);
            device.cmd_end_rendering(cmd);
        }
        self.end(device, cmd);
    }

    pub(crate) fn extent(&self) -> vk::Extent2D {
        self.extent
    }
}

/// Textura RGBA muestreable subida por staging.
///
/// Hoy es un solo píxel blanco: lo que se prueba es el camino (crear imagen con `SAMPLED`,
/// copiar buffer→imagen con el aspecto y el layout correctos, y volver a GENERAL), que es el que va
/// a usar el atlas de sprites. Se queda viva como semilla para la fase 5.
pub(crate) struct SampledTexture {
    image: GpuImage,
}

impl SampledTexture {
    pub(crate) fn white(gpu: &SharedGpu) -> Result<Self, VulkanError> {
        let extent = vk::Extent2D {
            width: 1,
            height: 1,
        };
        let image = gpu.create_image(
            extent,
            vk::Format::R8G8B8A8_SRGB,
            // Solo de lectura: no hace falta que sea adjunto, pero sí muestreable y destino de la
            // copia por staging.
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
            "2d-white",
        )?;
        gpu.upload_rgba_region(&image, 0, 0, 1, 1, &[255, 255, 255, 255])?;
        Ok(Self { image })
    }

    /// Dimensiones declaradas: se usan en el log de arranque para dejar constancia de que el recurso
    /// existe (y para que el campo no sea "escritura y olvido").
    pub(crate) fn label(&self) -> String {
        format!("{}x{}", self.image.extent.width, self.image.extent.height)
    }
}
