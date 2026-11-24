//! Frame 3D del motor sobre Ash (Vulkan).
//!
//! `State::render` ahora graba el frame entero a través de
//! `crate::engine3d::vulkan::FrameRenderer` y presenta con el swapchain de Ash.
//! El motor 2D sigue en wgpu y no toca este archivo.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use glam::{Mat4, Vec3 as GlamVec3};
use winit::dpi::PhysicalSize;

use rer_engine_core::surface::{SurfacePresentError, SurfacePresentError::*};

use crate::engine3d::bvh::{RtMaterialGpu, RtSceneBlob, RtTriangle};
use crate::engine3d::config_3d::reflection_graphics::{ReflectionDebugView, ReflectionMode};
use crate::engine3d::ecs::{MeshComponent, SurfacePbr};
use crate::engine3d::vulkan::{FrameAcquire, FrameTarget, MeshBatch, RtBatch, SkinnedBatch};

use super::{SceneUniforms, State};

/// Peso del IBL de sondas. Antes se reutilizaba `shadow_darkness` para esto; ahora
/// `light_params.y` queda exclusivamente para la oscuridad de sombra.
const PROBE_IBL_WEIGHT: f32 = 0.22;

/// Entidad seleccionada para el frame y flag de visibilidad estricta en el forward.
/// Las que no son estrictas entran solo en el conjunto de casters del shadow map.
type StagedEntity = (
    (crate::engine3d::ecs::EntityId, usize, usize, Mat4, i32),
    bool,
);

/// Secuencia de Halton para el jitter subpíxel de TAA.
fn halton(index: u32, base: u32) -> f32 {
    let mut index = index;
    let mut f = 1.0;
    let mut result = 0.0;
    while index > 0 {
        f /= base as f32;
        result += f * (index % base) as f32;
        index /= base;
    }
    result
}

mod batches;
mod frustum;
mod rt;
mod skinned;
use self::batches::*;
use self::frustum::*;
use self::rt::*;
use self::skinned::*;
impl State {
    /// Reconfigura el swapchain de Ash al nuevo tamaño del viewport.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        if let Err(e) = self.vulkan.resize(new_size.width, new_size.height) {
            log::warn!("[resize] no se pudo reconfigurar el swapchain: {e}");
        }
    }

    /// Fija el tamaño lógico del viewport sin tocar el swapchain.
    ///
    /// Lo usa el editor: el swapchain es de la ventana completa y la escena se dibuja
    /// en un sub-rectángulo, así que `State` solo necesita saber su tamaño.
    pub fn set_viewport_size(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
        }
    }

    fn editor_selection_flag(
        &self,
        entity_id: crate::engine3d::ecs::EntityId,
        is_selected: bool,
        is_hovered: bool,
    ) -> f32 {
        if self.preview_playing {
            // Aura del tooltip (flag 8.0): tinte del color elegido + rim.
            return if self.interaction_highlight_target == Some(entity_id) {
                8.0
            } else {
                0.0
            };
        }
        if self.socket_bone_pick_entity == Some(entity_id) {
            return 0.0;
        }
        if self.bone_physics_pick_entity == Some(entity_id) {
            return 0.0;
        }
        if is_selected {
            1.0
        } else if is_hovered {
            2.0
        } else {
            0.0
        }
    }

    /// Graba y presenta un frame entero (host standalone).
    pub fn render(&mut self) -> Result<(), SurfacePresentError> {
        let frame = match self.vulkan.begin_frame() {
            Ok(frame) => frame,
            Err(FrameAcquire::Outdated) => return Err(Reconfigure),
            Err(FrameAcquire::Skip) => return Err(SkipFrame),
            Err(FrameAcquire::Fail(_e)) => return Err(Validation),
        };
        self.render_into(&frame)?;
        self.finish_present(&frame);
        self.vulkan.submit_and_present(&frame).map_err(|e| match e {
            FrameAcquire::Outdated | FrameAcquire::Skip => Reconfigure,
            FrameAcquire::Fail(_e) => Validation,
        })
    }

    /// Graba la escena en un `FrameTarget` abierto por otro host (el editor).
    ///
    /// Todos los datos de `FrameInput` se construyen como locales de este método, así que
    /// `frame` puede seguir prestado mientras `frame_renderer` se toma mutable.
    pub fn render_into(&mut self, frame: &FrameTarget) -> Result<(), SurfacePresentError> {
        self.update_animations();
        self.update_skinned_animations();
        self.sync_socket_attached_children();
        self.sync_player_ui_hud();

        let size = self.size;
        let aspect = size.width as f32 / size.height.max(1) as f32;
        // SSR y RTX son mutuamente excluyentes: el modo decide la cadena completa.
        let reflections = self.reflection_mode.is_active();
        let raytracing = self.reflection_mode == ReflectionMode::Rt;
        let ssr = self.reflection_mode == ReflectionMode::Ssr;

        // Gizmos siempre: si SSR/RT está apagado (default) la sonda no tiene malla
        // y sin wireframe el botón «Insertar sonda» parece no hacer nada.
        self.ensure_probe_slots_allocated();
        let probe_list = self.reflection_probe_render_list();
        let probe_fields = if reflections {
            self.reflection_probe_fields()
        } else {
            Vec::new()
        };
        let probes_active = reflections && self.reflection_probes_enabled && !probe_list.is_empty();

        // Uniformes de escena completos (cámara + luz + bias), como en el render wgpu.
        let view = self.camera_view_matrix();
        let eye = self.camera_world_position();
        let proj_stable = self.camera.proj_matrix(aspect);

        // Jitter Halton de TAA en NDC. La matriz estable se usa para velocity y para el
        // historial; solo `view_proj` (la que rasteriza) lleva el offset de subpíxel.
        let jitter_index = self.taa_frame;
        self.taa_frame = self.taa_frame.wrapping_add(1);
        let (jitter_x, jitter_y) = if self.taa_enabled && self.taa_jitter_scale > 0.0 {
            let scale = self.taa_jitter_scale.clamp(0.0, 2.0);
            let inv_w = 2.0 / size.width.max(1) as f32;
            let inv_h = 2.0 / size.height.max(1) as f32;
            (
                (halton(jitter_index + 1, 2) - 0.5) * inv_w * scale,
                (halton(jitter_index + 1, 3) - 0.5) * inv_h * scale,
            )
        } else {
            (0.0, 0.0)
        };
        let proj = Mat4::from_translation(GlamVec3::new(jitter_x, jitter_y, 0.0)) * proj_stable;
        let vp_stable = (proj_stable * view).to_cols_array_2d();
        let vp = proj * view;
        let view_proj = vp.to_cols_array_2d();
        let inv_view_proj = vp.inverse().to_cols_array_2d();
        let light_params = self.scene_light_params();
        let probe_weight = if probes_active { PROBE_IBL_WEIGHT } else { 0.0 };
        // Primera persona: el fragment shader recorta la geometría dentro de
        // este radio alrededor del ojo (w = 0 en editor => recorte desactivado).
        let camera_clip_radius = if self.uses_player_fps_viewport() {
            self.play_camera_clip_radius
        } else {
            0.0
        };
        let scene_uni = SceneUniforms {
            view_proj,
            view_proj_stable: vp_stable,
            prev_view_proj: self.prev_view_proj,
            inv_view_proj,
            cam_pos: [eye.x, eye.y, eye.z, camera_clip_radius],
            light_dir: self.scene_light_dir(),
            light_color: self.scene_light_color(),
            light_view_proj: self.build_light_view_proj(),
            light_params,
            jitter: [
                jitter_x,
                jitter_y,
                probe_weight,
                self.shadow_world_texel_m(),
            ],
            depth_plane: [
                self.camera.near,
                self.camera.far,
                if reflections { 1.0 } else { 0.0 },
                // La capa de Grass va en el uniforme: `tex_layer_pad.w` también es el IOR
                // y, si queda en 0, el shader lee el checker del editor.
                self.worldgen.material_base.unwrap_or(0) as f32,
            ],
            shadow_bias: self.scene_shadow_bias(),
            aura_color: {
                let [r, g, b] = self.interaction_highlight_color;
                [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 0.0]
            },
        };
        self.prev_view_proj = vp_stable;
        let scene_bytes = bytemuck::bytes_of(&scene_uni);

        // ── Partículas GPU-driven (F1: fuego) ────────────────────────────────────
        // Cada lote avanza el `sim_step` propio del emisor (precisión 1 del plan), así
        // que solo se construyen cuando este frame realmente va a simular. La base de
        // cámara sale de la matriz de vista (V es ortonormal: sus filas 0/1 son right/up
        // de cámara en mundo).
        let view_t = view.transpose();
        let particle_camera = crate::engine3d::vulkan::ParticleCamera {
            view_proj,
            cam_right: [view_t.x_axis.x, view_t.x_axis.y, view_t.x_axis.z, 0.0],
            cam_up: [view_t.y_axis.x, view_t.y_axis.y, view_t.y_axis.z, 0.0],
            near_far: [self.camera.near, self.camera.far],
        };
        self.sync_sun_particles();
        let particle_batches: Vec<crate::engine3d::vulkan::ParticleBatch> = self
            .particles
            .batches(self.delta_time, [eye.x, eye.y, eye.z])
            .into_iter()
            .map(|batch| crate::engine3d::vulkan::ParticleBatch {
                id: batch.id,
                max_particles: batch.max_particles,
                spawn: batch.spawn,
                reset: batch.reset,
                next_spawn: batch.next_spawn,
                dt: batch.dt,
                seed: batch.seed,
                read_is_a: batch.read_is_a,
                blend_alpha: batch.blend_alpha,
                wboit: batch.wboit,
                soft: batch.soft,
                params: batch.params,
            })
            .collect();

        // ── Datos del frame (locales) ────────────────────────────────────────────
        let frustum_vp = {
            let raw = self
                .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                .view_proj;
            Mat4::from_cols_array_2d(&raw)
        };
        // Un caster fuera de pantalla puede proyectar sombra dentro de la vista si
        // está aguas arriba del sol. Ampliamos su AABB por la dirección de luz y
        // conservamos esas entidades solo para el shadow map (ver `forward_ids`).
        let caster_margin = self.shadow_caster_margin_m();
        let light_dir = self.directional_light_dir;

        let staged: Vec<StagedEntity> = self
            .world
            .query2::<crate::engine3d::ecs::MeshComponent, crate::engine3d::ecs::Transform>()
            .filter_map(|(id, mc, t)| {
                if self.model_animation_bindings.contains_key(&id) {
                    return None;
                }
                let is_ground = self
                    .world
                    .get::<crate::engine3d::ecs::NameComponent>(id)
                    .is_some_and(|n| n.name.eq_ignore_ascii_case("ground"));
                let (mesh_center, mesh_half) = if is_ground {
                    (t.position, t.scale.abs() * 0.5)
                } else {
                    self.entity_world_pick_aabb(id, t)
                };
                if !is_ground
                    && !self
                        .world_bounds_3d
                        .intersects_world_aabb(mesh_center, mesh_half)
                {
                    return None;
                }
                let strict = is_ground || is_aabb_visible_3d(&frustum_vp, mesh_center, mesh_half);
                let caster = is_ground
                    || is_shadow_caster_visible(
                        &frustum_vp,
                        mesh_center,
                        mesh_half,
                        light_dir,
                        caster_margin,
                    );
                if !strict && !caster {
                    return None;
                }
                // LOD por distancia: solo entidades con variantes registradas
                // (`mesh_lod_variants`); el culling usa el AABB LOD0 sin cambios.
                let mut mesh_idx = mc.mesh_idx;
                if self.model_lod_enabled {
                    let level = crate::engine3d::config_3d::model_lod::select_lod_level(
                        mesh_center.distance(eye),
                        self.model_lod1_distance_m,
                        self.model_lod2_distance_m,
                    );
                    if level > 0
                        && let Some(lod_idx) = self.lod_mesh_idx_for(mc.mesh_idx, level)
                    {
                        mesh_idx = lod_idx;
                    }
                }
                Some(((id, mesh_idx, mc.tex_idx, t.to_matrix(), 0), strict))
            })
            .collect();

        let mut entities = Vec::with_capacity(staged.len());
        let mut forward_ids: HashSet<crate::engine3d::ecs::EntityId> =
            HashSet::with_capacity(staged.len());
        for (entry, strict) in staged {
            if strict {
                forward_ids.insert(entry.0);
            }
            entities.push(entry);
        }
        entities.sort_by_key(|a| a.4);

        // Los lotes se construyen una sola vez con el conjunto ampliado. La
        // partición deja el forward como prefijo del vec de casters.
        let batches = build_scene_instance_batches(self, &entities, &probe_fields);
        let partitioned = partition_scene_batches(batches, &forward_ids, |id| {
            entity_skips_shadow_cast(self, id)
        });

        // ── Probes: ranuras estables + catch-up / round-robin (una sonda por frame) ──
        let probe_captures: Vec<crate::engine3d::vulkan::ProbeCapture> = if probes_active {
            let ids: Vec<crate::engine3d::ecs::EntityId> =
                probe_list.iter().map(|(id, _, _)| *id).collect();
            self.sync_probe_capture_burst_for_entity_set(&ids);
            let burst = self.probe_capture_burst_all;
            self.probe_capture_burst_all = false;
            match crate::engine3d::probe_registry::next_probe_capture(
                burst,
                probe_list.len(),
                &mut self.probe_capture_cursor,
                &mut self.probe_capture_cooldown,
                &mut self.probe_capture_catchup,
            ) {
                Some(index) => {
                    let (_, position, slot) = probe_list[index];
                    vec![crate::engine3d::vulkan::ProbeCapture {
                        origin: position.to_array(),
                        slot,
                    }]
                }
                None => Vec::new(),
            }
        } else {
            self.probe_capture_catchup = 0;
            Vec::new()
        };

        // Geometría skinned trazable: se re-deforma cada pocos frames (ver
        // `SKINNED_TRACE_REFRESH_FRAMES`) porque rehacer el BVH en cada frame con un personaje
        // animado cuesta más de lo que un reflejo puede notar. El TLAS reutiliza esas mismas poses,
        // así que BVH y hardware ven exactamente la misma silueta.
        //
        // Va aquí, antes de enlazar `self.meshes`/texturas en los lotes: este paso crea y escribe
        // buffers GPU (necesita `&mut self`) y a partir de `static_batches` el `State` está
        // prestado hasta el final del frame.
        self.rt_skinned_tick = self.rt_skinned_tick.wrapping_add(1);
        self.rt_skinned_rebuild = false;
        if raytracing {
            let refresh_now = self
                .rt_skinned_tick
                .is_multiple_of(SKINNED_TRACE_REFRESH_FRAMES)
                || (self.vulkan.hw_rt() && self.rt_skinned_materials.is_empty())
                || (!self.vulkan.hw_rt() && self.rt_skinned_triangles.is_empty());
            if refresh_now {
                let targets = collect_skinned_trace_targets(self, &frustum_vp, &probe_fields);
                if self.vulkan.hw_rt() {
                    // HW RT usa el BLAS de posiciones poseadas; no deforma triángulos para el BVH.
                    self.rt_skinned_triangles.clear();
                    self.rt_skinned_materials = skinned_trace_materials(self, &targets);
                } else {
                    let (triangles, materials) = build_skinned_trace(self, &targets);
                    self.rt_skinned_triangles = triangles;
                    self.rt_skinned_materials = materials;
                }
                let rebuilt = refresh_skinned_rt_geometry(self, &targets);
                self.rt_skinned_rebuild = rebuilt;
            }
        } else {
            self.rt_skinned_triangles.clear();
            self.rt_skinned_materials.clear();
            self.rt_skinned_instances.clear();
        }

        let mut static_batches: Vec<MeshBatch> = Vec::new();
        let mut shadow_static_batches: Vec<MeshBatch> = Vec::new();
        for b in &partitioned.opaque {
            let Some(mesh) = self.meshes.get(b.mesh_idx) else {
                continue;
            };
            if b.forward_count > 0 {
                let instances = &b.instances[..b.forward_count];
                static_batches.push(MeshBatch {
                    vertex: &mesh.vertex_buffer,
                    index: &mesh.index_buffer,
                    index_count: mesh.index_count,
                    instances: bytemuck::cast_slice(instances),
                    instance_count: instances.len() as u32,
                    hi_res: b.hi_res,
                });
            }
            shadow_static_batches.push(MeshBatch {
                vertex: &mesh.vertex_buffer,
                index: &mesh.index_buffer,
                index_count: mesh.index_count,
                instances: bytemuck::cast_slice(&b.instances),
                instance_count: b.instances.len() as u32,
                hi_res: b.hi_res,
            });
        }
        for b in &partitioned.opaque_no_shadow {
            let Some(mesh) = self.meshes.get(b.mesh_idx) else {
                continue;
            };
            static_batches.push(MeshBatch {
                vertex: &mesh.vertex_buffer,
                index: &mesh.index_buffer,
                index_count: mesh.index_count,
                instances: bytemuck::cast_slice(&b.instances),
                instance_count: b.instances.len() as u32,
                hi_res: b.hi_res,
            });
        }
        let mut static_transparent_batches: Vec<MeshBatch> = Vec::new();
        for b in &partitioned.transparent {
            let Some(mesh) = self.meshes.get(b.mesh_idx) else {
                continue;
            };
            let instances_bytes = bytemuck::cast_slice(&b.instances);
            static_transparent_batches.push(MeshBatch {
                vertex: &mesh.vertex_buffer,
                index: &mesh.index_buffer,
                index_count: mesh.index_count,
                instances: instances_bytes,
                instance_count: b.instances.len() as u32,
                hi_res: b.hi_res,
            });
        }
        // Fantasma de colocación (quick build / plano): entra por el pase blend.
        let ghost_overlay = self.build_tool_ghost_overlay();
        if let Some((mesh_idx, ghost)) = &ghost_overlay
            && let Some(mesh) = self.meshes.get(*mesh_idx)
        {
            static_transparent_batches.push(MeshBatch {
                vertex: &mesh.vertex_buffer,
                index: &mesh.index_buffer,
                index_count: mesh.index_count,
                instances: bytemuck::cast_slice(std::slice::from_ref(ghost)),
                instance_count: 1,
                hi_res: false,
            });
        }

        // ── Instancias skinned (modelos animados por joints) ──
        // Una sola arena: forward y shadow apuntan al mismo bloque si la
        // entidad es visible y caster.
        let mut skinned_arena: Vec<u8> = Vec::new();
        // (gpu_idx, offset, len, transparent, hi_res)
        let mut skinned_specs: Vec<(usize, usize, usize, bool, bool)> = Vec::new();
        let mut shadow_skinned_specs: Vec<(usize, usize, usize, bool)> = Vec::new();
        for (&id, binding) in &self.model_animation_bindings {
            let Some(transform) = self.world.get::<crate::engine3d::ecs::Transform>(id) else {
                continue;
            };
            if self.sun_entity == Some(id) {
                continue;
            }
            let is_ground = self
                .world
                .get::<crate::engine3d::ecs::NameComponent>(id)
                .is_some_and(|n| n.name.eq_ignore_ascii_case("ground"));
            let (mesh_center, mesh_half) = if is_ground {
                (transform.position, transform.scale.abs() * 0.5)
            } else {
                self.entity_world_pick_aabb(id, transform)
            };
            if !is_ground
                && !self
                    .world_bounds_3d
                    .intersects_world_aabb(mesh_center, mesh_half)
            {
                continue;
            }
            let strict = is_ground || is_aabb_visible_3d(&frustum_vp, mesh_center, mesh_half);
            let caster = is_ground
                || is_shadow_caster_visible(
                    &frustum_vp,
                    mesh_center,
                    mesh_half,
                    light_dir,
                    caster_margin,
                );
            if !strict && !caster {
                continue;
            }
            let casts_shadow = caster && !entity_skips_shadow_cast(self, id);
            let is_selected =
                self.selected_entity == Some(id) || self.selected_entities.contains(&id);
            let is_hovered = self.hovered_entity == Some(id);
            let flag = self.editor_selection_flag(id, is_selected, is_hovered);
            for (part, &gpu_idx) in binding.part_gpu_indices.iter().enumerate() {
                let layer = binding
                    .part_tex_layers
                    .get(part)
                    .copied()
                    .unwrap_or(binding.tex_layer);
                // El número de capa se repite entre el array 1024 y el 4K: resolverlo
                // por búsqueda en `tex_layers` daría el primer índice (legacy) y
                // enlazaría el set equivocado. El `tex_idx` de la entidad es la clave real.
                let hi_res = self
                    .world
                    .get::<crate::engine3d::ecs::MeshComponent>(id)
                    .is_some_and(|mc| self.hi_res_tex_indices.contains(&mc.tex_idx));
                let mut inst =
                    crate::engine3d::mesh::InstanceData::new(transform.to_matrix(), flag, layer);
                if let Some(pbr) = self.world.get::<SurfacePbr>(id) {
                    apply_surface_pbr_to_instance(&mut inst, pbr);
                }
                let skinned = crate::engine3d::mesh::SkinnedInstanceData::from_instance(&inst);
                let transparent = inst.flag_pad[1] < 0.98;
                let bytes = bytemuck::bytes_of(&skinned);
                let needs_forward = strict;
                let needs_shadow = casts_shadow && !transparent;
                if needs_forward || needs_shadow {
                    let offset = skinned_arena.len();
                    skinned_arena.extend_from_slice(bytes);
                    if needs_forward {
                        skinned_specs.push((gpu_idx, offset, bytes.len(), transparent, hi_res));
                    }
                    if needs_shadow {
                        shadow_skinned_specs.push((gpu_idx, offset, bytes.len(), hi_res));
                    }
                }
            }
        }
        let mut skinned_batches: Vec<SkinnedBatch> = Vec::new();
        let mut skinned_transparent_batches: Vec<SkinnedBatch> = Vec::new();
        for (gpu_idx, offset, len, transparent, hi_res) in &skinned_specs {
            let Some(entry) = self.skinned_gpu_meshes.get(*gpu_idx) else {
                continue;
            };
            let batch = SkinnedBatch {
                vertex: &entry.mesh.vertex_buffer,
                index: &entry.mesh.index_buffer,
                index_count: entry.mesh.index_count,
                instances: &skinned_arena[*offset..*offset + *len],
                instance_count: 1,
                palette: &entry.joint_palette,
                hi_res: *hi_res,
            };
            if *transparent {
                skinned_transparent_batches.push(batch);
            } else {
                skinned_batches.push(batch);
            }
        }
        let mut shadow_skinned_batches: Vec<SkinnedBatch> = Vec::new();
        for (gpu_idx, offset, len, hi_res) in &shadow_skinned_specs {
            let Some(entry) = self.skinned_gpu_meshes.get(*gpu_idx) else {
                continue;
            };
            shadow_skinned_batches.push(SkinnedBatch {
                vertex: &entry.mesh.vertex_buffer,
                index: &entry.mesh.index_buffer,
                index_count: entry.mesh.index_count,
                instances: &skinned_arena[*offset..*offset + *len],
                instance_count: 1,
                palette: &entry.joint_palette,
                hi_res: *hi_res,
            });
        }

        // ── Overlays gizmo (paridad con `grid_pipeline`/`gizmo_pipeline` de wgpu) ──
        let gizmo_vp = self
            .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
            .view_proj;
        let world_uni_bytes =
            bytemuck::cast_slice(&crate::engine3d::gizmo::gizmo_uniform_world(gizmo_vp)).to_vec();
        let identity_uni_bytes =
            bytemuck::cast_slice(&crate::engine3d::gizmo::GIZMO_UNIFORM_IDENTITY).to_vec();

        let mut gizmos: Vec<crate::engine3d::vulkan::GizmoBatch> = Vec::new();

        if self.debug_mode && !self.preview_playing {
            let key =
                crate::engine3d::config_3d::collision_overlay::editor_collision_overlay_key(self);
            if self.collision_overlay_buffer.content_hash != key {
                let verts =
                    crate::engine3d::config_3d::collision_overlay::editor_collision_overlay_verts(
                        self,
                    );
                crate::engine3d::gizmo::write_lines(
                    &self.vulkan,
                    &mut self.collision_overlay_buffer,
                    &verts,
                );
                self.collision_overlay_buffer.content_hash = key;
            }
            if self.collision_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.collision_overlay_buffer.vertex_buffer,
                    vertex_count: self.collision_overlay_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
        }

        if !self.preview_playing || self.debug_mode {
            crate::engine3d::gizmo::refresh_lines(
                &self.vulkan,
                &mut self.probe_gizmo_buffer,
                probe_gizmo_key(&probe_list),
                || build_probe_gizmo_verts(&probe_list),
            );
            if self.probe_gizmo_buffer.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.probe_gizmo_buffer.vertex_buffer,
                    vertex_count: self.probe_gizmo_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
        }

        // Gizmo de cámara FP en modo editor: cubito en el ojo + frustum hasta el
        // rectángulo lejano, para visualizar a dónde mirará la cámara al pulsar Play.
        if !self.preview_playing
            && !self.player_ui_edit_active
            && self.has_play_character()
            && let Some((eye, yaw, pitch)) = self.play_character_camera_gizmo_pose()
        {
            let fov_y = self.camera.fov_y;
            let distance = self.fps_editor_frustum_distance;
            let key = fps_frustum_key(eye, yaw, pitch, fov_y, aspect, distance);
            if self.fps_frustum_buffer.content_hash != key {
                let verts = crate::engine3d::gizmo::fps_camera_frustum_verts(
                    eye, yaw, pitch, fov_y, aspect, distance,
                );
                crate::engine3d::gizmo::write_lines(
                    &self.vulkan,
                    &mut self.fps_frustum_buffer,
                    &verts,
                );
                self.fps_frustum_buffer.content_hash = key;
            }
            if self.fps_frustum_buffer.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.fps_frustum_buffer.vertex_buffer,
                    vertex_count: self.fps_frustum_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
        }

        if !self.preview_playing && !self.player_ui_edit_active {
            let key =
                crate::engine3d::config_3d::skeleton_debug::selected_skeleton_overlay_key(self);
            if self.skeleton_overlay_buffer.content_hash != key {
                let verts =
                    crate::engine3d::config_3d::skeleton_debug::selected_skeleton_overlay_verts(
                        self,
                    );
                crate::engine3d::gizmo::write_lines(
                    &self.vulkan,
                    &mut self.skeleton_overlay_buffer,
                    &verts,
                );
                self.skeleton_overlay_buffer.content_hash = key;
            }
            if self.skeleton_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.skeleton_overlay_buffer.vertex_buffer,
                    vertex_count: self.skeleton_overlay_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
            let key = crate::engine3d::config_3d::socket_debug::selected_socket_overlay_key(self);
            if self.socket_overlay_buffer.content_hash != key {
                let verts =
                    crate::engine3d::config_3d::socket_debug::selected_socket_overlay_verts(self);
                crate::engine3d::gizmo::write_lines(
                    &self.vulkan,
                    &mut self.socket_overlay_buffer,
                    &verts,
                );
                self.socket_overlay_buffer.content_hash = key;
            }
            if self.socket_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.socket_overlay_buffer.vertex_buffer,
                    vertex_count: self.socket_overlay_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
        }

        if (!self.preview_playing || self.debug_mode) && self.world_bounds_buffer.vertex_count > 0 {
            gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                vertices: &self.world_bounds_buffer.vertex_buffer,
                vertex_count: self.world_bounds_buffer.vertex_count,
                uniform: &world_uni_bytes,
                lines: true,
            });
        }

        // Zona clickeable del sol o del emisor bajo el cursor: disco relleno
        // del azul translúcido de hover, igual que el resaltado de modelos 3D.
        if !self.preview_playing && !self.player_ui_edit_active {
            let key = self.hover_highlight_overlay_key();
            if self.emitter_hover_overlay.content_hash != key {
                let verts = self.hover_highlight_overlay_verts();
                crate::engine3d::gizmo::write_triangles(
                    &self.vulkan,
                    &mut self.emitter_hover_overlay,
                    &verts,
                );
                self.emitter_hover_overlay.content_hash = key;
            }
            if self.emitter_hover_overlay.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.emitter_hover_overlay.vertex_buffer,
                    vertex_count: self.emitter_hover_overlay.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: false,
                });
            }
        }

        // Widget de transformación (traslación/rotación).
        let transform_uniform_bytes;
        if !self.preview_playing
            && !self.player_ui_edit_active
            && self.socket_bone_pick_entity.is_none()
            && self.bone_physics_pick_entity.is_none()
            && self.pivot_edit_mode.is_none()
            && let Some(origin) = self.selection_center()
        {
            let (gizmo_scale, axis_start) = match self.transform_gizmo_mode {
                crate::engine3d::config_3d::transform_gizmo::TransformGizmoMode::Translate => (
                    self.transform_gizmo_world_scale().unwrap_or(1.0),
                    self.transform_gizmo_axis_start_mesh()
                        .unwrap_or([0.0, 0.0, 0.0]),
                ),
                crate::engine3d::config_3d::transform_gizmo::TransformGizmoMode::Rotate => (
                    self.transform_gizmo_rotation_world_scale().unwrap_or(1.0),
                    [0.0, 0.0, 0.0],
                ),
            };
            let model =
                Mat4::from_translation(origin) * Mat4::from_scale(GlamVec3::splat(gizmo_scale));
            let hovered = self.hovered_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let active = self.active_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let uniform = crate::engine3d::gizmo::gizmo_uniform(
                gizmo_vp,
                model.to_cols_array_2d(),
                hovered,
                active,
                axis_start,
            );
            transform_uniform_bytes = bytemuck::cast_slice(&uniform).to_vec();
            let buffer = match self.transform_gizmo_mode {
                crate::engine3d::config_3d::transform_gizmo::TransformGizmoMode::Translate => {
                    &self.gizmo_buffer
                }
                crate::engine3d::config_3d::transform_gizmo::TransformGizmoMode::Rotate => {
                    &self.gizmo_rotate_buffer
                }
            };
            gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                vertices: &buffer.vertex_buffer,
                vertex_count: buffer.vertex_count,
                uniform: &transform_uniform_bytes,
                lines: false,
            });
        }

        // Overlays 2D del editor de Player UI (identidad, espacio NDC).
        if self.player_ui_edit_active {
            if self.ui_work_grid_buffer.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.ui_work_grid_buffer.vertex_buffer,
                    vertex_count: self.ui_work_grid_buffer.vertex_count,
                    uniform: &identity_uni_bytes,
                    lines: true,
                });
            }
            if self.player_ui_object_draw_overlay.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.player_ui_object_draw_overlay.vertex_buffer,
                    vertex_count: self.player_ui_object_draw_overlay.vertex_count,
                    uniform: &identity_uni_bytes,
                    lines: false,
                });
            }
        }
        if self.player_ui_hud_visible() {
            if self.player_ui_text_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.player_ui_text_overlay_buffer.vertex_buffer,
                    vertex_count: self.player_ui_text_overlay_buffer.vertex_count,
                    uniform: &identity_uni_bytes,
                    lines: false,
                });
            }
            if self.player_ui_text_editing_id.is_some()
                && self.player_ui_caret_blink_visible()
                && self.player_ui_caret_buffer.vertex_count > 0
            {
                gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                    vertices: &self.player_ui_caret_buffer.vertex_buffer,
                    vertex_count: self.player_ui_caret_buffer.vertex_count,
                    uniform: &identity_uni_bytes,
                    lines: false,
                });
            }
        }

        // Anillo de progreso de la interacción «mantener» (NDC, detrás del botón HUD).
        if self.preview_playing && self.interaction_ring_buffer.vertex_count > 0 {
            gizmos.push(crate::engine3d::vulkan::GizmoBatch {
                vertices: &self.interaction_ring_buffer.vertex_buffer,
                vertex_count: self.interaction_ring_buffer.vertex_count,
                uniform: &identity_uni_bytes,
                lines: false,
            });
        }

        let prompt_glyphs = &self.interaction_prompt_glyph_instances;
        let hud_ready = !self.player_ui_glyph_instances.is_empty() || !prompt_glyphs.is_empty();
        let mut hud_scratch = Vec::new();
        let hud_instances: &[u8] = if prompt_glyphs.is_empty() {
            bytemuck::cast_slice(&self.player_ui_glyph_instances)
        } else {
            hud_scratch.reserve(self.player_ui_glyph_instances.len() + prompt_glyphs.len());
            hud_scratch.extend_from_slice(&self.player_ui_glyph_instances);
            hud_scratch.extend_from_slice(prompt_glyphs);
            bytemuck::cast_slice(&hud_scratch)
        };
        let hud_instance_count = if prompt_glyphs.is_empty() {
            self.player_ui_glyph_instances.len() as u32
        } else {
            hud_scratch.len() as u32
        };
        let hud_vertex = hud_ready.then_some(&self.hud_quad_mesh.vertex_buffer);
        let hud_index = hud_ready.then_some(&self.hud_quad_mesh.index_buffer);
        let hud_index_count = if hud_ready {
            self.hud_quad_mesh.index_count
        } else {
            0
        };
        let hud_atlas = if hud_ready {
            Some(self.player_ui_text_atlas.image())
        } else {
            None
        };

        let rt_records = if raytracing {
            collect_rt_instances(self, &partitioned.opaque, &partitioned.opaque_no_shadow)
        } else {
            Vec::new()
        };
        // Con RT por hardware el TLAS ya tiene las instancias: no hace falta deformar
        // todos los triángulos ni reconstruir el BVH software cada vez que algo se mueve.
        let hw_rt = raytracing && self.vulkan.hw_rt();
        let packed_materials: Vec<RtMaterialGpu> = if hw_rt {
            self.rt_scene_cache = None;
            pack_rt_materials(self, &rt_records)
        } else {
            Vec::new()
        };
        let rt_blob: Option<Arc<RtSceneBlob>> = if raytracing && !hw_rt {
            let hash = rt_scene_hash(self, &rt_records);
            let cached = self
                .rt_scene_cache
                .as_ref()
                .and_then(|(cached_hash, blob)| (*cached_hash == hash).then(|| Arc::clone(blob)));
            match cached {
                Some(blob) => Some(blob),
                None => {
                    let blob = Arc::new(build_rt_scene(self, &rt_records));
                    self.rt_scene_cache = Some((hash, Arc::clone(&blob)));
                    Some(blob)
                }
            }
        } else if !raytracing {
            self.rt_scene_cache = None;
            None
        } else {
            None
        };

        let mut rt: Vec<RtBatch> = Vec::with_capacity(rt_records.len());
        for (index, rec) in rt_records.iter().enumerate() {
            let Some(mesh) = self.meshes.get(rec.mesh_idx) else {
                continue;
            };
            rt.push(RtBatch {
                key: rec.mesh_idx as u64,
                vertex: &mesh.vertex_buffer,
                index: mesh.rt_index_buffer(),
                vertex_stride: std::mem::size_of::<crate::engine3d::mesh::Vertex>() as u64,
                vertex_count: mesh.rt_positions.len() as u32,
                index_count: mesh.rt_index_count(),
                transform: mat4_to_tlas_transform(rec.transform),
                rebuild: false,
                material_slot: index,
            });
        }
        // Reflejos trazados por hardware para lo skinned: mismas poses que ya calculó el
        // refresco del BVH, con su propio BLAS (UPDATE en el command buffer del frame que
        // cambió la pose, sin submit ni fence extra).
        // La clave no colisiona con `mesh_idx` de los lotes estáticos por el bit alto.
        let skinned_base = rt_records.len();
        for instance in &self.rt_skinned_instances {
            let Some(entry) = self.skinned_gpu_meshes.get(instance.gpu_mesh_idx) else {
                continue;
            };
            let Some(buffer) = self.rt_skinned_vertices.get(&instance.gpu_mesh_idx) else {
                continue;
            };
            if instance.vertex_count == 0
                || entry.mesh.index_count < 3
                || buffer.size < u64::from(instance.vertex_count) * 12
            {
                continue;
            }
            rt.push(RtBatch {
                key: (instance.gpu_mesh_idx as u64) | (1 << 63),
                vertex: buffer,
                index: &entry.mesh.index_buffer,
                vertex_stride: 12,
                vertex_count: instance.vertex_count,
                index_count: entry.mesh.index_count,
                transform: instance.transform,
                rebuild: self.rt_skinned_rebuild,
                material_slot: skinned_base + instance.material_slot as usize,
            });
        }

        let empty: &[u8] = &[];
        let packed_material_bytes: &[u8] = bytemuck::cast_slice(&packed_materials);
        let (bvh_nodes, bvh_node_count, bvh_tris, bvh_tri_count, materials, material_count) =
            match rt_blob.as_ref() {
                Some(blob) => (
                    blob.nodes.as_slice(),
                    blob.node_count,
                    blob.tris.as_slice(),
                    blob.tri_count,
                    blob.materials.as_slice(),
                    blob.material_count,
                ),
                None if hw_rt => (
                    empty,
                    0,
                    empty,
                    0,
                    packed_material_bytes,
                    packed_materials.len() as u32,
                ),
                None => (empty, 0, empty, 0, empty, 0),
            };

        let albedo = self.texture_array.image();
        let model_albedo = self.model_texture_array.as_ref().map(|array| array.image());

        let input = crate::engine3d::vulkan::FrameInput {
            scene: scene_bytes,
            sky: self.sky_uniform_for_frame(),
            sun_above_horizon: self.sun_above_horizon(),
            shadows: self.shadow_tier
                != crate::engine3d::config_3d::shadow_graphics::ShadowTier::Off,
            shadow_map_size: self.active_shadow_map_size(),
            msaa_samples: self.msaa_sample_count,
            ssaa_scale: self.ssaa_scale,
            debug_ssr_view: self.reflection_debug_view == ReflectionDebugView::SsrDebug,
            debug_rt_view: self.reflection_debug_view == ReflectionDebugView::RtDebug,
            taa_enabled: self.taa_enabled,
            taa_blend: self.taa_blend,
            reflections,
            ssr,
            raytracing,
            rtx: self.rtx_preset(),
            ssr_preset: self.ssr_preset(),
            probe_captures: &probe_captures,
            static_batches: &static_batches,
            shadow_static_batches: &shadow_static_batches,
            static_transparent_batches: &static_transparent_batches,
            skinned_batches: &skinned_batches,
            shadow_skinned_batches: &shadow_skinned_batches,
            skinned_transparent_batches: &skinned_transparent_batches,
            gizmos: &gizmos,
            hud_vertex,
            hud_index,
            hud_index_count,
            hud_instances,
            hud_instance_count,
            hud_atlas,
            albedo,
            model_albedo,
            rt: &rt,
            materials,
            material_count,
            bvh_nodes,
            bvh_node_count,
            bvh_tris,
            bvh_tri_count,
            particles: &particle_batches,
            particle_camera,
            volumetric_center: [eye.x, eye.y, eye.z],
            volumetric_light_dir: {
                let d = self.scene_light_dir();
                [d[0], d[1], d[2]]
            },
            volumetric_resolution: if self.shadow_tier.volumetric_enabled() {
                self.shadow_tier.volumetric_resolution()
            } else {
                0
            },
            volumetric_steps: self.shadow_tier.volumetric_steps(),
            volumetric_blend: self.shadow_tier.volumetric_blend(),
        };

        // `last_draw_calls` se inicializaba a 0 y nunca se actualizaba en 3D
        // (el 2D sí lo cuenta en `vulkan_render.rs`). Reflejamos los lotes que
        // este frame envía al renderer antes de grabarlo.
        self.last_draw_calls = count_frame_draws(&input);

        self.frame_renderer
            .as_mut()
            .expect("FrameRenderer no inicializado")
            .record(frame, &input)
            .map_err(|_e| Validation)?;
        Ok(())
    }

    /// Transita la imagen del swapchain a `PRESENT_SRC_KHR` tras grabar la escena.
    ///
    /// En ventana única el editor no la llama: graba egui encima y transita al final.
    pub fn finish_present(&self, frame: &FrameTarget) {
        if let Some(renderer) = self.frame_renderer.as_ref() {
            renderer.finish_present(frame);
        }
    }
}

fn probe_gizmo_key(list: &[(crate::engine3d::ecs::EntityId, GlamVec3, usize)]) -> u64 {
    let mut hash = crate::engine3d::gizmo::overlay_hash();
    crate::engine3d::gizmo::mix_u64(&mut hash, list.len() as u64);
    for (id, position, slot) in list {
        crate::engine3d::gizmo::mix_u64(&mut hash, u64::from(*id));
        crate::engine3d::gizmo::mix_vec3(&mut hash, position.to_array());
        crate::engine3d::gizmo::mix_u64(&mut hash, *slot as u64);
    }
    hash
}

fn fps_frustum_key(
    eye: GlamVec3,
    yaw: f32,
    pitch: f32,
    fov_y: f32,
    aspect: f32,
    distance: f32,
) -> u64 {
    let mut hash = crate::engine3d::gizmo::overlay_hash();
    crate::engine3d::gizmo::mix_vec3(&mut hash, eye.to_array());
    crate::engine3d::gizmo::mix_f32(&mut hash, yaw);
    crate::engine3d::gizmo::mix_f32(&mut hash, pitch);
    crate::engine3d::gizmo::mix_f32(&mut hash, fov_y);
    crate::engine3d::gizmo::mix_f32(&mut hash, aspect);
    crate::engine3d::gizmo::mix_f32(&mut hash, distance);
    hash
}

/// Draws aproximados del frame: un draw por lote de geometría visible, más
/// HUD y pase de sombras. El conteo exacto de `vkCmdDraw*` está repartido entre
/// los pases del `FrameRenderer`; para el overlay basta con reflejar la carga.
fn count_frame_draws(input: &crate::engine3d::vulkan::FrameInput<'_>) -> u32 {
    let mut calls = 0u32;
    calls += input.static_batches.len() as u32;
    calls += input.static_transparent_batches.len() as u32;
    calls += input.skinned_batches.len() as u32;
    calls += input.skinned_transparent_batches.len() as u32;
    calls += input.gizmos.len() as u32;
    calls += input.rt.len() as u32;
    calls += input.particles.len() as u32;
    if input.hud_index_count > 0 {
        calls += 1;
    }
    if input.shadows && input.sun_above_horizon {
        calls += input.shadow_static_batches.len() as u32;
        calls += input.shadow_skinned_batches.len() as u32;
    }
    calls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tlas_transform_is_row_major_3x4() {
        let matrix = Mat4::from_translation(GlamVec3::new(1.0, 2.0, 3.0));
        assert_eq!(
            mat4_to_tlas_transform(matrix),
            [
                1.0, 0.0, 0.0, 1.0, //
                0.0, 1.0, 0.0, 2.0, //
                0.0, 0.0, 1.0, 3.0,
            ]
        );
    }

    fn test_view_proj() -> Mat4 {
        let proj = Mat4::perspective_rh(60f32.to_radians(), 1.0, 0.1, 100.0);
        let view = Mat4::look_at_rh(GlamVec3::ZERO, GlamVec3::NEG_Z, GlamVec3::Y);
        proj * view
    }

    #[test]
    fn frustum_keeps_aabb_in_front() {
        let vp = test_view_proj();
        assert!(is_aabb_visible_3d(
            &vp,
            GlamVec3::new(0.0, 0.0, -5.0),
            GlamVec3::splat(0.5)
        ));
        assert!(is_aabb_visible_3d(
            &vp,
            GlamVec3::new(0.0, 0.0, -80.0),
            GlamVec3::splat(0.5)
        ));
    }

    #[test]
    fn frustum_culls_aabb_behind_camera() {
        let vp = test_view_proj();
        assert!(!is_aabb_visible_3d(
            &vp,
            GlamVec3::new(0.0, 0.0, 5.0),
            GlamVec3::splat(0.5)
        ));
    }

    #[test]
    fn frustum_culls_aabb_before_near_plane() {
        let vp = test_view_proj();
        assert!(!is_aabb_visible_3d(
            &vp,
            GlamVec3::new(0.0, 0.0, -0.01),
            GlamVec3::splat(0.001)
        ));
    }
}
