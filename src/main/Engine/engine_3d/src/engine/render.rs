//! Frame 3D del motor sobre Ash (Vulkan).
//!
//! `State::render` ahora graba el frame entero a travÃ©s de
//! `crate::vulkan::FrameRenderer` y presenta con el swapchain de Ash.
//! El motor 2D sigue en wgpu y no toca este archivo.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use glam::{Mat4, Vec3 as GlamVec3};
use winit::dpi::PhysicalSize;

use rer_engine_shared::wgpu_surface::{SurfacePresentError, SurfacePresentError::*};

use crate::bvh::{RtMaterialGpu, RtSceneBlob, RtTriangle};
use crate::config_3d::reflection_graphics::ReflectionTier;
use crate::ecs::{MeshComponent, SurfacePbr};
use crate::gizmo;
use crate::mesh;
use crate::vulkan::{FrameAcquire, MeshBatch, RtBatch, SkinnedBatch};

use super::{SceneUniforms, State};

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct TransparentReflUniforms {
    enabled: f32,
    strength: f32,
    refl_mix: f32,
    _pad: f32,
}

fn instance_is_transparent(inst: &crate::mesh::InstanceData) -> bool {
    inst.flag_pad[1] < 0.99
}

fn apply_surface_pbr_to_instance(
    inst: &mut crate::mesh::InstanceData,
    pbr: &crate::ecs::SurfacePbr,
) {
    inst.flag_pad[3] = pbr.roughness;
    inst.tex_layer_pad[1] = pbr.metallic;
    inst.tex_layer_pad[3] = pbr.ior;
    inst.flag_pad[1] = crate::config_3d::pbr_presets::instance_visual_alpha(pbr);
}

/// Lote de instancias de una misma malla/capa/probe para el pase forward.
pub(crate) struct SceneInstanceBatch {
    pub(crate) mesh_idx: usize,
    pub(crate) texture_layer: u32,
    pub(crate) probe_layer: i32,
    pub(crate) instances: Vec<crate::mesh::InstanceData>,
    pub(crate) entity_ids: Vec<crate::ecs::EntityId>,
}

/// Instancia trazable (una entidad), compartida por el BVH software y el TLAS hardware.
#[derive(Clone, Copy)]
struct RtInstanceRecord {
    mesh_idx: usize,
    entity_id: crate::ecs::EntityId,
    transform: Mat4,
    probe_layer: i32,
}

fn collect_rt_instances(state: &State, batches: &[SceneInstanceBatch]) -> Vec<RtInstanceRecord> {
    let mut out = Vec::new();
    for batch in batches {
        let Some(mesh) = state.meshes.get(batch.mesh_idx) else {
            continue;
        };
        if mesh.rt_indices.len() < 3 {
            continue;
        }
        for (inst, entity_id) in batch.instances.iter().zip(batch.entity_ids.iter()) {
            out.push(RtInstanceRecord {
                mesh_idx: batch.mesh_idx,
                entity_id: *entity_id,
                transform: Mat4::from_cols_array_2d(&inst.model),
                probe_layer: inst.tex_layer_pad[2] as i32,
            });
        }
    }
    out
}

fn rt_scene_hash(state: &State, records: &[RtInstanceRecord]) -> u64 {
    let mut hasher = DefaultHasher::new();
    records.len().hash(&mut hasher);
    for rec in records {
        rec.entity_id.hash(&mut hasher);
        rec.mesh_idx.hash(&mut hasher);
        rec.probe_layer.hash(&mut hasher);
        for column in rec.transform.to_cols_array() {
            column.to_bits().hash(&mut hasher);
        }
        let tex_idx = state
            .world
            .get::<MeshComponent>(rec.entity_id)
            .map(|mesh| mesh.tex_idx)
            .unwrap_or(0);
        tex_idx.hash(&mut hasher);
        if let Some(pbr) = state.world.get::<SurfacePbr>(rec.entity_id) {
            pbr.roughness.to_bits().hash(&mut hasher);
            pbr.metallic.to_bits().hash(&mut hasher);
            pbr.ior.to_bits().hash(&mut hasher);
            pbr.opacity.to_bits().hash(&mut hasher);
        }
    }
    // La geometría skinned no está en `records` (se deforma en CPU y se cachea), así que su
    // refresco entra en el hash por tick: el BVH se reconstruye cuando cambia la pose cacheada y
    // no en los frames intermedios.
    if !state.rt_skinned_triangles.is_empty() {
        (state.rt_skinned_tick / SKINNED_TRACE_REFRESH_FRAMES.max(1)).hash(&mut hasher);
    }
    hasher.finish()
}

fn rt_material(state: &State, rec: &RtInstanceRecord) -> RtMaterialGpu {
    let tex_idx = state
        .world
        .get::<MeshComponent>(rec.entity_id)
        .map(|mesh| mesh.tex_idx)
        .unwrap_or(0);
    let albedo = state
        .tex_layer_albedo
        .get(tex_idx)
        .copied()
        .unwrap_or([0.75, 0.75, 0.75]);
    let (roughness, metallic, ior, dielectric) =
        if let Some(pbr) = state.world.get::<SurfacePbr>(rec.entity_id) {
            (pbr.roughness, pbr.metallic, pbr.ior.max(0.0), pbr.ior > 1.0)
        } else {
            (0.5, 0.0, 1.5, false)
        };
    RtMaterialGpu {
        albedo: [
            albedo[0],
            albedo[1],
            albedo[2],
            f32::from_bits(u32::from(dielectric)),
        ],
        pbr: [roughness, metallic, ior, tex_idx as f32],
        probe: [rec.probe_layer as f32, 0.0, 0.0, 0.0],
    }
}

/// Cada cuántos frames se re-deforma la geometría skinned para el BVH. A 60 fps son 15 Hz de
/// actualización de la silueta reflejada, de sobra para un reflejo y mucho más barato que un
/// `build_bvh` por frame.
const SKINNED_TRACE_REFRESH_FRAMES: u32 = 4;
/// Techo de triángulos skinned trazables por escena: protege el frame en rigs con mallas pesadas.
const SKINNED_TRACE_MAX_TRIANGLES: usize = 24_000;

/// Una pieza skinned visible, con todo lo necesario para trazarse.
#[derive(Clone)]
struct SkinnedTraceTarget {
    entity_id: crate::ecs::EntityId,
    gpu_mesh_idx: usize,
    part_index: usize,
    model_matrix: Mat4,
    probe_layer: i32,
    /// Clave en `State::model_assets` para llegar a los vértices de bind pose.
    asset_path: String,
}

/// Piezas skinned visibles, **ordenadas** por (entidad, pieza).
///
/// El orden no es cosmético: `build_skinned_trace` numera los materiales siguiendo esta lista y
/// `refresh_skinned_rt_geometry` crea las instancias del TLAS en el mismo orden. Iterar
/// `model_animation_bindings` (un `HashMap`) dos veces puede dar órdenes distintos y desalinear
/// material con geometría, porque el hit shader indexa `mats[slot]` con el índice de instancia.
fn collect_skinned_trace_targets(
    state: &State,
    frustum_vp: &Mat4,
    probe_fields: &[crate::probe_registry::ProbeField],
) -> Vec<SkinnedTraceTarget> {
    let mut out: Vec<SkinnedTraceTarget> = Vec::new();
    for (&id, binding) in &state.model_animation_bindings {
        let Some(transform) = state.world.get::<crate::ecs::Transform>(id) else {
            continue;
        };
        if state.sun_entity == Some(id) {
            continue;
        }
        let model_matrix = transform.to_matrix();
        let (center, half) = state.entity_world_pick_aabb(id, transform);
        if !is_aabb_visible_3d(frustum_vp, center, half) {
            continue;
        }
        if !state.model_assets.contains_key(&binding.asset_path) {
            continue;
        }
        let probe_layer = crate::probe_registry::select_probe_slot(probe_fields, center)
            .map(|slot| slot as i32)
            .unwrap_or(-1);
        for (part_index, &gpu_mesh_idx) in binding.part_gpu_indices.iter().enumerate() {
            if state
                .skinned_gpu_meshes
                .get(gpu_mesh_idx)
                .is_some_and(|entry| !entry.joint_palette.is_empty())
            {
                out.push(SkinnedTraceTarget {
                    entity_id: id,
                    gpu_mesh_idx,
                    part_index,
                    model_matrix,
                    probe_layer,
                    asset_path: binding.asset_path.clone(),
                });
            }
        }
    }
    out.sort_by_key(|target| (target.entity_id, target.part_index));
    out
}

/// Deforma en CPU las mallas skinned visibles y devuelve sus triángulos en espacio mundo con sus
/// materiales.
///
/// Usa la misma fórmula que `shaders/raster/skinned.vert`
/// (`model * (Σ pesos·palette[joint] · posición)`), así la silueta que trazan el BVH y el RT
/// coincide con la rasterizada. Los materiales se numeran desde 0: `build_rt_scene` les suma el
/// desplazamiento de los lotes estáticos.
fn build_skinned_trace(
    state: &State,
    targets: &[SkinnedTraceTarget],
) -> (Vec<crate::bvh::RtTriangle>, Vec<RtMaterialGpu>) {
    let mut triangles: Vec<crate::bvh::RtTriangle> = Vec::new();
    let mut materials: Vec<RtMaterialGpu> = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        // Un material por objetivo, siempre y en este orden: `refresh_skinned_rt_geometry` numera
        // sus instancias con el índice del objetivo, y un hueco aquí desalinearía materiales.
        let record = RtInstanceRecord {
            mesh_idx: 0,
            entity_id: target.entity_id,
            transform: target.model_matrix,
            probe_layer: target.probe_layer,
        };
        materials.push(rt_material(state, &record));
        let Some(asset) = state.model_assets.get(&target.asset_path) else {
            continue;
        };
        let (Some(entry), Some(part)) = (
            state.skinned_gpu_meshes.get(target.gpu_mesh_idx),
            asset.parts.get(target.part_index),
        ) else {
            continue;
        };
        if triangles.len() >= SKINNED_TRACE_MAX_TRIANGLES {
            return (triangles, materials);
        }
        let palette: Vec<Mat4> = entry
            .joint_palette
            .iter()
            .map(Mat4::from_cols_array_2d)
            .collect();
        if palette.is_empty() {
            continue;
        }
        let vertices = &part.mesh.vertices;
        for chunk in part.mesh.indices.chunks_exact(3) {
            let mut tri = crate::bvh::RtTriangle {
                v0: GlamVec3::ZERO,
                v1: GlamVec3::ZERO,
                v2: GlamVec3::ZERO,
                uv0: [0.0; 2],
                uv1: [0.0; 2],
                uv2: [0.0; 2],
                material_slot: index as u32,
            };
            let mut ok = true;
            for (out_pos, out_uv, src_uv) in [
                (&mut tri.v0, &mut tri.uv0, 0usize),
                (&mut tri.v1, &mut tri.uv1, 1usize),
                (&mut tri.v2, &mut tri.uv2, 2usize),
            ] {
                let index = chunk[src_uv] as usize;
                let Some(vertex) = vertices.get(index) else {
                    ok = false;
                    break;
                };
                let local =
                    crate::config_3d::model_asset::skinned_vertex_position(&palette, vertex);
                *out_pos = target.model_matrix.transform_point3(local);
                *out_uv = vertex.uv;
            }
            if ok {
                triangles.push(tri);
            }
        }
    }
    (triangles, materials)
}

/// Escribe en GPU las posiciones locales ya poseadas de cada pieza skinned y prepara las
/// instancias para el TLAS (camino de hardware).
///
/// Devuelve `true` si este frame reescribió alguna pose: solo entonces hay que reconstruir esos
/// BLAS, porque el resto de frames se reutilizan los cacheados en `FrameRenderer`. Sin esto, el
/// personaje animado no aparecía en los reflejos trazados por hardware (solo en el BVH).
///
/// El buffer guarda solo posiciones (`stride` 12 bytes) y conserva el orden de vértices del VBO
/// skinned, así se reutiliza su propio índice. `material_slot` sigue el orden de `targets`, el
/// mismo que usó `build_skinned_trace`.
fn refresh_skinned_rt_geometry(state: &mut State, targets: &[SkinnedTraceTarget]) -> bool {
    let material_count = state.rt_skinned_materials.len();
    let mut pending: Vec<(usize, Vec<u8>, u32, usize)> = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        // Sin material asignado no hay instancia posible: `build_skinned_trace` pudo truncar la
        // lista por el tope de triángulos.
        if index >= material_count {
            break;
        }
        let Some(asset) = state.model_assets.get(&target.asset_path) else {
            continue;
        };
        let (Some(entry), Some(part)) = (
            state.skinned_gpu_meshes.get(target.gpu_mesh_idx),
            asset.parts.get(target.part_index),
        ) else {
            continue;
        };
        let palette: Vec<Mat4> = entry
            .joint_palette
            .iter()
            .map(Mat4::from_cols_array_2d)
            .collect();
        if palette.is_empty() || part.mesh.indices.len() < 3 {
            continue;
        }
        let mut bytes = Vec::with_capacity(part.mesh.vertices.len() * 12);
        for vertex in &part.mesh.vertices {
            let posed = crate::config_3d::model_asset::skinned_vertex_position(&palette, vertex);
            bytes.extend_from_slice(bytemuck::bytes_of(&posed.to_array()));
        }
        if bytes.is_empty() {
            continue;
        }
        let vertex_count = (bytes.len() / 12) as u32;
        pending.push((target.gpu_mesh_idx, bytes, vertex_count, index));
    }

    let mut instances: Vec<crate::bvh::SkinnedRtInstance> = Vec::with_capacity(pending.len());
    let mut refreshed = false;
    for (gpu_mesh_idx, bytes, vertex_count, order) in pending {
        let needed = bytes.len() as u64;
        let must_create = match state.rt_skinned_vertices.get(&gpu_mesh_idx) {
            Some(buffer) => buffer.size < needed,
            None => true,
        };
        if must_create {
            match state
                .vulkan
                .create_trace_vertex_buffer(needed, "rt-skinned-vertices")
            {
                Ok(buffer) => {
                    state.rt_skinned_vertices.insert(gpu_mesh_idx, buffer);
                }
                Err(error) => {
                    // Sin buffer no hay instancia: el frame sigue dibujando, solo pierde ese
                    // reflejo. Se avisa una vez por pieza.
                    log::warn!(
                        "[reflexiones] sin buffer de pose para la pieza skinned {gpu_mesh_idx}: {error}"
                    );
                    continue;
                }
            }
        }
        let Some(buffer) = state.rt_skinned_vertices.get_mut(&gpu_mesh_idx) else {
            continue;
        };
        if let Err(error) = buffer.write(0, &bytes) {
            log::warn!(
                "[reflexiones] no se pudo subir la pose de la pieza {gpu_mesh_idx}: {error}"
            );
            continue;
        }
        let Some(target) = targets.get(order) else {
            continue;
        };
        instances.push(crate::bvh::SkinnedRtInstance {
            gpu_mesh_idx,
            transform: mat4_to_tlas_transform(target.model_matrix),
            vertex_count,
            // El índice del objetivo = su posición en `rt_skinned_materials`.
            material_slot: order as u32,
        });
        refreshed = true;
    }
    state.rt_skinned_instances = instances;
    refreshed
}

/// Triángulos en espacio mundo + materiales por instancia → BVH y buffers GPU.
fn build_rt_scene(state: &State, records: &[RtInstanceRecord]) -> RtSceneBlob {
    let mut triangles: Vec<RtTriangle> = Vec::new();
    let mut materials: Vec<RtMaterialGpu> = Vec::with_capacity(records.len());
    for (slot, rec) in records.iter().enumerate() {
        let slot = slot as u32;
        materials.push(rt_material(state, rec));
        let Some(mesh) = state.meshes.get(rec.mesh_idx) else {
            continue;
        };
        let positions = &mesh.rt_positions;
        let uvs = &mesh.rt_uvs;
        for chunk in mesh.rt_indices.chunks_exact(3) {
            let (i0, i1, i2) = (chunk[0] as usize, chunk[1] as usize, chunk[2] as usize);
            if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
                continue;
            }
            let v0 = rec
                .transform
                .transform_point3(GlamVec3::from_array(positions[i0]));
            let v1 = rec
                .transform
                .transform_point3(GlamVec3::from_array(positions[i1]));
            let v2 = rec
                .transform
                .transform_point3(GlamVec3::from_array(positions[i2]));
            triangles.push(RtTriangle {
                v0,
                v1,
                v2,
                uv0: uvs.get(i0).copied().unwrap_or([0.0, 0.0]),
                uv1: uvs.get(i1).copied().unwrap_or([0.0, 0.0]),
                uv2: uvs.get(i2).copied().unwrap_or([0.0, 0.0]),
                material_slot: slot,
            });
        }
    }
    // Geometría skinned ya deformada (pose del personaje en el último refresco). Sus materiales se
    // añadieron después de los de los lotes estáticos, así que se les corre el índice.
    let skinned_base = materials.len() as u32;
    for triangle in &state.rt_skinned_triangles {
        let mut triangle = *triangle;
        triangle.material_slot += skinned_base;
        triangles.push(triangle);
    }
    materials.extend(state.rt_skinned_materials.iter().copied());
    let (nodes, gpu_triangles) = crate::bvh::build_bvh(triangles);
    RtSceneBlob::from_parts(&nodes, &gpu_triangles, &materials)
}

/// Mat4 (column-major) → transform 3×4 row-major que espera el TLAS.
fn mat4_to_tlas_transform(matrix: Mat4) -> [f32; 12] {
    let c = matrix.to_cols_array();
    [
        c[0], c[4], c[8], c[12], c[1], c[5], c[9], c[13], c[2], c[6], c[10], c[14],
    ]
}

/// Wireframe de esferas para los reflection probes del editor.
fn build_probe_gizmo_verts(
    list: &[(crate::ecs::EntityId, GlamVec3, usize)],
) -> Vec<gizmo::GizmoVertex> {
    let color = [1.0_f32, 0.85, 0.18, 0.9];
    let radius = crate::probe_registry::REFLECTION_PROBE_GIZMO_RADIUS_M;
    let segments = 48u32;
    let mut verts = Vec::new();
    for (_, center, _) in list {
        for axis in 0..3 {
            for i in 0..segments {
                let a0 = std::f32::consts::TAU * i as f32 / segments as f32;
                let a1 = std::f32::consts::TAU * (i + 1) as f32 / segments as f32;
                let p0 = gizmo::rotation_ring_point(axis, radius, a0);
                let p1 = gizmo::rotation_ring_point(axis, radius, a1);
                for p in [p0, p1] {
                    verts.push(gizmo::GizmoVertex {
                        position: [center.x + p[0], center.y + p[1], center.z + p[2]],
                        color,
                    });
                }
            }
        }
    }
    verts
}

pub(crate) fn filter_batches_by_alpha(
    batches: &[SceneInstanceBatch],
    want_transparent: bool,
) -> Vec<SceneInstanceBatch> {
    batches
        .iter()
        .filter_map(|b| {
            let mut instances = Vec::new();
            let mut entity_ids = Vec::new();
            for (inst, eid) in b.instances.iter().zip(b.entity_ids.iter()) {
                if instance_is_transparent(inst) == want_transparent {
                    instances.push(*inst);
                    entity_ids.push(*eid);
                }
            }
            if instances.is_empty() {
                return None;
            }
            Some(SceneInstanceBatch {
                mesh_idx: b.mesh_idx,
                texture_layer: b.texture_layer,
                probe_layer: b.probe_layer,
                instances,
                entity_ids,
            })
        })
        .collect()
}

/// Instancias estÃ¡ticas para escena o captura de probe.
pub(crate) fn build_scene_instance_batches(
    state: &State,
    entities: &[(crate::ecs::EntityId, usize, usize, Mat4, i32)],
    probe_fields: &[crate::probe_registry::ProbeField],
) -> Vec<SceneInstanceBatch> {
    let mut batches: Vec<SceneInstanceBatch> = Vec::new();
    for (entity_id, mesh_idx, tex_idx, model_matrix, _layer) in entities {
        if state.quick_build_ghost_id == Some(*entity_id)
            || state.plane_tool_ghost_id == Some(*entity_id)
        {
            continue;
        }
        if state.preview_playing
            && !state.debug_mode
            && (state.collider_entities.contains(entity_id)
                || state.execution_area_entities.contains(entity_id))
        {
            continue;
        }
        let is_selected = state.selected_entity == Some(*entity_id)
            || state.selected_entities.contains(entity_id);
        let is_hovered = state.hovered_entity == Some(*entity_id);
        let flag = state.editor_selection_flag(*entity_id, is_selected, is_hovered);
        let layer = state.texture_layer_for(*tex_idx);
        let mut inst = crate::mesh::InstanceData::new(*model_matrix, flag, layer);
        if let Some(pbr) = state.world.get::<crate::ecs::SurfacePbr>(*entity_id) {
            apply_surface_pbr_to_instance(&mut inst, pbr);
        }
        // Ranura de sonda de la entidad: la más cercana cuyo volumen la contiene (`-1` = ninguna →
        // el shader cae al cielo procedural). Se resuelve por entidad y entra en la clave de lote,
        // así dos entidades con la misma malla pero distinta sonda no comparten instancing.
        let probe_layer = crate::probe_registry::select_probe_slot(
            probe_fields,
            model_matrix.transform_point3(GlamVec3::ZERO),
        )
        .map(|slot| slot as i32)
        .unwrap_or(-1);
        inst.tex_layer_pad[2] = probe_layer as f32;
        if state.is_plane_wall_entity(*entity_id) {
            inst.flag_pad[1] = crate::config_3d::plane_tools::PLANE_WALL_VISUAL_ALPHA;
            inst.flag_pad[2] = crate::config_3d::plane_tools::PLANE_WALL_RENDER_KIND;
        }
        let can_extend = batches.last().is_some_and(|b| {
            b.mesh_idx == *mesh_idx && b.texture_layer == layer && b.probe_layer == probe_layer
        });
        if can_extend {
            batches.last_mut().unwrap().instances.push(inst);
            batches.last_mut().unwrap().entity_ids.push(*entity_id);
        } else {
            batches.push(SceneInstanceBatch {
                mesh_idx: *mesh_idx,
                texture_layer: layer,
                probe_layer,
                instances: vec![inst],
                entity_ids: vec![*entity_id],
            });
        }
    }
    batches
}

impl State {
    /// Reconfigura el swapchain de Ash al nuevo tamaÃ±o del viewport.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        if let Err(e) = self.vulkan.resize(new_size.width, new_size.height) {
            log::warn!("[resize] no se pudo reconfigurar el swapchain: {e}");
        }
    }

    fn editor_selection_flag(
        &self,
        entity_id: crate::ecs::EntityId,
        is_selected: bool,
        is_hovered: bool,
    ) -> f32 {
        if self.preview_playing {
            return 0.0;
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

    /// Graba y presenta un frame entero con Ash (Vulkan).
    ///
    /// Todos los datos de `FrameInput` se construyen como locales de este mÃ©todo, asÃ­ que
    /// `FrameInput` nunca presta de `self` y no entra en conflicto con el prÃ©stamo mutable
    /// de `frame_renderer` al grabar.
    pub fn render(&mut self) -> Result<(), SurfacePresentError> {
        self.update_animations();
        self.update_skinned_animations();
        self.sync_socket_attached_children();
        self.sync_player_ui_hud();

        let size = self.size;
        let aspect = size.width as f32 / size.height.max(1) as f32;
        let reflections = self.reflection_tier != ReflectionTier::Off;
        let raytracing = reflections && self.reflection_raytracing_enabled;

        // Probes: la lista se resuelve una vez por frame (gizmos + captura).
        let mut probe_list: Vec<(crate::ecs::EntityId, GlamVec3, usize)> = Vec::new();
        let mut probe_fields: Vec<crate::probe_registry::ProbeField> = Vec::new();
        if reflections {
            self.ensure_probe_slots_allocated();
            probe_list = self.reflection_probe_render_list();
            probe_fields = self.reflection_probe_fields();
        }
        let probes_active = self.reflection_probes_enabled && !probe_list.is_empty();

        // Uniformes de escena completos (cámara + luz + bias), como en el render wgpu.
        let view = self.camera_view_matrix();
        let eye = self.camera_world_position();
        let proj = self.camera.proj_matrix(aspect);
        let vp_stable = (proj * view).to_cols_array_2d();
        let vp = proj * view;
        let view_proj = vp.to_cols_array_2d();
        let inv_view_proj = vp.inverse().to_cols_array_2d();
        let mut light_params = self.scene_light_params();
        light_params[1] = if probes_active {
            self.shadow_darkness.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let cascades = self.build_shadow_cascades(aspect);
        let cascade_count = cascades.count;
        let scene_uni = SceneUniforms {
            view_proj,
            view_proj_stable: vp_stable,
            prev_view_proj: self.prev_view_proj,
            inv_view_proj,
            cam_pos: [eye.x, eye.y, eye.z, 0.0],
            light_dir: self.scene_light_dir(),
            light_color: self.scene_light_color(),
            light_view_proj: cascades.view_proj,
            cascade_splits: cascades.splits,
            cascade_extent: cascades.extents,
            shadow_params: self.scene_shadow_params(cascade_count),
            light_params,
            jitter: [0.0, 0.0, 0.0, 0.0],
            depth_plane: [
                self.camera.near,
                self.camera.far,
                if reflections { 1.0 } else { 0.0 },
                0.0,
            ],
            shadow_bias: self.scene_shadow_bias(),
        };
        self.prev_view_proj = vp_stable;
        let scene_bytes = bytemuck::bytes_of(&scene_uni);

        let frame = match self.vulkan.begin_frame() {
            Ok(frame) => frame,
            Err(FrameAcquire::Outdated) => return Err(Reconfigure),
            Err(FrameAcquire::Skip) => return Err(SkipFrame),
            Err(FrameAcquire::Fail(_e)) => return Err(Validation),
        };

        // â”€â”€ Datos del frame (locales) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let frustum_vp = {
            let raw = self
                .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
                .view_proj;
            Mat4::from_cols_array_2d(&raw)
        };

        let mut entities: Vec<(crate::ecs::EntityId, usize, usize, Mat4, i32)> = self
            .world
            .query2::<crate::ecs::MeshComponent, crate::ecs::Transform>()
            .filter_map(|(id, mc, t)| {
                if self.model_animation_bindings.contains_key(&id) {
                    return None;
                }
                let mesh_idx = mc.mesh_idx;
                let is_ground = self
                    .world
                    .get::<crate::ecs::NameComponent>(id)
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
                if !is_ground && !is_aabb_visible_3d(&frustum_vp, mesh_center, mesh_half) {
                    return None;
                }
                Some((id, mesh_idx, mc.tex_idx, t.to_matrix(), 0))
            })
            .collect();
        entities.sort_by_key(|a| a.4);

        let batches = build_scene_instance_batches(self, &entities, &probe_fields);
        let opaque_batches = filter_batches_by_alpha(&batches, false);

        // ── Probes: ranuras estables + burst/round-robin de captura ──
        let probe_captures: Vec<crate::vulkan::ProbeCapture> = if probes_active {
            let ids: Vec<crate::ecs::EntityId> = probe_list.iter().map(|(id, _, _)| *id).collect();
            self.sync_probe_capture_burst_for_entity_set(&ids);
            let burst = self.probe_capture_burst_all;
            self.probe_capture_burst_all = false;
            if burst {
                // Cambió el conjunto de probes: refresco completo una sola vez.
                self.probe_capture_cooldown = 0;
                probe_list
                    .iter()
                    .map(|(_, position, slot)| crate::vulkan::ProbeCapture {
                        origin: position.to_array(),
                        slot: *slot,
                    })
                    .collect()
            } else if self.probe_capture_cooldown > 0 {
                self.probe_capture_cooldown -= 1;
                Vec::new()
            } else {
                // Refresco de una probe cada ~30 frames, no las 6 caras por frame.
                let index = self.probe_capture_cursor % probe_list.len();
                self.probe_capture_cursor = self.probe_capture_cursor.wrapping_add(1);
                self.probe_capture_cooldown = 30;
                let (_, position, slot) = probe_list[index];
                vec![crate::vulkan::ProbeCapture {
                    origin: position.to_array(),
                    slot,
                }]
            }
        } else {
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
                || self.rt_skinned_triangles.is_empty();
            if refresh_now {
                let targets = collect_skinned_trace_targets(self, &frustum_vp, &probe_fields);
                let (triangles, materials) = build_skinned_trace(self, &targets);
                self.rt_skinned_triangles = triangles;
                self.rt_skinned_materials = materials;
                let rebuilt = refresh_skinned_rt_geometry(self, &targets);
                self.rt_skinned_rebuild = rebuilt;
            }
        } else {
            self.rt_skinned_triangles.clear();
            self.rt_skinned_materials.clear();
            self.rt_skinned_instances.clear();
        }

        let mut static_batches: Vec<MeshBatch> = Vec::new();
        for b in &opaque_batches {
            let Some(mesh) = self.meshes.get(b.mesh_idx) else {
                continue;
            };
            let instances_bytes = bytemuck::cast_slice(&b.instances);
            static_batches.push(MeshBatch {
                vertex: &mesh.vertex_buffer,
                index: &mesh.index_buffer,
                index_count: mesh.index_count,
                instances: instances_bytes,
                instance_count: b.instances.len() as u32,
            });
        }
        let transparent_batches = filter_batches_by_alpha(&batches, true);
        let mut static_transparent_batches: Vec<MeshBatch> = Vec::new();
        for b in &transparent_batches {
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
            });
        }

        // ── Instancias skinned (modelos animados por joints) ──
        let mut skinned_arena: Vec<u8> = Vec::new();
        let mut skinned_specs: Vec<(usize, usize, usize, bool)> = Vec::new();
        for (&id, binding) in &self.model_animation_bindings {
            let Some(transform) = self.world.get::<crate::ecs::Transform>(id) else {
                continue;
            };
            if self.sun_entity == Some(id) {
                continue;
            }
            let is_ground = self
                .world
                .get::<crate::ecs::NameComponent>(id)
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
            if !is_ground && !is_aabb_visible_3d(&frustum_vp, mesh_center, mesh_half) {
                continue;
            }
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
                let mut inst = crate::mesh::InstanceData::new(transform.to_matrix(), flag, layer);
                if let Some(pbr) = self.world.get::<SurfacePbr>(id) {
                    apply_surface_pbr_to_instance(&mut inst, pbr);
                }
                let skinned = crate::mesh::SkinnedInstanceData::from_instance(&inst);
                let transparent = inst.flag_pad[1] < 0.98;
                let bytes = bytemuck::bytes_of(&skinned);
                let offset = skinned_arena.len();
                skinned_arena.extend_from_slice(bytes);
                skinned_specs.push((gpu_idx, offset, bytes.len(), transparent));
            }
        }
        let mut skinned_batches: Vec<SkinnedBatch> = Vec::new();
        let mut skinned_transparent_batches: Vec<SkinnedBatch> = Vec::new();
        for (gpu_idx, offset, len, transparent) in &skinned_specs {
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
            };
            if *transparent {
                skinned_transparent_batches.push(batch);
            } else {
                skinned_batches.push(batch);
            }
        }

        // ── Overlays gizmo (paridad con `grid_pipeline`/`gizmo_pipeline` de wgpu) ──
        let gizmo_vp = self
            .camera_to_uniform_at_anchor(self.orbit_view_anchor(), aspect)
            .view_proj;
        let world_uni_bytes = bytemuck::cast_slice(&gizmo::gizmo_uniform_world(gizmo_vp)).to_vec();
        let identity_uni_bytes = bytemuck::cast_slice(&gizmo::GIZMO_UNIFORM_IDENTITY).to_vec();

        let mut gizmos: Vec<crate::vulkan::GizmoBatch> = Vec::new();

        if self.debug_mode && !self.preview_playing {
            let verts = crate::config_3d::collision_overlay::editor_collision_overlay_verts(self);
            gizmo::write_lines(&self.vulkan, &mut self.collision_overlay_buffer, &verts);
            if self.collision_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
                    vertices: &self.collision_overlay_buffer.vertex_buffer,
                    vertex_count: self.collision_overlay_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
        }

        if !self.preview_playing || self.debug_mode {
            let verts = build_probe_gizmo_verts(&probe_list);
            gizmo::write_lines(&self.vulkan, &mut self.probe_gizmo_buffer, &verts);
            if self.probe_gizmo_buffer.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
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
            let verts = gizmo::fps_camera_frustum_verts(
                eye,
                yaw,
                pitch,
                self.camera.fov_y,
                aspect,
                self.fps_editor_frustum_distance,
            );
            gizmo::write_lines(&self.vulkan, &mut self.fps_frustum_buffer, &verts);
            if self.fps_frustum_buffer.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
                    vertices: &self.fps_frustum_buffer.vertex_buffer,
                    vertex_count: self.fps_frustum_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
        }

        if !self.preview_playing && !self.player_ui_edit_active {
            let verts = crate::config_3d::skeleton_debug::selected_skeleton_overlay_verts(self);
            gizmo::write_lines(&self.vulkan, &mut self.skeleton_overlay_buffer, &verts);
            if self.skeleton_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
                    vertices: &self.skeleton_overlay_buffer.vertex_buffer,
                    vertex_count: self.skeleton_overlay_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
            let verts = crate::config_3d::socket_debug::selected_socket_overlay_verts(self);
            gizmo::write_lines(&self.vulkan, &mut self.socket_overlay_buffer, &verts);
            if self.socket_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
                    vertices: &self.socket_overlay_buffer.vertex_buffer,
                    vertex_count: self.socket_overlay_buffer.vertex_count,
                    uniform: &world_uni_bytes,
                    lines: true,
                });
            }
        }

        if (!self.preview_playing || self.debug_mode) && self.world_bounds_buffer.vertex_count > 0 {
            gizmos.push(crate::vulkan::GizmoBatch {
                vertices: &self.world_bounds_buffer.vertex_buffer,
                vertex_count: self.world_bounds_buffer.vertex_count,
                uniform: &world_uni_bytes,
                lines: true,
            });
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
                crate::config_3d::transform_gizmo::TransformGizmoMode::Translate => (
                    self.transform_gizmo_world_scale().unwrap_or(1.0),
                    self.transform_gizmo_axis_start_mesh()
                        .unwrap_or([0.0, 0.0, 0.0]),
                ),
                crate::config_3d::transform_gizmo::TransformGizmoMode::Rotate => (
                    self.transform_gizmo_rotation_world_scale().unwrap_or(1.0),
                    [0.0, 0.0, 0.0],
                ),
            };
            let model =
                Mat4::from_translation(origin) * Mat4::from_scale(GlamVec3::splat(gizmo_scale));
            let hovered = self.hovered_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let active = self.active_gizmo_axis.map(|a| a as f32).unwrap_or(-1.0);
            let uniform = gizmo::gizmo_uniform(
                gizmo_vp,
                model.to_cols_array_2d(),
                hovered,
                active,
                axis_start,
            );
            transform_uniform_bytes = bytemuck::cast_slice(&uniform).to_vec();
            let buffer = match self.transform_gizmo_mode {
                crate::config_3d::transform_gizmo::TransformGizmoMode::Translate => {
                    &self.gizmo_buffer
                }
                crate::config_3d::transform_gizmo::TransformGizmoMode::Rotate => {
                    &self.gizmo_rotate_buffer
                }
            };
            gizmos.push(crate::vulkan::GizmoBatch {
                vertices: &buffer.vertex_buffer,
                vertex_count: buffer.vertex_count,
                uniform: &transform_uniform_bytes,
                lines: false,
            });
        }

        // Overlays 2D del editor de Player UI (identidad, espacio NDC).
        if self.player_ui_edit_active {
            if self.ui_work_grid_buffer.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
                    vertices: &self.ui_work_grid_buffer.vertex_buffer,
                    vertex_count: self.ui_work_grid_buffer.vertex_count,
                    uniform: &identity_uni_bytes,
                    lines: true,
                });
            }
            if self.player_ui_object_draw_overlay.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
                    vertices: &self.player_ui_object_draw_overlay.vertex_buffer,
                    vertex_count: self.player_ui_object_draw_overlay.vertex_count,
                    uniform: &identity_uni_bytes,
                    lines: false,
                });
            }
        }
        if self.player_ui_hud_visible() {
            if self.player_ui_text_overlay_buffer.vertex_count > 0 {
                gizmos.push(crate::vulkan::GizmoBatch {
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
                gizmos.push(crate::vulkan::GizmoBatch {
                    vertices: &self.player_ui_caret_buffer.vertex_buffer,
                    vertex_count: self.player_ui_caret_buffer.vertex_count,
                    uniform: &identity_uni_bytes,
                    lines: false,
                });
            }
        }

        let hud_ready = self.player_ui_hud_visible() && !self.player_ui_glyph_instances.is_empty();
        let hud_vertex = hud_ready.then_some(&self.hud_quad_mesh.vertex_buffer);
        let hud_index = hud_ready.then_some(&self.hud_quad_mesh.index_buffer);
        let hud_index_count = if hud_ready {
            self.hud_quad_mesh.index_count
        } else {
            0
        };
        let hud_instances: &[u8] = if hud_ready {
            bytemuck::cast_slice(&self.player_ui_glyph_instances)
        } else {
            &[]
        };
        let hud_instance_count = if hud_ready {
            self.player_ui_glyph_instances.len() as u32
        } else {
            0
        };
        let hud_atlas = if hud_ready {
            Some(self.player_ui_text_atlas.image())
        } else {
            None
        };

        let rt_records = if raytracing {
            collect_rt_instances(self, &opaque_batches)
        } else {
            Vec::new()
        };
        let rt_blob: Option<Arc<RtSceneBlob>> = if raytracing {
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
        } else {
            self.rt_scene_cache = None;
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
                vertex_stride: std::mem::size_of::<mesh::Vertex>() as u64,
                vertex_count: mesh.rt_positions.len() as u32,
                index_count: mesh.rt_index_count(),
                transform: mat4_to_tlas_transform(rec.transform),
                rebuild: false,
                material_slot: index,
            });
        }
        // Reflejos trazados por hardware para lo skinned: mismas poses que ya calculó el
        // refresco del BVH, con su propio BLAS (rebuild solo en el frame que cambió la pose).
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
                None => (empty, 0, empty, 0, empty, 0),
            };

        let albedo = self.texture_array.image();

        let input = crate::vulkan::FrameInput {
            scene: scene_bytes,
            shadows: self.shadow_tier != crate::config_3d::shadow_graphics::ShadowTier::Off,
            shadow_map_size: self.active_shadow_map_size(),
            shadow_cascades: cascade_count,
            msaa_samples: self.msaa_sample_count,
            debug_ssr_view: self.reflection_debug_view
                != crate::config_3d::reflection_graphics::ReflectionDebugView::Final,
            taa_enabled: self.taa_enabled,
            taa_blend: self.taa_blend,
            reflections,
            raytracing,
            probe_captures: &probe_captures,
            static_batches: &static_batches,
            static_transparent_batches: &static_transparent_batches,
            skinned_batches: &skinned_batches,
            skinned_transparent_batches: &skinned_transparent_batches,
            gizmos: &gizmos,
            hud_vertex,
            hud_index,
            hud_index_count,
            hud_instances,
            hud_instance_count,
            hud_atlas,
            albedo,
            rt: &rt,
            materials,
            material_count,
            bvh_nodes,
            bvh_node_count,
            bvh_tris,
            bvh_tri_count,
        };

        self.frame_renderer
            .as_mut()
            .expect("FrameRenderer no inicializado")
            .record(&frame, &input)
            .map_err(|_e| Validation)?;

        self.vulkan.submit_and_present(&frame).map_err(|e| match e {
            FrameAcquire::Outdated | FrameAcquire::Skip => Reconfigure,
            FrameAcquire::Fail(_e) => Validation,
        })
    }
}

pub(crate) fn is_aabb_visible_3d(view_proj: &glam::Mat4, center: GlamVec3, half: GlamVec3) -> bool {
    let m = view_proj.to_cols_array_2d();
    let r0 = [m[0][0], m[1][0], m[2][0], m[3][0]];
    let r1 = [m[0][1], m[1][1], m[2][1], m[3][1]];
    let r2 = [m[0][2], m[1][2], m[2][2], m[3][2]];
    let r3 = [m[0][3], m[1][3], m[2][3], m[3][3]];

    // Vulkan: z en [0, 1]. Los planos laterales usan r3 ± r0/r1 y el
    // near/far son r2 y r3 - r2 (no r3 ± r2, que es la convención OpenGL).
    let planes: [[f32; 4]; 6] = [
        [r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]],
        [r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]],
        [r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]],
        [r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]],
        [r2[0], r2[1], r2[2], r2[3]],
        [r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]],
    ];

    for plane in planes {
        let d = plane[0] * center.x + plane[1] * center.y + plane[2] * center.z + plane[3];
        if d < -half.x * plane[0].abs() - half.y * plane[1].abs() - half.z * plane[2].abs() {
            return false;
        }
    }
    true
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
