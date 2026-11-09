//! Frame 3D: raster (sombras, forward, probes, TAA, SSR, denoise), reflejos RT o BVH, gizmo y HUD.
//! El `unsafe` de este archivo es FFI de Vulkan (pipelines, descriptores y barreras).

use std::collections::HashMap;

use ash::vk;
use gpu_allocator::MemoryLocation;

use crate::engine3d::bvh::RtMaterialGpu;

use super::accel::{self, Accel, BlasInput, TlasInstance};
use super::error::{VulkanError, vk_error};
use super::select::align_up;
use crate::engine3d::vulkan::gpu::{FrameTarget, GpuBuffer, GpuImage, SharedGpu};

const FRAMES: usize = 2;
const COLOR: vk::Format = vk::Format::R8G8B8A8_UNORM;
const WORLD: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const VELOCITY: vk::Format = vk::Format::R16G16_SFLOAT;
const DEPTH: vk::Format = vk::Format::D32_SFLOAT;

/// Atajos de layout para las imágenes de profundidad compartidas.
const DEPTH_ATTACHMENT: vk::ImageLayout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
const DEPTH_READ_ONLY: vk::ImageLayout = vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL;

/// Suelo del shadow map: un mapa de 1 px no es muestreable y `ShadowTier::Off` reporta 1.
const SHADOW_MAP_MIN: u32 = 512;
const MRT: [vk::Format; 5] = [COLOR, COLOR, COLOR, WORLD, VELOCITY];

fn spv(name: &str) -> &'static [u8] {
    match name {
        "primary.rgen" => include_bytes!(concat!(env!("OUT_DIR"), "/primary.rgen.spv")),
        "reflect.rgen" => include_bytes!(concat!(env!("OUT_DIR"), "/reflect.rgen.spv")),
        "primary.rmiss" => include_bytes!(concat!(env!("OUT_DIR"), "/primary.rmiss.spv")),
        "primary.rchit" => include_bytes!(concat!(env!("OUT_DIR"), "/primary.rchit.spv")),
        "bvh" => include_bytes!(concat!(env!("OUT_DIR"), "/bvh.comp.spv")),
        "forward.vert" => include_bytes!(concat!(env!("OUT_DIR"), "/forward.vert.spv")),
        "forward.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/forward.frag.spv")),
        "shadow.vert" => include_bytes!(concat!(env!("OUT_DIR"), "/shadow.vert.spv")),
        "skinned.vert" => include_bytes!(concat!(env!("OUT_DIR"), "/skinned.vert.spv")),
        "gizmo.vert" => include_bytes!(concat!(env!("OUT_DIR"), "/gizmo.vert.spv")),
        "gizmo.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/gizmo.frag.spv")),
        "hud.vert" => include_bytes!(concat!(env!("OUT_DIR"), "/hud_inst.vert.spv")),
        "hud.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/hud_inst.frag.spv")),
        "full.vert" => include_bytes!(concat!(env!("OUT_DIR"), "/fullscreen.vert.spv")),
        "taa.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/taa.frag.spv")),
        "ssr.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/ssr.frag.spv")),
        "temporal.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/temporal.frag.spv")),
        "denoise.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/denoise.frag.spv")),
        "composite.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/composite.frag.spv")),
        "present.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/present.frag.spv")),
        "depth_only.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/depth_only.frag.spv")),
        "sky.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/sky.frag.spv")),
        _ => &[],
    }
}

pub struct MeshBatch<'a> {
    pub vertex: &'a GpuBuffer,
    pub index: &'a GpuBuffer,
    pub index_count: u32,
    pub instances: &'a [u8],
    pub instance_count: u32,
}

pub struct SkinnedBatch<'a> {
    pub vertex: &'a GpuBuffer,
    pub index: &'a GpuBuffer,
    pub index_count: u32,
    pub instances: &'a [u8],
    pub instance_count: u32,
    pub palette: &'a [[[f32; 4]; 4]],
}

pub struct GizmoBatch<'a> {
    pub vertices: &'a GpuBuffer,
    pub vertex_count: u32,
    pub uniform: &'a [u8],
    /// `true` para topología `LINE_LIST` (rejillas, bounds, esqueletos).
    pub lines: bool,
}

/// Geometría para BLAS. `rebuild` reconstruye el BLAS (mallas skinned).
pub struct RtBatch<'a> {
    pub key: u64,
    pub vertex: &'a GpuBuffer,
    pub index: &'a GpuBuffer,
    pub vertex_stride: u64,
    pub vertex_count: u32,
    pub index_count: u32,
    pub transform: [f32; 12],
    pub rebuild: bool,
    /// Índice en `FrameInput::materials`. Se pasa explícito porque los lotes skinned van despues
    /// de los estáticos y el BLAS de una pieza puede caer (buffer no creado, índice corto): si se
    /// dedujera del índice de lote, un salto desalinearía todos los materiales siguientes.
    pub material_slot: usize,
}

/// Captura de una sonda: centro de la esfera y ranura de cubemap (`slot * 6` es la primera capa).
#[derive(Clone, Copy, Debug)]
pub struct ProbeCapture {
    pub origin: [f32; 3],
    pub slot: usize,
}

pub struct FrameInput<'a> {
    pub scene: &'a [u8],
    pub shadows: bool,
    /// Lado del shadow map (px) exigido por el tier de sombras. `FrameRenderer` reasigna el mapa
    /// cuando cambia; sin esto el tier solo movía un número en el log.
    pub shadow_map_size: u32,
    /// Muestras de MSAA pedidas por el tier (1 = sin multisampling). `FrameRenderer` lo recorta
    /// a lo que el adaptador admite y recrea el G-buffer multisampleado.
    pub msaa_samples: u32,
    pub taa_enabled: bool,
    pub taa_blend: f32,
    pub reflections: bool,
    /// `true`: el pase de reflejos traza rayos (HW o BVH). `false`: solo SSR.
    pub raytracing: bool,
    /// Sondas a capturar este frame, cada una con su ranura del cubemap array.
    pub probe_captures: &'a [ProbeCapture],
    /// Vista de debug: en vez del resultado final se presenta la imagen SSR/trazado en crudo.
    /// Lo pide el editor con `set_reflection_debug_view`; no requiere cambiar ningún shader.
    pub debug_ssr_view: bool,
    pub static_batches: &'a [MeshBatch<'a>],
    /// Batches con alpha < 0.98 para el pase blend.
    pub static_transparent_batches: &'a [MeshBatch<'a>],
    pub skinned_batches: &'a [SkinnedBatch<'a>],
    pub skinned_transparent_batches: &'a [SkinnedBatch<'a>],
    pub gizmos: &'a [GizmoBatch<'a>],
    pub hud_vertex: Option<&'a GpuBuffer>,
    pub hud_index: Option<&'a GpuBuffer>,
    pub hud_index_count: u32,
    pub hud_instances: &'a [u8],
    pub hud_instance_count: u32,
    pub hud_atlas: Option<&'a GpuImage>,
    pub albedo: &'a GpuImage,
    pub rt: &'a [RtBatch<'a>],
    pub materials: &'a [u8],
    pub material_count: u32,
    pub bvh_nodes: &'a [u8],
    pub bvh_node_count: u32,
    pub bvh_tris: &'a [u8],
    pub bvh_tri_count: u32,
}

pub struct FrameRenderer {
    gpu: SharedGpu,
    targets: Targets,
    shaders: Vec<vk::ShaderModule>,
    layouts: Vec<vk::PipelineLayout>,
    pipelines: Vec<vk::Pipeline>,
    pool: vk::DescriptorPool,
    /// Los `VkDescriptorSetLayout` del renderer. Se guardan porque hay que destruirlos: el leak que
    /// reportaba `vkDestroyDevice(): N leaked objects` al cerrar eran exactamente estos. Son 12
    /// campos pero 11 objetos (`taa` comparte layout con `full`).
    set_layouts: SetLayouts,
    samplers: Vec<vk::Sampler>,
    sets: DescriptorSets,
    scene_ubo: [GpuBuffer; FRAMES],
    gizmo_ubo: [GpuBuffer; FRAMES],
    rt_ubo: [GpuBuffer; FRAMES],
    bvh_ubo: [GpuBuffer; FRAMES],
    instances: [GpuBuffer; FRAMES],
    joints: [GpuBuffer; FRAMES],
    materials: GpuBuffer,
    geos: GpuBuffer,
    nodes: GpuBuffer,
    tris: GpuBuffer,
    /// Mantiene vivo el buffer del SBT (su `Drop` libera la memoria).
    #[allow(dead_code)]
    sbt: Option<GpuBuffer>,
    /// Imagen de albedo con la que está enlazado el descriptor set; evita reescribirlo cada frame.
    albedo_image: vk::Image,
    /// Imagen del atlas HUD enlazada a `sets.hud` (se escribe una sola vez por atlas).
    hud_image: vk::Image,
    rt_pipeline: vk::Pipeline,
    bvh_pipeline: vk::Pipeline,
    rt_layout: vk::PipelineLayout,
    bvh_layout: vk::PipelineLayout,
    sbt_stride: u32,
    sbt_address: u64,
    blas_cache: HashMap<u64, Accel>,
    tlas: Option<Accel>,
    history_slot: usize,
    taa_frame: u32,
    taa_history_valid: bool,
    static_offsets: Vec<u64>,
    static_transparent_offsets: Vec<u64>,
    skinned_offsets: Vec<u64>,
    skinned_transparent_offsets: Vec<u64>,
    joint_bases: Vec<u32>,
    joint_bases_transparent: Vec<u32>,
    hud_offset: u64,
    /// Layout REAL de las imágenes de profundidad. Fuente única de verdad para las barreras:
    /// si un descriptor declara un layout distinto al que tiene la imagen, el resultado es
    /// contenido indefinido (los bloques negros en el cielo) y la validación lo reporta.
    shadow_layout: vk::ImageLayout,
    depth_layout: vk::ImageLayout,
    /// Lado actual del shadow map; coincide con `targets.shadow.extent.width`.
    shadow_size: u32,
    /// Muestras activas del G-buffer forward (1 = sin MSAA). Sigue al tier elegido por el editor.
    msaa_samples: u32,
    /// `[forward, forward_blend, skinned, skinned_blend]` creados con `msaa_samples` muestras.
    ms_pipelines: Option<[vk::Pipeline; 5]>,
    /// `[depth_only, depth_only_skinned]` a 1×: rellenan `targets.depth` tras el pase MSAA.
    depth_pipelines: [vk::Pipeline; 2],
    /// Módulos del forward, guardados para poder recrear los pipelines cuando cambia el tier.
    forward_vert: vk::ShaderModule,
    forward_frag: vk::ShaderModule,
    skinned_vert: vk::ShaderModule,
    /// Triángulo full-screen + shader de fondo, para recrear el pipeline del cielo con las
    /// muestras activas.
    full_vert: vk::ShaderModule,
    sky_frag: vk::ShaderModule,
    layout_full: vk::PipelineLayout,
}

/// Los sets se indexan por slot en vuelo (`FRAMES`). Un set único reescrito cada frame era
/// inseguro: `begin_frame` solo espera el fence de SU slot, así que el frame anterior podía estar
/// aún en vuelo mientras la CPU actualizaba el mismo set, y actualizar un set ya enlazado
/// invalida todo el command buffer (VVL: "destroyed or updated without UPDATE_AFTER_BIND").
/// `empty` queda compartido porque no se reescribe después de la inicialización.
struct DescriptorSets {
    scene: [vk::DescriptorSet; FRAMES],
    albedo: [vk::DescriptorSet; FRAMES],
    probe: [vk::DescriptorSet; FRAMES],
    joints: [vk::DescriptorSet; FRAMES],
    empty: vk::DescriptorSet,
    gizmo: [vk::DescriptorSet; FRAMES],
    hud: [vk::DescriptorSet; FRAMES],
    /// Un set por uso (0 = fondo, 1 = reflejos) y por slot: el trazado de fondo y el de reflejos
    /// no pueden compartir set aunque vayan en el mismo frame.
    rt: [[vk::DescriptorSet; 2]; FRAMES],
    bvh: [[vk::DescriptorSet; 2]; FRAMES],
    taa: [vk::DescriptorSet; FRAMES],
    ssr: [vk::DescriptorSet; FRAMES],
    denoise: [vk::DescriptorSet; FRAMES],
    composite: [vk::DescriptorSet; FRAMES],
    present: [vk::DescriptorSet; FRAMES],
    /// Fondo trazado para el quad del cielo del pase MSAA (misma layout que `full`).
    sky: [vk::DescriptorSet; FRAMES],
}

#[derive(Clone, Copy)]
struct Pipes {
    forward: vk::Pipeline,
    forward_blend: vk::Pipeline,
    skinned: vk::Pipeline,
    skinned_blend: vk::Pipeline,
    shadow: vk::Pipeline,
    shadow_skinned: vk::Pipeline,
    gizmo: vk::Pipeline,
    gizmo_lines: vk::Pipeline,
    hud: vk::Pipeline,
    taa: vk::Pipeline,
    ssr: vk::Pipeline,
    denoise: vk::Pipeline,
    composite: vk::Pipeline,
    present: vk::Pipeline,
    /// Quad del cielo del pase MSAA. Sin MSAA no se usa; se rellena con `present` para no dejar un
    /// pipeline nulo suelto en `Pipes`.
    sky: vk::Pipeline,
}

impl Drop for FrameRenderer {
    fn drop(&mut self) {
        let device = self.gpu.device();
        unsafe {
            let _ = device.device_wait_idle();
            for pipeline in self.pipelines.drain(..) {
                device.destroy_pipeline(pipeline, None);
            }
            for layout in self.layouts.drain(..) {
                device.destroy_pipeline_layout(layout, None);
            }
            for shader in self.shaders.drain(..) {
                device.destroy_shader_module(shader, None);
            }
            for sampler in self.samplers.drain(..) {
                device.destroy_sampler(sampler, None);
            }
            device.destroy_descriptor_pool(self.pool, None);
            // Despues del pool: los sets del pool mueren con él, y los pipeline layouts (que
            // apuntan a estos layouts) ya se destruyeron arriba.
            self.set_layouts.destroy(device);
        }
    }
}

mod commands;
mod forward;
mod layouts;
mod pipelines;
mod present;
mod reflections;
mod rt;
mod shadow;
mod targets;
use self::commands::*;
use self::layouts::*;
use self::pipelines::*;
use self::targets::*;

impl FrameRenderer {
    pub fn new(gpu: &SharedGpu, albedo: &GpuImage) -> Result<Self, VulkanError> {
        let extent = gpu.extent();
        let targets = Targets::new(gpu, extent)?;
        clear_probe_cube(gpu, targets.probe.image)?;
        let device = gpu.device();
        let (pool, set_layouts, sets) = create_sets(device)?;
        let mut shaders = Vec::new();
        let mut layouts = Vec::new();
        let mut pipelines = Vec::new();
        let linear = sampler(device, false, vk::SamplerAddressMode::REPEAT)?;
        let shadow_sampler = compare_sampler(device)?;
        let clamp = sampler(device, false, vk::SamplerAddressMode::CLAMP_TO_EDGE)?;
        let samplers = vec![linear, shadow_sampler, clamp];
        // Las vistas de álbum y probes se escriben en TODOS los slots: cambian en momentos
        // arbitrarios (carga de modelos, recreate) y un slot con la vista vieja apuntando a una
        // imagen ya destruida es ilegal.
        for slot in 0..FRAMES {
            write_combined(
                device,
                sets.albedo[slot],
                0,
                albedo.view,
                linear,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                sets.probe[slot],
                0,
                targets.probe.view,
                linear,
                vk::ImageLayout::GENERAL,
            )?;
        }
        let scene_ubo = [
            host_buffer(gpu, 512, "scene-ubo-0")?,
            host_buffer(gpu, 512, "scene-ubo-1")?,
        ];
        let gizmo_ubo = [
            host_buffer(gpu, 256, "gizmo-ubo-0")?,
            host_buffer(gpu, 256, "gizmo-ubo-1")?,
        ];
        let rt_ubo = [
            host_buffer(gpu, 256, "rt-ubo-0")?,
            host_buffer(gpu, 256, "rt-ubo-1")?,
        ];
        let bvh_ubo = [
            host_buffer(gpu, 256, "bvh-ubo-0")?,
            host_buffer(gpu, 256, "bvh-ubo-1")?,
        ];
        let instances = [
            host_buffer(gpu, 4096, "inst-0")?,
            host_buffer(gpu, 4096, "inst-1")?,
        ];
        let joints = [
            host_buffer(gpu, 4096, "joints-0")?,
            host_buffer(gpu, 4096, "joints-1")?,
        ];
        let materials = host_buffer(gpu, 64 * 1024, "rt-materials")?;
        let geos = host_buffer(gpu, 64 * 1024, "rt-geos")?;
        let nodes = host_buffer(gpu, 1024 * 1024, "bvh-nodes")?;
        let tris = host_buffer(gpu, 1024 * 1024, "bvh-tris")?;
        for slot in 0..FRAMES {
            write_uniform(device, sets.scene[slot], 0, &scene_ubo[slot])?;
            write_combined(
                device,
                sets.scene[slot],
                1,
                targets.shadow.view,
                shadow_sampler,
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            )?;
            write_uniform(device, sets.gizmo[slot], 0, &gizmo_ubo[slot])?;
            write_storage(device, sets.joints[slot], 0, &joints[slot])?;
            // Fondo para el quad del cielo del pase MSAA.
            write_combined(
                device,
                sets.sky[slot],
                0,
                targets.background.view,
                clamp,
                vk::ImageLayout::GENERAL,
            )?;
        }
        let forward_layout = pipeline_layout(
            device,
            &[
                set_layouts.scene,
                set_layouts.albedo,
                set_layouts.empty,
                set_layouts.probe,
            ],
            8,
        )?;
        let skinned_layout = pipeline_layout(
            device,
            &[
                set_layouts.scene,
                set_layouts.albedo,
                set_layouts.joints,
                set_layouts.probe,
            ],
            8,
        )?;
        let shadow_layout = pipeline_layout(device, &[set_layouts.scene], 0)?;
        let shadow_skinned_layout = pipeline_layout(
            device,
            &[set_layouts.scene, set_layouts.empty, set_layouts.joints],
            8,
        )?;
        let gizmo_layout = pipeline_layout(device, &[set_layouts.gizmo], 0)?;
        let hud_layout = pipeline_layout(device, &[set_layouts.hud], 0)?;
        let full_layout = pipeline_layout(device, &[set_layouts.full], 0)?;
        let taa_layout = pipeline_layout(device, &[set_layouts.taa], 8)?;
        let denoise_layout = pipeline_layout(device, &[set_layouts.denoise], 4)?;
        layouts.extend([
            forward_layout,
            skinned_layout,
            shadow_layout,
            shadow_skinned_layout,
            gizmo_layout,
            hud_layout,
            full_layout,
            taa_layout,
            denoise_layout,
        ]);
        let vert = shader(device, spv("forward.vert"), &mut shaders)?;
        let frag = shader(device, spv("forward.frag"), &mut shaders)?;
        let skinned_vert = shader(device, spv("skinned.vert"), &mut shaders)?;
        let shadow_vert = shader(device, spv("shadow.vert"), &mut shaders)?;
        let forward = graphics(
            device,
            forward_layout,
            vert,
            Some(frag),
            &static_vertex(),
            &MRT,
            Some(DEPTH),
            false,
            false,
            &mut pipelines,
        )?;
        let forward_blend = graphics(
            device,
            forward_layout,
            vert,
            Some(frag),
            &static_vertex(),
            &MRT,
            Some(DEPTH),
            true,
            false,
            &mut pipelines,
        )?;
        let skinned = graphics(
            device,
            skinned_layout,
            skinned_vert,
            Some(frag),
            &skinned_vertex(),
            &MRT,
            Some(DEPTH),
            false,
            false,
            &mut pipelines,
        )?;
        let skinned_blend = graphics(
            device,
            skinned_layout,
            skinned_vert,
            Some(frag),
            &skinned_vertex(),
            &MRT,
            Some(DEPTH),
            true,
            false,
            &mut pipelines,
        )?;
        let shadow = graphics(
            device,
            shadow_layout,
            shadow_vert,
            None,
            &static_vertex(),
            &[],
            Some(DEPTH),
            false,
            true,
            &mut pipelines,
        )?;
        let shadow_skinned = graphics(
            device,
            shadow_skinned_layout,
            skinned_vert,
            None,
            &skinned_vertex(),
            &[],
            Some(DEPTH),
            false,
            true,
            &mut pipelines,
        )?;
        let gizmo_vert = shader(device, spv("gizmo.vert"), &mut shaders)?;
        let gizmo_frag = shader(device, spv("gizmo.frag"), &mut shaders)?;
        let gizmo = graphics(
            device,
            gizmo_layout,
            gizmo_vert,
            Some(gizmo_frag),
            &gizmo_vertex(),
            &[gpu.swapchain_format()],
            None,
            true,
            false,
            &mut pipelines,
        )?;
        let hud_vert = shader(device, spv("hud.vert"), &mut shaders)?;
        let hud_frag = shader(device, spv("hud.frag"), &mut shaders)?;
        let hud = graphics(
            device,
            hud_layout,
            hud_vert,
            Some(hud_frag),
            &static_vertex(),
            &[gpu.swapchain_format()],
            None,
            true,
            false,
            &mut pipelines,
        )?;
        let full_vert = shader(device, spv("full.vert"), &mut shaders)?;
        let taa_frag = shader(device, spv("taa.frag"), &mut shaders)?;
        let ssr_frag = shader(device, spv("ssr.frag"), &mut shaders)?;
        let temporal_frag = shader(device, spv("temporal.frag"), &mut shaders)?;
        let denoise_frag = shader(device, spv("denoise.frag"), &mut shaders)?;
        let composite_frag = shader(device, spv("composite.frag"), &mut shaders)?;
        let present_frag = shader(device, spv("present.frag"), &mut shaders)?;
        let sky_frag = shader(device, spv("sky.frag"), &mut shaders)?;
        let taa = graphics(
            device,
            taa_layout,
            full_vert,
            Some(taa_frag),
            &empty_vertex(),
            &[COLOR],
            None,
            false,
            false,
            &mut pipelines,
        )?;
        let ssr = graphics(
            device,
            full_layout,
            full_vert,
            Some(ssr_frag),
            &empty_vertex(),
            &[COLOR],
            None,
            false,
            false,
            &mut pipelines,
        )?;
        let temporal = graphics(
            device,
            full_layout,
            full_vert,
            Some(temporal_frag),
            &empty_vertex(),
            &[COLOR],
            None,
            false,
            false,
            &mut pipelines,
        )?;
        let denoise = graphics(
            device,
            denoise_layout,
            full_vert,
            Some(denoise_frag),
            &empty_vertex(),
            &[COLOR],
            None,
            false,
            false,
            &mut pipelines,
        )?;
        let composite = graphics(
            device,
            full_layout,
            full_vert,
            Some(composite_frag),
            &empty_vertex(),
            &[COLOR],
            None,
            false,
            false,
            &mut pipelines,
        )?;
        let present = graphics(
            device,
            full_layout,
            full_vert,
            Some(present_frag),
            &empty_vertex(),
            &[gpu.swapchain_format()],
            None,
            false,
            false,
            &mut pipelines,
        )?;
        // Mismo shader que el gizmo, pero `LINE_LIST` para rejillas, bounds,
        // esqueletos y sockets (paridad con el `grid_pipeline` de wgpu).
        let gizmo_lines = graphics_topology(
            device,
            gizmo_layout,
            gizmo_vert,
            Some(gizmo_frag),
            &gizmo_vertex(),
            &[gpu.swapchain_format()],
            None,
            true,
            false,
            vk::PrimitiveTopology::LINE_LIST,
            &mut pipelines,
        )?;
        let (rt_pipeline, sbt, sbt_stride, sbt_address) = if gpu.hw_rt() {
            let built = create_rt_pipeline(
                gpu,
                &set_layouts.rt,
                &mut shaders,
                &mut layouts,
                &mut pipelines,
            )?;
            (built.0, Some(built.1), built.2, built.3)
        } else {
            (vk::Pipeline::null(), None, 0, 0)
        };
        let rt_layout = if gpu.hw_rt() {
            *layouts.last().unwrap_or(&vk::PipelineLayout::null())
        } else {
            vk::PipelineLayout::null()
        };
        let bvh_pipe = create_compute(
            device,
            spv("bvh"),
            set_layouts.bvh,
            &mut shaders,
            &mut layouts,
            &mut pipelines,
        )?;
        let bvh_layout = *layouts.last().unwrap_or(&vk::PipelineLayout::null());
        // Pipelines de solo-profundidad (sin adjuntos de color) para el segundo pase que rellena
        // la depth 1× cuando el G-buffer está multisampleado. Usan `depth_only.frag`, que solo
        // repite el `discard` del alfa cortado: así la profundidad coincide con la imagen resuelta.
        let depth_frag = shader(device, spv("depth_only.frag"), &mut shaders)?;
        let depth_only = graphics(
            device,
            forward_layout,
            vert,
            Some(depth_frag),
            &static_vertex(),
            &[],
            Some(DEPTH),
            false,
            false,
            &mut pipelines,
        )?;
        let depth_only_skinned = graphics(
            device,
            skinned_layout,
            skinned_vert,
            Some(depth_frag),
            &skinned_vertex(),
            &[],
            Some(DEPTH),
            false,
            false,
            &mut pipelines,
        )?;
        let depth_pipelines = [depth_only, depth_only_skinned];
        let _kept = (
            forward,
            forward_blend,
            skinned,
            skinned_blend,
            shadow,
            shadow_skinned,
            gizmo,
            hud,
            taa,
            ssr,
            temporal,
            denoise,
            composite,
            present,
            gizmo_lines,
            depth_only,
            depth_only_skinned,
        );
        Ok(Self {
            msaa_samples: 1,
            ms_pipelines: None,
            forward_vert: vert,
            forward_frag: frag,
            skinned_vert,
            depth_pipelines,
            full_vert,
            sky_frag,
            layout_full: full_layout,
            gpu: gpu.clone(),
            targets,
            shaders,
            layouts,
            pipelines,
            pool,
            set_layouts,
            samplers,
            sets,
            scene_ubo,
            gizmo_ubo,
            rt_ubo,
            bvh_ubo,
            instances,
            joints,
            materials,
            geos,
            nodes,
            tris,
            sbt,
            albedo_image: albedo.image,
            hud_image: vk::Image::null(),
            rt_pipeline,
            bvh_pipeline: bvh_pipe,
            rt_layout,
            bvh_layout,
            sbt_stride,
            sbt_address,
            blas_cache: HashMap::new(),
            tlas: None,
            history_slot: 0,
            taa_frame: 0,
            taa_history_valid: false,
            static_offsets: Vec::new(),
            static_transparent_offsets: Vec::new(),
            skinned_offsets: Vec::new(),
            skinned_transparent_offsets: Vec::new(),
            joint_bases: Vec::new(),
            joint_bases_transparent: Vec::new(),
            hud_offset: 0,
            // `create_image` deja las imágenes de aspecto DEPTH en ATTACHMENT.
            shadow_layout: DEPTH_ATTACHMENT,
            depth_layout: DEPTH_ATTACHMENT,
            shadow_size: crate::engine3d::engine::SHADOW_MAP_SIZE,
        })
    }

    pub fn record(
        &mut self,
        frame: &FrameTarget,
        input: &FrameInput<'_>,
    ) -> Result<(), VulkanError> {
        if self.targets.extent != frame.extent {
            self.targets.recreate(&self.gpu, frame.extent)?;
            // Las imágenes nuevas nacen en su layout inicial (DEPTH → ATTACHMENT); el registro
            // anterior ya no describe la realidad y hay que reiniciarlo.
            self.depth_layout = DEPTH_ATTACHMENT;
            self.shadow_layout = DEPTH_ATTACHMENT;
            // `recreate` también destruye el G-buffer multisampleado y el shadow map en su tamaño
            // base: hay que volver a crearlos, si no MSAA se apaga solo tras un resize porque
            // `ensure_msaa` ve el mismo nivel de muestras y sale por su early-return.
            self.targets.ensure_ms(&self.gpu, self.msaa_samples)?;
            self.shadow_size = crate::engine3d::engine::SHADOW_MAP_SIZE;
            self.rewrite_target_descriptors()?;
        }
        // El tier de sombras exige otro resolución: se reasigna SOLO el shadow map. Es seguro
        // porque `begin_frame` espera a la GPU (`RER_VK_SYNC_FRAMES`) antes de grabar, así la
        // imagen vieja no puede estar referenciada por un frame en vuelo.
        let wanted_shadow = input
            .shadow_map_size
            .clamp(SHADOW_MAP_MIN, self.gpu.max_image_dimension());
        if wanted_shadow != self.shadow_size {
            let mut size = wanted_shadow;
            // Un tier Ultra pide 8192² de D32 (256 MB). Si el adaptador no lo aguanta, se baja a
            // la mitad en vez de abortar el frame en cada fotograma.
            loop {
                match self.targets.recreate_shadow(&self.gpu, size) {
                    Ok(()) => break,
                    Err(error) if size > SHADOW_MAP_MIN => {
                        log::warn!(
                            "[sombras] no se pudo crear el shadow map de {size} px ({error}); se intenta con la mitad."
                        );
                        size /= 2;
                    }
                    Err(error) => return Err(error),
                }
            }
            self.shadow_size = size;
            self.shadow_layout = DEPTH_ATTACHMENT;
            self.rewrite_target_descriptors()?;
            log::info!("[sombras] shadow map reasignado a {size}x{size} px");
        }
        if self.albedo_image != input.albedo.image {
            self.albedo_set_image(input.albedo)?;
            self.albedo_image = input.albedo.image;
        }
        if let Some(atlas) = input.hud_atlas
            && self.hud_image != atlas.image
        {
            // Igual que el álbum de texturas: el atlas nuevo se enlaza en todos los slots.
            for index in 0..FRAMES {
                write_combined(
                    self.gpu.device(),
                    self.sets.hud[index],
                    0,
                    atlas.view,
                    self.samplers[0],
                    vk::ImageLayout::GENERAL,
                )?;
            }
            self.hud_image = atlas.image;
        }
        let slot = frame.slot.min(FRAMES - 1);
        self.scene_ubo[slot].write(0, input.scene)?;
        self.prepare_uploads(slot, input)?;
        let pipes = self.pipe_handles();
        let device = self.gpu.device().clone();
        let cmd = frame.command;
        // El tier de MSAA puede haber cambiado: se ajustan el G-buffer multisampleado y sus
        // pipelines antes de cualquier pase que dependa del número de muestras. `pipes` se queda
        // con los de 1× porque la captura de sondas dibuja en imágenes de una muestra; solo
        // `forward_pass` cambia a los pipelines MSAA.
        self.ensure_msaa(input.msaa_samples, &device)?;
        self.trace_background(slot, input, &pipes, &device, cmd)?;
        if input.shadows {
            self.shadow_pass(slot, input, &pipes, &device, cmd)?;
        } else {
            // Sin pase de sombras el mapa nunca pasó a READ_ONLY; el descriptor lo espera
            // en ese layout aunque el shader no lo muestree.
            let image = self.targets.shadow.image;
            transition_depth(
                &device,
                cmd,
                image,
                &mut self.shadow_layout,
                DEPTH_READ_ONLY,
            );
        }
        for capture in input.probe_captures {
            self.capture_probe(slot, input, *capture, &pipes, &device, cmd)?;
        }
        self.forward_pass(slot, input, &pipes, &device, cmd)?;
        if input.reflections && input.raytracing {
            self.reflection_pass(slot, input, &pipes, &device, cmd)?;
        }
        self.resolve_present(slot, frame, input, &pipes, &device, cmd)?;
        self.history_slot ^= 1;
        self.taa_frame = self.taa_frame.wrapping_add(1);
        Ok(())
    }
}

impl FrameRenderer {
    /// Transita la imagen del swapchain a `PRESENT_SRC_KHR`.
    ///
    /// La llama el host standalone tras `record`. En ventana única el editor graba egui
    /// encima de la misma imagen y hace la transición al final de su propio pase.
    pub fn finish_present(&self, frame: &FrameTarget) {
        crate::engine3d::vulkan::gpu::cmd_image_barrier(
            self.gpu.device(),
            frame.command,
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
    }
}

impl FrameRenderer {
    /// Enlaza el álbum de álbedos nuevo en TODOS los slots en vuelo.
    pub fn albedo_set_image(&self, albedo: &GpuImage) -> Result<(), VulkanError> {
        for slot in 0..FRAMES {
            write_combined(
                self.gpu.device(),
                self.sets.albedo[slot],
                0,
                albedo.view,
                self.samplers[0],
                vk::ImageLayout::GENERAL,
            )?;
        }
        Ok(())
    }
}
