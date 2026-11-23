//! Frame 3D: raster (sombras, forward, probes, TAA, SSR, denoise), reflejos RT o BVH, gizmo y HUD.
//! El `unsafe` de este archivo es FFI de Vulkan (pipelines, descriptores y barreras).

use std::collections::HashMap;

use ash::vk;
use gpu_allocator::MemoryLocation;

use crate::engine3d::bvh::RtMaterialGpu;
use crate::engine3d::config_3d::world_bounds::SkyUniform;

use super::accel::{self, Accel, BlasInput, TlasInstance, TlasSlot};
use super::error::{VulkanError, vk_error};
use super::select::align_up;
use crate::engine3d::vulkan::gpu::{FrameTarget, GpuBuffer, GpuImage, SharedGpu};

const FRAMES: usize = 2;
/// Máximo de pasadas extra de denoise (tier Ultra = 4 pasadas en total → 3 extra).
const MAX_EXTRA_DENOISE_PASSES: usize = 3;
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

/// Extent del G-buffer para SSAA: `viewport × escala`, recortado de forma uniforme para que
/// ninguna dimensión supere el límite de imagen del adaptador (así se conserva el aspecto).
fn supersampled_extent(extent: vk::Extent2D, scale: f32, max_dimension: u32) -> vk::Extent2D {
    let limit = max_dimension.max(1) as f32;
    let scale = (scale.max(1.0))
        .min(limit / extent.width.max(1) as f32)
        .min(limit / extent.height.max(1) as f32)
        .max(1.0);
    vk::Extent2D {
        width: (extent.width as f32 * scale).round().max(1.0) as u32,
        height: (extent.height as f32 * scale).round().max(1.0) as u32,
    }
}

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
        "sky_bg.frag" => include_bytes!(concat!(env!("OUT_DIR"), "/sky_bg.frag.spv")),
        // Partículas GPU-driven.
        "particle_spawn_sim" => include_bytes!(concat!(env!("OUT_DIR"), "/spawn_sim.comp.spv")),
        "particle_build_indirect" => {
            include_bytes!(concat!(env!("OUT_DIR"), "/build_indirect.comp.spv"))
        }
        "particle_render.vert" => {
            include_bytes!(concat!(env!("OUT_DIR"), "/particle_render.vert.spv"))
        }
        "particle_render.frag" => {
            include_bytes!(concat!(env!("OUT_DIR"), "/particle_render.frag.spv"))
        }
        // WBOIT (F3): acumulación + resolve.
        "particle_wboit.frag" => {
            include_bytes!(concat!(env!("OUT_DIR"), "/particle_wboit.frag.spv"))
        }
        "particle_wboit_resolve.frag" => {
            include_bytes!(concat!(env!("OUT_DIR"), "/particle_wboit_resolve.frag.spv"))
        }
        // Sombras volumétricas.
        "volume_splat" => include_bytes!(concat!(env!("OUT_DIR"), "/volume_splat.comp.spv")),
        "volume_density" => include_bytes!(concat!(env!("OUT_DIR"), "/volume_density.comp.spv")),
        "volume_light" => include_bytes!(concat!(env!("OUT_DIR"), "/volume_light.comp.spv")),
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

/// Lote de partículas de un emisor para este frame. `read_is_a` ya viene decidido por el
/// `sim_step` PROPIO del emisor (precisión 1 del plan); el pool GPU lo consume tal cual.
pub struct ParticleBatch {
    pub id: u64,
    /// Presupuesto del pool (capacidad); el pool GPU crece/recicla según esto.
    pub max_particles: u32,
    /// Partículas nuevas este frame (CPU: rate·dt con acarreo).
    pub spawn: u32,
    /// 1 = el pool reinicia contadores/indirecto de este emisor antes de simular
    /// (clima reemplazado con el mismo id / restauración de escena, F6).
    pub reset: u32,
    /// Emisión estimada del frame siguiente (`build_indirect.comp` la usa para el dispatch).
    pub next_spawn: u32,
    /// Delta time ya acotado (≤ 0.05 s).
    pub dt: f32,
    /// Semilla RNG del emisor (avanza por frame).
    pub seed: u32,
    /// 1 = este frame leyó A y escribió B.
    pub read_is_a: u32,
    /// 0 = blending aditivo (fuego), 1 = alpha estándar (lluvia/nieve, F2).
    pub blend_alpha: u32,
    /// 1 = acumulador WBOIT (humo/niebla/agua, F3); 0 = pase directo.
    pub wboit: u32,
    /// 1 = soft particles contra la depth del forward (F3).
    pub soft: u32,
    /// Parámetros del emisor (160 B, espejo std140 del GLSL).
    pub params: crate::engine3d::particles::EmitterParams,
}

/// Cámara para el pase de billboards: el jitter TAA de `view_proj` es el mismo que
/// rasterizó el forward, así que los billboards no parpadean respecto a la escena.
#[derive(Clone, Copy, Debug)]
pub struct ParticleCamera {
    pub view_proj: [[f32; 4]; 4],
    /// xyz = derecha de cámara en mundo, w reservado.
    pub cam_right: [f32; 4],
    /// xyz = arriba de cámara en mundo, w reservado.
    pub cam_up: [f32; 4],
    /// `[near, far]` de la cámara: linealización de la depth para soft particles (F3).
    pub near_far: [f32; 2],
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
    /// Esfera de cielo: degradado azul oscuro → azul cielo con brillo alineado al sol.
    pub sky: SkyUniform,
    /// Sombra solo mientras el sol está sobre el horizonte (si no, el mapa se
    /// capturaría desde debajo del suelo y se rompería).
    pub sun_above_horizon: bool,
    pub shadows: bool,
    /// Lado del shadow map (px) exigido por el tier de sombras. `FrameRenderer` reasigna el mapa
    /// cuando cambia; sin esto el tier solo movía un número en el log.
    pub shadow_map_size: u32,
    /// Muestras de MSAA pedidas por el tier (1 = sin multisampling). `FrameRenderer` lo recorta
    /// a lo que el adaptador admite y recrea el G-buffer multisampleado.
    pub msaa_samples: u32,
    /// Escala de SSAA pedida por el tier (1 = sin supersampling). El G-buffer se rasteriza a
    /// `viewport × escala` y se reduce al presentar; `FrameRenderer` la recorta al límite de imagen.
    pub ssaa_scale: f32,
    pub taa_enabled: bool,
    pub taa_blend: f32,
    /// Cualquier sistema de reflejos activo (SSR o RT).
    pub reflections: bool,
    /// `true`: se ejecuta la cadena SSR. `false`: solo RT (o ninguno).
    pub ssr: bool,
    /// `true`: el pase de reflejos traza rayos (HW o BVH). `false`: solo SSR.
    pub raytracing: bool,
    /// Parámetros reales del tier RTX (resolución, rayos, rebotes, denoise...).
    pub rtx: crate::engine3d::config_3d::reflection_graphics::RtxPreset,
    /// Parámetros reales del tier SSR (marcha, búsqueda binaria, denoise...).
    pub ssr_preset: crate::engine3d::config_3d::reflection_graphics::SsrPreset,
    /// Sondas a capturar este frame, cada una con su ranura del cubemap array.
    pub probe_captures: &'a [ProbeCapture],
    /// Vista de debug: en vez del resultado final se presenta la imagen SSR en crudo.
    pub debug_ssr_view: bool,
    /// Vista de debug: presenta la imagen de reflejo trazado en crudo.
    pub debug_rt_view: bool,
    pub static_batches: &'a [MeshBatch<'a>],
    /// Casters opacos. Suele ser el mismo backing que `static_batches` (prefijo
    /// más extra off-camera); `prepare_uploads` no vuelve a copiar el prefijo.
    pub shadow_static_batches: &'a [MeshBatch<'a>],
    /// Casters skinned (frustum ampliado). Reutilizan bytes/paleta del forward
    /// cuando la entidad ya está en ese pase.
    pub shadow_skinned_batches: &'a [SkinnedBatch<'a>],
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
    /// Lotes de partículas GPU-driven (F1: fuego). El pool GPU crece/recicla por `id`.
    pub particles: &'a [ParticleBatch],
    /// Cámara del pase de billboards (misma `view_proj` con jitter que el forward).
    pub particle_camera: ParticleCamera,
    /// Centro del volumen de sombras volumétricas (sigue a la cámara, como la niebla).
    pub volumetric_center: [f32; 3],
    /// Dirección hacia la luz (para la marcha desde cada voxel).
    pub volumetric_light_dir: [f32; 3],
    /// Resolución/pasos/blend del volumen derivados del tier de sombras (0 = desactivado).
    pub volumetric_resolution: u32,
    pub volumetric_steps: u32,
    pub volumetric_blend: f32,
}

pub struct FrameRenderer {
    gpu: SharedGpu,
    targets: Targets,
    shaders: Vec<vk::ShaderModule>,
    layouts: Vec<vk::PipelineLayout>,
    pipelines: Vec<vk::Pipeline>,
    pool: vk::DescriptorPool,
    /// Los `VkDescriptorSetLayout` del renderer. Se guardan porque hay que destruirlos: el leak que
    /// reportaba `vkDestroyDevice(): N leaked objects` al cerrar eran exactamente estos. Son 13
    /// campos pero 12 objetos (`taa` comparte layout con `full`; `sky_bg` es propio).
    set_layouts: SetLayouts,
    samplers: Vec<vk::Sampler>,
    sets: DescriptorSets,
    scene_ubo: [GpuBuffer; FRAMES],
    gizmo_ubo: [GpuBuffer; FRAMES],
    /// UBO del pase de reflejos (resolución reducida según el tier RTX).
    rt_ubo: [GpuBuffer; FRAMES],
    /// UBO del pase de fondo (resolución completa). No puede compartir buffer con `rt_ubo`:
    /// ambos sets apuntan al mismo buffer y el último write ganaría para los dos dispatches.
    rt_ubo_bg: [GpuBuffer; FRAMES],
    bvh_ubo: [GpuBuffer; FRAMES],
    /// UBO del fondo en el fallback BVH (misma razón que `rt_ubo_bg`).
    bvh_ubo_bg: [GpuBuffer; FRAMES],
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
    /// Cielo raster (`sky_bg.frag`) cuando RT está apagado: no despacha BVH vacío.
    sky_bg_pipeline: vk::Pipeline,
    sky_bg_layout: vk::PipelineLayout,
    sbt_stride: u32,
    sbt_address: u64,
    blas_cache: HashMap<u64, Accel>,
    /// Un TLAS por slot en vuelo: el fence del slot ya esperó, así se puede UPDATE in-place.
    tlas: [TlasSlot; FRAMES],
    history_slot: usize,
    taa_frame: u32,
    taa_history_valid: bool,
    /// Última combinación SSR/RT (bit 0 = SSR, bit 1 = RT) para invalidar el historial TAA.
    last_reflection_mode: u8,
    /// Escala de resolución del target de reflejos RT (del tier RTX).
    rt_scale: f32,
    /// Escala de resolución del target SSR (del tier SSR).
    ssr_scale: f32,
    /// Calidad de BVH con la que se construyó la caché de BLAS.
    blas_fast_trace: bool,
    static_offsets: Vec<u64>,
    /// Offsets de los lotes que escriben el shadow map. Es un SUBCONJUNTO filtrado
    /// de `static_batches`, así que no puede usar `static_offsets` (índice distinto).
    shadow_offsets: Vec<u64>,
    static_transparent_offsets: Vec<u64>,
    skinned_offsets: Vec<u64>,
    /// Offsets y bases de paleta de los casters skinned del shadow map.
    shadow_skinned_offsets: Vec<u64>,
    shadow_joint_bases: Vec<u32>,
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
    /// Escala de SSAA activa (1 = sin supersampling). Redimensiona el G-buffer respecto al viewport
    /// y el `present` reduce esa imagen al tamaño nativo con filtrado bilineal.
    ssaa_scale: f32,
    /// `[forward, forward_blend, skinned, skinned_blend]` creados con `msaa_samples` muestras.
    ms_pipelines: Option<[vk::Pipeline; 5]>,
    /// `[depth_only, depth_only_skinned]` a 1×: rellenan `targets.depth` tras el pase MSAA.
    depth_pipelines: [vk::Pipeline; 2],
    /// Pool GPU del sistema de partículas (buffers, pipelines y descriptores dedicados).
    particle_pool: ParticlePool,
    /// Volúmenes de densidad/transmitancia que proyectan la sombra del humo y la niebla
    /// sobre las superficies opacas.
    volumetric: VolumetricState,
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
    /// Un set DISTINTO por pasada extra de denoise: actualizar un set ya enlazado en el mismo
    /// command buffer invalida todo el buffer (VUID sin UPDATE_AFTER_BIND).
    denoise_extra: [[vk::DescriptorSet; MAX_EXTRA_DENOISE_PASSES]; FRAMES],
    composite: [vk::DescriptorSet; FRAMES],
    present: [vk::DescriptorSet; FRAMES],
    /// Fondo trazado para el quad del cielo del pase MSAA (misma layout que `full`).
    sky: [vk::DescriptorSet; FRAMES],
    /// UBO de cámara/cielo del triángulo `sky_bg` (un set por slot en vuelo).
    sky_bg: [vk::DescriptorSet; FRAMES],
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
mod particles;
mod pipelines;
mod present;
mod reflections;
mod rt;
mod shadow;
mod targets;
mod volumetric;
use self::commands::*;
use self::layouts::*;
use self::particles::ParticlePool;
use self::pipelines::*;
use self::targets::*;
use self::volumetric::VolumetricState;

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
        let rt_ubo_bg = [
            host_buffer(gpu, 256, "rt-ubo-bg-0")?,
            host_buffer(gpu, 256, "rt-ubo-bg-1")?,
        ];
        let bvh_ubo = [
            host_buffer(gpu, 256, "bvh-ubo-0")?,
            host_buffer(gpu, 256, "bvh-ubo-1")?,
        ];
        let bvh_ubo_bg = [
            host_buffer(gpu, 256, "bvh-ubo-bg-0")?,
            host_buffer(gpu, 256, "bvh-ubo-bg-1")?,
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
            write_uniform(device, sets.sky_bg[slot], 0, &rt_ubo_bg[slot])?;
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
        // `full` lo comparten SSR (push de 32 bytes con los parámetros del preset), composite
        // y present; un rango de push sin usar por estos últimos es inocuo.
        let full_layout = pipeline_layout(device, &[set_layouts.full], 32)?;
        let taa_layout = pipeline_layout(device, &[set_layouts.taa], 8)?;
        let denoise_layout = pipeline_layout(device, &[set_layouts.denoise], 4)?;
        let sky_bg_layout = pipeline_layout(device, &[set_layouts.sky_bg], 0)?;
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
            sky_bg_layout,
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
        let forward_blend = graphics_blend_color_only(
            device,
            forward_layout,
            vert,
            Some(frag),
            &static_vertex(),
            &MRT,
            Some(DEPTH),
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
        let skinned_blend = graphics_blend_color_only(
            device,
            skinned_layout,
            skinned_vert,
            Some(frag),
            &skinned_vertex(),
            &MRT,
            Some(DEPTH),
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
        // Después del registro indexado (`pipe_handles` 0..=14) y de depth_only: el MSAA
        // hace pop de los últimos, y meter esto entre taa y present desplazaba los índices.
        let sky_bg_frag = shader(device, spv("sky_bg.frag"), &mut shaders)?;
        let sky_bg_pipeline = graphics(
            device,
            sky_bg_layout,
            full_vert,
            Some(sky_bg_frag),
            &empty_vertex(),
            &[COLOR],
            None,
            false,
            false,
            &mut pipelines,
        )?;
        // Pool GPU de partículas: pipelines y descriptores dedicados, sin tocar el registro
        // indexado de `pipelines` del renderer (el MSAA hace pop de los últimos).
        let volume = VolumetricState::new(gpu, 32, 8, 0.0)?;
        let volume_ubos = [volume.ubo(0).buffer, volume.ubo(1).buffer];
        let atomic_views = [volume.atomic_view(0), volume.atomic_view(1)];
        let mut particle_pool =
            ParticlePool::new(gpu, targets.depth.view, volume_ubos, atomic_views)?;
        // Publicamos la textura 3D en el set de escena (bindings 2/3): es lo que el
        // forward consulta para proyectar la sombra del medio sobre las superficies.
        for (slot, set) in sets.scene.iter().enumerate() {
            write_combined(
                device,
                *set,
                2,
                volume.light_view(slot),
                volume.sampler(),
                vk::ImageLayout::GENERAL,
            )?;
            write_uniform(device, *set, 3, volume.ubo(slot))?;
        }
        // Enlace inicial de la depth (soft particles) y de los samplers del resolve WBOIT;
        // `rewrite_target_descriptors` lo repite cuando los targets se recrean.
        particle_pool.rewrite_targets(
            targets.scene.view,
            targets.particles_accum.view,
            targets.particles_reveal.view,
            targets.depth.view,
        )?;
        // El volumen existe desde el primer frame con resolución mínima; `record` ajusta
        // su resolución al tier activo y reenlaza los descriptores cuando cambia.
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
            ssaa_scale: 1.0,
            ms_pipelines: None,
            forward_vert: vert,
            forward_frag: frag,
            skinned_vert,
            depth_pipelines,
            particle_pool,
            volumetric: volume,
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
            rt_ubo_bg,
            bvh_ubo,
            bvh_ubo_bg,
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
            sky_bg_pipeline,
            sky_bg_layout,
            sbt_stride,
            sbt_address,
            blas_cache: HashMap::new(),
            tlas: [TlasSlot::new(), TlasSlot::new()],
            history_slot: 0,
            taa_frame: 0,
            taa_history_valid: false,
            last_reflection_mode: 0,
            rt_scale: 1.0,
            ssr_scale: 1.0,
            blas_fast_trace: false,
            static_offsets: Vec::new(),
            shadow_offsets: Vec::new(),
            static_transparent_offsets: Vec::new(),
            skinned_offsets: Vec::new(),
            shadow_skinned_offsets: Vec::new(),
            shadow_joint_bases: Vec::new(),
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

    /// El fondo (raygen/mode 0) SIEMPRE se traza al extent completo: el preset RTX solo
    /// reduce el pase de reflejos (mode 1). Reducir el fondo partía el cielo por la mitad.
    pub(super) fn rt_pass_extent(&self, use_rt_scale: bool) -> vk::Extent2D {
        if use_rt_scale {
            self.rt_extent()
        } else {
            self.targets.extent
        }
    }

    /// Extent del target SSR según la resolución del tier SSR.
    pub(super) fn ssr_extent(&self) -> vk::Extent2D {
        let base = self.targets.extent;
        let scale = self.ssr_scale.clamp(0.1, 1.0);
        vk::Extent2D {
            width: ((base.width as f32) * scale).round().max(1.0) as u32,
            height: ((base.height as f32) * scale).round().max(1.0) as u32,
        }
    }

    /// Extent del target de reflejos RT según la resolución del tier RTX.
    pub(super) fn rt_extent(&self) -> vk::Extent2D {
        let base = self.targets.extent;
        let scale = self.rt_scale.clamp(0.1, 1.0);
        vk::Extent2D {
            width: ((base.width as f32) * scale).round().max(1.0) as u32,
            height: ((base.height as f32) * scale).round().max(1.0) as u32,
        }
    }

    pub fn record(
        &mut self,
        frame: &FrameTarget,
        input: &FrameInput<'_>,
    ) -> Result<(), VulkanError> {
        // SSAA: el G-buffer se rasteriza a `viewport × escala`. El tier puede cambiar en caliente
        // igual que MSAA, así que se recalcula el extent antes de decidir si hay que recrear los
        // targets; el pase `present` reduce de vuelta al extent nativo de `frame`.
        if (self.ssaa_scale - input.ssaa_scale).abs() > 1e-4 {
            self.ssaa_scale = input.ssaa_scale.max(1.0);
        }
        let render_extent = supersampled_extent(
            frame.extent,
            self.ssaa_scale,
            self.gpu.max_image_dimension(),
        );
        if self.targets.extent != render_extent {
            self.targets.recreate(&self.gpu, render_extent)?;
            // Las imágenes nuevas nacen en su layout inicial (DEPTH → ATTACHMENT); el registro
            // anterior ya no describe la realidad y hay que reiniciarlo.
            self.depth_layout = DEPTH_ATTACHMENT;
            self.shadow_layout = DEPTH_ATTACHMENT;
            // `recreate` también destruye el G-buffer multisampleado y el shadow map en su tamaño
            // base: hay que volver a crearlos, si no MSAA (ni el tamaño SSAA) se apaga solo tras
            // un resize porque `ensure_msaa` ve el mismo nivel de muestras y sale por su early-return.
            self.targets.ensure_ms(&self.gpu, self.msaa_samples)?;
            self.shadow_size = crate::engine3d::engine::SHADOW_MAP_SIZE;
            self.rewrite_target_descriptors()?;
        }
        // Resolución RTX: se reasigna solo el target de reflejos, sin tocar el G-buffer.
        if (self.rt_scale - input.rtx.resolution_scale).abs() > 1e-4 {
            self.rt_scale = input.rtx.resolution_scale.clamp(0.1, 1.0);
            let extent = self.rt_extent();
            self.targets.resize_rt(&self.gpu, extent)?;
        } else if self.targets.reflection.extent != self.rt_extent() {
            // Tras un recreate de targets el reflection vuelve al extent base.
            let extent = self.rt_extent();
            self.targets.resize_rt(&self.gpu, extent)?;
        }
        // Resolución SSR: se reasigna solo su target, sin tocar el G-buffer ni el de RT.
        if (self.ssr_scale - input.ssr_preset.resolution_scale).abs() > 1e-4 {
            self.ssr_scale = input.ssr_preset.resolution_scale.clamp(0.1, 1.0);
            let extent = self.ssr_extent();
            self.targets.resize_ssr(&self.gpu, extent)?;
        } else if self.targets.ssr.extent != self.ssr_extent() {
            let extent = self.ssr_extent();
            self.targets.resize_ssr(&self.gpu, extent)?;
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
        // Partículas: compute de simulación ANTES de cualquier pase gráfico (fill del
        // contador de salida + sim + build_indirect + barreras Compute -> Graphics).
        if !input.particles.is_empty() {
            self.particle_pool
                .sim(slot, input.particles, &device, cmd)?;
        }
        // Sombras volumétricas: la calidad la fija el tier de sombras y el pase sigue el
        // mismo interruptor que el shadow map (`shadows && sun_above_horizon`).
        let volume_wanted =
            input.volumetric_resolution > 0 && input.shadows && input.sun_above_horizon;
        let volume_rebound = self.volumetric.ensure(
            &self.gpu,
            input.volumetric_resolution.max(8),
            input.volumetric_steps,
            input.volumetric_blend,
        )?;
        // Al cambiar el tier se reasignan las imágenes: reenlazamos sus descriptores antes
        // de que el splat y el forward las usen en este mismo frame.
        if volume_rebound {
            self.particle_pool
                .rewrite_volumetric_views(&self.volumetric)?;
            let sampler = self.volumetric.sampler();
            let light_views = [self.volumetric.light_view(0), self.volumetric.light_view(1)];
            let ubos = [self.volumetric.ubo(0).buffer, self.volumetric.ubo(1).buffer];
            self.rewrite_scene_volume_descriptors(sampler, light_views, ubos)?;
        }
        let volume = &mut self.volumetric;
        let casters: Vec<(u32, vk::DescriptorSet, u32)> = if volume_wanted {
            self.particle_pool.volumetric_casters(input.particles, slot)
        } else {
            Vec::new()
        };
        // Sin emisores de humo/niebla el compute 32³ no aporta sombra y se omite.
        let volume_active = volume_wanted && !casters.is_empty();
        volume.update(
            slot,
            input,
            &device,
            cmd,
            &casters,
            self.particle_pool.volumetric_splat_pipeline(),
            self.particle_pool.volumetric_splat_layout(),
            volume_active,
        )?;
        self.trace_background(slot, input, &pipes, &device, cmd)?;
        if input.shadows && input.sun_above_horizon {
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
        // Partículas: billboards aditivos sobre el HDR del forward (con su depth ya en
        // READ_ONLY) y antes de reflejos/SSR/composite, así el humo/llama entra en el
        // color que consumen los reflejos. Orden documentado respecto a translúcidos:
        // `forward_blend` dibuja los objetos translúcidos del forward ANTES (mismo
        // render pass, depth-write off) y las partículas se mezclan ENCIMA; F3 (WBOIT)
        // añadirá su propio pase entre ambos para humo/niebla/agua sin ordenar en CPU.
        if !input.particles.is_empty() {
            self.particles_draw(slot, input, &device, cmd)?;
        }
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
    /// Invalida el historial temporal tras cargar una escena o cambiar de escena.
    pub fn reset_taa_history(&mut self) {
        self.taa_history_valid = false;
        self.volumetric.reset();
    }

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
    /// Enlaza la textura 3D de transmitancia en el set de escena de todos los slots, para
    /// que el forward proyecte la sombra del humo/niebla sobre superficies opacas.
    pub(super) fn rewrite_scene_volume_descriptors(
        &mut self,
        sampler: vk::Sampler,
        light_views: [vk::ImageView; FRAMES],
        ubos: [vk::Buffer; FRAMES],
    ) -> Result<(), VulkanError> {
        for slot in 0..FRAMES {
            write_combined(
                self.gpu.device(),
                self.sets.scene[slot],
                2,
                light_views[slot],
                sampler,
                vk::ImageLayout::GENERAL,
            )?;
            let info = vk::DescriptorBufferInfo {
                buffer: ubos[slot],
                offset: 0,
                range: vk::WHOLE_SIZE,
            };
            let write = vk::WriteDescriptorSet::default()
                .dst_set(self.sets.scene[slot])
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(std::slice::from_ref(&info));
            unsafe { self.gpu.device().update_descriptor_sets(&[write], &[]) };
        }
        Ok(())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssaa_extent_scales_and_clamps_uniformly() {
        let viewport = vk::Extent2D {
            width: 800,
            height: 600,
        };
        assert_eq!(
            supersampled_extent(viewport, 1.0, 4096),
            viewport,
            "1× no cambia el extent"
        );
        assert_eq!(
            supersampled_extent(viewport, 2.0, 4096),
            vk::Extent2D {
                width: 1600,
                height: 1200
            }
        );
        // El límite se aplica a la vez en ambos ejes para no distorsionar el aspecto.
        let capped = supersampled_extent(
            vk::Extent2D {
                width: 1000,
                height: 500,
            },
            4.0,
            2000,
        );
        assert_eq!(capped.width, 2000);
        assert_eq!(capped.height, 1000);
    }
}
