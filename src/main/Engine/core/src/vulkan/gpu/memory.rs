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
    /// Una vista `TYPE_2D` por capa del array, para dibujar una cascada por capa (CSM).
    /// Vacío salvo que se llame a `attach_layer_views`. `Drop` las destruye todas.
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
    /// Vista de la capa `index` del array, o `None` si no se crearon vistas por capa.
    ///
    /// No se cae a la vista completa a propósito: con la vista de array, `begin_rendering`
    /// escribiría SIEMPRE la capa 0 y las cascadas lejanas quedarían sin pintar.
    pub fn layer_view(&self, index: u32) -> Option<vk::ImageView> {
        self.layer_views.get(index as usize).copied()
    }

    /// Cuántas vistas por capa tiene el array (0 si nunca se llamó a `attach_layer_views`).
    pub fn layer_view_count(&self) -> usize {
        self.layer_views.len()
    }

    /// Crea una vista `TYPE_2D` por capa del array. Sin esto no se puede dirigir cada
    /// cascada a su capa: el rango de capas del adjunto sale del `ImageView`.
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
        unsafe {
            for view in self.layer_views.drain(..) {
                self.gpu.inner.device.destroy_image_view(view, None);
            }
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
    /// Bytes ya mapeados del buffer, para leer el resultado de una copia GPU→CPU (sonda de
    /// sombras). Solo es válido después de que el trabajo que escribió el buffer terminó.
    pub fn mapped_bytes(&self) -> Option<&[u8]> {
        self.allocation.as_ref()?.mapped_slice()
    }

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
            layer_views: Vec::new(),
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
            layer_views: Vec::new(),
            format,
            extent,
            aspect,
            layers,
            mips,
        };
        self.transition_initial(&image)?;
        Ok(image)
    }

    /// Crea una imagen 3D (volumen) con vista `TYPE_3D`, para `sampler3D`/`image3D`.
    ///
    /// `GpuImage.extent` sigue siendo 2D (ancho/alto); la resolución Z la conserva el llamador,
    /// que es quien conoce el volumen y la usa en barreras y descriptores. `layer_count` de las
    /// barreras sigue siendo 1: en una imagen 3D toda la profundidad es un único subrecurso.
    pub fn create_image_3d(
        &self,
        extent: vk::Extent3D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        name: &str,
    ) -> Result<GpuImage, VulkanError> {
        let extent = vk::Extent3D {
            width: extent.width.max(1),
            height: extent.height.max(1),
            depth: extent.depth.max(1),
        };
        let image = unsafe {
            self.inner.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_3D)
                    .format(format)
                    .extent(extent)
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .initial_layout(vk::ImageLayout::UNDEFINED),
                None,
            )
        }
        .map_err(|error| vk_error(&format!("crear la imagen 3D {name}"), error))?;
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
                VulkanError::new(format!("Sin memoria GPU para el volumen {name}: {error}"))
            })?;
        unsafe {
            self.inner
                .device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .map_err(|error| vk_error(&format!("asociar memoria al volumen {name}"), error))?;
        }
        let view = unsafe {
            self.inner.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_3D)
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
        .map_err(|error| vk_error(&format!("crear la vista 3D {name}"), error))?;
        let image = GpuImage {
            gpu: self.clone(),
            image,
            allocation: Some(allocation),
            view,
            layer_views: Vec::new(),
            format,
            extent: vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
            aspect,
            layers: 1,
            mips: 1,
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

    /// Sube una cadena completa de mips de una capa con un único staging buffer y un único submit.
    ///
    /// Cada tupla de `mips` es `(ancho, alto, rgba)`; el nivel de mip es el índice de la tupla.
    pub fn upload_rgba_mips(
        &self,
        image: &GpuImage,
        layer: u32,
        mips: &[(u32, u32, &[u8])],
    ) -> Result<(), VulkanError> {
        self.upload_rgba_mips_at(image, 0, layer, mips)
    }

    fn upload_rgba_mips_at(
        &self,
        image: &GpuImage,
        first_mip: u32,
        layer: u32,
        mips: &[(u32, u32, &[u8])],
    ) -> Result<(), VulkanError> {
        if mips.is_empty() {
            return Ok(());
        }
        let offset_align = (self.inner.limits.optimal_buffer_copy_offset_alignment as usize).max(1);
        let row_align = (self.inner.limits.optimal_buffer_copy_row_pitch_alignment as usize).max(4);
        let mut packed: Vec<u8> = Vec::new();
        let mut regions = Vec::with_capacity(mips.len());
        for (level, &(width, height, rgba)) in mips.iter().enumerate() {
            let w = width.max(1);
            let h = height.max(1);
            let row_bytes = (w as usize).checked_mul(4).ok_or_else(|| {
                VulkanError::new("mip demasiado ancho para el staging".to_string())
            })?;
            let needed = row_bytes.checked_mul(h as usize).ok_or_else(|| {
                VulkanError::new("mip demasiado grande para el staging".to_string())
            })?;
            if rgba.len() < needed {
                return Err(VulkanError::new(format!(
                    "mip {} incompleto: {} bytes, se esperaban {needed}",
                    first_mip as usize + level,
                    rgba.len()
                )));
            }
            let pitch = align_up(row_bytes, row_align);
            let offset = align_up(packed.len(), offset_align);
            if offset > packed.len() {
                packed.resize(offset, 0);
            }
            let start = packed.len();
            let total = pitch
                .checked_mul(h as usize)
                .ok_or_else(|| VulkanError::new("staging de mips desbordado".to_string()))?;
            let end = start
                .checked_add(total)
                .ok_or_else(|| VulkanError::new("staging de mips desbordado".to_string()))?;
            packed.resize(end, 0);
            for row in 0..h as usize {
                let src = row * row_bytes;
                let dst = start + row * pitch;
                packed[dst..dst + row_bytes].copy_from_slice(&rgba[src..src + row_bytes]);
            }
            regions.push(
                vk::BufferImageCopy::default()
                    .buffer_offset(offset as u64)
                    .buffer_row_length((pitch / 4) as u32)
                    .buffer_image_height(h)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: image.aspect,
                        mip_level: first_mip + level as u32,
                        base_array_layer: layer,
                        layer_count: 1,
                    })
                    .image_extent(vk::Extent3D {
                        width: w,
                        height: h,
                        depth: 1,
                    }),
            );
        }
        let staging = self.create_buffer_with_data(
            &packed,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "tex-mips-staging",
        )?;
        let image_handle = image.image;
        let aspect = image.aspect;
        let range = vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: first_mip,
            level_count: mips.len() as u32,
            base_array_layer: layer,
            layer_count: 1,
        };
        self.one_shot(move |device, command| {
            cmd_image_barrier_subresource(
                device,
                command,
                image_handle,
                range,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::TRANSFER_WRITE,
            );
            unsafe {
                device.cmd_copy_buffer_to_image(
                    command,
                    staging.buffer,
                    image_handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &regions,
                );
            }
            cmd_image_barrier_subresource(
                device,
                command,
                image_handle,
                range,
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

    /// Sube un único nivel a una mip/capa concreta (API de compatibilidad).
    pub fn upload_rgba_layer(
        &self,
        image: &GpuImage,
        mip: u32,
        layer: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), VulkanError> {
        self.upload_rgba_mips_at(image, mip, layer, &[(width, height, rgba)])
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
        self.upload_rgba_layer_region(image, 0, 0, x, y, width, height, rgba)
    }

    /// Sube un subrectángulo RGBA a una capa del array (atlas de splats).
    pub fn upload_rgba_layer_region(
        &self,
        image: &GpuImage,
        mip: u32,
        layer: u32,
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
            "tex-atlas-staging",
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

    /// Copia `layers` capas (desde la capa 0) y los niveles de mip `0..mips` de `src` a `dst`.
    ///
    /// Ambas imágenes deben permanecer en `GENERAL` (invariante de `transition_initial`) y
    /// haberse creado con `TRANSFER_SRC` en `src` y `TRANSFER_DST` en `dst`.
    pub fn copy_image_layers(
        &self,
        src: &GpuImage,
        dst: &GpuImage,
        layers: u32,
        mips: u32,
    ) -> Result<(), VulkanError> {
        if layers == 0 || mips == 0 {
            return Ok(());
        }
        if src.layers < layers {
            return Err(VulkanError::new(format!(
                "la imagen origen tiene {} capas, se pidieron {layers}",
                src.layers
            )));
        }
        if dst.layers < layers {
            return Err(VulkanError::new(format!(
                "la imagen destino tiene {} capas, se pidieron {layers}",
                dst.layers
            )));
        }
        if src.mips < mips {
            return Err(VulkanError::new(format!(
                "la imagen origen tiene {} mips, se pidieron {mips}",
                src.mips
            )));
        }
        if dst.mips < mips {
            return Err(VulkanError::new(format!(
                "la imagen destino tiene {} mips, se pidieron {mips}",
                dst.mips
            )));
        }
        if src.extent != dst.extent {
            return Err(VulkanError::new(format!(
                "extent distinto al copiar capas: origen {:?}, destino {:?}",
                src.extent, dst.extent
            )));
        }
        if src.format != dst.format {
            return Err(VulkanError::new(format!(
                "format distinto al copiar capas: origen {:?}, destino {:?}",
                src.format, dst.format
            )));
        }
        if src.aspect != dst.aspect || !src.aspect.contains(vk::ImageAspectFlags::COLOR) {
            return Err(VulkanError::new(format!(
                "aspect incompatible al copiar capas: origen {:?}, destino {:?}",
                src.aspect, dst.aspect
            )));
        }

        let src_handle = src.image;
        let dst_handle = dst.image;
        let src_aspect = src.aspect;
        let dst_aspect = dst.aspect;
        let base_extent = src.extent;

        self.one_shot(move |device, command| {
            cmd_image_barrier(
                device,
                command,
                src_handle,
                src_aspect,
                mips,
                layers,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::MEMORY_WRITE,
                vk::AccessFlags::TRANSFER_READ,
            );
            cmd_image_barrier(
                device,
                command,
                dst_handle,
                dst_aspect,
                mips,
                layers,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                vk::AccessFlags::TRANSFER_WRITE,
            );

            let regions: Vec<vk::ImageCopy> = (0..mips)
                .map(|mip| vk::ImageCopy {
                    src_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: src_aspect,
                        mip_level: mip,
                        base_array_layer: 0,
                        layer_count: layers,
                    },
                    src_offset: vk::Offset3D::default(),
                    dst_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: dst_aspect,
                        mip_level: mip,
                        base_array_layer: 0,
                        layer_count: layers,
                    },
                    dst_offset: vk::Offset3D::default(),
                    extent: vk::Extent3D {
                        width: (base_extent.width >> mip).max(1),
                        height: (base_extent.height >> mip).max(1),
                        depth: 1,
                    },
                })
                .collect();
            unsafe {
                device.cmd_copy_image(
                    command,
                    src_handle,
                    vk::ImageLayout::GENERAL,
                    dst_handle,
                    vk::ImageLayout::GENERAL,
                    &regions,
                );
            }

            cmd_image_barrier(
                device,
                command,
                dst_handle,
                dst_aspect,
                mips,
                layers,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
            );
        })
    }

    /// Graba, envía y espera un command buffer de un solo uso (uploads y builds).
    ///
    /// Reutiliza el fence y el command buffer del device: `one_shot_lock` serializa su uso y
    /// `wait_for_fences` deja el fence señalado antes de que la siguiente llamada lo reinicie.
    pub fn one_shot(
        &self,
        record: impl FnOnce(&ash::Device, vk::CommandBuffer),
    ) -> Result<(), VulkanError> {
        let _guard = self
            .inner
            .one_shot_lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (fence, command) = self.one_shot_resources();
        unsafe {
            self.inner
                .device
                .reset_fences(&[fence])
                .map_err(|error| vk_error("reiniciar el fence de upload", error))?;
            self.inner
                .device
                .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                .map_err(|error| vk_error("reiniciar el command buffer de upload", error))?;
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
        Ok(())
    }
}

fn align_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

/// Barrera de imagen acotada a un rango explícito: el helper de `gpu/mod.rs` siempre
/// empieza en mip 0 y capa 0, aquí hace falta escribir una sola capa del array.
fn cmd_image_barrier_subresource(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    range: vk::ImageSubresourceRange,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
) {
    let barrier = super::image_memory_barrier2(
        image, range, old_layout, new_layout, src_stage, dst_stage, src_access, dst_access,
    );
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    unsafe {
        device.cmd_pipeline_barrier2(command, &dependency);
    }
}
