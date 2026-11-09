//! Contexto Vulkan/egui del editor, apoyado en el núcleo Ash compartido con los motores.
//!
//! No crea instancia, device ni swapchain propios: usa `SharedGpu`, el mismo que el motor
//! 3D. Así la ventana única solo tendrá que añadir la grabación de la escena dentro del
//! mismo command buffer.

use ash::vk;
use egui_ash_renderer::{
    DynamicRendering, Options, RenderMode, Renderer, allocator::DefaultAllocator,
};
use rer_engine_core::vulkan::gpu::cmd_image_barrier;
use rer_engine_core::vulkan::select::is_srgb_format;
use rer_engine_core::vulkan::{FRAMES_IN_FLIGHT, FrameAcquire, SharedGpu, VulkanOptions};
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct EditorGpu {
    // Orden de destrucción: egui → command pool (en `Drop`) → device (dentro de `gpu`).
    egui_renderer: Renderer<DefaultAllocator>,
    ui_pool: vk::CommandPool,
    gpu: SharedGpu,
    clear: vk::ClearValue,
}

impl EditorGpu {
    pub fn new(window: &Window, options: VulkanOptions) -> Result<Self, String> {
        let gpu = SharedGpu::new(window, &options).map_err(|error| error.to_string())?;
        let format = gpu.swapchain_format();

        let ui_pool = unsafe {
            gpu.device().create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(gpu.queue_family_index())
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )
        }
        .map_err(|error| format!("No se pudo crear el command pool del editor: {error:?}"))?;

        let egui_renderer = Renderer::with_default_allocator(
            gpu.instance(),
            gpu.physical_device(),
            gpu.device().clone(),
            RenderMode::DynamicRendering(DynamicRendering {
                color_attachment_format: format,
                depth_attachment_format: None,
                stencil_attachment_format: None,
            }),
            Options {
                in_flight_frames: FRAMES_IN_FLIGHT,
                enable_depth_test: false,
                enable_depth_write: false,
                srgb_framebuffer: is_srgb_format(format),
            },
        )
        .map_err(|error| format!("No se pudo crear el renderer egui: {error}"))?;

        Ok(Self {
            egui_renderer,
            ui_pool,
            gpu,
            clear: vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: crate::editor::theme::clear_color(),
                },
            },
        })
    }

    pub fn max_texture_dimension_2d(&self) -> u32 {
        self.gpu.max_image_dimension()
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        self.gpu
            .resize(size.width, size.height)
            .map_err(|error| error.to_string())
    }

    /// Dibuja un frame completo de egui y lo presenta.
    ///
    /// Devuelve `true` si egui pide repintar inmediatamente.
    pub fn render(
        &mut self,
        window: &Window,
        egui_ctx: &egui::Context,
        egui_state: &mut egui_winit::State,
        build: impl FnMut(&mut egui::Ui),
    ) -> Result<bool, String> {
        let raw_input = egui_state.take_egui_input(window);
        let full_output = egui_ctx.run_ui(raw_input, build);
        egui_state.handle_platform_output(window, full_output.platform_output);

        let primitives = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let repaint_now = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|output| output.repaint_delay.is_zero())
            .unwrap_or(true);

        // `set_textures` graba, envía y espera en la cola. Se hace antes de abrir el frame
        // para no invalidar el command buffer que devuelve `begin_frame`.
        if !full_output.textures_delta.set.is_empty() {
            self.egui_renderer
                .set_textures(
                    self.gpu.queue(),
                    self.ui_pool,
                    &full_output.textures_delta.set,
                )
                .map_err(|error| format!("No se pudieron subir texturas egui: {error}"))?;
        }

        let frame = match self.gpu.begin_frame() {
            Ok(frame) => frame,
            Err(FrameAcquire::Outdated | FrameAcquire::Skip) => {
                self.reconfigure(window)?;
                return Ok(true);
            }
            Err(FrameAcquire::Fail(error)) => return Err(error.to_string()),
        };

        let command = frame.command;
        let extent = frame.extent;
        let device = self.gpu.device();

        unsafe {
            cmd_image_barrier(
                device,
                command,
                frame.color_image,
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

            let attachment = vk::RenderingAttachmentInfo::default()
                .image_view(frame.color_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(self.clear);
            let rendering = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&attachment));
            device.cmd_begin_rendering(command, &rendering);
        }

        self.egui_renderer
            .cmd_draw(command, extent, full_output.pixels_per_point, &primitives)
            .map_err(|error| format!("No se pudo grabar egui: {error}"))?;

        unsafe {
            device.cmd_end_rendering(command);
            cmd_image_barrier(
                device,
                command,
                frame.color_image,
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
        }

        match self.gpu.submit_and_present(&frame) {
            Ok(()) | Err(FrameAcquire::Skip) => {}
            Err(FrameAcquire::Outdated) => self.reconfigure(window)?,
            Err(FrameAcquire::Fail(error)) => return Err(error.to_string()),
        }

        self.egui_renderer
            .free_textures(&full_output.textures_delta.free)
            .map_err(|error| format!("No se pudieron liberar texturas egui: {error}"))?;

        Ok(repaint_now)
    }

    fn reconfigure(&mut self, window: &Window) -> Result<(), String> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        self.gpu
            .resize(size.width, size.height)
            .map_err(|error| error.to_string())
    }
}

impl Drop for EditorGpu {
    fn drop(&mut self) {
        unsafe {
            self.gpu.device().destroy_command_pool(self.ui_pool, None);
        }
    }
}
