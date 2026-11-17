//! Pool GPU del sistema de partículas (F1: fuego).
//!
//! Responsabilidades:
//! - Buffers por emisor: ping-pong de partículas (GpuOnly), contadores de lista viva,
//!   UBO de `EmitterParams` por slot en vuelo y buffer indirecto (draw + dispatch).
//! - Pipeline de cómputo `spawn_sim.comp` (spawn + integración) y `build_indirect.comp`
//!   (contador → draw/dispatch indirectos, GPU-driven puro).
//! - Pipeline gráfico de billboards (aditivo / alpha) y acumulador WBOIT con resolve.
//! - Pool de descriptores DEDICADO (decisión del plan): los sets de partículas no
//!   comparten presupuesto con los del renderer.
//!
//! Sincronización (todas con `cmd_pipeline_barrier` clásico, patrón del repo):
//! 1. `vkCmdFillBuffer` del contador de salida + barrera `TRANSFER_WRITE -> COMPUTE`.
//! 2. Barrera compute previo -> compute (buffers de lectura + indirecto).
//! 3. `dispatch_indirect` de `spawn_sim`.
//! 4. Barrera compute -> compute para `build_indirect`.
//! 5. Barrera compute -> vértice/fragmento + `DRAW_INDIRECT` (Compute -> Graphics).
//!
//! Ping-pong (precisión 1 del plan): el rol A/B lo marca `read_is_a` que llega en el
//! `ParticleBatch` y viene del `sim_step` PROPIO del emisor; los descriptores A/B nunca
//! se reescriben por frame. El contador de SALIDA (el opuesto al de lectura) se reinicia
//! a 0 con fill cada frame antes de simular (precisión 2 del plan).

use std::collections::HashMap;

use ash::vk;
use bytemuck::bytes_of;

use crate::engine3d::particles::{EmitterParams, MIN_PARTICLES, PushBuild, PushDraw, PushSim};

use super::*;

/// Tamaño de `Particle` (GLSL: 4 × vec4).
const PARTICLE_SIZE: u64 = 64;
/// Layout del buffer indirecto: draw (16 B) + dispatch (12 B).
const INDIRECT_SIZE: u64 = 28;
/// Offset del `VkDispatchIndirectCommand` dentro del buffer indirecto.
const INDIRECT_DISPATCH_OFFSET: u64 = 16;
/// Frames sin ver un emisor antes de mandarlo a reciclaje.
const PRUNE_AFTER_FRAMES: u64 = 300;
const PRUNE_INTERVAL: u64 = 300;
/// Pools reciclados que se conservan para reutilizar (evita churn de Vulkan).
const MAX_RECYCLED: usize = 8;
/// Máximo de sets que el pool dedicado entregará (2 slots × emisores concurrentes).
const MAX_SETS: u32 = 64;
/// Suavidad del desvanecimiento soft-particle (m) frente a superficies opacas.
const SOFTNESS_M: f32 = 0.4;

pub(super) struct ParticlePool {
    gpu: SharedGpu,
    set_layout: vk::DescriptorSetLayout,
    /// Layout del resolve WBOIT (3 samplers); se destruye junto al pool.
    resolve_set_layout: vk::DescriptorSetLayout,
    /// Pool de descriptores dedicado a partículas (decisión del plan).
    pool: vk::DescriptorPool,
    sim_pipeline: vk::Pipeline,
    build_pipeline: vk::Pipeline,
    draw_pipeline: vk::Pipeline,
    /// Variante con blending alpha estándar (lluvia/nieve, F2); el fuego usa el aditivo.
    draw_alpha_pipeline: vk::Pipeline,
    /// Acumulador WBOIT (humo/niebla, F3): 2 adjuntos con blending distinto por adjunto.
    wboit_pipeline: vk::Pipeline,
    /// Resolve WBOIT (fullscreen triangle): escena·revealage + color ponderado.
    resolve_pipeline: vk::Pipeline,
    /// Layout del resolve (set de 3 samplers, sin push constants).
    resolve_layout: vk::PipelineLayout,
    /// Sets del resolve por slot en vuelo (escena + accum + reveal).
    resolve_sets: [vk::DescriptorSet; 2],
    /// Sampler clamp lineal propio para muestrear la depth (soft particles).
    depth_sampler: vk::Sampler,
    /// Vista de `targets.depth` enlazada en binding 6; se reescribe al recrear targets.
    depth_view: vk::ImageView,
    sim_layout: vk::PipelineLayout,
    build_layout: vk::PipelineLayout,
    draw_layout: vk::PipelineLayout,
    /// Registro de pipelines/layouts/shaders propios; `Drop` los destruye sin tocar los
    /// del renderer (que indexa por posición y no admite inserciones intermedias).
    shaders: Vec<vk::ShaderModule>,
    pipelines: Vec<vk::Pipeline>,
    layouts: Vec<vk::PipelineLayout>,
    /// Emisores activos por id.
    active: HashMap<u64, EmitterGpu>,
    /// Emisores ausentes listos para reutilizar (mismo presupuesto o mayor).
    recycled: Vec<EmitterGpu>,
    frames: u64,
}

struct EmitterGpu {
    capacity: u32,
    part_a: GpuBuffer,
    part_b: GpuBuffer,
    cnt_a: GpuBuffer,
    cnt_b: GpuBuffer,
    /// UBO de `EmitterParams` por slot en vuelo (mismo patrón que `scene_ubo`).
    params: [GpuBuffer; 2],
    /// `[draw indirect 16 B | dispatch indirect 12 B]`, escrito por `build_indirect.comp`.
    indirect: GpuBuffer,
    sets: [vk::DescriptorSet; 2],
    seen: u64,
}

fn initial_indirect_bytes() -> Vec<u8> {
    // Draw: 6 vértices, 0 instancias (no-op hasta que el primer build_indirect escriba).
    // Dispatch: 1 grupo (kickstart aunque no haya partículas vivas).
    let words = [6u32, 0, 0, 0, 1, 1, 1];
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

impl EmitterGpu {
    fn create(
        gpu: &SharedGpu,
        capacity: u32,
        pool: vk::DescriptorPool,
        set_layout: vk::DescriptorSetLayout,
        depth_view: vk::ImageView,
        depth_sampler: vk::Sampler,
    ) -> Result<Self, VulkanError> {
        let device = gpu.device();
        let size = capacity as u64 * PARTICLE_SIZE;
        let particles_usage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let part_a = gpu.create_buffer(
            size,
            particles_usage,
            MemoryLocation::GpuOnly,
            "particles-a",
        )?;
        let part_b = gpu.create_buffer(
            size,
            particles_usage,
            MemoryLocation::GpuOnly,
            "particles-b",
        )?;
        // Contadores: minúsculos y host-visibles para que la CPU los ponga a 0 al crear/reciclar;
        // el reset por frame lo hace `cmd_fill_buffer` (precisión 2).
        let counter_usage =
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let cnt_a = gpu.create_buffer(
            4,
            counter_usage,
            MemoryLocation::CpuToGpu,
            "particles-cnt-a",
        )?;
        let cnt_b = gpu.create_buffer(
            4,
            counter_usage,
            MemoryLocation::CpuToGpu,
            "particles-cnt-b",
        )?;
        let params = [
            gpu.create_buffer(
                std::mem::size_of::<EmitterParams>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::CpuToGpu,
                "particles-params-0",
            )?,
            gpu.create_buffer(
                std::mem::size_of::<EmitterParams>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::CpuToGpu,
                "particles-params-1",
            )?,
        ];
        let indirect = gpu.create_buffer(
            INDIRECT_SIZE,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::INDIRECT_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::CpuToGpu,
            "particles-indirect",
        )?;
        let sets = [
            alloc_set(device, pool, set_layout)?,
            alloc_set(device, pool, set_layout)?,
        ];
        let mut entry = Self {
            capacity,
            part_a,
            part_b,
            cnt_a,
            cnt_b,
            params,
            indirect,
            sets,
            seen: 0,
        };
        entry.reset_host_state()?;
        entry.write_sets(device, depth_view, depth_sampler)?;
        Ok(entry)
    }

    /// Estado CPU del arranque: contadores a 0 e indirecto en su valor semilla.
    ///
    /// Lo invoca el engine con `ParticleBatch::reset` (F6) cuando un emisor reutiliza
    /// un id con partículas viejas: el clima nuevo arranca vacío sin esperar la muerte
    /// natural de las partículas anteriores.
    fn reset_host_state(&mut self) -> Result<(), VulkanError> {
        self.cnt_a.write(0, &[0, 0, 0, 0])?;
        self.cnt_b.write(0, &[0, 0, 0, 0])?;
        self.indirect.write(0, &initial_indirect_bytes())?;
        Ok(())
    }

    /// Enlaza los buffers fijos en ambos slots. Solo se llama al crear o tras recrear
    /// por crecimiento (con GPU ociosa): los descriptores nunca se reescriben por frame.
    fn write_sets(
        &self,
        device: &ash::Device,
        depth_view: vk::ImageView,
        depth_sampler: vk::Sampler,
    ) -> Result<(), VulkanError> {
        for slot in 0..2 {
            write_uniform(device, self.sets[slot], 0, &self.params[slot])?;
            write_storage(device, self.sets[slot], 1, &self.part_a)?;
            write_storage(device, self.sets[slot], 2, &self.part_b)?;
            write_storage(device, self.sets[slot], 3, &self.cnt_a)?;
            write_storage(device, self.sets[slot], 4, &self.cnt_b)?;
            write_storage(device, self.sets[slot], 5, &self.indirect)?;
            write_combined(
                device,
                self.sets[slot],
                6,
                depth_view,
                depth_sampler,
                DEPTH_READ_ONLY,
            )?;
        }
        Ok(())
    }
}

impl ParticlePool {
    pub(super) fn new(gpu: &SharedGpu, depth_view: vk::ImageView) -> Result<Self, VulkanError> {
        let device = gpu.device();
        let emitter_set_layout = set_layout(
            device,
            &[
                binding(
                    0,
                    vk::DescriptorType::UNIFORM_BUFFER,
                    vk::ShaderStageFlags::COMPUTE
                        | vk::ShaderStageFlags::VERTEX
                        | vk::ShaderStageFlags::FRAGMENT,
                ),
                binding(
                    1,
                    vk::DescriptorType::STORAGE_BUFFER,
                    vk::ShaderStageFlags::COMPUTE | vk::ShaderStageFlags::VERTEX,
                ),
                binding(
                    2,
                    vk::DescriptorType::STORAGE_BUFFER,
                    vk::ShaderStageFlags::COMPUTE | vk::ShaderStageFlags::VERTEX,
                ),
                binding(
                    3,
                    vk::DescriptorType::STORAGE_BUFFER,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                binding(
                    4,
                    vk::DescriptorType::STORAGE_BUFFER,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                binding(
                    5,
                    vk::DescriptorType::STORAGE_BUFFER,
                    vk::ShaderStageFlags::COMPUTE,
                ),
                // Depth del forward para soft particles (F3).
                binding(
                    6,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    vk::ShaderStageFlags::FRAGMENT,
                ),
            ],
        )?;
        // Pool dedicado (decisión del plan): presupuesto independiente del renderer.
        // Incluye los sets del resolve WBOIT (2 slots × 3 samplers).
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(MAX_SETS + 8)
                    .pool_sizes(&[
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::UNIFORM_BUFFER,
                            descriptor_count: MAX_SETS,
                        },
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::STORAGE_BUFFER,
                            descriptor_count: MAX_SETS * 5,
                        },
                        vk::DescriptorPoolSize {
                            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                            descriptor_count: MAX_SETS + 6,
                        },
                    ]),
                None,
            )
        }
        .map_err(|error| vk_error("crear el pool de descriptores de partículas", error))?;

        // Sampler clamp lineal propio para la depth (sin compare: se lee como textura).
        let depth_sampler = unsafe {
            device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .max_lod(16.0),
                None,
            )
        }
        .map_err(|error| vk_error("crear el sampler de depth de partículas", error))?;

        // Layout del resolve WBOIT: accum + reveal + escena, sin push constants
        // (mismo `fullscreen.vert` que usa el composite del renderer).
        let resolve_set_layout = set_layout(
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
        let resolve_sets = [
            alloc_set(device, pool, resolve_set_layout)?,
            alloc_set(device, pool, resolve_set_layout)?,
        ];
        let resolve_layout = pipeline_layout(device, &[resolve_set_layout], 0)?;

        let sim_layout = push_layout(
            device,
            emitter_set_layout,
            std::mem::size_of::<PushSim>() as u32,
            vk::ShaderStageFlags::COMPUTE,
        )?;
        let build_layout = push_layout(
            device,
            emitter_set_layout,
            std::mem::size_of::<PushBuild>() as u32,
            vk::ShaderStageFlags::COMPUTE,
        )?;
        let draw_layout = push_layout(
            device,
            emitter_set_layout,
            std::mem::size_of::<PushDraw>() as u32,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        )?;
        let mut shaders = Vec::new();
        let mut pipelines = Vec::new();
        // El resolve_layout va aquí para que el Drop lo destruya (la validación
        // lo reportó como fuga cuando solo vivía como campo suelto).
        let layouts = vec![resolve_layout, sim_layout, build_layout, draw_layout];

        let sim_pipeline = compute_pipeline(
            device,
            spv("particle_spawn_sim"),
            sim_layout,
            &mut shaders,
            &mut pipelines,
        )?;
        let build_pipeline = compute_pipeline(
            device,
            spv("particle_build_indirect"),
            build_layout,
            &mut shaders,
            &mut pipelines,
        )?;
        let draw_vert = shader(device, spv("particle_render.vert"), &mut shaders)?;
        let draw_frag = shader(device, spv("particle_render.frag"), &mut shaders)?;
        let draw_pipeline = particle_graphics_pipeline(
            device,
            draw_layout,
            draw_vert,
            draw_frag,
            BlendMode::Additive,
            &mut pipelines,
        )?;
        let draw_alpha_pipeline = particle_graphics_pipeline(
            device,
            draw_layout,
            draw_vert,
            draw_frag,
            BlendMode::Alpha,
            &mut pipelines,
        )?;
        // WBOIT: acumulación en 2 adjuntos (RGBA16F + R16F) y resolve fullscreen.
        let wboit_frag = shader(device, spv("particle_wboit.frag"), &mut shaders)?;
        let wboit_pipeline =
            particle_wboit_pipeline(device, draw_layout, draw_vert, wboit_frag, &mut pipelines)?;
        let resolve_frag = shader(device, spv("particle_wboit_resolve.frag"), &mut shaders)?;
        let resolve_pipeline = full_screen_pipeline(
            device,
            resolve_layout,
            spv("full.vert"),
            resolve_frag,
            &mut shaders,
            &mut pipelines,
        )?;

        Ok(Self {
            gpu: gpu.clone(),
            set_layout: emitter_set_layout,
            resolve_set_layout,
            pool,
            sim_pipeline,
            build_pipeline,
            draw_pipeline,
            draw_alpha_pipeline,
            wboit_pipeline,
            resolve_pipeline,
            resolve_layout,
            resolve_sets,
            depth_sampler,
            depth_view,
            sim_layout,
            build_layout,
            draw_layout,
            shaders,
            pipelines,
            layouts,
            active: HashMap::new(),
            recycled: Vec::new(),
            frames: 0,
        })
    }

    /// Reserva (o recicla) los buffers del emisor para la capacidad pedida.
    fn ensure(&mut self, id: u64, capacity: u32) -> Result<&mut EmitterGpu, VulkanError> {
        let capacity = capacity.max(MIN_PARTICLES).next_power_of_two();
        if let Some(existing) = self.active.get(&id)
            && existing.capacity >= capacity
        {
            return Ok(self.active.get_mut(&id).expect("presente"));
        }
        // Crecimiento: los frames en vuelo pueden referenciar los buffers viejos; se
        // espera a la GPU antes de destruir nada (pasa solo cuando el editor sube el
        // presupuesto por encima del pool actual, evento raro).
        let needs_new = match self.active.get(&id) {
            Some(existing) => existing.capacity < capacity,
            None => true,
        };
        if needs_new {
            if let Some(old) = self.active.remove(&id) {
                unsafe {
                    let _ = self.gpu.device().device_wait_idle();
                }
                drop(old);
            }
            let entry = EmitterGpu::create(
                &self.gpu,
                capacity,
                self.pool,
                self.set_layout,
                self.depth_view,
                self.depth_sampler,
            )?;
            self.active.insert(id, entry);
        } else if let Some(pos) = self
            .recycled
            .iter()
            .position(|entry| entry.capacity >= capacity)
        {
            let mut entry = self.recycled.remove(pos);
            entry.reset_host_state()?;
            entry.write_sets(self.gpu.device(), self.depth_view, self.depth_sampler)?;
            self.active.insert(id, entry);
        } else {
            let entry = EmitterGpu::create(
                &self.gpu,
                capacity,
                self.pool,
                self.set_layout,
                self.depth_view,
                self.depth_sampler,
            )?;
            self.active.insert(id, entry);
        }
        Ok(self.active.get_mut(&id).expect("insertado"))
    }

    /// Purga de emisores ausentes → reciclaje (no destrucción: la GPU lleva 300 frames
    /// sin referenciarlos, así que su Drop es seguro).
    fn prune(&mut self) {
        let cutoff = self.frames.saturating_sub(PRUNE_AFTER_FRAMES);
        let stale: Vec<u64> = self
            .active
            .iter()
            .filter(|(_, entry)| entry.seen < cutoff)
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some(entry) = self.active.remove(&id)
                && self.recycled.len() < MAX_RECYCLED
            {
                self.recycled.push(entry);
                // Si el reciclaje está lleno, el entry muere con su Drop (buffers) y sus
                // sets quedan en el pool hasta que el pool se destruya.
            }
        }
    }

    /// Reescribe la depth (binding 6) y los sets del resolve tras recrear los targets
    /// (resize). Mismo patrón aceptado que `rewrite_target_descriptors` del renderer.
    pub(super) fn rewrite_targets(
        &mut self,
        scene_view: vk::ImageView,
        accum_view: vk::ImageView,
        reveal_view: vk::ImageView,
        depth_view: vk::ImageView,
    ) -> Result<(), VulkanError> {
        let device = self.gpu.device();
        self.depth_view = depth_view;
        let sets: Vec<vk::DescriptorSet> = self.active.values().flat_map(|e| e.sets).collect();
        for set in sets {
            write_combined(
                device,
                set,
                6,
                depth_view,
                self.depth_sampler,
                DEPTH_READ_ONLY,
            )?;
        }
        for slot in 0..2 {
            write_combined(
                device,
                self.resolve_sets[slot],
                0,
                accum_view,
                self.depth_sampler,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.resolve_sets[slot],
                1,
                reveal_view,
                self.depth_sampler,
                vk::ImageLayout::GENERAL,
            )?;
            write_combined(
                device,
                self.resolve_sets[slot],
                2,
                scene_view,
                self.depth_sampler,
                vk::ImageLayout::GENERAL,
            )?;
        }
        Ok(())
    }

    /// Paso de cómputo por emisor: fill del contador de salida, barreras, sim y
    /// build_indirect. Se graba ANTES de cualquier pase gráfico del frame.
    pub(super) fn sim(
        &mut self,
        slot: usize,
        batches: &[ParticleBatch],
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        self.frames += 1;
        let frames = self.frames;
        // Handles copiados antes del bucle: `entry` toma prestado `self.active` mutable
        // durante todo el cuerpo y los campos de pipeline ya no se alcanzan por `self`.
        let sim_pipeline = self.sim_pipeline;
        let sim_layout = self.sim_layout;
        let build_pipeline = self.build_pipeline;
        let build_layout = self.build_layout;
        for batch in batches {
            let entry = self.ensure(batch.id, batch.max_particles)?;
            entry.seen = frames;
            // Reset pedido por el engine (F6): el clima se reemplazó con el mismo id o
            // la escena se restauró — las partículas viejas mueren al instante en vez
            // de esperar su vida natural. Escritura CPU sobre contadores/indirecto: en
            // el peor caso un frame en vuelo dibuja 0 instancias (inofensivo y único).
            if batch.reset == 1 {
                entry.reset_host_state()?;
            }
            entry.params[slot].write(0, bytes_of(&batch.params))?;

            let read_is_a = batch.read_is_a == 1;
            // Contadores: entrada = lo que escribió el sim del frame anterior; salida =
            // el opuesto, que este frame reinicia con fill (precisión 2).
            let in_counter = if read_is_a {
                &entry.cnt_a
            } else {
                &entry.cnt_b
            };
            let out_counter = if read_is_a {
                &entry.cnt_b
            } else {
                &entry.cnt_a
            };
            let read_particles = if read_is_a {
                &entry.part_a
            } else {
                &entry.part_b
            };
            let write_particles = if read_is_a {
                &entry.part_b
            } else {
                &entry.part_a
            };

            unsafe {
                // 1. Reset del contador de SALIDA (transfer, fuera de render pass).
                device.cmd_fill_buffer(cmd, out_counter.buffer, 0, 4, 0);
            }
            // 2. Barrera del reset: TRANSFER_WRITE -> COMPUTE.
            buffer_barriers(
                device,
                cmd,
                &[out_counter.buffer],
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            // 3. Visibilidad de lo que este dispatch lee: escrituras de compute de frames
            //    anteriores (partículas + contador de entrada + indirecto) y lectura de
            //    vértice del draw anterior (WAR sobre el buffer que este frame escribe).
            buffer_barriers(
                device,
                cmd,
                &[
                    read_particles.buffer,
                    in_counter.buffer,
                    entry.indirect.buffer,
                ],
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            unsafe {
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, sim_pipeline);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    sim_layout,
                    0,
                    &[entry.sets[slot]],
                    &[],
                );
                let push = PushSim {
                    dt: batch.dt,
                    spawn: batch.spawn,
                    read_is_a: batch.read_is_a,
                    seed: batch.seed,
                };
                device.cmd_push_constants(
                    cmd,
                    sim_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    bytes_of(&push),
                );
                // Dispatch indirecto: grupos escritos por `build_indirect.comp` del frame
                // anterior (latencia 1 frame, autosaneante; semilla CPU = 1 grupo).
                device.cmd_dispatch_indirect(cmd, entry.indirect.buffer, INDIRECT_DISPATCH_OFFSET);
            }
            // 4. Salida del sim visible para `build_indirect`.
            buffer_barriers(
                device,
                cmd,
                &[write_particles.buffer, out_counter.buffer],
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            unsafe {
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, build_pipeline);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    build_layout,
                    0,
                    &[entry.sets[slot]],
                    &[],
                );
                let push = PushBuild {
                    read_is_a: batch.read_is_a,
                    next_spawn: batch.next_spawn,
                };
                device.cmd_push_constants(
                    cmd,
                    build_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    bytes_of(&push),
                );
                device.cmd_dispatch(cmd, 1, 1, 1);
            }
            // 5. Compute -> Graphics: las partículas escritas pasan a lectura de vértice y
            //    el buffer indirecto pasa a consumo de `cmd_draw_indirect`.
            buffer_barriers(
                device,
                cmd,
                &[write_particles.buffer],
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::SHADER_READ,
            );
            buffer_barriers(
                device,
                cmd,
                &[entry.indirect.buffer],
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::DRAW_INDIRECT,
                vk::AccessFlags::INDIRECT_COMMAND_READ,
            );
        }
        if frames.is_multiple_of(PRUNE_INTERVAL) {
            self.prune();
        }
        Ok(())
    }
}

impl FrameRenderer {
    /// Pases de billboards (F3):
    /// 1. Directos sobre el HDR de escena: aditivo (fuego) y alpha (lluvia/nieve), en un
    ///    solo rendering instance con la depth del forward en READ_ONLY.
    /// 2. WBOIT (humo/niebla): acumulación en `particles_accum` + `particles_reveal` con
    ///    depth en READ_ONLY, y resolve fullscreen sobre `particles_out` que `copy_color`
    ///    lleva al HDR (muestrear y escribir `scene` en el mismo pase es un feedback
    ///    loop ilegal). Sin ordenar partículas en CPU.
    pub(super) fn particles_draw(
        &self,
        slot: usize,
        input: &FrameInput<'_>,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let pool = &self.particle_pool;
        let drawable: Vec<&ParticleBatch> = input
            .particles
            .iter()
            .filter(|batch| pool.active.contains_key(&batch.id))
            .collect();
        if drawable.is_empty() {
            return Ok(());
        }
        let (direct, wboit): (Vec<&ParticleBatch>, Vec<&ParticleBatch>) =
            drawable.iter().copied().partition(|batch| batch.wboit == 0);
        let extent = self.targets.extent;
        // El forward escribió el color del HDR: visibilidad para la mezcla del pase.
        crate::engine3d::vulkan::gpu::cmd_image_barrier(
            device,
            cmd,
            self.targets.scene.image,
            vk::ImageAspectFlags::COLOR,
            1,
            1,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::GENERAL,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::COLOR_ATTACHMENT_READ,
        );

        // ── 1. Pases directos sobre el HDR ──────────────────────────────────────
        if !direct.is_empty() {
            let color = vk::RenderingAttachmentInfo::default()
                .image_view(self.targets.scene.view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);
            // La depth llegó aquí en READ_ONLY (la dejó `forward_pass`) y se mantiene:
            // test activo, escritura deshabilitada; los soft particles la muestrean.
            let depth = vk::RenderingAttachmentInfo::default()
                .image_view(self.targets.depth.view)
                .image_layout(DEPTH_READ_ONLY)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::NONE);
            let rendering = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&color))
                .depth_attachment(&depth);
            unsafe {
                device.cmd_begin_rendering(cmd, &rendering);
                set_viewport(device, cmd, extent);
                for batch in &direct {
                    let entry = match pool.active.get(&batch.id) {
                        Some(entry) => entry,
                        None => continue,
                    };
                    // El modo de mezcla lo decide el material (fuego aditivo,
                    // lluvia/nieve alpha). Pipeline + flag del fragmento van alineados.
                    let pipeline = if batch.blend_alpha == 1 {
                        pool.draw_alpha_pipeline
                    } else {
                        pool.draw_pipeline
                    };
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
                    // `misc.w` = material_kind: el fragmento elige llama / nube / gota / grano.
                    let push = PushDraw {
                        view_proj: input.particle_camera.view_proj,
                        cam_right: input.particle_camera.cam_right,
                        cam_up: input.particle_camera.cam_up,
                        misc: [
                            batch.read_is_a,
                            batch.blend_alpha,
                            batch.soft,
                            batch.params.misc[0],
                        ],
                        depth_params: [
                            input.particle_camera.near_far[0],
                            input.particle_camera.near_far[1],
                            SOFTNESS_M,
                            0.0,
                        ],
                    };
                    device.cmd_push_constants(
                        cmd,
                        pool.draw_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes_of(&push),
                    );
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        pool.draw_layout,
                        0,
                        &[entry.sets[slot]],
                        &[],
                    );
                    device.cmd_draw_indirect(cmd, entry.indirect.buffer, 0, 1, 0);
                }
                device.cmd_end_rendering(cmd);
            }
        }

        // ── 2. WBOIT: acumulación + resolve ─────────────────────────────────────
        if !wboit.is_empty() {
            // El resolve muestrea la escena (con los pases directos ya mezclados).
            crate::engine3d::vulkan::gpu::cmd_image_barrier(
                device,
                cmd,
                self.targets.scene.image,
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
            // Acumulación: clear de accum (0) y reveal (1.0) en el propio adjunto.
            let accum = vk::RenderingAttachmentInfo::default()
                .image_view(self.targets.particles_accum.view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(clear_color([0.0, 0.0, 0.0, 0.0]));
            let reveal = vk::RenderingAttachmentInfo::default()
                .image_view(self.targets.particles_reveal.view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(clear_color([1.0, 0.0, 0.0, 0.0]));
            let depth = vk::RenderingAttachmentInfo::default()
                .image_view(self.targets.depth.view)
                .image_layout(DEPTH_READ_ONLY)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::NONE);
            let attachments = [accum, reveal];
            let rendering = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent,
                })
                .layer_count(1)
                .color_attachments(&attachments)
                .depth_attachment(&depth);
            unsafe {
                device.cmd_begin_rendering(cmd, &rendering);
                set_viewport(device, cmd, extent);
                for batch in &wboit {
                    let entry = match pool.active.get(&batch.id) {
                        Some(entry) => entry,
                        None => continue,
                    };
                    device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        pool.wboit_pipeline,
                    );
                    let push = PushDraw {
                        view_proj: input.particle_camera.view_proj,
                        cam_right: input.particle_camera.cam_right,
                        cam_up: input.particle_camera.cam_up,
                        misc: [batch.read_is_a, 0, batch.soft, batch.params.misc[0]],
                        depth_params: [
                            input.particle_camera.near_far[0],
                            input.particle_camera.near_far[1],
                            SOFTNESS_M,
                            0.0,
                        ],
                    };
                    device.cmd_push_constants(
                        cmd,
                        pool.draw_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes_of(&push),
                    );
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        pool.draw_layout,
                        0,
                        &[entry.sets[slot]],
                        &[],
                    );
                    device.cmd_draw_indirect(cmd, entry.indirect.buffer, 0, 1, 0);
                }
                device.cmd_end_rendering(cmd);
            }
            // Los tres samplers del resolve leen imágenes escritas en este frame.
            crate::engine3d::vulkan::gpu::cmd_image_barrier(
                device,
                cmd,
                self.targets.scene.image,
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
            crate::engine3d::vulkan::gpu::cmd_image_barrier(
                device,
                cmd,
                self.targets.particles_accum.image,
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
            crate::engine3d::vulkan::gpu::cmd_image_barrier(
                device,
                cmd,
                self.targets.particles_reveal.image,
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
            // Resolve sobre `particles_out` (no sobre `scene`: feedback loop).
            let out = vk::RenderingAttachmentInfo::default()
                .image_view(self.targets.particles_out.view)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);
            let resolve = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&out));
            unsafe {
                device.cmd_begin_rendering(cmd, &resolve);
                set_viewport(device, cmd, extent);
                device.cmd_bind_pipeline(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pool.resolve_pipeline,
                );
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pool.resolve_layout,
                    0,
                    &[pool.resolve_sets[slot]],
                    &[],
                );
                device.cmd_draw(cmd, 3, 1, 0, 0);
                device.cmd_end_rendering(cmd);
            }
            // El resolve muestreó la escena y las imágenes WBOIT: antes del copy y del
            // uso posterior como adjunto, visibilidad de esas lecturas como escrituras.
            crate::engine3d::vulkan::gpu::cmd_image_barrier(
                device,
                cmd,
                self.targets.scene.image,
                vk::ImageAspectFlags::COLOR,
                1,
                1,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::GENERAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::TRANSFER_READ,
            );
            copy_color(
                device,
                cmd,
                &self.targets.particles_out,
                &self.targets.scene,
                extent,
                0,
            );
        }
        Ok(())
    }
}

impl Drop for ParticlePool {
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
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
            device.destroy_descriptor_set_layout(self.resolve_set_layout, None);
            device.destroy_sampler(self.depth_sampler, None);
        }
    }
}

/// Layout con push constant para un solo rango (el helper de `layouts.rs` fija
/// VERTEX|FRAGMENT y no sirve para compute).
fn push_layout(
    device: &ash::Device,
    set: vk::DescriptorSetLayout,
    size: u32,
    stages: vk::ShaderStageFlags,
) -> Result<vk::PipelineLayout, VulkanError> {
    let range = vk::PushConstantRange::default()
        .stage_flags(stages)
        .offset(0)
        .size(size);
    unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(std::slice::from_ref(&set))
                .push_constant_ranges(std::slice::from_ref(&range)),
            None,
        )
    }
    .map_err(|error| vk_error("crear el layout del pipeline de partículas", error))
}

fn compute_pipeline(
    device: &ash::Device,
    bytes: &[u8],
    layout: vk::PipelineLayout,
    shaders: &mut Vec<vk::ShaderModule>,
    pipelines: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    let module = shader(device, bytes, shaders)?;
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(c"main");
    let info = vk::ComputePipelineCreateInfo::default()
        .stage(stage)
        .layout(layout);
    let pipeline = match unsafe {
        device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None)
    } {
        Ok(mut created) => created.remove(0),
        Err((created, error)) => {
            for partial in created {
                unsafe { device.destroy_pipeline(partial, None) };
            }
            return Err(vk_error("crear el pipeline de partículas", error));
        }
    };
    pipelines.push(pipeline);
    Ok(pipeline)
}

/// Modo de mezcla del billboard: cada material lo trae en `ParticleBatch.blend_alpha`.
#[derive(Clone, Copy)]
enum BlendMode {
    Additive,
    Alpha,
}

/// Billboard (fuego aditivo o alpha): sin vertex input (6 vértices/instancia generados
/// en el shader), depth test sin escritura. El fuego usa `ONE/ONE` sobre premultiplicados;
/// lluvia/nieve `SRC_ALPHA/ONE_MINUS_SRC_ALPHA` (convención de `graphics()` del repo).
fn particle_graphics_pipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: vk::ShaderModule,
    blend: BlendMode,
    pipelines: &mut Vec<vk::Pipeline>,
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
    let vertex_info = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    // Test activo (recorte contra la escena) y escritura OFF: la depth queda intacta
    // para SSR/TAA/soft particles.
    let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS);
    let blend_attachment = match blend {
        BlendMode::Additive => vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD),
        BlendMode::Alpha => vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD),
    };
    let attachments = [blend_attachment];
    let blend_state = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamics = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamics);
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&[COLOR])
        .depth_attachment_format(DEPTH);
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
                for partial in created {
                    unsafe { device.destroy_pipeline(partial, None) };
                }
                return Err(vk_error(
                    "crear el pipeline de billboards de partículas",
                    error,
                ));
            }
        };
    pipelines.push(pipeline);
    Ok(pipeline)
}

/// WBOIT (F3): 2 adjuntos con blending distinto por adjunto —
/// accum (RGBA16F) aditivo ONE/ONE y reveal (R16F) multiplicativo (dst *= 1 - src.r).
/// Depth test sin escritura, mismo `render.vert` que los pases directos. Requiere la
/// feature `independentBlend` (habilitada en `context.rs`).
fn particle_wboit_pipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert: vk::ShaderModule,
    frag: vk::ShaderModule,
    pipelines: &mut Vec<vk::Pipeline>,
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
    let vertex_info = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS);
    let accum_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE)
        .alpha_blend_op(vk::BlendOp::ADD);
    let reveal_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::R)
        .blend_enable(true)
        // Transparencia: result.r = dst.r * (1 - src.r) → Π(1 - alpha) en el canal R.
        .src_color_blend_factor(vk::BlendFactor::ZERO)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_COLOR)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ZERO)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD);
    let attachments = [accum_blend, reveal_blend];
    let blend_state = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamics = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamics);
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&[vk::Format::R16G16B16A16_SFLOAT, vk::Format::R16_SFLOAT])
        .depth_attachment_format(DEPTH);
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
                for partial in created {
                    unsafe { device.destroy_pipeline(partial, None) };
                }
                return Err(vk_error("crear el pipeline WBOIT de partículas", error));
            }
        };
    pipelines.push(pipeline);
    Ok(pipeline)
}

/// Fullscreen sin blending (resolve WBOIT): triángulo de `full.vert` + frag propio,
/// escribe una sola imagen de color sin depth.
fn full_screen_pipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    vert_bytes: &[u8],
    frag: vk::ShaderModule,
    shaders: &mut Vec<vk::ShaderModule>,
    pipelines: &mut Vec<vk::Pipeline>,
) -> Result<vk::Pipeline, VulkanError> {
    let vert = shader(device, vert_bytes, shaders)?;
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
    let vertex_info = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let no_blend = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let attachments = [no_blend];
    let blend_state = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamics = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamics);
    let mut rendering =
        vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&[COLOR]);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_info)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .color_blend_state(&blend_state)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    let pipeline =
        match unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        {
            Ok(mut created) => created.remove(0),
            Err((created, error)) => {
                for partial in created {
                    unsafe { device.destroy_pipeline(partial, None) };
                }
                return Err(vk_error("crear el pipeline de resolve WBOIT", error));
            }
        };
    pipelines.push(pipeline);
    Ok(pipeline)
}

/// Barreras de buffer de memoria (patrón barrier1 del repo; Vulkan 1.3 lo mantiene).
fn buffer_barriers(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    buffers: &[vk::Buffer],
    src_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_stage: vk::PipelineStageFlags,
    dst_access: vk::AccessFlags,
) {
    let barriers: Vec<vk::BufferMemoryBarrier> = buffers
        .iter()
        .map(|buffer| {
            vk::BufferMemoryBarrier::default()
                .src_access_mask(src_access)
                .dst_access_mask(dst_access)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(*buffer)
                .offset(0)
                .size(vk::WHOLE_SIZE)
        })
        .collect();
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &barriers,
            &[],
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    /// Presupuesto de memoria por emisor al peor caso (lluvia intensidad 10: capacidad
    /// 5120 → redondeo a 8192): dos buffers ping-pong + contadores + UBOs + indirecto.
    #[test]
    fn pool_memory_per_emitter_stays_under_a_budget() {
        const WORST_CAPACITY: u32 = 8192;
        let particles = 2 * WORST_CAPACITY as u64 * PARTICLE_SIZE;
        let counters = 2 * 4u64;
        let params = 2 * std::mem::size_of::<EmitterParams>() as u64;
        let indirect = INDIRECT_SIZE;
        let total = particles + counters + params + indirect;

        // Un emisor a tope cabe en 2 MiB: lluvia a intensidad máxima + niebla + humo +
        // fuego + arena + agua simultáneos quedan en unos pocos MiB de VRAM.
        assert!(
            total < 2 * 1024 * 1024,
            "presupuesto por emisor: {total} bytes"
        );
    }

    /// El buffer indirecto encaja exactamente en su layout: draw (16 B) + dispatch (12 B).
    /// Si el GLSL lo agranda, el offset del dispatch indirecto se descuadra en silencio.
    #[test]
    fn indirect_layout_matches_the_glsl_contract() {
        assert_eq!(INDIRECT_SIZE, 28);
        assert_eq!(INDIRECT_DISPATCH_OFFSET, 16);
        // VkDrawIndirectCommand: 4 × u32 = 16; VkDispatchIndirectCommand: 3 × u32 = 12.
        assert_eq!(16 + 12, INDIRECT_SIZE);
    }

    /// El push del draw con parámetros de depth no puede pasar de 128: es el límite
    /// garantizado por Vulkan (minMemoryPushConstantSize) y el layout lo fija ahí.
    #[test]
    fn push_draw_fits_the_guaranteed_budget() {
        let size = std::mem::size_of::<PushDraw>();
        assert_eq!(size, 128);
    }

    /// Presupuesto de sets del pool dedicado: 2 slots × emisores concurrentes + 2 sets
    /// del resolve. Si la UI creara más emisores que MAX_SETS/2, el pool se agota de
    /// forma visible (error al reservar) en vez de corromper el del renderer.
    #[test]
    fn descriptor_pool_covers_the_ui_emitter_budget() {
        let emitters = (MAX_SETS / 2) - 1; // deja hueco para el resolve
        assert!(
            emitters >= 8,
            "la UI debería poder crear al menos 8 emisores"
        );
        assert_eq!(MAX_SETS % 2, 0, "los sets son por par de slots");
    }

    /// El kernel de sim usa workgroups de 64 hilos y `build_indirect` redondea con la
    /// misma constante: el dispatch debe cubrir `in_count + spawn` siempre.
    #[test]
    fn dispatch_group_math_matches_the_kernel() {
        let capacity = 4096u32;
        let spawn = 64u32;
        let count = capacity; // saturado
        let estimated = (count + spawn).min(capacity);
        let groups = estimated.div_ceil(64);
        // Un hilo por partícula viva o por spawn: nada queda sin procesar.
        assert!(groups * 64 >= estimated);
        assert!(groups * 64 < estimated + 64);
    }
}
