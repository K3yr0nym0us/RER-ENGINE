//! Ruta de render Vulkan (Ash) del motor 2D.
//!
//! El contexto no reimplementa Vulkan: reutiliza `rer_engine_core::vulkan`, el mismo núcleo que usa
//! el 3D (instancia, device, swapchain, `FrameTarget`, barreras). Aquí solo está lo propio del 2D:
//! pedir **sin** trazado, el atlas de sprites y el orden de pases del frame.
//!
//! `Vulkan2d` es barato de clonar indirectamente (el `State` lo guarda en `Arc`): contiene un
//! `Mutex<Renderer>` porque el `resize` y el `render` llegan por `&self` y ambos necesitan
//! préstamo mutable del grabador de comandos.

use std::sync::Mutex;

use rer_engine_core::surface::SurfacePresentError;
use rer_engine_core::vulkan::{FrameAcquire, FrameTarget, SharedGpu, VulkanError, VulkanOptions};
use winit::window::Window;

mod layout;
mod renderer;
mod targets;

pub(crate) use renderer::{FrameInput, GizmoBatch, GridBatch, HudBatch, NdcOverlay};

/// Contexto Ash del 2D. Se comparte con `Arc` desde el `State`: el `resize` y el `render` tienen
/// que ver los mismos recursos, y el renderer no es clonable (posee imágenes, buffers y
/// pipelines con `Drop`).
pub(crate) struct Vulkan2d {
    gpu: SharedGpu,
    renderer: Mutex<renderer::Renderer>,
}

impl Vulkan2d {
    /// Crea instancia, device y swapchain sobre la ventana dada, y todos los recursos del frame.
    ///
    /// `VulkanOptions::default()` es la clave de la separación: al no pedir `ray_tracing`, el
    /// núcleo omite las `VK_KHR_*` de trazado, no exige *buffer device address* y no registra los
    /// handles de RT. Un error aquí es duro: se propaga al arranque como cualquier otro fallo de GPU.
    pub(crate) fn new(window: &Window) -> Result<Self, VulkanError> {
        let gpu = SharedGpu::new(window, &VulkanOptions::default())?;
        let renderer = renderer::Renderer::new(&gpu)?;
        let extent = renderer.extent();
        log::info!(
            "[2d-vulkan] renderer listo: scene target {}x{} + atlas de sprites",
            extent.width,
            extent.height
        );
        Ok(Self {
            gpu,
            renderer: Mutex::new(renderer),
        })
    }

    /// Variante para el editor: reutiliza el `SharedGpu` de la ventana compartida y no
    /// crea instancia, device ni swapchain propios.
    #[cfg_attr(not(feature = "editor"), allow(dead_code))]
    pub(crate) fn with_gpu(gpu: SharedGpu) -> Result<Self, VulkanError> {
        let renderer = renderer::Renderer::new(&gpu)?;
        Ok(Self {
            gpu,
            renderer: Mutex::new(renderer),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, renderer::Renderer> {
        self.renderer
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Recrea el swapchain y los objetivos tras un `resize` de la ventana.
    pub(crate) fn resize(&self, width: u32, height: u32) -> Result<(), VulkanError> {
        self.gpu.resize(width, height)?;
        let extent = self.gpu.extent();
        if let Err(error) = self.lock().resize(extent.width, extent.height) {
            // Sin objetivos nuevos el frame seguiría usando el tamaño viejo: se ve mal, no se
            // cuelga. Se avisa una vez por fallo en vez de abortar el bucle de eventos.
            log::warn!("[2d-vulkan] no se pudo recrear el scene target: {error}");
        }
        Ok(())
    }

    /// Redimensiona solo el scene target offscreen (sin tocar el swapchain del editor).
    #[cfg_attr(not(feature = "editor"), allow(dead_code))]
    pub(crate) fn resize_scene(&self, width: u32, height: u32) -> Result<(), VulkanError> {
        self.lock().resize(width, height)
    }

    /// Cambia el present mode del swapchain (V-Sync on/off) recreándolo.
    pub(crate) fn set_vsync(&self, enabled: bool) -> Result<(), VulkanError> {
        self.gpu.set_vsync(enabled)
    }

    /// Copia una región RGBA al atlas Vulkan. La llama `TextureAtlas` en cada `pack`/`update`,
    /// así que las dos rutas de render ven exactamente los mismos píxeles.
    pub(crate) fn upload_atlas_region(
        &self,
        rgba: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), VulkanError> {
        self.lock().upload_atlas_region(rgba, x, y, width, height)
    }

    /// Copia una región RGBA al atlas de glyphs/UI de pantalla.
    pub(crate) fn upload_hud_atlas_region(
        &self,
        rgba: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), VulkanError> {
        self.lock()
            .upload_hud_atlas_region(rgba, x, y, width, height)
    }

    /// Un frame completo: adquiere la imagen, graba escena + composición y presenta.
    ///
    /// El renderer espera la imagen en `PRESENT_SRC_KHR` al terminar; `submit_and_present` es el
    /// que hace la espera y la presentación.
    pub(crate) fn render(&self, input: FrameInput<'_>) -> Result<(), SurfacePresentError> {
        let frame = match self.gpu.begin_frame() {
            Ok(frame) => frame,
            Err(FrameAcquire::Outdated) => return Err(SurfacePresentError::Reconfigure),
            Err(FrameAcquire::Skip) => return Err(SurfacePresentError::SkipFrame),
            // Sin más información recuperable: el bucle del motor trata Validation como "siguiente
            // frame".
            Err(FrameAcquire::Fail(_)) => return Err(SurfacePresentError::Validation),
        };
        let mut renderer = self.lock();
        renderer
            .render(&frame, input)
            .map_err(|_| SurfacePresentError::Validation)?;
        drop(renderer);
        self.gpu
            .submit_and_present(&frame)
            .map_err(|error| match error {
                FrameAcquire::Outdated => SurfacePresentError::Reconfigure,
                FrameAcquire::Skip => SurfacePresentError::SkipFrame,
                FrameAcquire::Fail(_) => SurfacePresentError::Validation,
            })
    }

    /// Graba el frame 2D en el `FrameTarget` abierto por el editor y no presenta.
    ///
    /// Deja la imagen del swapchain en `COLOR_ATTACHMENT_OPTIMAL` para que egui dibuje
    /// encima en el mismo command buffer.
    #[cfg_attr(not(feature = "editor"), allow(dead_code))]
    pub(crate) fn render_into(
        &self,
        frame: &FrameTarget,
        input: FrameInput<'_>,
    ) -> Result<(), SurfacePresentError> {
        let mut renderer = self.lock();
        renderer
            .render_into(frame, input)
            .map_err(|_| SurfacePresentError::Validation)
    }
}
