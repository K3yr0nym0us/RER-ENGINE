//! Ruta de render Vulkan (Ash) del motor 2D, en paralelo a la de wgpu.
//!
//! Se activa con `RER_2D_VULKAN=1`; sin esa variable el motor sigue usando la ruta wgpu de
//! siempre, así que la lógica de negocio del 2D no cambia mientras el camino nuevo no esté
//! completo (`RER_2D_VULKAN=1` es la puerta, retirar wgpu es la fase 6).
//!
//! El contexto no reimplementa Vulkan: reutiliza `engine_shared::vulkan`, el mismo núcleo que usa
//! el 3D (instancia, device, swapchain, `FrameTarget`, barreras). Aquí solo está lo propio del 2D:
//! pedir **sin** trazado, el atlas de sprites y el orden de pases del frame.
//!
//! `Vulkan2d` es barato de clonar indirectamente (el `State` lo guarda en `Arc`): contiene un
//! `Mutex<Renderer>` porque el `resize` y el `render` llegan por `&self` y ambos necesitan
//! préstamo mutable del grabador de comandos.

use std::sync::Mutex;

use rer_engine_shared::vulkan::{FrameAcquire, SharedGpu, VulkanError, VulkanOptions};
use rer_engine_shared::wgpu_surface::SurfacePresentError;
use winit::window::Window;

mod layout;
mod renderer;
mod targets;

pub(crate) use renderer::{FrameInput, GizmoBatch, GridBatch};

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
                "[2d-vulkan] RER_2D_VULKAN activo: render por Ash. La ruta wgpu queda inactiva \
                 en este proceso."
            );
        }
        on
    })
}

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
    /// handles de RT. Un error aquí es duro (no hay fallback a wgpu en caliente): se propaga al
    /// arranque como cualquier otro fallo de GPU.
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
            // frame", igual que hace la ruta wgpu hoy.
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
}
