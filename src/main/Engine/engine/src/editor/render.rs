//! Contexto Vulkan/egui del editor, apoyado en el núcleo Ash compartido con los motores.
//!
//! No crea instancia, device ni swapchain propios: usa `SharedGpu`, el mismo que el motor
//! 3D. En ventana única graba la escena del motor y la UI de egui en el mismo command
//! buffer: la escena dentro del rectángulo del `CentralPanel` y egui encima.

use ash::vk;
use egui_ash_renderer::{
    DynamicRendering, Options, RenderMode, Renderer, allocator::DefaultAllocator,
};
use rer_engine_core::vulkan::gpu::cmd_image_barrier;
use rer_engine_core::vulkan::select::is_srgb_format;
use rer_engine_core::vulkan::{FRAMES_IN_FLIGHT, FrameAcquire, SharedGpu, VulkanOptions};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::theme;

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
                    float32: theme::clear_color(),
                },
            },
        })
    }

    /// Copia del contexto para que el motor 3D comparta device y swapchain.
    pub fn shared_gpu(&self) -> SharedGpu {
        self.gpu.clone()
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

    /// Dibuja un frame completo: escena opcional del motor + egui, y lo presenta.
    ///
    /// `viewport` llega en puntos egui (coordenadas de ventana); la escena se graba en su
    /// equivalente físico. Devuelve `true` si egui pide repintar inmediatamente.
    pub fn render(
        &mut self,
        window: &Window,
        egui_ctx: &egui::Context,
        egui_state: &mut egui_winit::State,
        build: impl FnMut(&mut egui::Ui),
        engine: Option<&mut crate::editor::host::InProcessEngine>,
        viewport: Option<egui::Rect>,
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

        // Escena del motor en el rectángulo del viewport, si está disponible.
        let scene_area = if let (Some(app), Some(rect)) = (engine, viewport) {
            match scene_rect(rect, full_output.pixels_per_point, extent) {
                Some(area) => {
                    let scene_frame = crate::engine3d::vulkan::FrameTarget {
                        offset: area.offset,
                        extent: area.extent,
                        ..frame
                    };
                    let result = match app {
                        crate::editor::host::InProcessEngine::Three(app) => {
                            app.render_scene(&scene_frame)
                        }
                        crate::editor::host::InProcessEngine::Two(app) => {
                            app.render_scene(&scene_frame)
                        }
                    };
                    match result {
                        Ok(()) => Some(area),
                        Err(error) => {
                            log::warn!("[editor] no se pudo grabar la escena: {error:?}");
                            None
                        }
                    }
                }
                None => None,
            }
        } else {
            None
        };

        unsafe {
            // Si no hay escena, el editor transita la imagen; si la hay, el motor ya la
            // dejó en COLOR_ATTACHMENT_OPTIMAL dentro de su propio `record`.
            if scene_area.is_none() {
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
            }

            let attachment = vk::RenderingAttachmentInfo::default()
                .image_view(frame.color_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(if scene_area.is_some() {
                    vk::AttachmentLoadOp::LOAD
                } else {
                    vk::AttachmentLoadOp::CLEAR
                })
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

/// Convierte el rectángulo del `CentralPanel` (puntos) a píxeles físicos, recortado al swapchain.
fn scene_rect(rect: egui::Rect, pixels_per_point: f32, extent: vk::Extent2D) -> Option<vk::Rect2D> {
    let x = (rect.min.x * pixels_per_point).round() as i32;
    let y = (rect.min.y * pixels_per_point).round() as i32;
    let width = (rect.width() * pixels_per_point).round() as i32;
    let height = (rect.height() * pixels_per_point).round() as i32;
    if width <= 0 || height <= 0 {
        return None;
    }
    let x = x.clamp(0, extent.width as i32);
    let y = y.clamp(0, extent.height as i32);
    let width = width.min(extent.width as i32 - x).max(1) as u32;
    let height = height.min(extent.height as i32 - y).max(1) as u32;
    Some(vk::Rect2D {
        offset: vk::Offset2D { x, y },
        extent: vk::Extent2D { width, height },
    })
}

impl Drop for EditorGpu {
    fn drop(&mut self) {
        unsafe {
            self.gpu.device().destroy_command_pool(self.ui_pool, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scene_rect;
    use ash::vk;

    fn viewport() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(268.0, 46.0), egui::vec2(744.0, 554.0))
    }

    fn extent() -> vk::Extent2D {
        vk::Extent2D {
            width: 1600,
            height: 1000,
        }
    }

    #[test]
    fn maps_viewport_at_100_percent() {
        let area = scene_rect(viewport(), 1.0, extent()).unwrap();
        assert_eq!(area.offset, vk::Offset2D { x: 268, y: 46 });
        assert_eq!(
            area.extent,
            vk::Extent2D {
                width: 744,
                height: 554
            }
        );
    }

    #[test]
    fn maps_viewport_at_125_percent() {
        let area = scene_rect(viewport(), 1.25, extent()).unwrap();
        assert_eq!(area.offset, vk::Offset2D { x: 335, y: 58 });
        assert_eq!(
            area.extent,
            vk::Extent2D {
                width: 930,
                height: 693
            }
        );
    }

    #[test]
    fn maps_viewport_at_150_percent() {
        let area = scene_rect(viewport(), 1.5, extent()).unwrap();
        assert_eq!(area.offset, vk::Offset2D { x: 402, y: 69 });
        assert_eq!(
            area.extent,
            vk::Extent2D {
                width: 1116,
                height: 831
            }
        );
    }

    #[test]
    fn clips_viewport_to_swapchain() {
        let rect = egui::Rect::from_min_size(egui::pos2(1000.0, 900.0), egui::vec2(800.0, 400.0));
        let area = scene_rect(rect, 1.0, extent()).unwrap();
        assert_eq!(area.offset, vk::Offset2D { x: 1000, y: 900 });
        assert_eq!(
            area.extent,
            vk::Extent2D {
                width: 600,
                height: 100
            }
        );
    }

    #[test]
    fn rejects_empty_viewport() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(0.0, 100.0));
        assert!(scene_rect(rect, 1.0, extent()).is_none());
    }
}
