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
        crate::engine3d::vulkan::gpu::cmd_image_barrier(
            device,
            cmd,
            self.targets.normal.image,
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
        let views = [
            self.targets.probe_color.view,
            self.targets.probe_normal.view,
            self.targets.probe_albedo.view,
            self.targets.probe_world.view,
            self.targets.probe_velocity.view,
        ];
        const LIGHT_PARAMS_Y: usize = 64 * 4 + 16 * 2 + 16 + 64 + 4;
        let base_layer = (capture
            .slot
            .min(crate::engine3d::probe_registry::MAX_PROBES - 1) as u32)
            * 6;
        // La captura puede usar el G-buffer MS de sonda o el camino 1× según el tier actual.
        // Se decide una vez por sonda, no por cara.
        let probe_ms = self
            .targets
            .probe_ms
            .as_ref()
            .filter(|ms| ms.samples > 1 && ms.samples == self.msaa_samples);
        if let Some(ms) = probe_ms {
            log::debug!("[msaa] captura de sonda a {}x", ms.samples);
        }
        for face in 0..6 {
            let mut scene = input.scene.to_vec();
            let vp = cube_face_view_proj(capture.origin, face);
            if scene.len() >= 64 {
                scene[..64].copy_from_slice(bytemuck::bytes_of(&vp));
            }
            // Sin IBL durante la captura: evita muestrear el cubemap que se está escribiendo.
            if scene.len() >= LIGHT_PARAMS_Y + 4 {
                scene[LIGHT_PARAMS_Y..LIGHT_PARAMS_Y + 4].copy_from_slice(&0f32.to_le_bytes());
            }
            if !scene.is_empty() {
                unsafe { device.cmd_update_buffer(cmd, self.scene_ubo[slot].buffer, 0, &scene) };
                buffer_barrier(device, cmd, self.scene_ubo[slot].buffer);
            }
            if let Some(ms) = probe_ms {
                let ms_views = ms.views();
                begin_color_ms(device, cmd, &ms_views, ms.depth.view, &views, extent, true);
                set_viewport(device, cmd, extent);
                let ms_pipes = self.forward_pipes(*pipes);
                self.draw_forward(slot, input, &ms_pipes, device, cmd, 0.0);
                unsafe { device.cmd_end_rendering(cmd) };
            } else {
                begin_color(
                    device,
                    cmd,
                    &views,
                    Some(self.targets.probe_depth.view),
                    extent,
                    true,
                );
                set_viewport(device, cmd, extent);
                self.draw_forward(slot, input, pipes, device, cmd, 0.0);
                unsafe { device.cmd_end_rendering(cmd) };
            }
            // `copy_color` escribe en `base_array_layer`, así que cada cara de cada sonda ocupa su
            // propia capa (slot * 6 + cara) en vez de pisar la ranura 0.
            copy_color(
                device,
                cmd,
                &self.targets.probe_color,
                &self.targets.probe,
                extent,
                base_layer + face as u32,
            );
        }
        if !input.scene.is_empty() {
            unsafe { device.cmd_update_buffer(cmd, self.scene_ubo[slot].buffer, 0, input.scene) };
            buffer_barrier(device, cmd, self.scene_ubo[slot].buffer);
        }
        Ok(())
    }
}
