use glam::Vec3;

use crate::config_3d::character_anchor::{
    body_center_from_feet, center_from_feet, feet_from_transform,
    PlayCharacterMeshExtents, PLAY_CHARACTER_BODY_HEIGHT, PLAY_CHARACTER_COLLIDER_RADIUS,
};
use crate::config_3d::is_fbx_model_path;
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
        let radius = self.play_character_capsule_radius_world(t.scale);
        let body_h = self.play_character_body_height_world(t.scale.y);
        let half_height = PhysicsWorld::capsule_half_height_from_scale(body_h, radius);
        Some((radius, half_height, t.rotation, t.position))
    }

    pub(crate) fn play_character_exclude_collider(&self) -> Option<rapier3d::prelude::ColliderHandle> {
        self.play_character_entity
            .and_then(|id| self.physics.collider_handle_for_entity(id))
    }

    /// Posición de los pies del jugador (base de la cápsula de movimiento).
    fn play_character_transform_is_feet_pivot(&self) -> bool {
        self.play_character_mesh_extents.is_some()
    }

    pub(crate) fn play_character_feet_position(&self) -> Vec3 {
        if let Some((_, _, rot, anchor)) = self.play_character_capsule_params() {
            if self.play_character_transform_is_feet_pivot() {
                return anchor;
            }
            if self.is_play_controller_active() {
                return self.controller_feet_from_center(anchor);
            }
            let scale_y = self
                .play_character_entity
                .and_then(|id| self.world.get::<Transform>(id))
                .map(|t| t.scale.y)
                .unwrap_or(PLAY_CHARACTER_BODY_HEIGHT);
            return feet_from_transform(
                anchor,
                scale_y,
                rot,
                None,
            );
        }
        self.camera.target
    }

    pub(crate) fn set_play_character_feet_position(&mut self, feet: Vec3) {
        let Some(id) = self.play_character_entity else {
            return;
        };
        let feet_pivot = self.play_character_transform_is_feet_pivot();
        let in_play = self.is_play_controller_active();
        let center = if feet_pivot {
            feet
        } else if in_play {
            self.center_from_controller_feet(feet)
        } else {
            let (scale_y, rot) = self
                .world
                .get::<Transform>(id)
                .map(|t| (t.scale.y, t.rotation))
                .unwrap_or((PLAY_CHARACTER_BODY_HEIGHT, glam::Quat::IDENTITY));
            center_from_feet(feet, scale_y, rot, None)
        };
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = center;
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
        let feet_pivot = self.play_character_transform_is_feet_pivot();
        let feet = if feet_pivot {
            before_t.position
        } else {
            feet_from_transform(
                before_t.position,
                before_t.scale.y,
                before_t.rotation,
                None,
            )
        };

        let Some(t) = self.world.get_mut::<Transform>(id) else {
            return false;
        };

        let preserve_feet = rotation.is_some() || scale.is_some();
        if preserve_feet {
            let mut new_pos = if feet_pivot {
                feet
            } else {
                center_from_feet(feet, new_scale.y, new_rot, None)
            };
            if let Some(p) = position {
                new_pos += glam::Vec3::from_array(p) - before_t.position;
            }
            t.position = new_pos;
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

        let new_feet = self.play_character_feet_position();
        if (new_feet - feet).length_squared() > 1e-10 {
            self.sync_play_camera_on_player_feet_moved(feet, new_feet);
        }

        // Editor: cuerpo independiente del blanco orbital (cámara = gizmo). Play acopla en set_play_character_feet_position.
        true
    }

    pub(crate) fn capture_play_session_rotation_baselines(&mut self) {
        self.play_session_camera_yaw_baseline = self.camera.yaw;
        self.play_session_body_yaw_baseline = self
            .play_character_entity
            .and_then(|id| self.world.get::<Transform>(id))
            .map(|t| t.rotation.to_euler(glam::EulerRot::YXZ).0)
            .unwrap_or(0.0);
    }

    /// Solo en play: en editor la rotación del mesh viene del save / panel Transform.
    pub(crate) fn sync_play_character_body_rotation_after_mesh_assign(&mut self) {
        if self.is_play_controller_active() {
            self.sync_player_rotation_from_look();
        }
    }

    pub(crate) fn sync_player_rotation_from_look(&mut self) {
        let Some(id) = self.play_character_entity else {
            return;
        };
        let body_yaw = if self.is_play_controller_active() {
            self.play_session_body_yaw_baseline
                - (self.camera.yaw - self.play_session_camera_yaw_baseline)
        } else {
            crate::config_3d::fps_camera::mesh_yaw_from_camera_and_forward(
                self.camera.yaw,
                self.play_character_mesh_forward_xz,
            )
        };
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.rotation = glam::Quat::from_rotation_y(body_yaw);
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
        self.play_character_entity.is_some()
    }

    /// Registra el AABB de la malla para la cápsula de suelo y alinea `Transform.position` a los pies.
    /// No modifica escala, rotación ni `play_character_mesh_forward_xz`.
    /// Tras asignar la malla visual del jugador: FBX alinea pies al suelo; GLB/gltf conserva pivote centrado (main).
    pub(crate) fn apply_play_character_model_placement_after_load(
        &mut self,
        id: EntityId,
        model_path: &str,
        local_bounds: ([f32; 3], [f32; 3]),
    ) {
        let w = PLAY_CHARACTER_COLLIDER_RADIUS * 2.0;
        if is_fbx_model_path(model_path) {
            self.apply_play_character_mesh_ground_extents(id, local_bounds);
        } else {
            self.play_character_mesh_extents = None;
            let feet = self.play_character_feet_position();
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = glam::Vec3::new(
                    feet.x,
                    feet.y + PLAY_CHARACTER_BODY_HEIGHT * 0.5,
                    feet.z,
                );
            }
        }
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.scale = glam::Vec3::new(w, 1.0, w);
            let (_, _, yaw) = t.rotation.to_euler(glam::EulerRot::YXZ);
            t.rotation = glam::Quat::from_rotation_y(yaw);
        }
    }

    /// Solo FBX: AABB para cápsula de suelo y `Transform.position` en los pies.
    fn apply_play_character_mesh_ground_extents(
        &mut self,
        id: EntityId,
        local_bounds: ([f32; 3], [f32; 3]),
    ) {
        let feet = if let Some(t) = self.world.get::<Transform>(id) {
            if self.play_character_mesh_extents.is_some() {
                t.position
            } else if t.position.y >= PLAY_CHARACTER_BODY_HEIGHT * 0.4 {
                // Placeholder / centro del cuerpo antes del primer mesh FBX.
                feet_from_transform(t.position, t.scale.y, t.rotation, None)
            } else {
                // Ya son pies (p. ej. tras set_play_character_view al cargar .save).
                t.position
            }
        } else {
            self.play_character_feet_position()
        };
        self.play_character_mesh_extents =
            Some(PlayCharacterMeshExtents::from_local_bounds(local_bounds.0, local_bounds.1));
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = feet;
        }
    }

    pub(crate) fn attach_play_character_body(&mut self, id: EntityId) {
        self.play_character_mesh_extents = None;
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let body_px = [180u8, 200, 255, 255];
        let tex_idx = self.tex_layers.len();
        let body_layer = self.texture_array.pack(&self.queue, &body_px, 1, 1);
        self.tex_layers.push(body_layer);

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
        self.play_character_mesh_extents = None;
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
                entity_category: None,
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
