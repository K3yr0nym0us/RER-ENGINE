use glam::Mat4;
use rer_engine_core::surface::SurfacePresentError;

use crate::config_2d::transform_visual_center;
use crate::ecs::MeshComponent;
use crate::mesh;

use super::State;
use super::render_helpers::is_visible_2d;

impl State {
    pub fn render(&mut self) -> Result<(), SurfacePresentError> {
        self.update_animations();
        let vulkan = self.vulkan2d.clone();
        self.render_vulkan(&vulkan)
    }

    /// Recopila los sprites visibles del frame y los agrupa por malla.
    ///
    /// El culling por viewport 2D, el orden por `(layer, z)` y los flags de selección/hover viven
    /// en un único sitio para que la lógica de negocio no dependa del backend.
    pub(crate) fn collect_sprite_batches(&self) -> Vec<(usize, Vec<mesh::InstanceData>)> {
        let aspect_fc = self.size.width as f32 / self.size.height as f32;
        // query2<MeshComponent, Transform> itera solo entidades con ambos componentes,
        // evitando el scan de todas las entidades + doble lookup de hash por entidad.
        let visual_offsets = &self.visual_offsets;
        let mut entities: Vec<(crate::ecs::EntityId, usize, usize, Mat4, i32, f32)> = self
            .world
            .query2::<MeshComponent, crate::ecs::Transform>()
            .filter_map(|(id, mc, t)| {
                let mesh_idx = mc.mesh_idx;
                let tex_idx = mc.tex_idx;
                // ── Culling por viewport 2D ──────────────────────────────────
                // Para culling usamos la posición visual (con offset de pivot)
                let visual_pos =
                    transform_visual_center(t.position, visual_offsets.get(&id).copied());
                let visible = self
                    .camera_2d
                    .as_ref()
                    .map(|cam2d| is_visible_2d(cam2d, visual_pos, t.scale, aspect_fc))
                    .unwrap_or(true);
                if !visible {
                    return None;
                }
                let model_mat = Mat4::from_scale_rotation_translation(
                    t.scale,
                    t.rotation,
                    transform_visual_center(t.position, visual_offsets.get(&id).copied()),
                );
                let layer = self
                    .world
                    .get::<crate::ecs::RenderLayer>(id)
                    .map(|rl| rl.value)
                    .unwrap_or(0);
                let z = t.position.z;
                Some((id, mesh_idx, tex_idx, model_mat, layer, z))
            })
            .collect();
        // Sort by (layer ASC, z ASC) — lower layer first, within layer sort by z (back-to-front)
        entities.sort_by(|a, b| {
            let layer_cmp = a.4.cmp(&b.4);
            if layer_cmp != std::cmp::Ordering::Equal {
                layer_cmp
            } else {
                a.5.partial_cmp(&b.5).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        // Con el atlas todas las entidades comparten el mismo bind group,
        // así que solo agrupamos por geometría (mesh_idx).
        // El UV rect viaja en cada instancia — no hay cambio de bind group entre batches.
        let mut batches: Vec<(usize, Vec<mesh::InstanceData>)> = Vec::new();
        for (entity_id, mesh_idx, tex_idx, model_matrix, _layer, _z) in &entities {
            if self.preview_playing
                && !self.debug_mode
                && (self.collider_entities.contains(entity_id)
                    || self.execution_area_entities.contains(entity_id))
            {
                continue;
            }
            let is_selected = self.selected_entity == Some(*entity_id)
                || self.selected_entities.contains(entity_id);
            let flag = if self.preview_playing {
                0.0_f32
            } else if is_selected {
                1.0_f32 // dorado
            } else if self.hovered_entity == Some(*entity_id) {
                2.0_f32 // cian
            } else {
                0.0_f32
            };
            // anim_overrides tiene prioridad sobre uv_rects[]:
            // durante una animación activa evita mutar la UV base de la entidad.
            let uv = self
                .anim_overrides
                .get(tex_idx)
                .copied()
                .or_else(|| self.uv_rects.get(*tex_idx).copied())
                .unwrap_or(self.fallback_uv);
            let mut inst = mesh::InstanceData::new(*model_matrix, flag, uv);
            inst.flag_pad[2] = if self
                .world
                .get::<crate::config_2d::ColliderMarker>(*entity_id)
                .is_some()
            {
                1.0_f32
            } else if self
                .world
                .get::<crate::config_2d::ExecutionAreaMarker>(*entity_id)
                .is_some()
            {
                2.0_f32
            } else {
                0.0_f32
            };
            // Extender el último batch si coincide mesh (mismo UV rect viaja por instancia)
            let can_extend = batches.last().is_some_and(|(m, _)| *m == *mesh_idx);
            if can_extend {
                batches.last_mut().unwrap().1.push(inst);
            } else {
                batches.push((*mesh_idx, vec![inst]));
            }
        }
        batches
    }
}
