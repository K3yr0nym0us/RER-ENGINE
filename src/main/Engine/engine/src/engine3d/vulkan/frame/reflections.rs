use super::rt::cube_face_view_proj;
use super::*;

impl FrameRenderer {
    pub(super) fn reflection_pass(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let _ = pipes;
        let src_stage = if self.gpu.hw_rt() {
            vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR
        } else {
            vk::PipelineStageFlags::COMPUTE_SHADER
        };
        // El trazado lee normal/depth recién escritos por el forward pass. La depth ya queda
        // registrada en READ_ONLY por `forward_pass`; aquí solo se sincroniza el G-buffer normal.
        if let Some(normal) = self.targets.normal.as_ref() {
            crate::engine3d::vulkan::gpu::cmd_image_barrier(
                device,
                cmd,
                normal.image,
                vk::ImageAspectFlags::COLOR,
                1,
                1,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                src_stage,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_WRITE,
                vk::AccessFlags::SHADER_READ,
            );
        }
        if self.gpu.hw_rt() {
            let view = self.targets.reflection.view;
            self.dispatch_rays(slot, input, 1, view, device, cmd)?;
        } else {
            let view = self.targets.reflection.view;
            self.dispatch_bvh(slot, input, 1, view, device, cmd)?;
        }
        // El compute/ray tracing escribe la reflexión y el denoise la muestrea
        // inmediatamente: sin esta barrera aparecen bloques negros intermitentes.
        crate::engine3d::vulkan::gpu::cmd_image_barrier(
            device,
            cmd,
            self.targets.reflection.image,
            vk::ImageAspectFlags::COLOR,
            1,
            1,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::GENERAL,
            src_stage,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::AccessFlags::SHADER_READ,
        );
        Ok(())
    }

    /// Captura una sonda completa (6 caras) dentro de su ranura del cubemap array.
    pub(super) fn capture_probe(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        capture: ProbeCapture,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let face_size = crate::engine3d::probe_registry::PROBE_FACE_SIZE;
        let extent = vk::Extent2D {
            width: face_size,
            height: face_size,
        };
        let ibl_weight_offset = crate::engine3d::engine::SceneUniforms::IBL_WEIGHT_OFFSET;
        let probe_slot = capture
            .slot
            .min(crate::engine3d::probe_registry::MAX_PROBES - 1) as u32;
        let base_layer = probe_slot * 6;
        // La captura puede usar el G-buffer MS de sonda o el camino 1× según el tier actual.
        // Se decide una vez por sonda, no por cara.
        let probe_ms = self.targets.probe_ms.as_ref().filter(|ms| {
            ms.samples > 1
                && ms.samples == self.msaa_samples
                && self
                    .gbuffer_pipe_cache
                    .contains_key(&(GbufferMask::color_only(), ms.samples))
        });
        if let Some(ms) = probe_ms {
            log::debug!("[msaa] captura de sonda a {}x (solo color)", ms.samples);
        }
        let probe_samples = if probe_ms.is_some() {
            self.msaa_samples
        } else {
            1
        };
        let pipes = self.overlay_gbuffer_pipes(*pipes, GbufferMask::color_only(), probe_samples);

        if probe_ms.is_none()
            && let (Some(forward), Some(skinned)) = (self.probe_mv_forward, self.probe_mv_skinned)
        {
            // Un solo pase: seis VP van en el binding 4 y `gl_ViewIndex` elige la cara.
            let mut faces = [0u8; 6 * 64];
            for face in 0..6 {
                let vp = cube_face_view_proj(capture.origin, face);
                faces[face * 64..(face + 1) * 64].copy_from_slice(bytemuck::bytes_of(&vp));
            }
            let faces_ubo = self.probe_faces_ubo[slot].buffer;
            unsafe {
                device.cmd_update_buffer(cmd, faces_ubo, 0, &faces);
                // Sin IBL durante la captura: evita muestrear el cubemap que se está escribiendo.
                if input.scene.len() as u64 >= ibl_weight_offset + 4 {
                    device.cmd_update_buffer(
                        cmd,
                        self.scene_ubo[slot].buffer,
                        ibl_weight_offset,
                        &0f32.to_le_bytes(),
                    );
                }
            }
            buffer_barrier(device, cmd, faces_ubo);
            buffer_barrier(device, cmd, self.scene_ubo[slot].buffer);
            let views = [
                Some(self.targets.probe_slot_views[probe_slot as usize]),
                None,
                None,
                None,
                None,
            ];
            let depth = self.targets.probe_slot_depth_views[probe_slot as usize];
            begin_color_masked(
                device,
                cmd,
                &views,
                Some(depth),
                extent,
                true,
                PROBE_VIEW_MASK,
            );
            set_viewport(device, cmd, extent);
            let mut mv_pipes = pipes;
            mv_pipes.forward = forward;
            mv_pipes.skinned = skinned;
            self.draw_forward(slot, input, &mv_pipes, device, cmd, 0.0);
            unsafe { device.cmd_end_rendering(cmd) };
        } else {
            // Fallback (también si no hay multiview o si la sonda es MSAA): seis pases, cada uno
            // directamente en la vista de su capa del cubemap array (sin `copy_color`).
            for face in 0..6 {
                // Solo cambian 64 B de la view-proj y 4 B del parámetro de IBL: subir el UBO
                // completo por cara era redundante (la escena ya se escribió antes del pase).
                let layer = base_layer + face as u32;
                let face_view = Some(self.targets.probe_face_views[layer as usize]);
                let ubo = self.scene_ubo[slot].buffer;
                unsafe {
                    if input.scene.len() >= 64 {
                        let vp = cube_face_view_proj(capture.origin, face);
                        device.cmd_update_buffer(cmd, ubo, 0, bytemuck::bytes_of(&vp));
                    }
                    // Sin IBL durante la captura: evita muestrear el cubemap que se está escribiendo.
                    if input.scene.len() as u64 >= ibl_weight_offset + 4 {
                        device.cmd_update_buffer(cmd, ubo, ibl_weight_offset, &0f32.to_le_bytes());
                    }
                }
                buffer_barrier(device, cmd, ubo);
                if let Some(ms) = probe_ms {
                    let ms_views = ms.views();
                    let ms_depth = ms.depth.view;
                    let resolve = [face_view, None, None, None, None];
                    begin_color_ms(device, cmd, &ms_views, ms_depth, &resolve, extent, true);
                    set_viewport(device, cmd, extent);
                    self.draw_forward(slot, input, &pipes, device, cmd, 0.0);
                    unsafe { device.cmd_end_rendering(cmd) };
                } else {
                    let views = [face_view, None, None, None, None];
                    let depth = self.targets.probe_face_depth_views[layer as usize];
                    begin_color(device, cmd, &views, Some(depth), extent, true);
                    set_viewport(device, cmd, extent);
                    self.draw_forward(slot, input, &pipes, device, cmd, 0.0);
                    unsafe { device.cmd_end_rendering(cmd) };
                }
            }
        }
        if !input.scene.is_empty() {
            unsafe { device.cmd_update_buffer(cmd, self.scene_ubo[slot].buffer, 0, input.scene) };
            buffer_barrier(device, cmd, self.scene_ubo[slot].buffer);
        }
        Ok(())
    }
}
