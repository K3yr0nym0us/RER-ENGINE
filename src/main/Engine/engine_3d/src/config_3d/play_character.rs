use std::path::Path;

use glam::Vec3;

use crate::config_3d::character_anchor::{
    body_center_from_feet, center_from_feet, feet_from_transform, PlayCharacterMeshExtents,
    PLAY_CHARACTER_BODY_HEIGHT,
    PLAY_CHARACTER_COLLIDER_RADIUS,
};
use crate::config_3d::model_asset;
use crate::config_3d::{is_fbx_model_path, is_gltf_model_path};
use crate::config_3d::static_model_cache::play_character_cache_key;
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::ecs::{EntityId, MeshComponent, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{send_event, EngineEvent};
use crate::mesh;

impl State {
    /// Punto delante del jugador a altura de torso (colocación de reflection probes).
    pub(crate) fn default_reflection_probe_spawn_position(&self) -> [f32; 3] {
        const FORWARD_M: f32 = 2.0;

        let feet = self.play_character_feet_position();
        let half_h = self
            .play_character_entity
            .and_then(|id| self.world.get::<Transform>(id))
            .map(|t| self.play_character_body_height_world(t.scale.y) * 0.5)
            .unwrap_or(PLAY_CHARACTER_BODY_HEIGHT * 0.5);
        let body_center = Vec3::new(feet.x, feet.y + half_h, feet.z);
        let yaw = self.play_character_body_yaw();
        let (sy, cy) = yaw.sin_cos();
        let forward = Vec3::new(-sy, 0.0, cy);
        (body_center + forward * FORWARD_M).to_array()
    }

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

    /// Colliders que la cápsula de movimiento y la cámara FP no deben tratar como obstáculo
    /// (jugador + accesorios fusionados / en socket bajo el jugador, incl. jerarquía).
    pub(crate) fn play_character_movement_excluded_colliders(
        &self,
    ) -> Vec<rapier3d::prelude::ColliderHandle> {
        use std::collections::HashSet;

        let mut out = Vec::new();
        let Some(player_id) = self.play_character_entity else {
            return out;
        };
        if let Some(handle) = self.physics.collider_handle_for_entity(player_id) {
            out.push(handle);
        }

        let mut stack = vec![player_id];
        let mut visited = HashSet::from([player_id]);
        while let Some(parent_id) = stack.pop() {
            for (child_id, attachment) in &self.entity_attachments {
                let linked = match &attachment.anchor {
                    crate::config_3d::entity_attachments::AttachmentAnchor::Entity(id) => {
                        *id == parent_id
                    }
                    crate::config_3d::entity_attachments::AttachmentAnchor::Socket {
                        host_entity_id,
                        ..
                    } => *host_entity_id == parent_id,
                };
                if !linked || !visited.insert(*child_id) {
                    continue;
                }
                if let Some(handle) = self.physics.collider_handle_for_entity(*child_id) {
                    out.push(handle);
                }
                stack.push(*child_id);
            }
        }
        out
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
        self.play_character_entity == Some(id)
            && crate::entity_save_meta::is_play_character_visual_model_path(model_path)
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
        self.sync_attached_children_of(id);
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
            self.sync_attached_children_of(id);
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
        let key = self.model_cache_key(model_path);
        let play_asset_key =
            crate::config_3d::static_model_cache::play_character_cache_key(&key);
        if let Some(asset) = self.model_assets.get(&play_asset_key) {
            if let Some(b) = model_asset::model_asset_play_character_visual_bounds(asset) {
                return Some(b);
            }
        }
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
        self.sync_play_character_scale_to_body_height(id, model_path);
        self.align_play_character_transform_to_visual_mesh(id, model_path);
        self.emit_play_character_view_changed(true);
    }

    /// Bounds para colocación: bind pose skinned (lo que dibuja el motor), no solo caché estática.
    fn play_character_placement_bounds(&self, model_path: &str) -> Option<([f32; 3], [f32; 3])> {
        self.play_character_visual_local_bounds(model_path).or_else(|| {
            let cache_key = self.model_cache_key(model_path);
            let play_key = play_character_cache_key(&cache_key);
            self.static_model_cache
                .get(&play_key)
                .and_then(|parts| parts.first())
                .map(|p| p.local_bounds)
        })
    }

    /// Fija `Transform.position` según pies en mundo y AABB local (pivote malla normalizada).
    pub(crate) fn place_play_character_at_world_feet_with_bounds(
        &mut self,
        id: EntityId,
        local_bounds: ([f32; 3], [f32; 3]),
        world_feet: Vec3,
        snap_to_ground: bool,
    ) {
        let extents = PlayCharacterMeshExtents::from_local_bounds(local_bounds.0, local_bounds.1);
        self.play_character_mesh_extents = Some(extents);

        let Some(t) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let world_feet = if snap_to_ground {
            self.snap_play_character_feet_to_ground(world_feet)
        } else {
            world_feet
        };
        // Misma fórmula que `align_play_character_transform_to_visual_mesh` (skinned usa este pivote).
        let new_position = world_feet - extents.feet_world_offset(t.scale, t.rotation);

        let old_feet = self.play_character_feet_position();
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = new_position;
        }
        let new_feet = self.play_character_feet_position();
        if (new_feet - old_feet).length_squared() > 1e-10 {
            self.sync_play_camera_on_player_feet_moved(old_feet, new_feet);
            self.sync_attached_children_of(id);
        }
        self.camera.target = new_feet;
        if snap_to_ground {
            if !self.is_play_controller_active() {
                let body_h = self.play_character_visual_world_height();
                let body_center = new_feet + glam::Vec3::new(0.0, body_h * 0.5, 0.0);
                self.init_editor_viewport_for_player(body_center);
            }
            self.play_camera_eye_position = new_feet + self.play_character_eye_world_offset();
        }
    }

    /// Colocación en carga `.save` (bounds visuales / skinned, sin snap al suelo).
    pub(crate) fn place_play_character_at_world_feet(
        &mut self,
        id: EntityId,
        model_path: &str,
        world_feet: Vec3,
        snap_to_ground: bool,
    ) {
        let Some(bounds) = self.play_character_placement_bounds(model_path) else {
            self.set_play_character_feet_position(world_feet);
            self.camera.target = world_feet;
            return;
        };
        self.place_play_character_at_world_feet_with_bounds(id, bounds, world_feet, snap_to_ground);
    }

    /// Alinea `Transform.position` (origen de entidad) para que los pies de la malla toquen el suelo.
    pub(crate) fn align_play_character_transform_to_visual_mesh(
        &mut self,
        id: EntityId,
        model_path: &str,
    ) {
        let Some(bounds) = self.play_character_visual_local_bounds(model_path) else {
            return;
        };
        let extents = PlayCharacterMeshExtents::from_local_bounds(bounds.0, bounds.1);
        let Some(t) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let world_feet_mesh = t.position + extents.feet_world_offset(t.scale, t.rotation);
        self.place_play_character_at_world_feet_with_bounds(id, bounds, world_feet_mesh, true);
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

    /// Crea o reutiliza entidad jugador sin `setup_play_character_entity` (sin órbita/rotación por defecto).
    pub(crate) fn ensure_play_character_shell(
        &mut self,
        display_name: &str,
        marker_path: &str,
        visual_model_path: Option<&str>,
        desired_id: Option<EntityId>,
    ) -> EntityId {
        let desired = desired_id.filter(|&d| d != 0);
        if let Some(id) = self.play_character_entity {
            if self.world.get::<Transform>(id).is_some() {
                // En carga de .save hay que respetar el id del manifest para que
                // `attach_parent_id` de accesorios fusionados coincida con el jugador.
                let reuse_existing = match desired {
                    Some(wanted) if wanted != id && self.restoring_save_manifest => {
                        log::warn!(
                            "[restore] jugador runtime {id} ≠ id guardado {wanted}; recreando shell"
                        );
                        self.physics.remove_entity_body(id);
                        self.clear_entity_attachments_for_removed(id);
                        self.save_registry.remove_entity(id);
                        self.world.despawn(id);
                        self.character_entities.retain(|&e| e != id);
                        self.scenario_entities.retain(|&e| e != id);
                        self.play_character_entity = None;
                        false
                    }
                    _ => true,
                };
                if reuse_existing {
                    if !self.character_entities.contains(&id) {
                        self.character_entities.push(id);
                    }
                    if !self.scenario_entities.contains(&id) {
                        self.scenario_entities.push(id);
                    }
                    return id;
                }
            }
        }
        let label = if display_name.trim().is_empty() {
            "Player"
        } else {
            display_name
        };
        let id = if let Some(desired) = desired {
            if self.world.spawn_with_id(desired, Some(label)) {
                desired
            } else {
                log::warn!("[restore] id jugador guardado {desired} en uso; generando id nuevo");
                self.world.spawn(Some(label))
            }
        } else {
            self.world.spawn(Some(label))
        };
        log::info!("[restore] jugador creado id={id}");
        self.character_entities.push(id);
        self.scenario_entities.push(id);
        self.play_character_entity = Some(id);
        self.play_character_mesh_forward_xz = glam::Vec2::new(0.0, 1.0);
        self.play_character_mesh_extents = None;
        self.attach_play_character_body(id);
        self.init_play_character_common_state(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "character".to_string(),
                path: marker_path.to_string(),
                visual_model_path: visual_model_path.map(str::to_string),
                entity_category: Some("player".to_string()),
            },
        );
        id
    }

    /// Instala mesh jugador desde caché `::play_character` (.rerasset) sin auto-escala ni alineación de editor.
    pub(crate) fn install_play_character_visual_from_path(
        &mut self,
        id: EntityId,
        path: &str,
    ) -> Result<String, String> {
        let cache_key = self.model_cache_key(path);
        let library_path = self.model_library_path_for(path);
        let source_for_ext = self
            .imported_model_registry
            .get(&cache_key)
            .map(|e| e.source_path.clone())
            .unwrap_or_else(|| library_path.clone());

        self.ensure_play_character_model_cached(path)?;
        self.ensure_play_character_model_assets_cached(&cache_key);
        let mesh_cache_key = play_character_cache_key(&cache_key);
        let part = self
            .static_model_cache
            .get(&mesh_cache_key)
            .and_then(|parts| parts.first())
            .copied()
            .ok_or_else(|| format!("sin malla en caché jugador: {mesh_cache_key}"))?;

        if let Some(mc) = self.world.get_mut::<MeshComponent>(id) {
            mc.mesh_idx = part.mesh_idx;
            mc.tex_idx = part.tex_idx;
        } else {
            self.world.insert(
                id,
                MeshComponent {
                    mesh_idx: part.mesh_idx,
                    tex_idx: part.tex_idx,
                },
            );
        }

        self.register_or_update_visual_model_meta(id, &library_path, true);
        self.play_character_mesh_forward_xz = part.forward_xz;
        if is_fbx_model_path(&source_for_ext) {
            if let Some(skin_fwd) = model_asset::fbx_skinned_play_forward_xz(
                Path::new(&source_for_ext),
                crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT,
            ) {
                self.play_character_mesh_forward_xz = skin_fwd;
            }
        }
        self.physics.remove_entity_body(id);

        self.try_bind_model_animations_with_gltf(id, path, None);
        let placement_bounds = self
            .play_character_visual_local_bounds(path)
            .unwrap_or(part.local_bounds);
        self.play_character_mesh_extents =
            Some(PlayCharacterMeshExtents::from_local_bounds(
                placement_bounds.0,
                placement_bounds.1,
            ));
        let play_asset_key =
            play_character_cache_key(&cache_key);
        if let Some(asset) = self.model_assets.get(&play_asset_key) {
            if is_fbx_model_path(&source_for_ext) {
                self.play_character_mesh_forward_xz =
                    model_asset::resolve_fbx_play_character_forward_xz(asset);
            } else if is_gltf_model_path(&source_for_ext) {
                self.play_character_mesh_forward_xz =
                    model_asset::resolve_gltf_play_character_forward_xz(asset);
            }
        }

        Ok(cache_key)
    }

    pub(crate) fn emit_entity_model_replaced_for_play_character(&self, id: EntityId, path: &str) {
        let (position, rotation, scale) = match self.world.get::<Transform>(id) {
            Some(t) => (
                Some(self.play_character_feet_position().to_array()),
                Some([
                    t.rotation.x,
                    t.rotation.y,
                    t.rotation.z,
                    t.rotation.w,
                ]),
                Some(t.scale.to_array()),
            ),
            None => (None, None, None),
        };
        send_event(&EngineEvent::EntityModelReplaced {
            id,
            path: path.to_string(),
            position,
            rotation,
            scale,
        });
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
        self.init_play_character_entity_core(id, feet);
        self.attach_play_character_body(id);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = body_center_from_feet(feet);
        }
        self.finish_play_character_entity_setup(id);
    }

    /// Jugador sin cubo placeholder: transform provisional en los pies hasta instalar mesh.
    fn init_play_character_entity_core(&mut self, id: EntityId, feet: glam::Vec3) {
        self.character_entities.push(id);
        self.play_character_entity = Some(id);
        self.play_character_mesh_forward_xz = glam::Vec2::new(0.0, 1.0);
        self.play_character_mesh_extents = None;
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = feet;
            t.rotation = glam::Quat::IDENTITY;
            t.scale = glam::Vec3::ONE;
        }
    }

    fn finish_play_character_entity_setup(&mut self, id: EntityId) {
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

    fn wait_for_play_character_model_gpu(&mut self, model_id: &str) -> bool {
        use std::time::{Duration, Instant};

        let started = Instant::now();
        let timeout = Duration::from_secs(60);
        while started.elapsed() < timeout {
            if self.ensure_play_character_model_cached(model_id).is_ok() {
                self.ensure_play_character_model_assets_cached(model_id);
                return true;
            }
            if self
                .imported_model_registry
                .get(model_id)
                .is_some_and(|e| e.state == rer_engine_shared::assets::AssetState::Failed)
            {
                return false;
            }
            self.poll_and_advance_model_preloads(
                crate::config_3d::static_model_cache::MODEL_GPU_PARTS_DURING_SAVE_LOAD,
            );
            std::thread::yield_now();
        }
        log::warn!("Tiempo agotado esperando mesh base del jugador: {model_id}");
        false
    }

    fn apply_play_character_visual_install(
        &mut self,
        id: EntityId,
        model_path: &str,
        feet: glam::Vec3,
    ) -> bool {
        let local_bounds = self
            .static_model_cache
            .get(&play_character_cache_key(&self.model_cache_key(model_path)))
            .and_then(|parts| parts.first())
            .map(|p| p.local_bounds);

        if self
            .install_play_character_visual_from_path(id, model_path)
            .is_err()
        {
            return false;
        }

        if let Some(bounds) = local_bounds {
            self.apply_play_character_model_placement_after_load(id, model_path, bounds);
        } else {
            self.sync_play_character_scale_to_body_height(id, model_path);
            self.place_play_character_at_world_feet(id, model_path, feet, true);
        }
        self.sync_play_character_body_rotation_after_mesh_assign();
        self.finish_play_character_model_replace(id, model_path);
        let library_path = self.model_library_path_for(model_path);
        self.emit_entity_model_replaced_for_play_character(id, &library_path);
        true
    }

    /// Instala el mesh empaquetado sin cubo intermedio (mismo pipeline que replace manual).
    fn try_spawn_play_character_with_bundled_model(
        &mut self,
        id: EntityId,
        feet: glam::Vec3,
    ) -> bool {
        let Some(model_id) = crate::assets::bundled::ensure_bundled_default_player_model(self)
        else {
            return false;
        };
        if !self.wait_for_play_character_model_gpu(&model_id) {
            return false;
        }

        self.init_play_character_entity_core(id, feet);
        if !self.apply_play_character_visual_install(id, &model_id, feet) {
            return false;
        }
        self.finish_play_character_entity_setup(id);
        log::info!("Jugador creado con mesh base empaquetado: {model_id}");
        true
    }

    pub(crate) fn spawn_play_character(&mut self) {
        let feet = self.camera.target;
        let id = self.world.spawn(Some("Player"));

        if !self.try_spawn_play_character_with_bundled_model(id, feet) {
            log::info!("Mesh base no disponible; jugador con cubo placeholder (id={id})");
            self.setup_play_character_entity(id, feet);
        }

        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "character".to_string(),
                path: "[Player]".to_string(),
                visual_model_path: self
                    .save_registry
                    .meta
                    .get(&id)
                    .and_then(|m| m.visual_model_path.clone()),
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
}
