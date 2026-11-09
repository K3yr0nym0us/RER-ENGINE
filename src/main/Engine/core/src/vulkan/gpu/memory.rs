use super::*;

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
    #[allow(dead_code)]
    pub format: vk::Format,
    #[allow(dead_code)]
    pub extent: vk::Extent2D,
    pub aspect: vk::ImageAspectFlags,
    pub layers: u32,
    pub mips: u32,
}

impl Drop for GpuImage {
    fn drop(&mut self) {
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

/// Mayor número de muestras ≤ `desired` presente en `supported`.
///
/// Vulkan reporta el soporte de MSAA como banderas (`framebufferColorSampleCounts`,
/// `framebufferDepthSampleCounts`) con los bits 1/2/4/8/16/32/64; no existe un límite
/// `maxImageSampleCount` que consultar, así que estas banderas son la única fuente. MSAA solo
/// admite potencias de dos, por eso se recorre la tabla de menor a mayor y se conserva el último
/// valor soportado. Si no hay ninguno se devuelve 1 (sin multisampling) en lugar de un
/// `sample_count` que invalide el pipeline.
pub(super) fn pick_sample_count(supported: vk::SampleCountFlags, desired: u32) -> u32 {
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

impl SharedGpu {
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
        };
        self.transition_initial(&image)?;
        Ok(image)
    }

    pub(super) fn transition_initial(&self, image: &GpuImage) -> Result<(), VulkanError> {
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
        let _guard = self
            .inner
            .one_shot_lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let command = unsafe {
            self.inner.device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(self.inner.one_shot_pool)
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
                .free_command_buffers(self.inner.one_shot_pool, &[command]);
        }
        Ok(())
    }
}
