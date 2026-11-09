use super::*;

pub(super) struct Targets {
    pub(super) extent: vk::Extent2D,
    pub(super) scene: GpuImage,
    pub(super) normal: GpuImage,
    pub(super) albedo_out: GpuImage,
    pub(super) world: GpuImage,
    pub(super) velocity: GpuImage,
    pub(super) depth: GpuImage,
    pub(super) background: GpuImage,
    pub(super) reflection: GpuImage,
    pub(super) ssr: GpuImage,
    pub(super) denoise: GpuImage,
    pub(super) refl_history: GpuImage,
    pub(super) history: [GpuImage; 2],
    pub(super) shadow: GpuImage,
    pub(super) probe: GpuImage,
    pub(super) probe_color: GpuImage,
    pub(super) probe_normal: GpuImage,
    pub(super) probe_albedo: GpuImage,
    pub(super) probe_world: GpuImage,
    pub(super) probe_velocity: GpuImage,
    pub(super) probe_depth: GpuImage,
    /// G-buffer multisampleado del pase forward (solo existe si el tier de MSAA pide > 1).
    pub(super) ms: Option<MultisampleTargets>,
}

/// Copias multisampleadas de los 5 adjuntos del G-buffer + su profundidad.
///
/// Se resuelven contra las imágenes 1× que ya consume TAA/SSR/present, así que el resto del frame
/// no cambia. La profundidad NO se resuelve: se rellena con un segundo pase a 1× (ver
/// `forward_pass`), porque el resolve de un adjunto de profundidad tiene requisitos de layout que
/// no aportan nada frente a volver a rasterizar los lotes opacos.
pub(super) struct MultisampleTargets {
    pub(super) samples: u32,
    pub(super) scene: GpuImage,
    pub(super) normal: GpuImage,
    pub(super) albedo: GpuImage,
    pub(super) world: GpuImage,
    pub(super) velocity: GpuImage,
    pub(super) depth: GpuImage,
}

impl Targets {
    pub(super) fn new(gpu: &SharedGpu, extent: vk::Extent2D) -> Result<Self, VulkanError> {
        let extent = vk::Extent2D {
            width: extent.width.max(1),
            height: extent.height.max(1),
        };
        let color_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
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
        let normal = color("normal")?;
        let albedo_out = color("albedo-out")?;
        let world = gpu.create_image(
            extent,
            WORLD,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "world",
        )?;
        let velocity = gpu.create_image(
            extent,
            VELOCITY,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "velocity",
        )?;
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
        let refl_history = color("refl-history")?;
        let history = [color("history-0")?, color("history-1")?];
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
        let probe_color = gpu.create_image(
            probe_extent,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-face",
        )?;
        let probe_normal = gpu.create_image(
            probe_extent,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-n",
        )?;
        let probe_albedo = gpu.create_image(
            probe_extent,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-a",
        )?;
        let probe_world = gpu.create_image(
            probe_extent,
            WORLD,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-w",
        )?;
        let probe_velocity = gpu.create_image(
            probe_extent,
            VELOCITY,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-v",
        )?;
        let probe_depth = gpu.create_image(
            probe_extent,
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "probe-depth",
        )?;
        Ok(Self {
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
            refl_history,
            history,
            shadow,
            probe,
            probe_color,
            probe_normal,
            probe_albedo,
            probe_world,
            probe_velocity,
            probe_depth,
            ms: None,
        })
    }

    /// Crea (o recrea) el G-buffer multisampleado para `samples` muestras.
    pub(super) fn ensure_ms(&mut self, gpu: &SharedGpu, samples: u32) -> Result<(), VulkanError> {
        if samples <= 1 {
            // Soltar las imágenes liberadas por `Drop` de `GpuImage`.
            self.ms = None;
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
        self.ms = Some(MultisampleTargets {
            samples,
            scene: ms_color("scene-ms")?,
            normal: ms_color("normal-ms")?,
            albedo: ms_color("albedo-ms")?,
            world: gpu.create_image_with_samples(
                self.extent,
                WORLD,
                ms_samples,
                usage,
                vk::ImageAspectFlags::COLOR,
                "world-ms",
            )?,
            velocity: gpu.create_image_with_samples(
                self.extent,
                VELOCITY,
                ms_samples,
                usage,
                vk::ImageAspectFlags::COLOR,
                "velocity-ms",
            )?,
            depth: gpu.create_image_with_samples(
                self.extent,
                DEPTH,
                ms_samples,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageAspectFlags::DEPTH,
                "depth-ms",
            )?,
        });
        Ok(())
    }

    pub(super) fn recreate(
        &mut self,
        gpu: &SharedGpu,
        extent: vk::Extent2D,
    ) -> Result<(), VulkanError> {
        *self = Self::new(gpu, extent)?;
        Ok(())
    }

    /// Reasigna solo el shadow map al lado pedido por el tier. La imagen vieja se libera con su
    /// `Drop` (destruye view + image + bloque del allocator).
    pub(super) fn recreate_shadow(
        &mut self,
        gpu: &SharedGpu,
        size: u32,
    ) -> Result<(), VulkanError> {
        self.shadow = gpu.create_image(
            vk::Extent2D {
                width: size,
                height: size,
            },
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "shadow",
        )?;
        Ok(())
    }
}

impl FrameRenderer {
    pub(super) fn rewrite_target_descriptors(&self) -> Result<(), VulkanError> {
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
        Ok(())
    }

    /// Ajusta el G-buffer multisampleado y sus pipelines al número de muestras pedido por el tier.
    ///
    /// Se consulta al adaptador (`max_gbuffer_samples`), así que un `Ultra` en una GPU sin soporte
    /// baja a 1× en vez de crear pipelines inválidos. Los pipelines de 1× siguen existiendo porque
    /// la captura de sondas dibuja en imágenes de una muestra.
    pub(super) fn ensure_msaa(
        &mut self,
        requested: u32,
        device: &ash::Device,
    ) -> Result<(), VulkanError> {
        let wanted = self.gpu.max_gbuffer_samples(requested).clamp(1, 8);
        if wanted == self.msaa_samples {
            return Ok(());
        }
        self.targets.ensure_ms(&self.gpu, wanted)?;
        if let Some(old) = self.ms_pipelines.take() {
            for pipeline in old {
                unsafe { device.destroy_pipeline(pipeline, None) };
            }
            // Eran los últimos del registro: se quitan para no destruirlos otra vez en `Drop`.
            for _ in old {
                self.pipelines.pop();
            }
        }
        if wanted > 1 {
            let static_ms = with_samples(static_vertex(), wanted);
            let skinned_ms = with_samples(skinned_vertex(), wanted);
            let created = [
                graphics(
                    device,
                    self.layouts[0],
                    self.forward_vert,
                    Some(self.forward_frag),
                    &static_ms,
                    &MRT,
                    Some(DEPTH),
                    false,
                    false,
                    &mut self.pipelines,
                ),
                graphics(
                    device,
                    self.layouts[0],
                    self.forward_vert,
                    Some(self.forward_frag),
                    &static_ms,
                    &MRT,
                    Some(DEPTH),
                    true,
                    false,
                    &mut self.pipelines,
                ),
                graphics(
                    device,
                    self.layouts[1],
                    self.skinned_vert,
                    Some(self.forward_frag),
                    &skinned_ms,
                    &MRT,
                    Some(DEPTH),
                    false,
                    false,
                    &mut self.pipelines,
                ),
                graphics(
                    device,
                    self.layouts[1],
                    self.skinned_vert,
                    Some(self.forward_frag),
                    &skinned_ms,
                    &MRT,
                    Some(DEPTH),
                    true,
                    false,
                    &mut self.pipelines,
                ),
                // `vkCmdCopyImage` no puede copiar de 1× a 2× (VUID-vkCmdCopyImage-srcImage-00136),
                // así que el fondo trazado se dibuja con un quad full-screen sobre el adjunto
                // multisampleado, sin prueba ni escritura de profundidad.
                graphics_no_depth_test(
                    device,
                    self.layout_full,
                    self.full_vert,
                    Some(self.sky_frag),
                    &with_samples(empty_vertex(), wanted),
                    &MRT,
                    Some(DEPTH),
                    &mut self.pipelines,
                ),
            ];
            let mut built = Vec::new();
            for pipeline in created {
                match pipeline {
                    Ok(pipeline) => built.push(pipeline),
                    Err(error) => {
                        for partial in built {
                            unsafe { device.destroy_pipeline(partial, None) };
                        }
                        return Err(error);
                    }
                }
            }
            let [forward, forward_blend, skinned, skinned_blend, sky] =
                built.try_into().map_err(|_| {
                    VulkanError::new(
                        "No se pudieron crear los pipelines MSAA del forward.".to_string(),
                    )
                })?;
            self.ms_pipelines = Some([forward, forward_blend, skinned, skinned_blend, sky]);
        }
        self.msaa_samples = wanted;
        if wanted > 1 {
            log::info!("[msaa] G-buffer multisampleado a {wanted}x");
            if wanted != requested {
                // Sin este aviso, un Ultra que baja a 4x es indistinguible de un Ultra roto.
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
        Ok(())
    }
}
