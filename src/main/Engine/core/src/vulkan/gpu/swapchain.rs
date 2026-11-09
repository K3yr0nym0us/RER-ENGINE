use super::context::serialise_frames;
use super::*;

pub(super) struct SwapchainState {
    pub(super) handle: vk::SwapchainKHR,
    pub(super) format: vk::Format,
    pub(super) extent: vk::Extent2D,
    pub(super) views: Vec<vk::ImageView>,
    pub(super) images: Vec<vk::Image>,
    /// Un semáforo de presentación por imagen: reutilizar uno por frame slot podía
    /// señalarlo mientras la imagen anterior seguía en uso por el swapchain.
    pub(super) render_finished: Vec<vk::Semaphore>,
    pub(super) vsync: bool,
}

pub(super) fn create_swapchain(
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
        let mut render_finished = Vec::with_capacity(images.len());
        for _ in &images {
            render_finished.push(
                device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(|error| {
                        vk_error("crear el semáforo de presentación del swapchain", error)
                    })?,
            );
        }
        Ok(SwapchainState {
            handle,
            format: format.format,
            extent,
            views,
            images,
            render_finished,
            vsync,
        })
    }
}

impl SharedGpu {
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

    pub(super) fn recreate_swapchain(
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
            for semaphore in swapchain.render_finished.drain(..) {
                self.inner.device.destroy_semaphore(semaphore, None);
            }
            self.inner.swapchain_loader.destroy_swapchain(old, None);
        }
        *swapchain = created;
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
            offset: vk::Offset2D::default(),
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
        let swapchain = self
            .inner
            .swapchain
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let wait_semaphores = [sync.acquire];
        let signal_semaphores = [swapchain.render_finished[frame.image_index as usize]];
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
