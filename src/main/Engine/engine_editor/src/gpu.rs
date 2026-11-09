//! Contexto Vulkan/ash exclusivo del editor y renderer de egui.
//!
//! El editor ya no usa wgpu: comparte la misma API que el motor 3D (ash) para
//! que en el futuro ambos puedan usar el mismo `VkDevice`, swapchain y command
//! buffer.
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::CStr;

use ash::{Device, Entry, Instance, khr, vk};
use egui_ash_renderer::{Options, RenderMode, Renderer, allocator::DefaultAllocator};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::dpi::PhysicalSize;
use winit::window::Window;

const MAX_FRAMES_IN_FLIGHT: usize = 2;

pub struct Gpu {
    _entry: Entry,
    instance: Instance,
    surface_loader: khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    device: Device,
    queue: vk::Queue,
    swapchain_loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    swapchain_views: Vec<vk::ImageView>,
    surface_format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,
    current_frame: usize,
    clear: vk::ClearValue,
    egui_renderer: Option<Renderer<DefaultAllocator>>,
}

impl Gpu {
    pub fn new(window: &Window) -> Result<Self, String> {
        // FFI: creación explícita de instance/device/surface/swapchain.
        unsafe { Self::create_unchecked(window) }
    }

    unsafe fn create_unchecked(window: &Window) -> Result<Self, String> {
        let entry = Entry::load().map_err(|error| {
            format!("No se pudo cargar vulkan-1.dll ({error}). Instala los controladores Vulkan.")
        })?;

        let display = window
            .display_handle()
            .map_err(|error| format!("La ventana no expone display handle: {error}"))?
            .as_raw();
        let window_handle = window
            .window_handle()
            .map_err(|error| format!("La ventana no expone window handle: {error}"))?
            .as_raw();
        let instance_extensions = ash_window::enumerate_required_extensions(display)
            .map_err(|error| format!("No se pudieron enumerar extensiones Vulkan: {error:?}"))?;

        let app = vk::ApplicationInfo::default()
            .application_name(c"rer_engine_editor")
            .engine_name(c"rer-engine")
            .api_version(vk::API_VERSION_1_3);
        let instance_info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(instance_extensions);
        let instance = entry
            .create_instance(&instance_info, None)
            .map_err(|error| vk_err("crear la instancia Vulkan", error))?;

        let surface = ash_window::create_surface(&entry, &instance, display, window_handle, None)
            .map_err(|error| vk_err("crear la superficie", error))?;
        let surface_loader = khr::surface::Instance::new(&entry, &instance);

        let physical_device = pick_physical_device(&instance, &surface_loader, surface)?;
        let props = instance.get_physical_device_properties(physical_device);
        log::info!(
            "[editor] GPU: Vulkan/ash — {}",
            CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy()
        );

        let queue_family_index =
            pick_queue_family(&instance, &surface_loader, physical_device, surface)?;
        let priorities = [1.0f32];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities)];
        let device_extensions = [khr::swapchain::NAME.as_ptr()];
        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&device_extensions);
        let device = instance
            .create_device(physical_device, &device_info, None)
            .map_err(|error| vk_err("crear el device", error))?;
        let queue = device.get_device_queue(queue_family_index, 0);
        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);

        let (surface_format, extent) = swapchain_config(
            &surface_loader,
            physical_device,
            surface,
            window.inner_size(),
        )?;
        let swapchain = create_swapchain(
            &swapchain_loader,
            surface,
            surface_format,
            extent,
            vk::SwapchainKHR::null(),
        )?;
        let swapchain_images = swapchain_loader
            .get_swapchain_images(swapchain)
            .map_err(|error| vk_err("obtener imágenes de la swapchain", error))?;
        let swapchain_views =
            create_image_views(&device, &swapchain_images, surface_format.format)?;
        let render_pass = create_render_pass(&device, surface_format.format)?;
        let framebuffers = create_framebuffers(&device, render_pass, &swapchain_views, extent)?;

        let command_pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = device
            .create_command_pool(&command_pool_info, None)
            .map_err(|error| vk_err("crear el command pool", error))?;
        let command_buffers = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(MAX_FRAMES_IN_FLIGHT as u32),
            )
            .map_err(|error| vk_err("crear command buffers", error))?;

        let mut image_available = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut in_flight_fences = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            image_available.push(
                device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(|error| vk_err("crear semáforo", error))?,
            );
            in_flight_fences.push(
                device
                    .create_fence(
                        &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                        None,
                    )
                    .map_err(|error| vk_err("crear fence", error))?,
            );
        }
        let mut render_finished = Vec::with_capacity(swapchain_images.len());
        for _ in &swapchain_images {
            render_finished.push(
                device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(|error| vk_err("crear semáforo de present", error))?,
            );
        }

        let egui_renderer = Renderer::with_default_allocator(
            &instance,
            physical_device,
            device.clone(),
            RenderMode::RenderPass(render_pass),
            Options {
                in_flight_frames: MAX_FRAMES_IN_FLIGHT,
                enable_depth_test: false,
                enable_depth_write: false,
                srgb_framebuffer: is_srgb_format(surface_format.format),
            },
        )
        .map_err(|error| format!("No se pudo crear el renderer egui: {error}"))?;

        Ok(Self {
            _entry: entry,
            instance,
            surface_loader,
            surface,
            physical_device,
            device,
            queue,
            swapchain_loader,
            swapchain,
            swapchain_images,
            swapchain_views,
            surface_format,
            extent,
            render_pass,
            framebuffers,
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight_fences,
            current_frame: 0,
            clear: vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: crate::theme::clear_color(),
                },
            },
            egui_renderer: Some(egui_renderer),
        })
    }

    pub fn max_texture_dimension_2d(&self) -> u32 {
        unsafe {
            self.instance
                .get_physical_device_properties(self.physical_device)
                .limits
                .max_image_dimension2_d
        }
    }

    /// Dibuja un frame completo de egui y lo presenta.
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

        if !full_output.textures_delta.set.is_empty() {
            self.egui_renderer
                .as_mut()
                .ok_or_else(|| "El renderer egui no está inicializado".to_string())?
                .set_textures(
                    self.queue,
                    self.command_pool,
                    &full_output.textures_delta.set,
                )
                .map_err(|error| format!("No se pudieron subir texturas egui: {error}"))?;
        }

        let frame = self.current_frame;
        let fence = self.in_flight_fences[frame];
        unsafe {
            self.device
                .wait_for_fences(&[fence], true, u64::MAX)
                .map_err(|error| vk_err("esperar fence", error))?;
        }

        let image_index = match unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available[frame],
                vk::Fence::null(),
            )
        } {
            Ok((index, _)) => index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate_swapchain(window.inner_size())?;
                return Ok(true);
            }
            Err(error) => return Err(vk_err("adquirir imagen de swapchain", error)),
        };

        let command = self.command_buffers[frame];
        unsafe {
            self.device
                .reset_fences(&[fence])
                .map_err(|error| vk_err("resetear fence", error))?;
            self.device
                .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                .map_err(|error| vk_err("resetear command buffer", error))?;
            self.device
                .begin_command_buffer(
                    command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(|error| vk_err("abrir command buffer", error))?;

            let clear_values = [self.clear];
            self.device.cmd_begin_render_pass(
                command,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass)
                    .framebuffer(self.framebuffers[image_index as usize])
                    .render_area(self.extent.into())
                    .clear_values(&clear_values),
                vk::SubpassContents::INLINE,
            );
        }

        self.egui_renderer
            .as_mut()
            .ok_or_else(|| "El renderer egui no está inicializado".to_string())?
            .cmd_draw(
                command,
                self.extent,
                full_output.pixels_per_point,
                &primitives,
            )
            .map_err(|error| format!("No se pudo grabar egui: {error}"))?;

        unsafe {
            self.device.cmd_end_render_pass(command);
            self.device
                .end_command_buffer(command)
                .map_err(|error| vk_err("cerrar command buffer", error))?;

            let wait_semaphores = [self.image_available[frame]];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let signal_semaphores = [self.render_finished[image_index as usize]];
            let command_buffers = [command];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            self.device
                .queue_submit(self.queue, &[submit], fence)
                .map_err(|error| vk_err("enviar frame", error))?;

            let swapchains = [self.swapchain];
            let image_indices = [image_index];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            match self.swapchain_loader.queue_present(self.queue, &present) {
                Ok(false) => {}
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.recreate_swapchain(window.inner_size())?;
                }
                Err(error) => return Err(vk_err("presentar frame", error)),
            }
        }

        self.egui_renderer
            .as_mut()
            .ok_or_else(|| "El renderer egui no está inicializado".to_string())?
            .free_textures(&full_output.textures_delta.free)
            .map_err(|error| format!("No se pudieron liberar texturas egui: {error}"))?;
        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(repaint_now)
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        self.recreate_swapchain(size)
    }

    fn recreate_swapchain(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        unsafe {
            self.device
                .device_wait_idle()
                .map_err(|error| vk_err("esperar device idle", error))?;
            for framebuffer in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            for view in self.swapchain_views.drain(..) {
                self.device.destroy_image_view(view, None);
            }
        }

        let (surface_format, extent) = swapchain_config(
            &self.surface_loader,
            self.physical_device,
            self.surface,
            size,
        )?;
        let old_swapchain = self.swapchain;
        self.swapchain = unsafe {
            create_swapchain(
                &self.swapchain_loader,
                self.surface,
                surface_format,
                extent,
                old_swapchain,
            )
        }?;
        unsafe {
            self.swapchain_loader.destroy_swapchain(old_swapchain, None);
        }
        self.swapchain_images = unsafe {
            self.swapchain_loader
                .get_swapchain_images(self.swapchain)
                .map_err(|error| vk_err("obtener imágenes de la swapchain", error))?
        };
        self.swapchain_views = unsafe {
            create_image_views(&self.device, &self.swapchain_images, surface_format.format)
        }?;

        if surface_format.format != self.surface_format.format {
            if is_srgb_format(surface_format.format) != is_srgb_format(self.surface_format.format) {
                // Él renderer fija `srgb_framebuffer` al crearse; cambiarlo
                // requeriría volver a subir las texturas de egui.
                log::warn!(
                    "[editor] el swapchain cambió de espacio de color ({:?} → {:?}); reinicia el editor si los colores se ven mal",
                    self.surface_format.format,
                    surface_format.format
                );
            }
            let old_render_pass = self.render_pass;
            self.render_pass = unsafe { create_render_pass(&self.device, surface_format.format)? };
            if let Some(renderer) = self.egui_renderer.as_mut() {
                renderer
                    .set_render_mode(RenderMode::RenderPass(self.render_pass))
                    .map_err(|error| format!("No se pudo actualizar el renderer egui: {error}"))?;
            }
            unsafe {
                self.device.destroy_render_pass(old_render_pass, None);
            }
        }
        self.surface_format = surface_format;
        self.extent = extent;
        self.framebuffers = unsafe {
            create_framebuffers(
                &self.device,
                self.render_pass,
                &self.swapchain_views,
                extent,
            )
        }?;

        unsafe {
            for semaphore in self.render_finished.drain(..) {
                self.device.destroy_semaphore(semaphore, None);
            }
        }
        for _ in &self.swapchain_images {
            self.render_finished.push(unsafe {
                self.device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(|error| vk_err("crear semáforo de present", error))?
            });
        }
        Ok(())
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            // El renderer usa un clon del device: destruirlo antes del device.
            self.egui_renderer.take();
            for fence in self.in_flight_fences.drain(..) {
                self.device.destroy_fence(fence, None);
            }
            for semaphore in self.render_finished.drain(..) {
                self.device.destroy_semaphore(semaphore, None);
            }
            for semaphore in self.image_available.drain(..) {
                self.device.destroy_semaphore(semaphore, None);
            }
            self.device.destroy_command_pool(self.command_pool, None);
            for framebuffer in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            self.device.destroy_render_pass(self.render_pass, None);
            for view in self.swapchain_views.drain(..) {
                self.device.destroy_image_view(view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

fn swapchain_config(
    surface_loader: &khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    size: PhysicalSize<u32>,
) -> Result<(vk::SurfaceFormatKHR, vk::Extent2D), String> {
    let capabilities = unsafe {
        surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .map_err(|error| vk_err("consultar capacidades de swapchain", error))?
    };
    let formats = unsafe {
        surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .map_err(|error| vk_err("consultar formatos de swapchain", error))?
    };
    if formats.is_empty() {
        return Err("La swapchain no expone formatos de superficie".to_string());
    }
    let surface_format = formats
        .iter()
        .copied()
        .find(|format| format.format == vk::Format::B8G8R8A8_SRGB)
        .or_else(|| {
            formats
                .iter()
                .copied()
                .find(|format| format.format == vk::Format::R8G8B8A8_SRGB)
        })
        .or_else(|| {
            formats
                .iter()
                .copied()
                .find(|format| is_srgb_format(format.format))
        })
        .or_else(|| formats.first().copied())
        .ok_or_else(|| "No hay un formato de swapchain válido".to_string())?;

    let extent = if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        vk::Extent2D {
            width: size.width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: size.height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    };
    Ok((surface_format, extent))
}

unsafe fn create_swapchain(
    loader: &khr::swapchain::Device,
    surface: vk::SurfaceKHR,
    surface_format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
    old_swapchain: vk::SwapchainKHR,
) -> Result<vk::SwapchainKHR, String> {
    let info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(3)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(vk::PresentModeKHR::FIFO)
        .clipped(true)
        .old_swapchain(old_swapchain);
    loader
        .create_swapchain(&info, None)
        .map_err(|error| vk_err("crear swapchain", error))
}

unsafe fn create_image_views(
    device: &Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>, String> {
    images
        .iter()
        .map(|image| {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(*image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(1)
                                .base_array_layer(0)
                                .layer_count(1),
                        ),
                    None,
                )
                .map_err(|error| vk_err("crear image view", error))
        })
        .collect()
}

unsafe fn create_render_pass(
    device: &Device,
    format: vk::Format,
) -> Result<vk::RenderPass, String> {
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
    let color_reference = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_reference)];
    let dependencies = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
    device
        .create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies),
            None,
        )
        .map_err(|error| vk_err("crear render pass", error))
}

unsafe fn create_framebuffers(
    device: &Device,
    render_pass: vk::RenderPass,
    views: &[vk::ImageView],
    extent: vk::Extent2D,
) -> Result<Vec<vk::Framebuffer>, String> {
    views
        .iter()
        .map(|view| {
            let attachments = [*view];
            device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(render_pass)
                        .attachments(&attachments)
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1),
                    None,
                )
                .map_err(|error| vk_err("crear framebuffer", error))
        })
        .collect()
}

unsafe fn pick_physical_device(
    instance: &Instance,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<vk::PhysicalDevice, String> {
    let devices = instance
        .enumerate_physical_devices()
        .map_err(|error| vk_err("enumerar GPUs", error))?;
    let mut best = None;
    let mut best_score = -1;
    for device in devices {
        let Ok(extensions) = instance.enumerate_device_extension_properties(device) else {
            continue;
        };
        let has_swapchain = extensions.iter().any(|extension| {
            CStr::from_ptr(extension.extension_name.as_ptr()) == khr::swapchain::NAME
        });
        if !has_swapchain {
            continue;
        }
        let Ok(queue_family) = pick_queue_family(instance, surface_loader, device, surface) else {
            continue;
        };
        let _ = queue_family;
        let score = match instance.get_physical_device_properties(device).device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 3,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
            _ => 0,
        };
        if score > best_score {
            best_score = score;
            best = Some(device);
        }
    }
    best.ok_or_else(|| "No se encontró una GPU con soporte Vulkan/swapchain".to_string())
}

unsafe fn pick_queue_family(
    instance: &Instance,
    surface_loader: &khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Result<u32, String> {
    let families = instance
        .get_physical_device_queue_family_properties(physical_device)
        .into_iter()
        .enumerate()
        .find_map(|(index, family)| {
            let graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let present = surface_loader
                .get_physical_device_surface_support(physical_device, index as u32, surface)
                .unwrap_or(false);
            (graphics && present).then_some(index as u32)
        });
    families.ok_or_else(|| "No hay una cola gráfica/present compatible".to_string())
}

fn vk_err(context: &str, error: vk::Result) -> String {
    format!("{context}: {error:?}")
}

/// Formatos sRGB que el swapchain del editor acepta, igual que el motor 3D.
fn is_srgb_format(format: vk::Format) -> bool {
    matches!(
        format,
        vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB | vk::Format::A8B8G8R8_SRGB_PACK32
    )
}
