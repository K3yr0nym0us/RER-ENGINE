use super::*;
use crate::engine3d::config_3d::reflection_graphics::RtxPreset;
use crate::engine3d::config_3d::world_bounds::SkyUniform;

pub(super) fn cube_face_view_proj(origin: [f32; 3], face: usize) -> [[f32; 4]; 4] {
    let dirs = [
        (glam::Vec3::X, glam::Vec3::NEG_Y),
        (glam::Vec3::NEG_X, glam::Vec3::NEG_Y),
        (glam::Vec3::Y, glam::Vec3::Z),
        (glam::Vec3::NEG_Y, glam::Vec3::NEG_Z),
        (glam::Vec3::Z, glam::Vec3::NEG_Y),
        (glam::Vec3::NEG_Z, glam::Vec3::NEG_Y),
    ];
    let (dir, up) = dirs[face];
    let eye = glam::Vec3::from_array(origin);
    let view = glam::Mat4::look_at_rh(eye, eye + dir, up);
    let proj = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.05, 200.0);
    (proj * view).to_cols_array_2d()
}

fn rt_camera_bytes_base(scene: &[u8], extent: vk::Extent2D) -> Vec<u8> {
    let mut out = vec![0u8; 112];
    if scene.len() >= 64 * 4 {
        let inv = &scene[64 * 3..64 * 4];
        out[..64].copy_from_slice(inv);
        if scene.len() >= 64 * 4 + 16 {
            out[64..80].copy_from_slice(&scene[64 * 4..64 * 4 + 16]);
        }
        if scene.len() >= 64 * 4 + 32 {
            out[96..112].copy_from_slice(&scene[64 * 4 + 16..64 * 4 + 32]);
        }
    }
    let resolution = [extent.width as f32, extent.height as f32, 0.0, 0.0];
    out[80..96].copy_from_slice(bytemuck::bytes_of(&resolution));
    out
}

/// Uniform del pase RT: cámara + cielo + parámetros del tier RTX (bloque `Camera`).
///
/// `rt_params` = (rayos por píxel, rebotes, distancia de trazado, roughness máxima).
pub(super) fn rt_camera_bytes(
    scene: &[u8],
    extent: vk::Extent2D,
    sky: &SkyUniform,
    rtx: &RtxPreset,
) -> Vec<u8> {
    let mut out = rt_camera_bytes_base(scene, extent);
    out.extend_from_slice(bytemuck::bytes_of(sky));
    let params = [
        rtx.rays_per_pixel as f32,
        rtx.bounces as f32,
        rtx.trace_distance,
        rtx.max_roughness,
    ];
    out.extend_from_slice(bytemuck::bytes_of(&params));
    out
}

fn rtx_uniform_params(rtx: &RtxPreset) -> [u8; 16] {
    let params = [
        rtx.rays_per_pixel as f32,
        rtx.bounces as f32,
        rtx.trace_distance,
        rtx.max_roughness,
    ];
    let bytes = bytemuck::bytes_of(&params);
    let mut out = [0u8; 16];
    out.copy_from_slice(bytes);
    out
}

impl FrameRenderer {
    pub(super) fn trace_background(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        _pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        if !input.raytracing {
            self.draw_sky_background(slot, input, device, cmd)?;
        } else if self.gpu.hw_rt() {
            self.rebuild_tlas(slot, input, cmd)?;
            let view = self.targets.background.view;
            self.dispatch_rays(slot, input, 0, view, device, cmd)?;
        } else {
            let view = self.targets.background.view;
            self.dispatch_bvh(slot, input, 0, view, device, cmd)?;
        }
        Ok(())
    }

    /// Cielo procedural en `targets.background` sin despachar BVH ni subir nodos/triángulos.
    fn draw_sky_background(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let extent = self.rt_pass_extent(false);
        let uniform = rt_camera_bytes(input.scene, extent, &input.sky, &input.rtx);
        self.rt_ubo_bg[slot].write(0, &uniform)?;
        draw_fullscreen(
            device,
            cmd,
            self.sky_bg_pipeline,
            self.sky_bg_layout,
            self.sets.sky_bg[slot],
            self.targets.background.view,
            extent,
            &[],
        );
        // Raster escribe COLOR_ATTACHMENT; `copy_color` y `sky.frag` (MSAA) leen GENERAL.
        crate::engine3d::vulkan::gpu::cmd_image_barrier(
            device,
            cmd,
            self.targets.background.image,
            vk::ImageAspectFlags::COLOR,
            self.targets.background.mips,
            self.targets.background.layers,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::GENERAL,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_READ,
        );
        Ok(())
    }

    pub(super) fn rebuild_tlas(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        if !self.gpu.hw_rt() {
            return Ok(());
        }
        let fast_trace = input.rtx.fast_trace;
        let tlas_slot = slot.min(FRAMES - 1);
        if self.blas_fast_trace != fast_trace {
            // La firma del BLAS incluye `fast_trace`: cada slot lo reconstruirá con los
            // nuevos flags. El TLAS se invalida para reconstruirse también.
            for tlas in &mut self.tlas {
                tlas.invalidate();
            }
            self.blas_fast_trace = fast_trace;
        }
        let mut instances = Vec::with_capacity(input.rt.len());
        let mut geos = Vec::with_capacity(input.rt.len());
        let mut materials = Vec::with_capacity(input.rt.len());
        let stride = std::mem::size_of::<RtMaterialGpu>();
        let mut force_build = false;
        for batch in input.rt.iter() {
            if batch.index_count >= 3 {
                let is_new = !self.blas_cache.contains_key(&batch.key);
                let gpu = &self.gpu;
                let blas = self
                    .blas_cache
                    .entry(batch.key)
                    .or_insert_with(|| accel::Accel::empty(gpu));
                accel::record_blas(
                    &self.gpu,
                    blas,
                    &BlasInput {
                        vertex_address: batch.vertex.address,
                        index_address: batch.index.address,
                        vertex_stride: batch.vertex_stride,
                        max_vertex: batch.vertex_count.saturating_sub(1),
                        primitive_count: batch.index_count / 3,
                    },
                    fast_trace,
                    batch.rebuild,
                    tlas_slot,
                    cmd,
                )?;
                if is_new || batch.rebuild {
                    force_build = true;
                }
            }
            if let Some(blas) = self.blas_cache.get(&batch.key) {
                instances.push(TlasInstance {
                    blas_address: blas.address_for(tlas_slot),
                    transform: batch.transform,
                    custom_index: instances.len() as u32,
                });
                geos.extend_from_slice(&batch.vertex.address.to_le_bytes());
                geos.extend_from_slice(&batch.index.address.to_le_bytes());
                geos.extend_from_slice(&(batch.vertex_stride as u32).to_le_bytes());
                geos.extend_from_slice(&4u32.to_le_bytes());
                // El material que declara el lote se copia en la posición de esta instancia, así
                // `gl_InstanceCustomIndexEXT` indexa `mats` y `geos` en paralelo aunque un BLAS se
                // caiga: índice de lote != índice de material, justo el caso de las piezas skinned.
                let start = batch.material_slot * stride;
                if let Some(material) = input.materials.get(start..start + stride) {
                    materials.extend_from_slice(material);
                }
            }
        }
        Self::grow_host(&self.gpu, &mut self.geos, geos.len() as u64, "rt-geos")?;
        Self::grow_host(
            &self.gpu,
            &mut self.materials,
            materials.len() as u64,
            "rt-materials",
        )?;
        self.geos.write(0, &geos)?;
        self.materials.write(0, &materials)?;
        accel::record_tlas(
            &self.gpu,
            &mut self.tlas[tlas_slot],
            &instances,
            fast_trace,
            force_build,
            cmd,
        )
    }

    pub(super) fn dispatch_rays(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        raygen: u32,
        target: vk::ImageView,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let Some(tlas_handle) = self.tlas[slot.min(FRAMES - 1)].handle() else {
            return Ok(());
        };
        let Some(rt) = self.gpu.rt_pipeline().cloned() else {
            return Ok(());
        };
        let stride = self.sbt_stride;
        let address = self.sbt_address;
        let camera = rt_camera_bytes(
            input.scene,
            self.rt_pass_extent(raygen != 0),
            &input.sky,
            &input.rtx,
        );
        // Fondo y reflejos deben usar UBOs distintos: si compartieran buffer, el último
        // write (resolución del tier RTX) ganaría también para el pase de fondo.
        let ubo = if raygen == 0 {
            &mut self.rt_ubo_bg[slot]
        } else {
            &mut self.rt_ubo[slot]
        };
        ubo.write(0, &camera)?;
        // Un set distinto por uso: ver el comentario de `DescriptorSets::rt`.
        let set = self.sets.rt[slot][raygen as usize % 2];
        write_uniform(device, set, 0, ubo)?;
        write_storage_image(device, set, 1, target)?;
        write_acceleration(device, set, 2, tlas_handle)?;
        write_storage(device, set, 3, &self.materials)?;
        let depth_image = self.targets.depth.image;
        transition_depth(
            device,
            cmd,
            depth_image,
            &mut self.depth_layout,
            DEPTH_READ_ONLY,
        );
        write_combined(
            device,
            set,
            4,
            self.targets.depth.view,
            self.samplers[2],
            DEPTH_READ_ONLY,
        )?;
        write_combined(
            device,
            set,
            5,
            self.sampled_normal(),
            self.samplers[2],
            vk::ImageLayout::GENERAL,
        )?;
        write_storage(device, set, 6, &self.geos)?;
        write_combined(
            device,
            set,
            7,
            self.sampled_albedo(),
            self.samplers[2],
            vk::ImageLayout::GENERAL,
        )?;
        let pipeline = self.rt_pipeline;
        let layout = self.rt_layout;
        let extent = self.rt_pass_extent(raygen != 0);
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::RAY_TRACING_KHR, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::RAY_TRACING_KHR,
                layout,
                0,
                &[set],
                &[],
            );
            let raygen_region = vk::StridedDeviceAddressRegionKHR {
                device_address: address + u64::from(stride) * u64::from(raygen),
                stride: u64::from(stride),
                size: u64::from(stride),
            };
            let miss = vk::StridedDeviceAddressRegionKHR {
                device_address: address + u64::from(stride) * 2,
                stride: u64::from(stride),
                size: u64::from(stride),
            };
            let hit = vk::StridedDeviceAddressRegionKHR {
                device_address: address + u64::from(stride) * 3,
                stride: u64::from(stride),
                size: u64::from(stride),
            };
            let callable = vk::StridedDeviceAddressRegionKHR::default();
            rt.cmd_trace_rays(
                cmd,
                &raygen_region,
                &miss,
                &hit,
                &callable,
                extent.width.max(1),
                extent.height.max(1),
                1,
            );
        }
        Ok(())
    }

    pub(super) fn dispatch_bvh(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        mode: u32,
        target: vk::ImageView,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let mut uniform = rt_camera_bytes_base(input.scene, self.rt_pass_extent(mode != 0));
        uniform.extend_from_slice(&input.bvh_node_count.to_le_bytes());
        uniform.extend_from_slice(&input.bvh_tri_count.to_le_bytes());
        uniform.extend_from_slice(&mode.to_le_bytes());
        uniform.extend_from_slice(&input.material_count.to_le_bytes());
        uniform.extend_from_slice(bytemuck::bytes_of(&input.sky));
        uniform.extend_from_slice(&rtx_uniform_params(&input.rtx));
        let ubo = if mode == 0 {
            &mut self.bvh_ubo_bg[slot]
        } else {
            &mut self.bvh_ubo[slot]
        };
        ubo.write(0, &uniform)?;
        Self::grow_host(
            &self.gpu,
            &mut self.nodes,
            input.bvh_nodes.len() as u64,
            "bvh-nodes",
        )?;
        Self::grow_host(
            &self.gpu,
            &mut self.tris,
            input.bvh_tris.len() as u64,
            "bvh-tris",
        )?;
        Self::grow_host(
            &self.gpu,
            &mut self.materials,
            input.materials.len() as u64,
            "rt-materials",
        )?;
        self.nodes.write(0, input.bvh_nodes)?;
        self.tris.write(0, input.bvh_tris)?;
        self.materials.write(0, input.materials)?;
        // Un set distinto por uso (fondo vs reflejos): ver el comentario de `DescriptorSets::bvh`.
        let set = self.sets.bvh[slot][mode as usize % 2];
        write_uniform(device, set, 0, ubo)?;
        write_storage(device, set, 1, &self.nodes)?;
        write_storage(device, set, 2, &self.tris)?;
        write_storage(device, set, 3, &self.materials)?;
        write_storage_image(device, set, 4, target)?;
        // El descriptor declara el layout: hay que dejar la imagen en READ_ONLY antes de enlazarla.
        let depth_image = self.targets.depth.image;
        transition_depth(
            device,
            cmd,
            depth_image,
            &mut self.depth_layout,
            DEPTH_READ_ONLY,
        );
        write_combined(
            device,
            set,
            5,
            self.targets.depth.view,
            self.samplers[2],
            DEPTH_READ_ONLY,
        )?;
        write_combined(
            device,
            set,
            6,
            self.sampled_normal(),
            self.samplers[2],
            vk::ImageLayout::GENERAL,
        )?;
        write_combined(
            device,
            set,
            7,
            self.sampled_albedo(),
            self.samplers[2],
            vk::ImageLayout::GENERAL,
        )?;
        let pipeline = self.bvh_pipeline;
        let layout = self.bvh_layout;
        let extent = self.rt_pass_extent(mode != 0);
        let groups_x = extent.width.div_ceil(8).max(1);
        let groups_y = extent.height.div_ceil(8).max(1);
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                layout,
                0,
                &[set],
                &[],
            );
            device.cmd_dispatch(cmd, groups_x, groups_y, 1);
        }
        Ok(())
    }
}
