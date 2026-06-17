//! Física secundaria por hueso (jiggle) — spring-damper sobre la pose animada.

use glam::{EulerRot, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

use crate::config_3d::entity_sockets::resolve_joint_index;
use crate::config_3d::model_asset::ModelAsset;
use crate::ecs::{EntityId, Transform};
use crate::engine::UndoAction;
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

/// Modo persistido por hueso (`.save` + IPC).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BonePhysicsMode {
    None,
    Inherit,
    Static,
    Dynamic,
    Kinematic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BonePhysicsEntry {
    pub bone_name: String,
    pub mode: BonePhysicsMode,
}

pub type BonePhysicsSnapshot = BonePhysicsEntry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectiveBonePhysics {
    Off,
    Static,
    Dynamic,
    Kinematic,
}

#[derive(Clone, Debug)]
pub(crate) struct BonePhysicsSimState {
    pub rot_offset: Quat,
    pub ang_vel: Vec3,
    pub prev_anim_rot: Quat,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BonePhysicsEntityMotion {
    pub prev_position: Vec3,
    pub prev_velocity: Vec3,
    pub prev_rotation: Quat,
    pub initialized: bool,
}

const SPRING_STIFFNESS: f32 = 140.0;
const SPRING_DAMPING: f32 = 16.0;
const KINEMATIC_STIFFNESS: f32 = 90.0;
const KINEMATIC_DAMPING: f32 = 14.0;
const GRAVITY_TILT: f32 = 3.2;
const IMPULSE_SCALE: f32 = 18.0;
/// Impulso por aceleración lineal del host (salto, frenada, arranque).
const ENTITY_ACCEL_IMPULSE: f32 = 7.5;
/// Arrastre por velocidad lineal (inercia al moverse).
const ENTITY_VEL_DRAG: f32 = 1.4;
/// Impulso por giro del host.
const ENTITY_ANGULAR_IMPULSE: f32 = 5.0;

pub(crate) fn parse_bone_physics_mode(raw: &str) -> Option<BonePhysicsMode> {
    match raw.trim().to_lowercase().as_str() {
        "none" => Some(BonePhysicsMode::None),
        "inherit" => Some(BonePhysicsMode::Inherit),
        "static" => Some(BonePhysicsMode::Static),
        "dynamic" => Some(BonePhysicsMode::Dynamic),
        "kinematic" => Some(BonePhysicsMode::Kinematic),
        _ => None,
    }
}

pub(crate) fn resolve_effective_bone_physics(
    mode: BonePhysicsMode,
    entity_physics_type: &str,
) -> EffectiveBonePhysics {
    let resolved = match mode {
        BonePhysicsMode::None => return EffectiveBonePhysics::Off,
        BonePhysicsMode::Inherit => entity_physics_type,
        BonePhysicsMode::Static => "static",
        BonePhysicsMode::Dynamic => "dynamic",
        BonePhysicsMode::Kinematic => "kinematic",
    };
    match resolved.trim().to_lowercase().as_str() {
        "static" | "" | "none" => EffectiveBonePhysics::Static,
        "kinematic" => EffectiveBonePhysics::Kinematic,
        _ => EffectiveBonePhysics::Dynamic,
    }
}

impl State {
    pub(crate) fn entity_physics_type_label(&self, entity_id: EntityId) -> String {
        if self.physics.has_physics(entity_id) {
            self.physics.get_body_type(entity_id).to_string()
        } else {
            "none".to_string()
        }
    }

    pub(crate) fn list_entity_bone_physics(
        &self,
        entity_id: EntityId,
    ) -> Vec<BonePhysicsSnapshot> {
        self.entity_bone_physics
            .get(&entity_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.mode != BonePhysicsMode::None)
            .map(|e| BonePhysicsSnapshot {
                bone_name: e.bone_name,
                mode: e.mode,
            })
            .collect()
    }

    pub(crate) fn set_bone_physics(
        &mut self,
        entity_id: EntityId,
        bone_name: &str,
        mode: BonePhysicsMode,
    ) -> Result<(), String> {
        let bone_name = bone_name.trim();
        if bone_name.is_empty() {
            return Err("Nombre de hueso vacío.".to_string());
        }
        if !self.entity_has_skinned_model(entity_id) {
            return Err("La entidad no tiene modelo skinned.".to_string());
        }

        let previous = self.current_bone_physics_mode(entity_id, bone_name);

        if mode == BonePhysicsMode::None {
            self.remove_bone_physics_entry(entity_id, bone_name);
        } else {
            let _ = self.set_bone_physics_no_undo(entity_id, bone_name, mode)?;
        }

        if !self.is_applying_undo {
            self.redo_stack.clear();
            self.undo_stack
                .push(UndoAction::RestoreBonePhysics {
                    entity_id,
                    bone_name: bone_name.to_string(),
                    before: previous,
                    after: if mode == BonePhysicsMode::None {
                        None
                    } else {
                        Some(mode)
                    },
                });
            self.sync_editor_scenes_undo_dirty_to_renderer();
        }

        self.emit_entity_bone_physics_changed(entity_id);
        Ok(())
    }

    pub(crate) fn remove_bone_physics(&mut self, entity_id: EntityId, bone_name: &str) {
        let _ = self.set_bone_physics(entity_id, bone_name, BonePhysicsMode::None);
    }

    pub(crate) fn remove_bone_physics_entry(&mut self, entity_id: EntityId, bone_name: &str) {
        if let Some(list) = self.entity_bone_physics.get_mut(&entity_id) {
            list.retain(|e| !e.bone_name.eq_ignore_ascii_case(bone_name));
            if list.is_empty() {
                self.entity_bone_physics.remove(&entity_id);
                self.bone_physics_entity_motion.remove(&entity_id);
            }
        }
        self.clear_bone_physics_sim_for_entity_bone(entity_id, bone_name);
    }

    pub(crate) fn clear_bone_physics_sim_for_entity_bone(&mut self, entity_id: EntityId, bone_name: &str) {
        let Some(binding) = self.model_animation_bindings.get(&entity_id) else {
            return;
        };
        let Some(asset) = self.get_model_asset_for_entity(&binding.asset_path, entity_id) else {
            return;
        };
        if let Some(ji) = resolve_joint_index(&asset, bone_name) {
            self.bone_physics_sim.remove(&(entity_id, ji));
        }
    }

    pub(crate) fn restore_entity_bone_physics_from_saved(
        &mut self,
        entity_id: EntityId,
        entries: &[BonePhysicsSnapshot],
    ) {
        let list: Vec<BonePhysicsEntry> = entries
            .iter()
            .filter(|e| e.mode != BonePhysicsMode::None)
            .cloned()
            .map(|e| BonePhysicsEntry {
                bone_name: e.bone_name,
                mode: e.mode,
            })
            .collect();
        if list.is_empty() {
            self.entity_bone_physics.remove(&entity_id);
        } else {
            self.entity_bone_physics.insert(entity_id, list);
        }
        self.bone_physics_sim
            .retain(|(eid, _), _| *eid != entity_id);
        self.bone_physics_entity_motion.remove(&entity_id);
    }

    pub(crate) fn emit_entity_bone_physics_changed(&self, entity_id: EntityId) {
        let entries = self.list_entity_bone_physics(entity_id);
        send_event(&EngineEvent::EntityBonePhysicsChanged {
            entity_id,
            entries,
        });
    }

    pub(crate) fn emit_entity_bone_physics_if_any(&self, entity_id: EntityId) {
        if self.entity_bone_physics.contains_key(&entity_id) {
            self.emit_entity_bone_physics_changed(entity_id);
        }
    }

    pub(crate) fn bone_physics_joint_indices(&self, entity_id: EntityId) -> Vec<usize> {
        let Some(binding) = self.model_animation_bindings.get(&entity_id) else {
            return Vec::new();
        };
        let Some(asset) = self.get_model_asset_for_entity(&binding.asset_path, entity_id) else {
            return Vec::new();
        };
        let Some(entries) = self.entity_bone_physics.get(&entity_id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries {
            if let Some(ji) = resolve_joint_index(&asset, &entry.bone_name) {
                out.push(ji);
            }
        }
        out
    }

    pub(crate) fn apply_bone_physics_to_locals(
        &mut self,
        entity_id: EntityId,
        asset: &ModelAsset,
        dt: f32,
        locals: &mut [Mat4],
    ) {
        let Some(entries) = self.entity_bone_physics.get(&entity_id).cloned() else {
            return;
        };
        if entries.is_empty() {
            return;
        }

        let entity_physics = self.entity_physics_type_label(entity_id);
        let dt = dt.clamp(1.0e-5, 0.05);
        let entity_motion_impulse = self.sample_bone_physics_entity_motion(entity_id, dt);

        for entry in entries {
            let Some(ji) = resolve_joint_index(asset, &entry.bone_name) else {
                continue;
            };
            if ji >= locals.len() {
                continue;
            }

            let effective = resolve_effective_bone_physics(entry.mode, &entity_physics);
            let key = (entity_id, ji);

            let (scale, anim_rot, translation) = locals[ji].to_scale_rotation_translation();

            match effective {
                EffectiveBonePhysics::Off | EffectiveBonePhysics::Static => {
                    self.bone_physics_sim.remove(&key);
                    continue;
                }
                EffectiveBonePhysics::Dynamic | EffectiveBonePhysics::Kinematic => {
                    let sim = self.bone_physics_sim.entry(key).or_insert(BonePhysicsSimState {
                        rot_offset: Quat::IDENTITY,
                        ang_vel: Vec3::ZERO,
                        prev_anim_rot: anim_rot,
                    });

                    let stiffness = if effective == EffectiveBonePhysics::Dynamic {
                        SPRING_STIFFNESS
                    } else {
                        KINEMATIC_STIFFNESS
                    };
                    let damping = if effective == EffectiveBonePhysics::Dynamic {
                        SPRING_DAMPING
                    } else {
                        KINEMATIC_DAMPING
                    };

                    let delta_rot = anim_rot * sim.prev_anim_rot.inverse();
                    sim.prev_anim_rot = anim_rot;
                    let (dx, dy, dz) = delta_rot.to_euler(EulerRot::XYZ);
                    sim.ang_vel += Vec3::new(dx, dy, dz) * IMPULSE_SCALE;
                    sim.ang_vel += entity_motion_impulse;

                    let (rx, ry, rz) = sim.rot_offset.to_euler(EulerRot::XYZ);
                    let mut accel =
                        Vec3::new(-rx, -ry, -rz) * stiffness - sim.ang_vel * damping;

                    if effective == EffectiveBonePhysics::Dynamic {
                        let g = self.physics.gravity_magnitude().max(0.01) / 15.0;
                        accel.x += -GRAVITY_TILT * g;
                    }

                    sim.ang_vel += accel * dt;
                    let new_rx = rx + sim.ang_vel.x * dt;
                    let new_ry = ry + sim.ang_vel.y * dt;
                    let new_rz = rz + sim.ang_vel.z * dt;
                    sim.rot_offset =
                        Quat::from_euler(EulerRot::XYZ, new_rx, new_ry, new_rz).normalize();

                    locals[ji] = Mat4::from_scale_rotation_translation(
                        scale,
                        (anim_rot * sim.rot_offset).normalize(),
                        translation,
                    );
                }
            }
        }
    }

    /// Impulso angular por movimiento del host en espacio local (no depende del clip).
    fn sample_bone_physics_entity_motion(&mut self, entity_id: EntityId, dt: f32) -> Vec3 {
        let Some(t) = self.world.get::<Transform>(entity_id) else {
            return Vec3::ZERO;
        };
        let pos = t.position;
        let rot = t.rotation.normalize();

        let motion = self
            .bone_physics_entity_motion
            .entry(entity_id)
            .or_insert(BonePhysicsEntityMotion {
                prev_position: pos,
                prev_velocity: Vec3::ZERO,
                prev_rotation: rot,
                initialized: false,
            });

        if !motion.initialized {
            motion.prev_position = pos;
            motion.prev_velocity = Vec3::ZERO;
            motion.prev_rotation = rot;
            motion.initialized = true;
            return Vec3::ZERO;
        }

        let velocity = (pos - motion.prev_position) / dt;
        let accel = (velocity - motion.prev_velocity) / dt;
        let delta_rot = rot * motion.prev_rotation.inverse();
        let (arx, ary, arz) = delta_rot.to_euler(EulerRot::XYZ);
        let entity_ang_vel = Vec3::new(arx, ary, arz) / dt;

        motion.prev_position = pos;
        motion.prev_velocity = velocity;
        motion.prev_rotation = rot;

        let inv_rot = rot.inverse();
        let local_accel = inv_rot * accel;
        let local_vel = inv_rot * velocity;

        Vec3::new(
            (-local_accel.z - local_vel.z * ENTITY_VEL_DRAG) * ENTITY_ACCEL_IMPULSE
                + (-entity_ang_vel.y) * ENTITY_ANGULAR_IMPULSE,
            (-local_accel.y - local_vel.y * ENTITY_VEL_DRAG * 0.5) * ENTITY_ACCEL_IMPULSE * 0.85
                + entity_ang_vel.x * ENTITY_ANGULAR_IMPULSE * 0.35,
            (local_accel.x + local_vel.x * ENTITY_VEL_DRAG) * ENTITY_ACCEL_IMPULSE
                + entity_ang_vel.y * ENTITY_ANGULAR_IMPULSE,
        )
    }
}
