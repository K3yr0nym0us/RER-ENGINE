//! Ventana nativa winit independiente para el editor de emisores de partículas.
//!
//! Comparte el `SharedGpu` del editor (mismo device) y crea su propia superficie
//! Win32, así se puede arrastrar a otro monitor y usar el viewport a la vez.

use std::sync::Arc;

use ash::vk;
use egui_ash_renderer::{
    DynamicRendering, Options, RenderMode, Renderer, allocator::DefaultAllocator,
};
use rer_engine_core::vulkan::gpu::cmd_image_barrier;
use rer_engine_core::vulkan::select::is_srgb_format;
use rer_engine_core::vulkan::{FRAMES_IN_FLIGHT, FrameAcquire, PresentTarget, SharedGpu};
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::app::EditorApp;
use super::cli::Locale;
use super::i18n::tr;
use super::theme;

/// Ancho interior de la ventana del emisor (puntos egui).
const INNER_WIDTH: f64 = 420.0;
const MIN_INNER_WIDTH: f64 = 320.0;
const MIN_INNER_HEIGHT: f64 = 160.0;

pub(crate) struct ParticleWindow {
    window: Arc<Window>,
    present: PresentTarget,
    egui_renderer: Option<Renderer<DefaultAllocator>>,
    ui_pool: vk::CommandPool,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    clear: vk::ClearValue,
    parent_pos: Option<PhysicalPosition<i32>>,
    parent_outer: PhysicalSize<u32>,
    /// Tras el primer layout, la ventana ya se encogió al contenido y se centró.
    fitted: bool,
}

impl ParticleWindow {
    pub(crate) fn open(
        event_loop: &ActiveEventLoop,
        gpu: SharedGpu,
        locale: Locale,
        beside: &Window,
        icon: Option<winit::window::Icon>,
        is_new: bool,
    ) -> Result<Self, String> {
        let title = tr(
            locale,
            "Editor de emisor de partículas",
            "Particle emitter editor",
        );
        let inner_height = estimated_inner_height(is_new);
        let mut attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(INNER_WIDTH, inner_height))
            .with_min_inner_size(LogicalSize::new(MIN_INNER_WIDTH, MIN_INNER_HEIGHT))
            .with_visible(false);
        if let Some(icon) = icon {
            attributes = attributes.with_window_icon(Some(icon));
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| format!("No se pudo crear la ventana del emisor: {error}"))?,
        );
        let parent_pos = beside.outer_position().ok();
        let parent_outer = beside.outer_size();
        if let Some(parent_pos) = parent_pos {
            set_centered_over_parent(&window, parent_pos, parent_outer, window.inner_size());
        }
        let present =
            PresentTarget::new(gpu, window.as_ref()).map_err(|error| error.to_string())?;
        let format = present.swapchain_format();

        let ui_pool = unsafe {
            present.gpu().device().create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(present.gpu().queue_family_index())
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )
        }
        .map_err(|error| format!("No se pudo crear el command pool egui extra: {error:?}"))?;

        let egui_renderer = Renderer::with_default_allocator(
            present.gpu().instance(),
            present.gpu().physical_device(),
            present.gpu().device().clone(),
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
        .map_err(|error| format!("No se pudo crear el renderer egui extra: {error}"))?;

        let egui_ctx = egui::Context::default();
        theme::apply(&egui_ctx);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            Some(winit::window::Theme::Dark),
            Some(present.gpu().max_image_dimension() as usize),
        );

        window.set_visible(true);
        window.focus_window();

        Ok(Self {
            window,
            present,
            egui_renderer: Some(egui_renderer),
            ui_pool,
            egui_ctx,
            egui_state,
            clear: vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: theme::clear_color(),
                },
            },
            parent_pos,
            parent_outer,
            fitted: false,
        })
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub(crate) fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub(crate) fn inner_size(&self) -> PhysicalSize<u32> {
        self.window.inner_size()
    }

    pub(crate) fn on_window_event(&mut self, event: &WindowEvent) -> egui_winit::EventResponse {
        self.egui_state.on_window_event(self.window.as_ref(), event)
    }

    pub(crate) fn resize(&self, size: PhysicalSize<u32>) -> Result<(), String> {
        self.present
            .resize(size.width, size.height)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn render(&mut self, app: &mut EditorApp) -> Result<(), String> {
        let raw_input = self.egui_state.take_egui_input(self.window.as_ref());
        let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            crate::editor::ui::show_particle_editor(ui, app);
        });
        self.egui_state
            .handle_platform_output(self.window.as_ref(), full_output.platform_output);
        self.fit_to_content();

        let primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let Some(egui_renderer) = self.egui_renderer.as_mut() else {
            return Ok(());
        };
        if !full_output.textures_delta.set.is_empty() {
            egui_renderer
                .set_textures(
                    self.present.gpu().queue(),
                    self.ui_pool,
                    &full_output.textures_delta.set,
                )
                .map_err(|error| format!("No se pudieron subir texturas egui extra: {error}"))?;
        }

        let frame = match self.present.begin_frame() {
            Ok(frame) => frame,
            Err(FrameAcquire::Outdated | FrameAcquire::Skip) => {
                let size = self.window.inner_size();
                self.present
                    .resize(size.width, size.height)
                    .map_err(|error| error.to_string())?;
                return Ok(());
            }
            Err(FrameAcquire::Fail(error)) => return Err(error.to_string()),
        };

        let command = frame.command;
        let extent = frame.extent;
        let device = self.present.gpu().device();

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

        egui_renderer
            .cmd_draw(command, extent, full_output.pixels_per_point, &primitives)
            .map_err(|error| format!("No se pudo grabar egui extra: {error}"))?;

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

        match self.present.submit_and_present(&frame) {
            Ok(()) | Err(FrameAcquire::Skip) => {}
            Err(FrameAcquire::Outdated) => {
                let size = self.window.inner_size();
                self.present
                    .resize(size.width, size.height)
                    .map_err(|error| error.to_string())?;
            }
            Err(FrameAcquire::Fail(error)) => return Err(error.to_string()),
        }

        egui_renderer
            .free_textures(&full_output.textures_delta.free)
            .map_err(|error| format!("No se pudieron liberar texturas egui extra: {error}"))?;

        Ok(())
    }

    fn fit_to_content(&mut self) {
        if self.fitted {
            return;
        }
        let used = self.egui_ctx.globally_used_rect();
        if !used.is_finite() || used.height() <= 1.0 {
            return;
        }
        let scale = self.window.scale_factor();
        let width = f64::from(used.width()).max(INNER_WIDTH);
        let height = f64::from(used.height()).ceil().max(MIN_INNER_HEIGHT);
        let max_height = (f64::from(self.parent_outer.height) / scale) * 0.9;
        let height = if max_height > MIN_INNER_HEIGHT {
            height.min(max_height)
        } else {
            height
        };
        let logical = LogicalSize::new(width, height);
        let inner = self
            .window
            .request_inner_size(logical)
            .unwrap_or_else(|| logical.to_physical(scale));
        if let Some(parent_pos) = self.parent_pos {
            set_centered_over_parent(&self.window, parent_pos, self.parent_outer, inner);
        }
        self.fitted = true;
    }
}

impl Drop for ParticleWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = self.present.gpu().device().device_wait_idle();
        }
        self.egui_renderer.take();
        unsafe {
            self.present
                .gpu()
                .device()
                .destroy_command_pool(self.ui_pool, None);
        }
    }
}

/// Posición exterior para centrar `child` sobre el rectángulo exterior del padre.
fn centered_outer_position(
    parent_pos: PhysicalPosition<i32>,
    parent_size: PhysicalSize<u32>,
    child_size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        parent_pos.x + (parent_size.width as i32 - child_size.width as i32) / 2,
        parent_pos.y + (parent_size.height as i32 - child_size.height as i32) / 2,
    )
}

fn decoration_delta(window: &Window) -> PhysicalSize<u32> {
    let inner = window.inner_size();
    let outer = window.outer_size();
    PhysicalSize::new(
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    )
}

fn set_centered_over_parent(
    window: &Window,
    parent_pos: PhysicalPosition<i32>,
    parent_outer: PhysicalSize<u32>,
    child_inner: PhysicalSize<u32>,
) {
    let deco = decoration_delta(window);
    let child_outer = PhysicalSize::new(
        child_inner.width.saturating_add(deco.width),
        child_inner.height.saturating_add(deco.height),
    );
    window.set_outer_position(centered_outer_position(
        parent_pos,
        parent_outer,
        child_outer,
    ));
}

/// Estimación en puntos para el primer frame (el layout real ajusta después).
fn estimated_inner_height(is_new: bool) -> f64 {
    let material = if is_new { 56.0 } else { 20.0 };
    32.0 + material + 6.0 + 64.0 + 11.0 * 28.0 + 28.0 + 56.0
}

#[cfg(test)]
mod tests {
    use super::centered_outer_position;
    use winit::dpi::{PhysicalPosition, PhysicalSize};

    #[test]
    fn centers_child_over_parent_outer_rect() {
        let pos = centered_outer_position(
            PhysicalPosition::new(100, 50),
            PhysicalSize::new(1000, 800),
            PhysicalSize::new(400, 200),
        );
        assert_eq!(pos, PhysicalPosition::new(400, 350));
    }
}
