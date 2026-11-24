use super::*;
use bytemuck::{Pod, Zeroable};

const MODE_SSR: f32 = 1.0;
const MODE_RT: f32 = 2.0;
const TAA_MODE_SSR: f32 = 2.0;
const TAA_MODE_NORMAL: f32 = 1.0;
const SSR_ORIGIN_BIAS_M: f32 = 0.02;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SsrPush {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    d: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PassPush {
    params: [f32; 4],
}

fn scene_near_far(scene: &[u8]) -> (f32, f32) {
    let offset = SceneUniforms::DEPTH_PLANE_OFFSET as usize;
    let mut near = 0.1;
    let mut far = 1000.0;
    if scene.len() >= offset + 8 {
        near = f32::from_le_bytes(scene[offset..offset + 4].try_into().unwrap_or_default());
        far = f32::from_le_bytes(scene[offset + 4..offset + 8].try_into().unwrap_or_default());
    }
    if !near.is_finite() || near <= 0.0 {
        near = 0.1;
    }
    if !far.is_finite() || far <= near {
        far = (near + 1000.0).max(near * 2.0);
    }
    (near, far)
}

/// Sincronización explícita para pases fullscreen: dynamic rendering no sustituye esta barrera.
fn color_write_to_shader_read(device: &ash::Device, cmd: vk::CommandBuffer, image: vk::Image) {
    crate::engine3d::vulkan::gpu::cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        vk::AccessFlags::SHADER_READ,
    );
}

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
        let depth_sampler = self.samplers[3];
        let (near, far) = scene_near_far(input.scene);
        // RT tiene prioridad explícita, igual que en el forward: flags inconsistentes no activan SSR.
        let ssr = input.ssr && !input.raytracing;
        let mode_key = u8::from(ssr) | (u8::from(input.raytracing) << 1);
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
        if input.reflections && !ssr {
            // Con RT el composite no debe leer un SSR viejo; el push selecciona RT explícitamente.
            clear_image_color(
                device,
                cmd,
                self.targets.ssr.image,
                self.ssr_extent(),
                [0.0, 0.0, 0.0, 0.0],
            );
        }
        // Parámetros SSR (64 bytes, ver ssr.frag). `max_distance` y `thickness` están en metros.
        let min_thickness = input.ssr_preset.thickness.max(0.001);
        let max_thickness = (input.ssr_preset.thickness * 4.0)
            .max(0.05)
            .max(min_thickness);
        let width = extent.width as f32;
        let height = extent.height as f32;
        let ssr_push = SsrPush {
            a: [
                input.ssr_preset.march_steps as f32,
                input.ssr_preset.binary_search_steps as f32,
                input.ssr_preset.max_distance,
                min_thickness,
            ],
            b: [
                if input.ssr_preset.adaptive_thickness {
                    1.0
                } else {
                    0.0
                },
                max_thickness,
                input.ssr_preset.roughness_support,
                input.ssr_preset.edge_fade,
            ],
            c: [near, far, SSR_ORIGIN_BIAS_M, width],
            d: [height, width / height.max(1.0), 0.0, 0.0],
        };
        let source = if input.reflections {
            if ssr {
                // Forward color outputs -> fragment SSR. La depth ya transita a READ_ONLY en
                // `forward_pass`; aquí sincronizamos los adjuntos de color que muestrea SSR.
                color_write_to_shader_read(device, cmd, self.targets.scene.image);
                if let Some(normal) = self.targets.normal.as_ref() {
                    color_write_to_shader_read(device, cmd, normal.image);
                }
                if let Some(albedo) = self.targets.albedo_out.as_ref() {
                    color_write_to_shader_read(device, cmd, albedo.image);
                }
                write_combined(
                    device,
                    self.sets.ssr[slot],
                    0,
                    self.targets.depth.view,
                    depth_sampler,
                    vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                )?;
                write_combined(
                    device,
                    self.sets.ssr[slot],
                    1,
                    self.sampled_normal(),
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
                    4,
                    self.sampled_albedo(),
                    clamp,
                    vk::ImageLayout::GENERAL,
                )?;
                draw_fullscreen(
                    device,
                    cmd,
                    pipes.ssr,
                    self.ssr_layout,
                    self.sets.ssr[slot],
                    self.targets.ssr.view,
                    self.ssr_extent(),
                    bytemuck::bytes_of(&ssr_push),
                );
                // SSR -> denoise, sin depender de la sincronización implícita del render pass.
                color_write_to_shader_read(device, cmd, self.targets.ssr.image);
            }
            // En SSR se filtra la imagen SSR; en RT se filtra la imagen trazada.
            let denoise_source = if ssr {
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
                self.sampled_normal(),
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            // Cada sistema usa su propio perfil de denoise (nunca se mezclan). El modo del push
            // permite que SSR corrija la comparación de profundidad sin cambiar el filtro RT.
            let passes = if ssr {
                input.ssr_preset.denoise_passes.max(1)
            } else {
                input.rtx.denoise_passes.max(1)
            };
            let step = if ssr {
                input.ssr_preset.denoise_step.max(0.25)
            } else {
                input.rtx.denoise_step.max(0.25)
            };
            let denoise_push = PassPush {
                params: [step, near, far, if ssr { MODE_SSR } else { MODE_RT }],
            };
            draw_fullscreen(
                device,
                cmd,
                pipes.denoise,
                self.layouts[8],
                self.sets.denoise[slot],
                self.targets.denoise.view,
                extent,
                bytemuck::bytes_of(&denoise_push),
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
                    self.sampled_normal(),
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
                    bytemuck::bytes_of(&denoise_push),
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
            // El push decide el modo; mantenemos RT limpio en SSR y SSR limpio en RT para que
            // las texturas no seleccionadas no puedan contaminar el composite.
            let composite_rt = if ssr {
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
            let composite_ssr = if ssr { denoised } else { self.targets.ssr.view };
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
                self.sampled_normal(),
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.composite[slot],
                4,
                self.sampled_world(),
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            // binding 5: albedo.a codifica el Fresnel de transparentes para el composite.
            write_combined(
                device,
                self.sets.composite[slot],
                5,
                self.sampled_albedo(),
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            let composite_push = PassPush {
                params: [if ssr { MODE_SSR } else { MODE_RT }, 0.0, 0.0, 0.0],
            };
            draw_fullscreen(
                device,
                cmd,
                pipes.composite,
                self.layouts[6],
                self.sets.composite[slot],
                self.targets.history[slot].view,
                extent,
                bytemuck::bytes_of(&composite_push),
            );
            if ssr {
                // composite -> TAA (o -> present si TAA está apagado), solo en la ruta SSR.
                color_write_to_shader_read(device, cmd, self.targets.history[slot].image);
            }
            self.targets.history[slot].view
        } else {
            self.targets.scene.view
        };
        let present_source = if input.taa_enabled {
            let history = self.history_slot ^ 1;
            let (history_view, target, target_image) = if input.reflections {
                (
                    self.targets.refl_history[history].view,
                    self.targets.refl_history[slot].view,
                    self.targets.refl_history[slot].image,
                )
            } else {
                (
                    self.targets.history[history].view,
                    self.targets.history[slot].view,
                    self.targets.history[slot].image,
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
                self.sampled_velocity(),
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            // Bindings 3/4 ya existen en `full/taa`; no se inserta ni desplaza ningún descriptor.
            write_combined(
                device,
                self.sets.taa[slot],
                3,
                self.targets.depth.view,
                clamp,
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            )?;
            write_combined(
                device,
                self.sets.taa[slot],
                4,
                self.sampled_normal(),
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
            // SSR usa el blend TAA de escena. RT conserva exactamente su preset temporal.
            let desired_blend = if ssr {
                input.taa_blend
            } else if input.raytracing {
                input.rtx.temporal_blend
            } else {
                input.taa_blend
            };
            let blend = if self.taa_history_valid {
                desired_blend
            } else {
                0.0
            };
            let taa_push = PassPush {
                params: [
                    blend,
                    if ssr { TAA_MODE_SSR } else { TAA_MODE_NORMAL },
                    near,
                    far,
                ],
            };
            draw_fullscreen(
                device,
                cmd,
                pipes.taa,
                self.layouts[7],
                self.sets.taa[slot],
                target,
                extent,
                bytemuck::bytes_of(&taa_push),
            );
            if ssr {
                // TAA -> present. No se añade ninguna barrera nueva a la ruta RT.
                color_write_to_shader_read(device, cmd, target_image);
            }
            target
        } else {
            source
        };
        self.taa_history_valid = input.taa_enabled;
        // Vista de debug del editor: se presenta la imagen SSR/trazada en crudo, sin composite ni
        // TAA encima, para ver qué devuelve realmente el pase de reflejos.
        let present_source = if input.debug_rt_view && input.raytracing {
            self.targets.reflection.view
        } else if input.debug_ssr_view && ssr {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_constant_sizes_match_pipeline_ranges() {
        assert_eq!(std::mem::size_of::<SsrPush>(), SSR_PUSH_BYTES as usize);
        assert_eq!(
            std::mem::size_of::<PassPush>(),
            REFLECTION_PUSH_BYTES as usize
        );
        assert_eq!(MODE_SSR, 1.0);
        assert_eq!(MODE_RT, 2.0);
        assert_eq!(TAA_MODE_SSR, 2.0);
        assert_eq!(TAA_MODE_NORMAL, 1.0);
    }

    #[test]
    fn scene_near_far_reads_depth_plane_offset() {
        let offset = SceneUniforms::DEPTH_PLANE_OFFSET as usize;
        let mut scene = vec![0u8; offset + 8];
        scene[offset..offset + 4].copy_from_slice(&0.25f32.to_le_bytes());
        scene[offset + 4..offset + 8].copy_from_slice(&1800.0f32.to_le_bytes());
        assert_eq!(scene_near_far(&scene), (0.25, 1800.0));
    }

    #[test]
    fn scene_near_far_has_safe_fallbacks() {
        assert_eq!(scene_near_far(&[]), (0.1, 1000.0));
        let offset = SceneUniforms::DEPTH_PLANE_OFFSET as usize;
        let mut scene = vec![0u8; offset + 8];
        scene[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        scene[offset + 4..offset + 8].copy_from_slice(&0.0f32.to_le_bytes());
        let (near, far) = scene_near_far(&scene);
        assert_eq!(near, 0.1);
        assert!(far > near);
    }
}
