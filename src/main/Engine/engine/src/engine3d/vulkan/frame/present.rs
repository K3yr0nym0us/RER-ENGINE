use super::*;

impl FrameRenderer {
    pub(super) fn resolve_present(
        &mut self,
        slot: usize,
        frame: &FrameTarget,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let extent = self.targets.extent;
        let clamp = self.samplers[2];
        if self.last_reflections != input.reflections {
            // Al activar/desactivar reflejos cambia el historial de destino; no se puede
            // mezclar con el de la modalidad anterior.
            self.taa_history_valid = false;
            self.last_reflections = input.reflections;
        }
        if input.reflections && !input.raytracing {
            // Sin trazado, el composite debe caer en SSR: limpia el reflejo RT anterior.
            clear_image_color(
                device,
                cmd,
                self.targets.reflection.image,
                extent,
                [0.0, 0.0, 0.0, 0.0],
            );
        }
        let source = if input.reflections {
            write_combined(
                device,
                self.sets.ssr[slot],
                0,
                self.targets.depth.view,
                clamp,
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            )?;
            write_combined(
                device,
                self.sets.ssr[slot],
                1,
                self.targets.normal.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.ssr[slot],
                2,
                self.targets.scene.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            draw_fullscreen(
                device,
                cmd,
                pipes.ssr,
                self.layouts[6],
                self.sets.ssr[slot],
                self.targets.ssr.view,
                extent,
                &[],
            );
            write_combined(
                device,
                self.sets.denoise[slot],
                0,
                self.targets.reflection.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.denoise[slot],
                1,
                self.targets.depth.view,
                clamp,
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            )?;
            write_combined(
                device,
                self.sets.denoise[slot],
                2,
                self.targets.normal.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            let step = 1.0f32.to_le_bytes();
            draw_fullscreen(
                device,
                cmd,
                pipes.denoise,
                self.layouts[8],
                self.sets.denoise[slot],
                self.targets.denoise.view,
                extent,
                &step,
            );
            write_combined(
                device,
                self.sets.composite[slot],
                0,
                self.targets.scene.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.composite[slot],
                1,
                self.targets.denoise.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.composite[slot],
                2,
                self.targets.ssr.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.composite[slot],
                3,
                self.targets.normal.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            draw_fullscreen(
                device,
                cmd,
                pipes.composite,
                self.layouts[6],
                self.sets.composite[slot],
                self.targets.history[slot].view,
                extent,
                &[],
            );
            self.targets.history[slot].view
        } else {
            self.targets.scene.view
        };
        let present_source = if input.taa_enabled {
            let history = self.history_slot ^ 1;
            let (history_view, target) = if input.reflections {
                (
                    self.targets.refl_history[history].view,
                    self.targets.refl_history[slot].view,
                )
            } else {
                (
                    self.targets.history[history].view,
                    self.targets.history[slot].view,
                )
            };
            write_combined(
                device,
                self.sets.taa[slot],
                0,
                source,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.taa[slot],
                1,
                history_view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.taa[slot],
                2,
                self.targets.velocity.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            let mut push = [0u8; 8];
            let blend = if self.taa_history_valid {
                input.taa_blend
            } else {
                0.0
            };
            push[..4].copy_from_slice(&blend.to_le_bytes());
            push[4..8].copy_from_slice(&1.0f32.to_le_bytes());
            draw_fullscreen(
                device,
                cmd,
                pipes.taa,
                self.layouts[7],
                self.sets.taa[slot],
                target,
                extent,
                &push,
            );
            target
        } else {
            source
        };
        self.taa_history_valid = input.taa_enabled;
        // Vista de debug del editor: se presenta la imagen SSR/trazada en crudo, sin composite ni
        // TAA encima, para ver qué devuelve realmente el pase de reflejos.
        let present_source = if input.debug_ssr_view && input.reflections {
            self.targets.ssr.view
        } else {
            present_source
        };
        write_combined(
            device,
            self.sets.present[slot],
            0,
            present_source,
            clamp,
            vk::ImageLayout::GENERAL,
        )?;
        crate::engine3d::vulkan::gpu::cmd_image_barrier(
            device,
            cmd,
            frame.color_image,
            vk::ImageAspectFlags::COLOR,
            1,
            1,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags::empty(),
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        );
        // Área del swapchain donde se dibuja la escena. En standalone es toda la imagen;
        // el editor la reduce al rectángulo del viewport.
        let area = vk::Rect2D {
            offset: frame.offset,
            extent,
        };
        begin_swapchain(device, cmd, frame.color_view, area);
        set_viewport_rect(device, cmd, area);
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipes.present);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layouts[6],
                0,
                &[self.sets.present[slot]],
                &[],
            );
            device.cmd_draw(cmd, 3, 1, 0, 0);
        }
        unsafe { device.cmd_end_rendering(cmd) };
        // Cada lote de gizmo actualiza su UBO con `cmd_update_buffer` + barrera, y esas llamadas
        // son ilegales dentro de una instancia de render pass dinámico (lo confirmaba la capa de
        // validación). Se graba una pasada por lote con `load_op = LOAD`, que conserva lo ya
        // dibujado en el swapchain.
        for gizmo in input.gizmos {
            if gizmo.vertex_count == 0 {
                continue;
            }
            unsafe { device.cmd_update_buffer(cmd, self.gizmo_ubo[slot].buffer, 0, gizmo.uniform) };
            buffer_barrier(device, cmd, self.gizmo_ubo[slot].buffer);
            let pipeline = if gizmo.lines {
                pipes.gizmo_lines
            } else {
                pipes.gizmo
            };
            begin_swapchain_load(device, cmd, frame.color_view, area);
            set_viewport_rect(device, cmd, area);
            unsafe {
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.layouts[4],
                    0,
                    &[self.sets.gizmo[slot]],
                    &[],
                );
                device.cmd_bind_vertex_buffers(cmd, 0, &[gizmo.vertices.buffer], &[0]);
                device.cmd_draw(cmd, gizmo.vertex_count, 1, 0, 0);
                device.cmd_end_rendering(cmd);
            }
        }
        if let (Some(vertex), Some(index)) = (input.hud_vertex, input.hud_index) {
            begin_swapchain_load(device, cmd, frame.color_view, area);
            set_viewport_rect(device, cmd, area);
            unsafe {
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipes.hud);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.layouts[5],
                    0,
                    &[self.sets.hud[slot]],
                    &[],
                );
                device.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[vertex.buffer, self.instances[slot].buffer],
                    &[0, self.hud_offset],
                );
                device.cmd_bind_index_buffer(cmd, index.buffer, 0, vk::IndexType::UINT32);
                device.cmd_draw_indexed(
                    cmd,
                    input.hud_index_count,
                    input.hud_instance_count.max(1),
                    0,
                    0,
                    0,
                );
                device.cmd_end_rendering(cmd);
            }
        }
        Ok(())
    }
}
