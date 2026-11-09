// Las funciones `unsafe fn` de este archivo son el límite de seguridad del FFI de Vulkan.
// Cada llamada a ash es `unsafe` por contrato del crate; anidar un bloque por llamada no
// estrecha el invariante (Entry vive más que Instance, e Instance más que Device).
#![allow(unsafe_op_in_unsafe_fn)]

//! Contexto Vulkan compartido (Ash): instancia, adapter, device, superficie y swapchain.
//!
//! `ash` no libera `Instance` ni `Device` en `Drop`: el driver ata la vida de cada objeto
//! al padre (buffer → device → instance → entry). Por eso este módulo usa `unsafe` de FFI
//! y destruye en orden explícito. No hay un wrapper safe que exprese ese grafo.

use std::ffi::CStr;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ash::vk;
pub use gpu_allocator::MemoryLocation;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, AllocationScheme, Allocator, AllocatorCreateDesc,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use super::error::{VulkanError, vk_error};
use super::select::{
    RT_DEVICE_EXTENSIONS, hw_rt_supported, missing_extensions, pick_present_mode,
    pick_surface_format,
};

/// Número de frames que el núcleo mantiene en vuelo. Los consumidores necesitan un juego de
/// recursos por slot (uniformes, instancias); exponerlo evita que cada motor duplique el `2`.
pub const FRAMES_IN_FLIGHT: usize = 2;

/// Nombre de la capa de validación de Khronos (Vulkan SDK).
const VALIDATION_LAYER_NAME: &str = "VK_LAYER_KHRONOS_validation";
const VALIDATION_LAYER: &std::ffi::CStr = c"VK_LAYER_KHRONOS_validation";

pub struct RtProperties {
    pub shader_group_handle_size: u32,
    pub shader_group_handle_alignment: u32,
    pub shader_group_base_alignment: u32,
    pub min_scratch_alignment: u32,
}

struct GpuInner {
    /// Mantiene viva la librería de Vulkan durante toda la vida de `Instance`/`Device`.
    #[allow(dead_code)]
    entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical: vk::PhysicalDevice,
    device: ash::Device,
    queue: Mutex<vk::Queue>,
    queue_family: u32,
    hw_rt: bool,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: Mutex<SwapchainState>,
    /// Se suelta a mano en `Drop`, antes de `destroy_device`.
    allocator: ManuallyDrop<Mutex<Allocator>>,
    command_pool: vk::CommandPool,
    /// Pool exclusivo de `one_shot`, para que el hilo de carga del motor y el frame de
    /// espera del editor no compartan el mismo command pool.
    one_shot_pool: vk::CommandPool,
    one_shot_lock: Mutex<()>,
    frames: Mutex<[FrameSync; FRAMES_IN_FLIGHT]>,
    frame_slot: AtomicUsize,
    accel: Option<ash::khr::acceleration_structure::Device>,
    rt_pipeline: Option<ash::khr::ray_tracing_pipeline::Device>,
    rt_props: Option<RtProperties>,
    /// Cargador y messenger de `VK_EXT_debug_utils` (solo con `RER_VK_VALIDATION=1`).
    debug_utils: Option<ash::ext::debug_utils::Instance>,
    messenger: Option<vk::DebugUtilsMessengerEXT>,
    /// Límites del adaptador: deciden el MSAA real y el tamaño máximo de imágenes.
    limits: vk::PhysicalDeviceLimits,
}

impl Drop for GpuInner {
    fn drop(&mut self) {
        // FFI: destruir hijos antes que el device, y el device antes que la instance.
        unsafe {
            let _ = self.device.device_wait_idle();
            {
                let frames = self
                    .frames
                    .get_mut()
                    .unwrap_or_else(|poison| poison.into_inner());
                for frame in frames.iter() {
                    self.device.destroy_semaphore(frame.acquire, None);
                    self.device.destroy_fence(frame.fence, None);
                }
            }
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_command_pool(self.one_shot_pool, None);
            {
                let swapchain = self
                    .swapchain
                    .get_mut()
                    .unwrap_or_else(|poison| poison.into_inner());
                for view in swapchain.views.drain(..) {
                    self.device.destroy_image_view(view, None);
                }
                for semaphore in swapchain.render_finished.drain(..) {
                    self.device.destroy_semaphore(semaphore, None);
                }
                self.swapchain_loader
                    .destroy_swapchain(swapchain.handle, None);
            }
            // El allocator libera los bloques de memoria con el device todavía vivo.
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let (Some(loader), Some(messenger)) = (self.debug_utils.as_ref(), self.messenger) {
                loader.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// Device Vulkan compartido por mallas, texturas y el frame.
///
/// Cada recurso guarda un `Arc` para que el device no se destruya antes que sus buffers.
#[derive(Clone)]
pub struct SharedGpu {
    inner: Arc<GpuInner>,
}

/// Qué le pide al adaptador el motor que crea el contexto.
///
/// El núcleo es el mismo para los dos motores; lo único que cambia son las capacidades que cada uno
/// necesita. Con `ray_tracing = false` no se solicitan las extensiones `VK_KHR_*` de trazado (el 3D
/// las pide **todo-o-nada** y cae al BVH si falta alguna) ni se exige `buffer device address`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VulkanOptions {
    /// Pedir RT (`acceleration_structure`, `ray_tracing_pipeline`, `deferred_host_operations`),
    /// `shader_int64` y `buffer_device_address`. Lo usa el 3D; el 2D lo deja en `false`.
    pub ray_tracing: bool,
    /// Activar `imageCubeArray`: las sondas de reflejo del 3D muestrean un `samplerCubeArray`.
    pub image_cube_array: bool,
}

impl VulkanOptions {
    /// Opciones del motor 3D: trazado por hardware con fallback BVH y sondas en cubemap array.
    pub const fn three_d() -> Self {
        Self {
            ray_tracing: true,
            image_cube_array: true,
        }
    }
}

mod context;
mod frame;
mod memory;
mod swapchain;

use self::frame::FrameSync;
pub use self::frame::{FrameAcquire, FrameTarget};
use self::memory::pick_sample_count;
pub use self::memory::{GpuBuffer, GpuImage};
use self::swapchain::SwapchainState;

impl SharedGpu {
    pub fn hw_rt(&self) -> bool {
        self.inner.hw_rt
    }

    /// Límite de dimensión 2D de texturas del adaptador (para recortar tamaños de shadow map).
    pub fn max_image_dimension(&self) -> u32 {
        self.inner.limits.max_image_dimension2_d.max(2048)
    }

    /// Muestras de color admitidas como adjunto, como máximo `desired`.
    pub fn max_color_samples(&self, desired: u32) -> u32 {
        pick_sample_count(self.inner.limits.framebuffer_color_sample_counts, desired)
    }

    /// Muestras de profundidad admitidas como adjunto, como máximo `desired`.
    pub fn max_depth_samples(&self, desired: u32) -> u32 {
        pick_sample_count(self.inner.limits.framebuffer_depth_sample_counts, desired)
    }

    /// Muestras efectivas para el G-buffer: lo que el adaptador admite a la vez como adjunto de
    /// color Y de profundidad. Si la intersección no llega a 2× devuelve 1 y el renderer trabaja
    /// sin multisampling, en vez de crear pipelines inválidos.
    pub fn max_gbuffer_samples(&self, desired: u32) -> u32 {
        let both = self.inner.limits.framebuffer_color_sample_counts
            & self.inner.limits.framebuffer_depth_sample_counts;
        pick_sample_count(both, desired)
    }

    /// Techo de multisampling del adaptador para el G-buffer, sin capar por tier. Sirve para decir
    /// por qué un tier Ultra 8× acabó en 4× en vez de dejar al usuario sin pistas.
    pub fn gbuffer_sample_ceiling(&self) -> u32 {
        let both = self.inner.limits.framebuffer_color_sample_counts
            & self.inner.limits.framebuffer_depth_sample_counts;
        pick_sample_count(both, 64)
    }

    pub fn rt_properties(&self) -> Option<&RtProperties> {
        self.inner.rt_props.as_ref()
    }

    pub fn device(&self) -> &ash::Device {
        &self.inner.device
    }

    /// Instancia Vulkan, necesaria para crear el renderer de egui sobre este mismo device.
    pub fn instance(&self) -> &ash::Instance {
        &self.inner.instance
    }

    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.inner.physical
    }

    /// Cola gráfica/de presentación que usan los motores y el editor.
    pub fn queue(&self) -> vk::Queue {
        *self
            .inner
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn queue_family_index(&self) -> u32 {
        self.inner.queue_family
    }

    pub fn accel(&self) -> Option<&ash::khr::acceleration_structure::Device> {
        self.inner.accel.as_ref()
    }

    pub fn rt_pipeline(&self) -> Option<&ash::khr::ray_tracing_pipeline::Device> {
        self.inner.rt_pipeline.as_ref()
    }
}

/// Barrera de imagen. `pub` porque el renderer de cada motor la llama para transitar layouts
/// fuera del ciclo normal de recursos (adjunto ↔ lectura en los pases).
pub fn cmd_image_barrier(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
    mips: u32,
    layers: u32,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
) {
    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: mips.max(1),
            base_array_layer: 0,
            layer_count: layers.max(1),
        })
        .src_access_mask(src_access)
        .dst_access_mask(dst_access);
    unsafe {
        device.cmd_pipeline_barrier(
            command,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::pick_sample_count;
    use ash::vk;

    fn all_samples() -> vk::SampleCountFlags {
        vk::SampleCountFlags::TYPE_1
            | vk::SampleCountFlags::TYPE_2
            | vk::SampleCountFlags::TYPE_4
            | vk::SampleCountFlags::TYPE_8
    }

    #[test]
    fn pick_sample_count_never_exceeds_desired() {
        let supported = all_samples();
        assert_eq!(pick_sample_count(supported, 8), 8);
        assert_eq!(pick_sample_count(supported, 6), 4);
        assert_eq!(pick_sample_count(supported, 3), 2);
        assert_eq!(pick_sample_count(supported, 1), 1);
    }

    #[test]
    fn pick_sample_count_falls_back_to_supported_level() {
        let supported = vk::SampleCountFlags::TYPE_1 | vk::SampleCountFlags::TYPE_4;
        assert_eq!(pick_sample_count(supported, 8), 4);
        assert_eq!(pick_sample_count(supported, 2), 1);
    }

    #[test]
    fn pick_sample_count_without_msaa_support_returns_one() {
        assert_eq!(pick_sample_count(vk::SampleCountFlags::empty(), 8), 1);
    }
}
