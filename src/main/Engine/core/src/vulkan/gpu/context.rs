use super::frame::create_frames;
use super::swapchain::create_swapchain;
use super::*;

/// Validación: en builds de debug se activa por defecto (los artefactos visuales se diagnostican
/// con el validador, no a ciegas). Se desactiva con `RER_VK_VALIDATION=0` y activa en release con `=1`.
pub(super) fn validation_requested() -> bool {
    match std::env::var("RER_VK_VALIDATION") {
        Ok(value) => !value.is_empty() && value != "0",
        Err(_) => cfg!(debug_assertions),
    }
}

/// Modo serializado para diagnosticar contenido de targets compartidos (`RER_VK_SYNC_FRAMES=1`).
pub(super) fn serialise_frames() -> bool {
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

pub(super) fn feature_query() -> (
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

pub(super) fn extension_name_list(props: &[vk::ExtensionProperties]) -> Vec<String> {
    props
        .iter()
        .map(|prop| cstr_lossy(prop.extension_name.as_ptr()))
        .collect()
}

pub(super) fn cstr_lossy(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

pub(super) unsafe fn pick_physical_device(
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

pub(super) unsafe fn pick_queue_family(
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

impl SharedGpu {
    pub fn new(window: &Window, options: &VulkanOptions) -> Result<Self, VulkanError> {
        let gpu = Self::create(window, options)?;
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
            if !options.ray_tracing {
                "no solicitado"
            } else if gpu.hw_rt() {
                "disponible"
            } else {
                "no disponible (BVH por software)"
            }
        );
        Ok(gpu)
    }

    pub(super) fn create(window: &Window, options: &VulkanOptions) -> Result<Self, VulkanError> {
        // FFI: cargar vulkan-1.dll y crear instance/device. El Drop de GpuInner los destruye.
        unsafe { Self::create_unchecked(window, options) }
    }

    pub(super) unsafe fn create_unchecked(
        window: &Window,
        options: &VulkanOptions,
    ) -> Result<Self, VulkanError> {
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
            .application_name(c"rer_engine")
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
        // Sin `ray_tracing` no se pregunta ni se avisa: el 2D no usa trazado y un aviso suyo
        // sería ruido ("se usará el BVH por software" sin que nadie lo haya pedido).
        let mut hw_rt = options.ray_tracing && hw_rt_supported(&available);
        if options.ray_tracing && !missing.is_empty() {
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
        if options.ray_tracing && queried_12.buffer_device_address == vk::FALSE {
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
            // Solo el motor que traza necesita direcciones de dispositivo; activarla sin soporte
            // haría fallar `create_device`, así que sigue a la opción del motor.
            .buffer_device_address(options.ray_tracing)
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
            // Las probes del 3D usan un `texture_cube_array` (6 caras por ranura) y el forward lo
            // muestrea con `samplerCubeArray`: sin la feature la vista y el muestreo quedan
            // indefinidos (reflejos duplicados / basura). El 2D no la pide.
            image_cube_array: if options.image_cube_array {
                vk::TRUE
            } else {
                vk::FALSE
            },
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
            buffer_device_address: options.ray_tracing,
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
        let one_shot_pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(queue_family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )
            .map_err(|error| vk_error("crear el command pool de un solo uso", error))?;

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
                queue_family,
                hw_rt,
                swapchain_loader,
                swapchain: Mutex::new(swapchain),
                allocator: ManuallyDrop::new(Mutex::new(allocator)),
                command_pool,
                one_shot_pool,
                one_shot_lock: Mutex::new(()),
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
}
