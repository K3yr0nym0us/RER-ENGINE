//! Frame 3D: raster (sombras, forward, probes, TAA, SSR, denoise), reflejos RT o BVH, gizmo y HUD.
//! El `unsafe` de este archivo es FFI de Vulkan (pipelines, descriptores y barreras).

use std::collections::HashMap;

use ash::vk;
use gpu_allocator::MemoryLocation;

use crate::bvh::RtMaterialGpu;
use crate::config_3d::shadow_cascades::MAX_SHADOW_CASCADES;
use crate::engine::SceneUniforms;

use super::accel::{self, Accel, BlasInput, TlasInstance};
use super::error::{VulkanError, vk_error};
use super::gpu::{FrameTarget, GpuBuffer, GpuImage, SharedGpu};
use super::select::align_up;

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
/// Capas del array de sombras. Fijas: el tier decide cuántas se DIBUJAN, nunca cuántas existen, así
/// que cambiar de tier no toca ni la imagen ni los descriptores del `sampler2DArrayShadow`.
const SHADOW_LAYERS: u32 = MAX_SHADOW_CASCADES as u32;
const MRT: [vk::Format; 5] = [COLOR, COLOR, COLOR, WORLD, VELOCITY];

/// Offsets dentro del UBO de escena. Se derivan de la propia `SceneUniforms`: hasta CSM estaban
/// escritos a mano, y reordenar el struct los habría roto en silencio (el parcheado por cascada y el
/// `Camera` de RT copian bytes crudos).
const SCENE_VIEW_PROJ: usize = std::mem::offset_of!(SceneUniforms, view_proj);
const SCENE_INV_VIEW_PROJ: usize = std::mem::offset_of!(SceneUniforms, inv_view_proj);
const SCENE_CAM_POS: usize = std::mem::offset_of!(SceneUniforms, cam_pos);
const SCENE_LIGHT_DIR: usize = std::mem::offset_of!(SceneUniforms, light_dir);
const SCENE_LIGHT_PARAMS_Y: usize = std::mem::offset_of!(SceneUniforms, light_params) + 4;
const SCENE_LIGHT_VIEW_PROJ: usize = std::mem::offset_of!(SceneUniforms, light_view_proj);
/// Bytes de una matriz del array `light_view_proj[4]`.
const SCENE_MATRIX_BYTES: usize = std::mem::size_of::<[[f32; 4]; 4]>();

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
    /// Cascadas que el tier activo va a dibujar y muestrear (0 = sombras apagadas). El array siempre
    /// tiene `SHADOW_LAYERS` capas; esto dice cuántas se recorren en el pase de sombras.
    pub shadow_cascades: usize,
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

struct Targets {
    extent: vk::Extent2D,
    scene: GpuImage,
    normal: GpuImage,
    albedo_out: GpuImage,
    world: GpuImage,
    velocity: GpuImage,
    depth: GpuImage,
    background: GpuImage,
    reflection: GpuImage,
    ssr: GpuImage,
    denoise: GpuImage,
    refl_history: GpuImage,
    history: [GpuImage; 2],
    shadow: GpuImage,
    probe: GpuImage,
    probe_color: GpuImage,
    probe_normal: GpuImage,
    probe_albedo: GpuImage,
    probe_world: GpuImage,
    probe_velocity: GpuImage,
    probe_depth: GpuImage,
    /// G-buffer multisampleado del pase forward (solo existe si el tier de MSAA pide > 1).
    ms: Option<MultisampleTargets>,
}

/// Copias multisampleadas de los 5 adjuntos del G-buffer + su profundidad.
///
/// Se resuelven contra las imágenes 1× que ya consume TAA/SSR/present, así que el resto del frame
/// no cambia. La profundidad NO se resuelve: se rellena con un segundo pase a 1× (ver
/// `forward_pass`), porque el resolve de un adjunto de profundidad tiene requisitos de layout que
/// no aportan nada frente a volver a rasterizar los lotes opacos.
struct MultisampleTargets {
    samples: u32,
    scene: GpuImage,
    normal: GpuImage,
    albedo: GpuImage,
    world: GpuImage,
    velocity: GpuImage,
    depth: GpuImage,
}

pub struct FrameRenderer {
    gpu: SharedGpu,
    targets: Targets,
    shaders: Vec<vk::ShaderModule>,
    layouts: Vec<vk::PipelineLayout>,
    pipelines: Vec<vk::Pipeline>,
    pool: vk::DescriptorPool,
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
        }
    }
}

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
            host_buffer(gpu, crate::engine::SCENE_UBO_BYTES as u64, "scene-ubo-0")?,
            host_buffer(gpu, crate::engine::SCENE_UBO_BYTES as u64, "scene-ubo-1")?,
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
            shadow_size: crate::engine::SHADOW_MAP_SIZE,
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
            self.shadow_size = crate::engine::SHADOW_MAP_SIZE;
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
            // en ese layout aunque el shader no lo muestree. Se barre el array COMPLETO: las capas
            // que el tier no dibuja siguen existiendo y el sampler las puede indexar.
            let image = self.targets.shadow.image;
            transition_depth_layers(
                &device,
                cmd,
                image,
                &mut self.shadow_layout,
                DEPTH_READ_ONLY,
                SHADOW_LAYERS,
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
    fn forward_pipes(&self, base: Pipes) -> Pipes {
        match self.ms_pipelines {
            Some([forward, forward_blend, skinned, skinned_blend, sky])
                if self.msaa_samples > 1 =>
            {
                let mut pipes = base;
                pipes.forward = forward;
                pipes.forward_blend = forward_blend;
                pipes.skinned = skinned;
                pipes.skinned_blend = skinned_blend;
                pipes.sky = sky;
                pipes
            }
            _ => base,
        }
    }

    /// `Pipes` con los pipelines de solo-profundidad (sin adjuntos de color) para el segundo pase.
    fn depth_pipes(&self, base: Pipes) -> Pipes {
        let mut pipes = base;
        pipes.forward = self.depth_pipelines[0];
        pipes.skinned = self.depth_pipelines[1];
        pipes
    }

    fn pipe_handles(&self) -> Pipes {
        Pipes {
            forward: self.pipelines[0],
            forward_blend: self.pipelines[1],
            skinned: self.pipelines[2],
            skinned_blend: self.pipelines[3],
            shadow: self.pipelines[4],
            shadow_skinned: self.pipelines[5],
            gizmo: self.pipelines[6],
            hud: self.pipelines[7],
            taa: self.pipelines[8],
            ssr: self.pipelines[9],
            denoise: self.pipelines[11],
            composite: self.pipelines[12],
            present: self.pipelines[13],
            gizmo_lines: self.pipelines[14],
            sky: self.pipelines[13],
        }
    }

    fn rewrite_target_descriptors(&self) -> Result<(), VulkanError> {
        let device = self.gpu.device();
        for slot in 0..FRAMES {
            write_combined(
                device,
                self.sets.scene[slot],
                1,
                self.targets.shadow.view,
                self.samplers[1],
                vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            )?;
            write_combined(
                device,
                self.sets.probe[slot],
                0,
                self.targets.probe.view,
                self.samplers[0],
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.sets.sky[slot],
                0,
                self.targets.background.view,
                self.samplers[2],
                vk::ImageLayout::GENERAL,
            )?;
        }
        Ok(())
    }

    /// Ajusta el G-buffer multisampleado y sus pipelines al número de muestras pedido por el tier.
    ///
    /// Se consulta al adaptador (`max_gbuffer_samples`), así que un `Ultra` en una GPU sin soporte
    /// baja a 1× en vez de crear pipelines inválidos. Los pipelines de 1× siguen existiendo porque
    /// la captura de sondas dibuja en imágenes de una muestra.
    fn ensure_msaa(&mut self, requested: u32, device: &ash::Device) -> Result<(), VulkanError> {
        let wanted = self.gpu.max_gbuffer_samples(requested).clamp(1, 8);
        if wanted == self.msaa_samples {
            return Ok(());
        }
        self.targets.ensure_ms(&self.gpu, wanted)?;
        if let Some(old) = self.ms_pipelines.take() {
            for pipeline in old {
                unsafe { device.destroy_pipeline(pipeline, None) };
            }
            // Eran los últimos del registro: se quitan para no destruirlos otra vez en `Drop`.
            for _ in old {
                self.pipelines.pop();
            }
        }
        if wanted > 1 {
            let static_ms = with_samples(static_vertex(), wanted);
            let skinned_ms = with_samples(skinned_vertex(), wanted);
            let created = [
                graphics(
                    device,
                    self.layouts[0],
                    self.forward_vert,
                    Some(self.forward_frag),
                    &static_ms,
                    &MRT,
                    Some(DEPTH),
                    false,
                    false,
                    &mut self.pipelines,
                ),
                graphics(
                    device,
                    self.layouts[0],
                    self.forward_vert,
                    Some(self.forward_frag),
                    &static_ms,
                    &MRT,
                    Some(DEPTH),
                    true,
                    false,
                    &mut self.pipelines,
                ),
                graphics(
                    device,
                    self.layouts[1],
                    self.skinned_vert,
                    Some(self.forward_frag),
                    &skinned_ms,
                    &MRT,
                    Some(DEPTH),
                    false,
                    false,
                    &mut self.pipelines,
                ),
                graphics(
                    device,
                    self.layouts[1],
                    self.skinned_vert,
                    Some(self.forward_frag),
                    &skinned_ms,
                    &MRT,
                    Some(DEPTH),
                    true,
                    false,
                    &mut self.pipelines,
                ),
                // `vkCmdCopyImage` no puede copiar de 1× a 2× (VUID-vkCmdCopyImage-srcImage-00136),
                // así que el fondo trazado se dibuja con un quad full-screen sobre el adjunto
                // multisampleado, sin prueba ni escritura de profundidad.
                graphics_no_depth_test(
                    device,
                    self.layout_full,
                    self.full_vert,
                    Some(self.sky_frag),
                    &with_samples(empty_vertex(), wanted),
                    &MRT,
                    Some(DEPTH),
                    &mut self.pipelines,
                ),
            ];
            let mut built = Vec::new();
            for pipeline in created {
                match pipeline {
                    Ok(pipeline) => built.push(pipeline),
                    Err(error) => {
                        for partial in built {
                            unsafe { device.destroy_pipeline(partial, None) };
                        }
                        return Err(error);
                    }
                }
            }
            let [forward, forward_blend, skinned, skinned_blend, sky] =
                built.try_into().map_err(|_| {
                    VulkanError::new(
                        "No se pudieron crear los pipelines MSAA del forward.".to_string(),
                    )
                })?;
            self.ms_pipelines = Some([forward, forward_blend, skinned, skinned_blend, sky]);
        }
        self.msaa_samples = wanted;
        if wanted > 1 {
            log::info!("[msaa] G-buffer multisampleado a {wanted}x");
            if wanted != requested {
                // Sin este aviso, un Ultra que baja a 4x es indistinguible de un Ultra roto.
                log::warn!(
                    "[msaa] el tier pidió {requested}x y el adaptador llega a {}x como G-buffer \
                     (color y profundidad a la vez): se usa {wanted}x",
                    self.gpu.gbuffer_sample_ceiling()
                );
            }
        } else if requested > 1 {
            log::warn!(
                "[msaa] el tier pidió {requested}x pero la intersección color∩profundidad del \
                 adaptador no da multisampling: el G-buffer trabaja a 1x"
            );
        }
        Ok(())
    }

    fn trace_background(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let _ = (pipes, device, cmd);
        if !input.raytracing {
            // Sin trazado: el shader del BVH con `counts.x == 0` escribe el cielo.
            let view = self.targets.background.view;
            self.dispatch_bvh(slot, input, 0, view, device, cmd)?;
        } else if self.gpu.hw_rt() {
            self.rebuild_tlas(input)?;
            let view = self.targets.background.view;
            self.dispatch_rays(slot, input, 0, view, device, cmd)?;
        } else {
            let view = self.targets.background.view;
            self.dispatch_bvh(slot, input, 0, view, device, cmd)?;
        }
        Ok(())
    }

    fn rebuild_tlas(&mut self, input: &FrameInput<'_>) -> Result<(), VulkanError> {
        if !self.gpu.hw_rt() {
            return Ok(());
        }
        let mut instances = Vec::with_capacity(input.rt.len());
        let mut geos = Vec::with_capacity(input.rt.len());
        let mut materials = Vec::with_capacity(input.rt.len());
        let stride = std::mem::size_of::<RtMaterialGpu>();
        for batch in input.rt.iter() {
            if batch.rebuild {
                self.blas_cache.remove(&batch.key);
            }
            if !self.blas_cache.contains_key(&batch.key) && batch.index_count >= 3 {
                let blas = accel::build_blas(
                    &self.gpu,
                    &BlasInput {
                        vertex_address: batch.vertex.address,
                        index_address: batch.index.address,
                        vertex_stride: batch.vertex_stride,
                        max_vertex: batch.vertex_count.saturating_sub(1),
                        primitive_count: batch.index_count / 3,
                    },
                )?;
                self.blas_cache.insert(batch.key, blas);
            }
            if let Some(blas) = self.blas_cache.get(&batch.key) {
                instances.push(TlasInstance {
                    blas_address: blas.address,
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
        self.tlas = Some(accel::build_tlas(&self.gpu, &instances)?);
        Ok(())
    }

    fn dispatch_rays(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        raygen: u32,
        target: vk::ImageView,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let Some(tlas_handle) = self.tlas.as_ref().map(|tlas| tlas.handle) else {
            return Ok(());
        };
        let Some(rt) = self.gpu.rt_pipeline().cloned() else {
            return Ok(());
        };
        let stride = self.sbt_stride;
        let address = self.sbt_address;
        let camera = rt_camera_bytes(input.scene, self.targets.extent);
        self.rt_ubo[slot].write(0, &camera)?;
        // Un set distinto por uso: ver el comentario de `DescriptorSets::rt`.
        let set = self.sets.rt[slot][raygen as usize % 2];
        write_uniform(device, set, 0, &self.rt_ubo[slot])?;
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
            self.targets.normal.view,
            self.samplers[2],
            vk::ImageLayout::GENERAL,
        )?;
        write_storage(device, set, 6, &self.geos)?;
        let pipeline = self.rt_pipeline;
        let layout = self.rt_layout;
        let extent = self.targets.extent;
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

    fn dispatch_bvh(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        mode: u32,
        target: vk::ImageView,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let mut uniform = rt_camera_bytes(input.scene, self.targets.extent);
        uniform.extend_from_slice(&input.bvh_node_count.to_le_bytes());
        uniform.extend_from_slice(&input.bvh_tri_count.to_le_bytes());
        uniform.extend_from_slice(&mode.to_le_bytes());
        uniform.extend_from_slice(&input.material_count.to_le_bytes());
        self.bvh_ubo[slot].write(0, &uniform)?;
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
        write_uniform(device, set, 0, &self.bvh_ubo[slot])?;
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
            self.targets.normal.view,
            self.samplers[2],
            vk::ImageLayout::GENERAL,
        )?;
        let pipeline = self.bvh_pipeline;
        let layout = self.bvh_layout;
        let extent = self.targets.extent;
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

    fn prepare_uploads(&mut self, slot: usize, input: &FrameInput<'_>) -> Result<(), VulkanError> {
        let mut bytes = Vec::new();
        let mut static_offsets = Vec::new();
        for batch in input.static_batches {
            static_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
        let mut static_transparent_offsets = Vec::new();
        for batch in input.static_transparent_batches {
            static_transparent_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
        let mut skinned_offsets = Vec::new();
        for batch in input.skinned_batches {
            skinned_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
        let mut skinned_transparent_offsets = Vec::new();
        for batch in input.skinned_transparent_batches {
            skinned_transparent_offsets.push(align_up(bytes.len() as u32, 256) as u64);
            let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
            bytes.extend(std::iter::repeat_n(0, pad));
            bytes.extend_from_slice(batch.instances);
        }
        self.hud_offset = align_up(bytes.len() as u32, 256) as u64;
        let pad = align_up(bytes.len() as u32, 256) as usize - bytes.len();
        bytes.extend(std::iter::repeat_n(0, pad));
        bytes.extend_from_slice(input.hud_instances);
        if bytes.is_empty() {
            bytes.extend_from_slice(&[0, 0, 0, 0]);
        }
        let gpu = self.gpu.clone();
        Self::grow_host(&gpu, &mut self.instances[slot], bytes.len() as u64, "inst")?;
        self.instances[slot].write(0, &bytes)?;
        let mut joints = Vec::new();
        let mut joint_bases = Vec::new();
        let mut joint_bases_transparent = Vec::new();
        for batch in input.skinned_batches {
            joint_bases.push((joints.len() / 64) as u32);
            for matrix in batch.palette {
                for column in matrix {
                    joints.extend_from_slice(bytemuck::cast_slice(column));
                }
            }
        }
        for batch in input.skinned_transparent_batches {
            joint_bases_transparent.push((joints.len() / 64) as u32);
            for matrix in batch.palette {
                for column in matrix {
                    joints.extend_from_slice(bytemuck::cast_slice(column));
                }
            }
        }
        if joints.is_empty() {
            joints.extend_from_slice(&[0, 0, 0, 0]);
        }
        Self::grow_host(&gpu, &mut self.joints[slot], joints.len() as u64, "joints")?;
        self.joints[slot].write(0, &joints)?;
        write_storage(
            self.gpu.device(),
            self.sets.joints[slot],
            0,
            &self.joints[slot],
        )?;
        self.static_offsets = static_offsets;
        self.static_transparent_offsets = static_transparent_offsets;
        self.skinned_offsets = skinned_offsets;
        self.skinned_transparent_offsets = skinned_transparent_offsets;
        self.joint_bases = joint_bases;
        self.joint_bases_transparent = joint_bases_transparent;
        Ok(())
    }

    fn grow_host(
        gpu: &SharedGpu,
        buffer: &mut GpuBuffer,
        size: u64,
        name: &str,
    ) -> Result<(), VulkanError> {
        if buffer.size >= size {
            return Ok(());
        }
        *buffer = host_buffer(gpu, size.next_power_of_two(), name)?;
        Ok(())
    }

    /// Dibuja cada cascada en su propia capa del array de sombras.
    ///
    /// `shadow.vert` proyecta contra `light_view_proj[0]`, así que por cascada se reescribe ESA
    /// ranura del UBO con `cmd_update_buffer` — el mismo truco que la captura de sondas: la escritura
    /// va dentro del command buffer, en orden, así que cada pase ve SU matriz. Al terminar se
    /// restaura el bloque original porque el forward necesita las 4 cascadas completas.
    ///
    /// La barrera de layout abarca TODAS las capas de una sola vez: pasar de adjunto a lectura capa a
    /// capa dejaría las restantes en un layout que el descriptor no declara (contenido indefinido).
    fn shadow_pass(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let count = input
            .shadow_cascades
            .clamp(1, MAX_SHADOW_CASCADES)
            .min(SHADOW_LAYERS as usize);
        let shadow_image = self.targets.shadow.image;
        let extent = self.targets.shadow.extent;
        transition_depth_layers(
            device,
            cmd,
            shadow_image,
            &mut self.shadow_layout,
            DEPTH_ATTACHMENT,
            SHADOW_LAYERS,
        );
        for cascade in 0..count {
            let mut scene = input.scene.to_vec();
            Self::patch_cascade_matrix(&mut scene, cascade);
            if !scene.is_empty() {
                unsafe { device.cmd_update_buffer(cmd, self.scene_ubo[slot].buffer, 0, &scene) };
                buffer_barrier(device, cmd, self.scene_ubo[slot].buffer);
            }
            begin_depth(
                device,
                cmd,
                self.targets.shadow.layer_view(cascade as u32),
                extent,
            );
            set_viewport(device, cmd, extent);
            self.draw_depth(
                slot,
                input,
                pipes.shadow,
                pipes.shadow_skinned,
                self.layouts[2],
                self.layouts[3],
                device,
                cmd,
            );
            unsafe { device.cmd_end_rendering(cmd) };
        }
        // Se devuelve el bloque tal cual lo armó `State`: el forward elige cascada por profundidad,
        // así que necesita las 4 matrices, no la que quedó escrita en el último pase.
        if !input.scene.is_empty() {
            unsafe { device.cmd_update_buffer(cmd, self.scene_ubo[slot].buffer, 0, input.scene) };
            buffer_barrier(device, cmd, self.scene_ubo[slot].buffer);
        }
        transition_depth_layers(
            device,
            cmd,
            shadow_image,
            &mut self.shadow_layout,
            DEPTH_READ_ONLY,
            SHADOW_LAYERS,
        );
        Ok(())
    }

    /// Lleva la matriz de la cascada `index` a la ranura 0 del array, dentro de los bytes del UBO.
    fn patch_cascade_matrix(scene: &mut [u8], index: usize) {
        let matrix = SCENE_MATRIX_BYTES;
        let source = SCENE_LIGHT_VIEW_PROJ + index * matrix;
        let Some(source_end) = source.checked_add(matrix) else {
            return;
        };
        let target_end = SCENE_LIGHT_VIEW_PROJ + matrix;
        if source_end > scene.len() || target_end > scene.len() {
            return;
        }
        scene.copy_within(source..source_end, SCENE_LIGHT_VIEW_PROJ);
    }

    /// Captura una sonda completa (6 caras) dentro de su ranura del cubemap array.
    fn capture_probe(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        capture: ProbeCapture,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let face_size = crate::probe_registry::PROBE_FACE_SIZE;
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
        const LIGHT_PARAMS_Y: usize = SCENE_LIGHT_PARAMS_Y;
        let base_layer = (capture.slot.min(crate::probe_registry::MAX_PROBES - 1) as u32) * 6;
        for face in 0..6 {
            let mut scene = input.scene.to_vec();
            let vp = cube_face_view_proj(capture.origin, face);
            if scene.len() >= SCENE_VIEW_PROJ + 64 {
                scene[SCENE_VIEW_PROJ..SCENE_VIEW_PROJ + 64]
                    .copy_from_slice(bytemuck::bytes_of(&vp));
            }
            // Sin IBL durante la captura: evita muestrear el cubemap que se está escribiendo.
            if scene.len() >= LIGHT_PARAMS_Y + 4 {
                scene[LIGHT_PARAMS_Y..LIGHT_PARAMS_Y + 4].copy_from_slice(&0f32.to_le_bytes());
            }
            if !scene.is_empty() {
                unsafe { device.cmd_update_buffer(cmd, self.scene_ubo[slot].buffer, 0, &scene) };
                buffer_barrier(device, cmd, self.scene_ubo[slot].buffer);
            }
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

    fn forward_pass(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let extent = self.targets.extent;
        let resolve = [
            self.targets.scene.view,
            self.targets.normal.view,
            self.targets.albedo_out.view,
            self.targets.world.view,
            self.targets.velocity.view,
        ];
        // `ms.samples == msaa_samples`: si el G-buffer no se recreó aún para este nivel, se cae
        // al camino de 1× en vez de mezclar pipelines y adjuntos con muestras distintas.
        let msaa = self.msaa_samples > 1
            && self
                .targets
                .ms
                .as_ref()
                .is_some_and(|ms| ms.samples == self.msaa_samples);
        // Sin MSAA el fondo se copia 1:1 a la escena. Con MSAA no se puede copiar (Vulkan exige el
        // mismo número de muestras en origen y destino, VUID-vkCmdCopyImage-srcImage-00136), así
        // que el quad del cielo se dibuja dentro del propio pase multisampleado.
        if !msaa {
            copy_color(
                device,
                cmd,
                &self.targets.background,
                &self.targets.scene,
                extent,
                0,
            );
        }
        let color_pipes = self.forward_pipes(*pipes);
        // Vistas multisampleadas copiadas fuera del borrow: los pases necesitan `&mut self` para
        // registrar los layouts de profundidad y `self.targets.ms` prestado no convive con eso.
        let ms_pack: Option<([vk::ImageView; 5], vk::ImageView)> = if msaa {
            self.targets.ms.as_ref().map(|ms| {
                (
                    [
                        ms.scene.view,
                        ms.normal.view,
                        ms.albedo.view,
                        ms.world.view,
                        ms.velocity.view,
                    ],
                    ms.depth.view,
                )
            })
        } else {
            None
        };
        match ms_pack {
            Some((ms_views, ms_depth)) => {
                // `clear = true`: el adjunto de escena empieza vacío y el quad del cielo lo pinta.
                begin_color_ms(device, cmd, &ms_views, ms_depth, &resolve, extent, true);
                set_viewport(device, cmd, extent);
                unsafe {
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, color_pipes.sky);
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.layout_full,
                        0,
                        &[self.sets.sky[slot]],
                        &[],
                    );
                    device.cmd_draw(cmd, 3, 1, 0, 0);
                }
                self.draw_forward(slot, input, &color_pipes, device, cmd, 0.0);
                self.draw_forward(slot, input, &color_pipes, device, cmd, 1.0);
                unsafe { device.cmd_end_rendering(cmd) };
                // La profundidad del pase MSAA se descarta: se vuelve a rasterizar la geometría
                // opaca contra `targets.depth` con el fragment shader del forward, para conservar
                // el `discard` del alfa cortado y no resolver un adjunto de profundidad.
                let depth_image = self.targets.depth.image;
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_ATTACHMENT,
                );
                begin_depth(device, cmd, self.targets.depth.view, extent);
                set_viewport(device, cmd, extent);
                let depth_pipes = self.depth_pipes(*pipes);
                self.draw_forward(slot, input, &depth_pipes, device, cmd, 0.0);
                unsafe { device.cmd_end_rendering(cmd) };
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_READ_ONLY,
                );
            }
            _ => {
                let views = resolve;
                let depth_image = self.targets.depth.image;
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_ATTACHMENT,
                );
                begin_color(
                    device,
                    cmd,
                    &views,
                    Some(self.targets.depth.view),
                    extent,
                    false,
                );
                set_viewport(device, cmd, extent);
                self.draw_forward(slot, input, &color_pipes, device, cmd, 0.0);
                self.draw_forward(slot, input, &color_pipes, device, cmd, 1.0);
                unsafe { device.cmd_end_rendering(cmd) };
                // El trazado de reflejos, el SSR, el denoise y la sonda muestrean esta imagen.
                transition_depth(
                    device,
                    cmd,
                    depth_image,
                    &mut self.depth_layout,
                    DEPTH_READ_ONLY,
                );
            }
        }
        Ok(())
    }

    fn draw_depth(
        &self,
        slot: usize,
        input: &FrameInput<'_>,
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
        self.draw_indexed(
            slot,
            input.static_batches.iter().map(|batch| {
                (
                    batch.vertex,
                    batch.index,
                    batch.index_count,
                    batch.instance_count,
                )
            }),
            &self.static_offsets,
            device,
            cmd,
        );
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
        for (index, batch) in input.skinned_batches.iter().enumerate() {
            let push = push_bytes(self.joint_bases.get(index).copied().unwrap_or(0), 0.0);
            unsafe {
                device.cmd_push_constants(
                    cmd,
                    skinned_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    &push,
                );
            }
            let offset = self.skinned_offsets.get(index).copied().unwrap_or(0);
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

    fn draw_forward(
        &self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pass_kind: f32,
    ) {
        let pipeline = if pass_kind < 0.5 {
            pipes.forward
        } else {
            pipes.forward_blend
        };
        let skinned = if pass_kind < 0.5 {
            pipes.skinned
        } else {
            pipes.skinned_blend
        };
        let push = push_bytes(0, pass_kind);
        let static_batches = if pass_kind < 0.5 {
            input.static_batches
        } else {
            input.static_transparent_batches
        };
        let skinned_batches = if pass_kind < 0.5 {
            input.skinned_batches
        } else {
            input.skinned_transparent_batches
        };
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layouts[0],
                0,
                &[
                    self.sets.scene[slot],
                    self.sets.albedo[slot],
                    self.sets.empty,
                    self.sets.probe[slot],
                ],
                &[],
            );
            device.cmd_push_constants(
                cmd,
                self.layouts[0],
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                &push,
            );
        }
        let offsets = if pass_kind < 0.5 {
            &self.static_offsets
        } else {
            &self.static_transparent_offsets
        };
        self.draw_indexed(
            slot,
            static_batches.iter().map(|batch| {
                (
                    batch.vertex,
                    batch.index,
                    batch.index_count,
                    batch.instance_count,
                )
            }),
            offsets,
            device,
            cmd,
        );
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, skinned);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layouts[1],
                0,
                &[
                    self.sets.scene[slot],
                    self.sets.albedo[slot],
                    self.sets.joints[slot],
                    self.sets.probe[slot],
                ],
                &[],
            );
        }
        let joint_bases = if pass_kind < 0.5 {
            &self.joint_bases
        } else {
            &self.joint_bases_transparent
        };
        let skinned_offsets = if pass_kind < 0.5 {
            &self.skinned_offsets
        } else {
            &self.skinned_transparent_offsets
        };
        for (index, batch) in skinned_batches.iter().enumerate() {
            let push = push_bytes(joint_bases.get(index).copied().unwrap_or(0), pass_kind);
            unsafe {
                device.cmd_push_constants(
                    cmd,
                    self.layouts[1],
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    &push,
                );
            }
            let offset = skinned_offsets.get(index).copied().unwrap_or(0);
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

    fn draw_indexed<'a>(
        &self,
        slot: usize,
        batches: impl Iterator<Item = (&'a GpuBuffer, &'a GpuBuffer, u32, u32)>,
        offsets: &[u64],
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) {
        for (index, (vertex, index_buffer, index_count, instance_count)) in batches.enumerate() {
            let offset = offsets.get(index).copied().unwrap_or(0);
            draw_one(
                device,
                cmd,
                vertex,
                index_buffer,
                index_count,
                instance_count,
                &self.instances[slot],
                offset,
            );
        }
    }

    fn reflection_pass(
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
        super::gpu::cmd_image_barrier(
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
        super::gpu::cmd_image_barrier(
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

    fn resolve_present(
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
                self.targets.history[history].view,
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
                1.0
            };
            push[..4].copy_from_slice(&blend.to_le_bytes());
            push[4..8].copy_from_slice(&1.0f32.to_le_bytes());
            let target = if input.reflections {
                self.targets.refl_history.view
            } else {
                self.targets.history[slot].view
            };
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
        super::gpu::cmd_image_barrier(
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
        begin_swapchain(device, cmd, frame.color_view, extent);
        set_viewport(device, cmd, extent);
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
            begin_swapchain_load(device, cmd, frame.color_view, extent);
            set_viewport(device, cmd, extent);
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
            begin_swapchain_load(device, cmd, frame.color_view, extent);
            set_viewport(device, cmd, extent);
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
        super::gpu::cmd_image_barrier(
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
}

fn push_bytes(joint_base: u32, pass_kind: f32) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[..4].copy_from_slice(&joint_base.to_le_bytes());
    out[4..].copy_from_slice(&pass_kind.to_le_bytes());
    out
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
    let scissor = vk::Rect2D {
        offset: vk::Offset2D::default(),
        extent,
    };
    unsafe {
        device.cmd_set_viewport(cmd, 0, &[viewport]);
        device.cmd_set_scissor(cmd, 0, &[scissor]);
    }
}

fn clear_color(rgba: [f32; 4]) -> vk::ClearValue {
    vk::ClearValue {
        color: vk::ClearColorValue { float32: rgba },
    }
}

/// Borra una imagen de color en `GENERAL` con barreras de memoria (sin cambio de layout).
/// Deja definido el cubemap de probes (6 caras) con el color de cielo.
fn clear_probe_cube(gpu: &SharedGpu, image: vk::Image) -> Result<(), VulkanError> {
    gpu.one_shot(move |device, cmd| {
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: crate::probe_registry::PROBE_CUBE_ARRAY_LAYERS,
        };
        unsafe {
            device.cmd_clear_color_image(
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                &vk::ClearColorValue {
                    float32: [0.72, 0.86, 0.98, 1.0],
                },
                &[range],
            );
        }
    })
}

fn clear_image_color(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    extent: vk::Extent2D,
    rgba: [f32; 4],
) {
    super::gpu::cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::AccessFlags::SHADER_WRITE
            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::SHADER_READ,
        vk::AccessFlags::TRANSFER_WRITE,
    );
    let range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    unsafe {
        device.cmd_clear_color_image(
            cmd,
            image,
            vk::ImageLayout::GENERAL,
            &vk::ClearColorValue { float32: rgba },
            &[range],
        );
    }
    super::gpu::cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::COLOR,
        1,
        1,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
    );
    let _ = extent;
}

fn begin_depth(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
) {
    let depth = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
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
        .layer_count(1)
        .depth_attachment(&depth);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

/// Inicia el pase forward sobre el G-buffer multisampleado y resuelve cada adjunto sobre su
/// imagen 1× (la que ya consumen TAA/SSR/present). La profundidad multisampleada se descarta:
/// `targets.depth` se rellena después con un segundo pase a 1×.
fn begin_color_ms(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    ms_views: &[vk::ImageView; 5],
    ms_depth: vk::ImageView,
    resolve: &[vk::ImageView; 5],
    extent: vk::Extent2D,
    clear: bool,
) {
    let colors: Vec<vk::RenderingAttachmentInfo> = ms_views
        .iter()
        .zip(resolve)
        .enumerate()
        .map(|(index, (view, resolve_view))| {
            let value = if index == 0 {
                [0.72, 0.86, 0.98, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            vk::RenderingAttachmentInfo::default()
                .image_view(*view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(if clear || index > 0 {
                    vk::AttachmentLoadOp::CLEAR
                } else {
                    vk::AttachmentLoadOp::LOAD
                })
                .store_op(vk::AttachmentStoreOp::STORE)
                .resolve_image_view(*resolve_view)
                .resolve_image_layout(vk::ImageLayout::GENERAL)
                // `resolveMode != NONE` es lo que hace que el contenido multisampleado llegue a la
                // imagen 1×; con NONE el resolve no ocurre y la imagen 1× quedaría vacía.
                .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                .clear_value(clear_color(value))
        })
        .collect();
    let depth_info = vk::RenderingAttachmentInfo::default()
        .image_view(ms_depth)
        .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
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
        .layer_count(1)
        .color_attachments(&colors)
        .depth_attachment(&depth_info);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

fn begin_color(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    views: &[vk::ImageView; 5],
    depth: Option<vk::ImageView>,
    extent: vk::Extent2D,
    clear: bool,
) {
    let colors: Vec<vk::RenderingAttachmentInfo> = views
        .iter()
        .enumerate()
        .map(|(index, view)| {
            let value = if index == 0 {
                [0.72, 0.86, 0.98, 1.0]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            vk::RenderingAttachmentInfo::default()
                .image_view(*view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(if clear || index > 0 {
                    vk::AttachmentLoadOp::CLEAR
                } else {
                    vk::AttachmentLoadOp::LOAD
                })
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(clear_color(value))
        })
        .collect();
    let depth_info = depth.map(|view| {
        vk::RenderingAttachmentInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            })
    });
    let mut info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&colors);
    if let Some(depth_info) = depth_info.as_ref() {
        info = info.depth_attachment(depth_info);
    }
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

fn begin_swapchain(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
) {
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_color([0.72, 0.86, 0.98, 1.0]));
    let colors = [color];
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&colors);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

fn begin_swapchain_load(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
) {
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE);
    let colors = [color];
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&colors);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

/// Array de profundidad para CSM: `SHADOW_LAYERS` capas, una por cascada, con su vista por capa.
///
/// Se asignan SIEMPRE las 4 capas, aunque el tier dibuje 2 o 3: así cambiar de tier sólo reasigna
/// resolución y el `sampler2DArrayShadow` del descriptor nunca cambia de forma.
fn create_shadow_array(gpu: &SharedGpu, size: u32) -> Result<GpuImage, VulkanError> {
    let mut image = gpu.create_image_layers(
        vk::Extent2D {
            width: size,
            height: size,
        },
        SHADOW_LAYERS,
        1,
        DEPTH,
        vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        vk::ImageAspectFlags::DEPTH,
        "shadow",
    )?;
    image.attach_layer_views("sombras")?;
    Ok(image)
}

/// El descriptor y el adjunto declaran un layout; si la imagen no está realmente en ese layout,
/// lo que lee la GPU es indefinido (los bloques negros del cielo) y la validación lo reporta.
/// Esta es la ÚNICA función que cambia layouts de profundidad y deja el layout real registrado en
/// `current`, así no se desincroniza como el booleano "ya fue muestreada" que había antes.
/// Las máscaras son conservadoras porque la misma imagen alterna adjunto, lectura por compute/RT
/// y lectura por fragment; solo se emite barrera cuando el layout cambia de verdad.
fn transition_depth(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    current: &mut vk::ImageLayout,
    want: vk::ImageLayout,
) {
    transition_depth_layers(device, cmd, image, current, want, 1);
}

/// Igual que `transition_depth` pero sobre `layers` capas del array.
///
/// El shadow map es un array y TODAS sus capas comparten un único layout registrado: si la barrera
/// sólo cubriera la capa 0, las cascadas 1..3 quedarían en ATTACHMENT mientras el descriptor las
/// declara READ_ONLY, y el resultado es lectura indefinida (más el error de validación correspondiente).
fn transition_depth_layers(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    current: &mut vk::ImageLayout,
    want: vk::ImageLayout,
    layers: u32,
) {
    if *current == want {
        return;
    }
    super::gpu::cmd_image_barrier(
        device,
        cmd,
        image,
        vk::ImageAspectFlags::DEPTH,
        1,
        layers,
        *current,
        want,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
        vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
    );
    *current = want;
}

fn copy_color(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    src: &GpuImage,
    dst: &GpuImage,
    extent: vk::Extent2D,
    layer: u32,
) {
    super::gpu::cmd_image_barrier(
        device,
        cmd,
        src.image,
        vk::ImageAspectFlags::COLOR,
        src.mips,
        src.layers,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::AccessFlags::SHADER_WRITE,
        vk::AccessFlags::TRANSFER_READ,
    );
    super::gpu::cmd_image_barrier(
        device,
        cmd,
        dst.image,
        vk::ImageAspectFlags::COLOR,
        dst.mips,
        dst.layers,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::AccessFlags::SHADER_READ,
        vk::AccessFlags::TRANSFER_WRITE,
    );
    let region = vk::ImageCopy::default()
        .src_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .dst_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: layer,
            layer_count: 1,
        })
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        });
    unsafe {
        device.cmd_copy_image(
            cmd,
            src.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            dst.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        )
    };
    super::gpu::cmd_image_barrier(
        device,
        cmd,
        src.image,
        vk::ImageAspectFlags::COLOR,
        src.mips,
        src.layers,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::TRANSFER_READ,
        vk::AccessFlags::SHADER_READ,
    );
    super::gpu::cmd_image_barrier(
        device,
        cmd,
        dst.image,
        vk::ImageAspectFlags::COLOR,
        dst.mips,
        dst.layers,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::GENERAL,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
    );
}

fn draw_one(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    vertex: &GpuBuffer,
    index: &GpuBuffer,
    index_count: u32,
    instances: u32,
    instance_buffer: &GpuBuffer,
    instance_offset: u64,
) {
    if index_count == 0 || instances == 0 {
        return;
    }
    unsafe {
        device.cmd_bind_vertex_buffers(
            cmd,
            0,
            &[vertex.buffer, instance_buffer.buffer],
            &[0, instance_offset],
        );
        device.cmd_bind_index_buffer(cmd, index.buffer, 0, vk::IndexType::UINT32);
        device.cmd_draw_indexed(cmd, index_count, instances, 0, 0, 0);
    }
}

fn draw_fullscreen(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set: vk::DescriptorSet,
    target: vk::ImageView,
    extent: vk::Extent2D,
    push: &[u8],
) {
    begin_color_one(device, cmd, target, extent);
    set_viewport(device, cmd, extent);
    unsafe {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            0,
            &[set],
            &[],
        );
        if !push.is_empty() {
            device.cmd_push_constants(
                cmd,
                layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                push,
            );
        }
        device.cmd_draw(cmd, 3, 1, 0, 0);
        device.cmd_end_rendering(cmd);
    }
}

fn begin_color_one(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    view: vk::ImageView,
    extent: vk::Extent2D,
) {
    let color = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(vk::ImageLayout::GENERAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_color([0.0, 0.0, 0.0, 0.0]));
    let colors = [color];
    let info = vk::RenderingInfo::default()
        .render_area(vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        })
        .layer_count(1)
        .color_attachments(&colors);
    unsafe { device.cmd_begin_rendering(cmd, &info) };
}

fn buffer_barrier(device: &ash::Device, cmd: vk::CommandBuffer, buffer: vk::Buffer) {
    let barrier = vk::BufferMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::UNIFORM_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffer)
        .offset(0)
        .size(vk::WHOLE_SIZE);
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::VERTEX_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[barrier],
            &[],
        );
    }
}

fn cube_face_view_proj(origin: [f32; 3], face: usize) -> [[f32; 4]; 4] {
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

fn rt_camera_bytes(scene: &[u8], extent: vk::Extent2D) -> Vec<u8> {
    let mut out = vec![0u8; 112];
    // Los índices salen de `SceneUniforms`, no de números escritos a mano: ver `SCENE_*` arriba.
    let inv_end = SCENE_INV_VIEW_PROJ + 64;
    let cam_end = SCENE_CAM_POS + 16;
    let light_end = SCENE_LIGHT_DIR + 16;
    if scene.len() >= inv_end {
        out[..64].copy_from_slice(&scene[SCENE_INV_VIEW_PROJ..inv_end]);
        if scene.len() >= cam_end {
            out[64..80].copy_from_slice(&scene[SCENE_CAM_POS..cam_end]);
        }
        if scene.len() >= light_end {
            out[96..112].copy_from_slice(&scene[SCENE_LIGHT_DIR..light_end]);
        }
    }
    let resolution = [extent.width as f32, extent.height as f32, 0.0, 0.0];
    out[80..96].copy_from_slice(bytemuck::bytes_of(&resolution));
    out
}

fn host_buffer(gpu: &SharedGpu, size: u64, name: &str) -> Result<GpuBuffer, VulkanError> {
    gpu.create_buffer(
        size,
        vk::BufferUsageFlags::UNIFORM_BUFFER
            | vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
            | vk::BufferUsageFlags::VERTEX_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST,
        MemoryLocation::CpuToGpu,
        name,
    )
}

struct SetLayouts {
    scene: vk::DescriptorSetLayout,
    albedo: vk::DescriptorSetLayout,
    probe: vk::DescriptorSetLayout,
    joints: vk::DescriptorSetLayout,
    empty: vk::DescriptorSetLayout,
    gizmo: vk::DescriptorSetLayout,
    hud: vk::DescriptorSetLayout,
    full: vk::DescriptorSetLayout,
    taa: vk::DescriptorSetLayout,
    denoise: vk::DescriptorSetLayout,
    rt: vk::DescriptorSetLayout,
    bvh: vk::DescriptorSetLayout,
}

fn create_sets(
    device: &ash::Device,
) -> Result<(vk::DescriptorPool, SetLayouts, DescriptorSets), VulkanError> {
    let scene = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ],
    )?;
    let albedo = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::ShaderStageFlags::FRAGMENT,
        )],
    )?;
    let probe = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::ShaderStageFlags::FRAGMENT,
        )],
    )?;
    let joints = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::STORAGE_BUFFER,
            vk::ShaderStageFlags::VERTEX,
        )],
    )?;
    let empty = set_layout(device, &[])?;
    let gizmo = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::ShaderStageFlags::VERTEX,
        )],
    )?;
    let hud = set_layout(
        device,
        &[binding(
            0,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::ShaderStageFlags::FRAGMENT,
        )],
    )?;
    let full = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                2,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                3,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ],
    )?;
    let taa = full;
    let denoise = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                1,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                2,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ],
    )?;
    let rt_stages = vk::ShaderStageFlags::RAYGEN_KHR
        | vk::ShaderStageFlags::MISS_KHR
        | vk::ShaderStageFlags::CLOSEST_HIT_KHR;
    let rt = set_layout(
        device,
        &[
            binding(0, vk::DescriptorType::UNIFORM_BUFFER, rt_stages),
            binding(1, vk::DescriptorType::STORAGE_IMAGE, rt_stages),
            binding(2, vk::DescriptorType::ACCELERATION_STRUCTURE_KHR, rt_stages),
            binding(3, vk::DescriptorType::STORAGE_BUFFER, rt_stages),
            binding(4, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, rt_stages),
            binding(5, vk::DescriptorType::COMBINED_IMAGE_SAMPLER, rt_stages),
            binding(6, vk::DescriptorType::STORAGE_BUFFER, rt_stages),
        ],
    )?;
    let bvh = set_layout(
        device,
        &[
            binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                1,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                2,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                3,
                vk::DescriptorType::STORAGE_BUFFER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                5,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::COMPUTE,
            ),
            binding(
                6,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::COMPUTE,
            ),
        ],
    )?;
    let pool = unsafe {
        device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(128)
                .pool_sizes(&[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::UNIFORM_BUFFER,
                        descriptor_count: 64,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        descriptor_count: 128,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: 64,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_IMAGE,
                        descriptor_count: 16,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                        descriptor_count: 4,
                    },
                ]),
            None,
        )
    }
    .map_err(|error| vk_error("crear el pool de descriptores", error))?;
    let layouts = SetLayouts {
        scene,
        albedo,
        probe,
        joints,
        empty,
        gizmo,
        hud,
        full,
        taa,
        denoise,
        rt,
        bvh,
    };
    let sets = DescriptorSets {
        scene: [
            alloc_set(device, pool, scene)?,
            alloc_set(device, pool, scene)?,
        ],
        albedo: [
            alloc_set(device, pool, albedo)?,
            alloc_set(device, pool, albedo)?,
        ],
        probe: [
            alloc_set(device, pool, probe)?,
            alloc_set(device, pool, probe)?,
        ],
        joints: [
            alloc_set(device, pool, joints)?,
            alloc_set(device, pool, joints)?,
        ],
        empty: alloc_set(device, pool, empty)?,
        gizmo: [
            alloc_set(device, pool, gizmo)?,
            alloc_set(device, pool, gizmo)?,
        ],
        hud: [alloc_set(device, pool, hud)?, alloc_set(device, pool, hud)?],
        rt: [
            [alloc_set(device, pool, rt)?, alloc_set(device, pool, rt)?],
            [alloc_set(device, pool, rt)?, alloc_set(device, pool, rt)?],
        ],
        bvh: [
            [alloc_set(device, pool, bvh)?, alloc_set(device, pool, bvh)?],
            [alloc_set(device, pool, bvh)?, alloc_set(device, pool, bvh)?],
        ],
        taa: [alloc_set(device, pool, taa)?, alloc_set(device, pool, taa)?],
        ssr: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
        denoise: [
            alloc_set(device, pool, denoise)?,
            alloc_set(device, pool, denoise)?,
        ],
        composite: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
        present: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
        sky: [
            alloc_set(device, pool, full)?,
            alloc_set(device, pool, full)?,
        ],
    };
    Ok((pool, layouts, sets))
}

fn binding(
    slot: u32,
    ty: vk::DescriptorType,
    stages: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(slot)
        .descriptor_type(ty)
        .descriptor_count(1)
        .stage_flags(stages)
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
    .map_err(|error| vk_error("crear un descriptor set layout", error))
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
    .map_err(|error| vk_error("reservar un descriptor set", error))
    .map(|sets| sets[0])
}

fn pipeline_layout(
    device: &ash::Device,
    sets: &[vk::DescriptorSetLayout],
    push: u32,
) -> Result<vk::PipelineLayout, VulkanError> {
    let range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(push);
    let ranges = if push == 0 {
        &[][..]
    } else {
        std::slice::from_ref(&range)
    };
    unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(sets)
                .push_constant_ranges(ranges),
            None,
        )
    }
    .map_err(|error| vk_error("crear un pipeline layout", error))
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
    .map_err(|error| vk_error("crear un shader module", error))?;
    keep.push(module);
    Ok(module)
}

fn sampler(
    device: &ash::Device,
    _compare: bool,
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
                .max_lod(16.0),
            None,
        )
    }
    .map_err(|error| vk_error("crear un sampler", error))
}

fn compare_sampler(device: &ash::Device) -> Result<vk::Sampler, VulkanError> {
    unsafe {
        device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .compare_enable(true)
                .compare_op(vk::CompareOp::LESS_OR_EQUAL),
            None,
        )
    }
    .map_err(|error| vk_error("crear el sampler de sombras", error))
}

struct VertexSpec {
    bindings: Vec<vk::VertexInputBindingDescription>,
    attributes: Vec<vk::VertexInputAttributeDescription>,
    /// Muestras del framebuffer al que se dibuja (1 = sin MSAA). Se mete aquí en vez de añadir un
    /// argumento más a `graphics()` porque Vulkan exige que `rasterizationSamples` coincida con el
    /// `sampleCount` de TODOS los adjuntos del render pass.
    samples: u32,
}

/// Copia una spec de vértices con otro número de muestras (para los pipelines MSAA del forward).
fn with_samples(mut spec: VertexSpec, samples: u32) -> VertexSpec {
    spec.samples = samples.max(1);
    spec
}

/// `sample_count` (1/2/4/8/16) a banderas de Vulkan. Cualquier otro valor se interpreta como 1×.
fn sample_flags(samples: u32) -> vk::SampleCountFlags {
    match samples {
        2 => vk::SampleCountFlags::TYPE_2,
        4 => vk::SampleCountFlags::TYPE_4,
        8 => vk::SampleCountFlags::TYPE_8,
        16 => vk::SampleCountFlags::TYPE_16,
        _ => vk::SampleCountFlags::TYPE_1,
    }
}

fn static_vertex() -> VertexSpec {
    VertexSpec {
        bindings: vec![
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: 32,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: 96,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ],
        attributes: vec![
            attr(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
            attr(1, 0, vk::Format::R32G32B32_SFLOAT, 12),
            attr(2, 0, vk::Format::R32G32_SFLOAT, 24),
            attr(3, 1, vk::Format::R32G32B32A32_SFLOAT, 0),
            attr(4, 1, vk::Format::R32G32B32A32_SFLOAT, 16),
            attr(5, 1, vk::Format::R32G32B32A32_SFLOAT, 32),
            attr(6, 1, vk::Format::R32G32B32A32_SFLOAT, 48),
            attr(7, 1, vk::Format::R32G32B32A32_SFLOAT, 64),
            attr(8, 1, vk::Format::R32G32B32A32_SFLOAT, 80),
        ],
        samples: 1,
    }
}

fn skinned_vertex() -> VertexSpec {
    VertexSpec {
        bindings: vec![
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: 64,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: 96,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ],
        attributes: vec![
            attr(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
            attr(1, 0, vk::Format::R32G32B32_SFLOAT, 12),
            attr(2, 0, vk::Format::R32G32_SFLOAT, 24),
            attr(3, 0, vk::Format::R32G32B32A32_UINT, 32),
            attr(4, 0, vk::Format::R32G32B32A32_SFLOAT, 48),
            attr(5, 1, vk::Format::R32G32B32A32_SFLOAT, 0),
            attr(6, 1, vk::Format::R32G32B32A32_SFLOAT, 16),
            attr(7, 1, vk::Format::R32G32B32A32_SFLOAT, 32),
            attr(8, 1, vk::Format::R32G32B32A32_SFLOAT, 48),
            attr(9, 1, vk::Format::R32G32B32A32_SFLOAT, 64),
            attr(10, 1, vk::Format::R32G32B32A32_SFLOAT, 80),
        ],
        samples: 1,
    }
}

fn gizmo_vertex() -> VertexSpec {
    VertexSpec {
        bindings: vec![vk::VertexInputBindingDescription {
            binding: 0,
            stride: 28,
            input_rate: vk::VertexInputRate::VERTEX,
        }],
        attributes: vec![
            attr(0, 0, vk::Format::R32G32B32_SFLOAT, 0),
            attr(1, 0, vk::Format::R32G32B32A32_SFLOAT, 12),
        ],
        samples: 1,
    }
}

fn empty_vertex() -> VertexSpec {
    VertexSpec {
        bindings: Vec::new(),
        attributes: Vec::new(),
        samples: 1,
    }
}

fn attr(
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
fn graphics(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    blend: bool,
    depth_bias: bool,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    graphics_with_depth_test(
        device,
        layout,
        vert,
        frag,
        vertex,
        colors,
        depth,
        blend,
        depth_bias,
        vk::PrimitiveTopology::TRIANGLE_LIST,
        true,
        keep,
    )
}

/// Igual que `graphics` pero sin prueba ni escritura de profundidad.
///
/// Lo usa el quad del cielo dentro del pase MSAA: el adjunto de profundidad existe (el resto de
/// adjuntos sí la usan), pero el fondo no debe tapar la geometría que se dibuja después.
#[allow(clippy::too_many_arguments)]
fn graphics_no_depth_test(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    graphics_with_depth_test(
        device,
        layout,
        vert,
        frag,
        vertex,
        colors,
        depth,
        false,
        false,
        vk::PrimitiveTopology::TRIANGLE_LIST,
        false,
        keep,
    )
}

#[allow(clippy::too_many_arguments)]
fn graphics_topology(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    blend: bool,
    depth_bias: bool,
    topology: vk::PrimitiveTopology,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    graphics_with_depth_test(
        device, layout, vert, frag, vertex, colors, depth, blend, depth_bias, topology, true, keep,
    )
}

#[allow(clippy::too_many_arguments)]
fn graphics_with_depth_test(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: Option<vk::ShaderModule>,
    vertex: &VertexSpec,
    colors: &[vk::Format],
    depth: Option<vk::Format>,
    blend: bool,
    depth_bias: bool,
    topology: vk::PrimitiveTopology,
    depth_test: bool,
    keep: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    let name = c"main";
    let mut stages = vec![
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(name),
    ];
    if let Some(frag) = frag {
        stages.push(
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag)
                .name(name),
        );
    }
    let vertex_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&vertex.bindings)
        .vertex_attribute_descriptions(&vertex.attributes);
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(topology);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(if depth_bias {
            vk::CullModeFlags::BACK
        } else {
            vk::CullModeFlags::NONE
        })
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .depth_bias_enable(depth_bias)
        .depth_bias_constant_factor(if depth_bias { 1.5 } else { 0.0 })
        .depth_bias_slope_factor(if depth_bias { 2.0 } else { 0.0 });
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(sample_flags(vertex.samples));
    let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(depth.is_some() && depth_test)
        .depth_write_enable(depth.is_some() && depth_test && !blend)
        .depth_compare_op(vk::CompareOp::LESS);
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
    if let Some(depth) = depth {
        rendering = rendering.depth_attachment_format(depth);
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
    let pipeline =
        match unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        {
            Ok(mut created) => created.remove(0),
            Err((created, error)) => {
                for pipeline in created {
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
                return Err(vk_error("crear un pipeline gráfico", error));
            }
        };
    keep.push(pipeline);
    Ok(pipeline)
}

fn create_compute(
    device: &ash::Device,
    bytes: &[u8],
    layout_set: vk::DescriptorSetLayout,
    shaders: &mut Vec<vk::ShaderModule>,
    layouts: &mut Vec<vk::PipelineLayout>,
    pipelines: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    let module = shader(device, bytes, shaders)?;
    let layout = pipeline_layout(device, &[layout_set], 0)?;
    layouts.push(layout);
    let name = c"main";
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(name);
    let info = vk::ComputePipelineCreateInfo::default()
        .stage(stage)
        .layout(layout);
    let pipeline = match unsafe {
        device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None)
    } {
        Ok(mut created) => created.remove(0),
        Err((created, error)) => {
            for pipeline in created {
                unsafe { device.destroy_pipeline(pipeline, None) };
            }
            return Err(vk_error("crear el pipeline de BVH", error));
        }
    };
    pipelines.push(pipeline);
    Ok(pipeline)
}

fn create_rt_pipeline(
    gpu: &SharedGpu,
    set: &vk::DescriptorSetLayout,
    shaders: &mut Vec<vk::ShaderModule>,
    layouts: &mut Vec<vk::PipelineLayout>,
    pipelines: &mut Vec<vk::Pipeline>,
) -> Result<(vk::Pipeline, GpuBuffer, u32, u64), VulkanError> {
    let device = gpu.device();
    let rt = gpu.rt_pipeline().ok_or_else(|| {
        VulkanError::new("El device no cargó VK_KHR_ray_tracing_pipeline.".to_string())
    })?;
    let rgen = shader(device, spv("primary.rgen"), shaders)?;
    let reflect = shader(device, spv("reflect.rgen"), shaders)?;
    let miss = shader(device, spv("primary.rmiss"), shaders)?;
    let hit = shader(device, spv("primary.rchit"), shaders)?;
    let name = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::RAYGEN_KHR)
            .module(rgen)
            .name(name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::RAYGEN_KHR)
            .module(reflect)
            .name(name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::MISS_KHR)
            .module(miss)
            .name(name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
            .module(hit)
            .name(name),
    ];
    let unused = vk::SHADER_UNUSED_KHR;
    let groups = [
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
            .general_shader(0)
            .closest_hit_shader(unused)
            .any_hit_shader(unused)
            .intersection_shader(unused),
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
            .general_shader(1)
            .closest_hit_shader(unused)
            .any_hit_shader(unused)
            .intersection_shader(unused),
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
            .general_shader(2)
            .closest_hit_shader(unused)
            .any_hit_shader(unused)
            .intersection_shader(unused),
        vk::RayTracingShaderGroupCreateInfoKHR::default()
            .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
            .general_shader(unused)
            .closest_hit_shader(3)
            .any_hit_shader(unused)
            .intersection_shader(unused),
    ];
    let layout = pipeline_layout(device, &[*set], 0)?;
    layouts.push(layout);
    let info = vk::RayTracingPipelineCreateInfoKHR::default()
        .stages(&stages)
        .groups(&groups)
        .max_pipeline_ray_recursion_depth(1)
        .layout(layout);
    let pipeline = match unsafe {
        rt.create_ray_tracing_pipelines(
            vk::DeferredOperationKHR::null(),
            vk::PipelineCache::null(),
            &[info],
            None,
        )
    } {
        Ok(mut created) => created.remove(0),
        Err((created, error)) => {
            for pipeline in created {
                if pipeline != vk::Pipeline::null() {
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
            }
            return Err(vk_error("crear el pipeline de ray tracing", error));
        }
    };
    pipelines.push(pipeline);
    let props = gpu.rt_properties().ok_or_else(|| {
        VulkanError::new("Faltan las propiedades de ray tracing del device.".to_string())
    })?;
    let handle_size = props.shader_group_handle_size;
    let stride = align_up(handle_size, props.shader_group_handle_alignment.max(1))
        .max(props.shader_group_base_alignment.max(1));
    let handles = unsafe {
        rt.get_ray_tracing_shader_group_handles(pipeline, 0, 4, 4 * handle_size as usize)
    }
    .map_err(|error| vk_error("leer los handles del SBT", error))?;
    let mut table = vec![0u8; stride as usize * 4];
    for group in 0..4 {
        let src = group * handle_size as usize;
        let dst = group * stride as usize;
        table[dst..dst + handle_size as usize]
            .copy_from_slice(&handles[src..src + handle_size as usize]);
    }
    let buffer = gpu.create_buffer_with_data(
        &table,
        vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        MemoryLocation::CpuToGpu,
        "sbt",
    )?;
    let address = buffer.address;
    Ok((pipeline, buffer, stride, address))
}

fn write_uniform(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    buffer: &GpuBuffer,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorBufferInfo {
        buffer: buffer.buffer,
        offset: 0,
        range: buffer.size,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

fn write_storage(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    buffer: &GpuBuffer,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorBufferInfo {
        buffer: buffer.buffer,
        offset: 0,
        range: buffer.size,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .buffer_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

fn write_combined(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    view: vk::ImageView,
    sampler: vk::Sampler,
    layout: vk::ImageLayout,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorImageInfo {
        sampler,
        image_view: view,
        image_layout: layout,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

fn write_storage_image(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    view: vk::ImageView,
) -> Result<(), VulkanError> {
    let info = vk::DescriptorImageInfo {
        sampler: vk::Sampler::null(),
        image_view: view,
        image_layout: vk::ImageLayout::GENERAL,
    };
    let write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
        .image_info(std::slice::from_ref(&info));
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

fn write_acceleration(
    device: &ash::Device,
    set: vk::DescriptorSet,
    binding: u32,
    accel: vk::AccelerationStructureKHR,
) -> Result<(), VulkanError> {
    let structures = [accel];
    let mut write_as = vk::WriteDescriptorSetAccelerationStructureKHR::default()
        .acceleration_structures(&structures);
    let mut write = vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
        .push_next(&mut write_as);
    write.descriptor_count = 1;
    unsafe { device.update_descriptor_sets(&[write], &[]) };
    Ok(())
}

impl Targets {
    fn new(gpu: &SharedGpu, extent: vk::Extent2D) -> Result<Self, VulkanError> {
        let extent = vk::Extent2D {
            width: extent.width.max(1),
            height: extent.height.max(1),
        };
        let color_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        let color = |name| {
            gpu.create_image(
                extent,
                COLOR,
                color_usage,
                vk::ImageAspectFlags::COLOR,
                name,
            )
        };
        let scene = color("scene")?;
        let normal = color("normal")?;
        let albedo_out = color("albedo-out")?;
        let world = gpu.create_image(
            extent,
            WORLD,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "world",
        )?;
        let velocity = gpu.create_image(
            extent,
            VELOCITY,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "velocity",
        )?;
        let depth = gpu.create_image(
            extent,
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "depth",
        )?;
        let background = color("rt-background")?;
        let reflection = color("reflection")?;
        let ssr = color("ssr")?;
        let denoise = color("denoise")?;
        let refl_history = color("refl-history")?;
        let history = [color("history-0")?, color("history-1")?];
        let shadow = create_shadow_array(gpu, crate::engine::SHADOW_MAP_SIZE)?;
        // Un cubemap array: 6 caras por ranura. Con un solo cubemap compartido, la última sonda
        // capturada del frame tapaba a las demás y todas las entidades recibían el mismo entorno.
        let probe = gpu.create_image_layers(
            vk::Extent2D {
                width: crate::probe_registry::PROBE_FACE_SIZE,
                height: crate::probe_registry::PROBE_FACE_SIZE,
            },
            crate::probe_registry::PROBE_CUBE_ARRAY_LAYERS,
            1,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-cube-array",
        )?;
        let probe_extent = vk::Extent2D {
            width: crate::probe_registry::PROBE_FACE_SIZE,
            height: crate::probe_registry::PROBE_FACE_SIZE,
        };
        let probe_color = gpu.create_image(
            probe_extent,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-face",
        )?;
        let probe_normal = gpu.create_image(
            probe_extent,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-n",
        )?;
        let probe_albedo = gpu.create_image(
            probe_extent,
            COLOR,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-a",
        )?;
        let probe_world = gpu.create_image(
            probe_extent,
            WORLD,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-w",
        )?;
        let probe_velocity = gpu.create_image(
            probe_extent,
            VELOCITY,
            color_usage,
            vk::ImageAspectFlags::COLOR,
            "probe-v",
        )?;
        let probe_depth = gpu.create_image(
            probe_extent,
            DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            "probe-depth",
        )?;
        Ok(Self {
            extent,
            scene,
            normal,
            albedo_out,
            world,
            velocity,
            depth,
            background,
            reflection,
            ssr,
            denoise,
            refl_history,
            history,
            shadow,
            probe,
            probe_color,
            probe_normal,
            probe_albedo,
            probe_world,
            probe_velocity,
            probe_depth,
            ms: None,
        })
    }

    /// Crea (o recrea) el G-buffer multisampleado para `samples` muestras.
    fn ensure_ms(&mut self, gpu: &SharedGpu, samples: u32) -> Result<(), VulkanError> {
        if samples <= 1 {
            // Soltar las imágenes liberadas por `Drop` de `GpuImage`.
            self.ms = None;
            return Ok(());
        }
        let ms_samples = sample_flags(samples);
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        let ms_color = |name| {
            gpu.create_image_with_samples(
                self.extent,
                COLOR,
                ms_samples,
                usage,
                vk::ImageAspectFlags::COLOR,
                name,
            )
        };
        self.ms = Some(MultisampleTargets {
            samples,
            scene: ms_color("scene-ms")?,
            normal: ms_color("normal-ms")?,
            albedo: ms_color("albedo-ms")?,
            world: gpu.create_image_with_samples(
                self.extent,
                WORLD,
                ms_samples,
                usage,
                vk::ImageAspectFlags::COLOR,
                "world-ms",
            )?,
            velocity: gpu.create_image_with_samples(
                self.extent,
                VELOCITY,
                ms_samples,
                usage,
                vk::ImageAspectFlags::COLOR,
                "velocity-ms",
            )?,
            depth: gpu.create_image_with_samples(
                self.extent,
                DEPTH,
                ms_samples,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                vk::ImageAspectFlags::DEPTH,
                "depth-ms",
            )?,
        });
        Ok(())
    }

    fn recreate(&mut self, gpu: &SharedGpu, extent: vk::Extent2D) -> Result<(), VulkanError> {
        *self = Self::new(gpu, extent)?;
        Ok(())
    }

    /// Reasigna solo el shadow map al lado pedido por el tier. La imagen vieja se libera con su
    /// `Drop` (destruye las vistas por capa, la view completa, la image y el bloque del allocator).
    fn recreate_shadow(&mut self, gpu: &SharedGpu, size: u32) -> Result<(), VulkanError> {
        self.shadow = create_shadow_array(gpu, size)?;
        Ok(())
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
