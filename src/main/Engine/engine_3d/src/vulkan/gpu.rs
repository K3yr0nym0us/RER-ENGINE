// Las funciones `unsafe fn` de este archivo son el límite de seguridad del FFI de Vulkan.
// Cada llamada a ash es `unsafe` por contrato del crate; anidar un bloque por llamada no
// estrecha el invariante (Entry vive más que Instance, e Instance más que Device).
#![allow(unsafe_op_in_unsafe_fn)]

//! Contexto Vulkan del motor 3D.
//!
//! `ash` no libera `Instance` ni `Device` en `Drop`: el driver ata la vida de cada objeto
//! al padre (buffer → device → instance → entry). Por eso este módulo usa `unsafe` de FFI
//! y destruye en orden explícito. No hay un wrapper safe que exprese ese grafo.

use std::ffi::CStr;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ash::vk;
use gpu_allocator::MemoryLocation;
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

const FRAMES_IN_FLIGHT: usize = 2;

/// Nombre de la capa de validación de Khronos (Vulkan SDK).
const VALIDATION_LAYER_NAME: &str = "VK_LAYER_KHRONOS_validation";
const VALIDATION_LAYER: &std::ffi::CStr = c"VK_LAYER_KHRONOS_validation";

/// Validación: en builds de debug se activa por defecto (los artefactos visuales se diagnostican
/// con el validador, no a ciegas). Se desactiva con `RER_VK_VALIDATION=0` y activa en release con `=1`.
fn validation_requested() -> bool {
    match std::env::var("RER_VK_VALIDATION") {
        Ok(value) => !value.is_empty() && value != "0",
        Err(_) => cfg!(debug_assertions),
    }
}

/// Modo serializado para diagnosticar contenido de targets compartidos (`RER_VK_SYNC_FRAMES=1`).
fn serialise_frames() -> bool {
    std::env::var("RER_VK_SYNC_FRAMES")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(true)
}

/// FFI: callback de `VK_EXT_debug_utils`. Solo lee `p_message` (C string del validation layer).
unsafe extern "system" fn vk_debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut std::ffi::c_void,
) -> vk::Bool32 {
    if data.is_null() {
        return vk::FALSE;
    }
    // La especificación garantiza `p_message` válido durante la llamada.
    let message = unsafe { std::ffi::CStr::from_ptr((*data).p_message) };
    let message = message.to_string_lossy();
    let message = message.replace('\n', " | ");
    if severity.intersects(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        log::error!("[vulkan-validación] {message}");
    } else {
        log::warn!("[vulkan-validación] {message}");
    }
    vk::FALSE
}

pub struct RtProperties {
    pub shader_group_handle_size: u32,
    pub shader_group_handle_alignment: u32,
    pub shader_group_base_alignment: u32,
    pub min_scratch_alignment: u32,
}

struct FrameSync {
    acquire: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
    command: vk::CommandBuffer,
}

struct SwapchainState {
    handle: vk::SwapchainKHR,
    format: vk::Format,
    extent: vk::Extent2D,
    views: Vec<vk::ImageView>,
    images: Vec<vk::Image>,
    vsync: bool,
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
    hw_rt: bool,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: Mutex<SwapchainState>,
    /// Se suelta a mano en `Drop`, antes de `destroy_device`.
    allocator: ManuallyDrop<Mutex<Allocator>>,
    command_pool: vk::CommandPool,
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
                    self.device.destroy_semaphore(frame.render_finished, None);
                    self.device.destroy_fence(frame.fence, None);
                }
            }
            self.device.destroy_command_pool(self.command_pool, None);
            {
                let swapchain = self
                    .swapchain
                    .get_mut()
                    .unwrap_or_else(|poison| poison.into_inner());
                for view in swapchain.views.drain(..) {
                    self.device.destroy_image_view(view, None);
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

pub struct GpuBuffer {
    gpu: SharedGpu,
    pub buffer: vk::Buffer,
    allocation: Option<Allocation>,
    pub size: u64,
    pub address: u64,
}

impl Drop for GpuBuffer {
    fn drop(&mut self) {
        unsafe {
            self.gpu.inner.device.destroy_buffer(self.buffer, None);
        }
        if let Some(allocation) = self.allocation.take() {
            let mut allocator = self
                .gpu
                .inner
                .allocator
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Err(error) = allocator.free(allocation) {
                log::error!("[vulkan] no se pudo liberar un buffer: {error}");
            }
        }
    }
}

pub struct GpuImage {
    gpu: SharedGpu,
    pub image: vk::Image,
    allocation: Option<Allocation>,
    pub view: vk::ImageView,
    /// Una vista `TYPE_2D` por capa del array, cuando hace falta dibujar capa a capa (CSM).
    /// Vacío salvo que se llame a `attach_layer_views`.
    layer_views: Vec<vk::ImageView>,
    #[allow(dead_code)]
    pub format: vk::Format,
    #[allow(dead_code)]
    pub extent: vk::Extent2D,
    pub aspect: vk::ImageAspectFlags,
    pub layers: u32,
    pub mips: u32,
}

impl GpuImage {
    /// Vista de la capa `index` del array, o la vista completa si no se crearon las por capa.
    ///
    /// `vkCmdBeginRendering` toma el rango de capas del imageView del adjunto, así que para dibujar
    /// UNA cascada del shadow map hay que pasarle la vista de ESA capa.
    pub fn layer_view(&self, index: u32) -> vk::ImageView {
        self.layer_views
            .get(index as usize)
            .copied()
            .unwrap_or(self.view)
    }

    /// Crea (o recrea) una vista `TYPE_2D` por capa del array.
    ///
    /// Sin esto no se puede dibujar una cascada sin tocar las demás: `vkCmdBeginRendering` toma el
    /// rango de capas del imageView del adjunto, y la vista completa del array abarca todas.
    pub fn attach_layer_views(&mut self, what: &str) -> Result<(), VulkanError> {
        let device = &self.gpu.inner.device;
        unsafe {
            for view in self.layer_views.drain(..) {
                device.destroy_image_view(view, None);
            }
        }
        let mut views = Vec::with_capacity(self.layers as usize);
        for layer in 0..self.layers {
            let created = unsafe {
                device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(self.image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(self.format)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: self.aspect,
                            base_mip_level: 0,
                            level_count: self.mips,
                            base_array_layer: layer,
                            layer_count: 1,
                        }),
                    None,
                )
            };
            match created {
                Ok(view) => views.push(view),
                Err(error) => {
                    for view in views {
                        unsafe { device.destroy_image_view(view, None) };
                    }
                    return Err(vk_error(
                        &format!("crear la vista de la capa de {what}"),
                        error,
                    ));
                }
            }
        }
        self.layer_views = views;
        Ok(())
    }
}

impl Drop for GpuImage {
    fn drop(&mut self) {
        for view in self.layer_views.drain(..) {
            unsafe { self.gpu.inner.device.destroy_image_view(view, None) };
        }
        unsafe {
            self.gpu.inner.device.destroy_image_view(self.view, None);
            self.gpu.inner.device.destroy_image(self.image, None);
        }
        if let Some(allocation) = self.allocation.take() {
            let mut allocator = self
                .gpu
                .inner
                .allocator
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Err(error) = allocator.free(allocation) {
                log::error!("[vulkan] no se pudo liberar una imagen: {error}");
            }
        }
    }
}

pub struct FrameTarget {
    pub slot: usize,
    pub image_index: u32,
    pub command: vk::CommandBuffer,
    pub color_view: vk::ImageView,
    pub color_image: vk::Image,
    pub extent: vk::Extent2D,
    #[allow(dead_code)]
    pub format: vk::Format,
}

impl SharedGpu {
    pub fn new(window: &Window) -> Result<Self, VulkanError> {
        let gpu = Self::create(window)?;
        // Comprueba el allocator antes de registrar el device como listo.
        let smoke = gpu.create_buffer(
            256,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "smoke",
        )?;
        drop(smoke);
        log::info!(
            "[vulkan] contexto Ash listo. RT por hardware: {}",
            if gpu.hw_rt() {
                "disponible"
            } else {
                "no disponible (BVH por software)"
            }
        );
        Ok(gpu)
    }

    fn create(window: &Window) -> Result<Self, VulkanError> {
        // FFI: cargar vulkan-1.dll y crear instance/device. El Drop de GpuInner los destruye.
        unsafe { Self::create_unchecked(window) }
    }

    unsafe fn create_unchecked(window: &Window) -> Result<Self, VulkanError> {
        let entry = ash::Entry::load().map_err(|error| {
            VulkanError::new(format!(
                "No se pudo cargar vulkan-1.dll ({error}). Instala los controladores de la GPU o el Vulkan Runtime."
            ))
        })?;

        let display = window
            .display_handle()
            .map_err(|error| VulkanError::new(format!("Ventana sin display handle: {error}")))?
            .as_raw();
        let window_handle = window
            .window_handle()
            .map_err(|error| VulkanError::new(format!("Ventana sin window handle: {error}")))?
            .as_raw();
        // `enumerate_required_extensions` devuelve un slice estático: se copia para poder
        // añadir `VK_EXT_debug_utils` cuando la validación está activa.
        let mut instance_exts: Vec<*const std::ffi::c_char> =
            ash_window::enumerate_required_extensions(display)
                .map_err(|error| {
                    VulkanError::new(format!(
                        "No se pudieron enumerar las extensiones de superficie: {error:?}"
                    ))
                })?
                .to_vec();

        // Capa de validación opcional: RER_VK_VALIDATION=1. Es la única forma fiable de
        // saber si un parpadeo viene de un uso incorrecto de la API (layouts, descriptores,
        // sync) y no de la lógica de render.
        let mut enabled_layers: Vec<*const std::ffi::c_char> = Vec::new();
        let mut validation = false;
        if validation_requested() {
            let layers = entry
                .enumerate_instance_layer_properties()
                .unwrap_or_default();
            let exts = entry
                .enumerate_instance_extension_properties(None)
                .unwrap_or_default();
            let has_layer = layers
                .iter()
                .any(|layer| cstr_lossy(layer.layer_name.as_ptr()) == VALIDATION_LAYER_NAME);
            let has_ext = extension_name_list(&exts)
                .iter()
                .any(|name| name == "VK_EXT_debug_utils");
            if has_layer && has_ext {
                enabled_layers.push(VALIDATION_LAYER.as_ptr());
                instance_exts.push(vk::EXT_DEBUG_UTILS_NAME.as_ptr());
                validation = true;
                log::warn!("[vulkan-validación] capa de validación ACTIVADA (deslento el render)");
            } else {
                let falta = if !has_layer {
                    "la capa VK_LAYER_KHRONOS_validation (instala el Vulkan SDK)"
                } else {
                    "VK_EXT_debug_utils"
                };
                log::warn!("[vulkan-validación] pedido con RER_VK_VALIDATION pero falta {falta}");
            }
        }

        let app = vk::ApplicationInfo::default()
            .application_name(c"rer_engine_3d")
            .engine_name(c"rer-engine")
            .api_version(vk::API_VERSION_1_3);
        let mut instance_info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&instance_exts);
        if validation {
            instance_info = instance_info.enabled_layer_names(&enabled_layers);
        }
        let instance = entry
            .create_instance(&instance_info, None)
            .map_err(|error| vk_error("crear la instancia Vulkan", error))?;

        // El messenger debe crearse tras la instancia; se destruye en `Drop`.
        let (debug_utils, messenger) = if validation {
            let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(vk_debug_callback));
            let messenger = loader
                .create_debug_utils_messenger(&info, None)
                .map_err(|error| vk_error("crear el messenger de validación", error))?;
            (Some(loader), Some(messenger))
        } else {
            (None, None)
        };

        let surface = ash_window::create_surface(&entry, &instance, display, window_handle, None)
            .map_err(|error| vk_error("crear la superficie Win32", error))?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

        let physical = pick_physical_device(&instance, &surface_loader, surface)?;
        let props = instance.get_physical_device_properties(physical);
        let name = cstr_lossy(props.device_name.as_ptr());
        log::info!("[vulkan] adaptador: {name}");

        let extensions = instance
            .enumerate_device_extension_properties(physical)
            .map_err(|error| vk_error("enumerar extensiones del adaptador", error))?;
        let available = extension_name_list(&extensions);
        let missing = missing_extensions(RT_DEVICE_EXTENSIONS, &available);
        let mut hw_rt = hw_rt_supported(&available);
        if !missing.is_empty() {
            for ext in &missing {
                log::warn!(
                    "[vulkan] extensión de ray tracing ausente: {ext}. Se usará el BVH por software."
                );
            }
        }

        let queue_family = pick_queue_family(&instance, &surface_loader, physical, surface)?;

        let (mut queried_as, mut queried_rt, mut queried_12, mut queried_13) = feature_query();
        let mut feature_header = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut queried_13)
            .push_next(&mut queried_12)
            .push_next(&mut queried_as)
            .push_next(&mut queried_rt);
        instance.get_physical_device_features2(physical, &mut feature_header);

        if queried_13.dynamic_rendering == vk::FALSE || queried_13.synchronization2 == vk::FALSE {
            return Err(VulkanError::new(
                "El adaptador no expone Vulkan 1.3 (dynamic rendering / synchronization2).",
            ));
        }
        if queried_12.buffer_device_address == vk::FALSE {
            return Err(VulkanError::new(
                "El adaptador no expone buffer device address, necesario para el trazado.",
            ));
        }
        if hw_rt
            && (queried_as.acceleration_structure == vk::FALSE
                || queried_rt.ray_tracing_pipeline == vk::FALSE)
        {
            log::warn!(
                "[vulkan] las extensiones RT están presentes pero las features no. Se usará el BVH por software."
            );
            hw_rt = false;
        }

        let mut enabled_as = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
            .acceleration_structure(true);
        let mut enabled_rt =
            vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default().ray_tracing_pipeline(true);
        let mut enabled_12 = vk::PhysicalDeviceVulkan12Features::default()
            .buffer_device_address(true)
            .descriptor_indexing(true)
            .runtime_descriptor_array(true)
            .shader_sampled_image_array_non_uniform_indexing(true)
            .shader_storage_buffer_array_non_uniform_indexing(true)
            .descriptor_binding_partially_bound(true)
            .scalar_block_layout(true);
        let mut enabled_13 = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);
        // `DemoteToHelperInvocation` la emiten los `discard` del forward al apuntar a SPIR-V 1.6;
        // sin la feature, crear el módulo SPIR-V es inválido (lo reportaba la validación).
        if queried_13.shader_demote_to_helper_invocation != vk::FALSE {
            enabled_13 = enabled_13.shader_demote_to_helper_invocation(true);
        } else {
            log::warn!(
                "[vulkan] el adaptador no expone shaderDemoteToHelperInvocation: los recortes del forward usarán discard clásico."
            );
        }

        // `shaderInt64` vive en VkPhysicalDeviceFeatures (no en Vulkan12Features) y la declaran
        // los shaders de RT que usan `uint64_t` como puntero de geometría.
        let core_features = instance.get_physical_device_features(physical);
        if core_features.shader_int64 == vk::FALSE && hw_rt {
            log::warn!(
                "[vulkan] el adaptador no expone shaderInt64: se usará el BVH por software."
            );
            hw_rt = false;
        }
        let base_features = vk::PhysicalDeviceFeatures {
            sampler_anisotropy: vk::TRUE,
            shader_sampled_image_array_dynamic_indexing: vk::TRUE,
            fill_mode_non_solid: vk::TRUE,
            fragment_stores_and_atomics: vk::TRUE,
            shader_int64: core_features.shader_int64,
            // Las probes usan un `texture_cube_array` (6 caras por ranura) y el forward lo muestrea
            // con `samplerCubeArray`. Las dos cosas exigen `imageCubeArray`: sin la feature la vista
            // y el muestreo quedan indefinidos (reflejos duplicados / basura).
            image_cube_array: vk::TRUE,
            ..vk::PhysicalDeviceFeatures::default()
        };

        let queue_priorities = [1.0_f32];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&queue_priorities)];

        let mut device_exts = vec![ash::khr::swapchain::NAME.as_ptr()];
        if hw_rt {
            device_exts.push(ash::khr::acceleration_structure::NAME.as_ptr());
            device_exts.push(ash::khr::ray_tracing_pipeline::NAME.as_ptr());
            device_exts.push(ash::khr::deferred_host_operations::NAME.as_ptr());
        }

        let mut device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&device_exts)
            .enabled_features(&base_features)
            .push_next(&mut enabled_13)
            .push_next(&mut enabled_12);
        if hw_rt {
            device_info = device_info
                .push_next(&mut enabled_as)
                .push_next(&mut enabled_rt);
        }
        let device = instance
            .create_device(physical, &device_info, None)
            .map_err(|error| vk_error("crear el device Vulkan", error))?;
        let queue = device.get_device_queue(queue_family, 0);

        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: physical,
            debug_settings: Default::default(),
            buffer_device_address: true,
            allocation_sizes: Default::default(),
        })
        .map_err(|error| {
            VulkanError::new(format!(
                "No se pudo crear el allocator de memoria GPU: {error}"
            ))
        })?;

        let size = window.inner_size();
        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);
        let swapchain = create_swapchain(
            &device,
            &surface_loader,
            &swapchain_loader,
            physical,
            surface,
            size.width.max(1),
            size.height.max(1),
            false,
            vk::SwapchainKHR::null(),
        )?;

        let command_pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(queue_family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
            .map_err(|error| vk_error("crear el command pool", error))?;
        let frames = create_frames(&device, command_pool)?;

        let (accel, rt_pipeline, rt_props) = if hw_rt {
            let accel = ash::khr::acceleration_structure::Device::new(&instance, &device);
            let rt_pipeline = ash::khr::ray_tracing_pipeline::Device::new(&instance, &device);
            let mut rt = vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
            let mut as_props = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
            let mut props2 = vk::PhysicalDeviceProperties2::default()
                .push_next(&mut rt)
                .push_next(&mut as_props);
            instance.get_physical_device_properties2(physical, &mut props2);
            (
                Some(accel),
                Some(rt_pipeline),
                Some(RtProperties {
                    shader_group_handle_size: rt.shader_group_handle_size,
                    shader_group_handle_alignment: rt.shader_group_handle_alignment.max(1),
                    shader_group_base_alignment: rt.shader_group_base_alignment.max(1),
                    min_scratch_alignment: as_props
                        .min_acceleration_structure_scratch_offset_alignment
                        .max(1),
                }),
            )
        } else {
            (None, None, None)
        };

        Ok(Self {
            inner: Arc::new(GpuInner {
                entry,
                instance,
                surface_loader,
                surface,
                physical,
                device,
                queue: Mutex::new(queue),
                hw_rt,
                swapchain_loader,
                swapchain: Mutex::new(swapchain),
                allocator: ManuallyDrop::new(Mutex::new(allocator)),
                command_pool,
                frames: Mutex::new(frames),
                frame_slot: AtomicUsize::new(0),
                accel,
                rt_pipeline,
                rt_props,
                debug_utils,
                messenger,
                limits: props.limits,
            }),
        })
    }

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

    pub fn rt_properties(&self) -> Option<&RtProperties> {
        self.inner.rt_props.as_ref()
    }

    pub fn device(&self) -> &ash::Device {
        &self.inner.device
    }

    pub fn accel(&self) -> Option<&ash::khr::acceleration_structure::Device> {
        self.inner.accel.as_ref()
    }

    pub fn rt_pipeline(&self) -> Option<&ash::khr::ray_tracing_pipeline::Device> {
        self.inner.rt_pipeline.as_ref()
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.inner
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .extent
    }

    pub fn swapchain_format(&self) -> vk::Format {
        self.inner
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .format
    }

    pub fn resize(&self, width: u32, height: u32) -> Result<(), VulkanError> {
        self.recreate_swapchain(width, height, None)
    }

    pub fn set_vsync(&self, enabled: bool) -> Result<(), VulkanError> {
        let extent = self.extent();
        self.recreate_swapchain(extent.width, extent.height, Some(enabled))
    }

    fn recreate_swapchain(
        &self,
        width: u32,
        height: u32,
        vsync: Option<bool>,
    ) -> Result<(), VulkanError> {
        unsafe {
            self.inner
                .device
                .device_wait_idle()
                .map_err(|error| vk_error("esperar la GPU antes de recrear el swapchain", error))?;
        }
        let mut swapchain = self
            .inner
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let vsync = vsync.unwrap_or(swapchain.vsync);
        let old = swapchain.handle;
        let created = create_swapchain(
            &self.inner.device,
            &self.inner.surface_loader,
            &self.inner.swapchain_loader,
            self.inner.physical,
            self.inner.surface,
            width.max(1),
            height.max(1),
            vsync,
            old,
        )?;
        unsafe {
            for view in swapchain.views.drain(..) {
                self.inner.device.destroy_image_view(view, None);
            }
            self.inner.swapchain_loader.destroy_swapchain(old, None);
        }
        *swapchain = created;
        Ok(())
    }

    /// Buffer de vertices para trazado: posiciones ya poseadas de una malla skinned, leidas por el
    /// BLAS. Necesita `SHADER_DEVICE_ADDRESS` (el hit shader las carga por puntero) y
    /// `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`; esa combinacion vive aqui, en el modulo
    /// Vulkan, para que el resto del crate no maneje banderas de Ash.
    pub fn create_trace_vertex_buffer(
        &self,
        size: u64,
        name: &str,
    ) -> Result<GpuBuffer, VulkanError> {
        self.create_buffer(
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::CpuToGpu,
            name,
        )
    }

    pub fn create_buffer(
        &self,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &str,
    ) -> Result<GpuBuffer, VulkanError> {
        let size = size.max(4);
        let buffer = unsafe {
            self.inner.device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )
        }
        .map_err(|error| vk_error(&format!("crear el buffer {name}"), error))?;
        let requirements = unsafe { self.inner.device.get_buffer_memory_requirements(buffer) };
        let mut allocator = self
            .inner
            .allocator
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|error| {
                unsafe { self.inner.device.destroy_buffer(buffer, None) };
                VulkanError::new(format!("Sin memoria GPU para {name}: {error}"))
            })?;
        unsafe {
            self.inner
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|error| vk_error(&format!("asociar memoria a {name}"), error))?;
        }
        let address = if usage.contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS) {
            unsafe {
                self.inner.device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(buffer),
                )
            }
        } else {
            0
        };
        Ok(GpuBuffer {
            gpu: self.clone(),
            buffer,
            allocation: Some(allocation),
            size,
            address,
        })
    }

    pub fn create_buffer_with_data(
        &self,
        data: &[u8],
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &str,
    ) -> Result<GpuBuffer, VulkanError> {
        let mut buffer = self.create_buffer(data.len() as u64, usage, location, name)?;
        buffer.write(0, data)?;
        Ok(buffer)
    }

    pub fn create_image(
        &self,
        extent: vk::Extent2D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        name: &str,
    ) -> Result<GpuImage, VulkanError> {
        self.create_image_with_samples(
            extent,
            format,
            vk::SampleCountFlags::TYPE_1,
            usage,
            aspect,
            name,
        )
    }

    /// Igual que `create_image` pero con `samples > 1` (adjuntos del G-buffer multisampleado).
    pub fn create_image_with_samples(
        &self,
        extent: vk::Extent2D,
        format: vk::Format,
        samples: vk::SampleCountFlags,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        name: &str,
    ) -> Result<GpuImage, VulkanError> {
        let image = unsafe {
            self.inner.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: extent.width.max(1),
                        height: extent.height.max(1),
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(samples)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )
        }
        .map_err(|error| vk_error(&format!("crear la imagen {name}"), error))?;
        let requirements = unsafe { self.inner.device.get_image_memory_requirements(image) };
        let mut allocator = self
            .inner
            .allocator
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|error| {
                unsafe { self.inner.device.destroy_image(image, None) };
                VulkanError::new(format!("Sin memoria GPU para la imagen {name}: {error}"))
            })?;
        unsafe {
            self.inner
                .device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .map_err(|error| vk_error(&format!("asociar memoria a la imagen {name}"), error))?;
        }
        let view = unsafe {
            self.inner.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )
        }
        .map_err(|error| vk_error(&format!("crear la vista {name}"), error))?;
        let image = GpuImage {
            gpu: self.clone(),
            image,
            allocation: Some(allocation),
            view,
            format,
            extent,
            aspect,
            layers: 1,
            mips: 1,
            layer_views: Vec::new(),
        };
        self.transition_initial(&image)?;
        Ok(image)
    }

    pub fn create_image_layers(
        &self,
        extent: vk::Extent2D,
        layers: u32,
        mips: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        name: &str,
    ) -> Result<GpuImage, VulkanError> {
        let layers = layers.max(1);
        let mips = mips.max(1);
        let image = unsafe {
            self.inner.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: extent.width.max(1),
                        height: extent.height.max(1),
                        depth: 1,
                    })
                    .mip_levels(mips)
                    .array_layers(layers)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .flags(if layers >= 6 && layers.is_multiple_of(6) {
                        vk::ImageCreateFlags::CUBE_COMPATIBLE
                    } else {
                        vk::ImageCreateFlags::empty()
                    }),
                None,
            )
        }
        .map_err(|error| vk_error(&format!("crear la imagen {name}"), error))?;
        let requirements = unsafe { self.inner.device.get_image_memory_requirements(image) };
        let mut allocator = self
            .inner
            .allocator
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|error| {
                unsafe { self.inner.device.destroy_image(image, None) };
                VulkanError::new(format!("Sin memoria GPU para la imagen {name}: {error}"))
            })?;
        unsafe {
            self.inner
                .device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .map_err(|error| vk_error(&format!("asociar memoria a la imagen {name}"), error))?;
        }
        let view_type = if layers == 6 {
            vk::ImageViewType::CUBE
        } else if layers > 6 && layers.is_multiple_of(6) {
            vk::ImageViewType::CUBE_ARRAY
        } else if layers > 1 {
            vk::ImageViewType::TYPE_2D_ARRAY
        } else {
            vk::ImageViewType::TYPE_2D
        };
        let view = unsafe {
            self.inner.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(view_type)
                    .format(format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: aspect,
                        base_mip_level: 0,
                        level_count: mips,
                        base_array_layer: 0,
                        layer_count: layers,
                    }),
                None,
            )
        }
        .map_err(|error| vk_error(&format!("crear la vista {name}"), error))?;
        let image = GpuImage {
            gpu: self.clone(),
            image,
            allocation: Some(allocation),
            view,
            format,
            extent,
            aspect,
            layers,
            mips,
            layer_views: Vec::new(),
        };
        self.transition_initial(&image)?;
        Ok(image)
    }

    fn transition_initial(&self, image: &GpuImage) -> Result<(), VulkanError> {
        let image_handle = image.image;
        let aspect = image.aspect;
        let mips = image.mips;
        let layers = image.layers;
        let (dst, dst_stage, dst_access) = if aspect.contains(vk::ImageAspectFlags::DEPTH) {
            (
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            )
        } else {
            (
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
            )
        };
        self.one_shot(move |device, command| {
            cmd_image_barrier(
                device,
                command,
                image_handle,
                aspect,
                mips,
                layers,
                vk::ImageLayout::UNDEFINED,
                dst,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                dst_stage,
                vk::AccessFlags::empty(),
                dst_access,
            );
        })
    }

    pub fn upload_rgba_layer(
        &self,
        image: &GpuImage,
        mip: u32,
        layer: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), VulkanError> {
        let staging = self.create_buffer_with_data(
            rgba,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "tex-staging",
        )?;
        let image_handle = image.image;
        let aspect = image.aspect;
        let mips = image.mips;
        let layers = image.layers;
        self.one_shot(move |device, command| {
            cmd_image_barrier(
                device,
                command,
                image_handle,
                aspect,
                mips,
                layers,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::TRANSFER_WRITE,
            );
            let region = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: aspect,
                    mip_level: mip,
                    base_array_layer: layer,
                    layer_count: 1,
                })
                .image_extent(vk::Extent3D {
                    width: width.max(1),
                    height: height.max(1),
                    depth: 1,
                });
            unsafe {
                device.cmd_copy_buffer_to_image(
                    command,
                    staging.buffer,
                    image_handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
            }
            cmd_image_barrier(
                device,
                command,
                image_handle,
                aspect,
                mips,
                layers,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
            );
        })?;
        Ok(())
    }

    /// Sube un subrectángulo RGBA a una imagen (atlas HUD).
    ///
    /// Las imágenes del motor se mantienen en `GENERAL`; el barrier de ida y vuelta
    /// deja la textura lista para muestreo tras la copia.
    pub fn upload_rgba_region(
        &self,
        image: &GpuImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), VulkanError> {
        let staging = self.create_buffer_with_data(
            rgba,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "hud-atlas-staging",
        )?;
        let image_handle = image.image;
        let aspect = image.aspect;
        let mips = image.mips;
        let layers = image.layers;
        self.one_shot(move |device, command| {
            cmd_image_barrier(
                device,
                command,
                image_handle,
                aspect,
                mips,
                layers,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::TRANSFER_WRITE,
            );
            let region = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: aspect,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D {
                    x: x as i32,
                    y: y as i32,
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width: width.max(1),
                    height: height.max(1),
                    depth: 1,
                });
            unsafe {
                device.cmd_copy_buffer_to_image(
                    command,
                    staging.buffer,
                    image_handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
            }
            cmd_image_barrier(
                device,
                command,
                image_handle,
                aspect,
                mips,
                layers,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
            );
        })?;
        Ok(())
    }

    /// Graba, envía y espera un command buffer de un solo uso (uploads y builds).
    pub fn one_shot(
        &self,
        record: impl FnOnce(&ash::Device, vk::CommandBuffer),
    ) -> Result<(), VulkanError> {
        let command = unsafe {
            self.inner.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(self.inner.command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .map_err(|error| vk_error("reservar un command buffer", error))?[0];
        unsafe {
            self.inner
                .device
                .begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(|error| vk_error("abrir un command buffer", error))?;
        }
        record(&self.inner.device, command);
        unsafe {
            self.inner
                .device
                .end_command_buffer(command)
                .map_err(|error| vk_error("cerrar un command buffer", error))?;
        }
        let fence = unsafe {
            self.inner
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .map_err(|error| vk_error("crear un fence de upload", error))?
        };
        let buffers = [command];
        let submit = vk::SubmitInfo::default().command_buffers(&buffers);
        {
            let queue = self
                .inner
                .queue
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            unsafe {
                self.inner
                    .device
                    .queue_submit(*queue, &[submit], fence)
                    .map_err(|error| vk_error("enviar un upload a la cola", error))?;
                self.inner
                    .device
                    .wait_for_fences(&[fence], true, u64::MAX)
                    .map_err(|error| vk_error("esperar un upload", error))?;
            }
        }
        unsafe {
            self.inner.device.destroy_fence(fence, None);
            self.inner
                .device
                .free_command_buffers(self.inner.command_pool, &[command]);
        }
        Ok(())
    }

    pub fn begin_frame(&self) -> Result<FrameTarget, FrameAcquire> {
        let slot = self.inner.frame_slot.fetch_add(1, Ordering::Relaxed) % FRAMES_IN_FLIGHT;
        let mut frames = self
            .inner
            .frames
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let frame = &mut frames[slot];
        unsafe {
            if serialise_frames() {
                // Diagnóstico: se espera a que la GPU quede ociosa antes de grabar. Así el
                // frame anterior no puede estar usando los targets/descriptores compartidos
                // que este frame reescribe. Con `RER_VK_SYNC_FRAMES=0` se desactiva.
                self.inner.device.device_wait_idle().map_err(|error| {
                    FrameAcquire::Fail(vk_error("esperar a la GPU (modo serializado)", error))
                })?;
            }
            self.inner
                .device
                .wait_for_fences(&[frame.fence], true, u64::MAX)
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("esperar el frame anterior", error))
                })?;
            self.inner
                .device
                .reset_fences(&[frame.fence])
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("reiniciar el fence del frame", error))
                })?;
        }
        let swapchain = self
            .inner
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if swapchain.extent.width == 0 || swapchain.extent.height == 0 {
            return Err(FrameAcquire::Skip);
        }
        let acquired = unsafe {
            self.inner.swapchain_loader.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                frame.acquire,
                vk::Fence::null(),
            )
        };
        let image_index = match acquired {
            Ok((index, _suboptimal)) => index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Err(FrameAcquire::Outdated),
            Err(vk::Result::NOT_READY) | Err(vk::Result::TIMEOUT) => {
                return Err(FrameAcquire::Skip);
            }
            Err(error) => {
                return Err(FrameAcquire::Fail(vk_error(
                    "adquirir la imagen del swapchain",
                    error,
                )));
            }
        };
        unsafe {
            self.inner
                .device
                .reset_command_buffer(frame.command, vk::CommandBufferResetFlags::empty())
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("reiniciar el command buffer", error))
                })?;
            self.inner
                .device
                .begin_command_buffer(frame.command, &vk::CommandBufferBeginInfo::default())
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("abrir el command buffer del frame", error))
                })?;
        }
        Ok(FrameTarget {
            slot,
            image_index,
            command: frame.command,
            color_view: swapchain.views[image_index as usize],
            color_image: swapchain.images[image_index as usize],
            extent: swapchain.extent,
            format: swapchain.format,
        })
    }

    pub fn submit_and_present(&self, frame: &FrameTarget) -> Result<(), FrameAcquire> {
        unsafe {
            self.inner
                .device
                .end_command_buffer(frame.command)
                .map_err(|error| {
                    FrameAcquire::Fail(vk_error("cerrar el command buffer del frame", error))
                })?;
        }
        let frames = self
            .inner
            .frames
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let sync = &frames[frame.slot];
        let wait_semaphores = [sync.acquire];
        let signal_semaphores = [sync.render_finished];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = [frame.command];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        let queue = self
            .inner
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        unsafe {
            self.inner
                .device
                .queue_submit(*queue, &[submit], sync.fence)
                .map_err(|error| FrameAcquire::Fail(vk_error("enviar el frame", error)))?;
        }
        let swapchain = self
            .inner
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let indices = [frame.image_index];
        let swapchains = [swapchain.handle];
        let present = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&indices);
        let presented = unsafe { self.inner.swapchain_loader.queue_present(*queue, &present) };
        // En modo serializado la cola se suelta ANTES de esperar: así ninguna otra ruta (uploads
        // `one_shot`) se bloquea contra el wait. Esperar aquí, y no solo en `begin_frame`, es lo
        // que protege los recursos: el `Drop` de un buffer/imagen de un frame ya enviado todavía
        // estaría en vuelo si nos quedáramos con el wait sólo al principio del siguiente frame
        // (la validación lo reportaba como `vkDestroyBuffer(): can't be called on VkBuffer ...
        // currently in use by VkCommandBuffer`).
        drop(queue);
        let result = match presented {
            Ok(false) => Ok(()),
            Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Err(FrameAcquire::Outdated),
            Err(vk::Result::ERROR_SURFACE_LOST_KHR) => Err(FrameAcquire::Outdated),
            Err(error) => Err(FrameAcquire::Fail(vk_error("presentar el frame", error))),
        };
        if serialise_frames() {
            unsafe {
                self.inner.device.device_wait_idle().map_err(|error| {
                    FrameAcquire::Fail(vk_error("esperar a la GPU (presentado)", error))
                })?;
            }
        }
        result
    }
}

pub enum FrameAcquire {
    Skip,
    Outdated,
    Fail(VulkanError),
}

impl GpuBuffer {
    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), VulkanError> {
        let Some(allocation) = self.allocation.as_mut() else {
            return Err(VulkanError::new(
                "El buffer no tiene memoria mapeable.".to_string(),
            ));
        };
        let Some(mapped) = allocation.mapped_slice_mut() else {
            return Err(VulkanError::new(
                "El buffer no está en memoria visible para la CPU.".to_string(),
            ));
        };
        let end = offset + data.len();
        if end > mapped.len() {
            return Err(VulkanError::new(format!(
                "Escritura fuera del buffer ({end} > {}).",
                mapped.len()
            )));
        }
        mapped[offset..end].copy_from_slice(data);
        if !allocation
            .memory_properties()
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        {
            let range = vk::MappedMemoryRange::default()
                .memory(unsafe { allocation.memory() })
                .offset(allocation.offset())
                .size(allocation.size());
            unsafe {
                self.gpu
                    .inner
                    .device
                    .flush_mapped_memory_ranges(&[range])
                    .map_err(|error| vk_error("vaciar la memoria mapeada", error))?;
            }
        }
        Ok(())
    }
}

pub(crate) fn cmd_image_barrier(
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

fn feature_query() -> (
    vk::PhysicalDeviceAccelerationStructureFeaturesKHR<'static>,
    vk::PhysicalDeviceRayTracingPipelineFeaturesKHR<'static>,
    vk::PhysicalDeviceVulkan12Features<'static>,
    vk::PhysicalDeviceVulkan13Features<'static>,
) {
    (
        vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default(),
        vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default(),
        vk::PhysicalDeviceVulkan12Features::default(),
        vk::PhysicalDeviceVulkan13Features::default(),
    )
}

fn extension_name_list(props: &[vk::ExtensionProperties]) -> Vec<String> {
    props
        .iter()
        .map(|prop| cstr_lossy(prop.extension_name.as_ptr()))
        .collect()
}

fn cstr_lossy(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

unsafe fn pick_physical_device(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<vk::PhysicalDevice, VulkanError> {
    let devices = instance
        .enumerate_physical_devices()
        .map_err(|error| vk_error("enumerar adaptadores Vulkan", error))?;
    let mut best: Option<(u32, vk::PhysicalDevice)> = None;
    for device in devices {
        let props = instance.get_physical_device_properties(device);
        if vk::api_version_major(props.api_version) < 1
            || (vk::api_version_major(props.api_version) == 1
                && vk::api_version_minor(props.api_version) < 3)
        {
            continue;
        }
        let extensions = instance
            .enumerate_device_extension_properties(device)
            .map_err(|error| vk_error("enumerar extensiones de un adaptador", error))?;
        let names = extension_name_list(&extensions);
        if !names.iter().any(|name| name == "VK_KHR_swapchain") {
            continue;
        }
        if pick_queue_family(instance, surface_loader, device, surface).is_err() {
            continue;
        }
        let score = match props.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 3,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
            _ => 0,
        };
        if best
            .map(|(best_score, _)| score > best_score)
            .unwrap_or(true)
        {
            best = Some((score, device));
        }
    }
    best.map(|(_, device)| device).ok_or_else(|| {
        VulkanError::new(
            "No hay un adaptador Vulkan 1.3 con swapchain. Actualiza los controladores de la GPU."
                .to_string(),
        )
    })
}

unsafe fn pick_queue_family(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    physical: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Result<u32, VulkanError> {
    let families = instance.get_physical_device_queue_family_properties(physical);
    for (index, family) in families.iter().enumerate() {
        let graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
        let present = surface_loader
            .get_physical_device_surface_support(physical, index as u32, surface)
            .unwrap_or(false);
        if graphics && present {
            return Ok(index as u32);
        }
    }
    Err(VulkanError::new(
        "Ninguna familia de colas puede dibujar y presentar en la ventana.".to_string(),
    ))
}

fn create_swapchain(
    device: &ash::Device,
    surface_loader: &ash::khr::surface::Instance,
    swapchain_loader: &ash::khr::swapchain::Device,
    physical: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    width: u32,
    height: u32,
    vsync: bool,
    old: vk::SwapchainKHR,
) -> Result<SwapchainState, VulkanError> {
    // FFI: consulta de surface y creación del swapchain. El estado vive en GpuInner.
    unsafe {
        let caps = surface_loader
            .get_physical_device_surface_capabilities(physical, surface)
            .map_err(|error| vk_error("consultar las capacidades de la superficie", error))?;
        let formats = surface_loader
            .get_physical_device_surface_formats(physical, surface)
            .map_err(|error| vk_error("consultar los formatos de la superficie", error))?;
        let modes = surface_loader
            .get_physical_device_surface_present_modes(physical, surface)
            .map_err(|error| vk_error("consultar los modos de presentación", error))?;
        let format = pick_surface_format(&formats).ok_or_else(|| {
            VulkanError::new("La superficie no ofrece ningún formato de color.".to_string())
        })?;
        let present = pick_present_mode(&modes, vsync);
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        let mut image_count = caps.min_image_count + 1;
        if caps.max_image_count > 0 && image_count > caps.max_image_count {
            image_count = caps.max_image_count;
        }
        let mut usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST;
        if !caps
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_DST)
        {
            usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
        }
        let alpha = if caps
            .supported_composite_alpha
            .contains(vk::CompositeAlphaFlagsKHR::OPAQUE)
        {
            vk::CompositeAlphaFlagsKHR::OPAQUE
        } else {
            vk::CompositeAlphaFlagsKHR::INHERIT
        };
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(present)
            .clipped(true)
            .old_swapchain(old);
        let handle = swapchain_loader
            .create_swapchain(&info, None)
            .map_err(|error| vk_error("crear el swapchain", error))?;
        let images = swapchain_loader
            .get_swapchain_images(handle)
            .map_err(|error| vk_error("obtener las imágenes del swapchain", error))?;
        let mut views = Vec::with_capacity(images.len());
        for image in &images {
            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(*image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                    None,
                )
                .map_err(|error| vk_error("crear la vista del swapchain", error))?;
            views.push(view);
        }
        Ok(SwapchainState {
            handle,
            format: format.format,
            extent,
            views,
            images,
            vsync,
        })
    }
}

unsafe fn create_frames(
    device: &ash::Device,
    pool: vk::CommandPool,
) -> Result<[FrameSync; FRAMES_IN_FLIGHT], VulkanError> {
    let buffers = device
        .allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(FRAMES_IN_FLIGHT as u32),
        )
        .map_err(|error| vk_error("reservar los command buffers del frame", error))?;
    let mut frames = Vec::with_capacity(FRAMES_IN_FLIGHT);
    for command in buffers {
        let acquire = device
            .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            .map_err(|error| vk_error("crear el semáforo de adquisición", error))?;
        let render_finished = device
            .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            .map_err(|error| vk_error("crear el semáforo de presentación", error))?;
        let fence = device
            .create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )
            .map_err(|error| vk_error("crear el fence del frame", error))?;
        frames.push(FrameSync {
            acquire,
            render_finished,
            fence,
            command,
        });
    }
    frames
        .try_into()
        .map_err(|_| VulkanError::new("No se pudieron crear los frames en vuelo.".to_string()))
}

/// Mayor número de muestras ≤ `desired` presente en `supported`.
///
/// Vulkan reporta el soporte de MSAA como banderas (`framebufferColorSampleCounts`,
/// `framebufferDepthSampleCounts`) con los bits 1/2/4/8/16/32/64; no existe un límite
/// `maxImageSampleCount` que consultar, así que estas banderas son la única fuente. MSAA solo
/// admite potencias de dos, por eso se recorre la tabla de menor a mayor y se conserva el último
/// valor soportado. Si no hay ninguno se devuelve 1 (sin multisampling) en lugar de un
/// `sample_count` que invalide el pipeline.
fn pick_sample_count(supported: vk::SampleCountFlags, desired: u32) -> u32 {
    const CANDIDATES: [(u32, vk::SampleCountFlags); 6] = [
        (2, vk::SampleCountFlags::TYPE_2),
        (4, vk::SampleCountFlags::TYPE_4),
        (8, vk::SampleCountFlags::TYPE_8),
        (16, vk::SampleCountFlags::TYPE_16),
        (32, vk::SampleCountFlags::TYPE_32),
        (64, vk::SampleCountFlags::TYPE_64),
    ];
    let mut best = 1;
    for (count, flag) in CANDIDATES {
        if count > desired {
            break;
        }
        if supported.contains(flag) {
            best = count;
        }
    }
    best
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
