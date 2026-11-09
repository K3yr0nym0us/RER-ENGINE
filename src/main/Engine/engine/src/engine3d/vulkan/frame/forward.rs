use super::*;

impl FrameRenderer {
    pub(super) fn forward_pipes(&self, base: Pipes) -> Pipes {
        match self.ms_pipelines {
            Some([forward, forward_blend, skinned, skinned_blend, sky])
                if self.msaa_samples > 1 =>
            {
                let mut pipes = base;
                pipes.forward = forward;
                pipes.forward_blend = forward_blend;
                pipes.skinned = skinned;
                pipes.skinned_blend = skinned_blend;
                pipes.sky = sky;
                pipes
            }
            _ => base,
        }
    }

    /// `Pipes` con los pipelines de solo-profundidad (sin adjuntos de color) para el segundo pase.
    pub(super) fn depth_pipes(&self, base: Pipes) -> Pipes {
        let mut pipes = base;
        pipes.forward = self.depth_pipelines[0];
        pipes.skinned = self.depth_pipelines[1];
        pipes
    }

    pub(super) fn pipe_handles(&self) -> Pipes {
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
        let mut bytes = Vec::new();
        let mut static_offsets = Vec::new();
        for batch in input.static_batches {
            static_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
        let mut static_transparent_offsets = Vec::new();
        for batch in input.static_transparent_batches {
            static_transparent_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
        let mut skinned_offsets = Vec::new();
        for batch in input.skinned_batches {
            skinned_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
        let mut skinned_transparent_offsets = Vec::new();
        for batch in input.skinned_transparent_batches {
            skinned_transparent_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
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
        let mut joints = Vec::new();
        let mut joint_bases = Vec::new();
        let mut joint_bases_transparent = Vec::new();
        for batch in input.skinned_batches {
            joint_bases.push((joints.len() / 64) as u32);
            for matrix in batch.palette {
                for column in matrix {
                    joints.extend_from_slice(bytemuck::cast_slice(column));
                }
            }
        }
        for batch in input.skinned_transparent_batches {
            joint_bases_transparent.push((joints.len() / 64) as u32);
            for matrix in batch.palette {
                for column in matrix {
                    joints.extend_from_slice(bytemuck::cast_slice(column));
                }
            }
        }
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
        self.static_transparent_offsets = static_transparent_offsets;
        self.skinned_offsets = skinned_offsets;
        self.skinned_transparent_offsets = skinned_transparent_offsets;
        self.joint_bases = joint_bases;
        self.joint_bases_transparent = joint_bases_transparent;
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
        let resolve = [
            self.targets.scene.view,
            self.targets.normal.view,
            self.targets.albedo_out.view,
            self.targets.world.view,
            self.targets.velocity.view,
        ];
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
        let ms_pack: Option<([vk::ImageView; 5], vk::ImageView)> = if msaa {
            self.targets.ms.as_ref().map(|ms| {
                (
                    [
                        ms.scene.view,
                        ms.normal.view,
                        ms.albedo.view,
                        ms.world.view,
                        ms.velocity.view,
                    ],
                    ms.depth.view,
                )
            })
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
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layouts[0],
                0,
                &[
                    self.sets.scene[slot],
                    self.sets.albedo[slot],
                    self.sets.empty,
                    self.sets.probe[slot],
                ],
                &[],
            );
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
        self.draw_indexed(
            slot,
            static_batches.iter().map(|batch| {
                (
                    batch.vertex,
                    batch.index,
                    batch.index_count,
                    batch.instance_count,
                )
            }),
            offsets,
            device,
            cmd,
        );
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, skinned);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layouts[1],
                0,
                &[
                    self.sets.scene[slot],
                    self.sets.albedo[slot],
                    self.sets.joints[slot],
                    self.sets.probe[slot],
                ],
                &[],
            );
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
        for (index, batch) in skinned_batches.iter().enumerate() {
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
