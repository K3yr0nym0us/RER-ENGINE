//! Ventana nativa winit independiente para las modales migradas del renderer.
//!
//! Comparte el `SharedGpu` del editor (mismo device) y crea su propia superficie.
//! Cada modal se dibuja en su propia ventana, fuera del viewport del motor; el
//! contenido vive en los módulos `ui::*_modal` y esta ventana solo aporta el
//! ciclo de vida y el render. El editor de emisores no usa esta ventana: es una
//! modal egui dentro del viewport principal (ver `ui::particles_config_modal`).

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
use winit::window::{Window, WindowButtons, WindowId};

use super::app::EditorApp;
use super::theme;

/// Modal mostrada en la ventana externa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeModal {
    /// Inspector de la entidad seleccionada (antes modal Electron `sm`).
    EntityProperties,
    /// Selector de modelos para «Reemplazar modelo», abierto desde propiedades
    /// como ventana nativa aparte (antes submodal de Electron).
    ModelPicker,
    RemoveAsset,
    ExitConfirm,
    DeleteScene,
    CreateScene,
    DeleteUiScreen,
    CreateUiScreen,
    RenameUiScreen,
    ControlBindings,
    WorldGen,
    Plugins,
    HudEditor,
    UserGuide,
    QuickBuild,
    SocketConfig,
    SceneScript,
    SceneVisualScript,
    /// Formulario de creación/edición de un tooltip de interacción.
    InteractionTooltip,
    /// Confirmación de borrado de un tooltip.
    DeleteInteractionTooltip,
    /// Editor nativo del script asignado a un tooltip.
    TooltipScriptEditor,
    /// Confirmación de quitar el script/nodo de un tooltip.
    RemoveTooltipScript,
}

/// Ancho interior (puntos egui), alineado con el diseño de las modales nativas.
const INNER_WIDTH: f64 = 380.0;
const MIN_INNER_HEIGHT: f64 = 120.0;
/// Ancho mínimo del modal de propiedades: lo suficiente para una pestaña y sus
/// campos. El ancho real lo manda `content_width` (solo las pestañas visibles).
const ENTITY_PROPS_WIDTH: f64 = 360.0;
const ENTITY_PROPS_HEIGHT: f64 = 560.0;
/// El selector de modelos es solo un combo + un botón: ventana compacta que se
/// ajusta a su contenido (antes submodal `sm` del inspector en Electron).
const MODEL_PICKER_WIDTH: f64 = 300.0;
/// Alto suficiente para el rótulo del modelo actual, el combo y el botón: la
/// ventana es de tamaño fijo (no redimensionable), así que debe caber todo.
const MODEL_PICKER_HEIGHT: f64 = 175.0;
const MODEL_PICKER_MIN_WIDTH: f64 = 260.0;
/// La guía es contenido estático largo: ventana ancha con scroll (antes `lg`).
const GUIDE_WIDTH: f64 = 760.0;
const GUIDE_HEIGHT: f64 = 560.0;
/// El selector de blueprints también usa tamaño fijo (antes modal `lg`).
const QUICKBUILD_WIDTH: f64 = 720.0;
const QUICKBUILD_HEIGHT: f64 = 520.0;
/// El editor de sockets crece con su contenido (ancho base, sin alto fijo).
const SOCKETS_WIDTH: f64 = 460.0;
const SOCKETS_INITIAL_HEIGHT: f64 = 240.0;
/// El editor de scripts reemplaza la modal `lg` del renderer.
const SCRIPT_WIDTH: f64 = 820.0;
const SCRIPT_HEIGHT: f64 = 620.0;
/// El editor de nodos era una modal `xl` redimensionable en el renderer.
const VISUAL_SCRIPT_WIDTH: f64 = 1180.0;
const VISUAL_SCRIPT_HEIGHT: f64 = 760.0;
const VISUAL_SCRIPT_MIN_WIDTH: f64 = 900.0;
const VISUAL_SCRIPT_MIN_HEIGHT: f64 = 560.0;
/// La modal de controles era `xl` en el renderer (1140 de contenido + 48 de padding).
const CONTROLS_WIDTH: f64 = 1188.0;
const CONTROLS_HEIGHT: f64 = 460.0;
/// El generador de mundo era una modal `lg` con contenido alto y scroll.
const WORLDGEN_WIDTH: f64 = 900.0;
const WORLDGEN_HEIGHT: f64 = 780.0;
/// La modal de plugins era `lg` (800 de contenido + 48 de padding).
const PLUGINS_WIDTH: f64 = 848.0;
const PLUGINS_HEIGHT: f64 = 640.0;
/// El editor de HUD era una modal `sm` (300 de contenido + 48 de padding).
const HUD_WIDTH: f64 = 348.0;
const HUD_HEIGHT: f64 = 560.0;
/// Editor del script asignado a un tooltip (520 de contenido + 48 de padding).
const TOOLTIP_SCRIPT_WIDTH: f64 = 568.0;
const TOOLTIP_SCRIPT_HEIGHT: f64 = 560.0;

pub(crate) struct ModalWindow {
    window: Arc<Window>,
    present: PresentTarget,
    egui_renderer: Option<Renderer<DefaultAllocator>>,
    ui_pool: vk::CommandPool,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    clear: vk::ClearValue,
    parent_pos: Option<PhysicalPosition<i32>>,
    parent_outer: PhysicalSize<u32>,
    fitted: bool,
    /// Las confirmaciones se ajustan al contenido; la guía usa tamaño fijo + scroll.
    fit_to_content: bool,
    /// La ventana de sockets se reajusta cada frame a medida que crece su contenido.
    dynamic_size: bool,
    /// Ancho base elegido al abrir (se mantiene al reajustar la altura).
    base_width: f64,
    kind: NativeModal,
}

impl ModalWindow {
    pub(crate) fn open(
        event_loop: &ActiveEventLoop,
        gpu: SharedGpu,
        beside: &Window,
        icon: Option<winit::window::Icon>,
        kind: NativeModal,
        title: String,
    ) -> Result<Self, String> {
        let (width, height) = match kind {
            NativeModal::EntityProperties => (ENTITY_PROPS_WIDTH, ENTITY_PROPS_HEIGHT),
            NativeModal::ModelPicker => (MODEL_PICKER_WIDTH, MODEL_PICKER_HEIGHT),
            NativeModal::UserGuide => (GUIDE_WIDTH, GUIDE_HEIGHT),
            NativeModal::QuickBuild => (QUICKBUILD_WIDTH, QUICKBUILD_HEIGHT),
            NativeModal::SocketConfig => (SOCKETS_WIDTH, SOCKETS_INITIAL_HEIGHT),
            NativeModal::SceneScript => (SCRIPT_WIDTH, SCRIPT_HEIGHT),
            NativeModal::SceneVisualScript => (VISUAL_SCRIPT_WIDTH, VISUAL_SCRIPT_HEIGHT),
            NativeModal::ControlBindings => (CONTROLS_WIDTH, CONTROLS_HEIGHT),
            NativeModal::WorldGen => (WORLDGEN_WIDTH, WORLDGEN_HEIGHT),
            NativeModal::Plugins => (PLUGINS_WIDTH, PLUGINS_HEIGHT),
            NativeModal::HudEditor => (HUD_WIDTH, HUD_HEIGHT),
            NativeModal::TooltipScriptEditor => (TOOLTIP_SCRIPT_WIDTH, TOOLTIP_SCRIPT_HEIGHT),
            _ => (INNER_WIDTH, 320.0),
        };
        // Propiedades también debe poder crecer: con `resizable=false` Windows
        // ignora `request_inner_size` y la ventana se queda en el ancho inicial.
        let resizable =
            kind == NativeModal::SceneVisualScript || kind == NativeModal::EntityProperties;
        let (min_width, min_height) = match kind {
            NativeModal::SceneVisualScript => (VISUAL_SCRIPT_MIN_WIDTH, VISUAL_SCRIPT_MIN_HEIGHT),
            NativeModal::EntityProperties => (ENTITY_PROPS_WIDTH, MIN_INNER_HEIGHT),
            NativeModal::ModelPicker => (MODEL_PICKER_MIN_WIDTH, MIN_INNER_HEIGHT),
            _ => (320.0, MIN_INNER_HEIGHT),
        };
        let mut attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(width, height))
            .with_min_inner_size(LogicalSize::new(min_width, min_height))
            .with_resizable(resizable)
            // El selector solo tiene botón de cerrar: ni minimizar ni maximizar.
            .with_enabled_buttons(if kind == NativeModal::ModelPicker {
                WindowButtons::CLOSE
            } else {
                WindowButtons::all()
            })
            // Estas ventanas acompañan al viewport: nunca deben quedar detrás
            // de la ventana nativa del motor (p. ej. para elegir huesos clickeando
            // en la escena con la modal de sockets abierta).
            // Estas ventanas acompañan al viewport: nunca deben quedar detrás
            // de la ventana nativa del motor (p. ej. para elegir huesos clickeando
            // en la escena con la modal de sockets abierta).
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
            .with_visible(false);
        if let Some(icon) = icon {
            attributes = attributes.with_window_icon(Some(icon));
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| format!("No se pudo crear la ventana modal: {error}"))?,
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
        .map_err(|error| format!("No se pudo crear el command pool de la modal: {error:?}"))?;

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
        .map_err(|error| format!("No se pudo crear el renderer de la modal: {error}"))?;

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
            fit_to_content: matches!(
                kind,
                NativeModal::RemoveAsset
                    | NativeModal::ExitConfirm
                    | NativeModal::DeleteScene
                    | NativeModal::CreateScene
                    | NativeModal::DeleteUiScreen
                    | NativeModal::CreateUiScreen
                    | NativeModal::RenameUiScreen
                    | NativeModal::InteractionTooltip
                    | NativeModal::DeleteInteractionTooltip
                    | NativeModal::TooltipScriptEditor
                    | NativeModal::RemoveTooltipScript
            ),
            dynamic_size: matches!(
                kind,
                NativeModal::SocketConfig | NativeModal::EntityProperties
            ),
            base_width: width,
            kind,
        })
    }

    pub(crate) fn kind(&self) -> NativeModal {
        self.kind
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Ventana subyacente, para centrar sobre ella una modal hija.
    pub(crate) fn window(&self) -> &Window {
        &self.window
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
        let kind = self.kind;
        let raw_input = self.egui_state.take_egui_input(self.window.as_ref());
        let mut full_output = self.egui_ctx.run_ui(raw_input, |ui| match kind {
            NativeModal::EntityProperties => {
                crate::editor::ui::entity_properties_modal::show_in_window(ui, app)
            }
            NativeModal::ModelPicker => {
                crate::editor::ui::entity_properties_modal::model_picker_window(ui, app)
            }
            NativeModal::RemoveAsset => {
                crate::editor::ui::remove_asset_modal::show_in_window(ui, app)
            }
            NativeModal::ExitConfirm => {
                crate::editor::ui::exit_confirm_modal::show_in_window(ui, app)
            }
            NativeModal::DeleteScene => {
                crate::editor::ui::delete_scene_modal::show_in_window(ui, app)
            }
            NativeModal::CreateScene => {
                crate::editor::ui::create_scene_modal::show_in_window(ui, app)
            }
            NativeModal::DeleteUiScreen => {
                crate::editor::ui::delete_ui_screen_modal::show_in_window(ui, app)
            }
            NativeModal::CreateUiScreen => {
                crate::editor::ui::create_ui_screen_modal::show_in_window(ui, app)
            }
            NativeModal::RenameUiScreen => {
                crate::editor::ui::rename_ui_screen_modal::show_in_window(ui, app)
            }
            NativeModal::ControlBindings => {
                crate::editor::ui::control_bindings_modal::show_in_window(ui, app)
            }
            NativeModal::WorldGen => crate::editor::ui::worldgen_modal::show_in_window(ui, app),
            NativeModal::Plugins => crate::editor::ui::plugins_modal::show_in_window(ui, app),
            NativeModal::HudEditor => crate::editor::ui::hud_editor_modal::show_in_window(ui, app),
            NativeModal::UserGuide => crate::editor::ui::user_guide_modal::show_in_window(ui, app),
            NativeModal::QuickBuild => {
                crate::editor::ui::quick_build_modal::show_in_window(ui, app)
            }
            NativeModal::SocketConfig => {
                crate::editor::ui::socket_config_modal::show_in_window(ui, app)
            }
            NativeModal::SceneScript => {
                crate::editor::ui::scene_script_modal::show_in_window(ui, app)
            }
            NativeModal::SceneVisualScript => {
                crate::editor::ui::visual_script_modal::show_in_window(ui, app)
            }
            NativeModal::InteractionTooltip => {
                crate::editor::ui::interaction_tooltip_modal::show_in_window(ui, app)
            }
            NativeModal::DeleteInteractionTooltip => {
                crate::editor::ui::delete_interaction_tooltip_modal::show_in_window(ui, app)
            }
            NativeModal::TooltipScriptEditor => {
                crate::editor::ui::tooltip_script_editor_modal::show_in_window(ui, app)
            }
            NativeModal::RemoveTooltipScript => {
                crate::editor::ui::remove_tooltip_script_modal::show_in_window(ui, app)
            }
        });
        let texture_sets = std::mem::take(&mut full_output.textures_delta.set);
        let texture_frees = std::mem::take(&mut full_output.textures_delta.free);
        self.egui_state
            .handle_platform_output(self.window.as_ref(), full_output.platform_output);
        self.fit_to_content(app);

        let primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let Some(egui_renderer) = self.egui_renderer.as_mut() else {
            return Ok(());
        };
        for (id, deltas) in &texture_sets {
            for delta in deltas {
                egui_renderer
                    .set_texture(self.present.gpu().queue(), self.ui_pool, *id, delta)
                    .map_err(|error| {
                        format!("No se pudieron subir texturas de la modal: {error}")
                    })?;
            }
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
            .map_err(|error| format!("No se pudo grabar la modal: {error}"))?;

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

        for id in &texture_frees {
            egui_renderer
                .free_texture(*id)
                .map_err(|error| format!("No se pudieron liberar texturas de la modal: {error}"))?;
        }

        Ok(())
    }

    /// Ajusta la ventana al contenido y la centra sobre el editor. Las ventanas
    /// dinámicas (sockets) se reajustan cada frame; las demás solo una vez.
    fn fit_to_content(&mut self, app: &EditorApp) {
        if !self.fit_to_content && !self.dynamic_size {
            return;
        }
        if self.fit_to_content && self.fitted {
            return;
        }
        let scale = self.window.scale_factor();
        let (width, height) = if self.dynamic_size {
            let is_props = self.kind == NativeModal::EntityProperties;
            let content_height = if is_props {
                app.entity_properties.content_height
            } else {
                app.socket_config.content_height
            };
            let height = f64::from(content_height);
            // Propiedades crece en ancho con sus pestañas; Sockets mantiene el suyo.
            let width = if is_props {
                f64::from(app.entity_properties.content_width).max(self.base_width)
            } else {
                self.base_width
            };
            (width, height.ceil().max(MIN_INNER_HEIGHT))
        } else {
            let used = self.egui_ctx.globally_used_rect();
            if !used.is_finite() || used.height() <= 1.0 {
                return;
            }
            let min_width = if self.kind == NativeModal::ModelPicker {
                MODEL_PICKER_MIN_WIDTH
            } else {
                INNER_WIDTH
            };
            (
                f64::from(used.width()).max(min_width),
                f64::from(used.height()).ceil().max(MIN_INNER_HEIGHT),
            )
        };
        let max_height = (f64::from(self.parent_outer.height) / scale) * 0.9;
        let height = if max_height > MIN_INNER_HEIGHT {
            height.min(max_height)
        } else {
            height
        };
        let max_width = (f64::from(self.parent_outer.width) / scale) * 0.9;
        let width = if max_width > self.base_width {
            width.min(max_width)
        } else {
            width
        };
        let current = self.window.inner_size();
        if (f64::from(current.width) / scale - width).abs() < 1.0
            && (f64::from(current.height) / scale - height).abs() < 1.0
        {
            return;
        }
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

/// Posición exterior para centrar la modal sobre el rectángulo exterior del editor.
fn set_centered_over_parent(
    window: &Window,
    parent_pos: PhysicalPosition<i32>,
    parent_outer: PhysicalSize<u32>,
    child_inner: PhysicalSize<u32>,
) {
    let inner = window.inner_size();
    let outer = window.outer_size();
    let deco = PhysicalSize::new(
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    );
    let child_outer = PhysicalSize::new(
        child_inner.width.saturating_add(deco.width),
        child_inner.height.saturating_add(deco.height),
    );
    window.set_outer_position(PhysicalPosition::new(
        parent_pos.x + (parent_outer.width as i32 - child_outer.width as i32) / 2,
        parent_pos.y + (parent_outer.height as i32 - child_outer.height as i32) / 2,
    ));
}

impl Drop for ModalWindow {
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
