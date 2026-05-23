use glam::Vec3;

use crate::config_3d::character_anchor::{
    feet_from_transform, center_from_feet, body_center_from_feet,
    PLAY_CHARACTER_BODY_HEIGHT, PLAY_CHARACTER_COLLIDER_RADIUS,
};
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh;

impl State {
    fn play_character_capsule_params(&self) -> Option<(f32, f32, glam::Quat, Vec3)> {
        let id = self.play_character_entity?;
        let t = self.world.get::<Transform>(id)?;
        let radius = PLAY_CHARACTER_COLLIDER_RADIUS;
        let half_height =
            PhysicsWorld::capsule_half_height_from_scale(PLAY_CHARACTER_BODY_HEIGHT, radius);
        Some((radius, half_height, t.rotation, t.position))
    }

    fn play_character_exclude_collider(&self) -> Option<rapier3d::prelude::ColliderHandle> {
        self.play_character_entity
            .and_then(|id| self.physics.collider_handle_for_entity(id))
    }

    /// Posición de los pies del jugador (base de la cápsula de movimiento).
    pub(crate) fn play_character_feet_position(&self) -> Vec3 {
        if let Some((_, _, rot, center)) = self.play_character_capsule_params() {
            if self.is_play_controller_active() {
                return Self::controller_feet_from_center(center);
            }
            let scale_y = self
                .play_character_entity
                .and_then(|id| self.world.get::<Transform>(id))
                .map(|t| t.scale.y)
                .unwrap_or(PLAY_CHARACTER_BODY_HEIGHT);
            return feet_from_transform(center, scale_y, rot);
        }
        self.camera.target
    }

    pub(crate) fn set_play_character_feet_position(&mut self, feet: Vec3) {
        let in_play = self.is_play_controller_active();
        if let Some(id) = self.play_character_entity {
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                if in_play {
                    t.position = Self::center_from_controller_feet(feet);
                } else {
                    let scale_y = t.scale.y;
                    let rot = t.rotation;
                    t.position = center_from_feet(feet, scale_y, rot);
                }
            }
        }
        if self.is_play_controller_active() {
            self.camera.target = feet;
        }
    }

    /// Transform del jugador desde el panel Propiedades en editor: cuerpo solo, cámara intacta.
    pub(crate) fn apply_play_character_transform_editor(
        &mut self,
        id: EntityId,
        position: Option<[f32; 3]>,
        rotation: Option<glam::Quat>,
        scale: Option<glam::Vec3>,
    ) -> bool {
        let Some(before_t) = self.world.get::<Transform>(id).cloned() else {
            return false;
        };
        let new_rot = rotation.unwrap_or(before_t.rotation);
        let new_scale = scale.unwrap_or(before_t.scale);
        let feet = feet_from_transform(before_t.position, before_t.scale.y, before_t.rotation);

        let Some(t) = self.world.get_mut::<Transform>(id) else {
            return false;
        };

        let preserve_feet = rotation.is_some() || scale.is_some();
        if preserve_feet {
            let mut center = center_from_feet(feet, new_scale.y, new_rot);
            if let Some(p) = position {
                center += glam::Vec3::from_array(p) - before_t.position;
            }
            t.position = center;
            t.rotation = new_rot;
            t.scale = new_scale;
        } else if let Some(p) = position {
            t.position = glam::Vec3::from_array(p);
            t.rotation = new_rot;
            t.scale = new_scale;
        } else {
            t.rotation = new_rot;
            t.scale = new_scale;
        }

        // Editor: cuerpo independiente del blanco orbital (cámara = gizmo). Play acopla en set_play_character_feet_position.
        true
    }

    /// Alinea el mesh del jugador al yaw de la cámara (preview/play y panel Cámara).
    pub(crate) fn sync_player_rotation_from_look(&mut self) {
        let Some(id) = self.play_character_entity else {
            return;
        };
        let mesh_yaw = crate::config_3d::fps_camera::mesh_yaw_from_camera_and_forward(
            self.camera.yaw,
            self.play_character_mesh_forward_xz,
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            // Solo yaw: pitch/roll en el transform acostaban el mesh aunque la malla esté enderezada.
            t.rotation = glam::Quat::from_rotation_y(mesh_yaw);
        }
    }

    /// El jugador principal no debe tener cuerpo Rapier (solo cápsula cinemática por queries).
    pub(crate) fn ensure_play_character_kinematic_only(&mut self) {
        let Some(id) = self.play_character_entity else {
            return;
        };
        self.physics.remove_entity_body(id);
    }

    pub(crate) fn has_play_character(&self) -> bool {
        self.play_character_entity.is_some() && self.camera_2d.is_none()
    }

    /// Cuerpo placeholder del jugador (cubo). La posición del transform = centro del cuerpo.
    pub(crate) fn attach_play_character_body(&mut self, id: EntityId) {
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let body_px = [180u8, 200, 255, 255];
        let tex_idx = self.uv_rects.len();
        let body_uv = self.atlas.pack(&self.queue, &body_px, 1, 1);
        self.uv_rects.push(body_uv);

        self.world.insert(
            id,
            MeshComponent {
                mesh_idx,
                tex_idx,
            },
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            let w = PLAY_CHARACTER_COLLIDER_RADIUS * 2.0;
            t.scale = glam::Vec3::new(w, PLAY_CHARACTER_BODY_HEIGHT, w);
        }
    }

    pub(crate) fn setup_play_character_entity(&mut self, id: EntityId, feet: glam::Vec3) {
        self.character_entities.push(id);
        self.play_character_entity = Some(id);
        self.play_character_mesh_forward_xz = glam::Vec2::new(0.0, 1.0);
        self.attach_play_character_body(id);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = body_center_from_feet(feet);
        }
        if !self.is_play_controller_active() {
            if let Some(t) = self.world.get::<Transform>(id) {
                self.init_editor_viewport_for_player(t.position);
            }
            self.ensure_editor_camera_entity();
        }
        self.sync_player_rotation_from_look();
        self.sync_fps_camera_mode();
        self.ensure_play_character_kinematic_only();
        self.emit_play_character_view_changed(true);
    }

    pub(crate) fn spawn_play_character(&mut self) {
        let feet = self.camera.target;
        let id = self.world.spawn(Some("Player"));
        self.setup_play_character_entity(id, feet);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "character".to_string(),
                path: "[Player]".to_string(),
                visual_model_path: None,
                points: None,
            },
        );
        send_event(&EngineEvent::CharacterLoaded {
            id,
            path: "[Player]".to_string(),
        });
    }

    pub(crate) fn play_character_capsule_for_controller(
        &self,
    ) -> Option<(f32, f32, glam::Quat, Vec3)> {
        self.play_character_capsule_params()
    }

    pub(crate) fn play_character_exclude_collider_for_controller(
        &self,
    ) -> Option<rapier3d::prelude::ColliderHandle> {
        self.play_character_exclude_collider()
    }
}
