//! Demo PBR material validation (5 rows × N spheres).
//!
//! Contenido de demostración únicamente: entidades normales con `SurfacePbr` estándar.
//! No forma parte del sistema de reflejos; borrar todas las esferas MatVal no afecta probes.

use glam::{Quat, Vec3};

use crate::ecs::{EntityId, MeshComponent, NameComponent, NonSelectable, SurfacePbr, Transform};
use crate::engine::State;
use crate::entity_save_meta::{entity_path_marker, EntitySaveMeta};
use crate::ipc::{send_event, EngineEvent};

use super::pbr_presets::{
    build_label_rgba, build_preset_albedo_rgba, preset_roughness_at_column,
    surface_pbr_from_preset, PbrMaterialPreset, MAT_VAL_ROW_COUNT, MAT_VAL_SPHERES_PER_ROW,
    MATERIAL_VALIDATION_ROWS,
};

pub const MAT_VAL_PATH: &str = "[MatVal]";
pub const MAT_VAL_LABEL_PATH: &str = "[MatValLabel]";

const SPHERE_RADIUS: f32 = 0.55;
const X_POSITIONS: [f32; MAT_VAL_SPHERES_PER_ROW] = [-4.0, -2.0, 0.0, 2.0, 4.0];
/// Primera fila detrás del jugador (~z=5); filas siguientes hacia −Z (espacio libre).
const ROW_Z_START: f32 = 1.5;
const ROW_Z_SPACING: f32 = 3.5;
const LABEL_X: f32 = -6.2;
const LABEL_Y: f32 = SPHERE_RADIUS + 1.35;
const LABEL_WIDTH: f32 = 2.8;
const LABEL_HEIGHT: f32 = 0.55;
const TEX_SIZE: u32 = 32;

pub fn ensure_material_validation_scene(state: &mut State) {
    if !needs_rebuild(state) {
        update_existing_scene(state);
        return;
    }
    clear_validation_scene(state);
    spawn_validation_scene(state);
}

fn needs_rebuild(state: &State) -> bool {
    mat_val_sphere_ids(state).len() != MAT_VAL_ROW_COUNT * MAT_VAL_SPHERES_PER_ROW
        || mat_val_label_ids(state).len() != MAT_VAL_ROW_COUNT
}

fn mat_val_sphere_ids(state: &State) -> Vec<EntityId> {
    state
        .save_registry
        .meta
        .iter()
        .filter(|(_, m)| entity_path_marker(&m.path) == Some(MAT_VAL_PATH))
        .map(|(id, _)| *id)
        .collect()
}

fn mat_val_label_ids(state: &State) -> Vec<EntityId> {
    state
        .save_registry
        .meta
        .iter()
        .filter(|(_, m)| entity_path_marker(&m.path) == Some(MAT_VAL_LABEL_PATH))
        .map(|(id, _)| *id)
        .collect()
}

fn clear_validation_scene(state: &mut State) {
    let mut ids: Vec<EntityId> = mat_val_sphere_ids(state);
    ids.extend(mat_val_label_ids(state));
    for id in ids {
        despawn_validation_entity(state, id);
    }
}

fn despawn_validation_entity(state: &mut State, id: EntityId) {
    state.selected_entities.retain(|&e| e != id);
    if Some(id) == state.selected_entity {
        state.selected_entity = state.selected_entities.last().copied();
    }
    if Some(id) == state.hovered_entity {
        state.hovered_entity = None;
    }
    state.physics.remove_entity_body(id);
    state.scenario_entities.retain(|&e| e != id);
    state.entity_colision.remove(&id);
    state.save_registry.remove_entity(id);
    state.world.despawn(id);
    send_event(&EngineEvent::EntityRemoved {
        id,
        kind: "model".to_string(),
    });
}

fn row_z(row: usize) -> f32 {
    ROW_Z_START - row as f32 * ROW_Z_SPACING
}

fn sphere_position(row: usize, col: usize) -> [f32; 3] {
    [X_POSITIONS[col], SPHERE_RADIUS, row_z(row)]
}

fn update_existing_scene(state: &mut State) {
    let mut spheres = mat_val_sphere_ids(state);
    spheres.sort_unstable();
    for row in 0..MAT_VAL_ROW_COUNT {
        let preset = &MATERIAL_VALIDATION_ROWS[row];
        let tex_idx = state.mat_val_texture_for_preset(preset);
        for col in 0..MAT_VAL_SPHERES_PER_ROW {
            let idx = row * MAT_VAL_SPHERES_PER_ROW + col;
            let Some(id) = spheres.get(idx).copied() else {
                continue;
            };
            let rough = preset_roughness_at_column(preset, col, MAT_VAL_SPHERES_PER_ROW);
            let pos = sphere_position(row, col);
            if let Some(t) = state.world.get_mut::<Transform>(id) {
                t.position = Vec3::from_array(pos);
                t.scale = Vec3::splat(SPHERE_RADIUS * 2.0);
            }
            let pbr = surface_pbr_from_preset(preset, rough);
            if let Some(existing) = state.world.get_mut::<SurfacePbr>(id) {
                *existing = pbr;
            } else {
                state.world.insert(id, pbr);
            }
            state.apply_mat_val_sphere_visual(id, tex_idx);
        }
    }
    let mut labels = mat_val_label_ids(state);
    labels.sort_unstable();
    for (row, &id) in labels.iter().enumerate().take(MAT_VAL_ROW_COUNT) {
        let preset = &MATERIAL_VALIDATION_ROWS[row];
        let tex_idx = state.mat_val_texture_for_label(preset);
        let mesh_idx = state.mat_val_label_mesh_idx();
        if let Some(t) = state.world.get_mut::<Transform>(id) {
            t.position = Vec3::new(LABEL_X, LABEL_Y, row_z(row));
            t.rotation = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
            t.scale = Vec3::new(LABEL_WIDTH, LABEL_HEIGHT, 1.0);
        }
        if let Some(mc) = state.world.get_mut::<MeshComponent>(id) {
            mc.mesh_idx = mesh_idx;
            mc.tex_idx = tex_idx;
        }
        if let Some(name) = state.world.get_mut::<NameComponent>(id) {
            name.name = format!("MatVal · {}", preset.label);
        }
    }
}

fn spawn_validation_scene(state: &mut State) {
    for row in 0..MAT_VAL_ROW_COUNT {
        let preset = &MATERIAL_VALIDATION_ROWS[row];
        let tex_idx = state.mat_val_texture_for_preset(preset);
        for col in 0..MAT_VAL_SPHERES_PER_ROW {
            let rough = preset_roughness_at_column(preset, col, MAT_VAL_SPHERES_PER_ROW);
            let name = format!(
                "MatVal · {} · {:.2}",
                preset.label,
                rough
            );
            spawn_mat_val_sphere(state, preset, &name, sphere_position(row, col), rough, tex_idx);
        }
        spawn_mat_val_row_label(state, preset, row);
    }
    log::info!(
        "[MatVal] escena de validación PBR: {} filas × {} esferas",
        MAT_VAL_ROW_COUNT,
        MAT_VAL_SPHERES_PER_ROW
    );
}

fn spawn_mat_val_sphere(
    state: &mut State,
    preset: &PbrMaterialPreset,
    name: &str,
    position: [f32; 3],
    roughness: f32,
    tex_idx: usize,
) {
    let id = state.world.spawn(Some(name));
    state.apply_mat_val_sphere_visual(id, tex_idx);
    if let Some(t) = state.world.get_mut::<Transform>(id) {
        t.position = Vec3::from_array(position);
        t.scale = Vec3::splat(SPHERE_RADIUS * 2.0);
    }
    state
        .world
        .insert(id, surface_pbr_from_preset(preset, roughness));
    state.entity_colision.insert(id, false);
    state.scenario_entities.push(id);
    state.save_registry.register_meta(
        id,
        EntitySaveMeta {
            kind: "model".to_string(),
            path: MAT_VAL_PATH.to_string(),
            visual_model_path: None,
            entity_category: Some("object".to_string()),
        },
    );
    send_event(&EngineEvent::ModelLoaded {
        id,
        name: Some(name.to_string()),
        position: Some(position),
        scale: Some([SPHERE_RADIUS * 2.0; 3]),
        rotation: Some([0.0, 0.0, 0.0, 1.0]),
        path: Some(MAT_VAL_PATH.to_string()),
        kind: Some("model".to_string()),
        blueprint_id: None,
        physics_enabled: Some(false),
        physics_type: None,
        entity_category: Some("object".to_string()),
    });
}

fn spawn_mat_val_row_label(state: &mut State, preset: &PbrMaterialPreset, row: usize) {
    let name = format!("MatVal · {}", preset.label);
    let id = state.world.spawn(Some(&name));
    let mesh_idx = state.mat_val_label_mesh_idx();
    let tex_idx = state.mat_val_texture_for_label(preset);
    state.world.insert(
        id,
        MeshComponent {
            mesh_idx,
            tex_idx,
        },
    );
    state.world.insert(id, NonSelectable);
    if let Some(t) = state.world.get_mut::<Transform>(id) {
        t.position = Vec3::new(LABEL_X, LABEL_Y, row_z(row));
        t.rotation = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        t.scale = Vec3::new(LABEL_WIDTH, LABEL_HEIGHT, 1.0);
    }
    state.save_registry.register_meta(
        id,
        EntitySaveMeta {
            kind: "model".to_string(),
            path: MAT_VAL_LABEL_PATH.to_string(),
            visual_model_path: None,
            entity_category: Some("object".to_string()),
        },
    );
    send_event(&EngineEvent::ModelLoaded {
        id,
        name: Some(name.clone()),
        position: Some([LABEL_X, LABEL_Y, row_z(row)]),
        scale: Some([LABEL_WIDTH, LABEL_HEIGHT, 1.0]),
        rotation: Some([0.0, 0.0, 0.0, 1.0]),
        path: Some(MAT_VAL_LABEL_PATH.to_string()),
        kind: Some("model".to_string()),
        blueprint_id: None,
        physics_enabled: Some(false),
        physics_type: None,
        entity_category: Some("object".to_string()),
    });
}

impl State {
    pub(crate) fn mat_val_label_mesh_idx(&mut self) -> usize {
        if let Some(idx) = self.mat_val_label_mesh_idx {
            return idx;
        }
        let idx = self.meshes.len();
        self.meshes
            .push(crate::mesh::create_unit_quad_xy(&self.device));
        self.mat_val_label_mesh_idx = Some(idx);
        idx
    }

    pub(crate) fn mat_val_texture_for_preset(&mut self, preset: &PbrMaterialPreset) -> usize {
        let key = format!("preset:{}", preset.id);
        if let Some(&idx) = self.mat_val_texture_cache.get(&key) {
            return idx;
        }
        let rgba = build_preset_albedo_rgba(preset, TEX_SIZE);
        let tex_idx = self.tex_layers.len();
        let layer = self
            .texture_array
            .pack(&self.queue, &rgba, TEX_SIZE, TEX_SIZE);
        self.tex_layers.push(layer);
        self.mat_val_texture_cache.insert(key, tex_idx);
        tex_idx
    }

    pub(crate) fn mat_val_texture_for_label(&mut self, preset: &PbrMaterialPreset) -> usize {
        let key = format!("label:v2:{}", preset.id);
        if let Some(&idx) = self.mat_val_texture_cache.get(&key) {
            return idx;
        }
        let (rgba, w, h) = build_label_rgba(preset.label_tag, preset.base_color);
        let tex_idx = self.tex_layers.len();
        let layer = self.texture_array.pack(&self.queue, &rgba, w, h);
        self.tex_layers.push(layer);
        self.mat_val_texture_cache.insert(key, tex_idx);
        tex_idx
    }

    pub(crate) fn apply_mat_val_sphere_visual(&mut self, id: EntityId, tex_idx: usize) {
        let mesh_idx = self.sun_icon_mesh_idx();
        if let Some(mc) = self.world.get_mut::<MeshComponent>(id) {
            mc.mesh_idx = mesh_idx;
            mc.tex_idx = tex_idx;
        } else {
            self.world.insert(
                id,
                MeshComponent {
                    mesh_idx,
                    tex_idx,
                },
            );
        }
    }
}
