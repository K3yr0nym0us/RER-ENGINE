//! Renderer Vulkan del 2D (fase 5): escena instanciada a un objetivo fuera de pantalla, blit al
//! swapchain, rejilla/colisiones y gizmo.
//!
//! Es la traducción del `render()` wgpu a Ash. La lógica de qué se dibuja (culling, orden, flags,
//! UVs, rejilla, colisiones, gizmo) se calcula fuera, en `engine/vulkan_render.rs`, y llega aquí
//! como datos CPU; este módulo solo se ocupa de Vulkan.
//!
//! Orden del frame, calcado del camino wgpu ya validado:
//! 1. pase de escena en `SceneTargets` (color+depth, `load CLEAR`) con el pipeline instanciado;
//! 2. la imagen de color pasa a `GENERAL` y se muestrea;
//! 3. un único `begin_rendering` en el swapchain: blit fullscreen + rejilla + colisiones + gizmo.
//!    Al ser un solo pase sobre la misma imagen no hace falta dependencia entre ellos (el fallo de
//!    los `begin_rendering` consecutivos que documentó el CSM).

use ash::vk;
use rer_engine_shared::player_ui::ndc_draw::NdcVertex;
use rer_engine_shared::vulkan::gpu::cmd_image_barrier;
use rer_engine_shared::vulkan::{
    FRAMES_IN_FLIGHT, GpuBuffer, GpuImage, MemoryLocation, SharedGpu, VulkanError,
};

use crate::engine::types::SceneUniforms;
use crate::gizmo;
use crate::mesh::{InstanceData, Mesh, Vertex};

use super::targets::{COLOR_FORMAT, DEPTH_FORMAT, SceneTargets};

const ATLAS_SIZE: u32 = 4096;
const ATLAS_FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;

/// Un batch de rejilla: uniformes + vértices de línea.
pub(crate) struct GridBatch<'a> {
    pub uniforms: [[f32; 4]; 9],
    pub vertices: &'a [NdcVertex],
}

/// El gizmo usa los ejes estáticos del renderer; el frame solo aporta sus uniformes.
pub(crate) struct GizmoBatch {
    pub uniforms: [[f32; 4]; 9],
}

/// Datos CPU de un frame. Se construyen en `render_vulkan` desde el `State`, sin depender de wgpu.
pub(crate) struct FrameInput<'a> {
    pub clear: [f32; 4],
    pub scene_uniforms: &'a SceneUniforms,
    pub use_2d_pipeline: bool,
    pub batches: &'a [(usize, Vec<InstanceData>)],
    pub meshes: &'a [Mesh],
    pub grid: Option<GridBatch<'a>>,
    pub collision: &'a [NdcVertex],
    pub gizmo: Option<GizmoBatch>,
}

struct VkMesh {
    vertex: GpuBuffer,
    index: GpuBuffer,
    index_count: u32,
}

/// Imagen del atlas compartida por todos los sprites (set 1).
struct Atlas {
    image: GpuImage,
}

pub(crate) struct Renderer {
    gpu: SharedGpu,
    scene: SceneTargets,
    atlas: Atlas,
    meshes: Vec<VkMesh>,
    mesh_signature: (usize, u64),

    shaders: Vec<vk::ShaderModule>,
    sampler_atlas: vk::Sampler,
    sampler_blit: vk::Sampler,

    set_layout_scene: vk::DescriptorSetLayout,
    set_layout_atlas: vk::DescriptorSetLayout,
    set_layout_line: vk::DescriptorSetLayout,
    set_layout_blit: vk::DescriptorSetLayout,
    layout_scene: vk::PipelineLayout,
    layout_line: vk::PipelineLayout,
    layout_blit: vk::PipelineLayout,
    pool: vk::DescriptorPool,

    scene_sets: Vec<vk::DescriptorSet>,
    line_sets: Vec<vk::DescriptorSet>,
    gizmo_sets: Vec<vk::DescriptorSet>,
    atlas_set: vk::DescriptorSet,
    blit_set: vk::DescriptorSet,

    scene_uniforms: Vec<GpuBuffer>,
    line_uniforms: Vec<GpuBuffer>,
    gizmo_uniforms: Vec<GpuBuffer>,
    instance_buffers: Vec<GpuBuffer>,
    line_buffers: Vec<GpuBuffer>,
    gizmo_vertices: GpuBuffer,
    gizmo_vertex_count: u32,

    pipe_scene_opaque: vk::Pipeline,
    pipe_scene_2d: vk::Pipeline,
    pipe_blit: vk::Pipeline,
    pipe_line: vk::Pipeline,
    pipe_gizmo: vk::Pipeline,
}

impl Renderer {
    pub(crate) fn new(gpu: &SharedGpu) -> Result<Self, VulkanError> {
        let device = gpu.device();
        let scene = SceneTargets::new(gpu, gpu.extent())?;

        // Atlas 4096²: el mismo tamaño que la ruta wgpu. `transition_initial` lo deja en GENERAL,
        // que es el layout desde el que se muestrea y al que vuelve cada subida por staging.
        let atlas_image = gpu.create_image(
            vk::Extent2D {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
            },
            ATLAS_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
            "2d-atlas",
        )?;

        let sampler_atlas = make_sampler(device, vk::SamplerAddressMode::CLAMP_TO_EDGE)?;
        let sampler_blit = make_sampler(device, vk::SamplerAddressMode::CLAMP_TO_EDGE)?;

        let set_layout_scene = set_layout(
            device,
            &[vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)],
        )?;
        let set_layout_atlas = set_layout(
            device,
            &[vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)],
        )?;
        let set_layout_line = set_layout(
            device,
            &[vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX)],
        )?;
        let set_layout_blit = set_layout(
            device,
            &[vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)],
        )?;

        let layout_scene = pipeline_layout(device, &[set_layout_scene, set_layout_atlas])?;
        let layout_line = pipeline_layout(device, &[set_layout_line])?;
        let layout_blit = pipeline_layout(device, &[set_layout_blit])?;

        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(16)
                    .pool_sizes(&[
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::UNIFORM_BUFFER,
                            descriptor_count: 8,
                        },
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                            descriptor_count: 4,
                        },
                    ]),
                None,
            )
        }
        .map_err(|error| vk_msg("crear el pool de descriptores", error))?;

        let mut scene_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut line_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut gizmo_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            scene_sets.push(alloc_set(device, pool, set_layout_scene)?);
            line_sets.push(alloc_set(device, pool, set_layout_line)?);
            gizmo_sets.push(alloc_set(device, pool, set_layout_line)?);
        }
        let atlas_set = alloc_set(device, pool, set_layout_atlas)?;
        let blit_set = alloc_set(device, pool, set_layout_blit)?;

        let mut shaders = Vec::new();
        let vert_scene = shader(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/shader.vert.spv")),
            &mut shaders,
        )?;
        let frag_scene = shader(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/shader.frag.spv")),
            &mut shaders,
        )?;
        let vert_blit = shader(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/scene_blit.vert.spv")),
            &mut shaders,
        )?;
        let frag_blit = shader(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/scene_blit.frag.spv")),
            &mut shaders,
        )?;
        let vert_line = shader(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/gizmo.vert.spv")),
            &mut shaders,
        )?;
        let frag_line = shader(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/gizmo.frag.spv")),
            &mut shaders,
        )?;

        let scene_bindings = vec![
            vertex_binding(
                0,
                std::mem::size_of::<Vertex>() as u32,
                vk::VertexInputRate::VERTEX,
            ),
            vertex_binding(
                1,
                std::mem::size_of::<InstanceData>() as u32,
                vk::VertexInputRate::INSTANCE,
            ),
        ];
        let scene_attributes = scene_vertex_attributes();
        let line_bindings = vec![vertex_binding(
            0,
            std::mem::size_of::<NdcVertex>() as u32,
            vk::VertexInputRate::VERTEX,
        )];
        let line_attributes = line_vertex_attributes();

        let swapchain_format = gpu.swapchain_format();
        let pipe_scene_opaque = make_pipeline(
            device,
            layout_scene,
            vert_scene,
            frag_scene,
            &scene_bindings,
            &scene_attributes,
            &[COLOR_FORMAT],
            Some(DepthConfig {
                format: DEPTH_FORMAT,
                test: true,
                write: true,
                compare: vk::CompareOp::LESS,
            }),
            true,
            vk::CullModeFlags::BACK,
            vk::PrimitiveTopology::TRIANGLE_LIST,
        )?;
        let pipe_scene_2d = make_pipeline(
            device,
            layout_scene,
            vert_scene,
            frag_scene,
            &scene_bindings,
            &scene_attributes,
            &[COLOR_FORMAT],
            Some(DepthConfig {
                format: DEPTH_FORMAT,
                test: true,
                write: false,
                compare: vk::CompareOp::ALWAYS,
            }),
            true,
            vk::CullModeFlags::NONE,
            vk::PrimitiveTopology::TRIANGLE_LIST,
        )?;
        let pipe_blit = make_pipeline(
            device,
            layout_blit,
            vert_blit,
            frag_blit,
            &[],
            &[],
            &[swapchain_format],
            None,
            false,
            vk::CullModeFlags::NONE,
            vk::PrimitiveTopology::TRIANGLE_LIST,
        )?;
        let pipe_line = make_pipeline(
            device,
            layout_line,
            vert_line,
            frag_line,
            &line_bindings,
            &line_attributes,
            &[swapchain_format],
            None,
            true,
            vk::CullModeFlags::NONE,
            vk::PrimitiveTopology::LINE_LIST,
        )?;
        let pipe_gizmo = make_pipeline(
            device,
            layout_line,
            vert_line,
            frag_line,
            &line_bindings,
            &line_attributes,
            &[swapchain_format],
            None,
            true,
            vk::CullModeFlags::NONE,
            vk::PrimitiveTopology::TRIANGLE_LIST,
        )?;

        let mut scene_uniforms = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut line_uniforms = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut gizmo_uniforms = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut instance_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut line_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            scene_uniforms.push(gpu.create_buffer(
                std::mem::size_of::<SceneUniforms>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-scene-uniforms",
            )?);
            line_uniforms.push(gpu.create_buffer(
                std::mem::size_of::<[[f32; 4]; 9]>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-line-uniforms",
            )?);
            gizmo_uniforms.push(gpu.create_buffer(
                std::mem::size_of::<[[f32; 4]; 9]>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-gizmo-uniforms",
            )?);
            instance_buffers.push(gpu.create_buffer(
                4,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-instances",
            )?);
            line_buffers.push(gpu.create_buffer(
                4,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-lines",
            )?);
        }

        let gizmo_verts = gizmo::axes_vertices(1.14);
        let gizmo_vertices = gpu.create_buffer_with_data(
            bytemuck::cast_slice(&gizmo_verts),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            MemoryLocation::CpuToGpu,
            "2d-gizmo-vertices",
        )?;

        let mut renderer = Self {
            gpu: gpu.clone(),
            scene,
            atlas: Atlas { image: atlas_image },
            meshes: Vec::new(),
            mesh_signature: (usize::MAX, 0),
            shaders,
            sampler_atlas,
            sampler_blit,
            set_layout_scene,
            set_layout_atlas,
            set_layout_line,
            set_layout_blit,
            layout_scene,
            layout_line,
            layout_blit,
            pool,
            scene_sets,
            line_sets,
            gizmo_sets,
            atlas_set,
            blit_set,
            scene_uniforms,
            line_uniforms,
            gizmo_uniforms,
            instance_buffers,
            line_buffers,
            gizmo_vertices,
            gizmo_vertex_count: gizmo_verts.len() as u32,
            pipe_scene_opaque,
            pipe_scene_2d,
            pipe_blit,
            pipe_line,
            pipe_gizmo,
        };
        renderer.write_static_descriptors()?;
        Ok(renderer)
    }

    /// Descriptores que no cambian por frame: atlas y textura del blit. El del blit se reescribe
    /// cuando el `resize` recrea el scene target.
    fn write_static_descriptors(&mut self) -> Result<(), VulkanError> {
        let device = self.gpu.device();

        for (slot, set) in self.scene_sets.iter().enumerate() {
            write_uniform(device, *set, &self.scene_uniforms[slot])?;
        }
        for (slot, set) in self.line_sets.iter().enumerate() {
            write_uniform(device, *set, &self.line_uniforms[slot])?;
        }
        for (slot, set) in self.gizmo_sets.iter().enumerate() {
            write_uniform(device, *set, &self.gizmo_uniforms[slot])?;
        }
        write_sampled(
            device,
            self.atlas_set,
            self.atlas.image.view,
            self.sampler_atlas,
            vk::ImageLayout::GENERAL,
        )?;
        self.rewrite_blit_descriptor()
    }

    fn rewrite_blit_descriptor(&self) -> Result<(), VulkanError> {
        write_sampled(
            self.gpu.device(),
            self.blit_set,
            self.scene.color.view,
            self.sampler_blit,
            vk::ImageLayout::GENERAL,
        )
    }

    /// Recrea el scene target y el descriptor del blit tras un `resize`.
    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Result<(), VulkanError> {
        unsafe {
            self.gpu
                .device()
                .device_wait_idle()
                .map_err(|error| vk_msg("esperar a la GPU antes del resize", error))?;
        }
        let extent = vk::Extent2D { width, height };
        self.scene.resize(&self.gpu, extent)?;
        self.rewrite_blit_descriptor()
    }

    pub(crate) fn extent(&self) -> vk::Extent2D {
        self.scene.extent
    }

    /// Copia una región RGBA al atlas. La llama `TextureAtlas` en cada `pack`/`update`, así que el
    /// atlas de la ruta Ash recibe exactamente los mismos píxeles que el de wgpu.
    pub(crate) fn upload_atlas_region(
        &self,
        rgba: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), VulkanError> {
        self.gpu
            .upload_rgba_region(&self.atlas.image, x, y, width, height, rgba)
    }

    pub(crate) fn render(
        &mut self,
        frame: &rer_engine_shared::vulkan::FrameTarget,
        input: FrameInput<'_>,
    ) -> Result<(), VulkanError> {
        // El device se toma de una copia del `SharedGpu`: así el préstamo no ata `*self` y se
        // pueden recrear buffers/uniformes mientras el `&ash::Device` sigue vivo.
        let gpu = self.gpu.clone();
        let device = gpu.device();
        let cmd = frame.command;

        self.ensure_meshes(input.meshes)?;

        let slot = frame.slot % FRAMES_IN_FLIGHT;
        self.scene_uniforms[slot].write(0, bytemuck::bytes_of(input.scene_uniforms))?;

        // Instancias de todos los batches en un buffer contiguo; cada batch se dibuja con un
        // offset de instancia.
        let mut flat: Vec<InstanceData> = Vec::new();
        let mut ranges: Vec<(usize, u32)> = Vec::with_capacity(input.batches.len());
        for (mesh_idx, instances) in input.batches {
            let first = flat.len() as u32;
            flat.extend_from_slice(instances);
            ranges.push((*mesh_idx, first));
        }
        let instance_bytes = flat.len() * std::mem::size_of::<InstanceData>();
        if self.instance_buffers[slot].size < instance_bytes.max(4) as u64 {
            self.instance_buffers[slot] = self.gpu.create_buffer(
                instance_bytes.max(4) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-instances",
            )?;
        }
        if !flat.is_empty() {
            self.instance_buffers[slot].write(0, bytemuck::cast_slice(&flat))?;
        }

        // Rejilla + colisiones en un solo buffer de líneas; dos draws con distinto `first_vertex`.
        let grid_verts = input.grid.as_ref().map(|g| g.vertices).unwrap_or(&[]);
        if let Some(grid) = &input.grid {
            self.line_uniforms[slot].write(0, bytemuck::bytes_of(&grid.uniforms))?;
        }
        let line_total = grid_verts.len() + input.collision.len();
        let line_bytes = line_total * std::mem::size_of::<NdcVertex>();
        if self.line_buffers[slot].size < line_bytes.max(4) as u64 {
            self.line_buffers[slot] = self.gpu.create_buffer(
                line_bytes.max(4) as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-lines",
            )?;
        }
        if line_total > 0 {
            let mut verts: Vec<NdcVertex> = Vec::with_capacity(line_total);
            verts.extend_from_slice(grid_verts);
            verts.extend_from_slice(input.collision);
            self.line_buffers[slot].write(0, bytemuck::cast_slice(&verts))?;
        }

        if let Some(gz) = &input.gizmo {
            self.gizmo_uniforms[slot].write(0, bytemuck::bytes_of(&gz.uniforms))?;
        }

        // ── Pase 1: escena fuera de pantalla ────────────────────────────────
        self.scene.begin(device, cmd);
        let extent = self.scene.extent;
        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.scene.color.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: input.clear,
                    },
                });
            let depth_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(self.scene.depth.view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                });
            let colors = [color_attachment];
            let info = vk::RenderingInfo::default()
                .render_area(area(extent))
                .layer_count(1)
                .color_attachments(&colors)
                .depth_attachment(&depth_attachment);
            device.cmd_begin_rendering(cmd, &info);
            set_viewport(device, cmd, extent);

            let pipeline = if input.use_2d_pipeline {
                self.pipe_scene_2d
            } else {
                self.pipe_scene_opaque
            };
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout_scene,
                0,
                &[self.scene_sets[slot], self.atlas_set],
                &[],
            );

            for ((mesh_idx, instances), (_, first_instance)) in
                input.batches.iter().zip(ranges.iter())
            {
                if instances.is_empty() {
                    continue;
                }
                let Some(mesh) = self.meshes.get(*mesh_idx) else {
                    continue;
                };
                let offset = (*first_instance as u64) * std::mem::size_of::<InstanceData>() as u64;
                device.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[mesh.vertex.buffer, self.instance_buffers[slot].buffer],
                    &[0, offset],
                );
                device.cmd_bind_index_buffer(cmd, mesh.index.buffer, 0, vk::IndexType::UINT32);
                device.cmd_draw_indexed(cmd, mesh.index_count, instances.len() as u32, 0, 0, 0);
            }
            device.cmd_end_rendering(cmd);
        }
        self.scene.end(device, cmd);

        // ── Pase 2: swapchain (blit + rejilla + colisiones + gizmo) ─────────
        cmd_image_barrier(
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
        unsafe {
            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(frame.color_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: input.clear,
                    },
                });
            let colors = [color_attachment];
            let info = vk::RenderingInfo::default()
                .render_area(area(extent))
                .layer_count(1)
                .color_attachments(&colors);
            device.cmd_begin_rendering(cmd, &info);
            set_viewport(device, cmd, extent);

            // Blit del scene target (cubre toda la pantalla).
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipe_blit);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout_blit,
                0,
                &[self.blit_set],
                &[],
            );
            device.cmd_draw(cmd, 3, 1, 0, 0);

            // Rejilla y colisiones comparten uniform y pipeline de líneas.
            if line_total > 0 {
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipe_line);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.layout_line,
                    0,
                    &[self.line_sets[slot]],
                    &[],
                );
                device.cmd_bind_vertex_buffers(cmd, 0, &[self.line_buffers[slot].buffer], &[0]);
                if !grid_verts.is_empty() {
                    device.cmd_draw(cmd, grid_verts.len() as u32, 1, 0, 0);
                }
                if !input.collision.is_empty() {
                    device.cmd_draw(
                        cmd,
                        input.collision.len() as u32,
                        1,
                        grid_verts.len() as u32,
                        0,
                    );
                }
            }

            if let Some(gz) = &input.gizmo {
                let _ = gz;
                if self.gizmo_vertex_count > 0 {
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipe_gizmo);
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.layout_line,
                        0,
                        &[self.gizmo_sets[slot]],
                        &[],
                    );
                    device.cmd_bind_vertex_buffers(cmd, 0, &[self.gizmo_vertices.buffer], &[0]);
                    device.cmd_draw(cmd, self.gizmo_vertex_count, 1, 0, 0);
                }
            }
            device.cmd_end_rendering(cmd);
        }

        cmd_image_barrier(
            device,
            cmd,
            frame.color_image,
            vk::ImageAspectFlags::COLOR,
            1,
            1,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::empty(),
        );
        Ok(())
    }

    fn ensure_meshes(&mut self, meshes: &[Mesh]) -> Result<(), VulkanError> {
        let signature = (
            meshes.len(),
            meshes
                .iter()
                .map(|m| (m.vertices.len() + m.indices.len()) as u64)
                .sum::<u64>(),
        );
        if signature == self.mesh_signature {
            return Ok(());
        }
        let mut built = Vec::with_capacity(meshes.len());
        for mesh in meshes {
            let vertex = self.gpu.create_buffer_with_data(
                bytemuck::cast_slice(&mesh.vertices),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-mesh-vertices",
            )?;
            let index = self.gpu.create_buffer_with_data(
                bytemuck::cast_slice(&mesh.indices),
                vk::BufferUsageFlags::INDEX_BUFFER,
                MemoryLocation::CpuToGpu,
                "2d-mesh-indices",
            )?;
            built.push(VkMesh {
                vertex,
                index,
                index_count: mesh.index_count,
            });
        }
        self.meshes = built;
        self.mesh_signature = signature;
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let device = self.gpu.device();
            for pipeline in [
                self.pipe_scene_opaque,
                self.pipe_scene_2d,
                self.pipe_blit,
                self.pipe_line,
                self.pipe_gizmo,
            ] {
                device.destroy_pipeline(pipeline, None);
            }
            for layout in [self.layout_scene, self.layout_line, self.layout_blit] {
                device.destroy_pipeline_layout(layout, None);
            }
            for layout in [
                self.set_layout_scene,
                self.set_layout_atlas,
                self.set_layout_line,
                self.set_layout_blit,
            ] {
                device.destroy_descriptor_set_layout(layout, None);
            }
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_sampler(self.sampler_atlas, None);
            device.destroy_sampler(self.sampler_blit, None);
            for shader in self.shaders.drain(..) {
                device.destroy_shader_module(shader, None);
            }
        }
    }
}

struct DepthConfig {
    format: vk::Format,
    test: bool,
    write: bool,
    compare: vk::CompareOp,
}

fn area(extent: vk::Extent2D) -> vk::Rect2D {
    vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent,
    }
}

fn set_viewport(device: &ash::Device, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
    let viewport = vk::Viewport {
        x: 0.0,
        y: extent.height as f32,
        width: extent.width as f32,
        height: -(extent.height as f32),
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = area(extent);
    unsafe {
        device.cmd_set_viewport(cmd, 0, &[viewport]);
        device.cmd_set_scissor(cmd, 0, &[scissor]);
    }
}

fn vertex_binding(
    binding: u32,
    stride: u32,
    rate: vk::VertexInputRate,
) -> vk::VertexInputBindingDescription {
    vk::VertexInputBindingDescription {
        binding,
        stride,
        input_rate: rate,
    }
}

fn scene_vertex_attributes() -> Vec<vk::VertexInputAttributeDescription> {
    vec![
        attribute(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
        attribute(1, 0, vk::Format::R32G32B32_SFLOAT, 12),
        attribute(2, 0, vk::Format::R32G32_SFLOAT, 24),
        attribute(3, 1, vk::Format::R32G32B32A32_SFLOAT, 0),
        attribute(4, 1, vk::Format::R32G32B32A32_SFLOAT, 16),
        attribute(5, 1, vk::Format::R32G32B32A32_SFLOAT, 32),
        attribute(6, 1, vk::Format::R32G32B32A32_SFLOAT, 48),
        attribute(7, 1, vk::Format::R32G32B32A32_SFLOAT, 64),
        attribute(8, 1, vk::Format::R32G32B32A32_SFLOAT, 80),
    ]
}

fn line_vertex_attributes() -> Vec<vk::VertexInputAttributeDescription> {
    vec![
        attribute(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
        attribute(1, 0, vk::Format::R32G32B32A32_SFLOAT, 12),
    ]
}

fn attribute(
    location: u32,
    binding: u32,
    format: vk::Format,
    offset: u32,
) -> vk::VertexInputAttributeDescription {
    vk::VertexInputAttributeDescription {
        location,
        binding,
        format,
        offset,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_pipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: vk::ShaderModule,
    bindings: &[vk::VertexInputBindingDescription],
    attributes: &[vk::VertexInputAttributeDescription],
    colors: &[vk::Format],
    depth: Option<DepthConfig>,
    blend: bool,
    cull: vk::CullModeFlags,
    topology: vk::PrimitiveTopology,
) -> Result<vk::Pipeline, VulkanError> {
    let name = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(name),
    ];
    let vertex_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(bindings)
        .vertex_attribute_descriptions(attributes);
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(topology);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(cull)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(depth.as_ref().is_some_and(|d| d.test))
        .depth_write_enable(depth.as_ref().is_some_and(|d| d.write))
        .depth_compare_op(
            depth
                .as_ref()
                .map(|d| d.compare)
                .unwrap_or(vk::CompareOp::ALWAYS),
        );
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(blend)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD);
    let attachments = vec![blend_attachment; colors.len()];
    let blend_state = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamics = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamics);
    let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(colors);
    if let Some(depth) = &depth {
        rendering = rendering.depth_attachment_format(depth.format);
    }
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_info)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth_state)
        .color_blend_state(&blend_state)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    match unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) } {
        Ok(mut created) => Ok(created.remove(0)),
        Err((created, error)) => {
            for pipeline in created {
                unsafe { device.destroy_pipeline(pipeline, None) };
            }
            Err(vk_msg("crear un pipeline gráfico", error))
        }
    }
}

fn make_sampler(
    device: &ash::Device,
    address: vk::SamplerAddressMode,
) -> Result<vk::Sampler, VulkanError> {
    unsafe {
        device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(address)
                .address_mode_v(address)
                .address_mode_w(address)
                .max_lod(0.0),
            None,
        )
    }
    .map_err(|error| vk_msg("crear un sampler", error))
}

fn set_layout(
    device: &ash::Device,
    bindings: &[vk::DescriptorSetLayoutBinding<'_>],
) -> Result<vk::DescriptorSetLayout, VulkanError> {
    unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(bindings),
            None,
        )
    }
    .map_err(|error| vk_msg("crear un descriptor set layout", error))
}

fn alloc_set(
    device: &ash::Device,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> Result<vk::DescriptorSet, VulkanError> {
    let layouts = [layout];
    unsafe {
        device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts),
        )
    }
    .map_err(|error| vk_msg("reservar un descriptor set", error))
    .map(|sets| sets[0])
}

fn pipeline_layout(
    device: &ash::Device,
    sets: &[vk::DescriptorSetLayout],
) -> Result<vk::PipelineLayout, VulkanError> {
    unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default().set_layouts(sets),
            None,
        )
    }
    .map_err(|error| vk_msg("crear un pipeline layout", error))
}

fn shader(
    device: &ash::Device,
    bytes: &[u8],
    keep: &mut Vec<vk::ShaderModule>,
) -> Result<vk::ShaderModule, VulkanError> {
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    let module = unsafe {
        device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
    }
    .map_err(|error| vk_msg("crear un shader module", error))?;
    keep.push(module);
    Ok(module)
}

fn write_uniform(
    device: &ash::Device,
    set: vk::DescriptorSet,
    buffer: &GpuBuffer,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorBufferInfo::default()
        .buffer(buffer.buffer)
        .offset(0)
        .range(buffer.size);
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

fn write_sampled(
    device: &ash::Device,
    set: vk::DescriptorSet,
    view: vk::ImageView,
    sampler: vk::Sampler,
    layout: vk::ImageLayout,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorImageInfo::default()
        .image_view(view)
        .image_layout(layout)
        .sampler(sampler);
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

fn vk_msg(context: &str, error: vk::Result) -> VulkanError {
    VulkanError::new(format!("{context}: {error:?}"))
}
