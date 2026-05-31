use glam::Vec3;

use crate::config_3d::character_anchor::{
    body_center_from_feet, center_from_feet, feet_from_transform, PlayCharacterMeshExtents,
    PLAY_CHARACTER_BODY_HEIGHT,
    PLAY_CHARACTER_COLLIDER_RADIUS,
};
use crate::entity_save_meta::is_model_3d_asset_path;
use crate::config_3d::model_asset;
use crate::config_3d::static_model_cache::play_character_cache_key;
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh;

impl State {
    /// AABB de hover/click alineado a la cápsula del jugador (pies + altura), no al cubo genérico.
    pub(crate) fn play_character_world_pick_aabb(&self) -> Option<(glam::Vec3, glam::Vec3)> {
        let id = self.play_character_entity?;
        let t = self.world.get::<Transform>(id)?;
        let feet = self.play_character_feet_position();
        let half_h = self.play_character_body_height_world(t.scale.y) * 0.5;
        let r = self.play_character_capsule_radius_world(t.scale);
        let center = glam::Vec3::new(feet.x, feet.y + half_h, feet.z);
        let half = glam::Vec3::new(r.max(0.15), half_h.max(0.01), r.max(0.15));
        Some((center, half))
    }

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

    /// `true` tras reemplazar el mesh por un archivo 3D (`.glb`/`.fbx`); el cubo `[Player]` no cuenta.
    pub(crate) fn play_character_uses_mesh_driven_capsule(&self) -> bool {
        self.play_character_mesh_extents.is_some()
    }

    /// Aplica colisión por AABB de malla al jugador solo cuando hay modelo 3D en disco.
    pub(crate) fn should_apply_play_character_mesh_collision(
        &self,
        id: EntityId,
        model_path: &str,
    ) -> bool {
        self.play_character_entity == Some(id) && is_model_3d_asset_path(model_path)
    }

    /// Origen de entidad en los pies (preview/FBX), no en el centro del AABB (glTF skinned centrado).
    pub(crate) fn play_character_mesh_origin_at_feet(&self) -> bool {
        self.play_character_mesh_extents
            .is_some_and(|e| e.origin_at_feet())
    }

    fn play_character_transform_scale_rot(&self) -> (glam::Vec3, glam::Quat) {
        self.play_character_entity
            .and_then(|id| self.world.get::<Transform>(id))
            .map(|t| (t.scale, t.rotation))
            .unwrap_or((glam::Vec3::ONE, glam::Quat::IDENTITY))
    }

    pub(crate) fn play_character_feet_position(&self) -> Vec3 {
        if let Some((_, _, rot, anchor)) = self.play_character_capsule_params() {
            if let Some(ext) = self.play_character_mesh_extents {
                if !ext.origin_at_feet() {
                    let (scale, _) = self.play_character_transform_scale_rot();
                    return feet_from_transform(anchor, scale, rot, Some(&ext));
                }
                return anchor;
            }
            if self.is_play_controller_active() {
                return self.controller_feet_from_center(anchor);
            }
            let (scale, rot) = self.play_character_transform_scale_rot();
            return feet_from_transform(anchor, scale, rot, None);
        }
        self.camera.target
    }

    pub(crate) fn set_play_character_feet_position(&mut self, feet: Vec3) {
        let Some(id) = self.play_character_entity else {
            return;
        };
        let in_play = self.is_play_controller_active();
        let (scale, rot) = self.play_character_transform_scale_rot();
        let center = if self.play_character_mesh_origin_at_feet() {
            feet
        } else if let Some(ext) = self.play_character_mesh_extents {
            center_from_feet(feet, scale, rot, Some(&ext))
        } else if in_play {
            self.center_from_controller_feet(feet)
        } else {
            center_from_feet(feet, scale, rot, None)
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
        let feet = self.play_character_feet_position();
        let feet_origin = self.play_character_mesh_origin_at_feet();
        let mesh_ext = self.play_character_mesh_extents;

        let Some(t) = self.world.get_mut::<Transform>(id) else {
            return false;
        };

        let preserve_feet = rotation.is_some() || scale.is_some();
        if preserve_feet {
            let mut new_pos = if feet_origin {
                feet
            } else if let Some(ext) = mesh_ext {
                center_from_feet(feet, new_scale, new_rot, Some(&ext))
            } else {
                center_from_feet(feet, new_scale, new_rot, None)
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

    /// Estado `common` del YAML al crear el placeholder (sin Rapier en el jugador).
    pub(crate) fn init_play_character_common_state(&mut self, id: EntityId) {
        self.entity_colision.insert(id, true);
        self.ensure_play_character_kinematic_only();
        if let Some(m) = self.save_registry.meta.get_mut(&id) {
            m.entity_category = Some("player".to_string());
        }
    }

    pub(crate) fn has_play_character(&self) -> bool {
        self.play_character_entity.is_some()
    }

    /// AABB de colisión del mesh visual (runtime o caché `::play_character`).
    pub(crate) fn play_character_mesh_extents_from_visual_path(
        &self,
        visual_path: &str,
    ) -> Option<PlayCharacterMeshExtents> {
        if let Some(e) = self.play_character_mesh_extents {
            return Some(e);
        }
        let asset_key = self.model_path_key(visual_path);
        let play_key = play_character_cache_key(&asset_key);
        let part = self
            .static_model_cache
            .get(&play_key)
            .and_then(|parts| parts.first())
            .or_else(|| {
                self.static_model_cache
                    .get(&asset_key)
                    .and_then(|parts| parts.first())
            })?;
        Some(PlayCharacterMeshExtents::from_local_bounds(
            part.local_bounds.0,
            part.local_bounds.1,
        ))
    }

    /// Escala Y y extents desde bounds visuales para que el mesh mida 1.7 m en mundo.
    pub(crate) fn sync_play_character_scale_to_body_height(
        &mut self,
        id: EntityId,
        model_path: &str,
    ) {
        let Some((min, max)) = self.play_character_visual_local_bounds(model_path) else {
            return;
        };
        let ext = PlayCharacterMeshExtents::from_local_bounds(min, max);
        self.play_character_mesh_extents = Some(ext);
        let h = ext.height().max(0.01);
        let w = PLAY_CHARACTER_COLLIDER_RADIUS * 2.0;
        let scale_y = PLAY_CHARACTER_BODY_HEIGHT / h;
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.scale = glam::Vec3::new(w, scale_y, w);
        }
    }

    /// Registra altura de la malla (pies en Y=0) y alinea `Transform.position` a los pies.
    pub(crate) fn apply_play_character_model_placement_after_load(
        &mut self,
        id: EntityId,
        model_path: &str,
        local_bounds: ([f32; 3], [f32; 3]),
    ) {
        let bounds = self
            .play_character_visual_local_bounds(model_path)
            .unwrap_or(local_bounds);
        if self.play_character_restore_in_progress {
            self.play_character_mesh_extents =
                Some(PlayCharacterMeshExtents::from_local_bounds(bounds.0, bounds.1));
            return;
        }
        self.sync_play_character_scale_to_body_height(id, model_path);
        self.apply_play_character_mesh_ground_extents(id, bounds);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            let (_, _, yaw) = t.rotation.to_euler(glam::EulerRot::YXZ);
            t.rotation = glam::Quat::from_rotation_y(yaw);
        }
    }

    /// AABB local de la malla que se dibuja (skinned si hay animación, si no caché `::play_character`).
    pub(crate) fn play_character_visual_local_bounds(
        &self,
        model_path: &str,
    ) -> Option<([f32; 3], [f32; 3])> {
        let key = self.model_path_key(model_path);
        if let Some(asset) = self.model_assets.get(&key) {
            if let Some(b) = model_asset::model_asset_play_character_visual_bounds(asset) {
                return Some(b);
            }
        }
        let play_key = play_character_cache_key(&key);
        self.static_model_cache
            .get(&play_key)
            .or_else(|| self.static_model_cache.get(&key))
            .and_then(|parts| parts.first())
            .map(|p| p.local_bounds)
    }

    /// Tras `replace_entity_model` / bind de animación: alinea transform, cámara y notifica al front.
    pub(crate) fn finish_play_character_model_replace(&mut self, id: EntityId, model_path: &str) {
        if self.play_character_restore_in_progress {
            if let Some((min, max)) = self.play_character_visual_local_bounds(model_path) {
                self.play_character_mesh_extents =
                    Some(PlayCharacterMeshExtents::from_local_bounds(min, max));
            }
            self.emit_play_character_view_changed(true);
            return;
        }
        self.sync_play_character_scale_to_body_height(id, model_path);
        self.align_play_character_transform_to_visual_mesh(id, model_path);
        self.emit_play_character_view_changed(true);
    }

    /// Alinea `Transform.position` (origen de entidad) para que los pies de la malla toquen el suelo.
    pub(crate) fn align_play_character_transform_to_visual_mesh(
        &mut self,
        id: EntityId,
        model_path: &str,
    ) {
        let Some((min, max)) = self.play_character_visual_local_bounds(model_path) else {
            return;
        };
        let extents = PlayCharacterMeshExtents::from_local_bounds(min, max);
        self.play_character_mesh_extents = Some(extents);

        let Some(t) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let world_feet_mesh = t.position + extents.feet_world_offset(t.scale, t.rotation);
        let world_feet = self.snap_play_character_feet_to_ground(world_feet_mesh);
        let new_position = world_feet - extents.feet_world_offset(t.scale, t.rotation);

        let old_feet = self.play_character_feet_position();
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = new_position;
        }
        let new_feet = self.play_character_feet_position();
        if (new_feet - old_feet).length_squared() > 1e-10 {
            self.sync_play_camera_on_player_feet_moved(old_feet, new_feet);
        }
        self.camera.target = new_feet;
        if !self.is_play_controller_active() {
            let body_h = self.play_character_visual_world_height();
            let body_center = new_feet + glam::Vec3::new(0.0, body_h * 0.5, 0.0);
            self.init_editor_viewport_for_player(body_center);
        }
        self.play_camera_eye_position = new_feet + self.play_character_eye_world_offset();
    }

    /// Solo FBX: AABB para cápsula de suelo y `Transform.position` en los pies.
    fn apply_play_character_mesh_ground_extents(
        &mut self,
        id: EntityId,
        local_bounds: ([f32; 3], [f32; 3]),
    ) {
        let feet = if let Some(t) = self.world.get::<Transform>(id) {
            if self.play_character_mesh_extents.is_some() {
                self.play_character_feet_position()
            } else if t.position.y >= PLAY_CHARACTER_BODY_HEIGHT * 0.4 {
                feet_from_transform(t.position, t.scale, t.rotation, None)
            } else {
                t.position
            }
        } else {
            self.play_character_feet_position()
        };
        self.play_character_mesh_extents =
            Some(PlayCharacterMeshExtents::from_local_bounds(local_bounds.0, local_bounds.1));
        let feet = self.snap_play_character_feet_to_ground(feet);
        let extents = PlayCharacterMeshExtents::from_local_bounds(local_bounds.0, local_bounds.1);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = if extents.origin_at_feet() {
                feet
            } else {
                center_from_feet(feet, t.scale, t.rotation, Some(&extents))
            };
        }
        self.camera.target = feet;
    }

    /// Ajusta los pies al suelo estático (mesh collider del terreno), sin usar la malla visual del jugador.
    fn snap_play_character_feet_to_ground(&mut self, feet: glam::Vec3) -> glam::Vec3 {
        let mut out = feet;
        if let Some(ground_y) = self.physics.find_ground_y_at(out.x, out.z, out.y + 4.0, 12.0) {
            out.y = ground_y;
        }
        out
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
        self.init_play_character_common_state(id);
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
                entity_category: Some("player".to_string()),
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

    /// Pies + radio uniforme + altura total (píldora 1.7 m normalizada).
    pub(crate) fn play_character_capsule_wire_dims(&self) -> Option<(Vec3, f32, f32)> {
        let cap = self.play_character_collision_capsule();
        Some((self.play_character_feet_position(), cap.radius, cap.height))
    }

    pub(crate) fn play_character_exclude_collider_for_controller(
        &self,
    ) -> Option<rapier3d::prelude::ColliderHandle> {
        self.play_character_exclude_collider()
    }
}
