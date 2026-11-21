//! Demo PBR material comparison (1 sphere per preset row + labels + floor quads).

use glam::{Quat, Vec3};

use crate::engine3d::ecs::{
    EntityId, MeshComponent, NameComponent, NonSelectable, SurfacePbr, Transform,
};
use crate::engine3d::engine::State;
use crate::engine3d::entity_save_meta::{EntitySaveMeta, entity_path_marker};
use crate::engine3d::ipc::{EngineEvent, send_event};

use super::pbr_presets::{
    MAT_VAL_ROW_COUNT, MATERIAL_VALIDATION_ROWS, PbrMaterialPreset, build_label_rgba,
    build_preset_albedo_rgba, surface_pbr_from_preset,
};

pub const MAT_VAL_PATH: &str = "[MatVal]";
pub const MAT_VAL_LABEL_PATH: &str = "[MatValLabel]";
pub const PARTICLE_DEMO_LABEL_PATH: &str = "[ParticleDemoLabel]";

const TEX_SIZE: u32 = 32;

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

pub const MATERIAL_COMPARISON_PATH: &str = "[MatComparison]";

/// Alfombra MatComp: ancho X × largo Z (quad unitario escalado).
const MAT_COMP_QUAD_X_WIDTH: f32 = 2.0;
const MAT_COMP_QUAD_Z_LENGTH: f32 = 3.5;
/// Delante de las esferas (z=-5): borde trasero ≈ -6.0, sin invadir la fila de esferas.
const MAT_COMP_QUAD_Z: f32 = -4.0;
const MAT_COMP_X_POSITIONS: [f32; MAT_VAL_ROW_COUNT] = [-6.0, -3.0, 0.0, 3.0, 6.0];

fn apply_mat_comp_quad_transform(t: &mut Transform, x: f32) {
    t.position = Vec3::new(x, 0.03, MAT_COMP_QUAD_Z);
    t.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    t.scale = Vec3::new(MAT_COMP_QUAD_X_WIDTH, MAT_COMP_QUAD_Z_LENGTH, 1.0);
}

fn material_comparison_ids(state: &State) -> Vec<EntityId> {
    state
        .save_registry
        .meta
        .iter()
        .filter(|(_, m)| entity_path_marker(&m.path) == Some(MATERIAL_COMPARISON_PATH))
        .map(|(id, _)| *id)
        .collect()
}

fn clear_material_comparison(state: &mut State) {
    for id in material_comparison_ids(state) {
        despawn_validation_entity(state, id);
    }
}

fn material_comparison_sphere_ids(state: &State) -> Vec<EntityId> {
    state
        .save_registry
        .meta
        .iter()
        .filter(|(_, m)| entity_path_marker(&m.path) == Some(MATERIAL_COMPARISON_PATH))
        .filter_map(|(id, _)| {
            state.world.get::<NameComponent>(*id).and_then(|n| {
                if n.name.starts_with("MatComp · ")
                    && !n.name.contains("Label")
                    && !n.name.contains("Quad")
                    && !n.name.contains("Wall")
                {
                    Some(*id)
                } else {
                    None
                }
            })
        })
        .collect()
}

fn material_comparison_quad_ids(state: &State) -> Vec<EntityId> {
    state
        .save_registry
        .meta
        .iter()
        .filter(|(_, m)| entity_path_marker(&m.path) == Some(MATERIAL_COMPARISON_PATH))
        .filter_map(|(id, _)| {
            state.world.get::<NameComponent>(*id).and_then(|n| {
                if n.name.starts_with("MatCompQuad · ") {
                    Some(*id)
                } else {
                    None
                }
            })
        })
        .collect()
}

fn sync_material_comparison_pbr(state: &mut State) {
    let mut spheres = material_comparison_sphere_ids(state);
    spheres.sort_unstable();
    for (i, &id) in spheres.iter().enumerate().take(MAT_VAL_ROW_COUNT) {
        let preset = &MATERIAL_VALIDATION_ROWS[i];
        let roughness = preset.roughness_min;
        let pbr = surface_pbr_from_preset(preset, roughness);
        if let Some(existing) = state.world.get_mut::<SurfacePbr>(id) {
            *existing = pbr;
        } else {
            state.world.insert(id, pbr);
        }
        let tex_idx = state.mat_val_texture_for_preset(preset);
        state.apply_mat_val_sphere_visual(id, tex_idx);
    }

    let mut quads = material_comparison_quad_ids(state);
    quads.sort_unstable();
    for (i, &id) in quads.iter().enumerate().take(MAT_VAL_ROW_COUNT) {
        let preset = &MATERIAL_VALIDATION_ROWS[i];
        let roughness = preset.roughness_min;
        let pbr = surface_pbr_from_preset(preset, roughness);
        let tex_idx = state.mat_val_texture_for_preset(preset);
        if let Some(existing) = state.world.get_mut::<SurfacePbr>(id) {
            *existing = pbr;
        } else {
            state.world.insert(id, pbr);
        }
        if let Some(mc) = state.world.get_mut::<MeshComponent>(id) {
            mc.tex_idx = tex_idx;
        }
        if let Some(t) = state.world.get_mut::<Transform>(id) {
            let x = MAT_COMP_X_POSITIONS.get(i).copied().unwrap_or(0.0);
            apply_mat_comp_quad_transform(t, x);
        }
    }
    log::info!(
        "[MatVal] PBR sincronizado en {} esferas y {} alfombras",
        spheres.len().min(MAT_VAL_ROW_COUNT),
        quads.len().min(MAT_VAL_ROW_COUNT)
    );
}

fn particle_demo_label_ids(state: &State) -> Vec<EntityId> {
    state
        .save_registry
        .meta
        .iter()
        .filter(|(_, m)| entity_path_marker(&m.path) == Some(PARTICLE_DEMO_LABEL_PATH))
        .map(|(id, _)| *id)
        .collect()
}

fn particle_demo_label_pose(origin: [f32; 3]) -> (Vec3, Quat, Vec3) {
    const LABEL_W: f32 = 1.35;
    const LABEL_H: f32 = 0.45;
    const FRONT: f32 = 1.7;
    // Misma lógica que MatComp (todos de frente, sin look-at): el quad XY mira +Z;
    // −90° en Y los alinea hacia −X, el frente de la fila en z.
    let rotation = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
    let position = Vec3::new(origin[0] - FRONT, LABEL_H * 0.5, origin[2]);
    (position, rotation, Vec3::new(LABEL_W, LABEL_H, 1.0))
}

fn spawn_particle_demo_label(state: &mut State, text: &str, accent: [f32; 3], origin: [f32; 3]) {
    let (position, rotation, scale) = particle_demo_label_pose(origin);
    let name = format!("ParticleDemo · {text}");
    let id = state.world.spawn(Some(&name));
    let mesh_idx = state.mat_val_label_mesh_idx();
    let tex_idx = state.texture_for_label_text(&format!("particle-label:v2:{text}"), text, accent);
    state.world.insert(id, MeshComponent { mesh_idx, tex_idx });
    state.world.insert(id, NonSelectable);
    state.world.insert(
        id,
        SurfacePbr {
            roughness: 0.62,
            metallic: 0.0,
            ior: 0.0,
            opacity: 1.0,
        },
    );
    if let Some(t) = state.world.get_mut::<Transform>(id) {
        t.position = position;
        t.rotation = rotation;
        t.scale = scale;
    }
    state.save_registry.register_meta(
        id,
        EntitySaveMeta {
            kind: "model".to_string(),
            path: PARTICLE_DEMO_LABEL_PATH.to_string(),
            visual_model_path: None,
            entity_category: Some("object".to_string()),
        },
    );
    send_event(&EngineEvent::ModelLoaded {
        id,
        name: Some(name),
        position: Some(position.to_array()),
        scale: Some(scale.to_array()),
        rotation: Some([rotation.x, rotation.y, rotation.z, rotation.w]),
        path: Some(PARTICLE_DEMO_LABEL_PATH.to_string()),
        kind: Some("model".to_string()),
        blueprint_id: None,
        physics_enabled: Some(false),
        physics_type: None,
        entity_category: Some("object".to_string()),
    });
}

fn particle_demo_labels_current(
    state: &State,
    slots: &[crate::engine3d::particles::ParticleDemoSlot; 5],
) -> bool {
    let ids = particle_demo_label_ids(state);
    if ids.len() != slots.len() {
        return false;
    }
    let mut have: Vec<String> = ids
        .iter()
        .filter_map(|id| state.world.name(*id).map(str::to_owned))
        .collect();
    if have.len() != slots.len() {
        return false;
    }
    have.sort();
    let mut want: Vec<String> = slots
        .iter()
        .map(|slot| format!("ParticleDemo · {}", slot.label))
        .collect();
    want.sort();
    if have != want {
        return false;
    }
    slots.iter().all(|slot| {
        let expected_name = format!("ParticleDemo · {}", slot.label);
        let (want_pos, _, _) = particle_demo_label_pose(slot.config.origin);
        ids.iter().any(|id| {
            state.world.name(*id) == Some(expected_name.as_str())
                && state.world.get::<Transform>(*id).is_some_and(|transform| {
                    transform.position.distance(want_pos) < 0.05
                        && (transform.rotation * Vec3::Z).dot(Vec3::NEG_X) > 0.95
                })
        })
    })
}

/// Letreros de pie de la fila demo de partículas (FIRE/SMOKE/SAND/WATER/FOG).
pub fn ensure_particle_demo_labels(state: &mut State) {
    let slots = crate::engine3d::particles::particle_demo_slots();
    if particle_demo_labels_current(state, &slots) {
        return;
    }
    for id in particle_demo_label_ids(state) {
        despawn_validation_entity(state, id);
    }
    for slot in slots {
        spawn_particle_demo_label(state, slot.label, slot.accent, slot.config.origin);
    }
}

/// Escena comparativa: 1 esfera por material en fila, letrero arriba, pared entre cada una.
pub fn ensure_material_comparison_demo(state: &mut State) {
    if material_comparison_sphere_ids(state).len() != MAT_VAL_ROW_COUNT {
        spawn_material_comparison_scene(state);
        return;
    }
    sync_material_comparison_pbr(state);
}

/// Escena comparativa: 1 esfera por material en fila, letrero arriba, pared entre cada una.
pub fn spawn_material_comparison_scene(state: &mut State) {
    clear_validation_scene(state);
    clear_material_comparison(state);

    const SPHERE_RADIUS: f32 = 0.55;
    const SPHERE_Y: f32 = SPHERE_RADIUS + 0.05;
    const SPHERE_Z: f32 = -5.0;
    const LABEL_Y: f32 = SPHERE_Y + SPHERE_RADIUS + 0.1 + LABEL_H * 0.5;
    const LABEL_W: f32 = 2.0;
    const LABEL_H: f32 = 0.45;
    const WALL_Z: f32 = -4.0;
    const WALL_H: f32 = LABEL_Y + LABEL_H * 0.5;
    const WALL_D: f32 = 3.5;
    const WALL_T: f32 = 0.25;

    for i in 0..MAT_VAL_ROW_COUNT {
        let preset = &MATERIAL_VALIDATION_ROWS[i];
        let x = MAT_COMP_X_POSITIONS[i];
        let tex_idx = state.mat_val_texture_for_preset(preset);
        let roughness = preset.roughness_min;

        // ── Esfera ──
        let name = format!("MatComp · {}", preset.label);
        let id = state.world.spawn(Some(&name));
        state.apply_mat_val_sphere_visual(id, tex_idx);
        if let Some(t) = state.world.get_mut::<Transform>(id) {
            t.position = Vec3::new(x, SPHERE_Y, SPHERE_Z);
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
                path: MATERIAL_COMPARISON_PATH.to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(name),
            position: Some([x, SPHERE_Y, SPHERE_Z]),
            scale: Some([SPHERE_RADIUS * 2.0; 3]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some(MATERIAL_COMPARISON_PATH.to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(false),
            physics_type: None,
            entity_category: Some("object".to_string()),
        });

        // ── Letrero ──
        let label_name = format!("MatCompLabel · {}", preset.label);
        let label_id = state.world.spawn(Some(&label_name));
        let mesh_idx = state.mat_val_label_mesh_idx();
        let label_tex_idx = state.mat_val_texture_for_label(preset);
        state.world.insert(
            label_id,
            MeshComponent {
                mesh_idx,
                tex_idx: label_tex_idx,
            },
        );
        state.world.insert(label_id, NonSelectable);
        state.world.insert(
            label_id,
            SurfacePbr {
                roughness: 0.62,
                metallic: 0.0,
                ior: 0.0,
                opacity: 1.0,
            },
        );
        if let Some(t) = state.world.get_mut::<Transform>(label_id) {
            t.position = Vec3::new(x, LABEL_Y, SPHERE_Z);
            t.scale = Vec3::new(LABEL_W, LABEL_H, 1.0);
        }
        state.save_registry.register_meta(
            label_id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: MATERIAL_COMPARISON_PATH.to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        send_event(&EngineEvent::ModelLoaded {
            id: label_id,
            name: Some(label_name),
            position: Some([x, LABEL_Y, SPHERE_Z]),
            scale: Some([LABEL_W, LABEL_H, 1.0]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some(MATERIAL_COMPARISON_PATH.to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(false),
            physics_type: None,
            entity_category: Some("object".to_string()),
        });

        // ── Cuadro plano en el suelo ──
        let quad_name = format!("MatCompQuad · {}", preset.label);
        let quad_id = state.world.spawn(Some(&quad_name));
        let quad_mesh_idx = state.mat_val_label_mesh_idx();
        state.world.insert(
            quad_id,
            MeshComponent {
                mesh_idx: quad_mesh_idx,
                tex_idx,
            },
        );
        state.world.insert(quad_id, NonSelectable);
        state
            .world
            .insert(quad_id, surface_pbr_from_preset(preset, roughness));
        state
            .world
            .insert(quad_id, crate::engine3d::ecs::RenderLayer { value: 1 });
        if let Some(t) = state.world.get_mut::<Transform>(quad_id) {
            apply_mat_comp_quad_transform(t, x);
        }
        state.save_registry.register_meta(
            quad_id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: MATERIAL_COMPARISON_PATH.to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        send_event(&EngineEvent::ModelLoaded {
            id: quad_id,
            name: Some(quad_name),
            position: Some([x, 0.03, MAT_COMP_QUAD_Z]),
            scale: Some([MAT_COMP_QUAD_X_WIDTH, MAT_COMP_QUAD_Z_LENGTH, 1.0]),
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            path: Some(MATERIAL_COMPARISON_PATH.to_string()),
            kind: Some("model".to_string()),
            blueprint_id: None,
            physics_enabled: Some(false),
            physics_type: None,
            entity_category: Some("object".to_string()),
        });

        // ── Pared divisoria ──
        if i + 1 < MAT_VAL_ROW_COUNT {
            let wall_x = (MAT_COMP_X_POSITIONS[i] + MAT_COMP_X_POSITIONS[i + 1]) * 0.5;
            let wall_name = format!("MatCompWall · {}", preset.label);
            state.spawn_editor_box(
                &wall_name,
                [wall_x, WALL_H * 0.5, WALL_Z],
                [WALL_T, WALL_H, WALL_D],
            );
        }
    }

    // Paredes de los extremos
    let edge_wall_x = MAT_COMP_X_POSITIONS[0] - 1.5;
    state.spawn_editor_box(
        "MatCompWall · left",
        [edge_wall_x, WALL_H * 0.5, WALL_Z],
        [WALL_T, WALL_H, WALL_D],
    );
    let edge_wall_x = MAT_COMP_X_POSITIONS[MAT_VAL_ROW_COUNT - 1] + 1.5;
    state.spawn_editor_box(
        "MatCompWall · right",
        [edge_wall_x, WALL_H * 0.5, WALL_Z],
        [WALL_T, WALL_H, WALL_D],
    );
}

impl State {
    pub(crate) fn mat_val_label_mesh_idx(&mut self) -> usize {
        if let Some(idx) = self.mat_val_label_mesh_idx {
            return idx;
        }
        let idx = self.meshes.len();
        self.meshes
            .push(crate::engine3d::mesh::create_unit_quad_xy(&self.vulkan));
        self.mat_val_label_mesh_idx = Some(idx);
        idx
    }

    pub(crate) fn mat_val_texture_for_preset(&mut self, preset: &PbrMaterialPreset) -> usize {
        let key = format!("preset:v5:{}", preset.id);
        if let Some(&idx) = self.mat_val_texture_cache.get(&key) {
            return idx;
        }
        let rgba = build_preset_albedo_rgba(preset, TEX_SIZE);
        let tex_idx = self.tex_layers.len();
        let layer = self
            .texture_array
            .pack(&self.vulkan, &rgba, TEX_SIZE, TEX_SIZE)
            .unwrap();
        self.tex_layers.push(layer);
        self.mat_val_texture_cache.insert(key, tex_idx);
        tex_idx
    }

    pub(crate) fn mat_val_texture_for_label(&mut self, preset: &PbrMaterialPreset) -> usize {
        let key = format!("label:v3:{}", preset.id);
        if let Some(&idx) = self.mat_val_texture_cache.get(&key) {
            return idx;
        }
        let (rgba, w, h) = build_label_rgba(preset.label_tag, preset.base_color);
        let tex_idx = self.tex_layers.len();
        let layer = self.texture_array.pack(&self.vulkan, &rgba, w, h).unwrap();
        self.tex_layers.push(layer);
        self.mat_val_texture_cache.insert(key, tex_idx);
        tex_idx
    }

    pub(crate) fn texture_for_label_text(
        &mut self,
        cache_key: &str,
        text: &str,
        accent: [f32; 3],
    ) -> usize {
        if let Some(&idx) = self.mat_val_texture_cache.get(cache_key) {
            return idx;
        }
        let (rgba, w, h) = build_label_rgba(text, accent);
        let tex_idx = self.tex_layers.len();
        let layer = self.texture_array.pack(&self.vulkan, &rgba, w, h).unwrap();
        self.tex_layers.push(layer);
        self.mat_val_texture_cache
            .insert(cache_key.to_string(), tex_idx);
        tex_idx
    }

    pub(crate) fn apply_mat_val_sphere_visual(&mut self, id: EntityId, tex_idx: usize) {
        let mesh_idx = self.sun_icon_mesh_idx();
        if let Some(mc) = self.world.get_mut::<MeshComponent>(id) {
            mc.mesh_idx = mesh_idx;
            mc.tex_idx = tex_idx;
        } else {
            self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        }
    }
}

#[cfg(test)]
mod particle_demo_label_tests {
    use super::particle_demo_label_pose;
    use glam::Vec3;

    #[test]
    fn standing_labels_share_front_yaw() {
        let (a, rot_a, scale) = particle_demo_label_pose([10.0, 0.35, -1.0]);
        let (b, rot_b, _) = particle_demo_label_pose([10.0, 0.35, 7.0]);
        assert_eq!(rot_a, rot_b);
        assert!((scale.y - 0.45).abs() < 1e-5);
        let forward = rot_a * Vec3::Z;
        assert!(forward.dot(Vec3::NEG_X) > 0.99);
        assert!(forward.y.abs() < 1e-5);
        assert!((a.x - 8.3).abs() < 1e-4);
        assert!((b.x - a.x).abs() < 1e-5);
        assert!((a.z + 1.0).abs() < 1e-5);
        assert!((b.z - 7.0).abs() < 1e-5);
    }
}
