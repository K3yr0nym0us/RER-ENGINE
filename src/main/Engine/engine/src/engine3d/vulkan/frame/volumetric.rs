//! Sombras volumétricas de humo/niebla (F7).
//!
//! Pipeline de tres pasos de cómputo, a resolución de volumen (no de pantalla):
//! 1. `volume_splat.comp`: cada partícula de humo/niebla esparce densidad en una rejilla
//!    R32U con `imageAtomicAdd` (punto fijo 12.4). Usa el descriptor set de partículas.
//! 2. `volume_density.comp`: R32U → R32F muestreable.
//! 3. `volume_light.comp`: desde cada voxel se marcha hacia el sol acumulando densidad;
//!    se guarda `T = exp(-sigma * tau)` con reproyección temporal del volumen vecino.
//!
//! La textura 3D de transmitancia se publica en el set de escena para que el forward la
//! consulte una vez por fragmento y proyecte la sombra del medio sobre suelo y objetos.
//!
//! Los recursos son POR SLOT en vuelo: el pase de un slot lee como histórico la imagen de
//! luz del otro slot (escrita el frame anterior), así que no hay ping-pong dentro del slot
//! ni reescritura de descriptores por frame. Con `FRAMES = 2` el slot par siempre lee el
//! impar y viceversa.

use ash::vk;
use bytemuck::{Pod, Zeroable, bytes_of};

use super::*;
use crate::engine3d::vulkan::gpu::cmd_image_barrier;

/// Semiextensión del volumen que sigue a la cámara. Coincide con el preset de niebla
/// (`EmitterConfig::fog().volume = [26, 14, 26]`).
pub(super) const VOLUME_HALF_EXTENT: [f32; 3] = [26.0, 14.0, 26.0];

/// R32F es obligatorio como storage image y es filtrable en Vulkan core. R16F no está
/// garantizado como storage image en todos los adaptadores, así que se usa el formato ancho.
const VOLUME_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
const ATOMIC_FORMAT: vk::Format = vk::Format::R32_UINT;

/// Peso de la proyección del medio sobre la luz directa. 1 = bloqueo total de la luz,
/// 0 = sin proyección. 0.85 deja una sombra atenuada en vez de una silueta negra.
const SURFACE_SHADOW_STRENGTH: f32 = 0.85;

/// Uniforme del volumen (7 × vec4 = 112 B). Espejo de `VolParams` en los shaders.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct VolUniforms {
    pub center: [f32; 4],
    pub half_extent: [f32; 4],
    pub resolution: [f32; 4],
    pub light_dir: [f32; 4],
    /// x = escala de densidad, y = extinción sigma, z = pasos hacia la luz,
    /// w = intensidad con la que el medio proyecta sombra sobre superficies opacas.
    pub params: [f32; 4],
    /// Centro con el que se construyó la imagen de luz del OTRO slot (reproyección).
    pub history_center: [f32; 4],
    /// x = blend temporal (0 = descartar histórico).
    pub history_meta: [f32; 4],
}

/// Push constant del splat (16 B): el emisor elige qué buffer ping-pong leer.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PushSplat {
    pub misc: [u32; 4],
}

pub(super) struct VolumetricState {
    gpu: SharedGpu,
    resolution: u32,
    steps: u32,
    blend: f32,
    atomic: [GpuImage; FRAMES],
    density: [GpuImage; FRAMES],
    light: [GpuImage; FRAMES],
    ubos: [GpuBuffer; FRAMES],
    sampler: vk::Sampler,
    pool: vk::DescriptorPool,
    density_layout: vk::DescriptorSetLayout,
    light_layout: vk::DescriptorSetLayout,
    density_sets: [vk::DescriptorSet; FRAMES],
    light_sets: [vk::DescriptorSet; FRAMES],
    density_pipeline: vk::Pipeline,
    light_pipeline: vk::Pipeline,
    density_pipeline_layout: vk::PipelineLayout,
    light_pipeline_layout: vk::PipelineLayout,
    shaders: Vec<vk::ShaderModule>,
    pipelines: Vec<vk::Pipeline>,
    layouts: Vec<vk::PipelineLayout>,
    /// Centro usado por cada slot en su último frame (para el histórico del contrario).
    last_centers: [[f32; 4]; FRAMES],
    frames: u32,
}

fn volume_resolution(requested: u32, max_dimension: u32) -> u32 {
    let mut res = requested.max(8);
    while res > max_dimension {
        res /= 2;
    }
    // El eje Y es la mitad; aseguramos que ambos sean múltiplos del group size (8).
    res.max(8) & !7
}

impl VolumetricState {
    pub(super) fn new(
        gpu: &SharedGpu,
        requested: u32,
        steps: u32,
        blend: f32,
    ) -> Result<Self, VulkanError> {
        let resolution = volume_resolution(requested, gpu.max_image_dimension());
        let dim = vk::Extent3D {
            width: resolution,
            height: (resolution / 2).max(8),
            depth: resolution,
        };
        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        let device = gpu.device();
        let mut atomic = Vec::with_capacity(FRAMES);
        let mut density = Vec::with_capacity(FRAMES);
        let mut light = Vec::with_capacity(FRAMES);
        for slot in 0..FRAMES {
            atomic.push(gpu.create_image_3d(
                dim,
                ATOMIC_FORMAT,
                usage,
                vk::ImageAspectFlags::COLOR,
                &format!("vol-atomic-{slot}"),
            )?);
            density.push(gpu.create_image_3d(
                dim,
                VOLUME_FORMAT,
                usage,
                vk::ImageAspectFlags::COLOR,
                &format!("vol-density-{slot}"),
            )?);
            light.push(gpu.create_image_3d(
                dim,
                VOLUME_FORMAT,
                usage,
                vk::ImageAspectFlags::COLOR,
                &format!("vol-light-{slot}"),
            )?);
        }
        let atomic: [GpuImage; FRAMES] = atomic
            .try_into()
            .map_err(|_| VulkanError::new("vol atomic"))?;
        let density: [GpuImage; FRAMES] = density
            .try_into()
            .map_err(|_| VulkanError::new("vol density"))?;
        let light: [GpuImage; FRAMES] = light
            .try_into()
            .map_err(|_| VulkanError::new("vol light"))?;
        let ubos = [
            host_buffer(gpu, 128, "vol-ubo-0")?,
            host_buffer(gpu, 128, "vol-ubo-1")?,
        ];

        // Muestreo lineal con clamp: el borde del volumen no debe repetir densidad.
        let sampler = sampler(device, false, vk::SamplerAddressMode::CLAMP_TO_EDGE)?;

        let density_layout = set_layout(
            device,
            &[
                binding(
                    0,
                    vk::DescriptorType::STORAGE_IMAGE,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                binding(
                    1,
                    vk::DescriptorType::STORAGE_IMAGE,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                binding(
                    2,
                    vk::DescriptorType::UNIFORM_BUFFER,
                    vk::ShaderStageFlags::COMPUTE,
                ),
            ],
        )?;
        let light_layout = set_layout(
            device,
            &[
                binding(
                    0,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                binding(
                    1,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                binding(
                    2,
                    vk::DescriptorType::STORAGE_IMAGE,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                binding(
                    3,
                    vk::DescriptorType::UNIFORM_BUFFER,
                    vk::ShaderStageFlags::COMPUTE,
                ),
            ],
        )?;
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(8)
                    .pool_sizes(&[
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                            descriptor_count: 8,
                        },
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::STORAGE_IMAGE,
                            descriptor_count: 8,
                        },
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::UNIFORM_BUFFER,
                            descriptor_count: 8,
                        },
                    ]),
                None,
            )
        }
        .map_err(|error| vk_error("crear el pool de descriptores volumétricos", error))?;
        let density_sets = [
            alloc_set(device, pool, density_layout)?,
            alloc_set(device, pool, density_layout)?,
        ];
        let light_sets = [
            alloc_set(device, pool, light_layout)?,
            alloc_set(device, pool, light_layout)?,
        ];
        for slot in 0..FRAMES {
            write_storage_image(device, density_sets[slot], 0, atomic[slot].view)?;
            write_storage_image(device, density_sets[slot], 1, density[slot].view)?;
            write_uniform(device, density_sets[slot], 2, &ubos[slot])?;

            write_combined(
                device,
                light_sets[slot],
                0,
                density[slot].view,
                sampler,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                light_sets[slot],
                1,
                light[1 - slot].view,
                sampler,
                vk::ImageLayout::GENERAL,
            )?;
            write_storage_image(device, light_sets[slot], 2, light[slot].view)?;
            write_uniform(device, light_sets[slot], 3, &ubos[slot])?;
        }

        let mut shaders = Vec::new();
        let mut pipelines = Vec::new();
        let mut layouts = Vec::new();
        let density_pipeline = create_compute(
            device,
            gpu.pipeline_cache(),
            spv("volume_density"),
            density_layout,
            &mut shaders,
            &mut layouts,
            &mut pipelines,
        )?;
        let light_pipeline = create_compute(
            device,
            gpu.pipeline_cache(),
            spv("volume_light"),
            light_layout,
            &mut shaders,
            &mut layouts,
            &mut pipelines,
        )?;
        let density_pipeline_layout = layouts[0];
        let light_pipeline_layout = layouts[1];

        Ok(Self {
            gpu: gpu.clone(),
            resolution,
            steps,
            blend,
            atomic,
            density,
            light,
            ubos,
            sampler,
            pool,
            density_layout,
            light_layout,
            density_sets,
            light_sets,
            density_pipeline,
            light_pipeline,
            density_pipeline_layout,
            light_pipeline_layout,
            shaders,
            pipelines,
            layouts,
            last_centers: [[0.0; 4]; FRAMES],
            frames: 0,
        })
    }

    /// Ajusta la calidad al tier. Devuelve el estado viejo si se recrearon los recursos:
    /// el llamador debe reenlazar las vistas y **luego** soltarlo. Destruir las imágenes
    /// 3D mientras un command buffer en grabación sigue apuntando a sus views invalida el CB.
    pub(super) fn ensure(
        &mut self,
        gpu: &SharedGpu,
        requested: u32,
        steps: u32,
        blend: f32,
    ) -> Result<Option<Self>, VulkanError> {
        self.steps = steps;
        self.blend = blend;
        if volume_resolution(requested, gpu.max_image_dimension()) == self.resolution {
            return Ok(None);
        }
        // El tier cambia en caliente: el otro slot en vuelo puede estar muestreando el volumen.
        let _ = unsafe { gpu.device().device_wait_idle() };
        let mut next = Self::new(gpu, requested, steps, blend)?;
        std::mem::swap(self, &mut next);
        Ok(Some(next))
    }

    /// Invalida el histórico temporal (carga de escena, resize del volumen).
    pub(super) fn reset(&mut self) {
        self.frames = 0;
        self.last_centers = [[0.0; 4]; FRAMES];
    }

    pub(super) fn light_view(&self, slot: usize) -> vk::ImageView {
        self.light[slot].view
    }

    pub(super) fn atomic_view(&self, slot: usize) -> vk::ImageView {
        self.atomic[slot].view
    }

    pub(super) fn ubo(&self, slot: usize) -> &GpuBuffer {
        &self.ubos[slot]
    }

    pub(super) fn sampler(&self) -> vk::Sampler {
        self.sampler
    }

    /// Actualiza el volumen y publica su intensidad de proyección para este frame.
    /// Con `active = false` (tier Off o sol bajo el horizonte) solo se publica intensidad 0:
    /// el forward sigue consultando la textura, pero la sombra no se aplica.
    /// `casters` = (capacidad, descriptor set del emisor, read_is_a).
    pub(super) fn update(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        casters: &[(u32, vk::DescriptorSet, u32)],
        splat_pipeline: vk::Pipeline,
        splat_layout: vk::PipelineLayout,
        active: bool,
    ) -> Result<(), VulkanError> {
        let res = self.resolution;
        let center = [
            input.volumetric_center[0],
            input.volumetric_center[1],
            input.volumetric_center[2],
            0.0,
        ];
        let blend = if self.frames < 2 { 0.0 } else { self.blend };
        let uniforms = VolUniforms {
            center,
            half_extent: [
                VOLUME_HALF_EXTENT[0],
                VOLUME_HALF_EXTENT[1],
                VOLUME_HALF_EXTENT[2],
                0.0,
            ],
            resolution: [res as f32, (res / 2) as f32, res as f32, 0.0],
            light_dir: [
                input.volumetric_light_dir[0],
                input.volumetric_light_dir[1],
                input.volumetric_light_dir[2],
                0.0,
            ],
            params: [
                1.0,
                0.55,
                self.steps as f32,
                if active { SURFACE_SHADOW_STRENGTH } else { 0.0 },
            ],
            history_center: self.last_centers[1 - slot],
            history_meta: [blend, 0.0, 0.0, 0.0],
        };
        self.ubos[slot].write(0, bytes_of(&uniforms))?;
        self.last_centers[slot] = center;
        if !active {
            return Ok(());
        }

        let atomic_image = self.atomic[slot].image;
        let density_image = self.density[slot].image;
        let light_image = self.light[slot].image;
        let history_image = self.light[1 - slot].image;

        // Rejilla atómica: clear → splat.
        barrier(
            device,
            cmd,
            atomic_image,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_WRITE,
        );
        unsafe {
            device.cmd_clear_color_image(
                cmd,
                atomic_image,
                vk::ImageLayout::GENERAL,
                &vk::ClearColorValue {
                    uint32: [0, 0, 0, 0],
                },
                &[vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                }],
            );
        }
        barrier(
            device,
            cmd,
            atomic_image,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
        );

        if !casters.is_empty() {
            unsafe {
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, splat_pipeline);
            }
            for &(capacity, set, read_is_a) in casters {
                unsafe {
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::COMPUTE,
                        splat_layout,
                        0,
                        &[set],
                        &[],
                    );
                    device.cmd_push_constants(
                        cmd,
                        splat_layout,
                        vk::ShaderStageFlags::COMPUTE,
                        0,
                        bytes_of(&PushSplat {
                            misc: [read_is_a, 0, 0, 0],
                        }),
                    );
                    device.cmd_dispatch(cmd, capacity.div_ceil(64), 1, 1);
                }
            }
        }

        // Atómicos → conversión a volumen flotante.
        barrier(
            device,
            cmd,
            atomic_image,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ,
        );
        barrier(
            device,
            cmd,
            density_image,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
        );
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.density_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.density_pipeline_layout,
                0,
                &[self.density_sets[slot]],
                &[],
            );
            device.cmd_dispatch(cmd, res.div_ceil(8), (res / 2).div_ceil(8), res.div_ceil(8));
        }

        // Densidad → acumulación de luz. El histórico (otro slot) pasa a lectura.
        barrier(
            device,
            cmd,
            density_image,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ,
        );
        barrier(
            device,
            cmd,
            light_image,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
        );
        barrier(
            device,
            cmd,
            history_image,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_READ,
        );
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.light_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.light_pipeline_layout,
                0,
                &[self.light_sets[slot]],
                &[],
            );
            device.cmd_dispatch(cmd, res.div_ceil(8), (res / 2).div_ceil(8), res.div_ceil(8));
        }

        // La transmitancia la consume el forward al sombrear las superficies.
        barrier(
            device,
            cmd,
            light_image,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::SHADER_READ,
        );

        self.frames += 1;
        Ok(())
    }
}

/// Barrera de storage image dentro de `GENERAL` (cambia solo etapa/acceso).
fn barrier(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    src_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_stage: vk::PipelineStageFlags,
    dst_access: vk::AccessFlags,
) {
    cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::GENERAL,
        src_stage,
        dst_stage,
        src_access,
        dst_access,
    );
}

impl Drop for VolumetricState {
    fn drop(&mut self) {
        let device = self.gpu.device();
        unsafe {
            for pipeline in self.pipelines.drain(..) {
                device.destroy_pipeline(pipeline, None);
            }
            for layout in self.layouts.drain(..) {
                device.destroy_pipeline_layout(layout, None);
            }
            for shader in self.shaders.drain(..) {
                device.destroy_shader_module(shader, None);
            }
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.density_layout, None);
            device.destroy_descriptor_set_layout(self.light_layout, None);
            device.destroy_sampler(self.sampler, None);
        }
    }
}
