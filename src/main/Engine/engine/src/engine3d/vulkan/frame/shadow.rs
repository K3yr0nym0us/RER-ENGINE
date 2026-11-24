use super::*;

/// Lado del cuadrado central de cada capa que copia la sonda `RER_SHADOW_DEBUG`.
const SHADOW_PROBE_SIDE: u32 = 32;
/// Cada cuántas sondas se registra: la copia es barata, el log no debe inundar la consola.
const SHADOW_PROBE_REPORT_EVERY: u32 = 30;

/// Limpia las 4 capas del array en una sola instancia de dynamic rendering. `begin_depth`
/// (en `commands.rs`) fija `layer_count = 1`: con la vista de array eso solo limpiaría la
/// capa 0, y las capas que el tier no dibuja quedarían con contenido indefinido.
fn begin_depth_layers(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
    layers: u32,
) {
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(DEPTH_ATTACHMENT)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        });
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(layers)
        .depth_attachment(&depth);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

/// Barrera sobre las 4 capas del array de sombras. `transition_depth` (en `commands.rs`)
/// cubre una sola capa; aquí el descriptor apunta al array entero, así que el cambio de
/// layout tiene que abarcar todas las capas a la vez o el muestreo queda indefinido.
fn transition_shadow_layers(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    current: &mut vk::ImageLayout,
    want: vk::ImageLayout,
) {
    if *current == want {
        return;
    }
    let barrier = crate::engine3d::vulkan::gpu::image_memory_barrier2(
        image,
        vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: SHADOW_LAYERS,
        },
        *current,
        want,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
        vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
    );
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    unsafe { device.cmd_pipeline_barrier2(cmd, &dependency) };
    *current = want;
}

/// Bloque `CascadePush` de 80 B: `mat4 view_proj` en 0..64 y `vec4 joint_base` en 64..80
/// (`x` = base de la paleta del caster skinned, `y/z/w` = 0). La matriz se lee del UBO de
/// escena ya subido por `record`: el pase NO reescribe el UBO de escena entre draws.
fn cascade_push(scene: &[u8], cascade: usize, joint_base: u32) -> [u8; 80] {
    let mut push = [0u8; 80];
    // Identidad por si el bloque viniera corto: una matriz cero daría gl_Position.w = 0.
    let identity: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for (chunk, value) in push[..64]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(identity.into_iter().flatten())
    {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    let base = std::mem::offset_of!(SceneUniforms, cascade_view_proj) + cascade * 64;
    if let Some(bytes) = scene.get(base..base + 64) {
        push[..64].copy_from_slice(bytes);
    }
    push[64..68].copy_from_slice(&joint_base.to_le_bytes());
    push
}

impl FrameRenderer {
    /// Deja el array de sombras entero en `DEPTH_STENCIL_READ_ONLY_OPTIMAL`, que es el layout
    /// que declara el descriptor del `sampler2DArrayShadow`.
    pub(super) fn shadow_read_only(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        transition_shadow_layers(
            device,
            cmd,
            self.targets.shadow.image,
            &mut self.shadow_layout,
            DEPTH_READ_ONLY,
        );
    }

    /// Dibuja cada cascada en su propia capa del array de sombras.
    ///
    /// La VP de cada cascada viaja en push constants (0..64) junto con la base de paleta del
    /// caster skinned (64..80), en UN solo `cmd_push_constants` por draw. El UBO de escena se
    /// sube una sola vez en `record` y aquí no se toca: el forward necesita las 4 matrices
    /// completas para elegir cascada por profundidad.
    ///
    /// Las barreras ATTACHMENT↔READ_ONLY abarcan las 4 capas: pasar de adjunto a lectura capa a
    /// capa dejaría las demás en un layout que el descriptor no declara (contenido indefinido).
    /// Antes de dibujar se limpia el array ENTERO a 1.0: una capa que el tier no usa no puede
    /// quedar con basura, porque el forward la indexa en cuanto cambia de cascada.
    pub(super) fn shadow_pass(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let available = self
            .targets
            .shadow
            .layer_view_count()
            .min(MAX_SHADOW_CASCADES);
        let requested = shadow_cascade_override()
            .unwrap_or(input.shadow_cascades)
            .clamp(1, MAX_SHADOW_CASCADES);
        let count = requested.min(available.max(1));
        if available < requested {
            // Sin vistas por capa, TODAS las cascadas escribirían la capa 0 y las lejanas se
            // muestrearían sin pintar: la escena entera queda en sombra salvo la primera cascada.
            log::error!(
                "[sombras] el array tiene {available} vistas por capa para {requested} cascadas pedidas: se dibuja 1 sola cascada"
            );
        }
        let shadow_image = self.targets.shadow.image;
        let extent = self.targets.shadow.extent;

        transition_shadow_layers(
            device,
            cmd,
            shadow_image,
            &mut self.shadow_layout,
            DEPTH_ATTACHMENT,
        );
        // Limpieza de las 4 capas sin dibujar nada (1.0 = no ocluye). `layer_count = 4`
        // con la vista de array: `begin_depth` de `commands.rs` solo cubriría la capa 0.
        begin_depth_layers(device, cmd, self.targets.shadow.view, extent, SHADOW_LAYERS);
        unsafe { device.cmd_end_rendering(cmd) };
        transition_shadow_layers(
            device,
            cmd,
            shadow_image,
            &mut self.shadow_layout,
            DEPTH_READ_ONLY,
        );

        for cascade in 0..count {
            let Some(layer_view) = self.targets.shadow.layer_view(cascade as u32) else {
                break;
            };
            // Dependencia explícita entre instancias consecutivas de dynamic rendering: sin
            // esto el clear o la cascada anterior se solapan con la actual y la capa queda con
            // profundidad ~0 ("todo en sombra salvo un círculo alrededor de la cámara").
            transition_shadow_layers(
                device,
                cmd,
                shadow_image,
                &mut self.shadow_layout,
                DEPTH_ATTACHMENT,
            );
            begin_depth(device, cmd, layer_view, extent);
            set_viewport(device, cmd, extent);
            self.draw_depth(
                slot,
                input,
                cascade,
                pipes.shadow,
                pipes.shadow_skinned,
                self.layouts[2],
                self.layouts[3],
                device,
                cmd,
            );
            unsafe { device.cmd_end_rendering(cmd) };
            transition_shadow_layers(
                device,
                cmd,
                shadow_image,
                &mut self.shadow_layout,
                DEPTH_READ_ONLY,
            );
        }
        // Transición final de las 4 capas a READ_ONLY (las que no se dibujaron también).
        transition_shadow_layers(
            device,
            cmd,
            shadow_image,
            &mut self.shadow_layout,
            DEPTH_READ_ONLY,
        );
        if shadow_debug_enabled() {
            self.probe_shadow_layers(count, device, cmd);
        }
        Ok(())
    }

    fn draw_depth(
        &self,
        slot: usize,
        input: &FrameInput<'_>,
        cascade: usize,
        pipeline: vk::Pipeline,
        skinned_pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        skinned_layout: vk::PipelineLayout,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) {
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[self.sets.scene[slot]],
                &[],
            );
        }
        for (index, batch) in input.shadow_static_batches.iter().enumerate() {
            let push = cascade_push(input.scene, cascade, 0);
            unsafe {
                device.cmd_push_constants(cmd, layout, vk::ShaderStageFlags::VERTEX, 0, &push);
            }
            let offset = self.shadow_offsets.get(index).copied().unwrap_or(0);
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
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, skinned_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                skinned_layout,
                0,
                &[
                    self.sets.scene[slot],
                    self.sets.empty,
                    self.sets.joints[slot],
                ],
                &[],
            );
        }
        for (index, batch) in input.shadow_skinned_batches.iter().enumerate() {
            let joint_base = self.shadow_joint_bases.get(index).copied().unwrap_or(0);
            let push = cascade_push(input.scene, cascade, joint_base);
            unsafe {
                device.cmd_push_constants(
                    cmd,
                    skinned_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    &push,
                );
            }
            let offset = self.shadow_skinned_offsets.get(index).copied().unwrap_or(0);
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

    /// Copia el cuadrado central de cada capa del shadow map y registra estadísticas por capa.
    ///
    /// Cada capa se limpia a 1.0, así que `geometria` cuenta los texeles por debajo de 0.999:
    /// 0 significa que ESA capa no se dibujó, y un min/max sin sentido que se dibujó con la
    /// matriz de otra cascada. El informe corresponde al frame ANTERIOR (la copia del actual
    /// todavía no terminó), suficiente para diagnóstico.
    fn probe_shadow_layers(
        &mut self,
        cascades: usize,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) {
        let extent = self.targets.shadow.extent;
        let image = self.targets.shadow.image;
        let side = SHADOW_PROBE_SIDE
            .min(extent.width)
            .min(extent.height)
            .max(1);
        let stride = u64::from(side) * u64::from(side) * 4;
        let needed = stride * u64::from(SHADOW_LAYERS);
        if self
            .shadow_probe
            .as_ref()
            .is_none_or(|probe| probe.size < needed)
        {
            match self.gpu.create_buffer(
                needed,
                vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::CpuToGpu,
                "shadow-probe",
            ) {
                Ok(buffer) => self.shadow_probe = Some(buffer),
                Err(error) => {
                    log::warn!("[sombras-debug] sin buffer de sonda: {error}");
                    return;
                }
            }
        }
        let Some(probe_handle) = self.shadow_probe.as_ref().map(|probe| probe.buffer) else {
            return;
        };

        self.shadow_probe_tick = self.shadow_probe_tick.wrapping_add(1);
        if self.shadow_probe_tick % SHADOW_PROBE_REPORT_EVERY == 1 {
            self.report_shadow_probe(cascades, extent, side, stride);
        }

        transition_shadow_layers(
            device,
            cmd,
            image,
            &mut self.shadow_layout,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let offset_x = ((extent.width - side) / 2) as i32;
        let offset_y = ((extent.height - side) / 2) as i32;
        let regions: Vec<vk::BufferImageCopy> = (0..SHADOW_LAYERS)
            .map(|layer| {
                vk::BufferImageCopy::default()
                    .buffer_offset(u64::from(layer) * stride)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::DEPTH,
                        mip_level: 0,
                        base_array_layer: layer,
                        layer_count: 1,
                    })
                    .image_offset(vk::Offset3D {
                        x: offset_x,
                        y: offset_y,
                        z: 0,
                    })
                    .image_extent(vk::Extent3D {
                        width: side,
                        height: side,
                        depth: 1,
                    })
            })
            .collect();
        unsafe {
            device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                probe_handle,
                &regions,
            );
        }
        transition_shadow_layers(device, cmd, image, &mut self.shadow_layout, DEPTH_READ_ONLY);
    }

    /// Vuelca las estadísticas de la última sonda copiada (frame anterior).
    fn report_shadow_probe(&self, cascades: usize, extent: vk::Extent2D, side: u32, stride: u64) {
        use std::fmt::Write;

        let Some(bytes) = self
            .shadow_probe
            .as_ref()
            .and_then(|probe| probe.mapped_bytes())
        else {
            return;
        };
        let mut summary = String::new();
        for layer in 0..SHADOW_LAYERS {
            let start = (u64::from(layer) * stride) as usize;
            let Some(window) = bytes.get(start..start + stride as usize) else {
                continue;
            };
            let (mut min, mut max) = (1.0f32, 0.0f32);
            let (mut filled, mut total) = (0u32, 0u32);
            for chunk in window.as_chunks::<4>().0 {
                let value = f32::from_le_bytes(*chunk);
                if !value.is_finite() {
                    continue;
                }
                total += 1;
                min = min.min(value);
                max = max.max(value);
                if value < 0.999 {
                    filled += 1;
                }
            }
            let _ = write!(
                summary,
                " | capa {layer}: [{min:.4}..{max:.4}] geometria={filled}/{total}"
            );
        }
        log::warn!(
            "[sombras-debug] mapa {}x{}, sonda central {side}x{side}, cascadas={cascades}{summary}",
            extent.width,
            extent.height
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_push_puts_matrix_in_0_64_and_joint_base_in_64_80() {
        let mut scene = vec![0u8; std::mem::size_of::<SceneUniforms>()];
        let base = std::mem::offset_of!(SceneUniforms, cascade_view_proj) + 64;
        for (i, byte) in scene[base..base + 64].iter_mut().enumerate() {
            *byte = (i + 1) as u8;
        }
        let push = cascade_push(&scene, 1, 7);
        assert_eq!(&push[..64], &scene[base..base + 64]);
        assert_eq!(&push[64..68], &7u32.to_le_bytes());
        assert!(push[68..80].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn cascade_push_falls_back_to_identity_with_a_short_scene() {
        let push = cascade_push(&[], 3, 0);
        let identity: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let expected: Vec<u8> = identity
            .into_iter()
            .flatten()
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(&push[..64], expected.as_slice());
    }
}
