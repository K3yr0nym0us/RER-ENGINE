use super::*;

impl FrameRenderer {
    pub(super) fn forward_pipes(&self, base: Pipes) -> Pipes {
        let samples = if self.msaa_samples > 1 {
            self.msaa_samples
        } else {
            1
        };
        self.overlay_gbuffer_pipes(base, self.gbuffer_mask, samples)
    }

    pub(super) fn overlay_gbuffer_pipes(
        &self,
        base: Pipes,
        mask: GbufferMask,
        samples: u32,
    ) -> Pipes {
        self.gbuffer_pipe_cache
            .get(&(mask, samples))
            .map(|set| set.overlay(base))
            .unwrap_or(base)
    }

    /// `Pipes` con los pipelines de solo-profundidad (sin adjuntos de color) para el segundo pase.
    pub(super) fn depth_pipes(&self, base: Pipes) -> Pipes {
        let mut pipes = base;
        pipes.forward = self.depth_pipelines[0];
        pipes.skinned = self.depth_pipelines[1];
        pipes
    }

    pub(super) fn pipe_handles(&self) -> Pipes {
        // Índices fijos del registro raster 1× (taa=8 … gizmo_lines=14). Un pipeline
        // extra *antes* de present desplaza composite/present: UNORM sobre el swapchain
        // y el set `full` de composite enlazado como si fuera present.
        Pipes {
            forward: self.pipelines[0],
            forward_blend: self.pipelines[1],
            skinned: self.pipelines[2],
            skinned_blend: self.pipelines[3],
            shadow: self.pipelines[4],
            shadow_skinned: self.pipelines[5],
            gizmo: self.pipelines[6],
            hud: self.pipelines[7],
            taa: self.pipelines[8],
            ssr: self.pipelines[9],
            denoise: self.pipelines[11],
            composite: self.pipelines[12],
            present: self.pipelines[13],
            gizmo_lines: self.pipelines[14],
            sky: self.pipelines[13],
        }
    }

    pub(super) fn prepare_uploads(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
    ) -> Result<(), VulkanError> {
        let instance_slices = input
            .static_batches
            .iter()
            .map(|batch| batch.instances)
            .chain(
                input
                    .shadow_static_batches
                    .iter()
                    .map(|batch| batch.instances),
            )
            .chain(
                input
                    .static_transparent_batches
                    .iter()
                    .map(|batch| batch.instances),
            )
            .chain(input.skinned_batches.iter().map(|batch| batch.instances))
            .chain(
                input
                    .shadow_skinned_batches
                    .iter()
                    .map(|batch| batch.instances),
            )
            .chain(
                input
                    .skinned_transparent_batches
                    .iter()
                    .map(|batch| batch.instances),
            );
        let (mut bytes, instance_offsets) = pack_unique_slices(instance_slices);
        let offset_of = |slice: &[u8]| slice_offset(&instance_offsets, slice);
        let static_offsets: Vec<u64> = input
            .static_batches
            .iter()
            .map(|batch| offset_of(batch.instances))
            .collect();
        let shadow_offsets: Vec<u64> = input
            .shadow_static_batches
            .iter()
            .map(|batch| offset_of(batch.instances))
            .collect();
        let static_transparent_offsets: Vec<u64> = input
            .static_transparent_batches
            .iter()
            .map(|batch| offset_of(batch.instances))
            .collect();
        let skinned_offsets: Vec<u64> = input
            .skinned_batches
            .iter()
            .map(|batch| offset_of(batch.instances))
            .collect();
        let shadow_skinned_offsets: Vec<u64> = input
            .shadow_skinned_batches
            .iter()
            .map(|batch| offset_of(batch.instances))
            .collect();
        let skinned_transparent_offsets: Vec<u64> = input
            .skinned_transparent_batches
            .iter()
            .map(|batch| offset_of(batch.instances))
            .collect();
        self.hud_offset = align_up(bytes.len() as u32, 256) as u64;
        let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
        bytes.extend(std::iter::repeat_n(0, pad));
        bytes.extend_from_slice(input.hud_instances);
        if bytes.is_empty() {
            bytes.extend_from_slice(&[0, 0, 0, 0]);
        }
        let gpu = self.gpu.clone();
        Self::grow_host(&gpu, &mut self.instances[slot], bytes.len() as u64, "inst")?;
        self.instances[slot].write(0, &bytes)?;
        let palettes = input
            .skinned_batches
            .iter()
            .map(|batch| batch.palette)
            .chain(
                input
                    .skinned_transparent_batches
                    .iter()
                    .map(|batch| batch.palette),
            )
            .chain(
                input
                    .shadow_skinned_batches
                    .iter()
                    .map(|batch| batch.palette),
            );
        let (mut joints, palette_bases) = pack_unique_palettes(palettes);
        let palette_base = |palette: &[[[f32; 4]; 4]]| {
            if palette.is_empty() {
                0
            } else {
                palette_bases
                    .get(&(palette.as_ptr() as usize))
                    .copied()
                    .unwrap_or(0)
            }
        };
        let joint_bases: Vec<u32> = input
            .skinned_batches
            .iter()
            .map(|batch| palette_base(batch.palette))
            .collect();
        let joint_bases_transparent: Vec<u32> = input
            .skinned_transparent_batches
            .iter()
            .map(|batch| palette_base(batch.palette))
            .collect();
        let shadow_joint_bases: Vec<u32> = input
            .shadow_skinned_batches
            .iter()
            .map(|batch| palette_base(batch.palette))
            .collect();
        if joints.is_empty() {
            joints.extend_from_slice(&[0, 0, 0, 0]);
        }
        Self::grow_host(&gpu, &mut self.joints[slot], joints.len() as u64, "joints")?;
        self.joints[slot].write(0, &joints)?;
        write_storage(
            self.gpu.device(),
            self.sets.joints[slot],
            0,
            &self.joints[slot],
        )?;
        self.static_offsets = static_offsets;
        self.shadow_offsets = shadow_offsets;
        self.static_transparent_offsets = static_transparent_offsets;
        self.skinned_offsets = skinned_offsets;
        self.shadow_skinned_offsets = shadow_skinned_offsets;
        self.skinned_transparent_offsets = skinned_transparent_offsets;
        self.joint_bases = joint_bases;
        self.joint_bases_transparent = joint_bases_transparent;
        self.shadow_joint_bases = shadow_joint_bases;
        Ok(())
    }

    pub(super) fn grow_host(
        gpu: &SharedGpu,
        buffer: &mut GpuBuffer,
        size: u64,
        name: &str,
    ) -> Result<(), VulkanError> {
        if buffer.size >= size {
            return Ok(());
        }
        *buffer = host_buffer(gpu, size.next_power_of_two(), name)?;
        Ok(())
    }

    pub(super) fn forward_pass(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let extent = self.targets.extent;
        let resolve = self.targets.color_views();
        // `ms.samples == msaa_samples`: si el G-buffer no se recreó aún para este nivel, se cae
        // al camino de 1× en vez de mezclar pipelines y adjuntos con muestras distintas.
        let msaa = self.msaa_samples > 1
            && self
                .targets
                .ms
                .as_ref()
                .is_some_and(|ms| ms.samples == self.msaa_samples);
        // Sin MSAA el fondo se copia 1:1 a la escena. Con MSAA no se puede copiar (Vulkan exige el
        // mismo número de muestras en origen y destino, VUID-vkCmdCopyImage-srcImage-00136), así
        // que el quad del cielo se dibuja dentro del propio pase multisampleado.
        if !msaa {
            copy_color(
                device,
                cmd,
                &self.targets.background,
                &self.targets.scene,
                extent,
                0,
            );
        }
        let color_pipes = self.forward_pipes(*pipes);
        // Vistas multisampleadas copiadas fuera del borrow: los pases necesitan `&mut self` para
        // registrar los layouts de profundidad y `self.targets.ms` prestado no convive con eso.
        let ms_pack: Option<([Option<vk::ImageView>; 5], vk::ImageView)> = if msaa {
            self.targets
                .ms
                .as_ref()
                .map(|ms| (Targets::ms_views(ms), ms.depth.view))
        } else {
            None
        };
        match ms_pack {
            Some((ms_views, ms_depth)) => {
                // `clear = true`: el adjunto de escena empieza vacío y el quad del cielo lo pinta.
                begin_color_ms(device, cmd, &ms_views, ms_depth, &resolve, extent, true);
                set_viewport(device, cmd, extent);
                unsafe {
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, color_pipes.sky);
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.layout_full,
                        0,
                        &[self.sets.sky[slot]],
                        &[],
                    );
                    device.cmd_draw(cmd, 3, 1, 0, 0);
                }
                self.draw_forward(slot, input, &color_pipes, device, cmd, 0.0);
                self.draw_forward(slot, input, &color_pipes, device, cmd, 1.0);
                unsafe { device.cmd_end_rendering(cmd) };
                // La profundidad del pase MSAA se descarta: se vuelve a rasterizar la geometría
                // opaca contra `targets.depth` con el fragment shader del forward, para conservar
                // el `discard` del alfa cortado y no resolver un adjunto de profundidad.
                let depth_image = self.targets.depth.image;
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_ATTACHMENT,
                );
                begin_depth(device, cmd, self.targets.depth.view, extent);
                set_viewport(device, cmd, extent);
                let depth_pipes = self.depth_pipes(*pipes);
                self.draw_forward(slot, input, &depth_pipes, device, cmd, 0.0);
                unsafe { device.cmd_end_rendering(cmd) };
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_READ_ONLY,
                );
            }
            _ => {
                let views = resolve;
                let depth_image = self.targets.depth.image;
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_ATTACHMENT,
                );
                begin_color(
                    device,
                    cmd,
                    &views,
                    Some(self.targets.depth.view),
                    extent,
                    false,
                );
                set_viewport(device, cmd, extent);
                self.draw_forward(slot, input, &color_pipes, device, cmd, 0.0);
                self.draw_forward(slot, input, &color_pipes, device, cmd, 1.0);
                unsafe { device.cmd_end_rendering(cmd) };
                // El trazado de reflejos, el SSR, el denoise y la sonda muestrean esta imagen.
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_READ_ONLY,
                );
            }
        }
        Ok(())
    }

    pub(super) fn draw_forward(
        &self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pass_kind: f32,
    ) {
        let pipeline = if pass_kind < 0.5 {
            pipes.forward
        } else {
            pipes.forward_blend
        };
        let skinned = if pass_kind < 0.5 {
            pipes.skinned
        } else {
            pipes.skinned_blend
        };
        let push = push_bytes(0, pass_kind);
        let static_batches = if pass_kind < 0.5 {
            input.static_batches
        } else {
            input.static_transparent_batches
        };
        let skinned_batches = if pass_kind < 0.5 {
            input.skinned_batches
        } else {
            input.skinned_transparent_batches
        };
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_push_constants(
                cmd,
                self.layouts[0],
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &push,
            );
        }
        let offsets = if pass_kind < 0.5 {
            &self.static_offsets
        } else {
            &self.static_transparent_offsets
        };
        // El set de albedo se enlaza por tramos consecutivos de `hi_res`: los lotes
        // estándar siguen usando `sets.albedo` y solo los 4K cambian al segundo juego.
        let mut start = 0;
        while start < static_batches.len() {
            let hi_res = static_batches[start].hi_res;
            let mut end = start + 1;
            while end < static_batches.len() && static_batches[end].hi_res == hi_res {
                end += 1;
            }
            let albedo = if hi_res {
                self.sets.model_albedo[slot]
            } else {
                self.sets.albedo[slot]
            };
            unsafe {
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.layouts[0],
                    0,
                    &[
                        self.sets.scene[slot],
                        albedo,
                        self.sets.empty,
                        self.sets.probe[slot],
                    ],
                    &[],
                );
            }
            self.draw_indexed(
                slot,
                static_batches[start..end].iter().map(|batch| {
                    (
                        batch.vertex,
                        batch.index,
                        batch.index_count,
                        batch.instance_count,
                    )
                }),
                &offsets[start..end],
                device,
                cmd,
            );
            start = end;
        }
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, skinned);
        }
        let joint_bases = if pass_kind < 0.5 {
            &self.joint_bases
        } else {
            &self.joint_bases_transparent
        };
        let skinned_offsets = if pass_kind < 0.5 {
            &self.skinned_offsets
        } else {
            &self.skinned_transparent_offsets
        };
        let mut bound_hi_res: Option<bool> = None;
        for (index, batch) in skinned_batches.iter().enumerate() {
            if bound_hi_res != Some(batch.hi_res) {
                let albedo = if batch.hi_res {
                    self.sets.model_albedo[slot]
                } else {
                    self.sets.albedo[slot]
                };
                unsafe {
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.layouts[1],
                        0,
                        &[
                            self.sets.scene[slot],
                            albedo,
                            self.sets.joints[slot],
                            self.sets.probe[slot],
                        ],
                        &[],
                    );
                }
                bound_hi_res = Some(batch.hi_res);
            }
            let push = push_bytes(joint_bases.get(index).copied().unwrap_or(0), pass_kind);
            unsafe {
                device.cmd_push_constants(
                    cmd,
                    self.layouts[1],
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    &push,
                );
            }
            let offset = skinned_offsets.get(index).copied().unwrap_or(0);
            draw_one(
                device,
                cmd,
                batch.vertex,
                batch.index,
                batch.index_count,
                batch.instance_count,
                &self.instances[slot],
                offset,
            );
        }
    }

    pub(super) fn draw_indexed<'a>(
        &self,
        slot: usize,
        batches: impl Iterator<Item = (&'a GpuBuffer, &'a GpuBuffer, u32, u32)>,
        offsets: &[u64],
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) {
        for (index, (vertex, index_buffer, index_count, instance_count)) in batches.enumerate() {
            let offset = offsets.get(index).copied().unwrap_or(0);
            draw_one(
                device,
                cmd,
                vertex,
                index_buffer,
                index_count,
                instance_count,
                &self.instances[slot],
                offset,
            );
        }
    }
}

fn pack_unique_slices<'a>(
    slices: impl IntoIterator<Item = &'a [u8]>,
) -> (Vec<u8>, std::collections::HashMap<usize, u64>) {
    let mut longest: std::collections::HashMap<usize, &'a [u8]> = std::collections::HashMap::new();
    let mut order = Vec::new();
    for slice in slices {
        if slice.is_empty() {
            continue;
        }
        let key = slice.as_ptr() as usize;
        match longest.get(&key) {
            Some(old) if old.len() >= slice.len() => {}
            Some(_) => {
                longest.insert(key, slice);
            }
            None => {
                longest.insert(key, slice);
                order.push(key);
            }
        }
    }
    let mut bytes = Vec::new();
    let mut offsets = std::collections::HashMap::new();
    for key in order {
        let Some(slice) = longest.get(&key).copied() else {
            continue;
        };
        let offset = align_up(bytes.len() as u32, 256) as u64;
        let pad = offset as usize - bytes.len();
        bytes.extend(std::iter::repeat_n(0, pad));
        bytes.extend_from_slice(slice);
        offsets.insert(key, offset);
    }
    (bytes, offsets)
}

fn slice_offset(offsets: &std::collections::HashMap<usize, u64>, slice: &[u8]) -> u64 {
    if slice.is_empty() {
        0
    } else {
        offsets
            .get(&(slice.as_ptr() as usize))
            .copied()
            .unwrap_or(0)
    }
}

fn pack_unique_palettes<'a>(
    palettes: impl IntoIterator<Item = &'a [[[f32; 4]; 4]]>,
) -> (Vec<u8>, std::collections::HashMap<usize, u32>) {
    let mut joints = Vec::new();
    let mut bases = std::collections::HashMap::new();
    for palette in palettes {
        if palette.is_empty() {
            continue;
        }
        let key = palette.as_ptr() as usize;
        if bases.contains_key(&key) {
            continue;
        }
        let base = (joints.len() / 64) as u32;
        bases.insert(key, base);
        for matrix in palette {
            for column in matrix {
                joints.extend_from_slice(bytemuck::cast_slice(column));
            }
        }
    }
    (joints, bases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unique_slices_reuses_prefix_of_longer_backing() {
        let data = [1u8, 2, 3, 4, 5, 6];
        let full = &data[..];
        let prefix = &data[..2];
        let (bytes, offsets) = pack_unique_slices([prefix, full]);
        assert_eq!(slice_offset(&offsets, prefix), slice_offset(&offsets, full));
        assert!(bytes.windows(6).any(|window| window == full));
        assert_eq!(offsets.len(), 1);
    }

    #[test]
    fn pack_unique_palettes_reuses_same_pointer() {
        let palette = [[[1.0f32; 4]; 4]; 2];
        let (joints, bases) = pack_unique_palettes([&palette[..], &palette[..]]);
        assert_eq!(bases.len(), 1);
        assert_eq!(joints.len(), 2 * 64);
    }
}
