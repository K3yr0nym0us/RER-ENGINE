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
        let mode_key = u8::from(input.ssr) | (u8::from(input.raytracing) << 1);
        if self.last_reflection_mode != mode_key {
            // Al cambiar de sistema (o de Off a activo) cambia el historial de destino.
            self.taa_history_valid = false;
            self.last_reflection_mode = mode_key;
        }
        if input.reflections && !input.raytracing {
            // Sin trazado, el composite debe caer en SSR: limpia el reflejo RT anterior.
            clear_image_color(
                device,
                cmd,
                self.targets.reflection.image,
                self.rt_extent(),
                [0.0, 0.0, 0.0, 0.0],
            );
        }
        if input.reflections && !input.ssr {
            // Con RT el composite no debe leer un SSR viejo: roughness baja fuerza SSR en el
            // shader, así que dejamos la imagen limpia para que aporte cero.
            clear_image_color(
                device,
                cmd,
                self.targets.ssr.image,
                self.ssr_extent(),
                [0.0, 0.0, 0.0, 0.0],
            );
        }
        // Parámetros del preset SSR (32 bytes, ver ssr.frag).
        let ssr_params = [
            input.ssr_preset.march_steps as f32,
            input.ssr_preset.binary_search_steps as f32,
            input.ssr_preset.max_distance,
            input.ssr_preset.thickness,
            if input.ssr_preset.adaptive_thickness { 1.0 } else { 0.0 },
            input.ssr_preset.roughness_support,
            input.ssr_preset.edge_fade,
            0.0f32,
        ];
        let ssr_push = bytemuck::bytes_of(&ssr_params).to_vec();
        let source = if input.reflections {
            if input.ssr {
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
                write_combined(
                    device,
                    self.sets.ssr[slot],
                    5,
                    self.targets.albedo_out.view,
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
                    self.ssr_extent(),
                    &ssr_push,
                );
            }
            // En SSR se filtra la imagen SSR; en RT se filtra la imagen trazada.
            let denoise_source = if input.ssr {
                self.targets.ssr.view
            } else {
                self.targets.reflection.view
            };
            write_combined(
                device,
                self.sets.denoise[slot],
                0,
                denoise_source,
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
            // Cada sistema usa su propio perfil de denoise (nunca se mezclan).
            let passes = if input.raytracing {
                input.rtx.denoise_passes.max(1)
            } else {
                input.ssr_preset.denoise_passes.max(1)
            };
            let step = if input.raytracing {
                input.rtx.denoise_step.max(0.25)
            } else {
                input.ssr_preset.denoise_step.max(0.25)
            }
            .to_le_bytes();
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
            // Pasadas extra del tier RTX (ping-pong). CADA pasada usa su propio set: actualizar
            // un set ya enlazado en este command buffer invalidaba el buffer completo.
            let passes = passes.min(MAX_EXTRA_DENOISE_PASSES as u32 + 1) as usize;
            let mut denoised = self.targets.denoise.view;
            let mut denoised_image = self.targets.denoise.image;
            for pass in 1..passes {
                // La pasada anterior escribió esta imagen como attachment; hay que hacer
                // visible esa escritura antes de muestrearla. Sin barrera aparecen bloques
                // negros intermitentes al encadenar 3-4 pasadas (High/Ultra).
                crate::engine3d::vulkan::gpu::cmd_image_barrier(
                    device,
                    cmd,
                    denoised_image,
                    vk::ImageAspectFlags::COLOR,
                    1,
                    1,
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::GENERAL,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                        | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                );
                let set = self.sets.denoise_extra[slot][pass - 1];
                write_combined(device, set, 0, denoised, clamp, vk::ImageLayout::GENERAL)?;
                write_combined(
                    device,
                    set,
                    1,
                    self.targets.depth.view,
                    clamp,
                    vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                )?;
                write_combined(
                    device,
                    set,
                    2,
                    self.targets.normal.view,
                    clamp,
                    vk::ImageLayout::GENERAL,
                )?;
                denoised = if pass % 2 == 1 {
                    self.targets.denoise2.view
                } else {
                    self.targets.denoise.view
                };
                denoised_image = if pass % 2 == 1 {
                    self.targets.denoise2.image
                } else {
                    self.targets.denoise.image
                };
                draw_fullscreen(
                    device,
                    cmd,
                    pipes.denoise,
                    self.layouts[8],
                    set,
                    denoised,
                    extent,
                    &step,
                );
            }
            // El composite lee el resultado final del denoise: misma barrera que entre pasadas.
            crate::engine3d::vulkan::gpu::cmd_image_barrier(
                device,
                cmd,
                denoised_image,
                vk::ImageAspectFlags::COLOR,
                1,
                1,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_WRITE,
                vk::AccessFlags::SHADER_READ,
            );
            write_combined(
                device,
                self.sets.composite[slot],
                0,
                self.targets.scene.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            // En SSR, binding 1 debe quedar limpio para que el composite elija la SSR filtrada
            // del binding 2 (y sepa que está en modo SSR mirando las alfas). En RT es al revés.
            let composite_rt = if input.ssr {
                self.targets.reflection.view
            } else {
                denoised
            };
            write_combined(
                device,
                self.sets.composite[slot],
                1,
                composite_rt,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            let composite_ssr = if input.ssr {
                denoised
            } else {
                self.targets.ssr.view
            };
            write_combined(
                device,
                self.sets.composite[slot],
                2,
                composite_ssr,
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
            write_combined(
                device,
                self.sets.composite[slot],
                4,
                self.targets.world.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            // binding 5: albedo.a codifica el Fresnel de transparentes para el composite.
            write_combined(
                device,
                self.sets.composite[slot],
                5,
                self.targets.albedo_out.view,
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
            let desired_blend = if input.raytracing {
                input.rtx.temporal_blend
            } else if input.reflections {
                input.ssr_preset.temporal_blend
            } else {
                input.taa_blend
            };
            let blend = if self.taa_history_valid {
                desired_blend
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
        let present_source = if input.debug_rt_view && input.raytracing {
            self.targets.reflection.view
        } else if input.debug_ssr_view && input.ssr {
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
        // el editor la reduce al rectángulo del viewport. Con SSAA los targets están a `extent`
        // (supersampleado) pero el swapchain sigue a `frame.extent` (nativo): el shader de
        // `present` muestrea la imagen de origen con filtrado bilineal y la reduce al tamaño real.
        let area = vk::Rect2D {
            offset: frame.offset,
            extent: frame.extent,
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
