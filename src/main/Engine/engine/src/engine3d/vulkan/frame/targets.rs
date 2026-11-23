use super::*;

pub(super) struct Targets {
    gpu: SharedGpu,
    pub(super) extent: vk::Extent2D,
    pub(super) scene: GpuImage,
    pub(super) normal: Option<GpuImage>,
    pub(super) albedo_out: Option<GpuImage>,
    pub(super) world: Option<GpuImage>,
    pub(super) velocity: Option<GpuImage>,
    pub(super) depth: GpuImage,
    pub(super) background: GpuImage,
    pub(super) reflection: GpuImage,
    pub(super) ssr: GpuImage,
    pub(super) denoise: GpuImage,
    /// Segundo buffer para las pasadas extra de denoise del tier RTX.
    pub(super) denoise2: GpuImage,
    /// WBOIT (F3): acumulación ponderada (RGBA16F), revealage (R16F) y salida compuesta
    /// que `copy_color` lleva al HDR (el resolve no puede escribir la escena que muestrea).
    pub(super) particles_accum: GpuImage,
    pub(super) particles_reveal: GpuImage,
    pub(super) particles_out: GpuImage,
    /// Historial TAA propio cuando los reflejos están activos (ping-pong).
    pub(super) refl_history: [GpuImage; 2],
    pub(super) history: [GpuImage; 2],
    pub(super) shadow: GpuImage,
    pub(super) probe: GpuImage,
    /// Una vista 2D por capa del cubemap array (`slot * 6 + cara`): la sonda dibuja
    /// directamente en su capa, sin render target intermedio ni copia.
    pub(super) probe_face_views: Vec<vk::ImageView>,
    /// Igual sobre `probe_depth` (también array) para el fallback de seis pases.
    pub(super) probe_face_depth_views: Vec<vk::ImageView>,
    /// Vista 2D-array de las 6 caras de cada sonda, para el pase multiview.
    pub(super) probe_slot_views: Vec<vk::ImageView>,
    pub(super) probe_slot_depth_views: Vec<vk::ImageView>,
    /// Se mantiene vivo por su imagen; los pases usan las vistas por cara/slot de abajo.
    _probe_depth: GpuImage,
    /// G-buffer multisampleado del pase forward (solo existe si el tier de MSAA pide > 1).
    pub(super) ms: Option<MultisampleTargets>,
    /// Cara de sonda multisampleada (solo existe si el tier de MSAA pide > 1).
    pub(super) probe_ms: Option<ProbeMultisampleTargets>,
}

/// Copias multisampleadas de los adjuntos activos del G-buffer + su profundidad.
///
/// Se resuelven contra las imágenes 1× que ya consume TAA/SSR/present, así que el resto del frame
/// no cambia. Ranuras apagadas por la máscara no se asignan. La profundidad NO se resuelve: se
/// rellena con un segundo pase a 1× (ver `forward_pass`).
pub(super) struct MultisampleTargets {
    pub(super) samples: u32,
    pub(super) scene: GpuImage,
    pub(super) normal: Option<GpuImage>,
    pub(super) albedo: Option<GpuImage>,
    pub(super) world: Option<GpuImage>,
    pub(super) velocity: Option<GpuImage>,
    pub(super) depth: GpuImage,
}

/// Copia multisampleada de la cara de sonda (solo Color).
///
/// Se resuelve directamente contra la vista 2D de la capa del cubemap que corresponda.
/// Ranuras 1..=4 del `begin_rendering` quedan en `RenderingAttachmentInfo::default()`.
/// La profundidad MS no se resuelve: solo se usa como adjunto del pase y se descarta
/// con `DONT_CARE`.
pub(super) struct ProbeMultisampleTargets {
    pub(super) samples: u32,
    pub(super) scene: GpuImage,
    pub(super) depth: GpuImage,
}

impl ProbeMultisampleTargets {
    pub(super) fn views(&self) -> [Option<vk::ImageView>; 5] {
        [Some(self.scene.view), None, None, None, None]
    }
}

fn opt_view(image: &Option<GpuImage>) -> Option<vk::ImageView> {
    image.as_ref().map(|image| image.view)
}

/// Vista sobre un rango de capas de una imagen array de sondas (`count = 1` → una cara).
fn probe_view(
    gpu: &SharedGpu,
    image: vk::Image,
    format: vk::Format,
    aspect: vk::ImageAspectFlags,
    base_layer: u32,
    count: u32,
    name: &str,
) -> Result<vk::ImageView, VulkanError> {
    let view_type = if count == 1 {
        vk::ImageViewType::TYPE_2D
    } else {
        vk::ImageViewType::TYPE_2D_ARRAY
    };
    unsafe {
        gpu.device().create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(view_type)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: aspect,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: base_layer,
                    layer_count: count,
                }),
            None,
        )
    }
    .map_err(|error| vk_error(&format!("crear la vista {name}"), error))
}

impl Drop for Targets {
    fn drop(&mut self) {
        let device = self.gpu.device();
        for view in self
            .probe_face_views
            .iter()
            .chain(self.probe_face_depth_views.iter())
            .chain(self.probe_slot_views.iter())
            .chain(self.probe_slot_depth_views.iter())
        {
            unsafe { device.destroy_image_view(*view, None) };
        }
    }
}

fn gbuffer_color_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::COLOR_ATTACHMENT
        | vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::STORAGE
        | vk::ImageUsageFlags::TRANSFER_SRC
        | vk::ImageUsageFlags::TRANSFER_DST
}

impl Targets {
    pub(super) fn color_views(&self) -> [Option<vk::ImageView>; 5] {
        [
            Some(self.scene.view),
            opt_view(&self.normal),
            opt_view(&self.albedo_out),
            opt_view(&self.world),
            opt_view(&self.velocity),
        ]
    }

    pub(super) fn ms_views(ms: &MultisampleTargets) -> [Option<vk::ImageView>; 5] {
        [
            Some(ms.scene.view),
            opt_view(&ms.normal),
            opt_view(&ms.albedo),
            opt_view(&ms.world),
            opt_view(&ms.velocity),
        ]
    }

    pub(super) fn new(
        gpu: &SharedGpu,
        extent: vk::Extent2D,
        mask: GbufferMask,
    ) -> Result<Self, VulkanError> {
        let extent = vk::Extent2D {
            width: extent.width.max(1),
            height: extent.height.max(1),
        };
        let color_usage = gbuffer_color_usage();
        let color = |name| {
            gpu.create_image(
                extent,
                COLOR,
                color_usage,
                vk::ImageAspectFlags::COLOR,
                name,
            )
        };
        let scene = color("scene")?;
        let normal = if mask.contains(GbufferMask::NORMAL) {
            Some(color("normal")?)
        } else {
            None
        };
        let albedo_out = if mask.contains(GbufferMask::ALBEDO) {
            Some(color("albedo-out")?)
        } else {
            None
        };
        let world = if mask.contains(GbufferMask::WORLD) {
            Some(gpu.create_image(
                extent,
                WORLD,
                color_usage,
                vk::ImageAspectFlags::COLOR,
                "world",
            )?)
        } else {
            None
        };
        let velocity = if mask.contains(GbufferMask::VELOCITY) {
            Some(gpu.create_image(
                extent,
                VELOCITY,
                color_usage,
                vk::ImageAspectFlags::COLOR,
                "velocity",
            )?)
        } else {
            None
        };
        let depth = gpu.create_image(
            extent,
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "depth",
        )?;
        let background = color("rt-background")?;
        let reflection = color("reflection")?;
        let ssr = color("ssr")?;
        let denoise = color("denoise")?;
        let denoise2 = color("denoise2")?;
        let refl_history = [color("refl-history-0")?, color("refl-history-1")?];
        let history = [color("history-0")?, color("history-1")?];
        // WBOIT (F3): a resolución 1× — la acumulación en un framebuffer multisampleado
        // no tiene sentido y el resolve muestrea la depth 1×.
        let wboit_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC;
        let particles_accum = gpu.create_image(
            extent,
            vk::Format::R16G16B16A16_SFLOAT,
            wboit_usage,
            vk::ImageAspectFlags::COLOR,
            "particles-wboit-accum",
        )?;
        let particles_reveal = gpu.create_image(
            extent,
            vk::Format::R16_SFLOAT,
            wboit_usage,
            vk::ImageAspectFlags::COLOR,
            "particles-wboit-reveal",
        )?;
        let particles_out = color("particles-wboit-out")?;
        let shadow = gpu.create_image(
            vk::Extent2D {
                width: crate::engine3d::engine::SHADOW_MAP_SIZE,
                height: crate::engine3d::engine::SHADOW_MAP_SIZE,
            },
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "shadow",
        )?;
        // Un cubemap array: 6 caras por ranura. Con un solo cubemap compartido, la última sonda
        // capturada del frame tapaba a las demás y todas las entidades recibían el mismo entorno.
        let probe = gpu.create_image_layers(
            vk::Extent2D {
                width: crate::engine3d::probe_registry::PROBE_FACE_SIZE,
                height: crate::engine3d::probe_registry::PROBE_FACE_SIZE,
            },
            crate::engine3d::probe_registry::PROBE_CUBE_ARRAY_LAYERS,
            1,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-cube-array",
        )?;
        let probe_extent = vk::Extent2D {
            width: crate::engine3d::probe_registry::PROBE_FACE_SIZE,
            height: crate::engine3d::probe_registry::PROBE_FACE_SIZE,
        };
        let probe_depth = gpu.create_image_layers(
            probe_extent,
            crate::engine3d::probe_registry::PROBE_CUBE_ARRAY_LAYERS,
            1,
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "probe-depth-array",
        )?;
        let layers = crate::engine3d::probe_registry::PROBE_CUBE_ARRAY_LAYERS;
        let slots = crate::engine3d::probe_registry::MAX_PROBES as u32;
        let mut probe_face_views = Vec::with_capacity(layers as usize);
        let mut probe_face_depth_views = Vec::with_capacity(layers as usize);
        let mut probe_slot_views = Vec::with_capacity(slots as usize);
        let mut probe_slot_depth_views = Vec::with_capacity(slots as usize);
        for layer in 0..layers {
            probe_face_views.push(probe_view(
                gpu,
                probe.image,
                COLOR,
                vk::ImageAspectFlags::COLOR,
                layer,
                1,
                "probe-face",
            )?);
            probe_face_depth_views.push(probe_view(
                gpu,
                probe_depth.image,
                DEPTH,
                vk::ImageAspectFlags::DEPTH,
                layer,
                1,
                "probe-face-depth",
            )?);
        }
        for slot in 0..slots {
            probe_slot_views.push(probe_view(
                gpu,
                probe.image,
                COLOR,
                vk::ImageAspectFlags::COLOR,
                slot * 6,
                6,
                "probe-slot",
            )?);
            probe_slot_depth_views.push(probe_view(
                gpu,
                probe_depth.image,
                DEPTH,
                vk::ImageAspectFlags::DEPTH,
                slot * 6,
                6,
                "probe-slot-depth",
            )?);
        }
        Ok(Self {
            gpu: gpu.clone(),
            extent,
            scene,
            normal,
            albedo_out,
            world,
            velocity,
            depth,
            background,
            reflection,
            ssr,
            denoise,
            denoise2,
            particles_accum,
            particles_reveal,
            particles_out,
            refl_history,
            history,
            shadow,
            probe,
            probe_face_views,
            probe_face_depth_views,
            probe_slot_views,
            probe_slot_depth_views,
            _probe_depth: probe_depth,
            ms: None,
            probe_ms: None,
        })
    }

    /// Crea (o recrea) el G-buffer 1× opcional según la máscara. Devuelve las imágenes
    /// retiradas para soltarlas **después** de reescribir descriptores (mismo protocolo que
    /// el shadow map).
    pub(super) fn apply_mask(
        &mut self,
        gpu: &SharedGpu,
        mask: GbufferMask,
        ms_samples: u32,
    ) -> Result<Vec<GpuImage>, VulkanError> {
        let usage = gbuffer_color_usage();
        let mut retired = Vec::new();
        Self::sync_slot(
            &mut self.normal,
            mask.contains(GbufferMask::NORMAL),
            || {
                gpu.create_image(
                    self.extent,
                    COLOR,
                    usage,
                    vk::ImageAspectFlags::COLOR,
                    "normal",
                )
            },
            &mut retired,
        )?;
        Self::sync_slot(
            &mut self.albedo_out,
            mask.contains(GbufferMask::ALBEDO),
            || {
                gpu.create_image(
                    self.extent,
                    COLOR,
                    usage,
                    vk::ImageAspectFlags::COLOR,
                    "albedo-out",
                )
            },
            &mut retired,
        )?;
        Self::sync_slot(
            &mut self.world,
            mask.contains(GbufferMask::WORLD),
            || {
                gpu.create_image(
                    self.extent,
                    WORLD,
                    usage,
                    vk::ImageAspectFlags::COLOR,
                    "world",
                )
            },
            &mut retired,
        )?;
        Self::sync_slot(
            &mut self.velocity,
            mask.contains(GbufferMask::VELOCITY),
            || {
                gpu.create_image(
                    self.extent,
                    VELOCITY,
                    usage,
                    vk::ImageAspectFlags::COLOR,
                    "velocity",
                )
            },
            &mut retired,
        )?;
        if ms_samples > 1 || self.ms.is_some() {
            self.ensure_ms(gpu, ms_samples, mask)?;
        }
        Ok(retired)
    }

    fn sync_slot(
        slot: &mut Option<GpuImage>,
        want: bool,
        create: impl FnOnce() -> Result<GpuImage, VulkanError>,
        retired: &mut Vec<GpuImage>,
    ) -> Result<(), VulkanError> {
        match (slot.is_some(), want) {
            (true, false) => {
                if let Some(old) = slot.take() {
                    retired.push(old);
                }
            }
            (false, true) => *slot = Some(create()?),
            _ => {}
        }
        Ok(())
    }

    /// Crea (o recrea) el G-buffer multisampleado para `samples` muestras y la máscara activa.
    pub(super) fn ensure_ms(
        &mut self,
        gpu: &SharedGpu,
        samples: u32,
        mask: GbufferMask,
    ) -> Result<(), VulkanError> {
        if samples <= 1 {
            // Soltar las imágenes liberadas por `Drop` de `GpuImage`.
            self.ms = None;
            self.probe_ms = None;
            return Ok(());
        }
        let ms_samples = sample_flags(samples);
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        let ms_color = |name| {
            gpu.create_image_with_samples(
                self.extent,
                COLOR,
                ms_samples,
                usage,
                vk::ImageAspectFlags::COLOR,
                name,
            )
        };
        let ms = MultisampleTargets {
            samples,
            scene: ms_color("scene-ms")?,
            normal: if mask.contains(GbufferMask::NORMAL) {
                Some(ms_color("normal-ms")?)
            } else {
                None
            },
            albedo: if mask.contains(GbufferMask::ALBEDO) {
                Some(ms_color("albedo-ms")?)
            } else {
                None
            },
            world: if mask.contains(GbufferMask::WORLD) {
                Some(gpu.create_image_with_samples(
                    self.extent,
                    WORLD,
                    ms_samples,
                    usage,
                    vk::ImageAspectFlags::COLOR,
                    "world-ms",
                )?)
            } else {
                None
            },
            velocity: if mask.contains(GbufferMask::VELOCITY) {
                Some(gpu.create_image_with_samples(
                    self.extent,
                    VELOCITY,
                    ms_samples,
                    usage,
                    vk::ImageAspectFlags::COLOR,
                    "velocity-ms",
                )?)
            } else {
                None
            },
            depth: gpu.create_image_with_samples(
                self.extent,
                DEPTH,
                ms_samples,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageAspectFlags::DEPTH,
                "depth-ms",
            )?,
        };

        // La captura de sondas usa su propio tamaño (64×64). Se multisamplea aparte para no
        // reutilizar el G-buffer de pantalla, que es mucho mayor.
        let probe_extent = vk::Extent2D {
            width: crate::engine3d::probe_registry::PROBE_FACE_SIZE,
            height: crate::engine3d::probe_registry::PROBE_FACE_SIZE,
        };
        let ms_probe_color = |name| {
            gpu.create_image_with_samples(
                probe_extent,
                COLOR,
                ms_samples,
                usage,
                vk::ImageAspectFlags::COLOR,
                name,
            )
        };
        let probe_ms = ProbeMultisampleTargets {
            samples,
            scene: ms_probe_color("probe-scene-ms")?,
            depth: gpu.create_image_with_samples(
                probe_extent,
                DEPTH,
                ms_samples,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageAspectFlags::DEPTH,
                "probe-depth-ms",
            )?,
        };

        self.ms = Some(ms);
        self.probe_ms = Some(probe_ms);
        log::info!("[msaa] cara de sonda: solo color a {samples}x");
        Ok(())
    }

    pub(super) fn recreate(
        &mut self,
        gpu: &SharedGpu,
        extent: vk::Extent2D,
        mask: GbufferMask,
    ) -> Result<(), VulkanError> {
        *self = Self::new(gpu, extent, mask)?;
        Ok(())
    }

    /// Reasigna solo el shadow map al lado pedido por el tier.
    ///
    /// Devuelve la imagen vieja para soltarla **después** de reescribir los descriptor sets.
    /// Si se destruye el `ImageView` mientras el command buffer está grabando y el set de
    /// escena todavía lo referencia, el validador invalida el buffer (VUID recording).
    pub(super) fn recreate_shadow(
        &mut self,
        gpu: &SharedGpu,
        size: u32,
    ) -> Result<GpuImage, VulkanError> {
        let shadow = gpu.create_image(
            vk::Extent2D {
                width: size,
                height: size,
            },
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "shadow",
        )?;
        Ok(std::mem::replace(&mut self.shadow, shadow))
    }

    /// Reasigna solo el target de reflejos RT cuando cambia la resolución del tier RTX.
    pub(super) fn resize_rt(
        &mut self,
        gpu: &SharedGpu,
        extent: vk::Extent2D,
    ) -> Result<(), VulkanError> {
        if self.reflection.extent == extent {
            return Ok(());
        }
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        self.reflection = gpu.create_image(
            extent,
            COLOR,
            usage,
            vk::ImageAspectFlags::COLOR,
            "reflection",
        )?;
        Ok(())
    }

    /// Reasigna solo el target SSR cuando cambia la resolución del tier SSR.
    pub(super) fn resize_ssr(
        &mut self,
        gpu: &SharedGpu,
        extent: vk::Extent2D,
    ) -> Result<(), VulkanError> {
        if self.ssr.extent == extent {
            return Ok(());
        }
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        self.ssr = gpu.create_image(extent, COLOR, usage, vk::ImageAspectFlags::COLOR, "ssr")?;
        Ok(())
    }
}

impl FrameRenderer {
    pub(super) fn rewrite_target_descriptors(&mut self) -> Result<(), VulkanError> {
        let device = self.gpu.device();
        for slot in 0..FRAMES {
            write_combined(
                device,
                self.sets.scene[slot],
                1,
                self.targets.shadow.view,
                self.samplers[1],
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            )?;
            write_combined(
                device,
                self.sets.probe[slot],
                0,
                self.targets.probe.view,
                self.samplers[0],
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.sky[slot],
                0,
                self.targets.background.view,
                self.samplers[2],
                vk::ImageLayout::GENERAL,
            )?;
        }
        // Sombras volumétricas: el forward muestrea la textura 3D desde el set de escena.
        let sampler = self.volumetric.sampler();
        let light_views = [self.volumetric.light_view(0), self.volumetric.light_view(1)];
        let ubos = [self.volumetric.ubo(0).buffer, self.volumetric.ubo(1).buffer];
        self.rewrite_scene_volume_descriptors(sampler, light_views, ubos)?;
        // Partículas (F3): la depth del soft particles y los samplers del resolve WBOIT
        // apuntan a imágenes recién recreadas.
        self.particle_pool.rewrite_targets(
            self.targets.scene.view,
            self.targets.particles_accum.view,
            self.targets.particles_reveal.view,
            self.targets.depth.view,
        )?;
        Ok(())
    }

    /// Ajusta el G-buffer multisampleado y rellena la caché de pipelines `(máscara, samples)`.
    ///
    /// Se consulta al adaptador (`max_gbuffer_samples`), así que un `Ultra` en una GPU sin soporte
    /// baja a 1× en vez de crear pipelines inválidos. Los pipelines no entran en `self.pipelines`
    /// (`pipe_handles` 0..=14 se mantiene).
    pub(super) fn ensure_msaa(
        &mut self,
        requested: u32,
        device: &ash::Device,
    ) -> Result<(), VulkanError> {
        let wanted = self.gpu.max_gbuffer_samples(requested).clamp(1, 8);
        if wanted != self.msaa_samples {
            self.targets
                .ensure_ms(&self.gpu, wanted, self.gbuffer_mask)?;
            self.msaa_samples = wanted;
            if wanted > 1 {
                log::info!("[msaa] G-buffer multisampleado a {wanted}x");
                if wanted != requested {
                    log::warn!(
                        "[msaa] el tier pidió {requested}x y el adaptador llega a {}x como G-buffer \
                         (color y profundidad a la vez): se usa {wanted}x",
                        self.gpu.gbuffer_sample_ceiling()
                    );
                }
            } else if requested > 1 {
                log::warn!(
                    "[msaa] el tier pidió {requested}x pero la intersección color∩profundidad del \
                     adaptador no da multisampling: el G-buffer trabaja a 1x"
                );
            }
        }
        self.warm_gbuffer_pipes(device)?;
        Ok(())
    }

    /// Recorta el G-buffer de pantalla a la máscara (TAA/SSR/RT). Protocolo shadow-tier:
    /// `wait_idle`, recrear, reescribir descriptores, soltar las imágenes viejas después.
    pub(super) fn ensure_gbuffer_mask(
        &mut self,
        mask: GbufferMask,
        device: &ash::Device,
    ) -> Result<(), VulkanError> {
        if mask == self.gbuffer_mask {
            return Ok(());
        }
        let velocity_on = !self.gbuffer_mask.contains(GbufferMask::VELOCITY)
            && mask.contains(GbufferMask::VELOCITY);
        unsafe {
            device.device_wait_idle().map_err(|error| {
                vk_error("esperar a la GPU antes de cambiar el G-buffer", error)
            })?;
        }
        let retired = self
            .targets
            .apply_mask(&self.gpu, mask, self.msaa_samples)?;
        self.gbuffer_mask = mask;
        self.rewrite_target_descriptors()?;
        drop(retired);
        self.warm_gbuffer_pipes(device)?;
        if velocity_on {
            self.reset_taa_history();
            log::info!("[gbuffer] velocity activada: historial TAA invalidado");
        }
        log::info!("[gbuffer] máscara {:#04x}", mask.bits());
        Ok(())
    }

    pub(super) fn warm_gbuffer_pipes(&mut self, device: &ash::Device) -> Result<(), VulkanError> {
        self.ensure_gbuffer_pipes(device, GbufferMask::color_only(), 1)?;
        self.ensure_gbuffer_pipes(device, self.gbuffer_mask, 1)?;
        if self.msaa_samples > 1 {
            self.ensure_gbuffer_pipes(device, GbufferMask::color_only(), self.msaa_samples)?;
            self.ensure_gbuffer_pipes(device, self.gbuffer_mask, self.msaa_samples)?;
        }
        Ok(())
    }

    fn ensure_gbuffer_pipes(
        &mut self,
        device: &ash::Device,
        mask: GbufferMask,
        samples: u32,
    ) -> Result<(), VulkanError> {
        if self.gbuffer_pipe_cache.contains_key(&(mask, samples)) {
            return Ok(());
        }
        let sky = if samples > 1 {
            Some((self.layout_full, self.full_vert, self.sky_frag))
        } else {
            None
        };
        let set = GbufferPipeSet::create(
            device,
            self.gpu.pipeline_cache(),
            self.layouts[0],
            self.layouts[1],
            self.forward_vert,
            self.forward_frag,
            self.skinned_vert,
            &mask.formats(),
            samples,
            sky,
        )?;
        self.gbuffer_pipe_cache.insert((mask, samples), set);
        Ok(())
    }
}
