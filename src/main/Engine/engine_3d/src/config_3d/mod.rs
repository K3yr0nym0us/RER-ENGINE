// ── Lógica exclusiva del modo 3D ─────────────────────────────────────────────
//
// Contiene:
//  · camera_3d        — Camera (órbita) + CameraUniform
//  · character_anchor / play_character / fps_camera / play_controller
//  · load_model       — carga un .glb/.gltf y añade mallas a la escena
//  · ray_cast         — proyecta un rayo desde píxel y devuelve la entidad más cercana
//  · pick_entity      — dispara el picking 3D y emite IPC
//  · project_to_screen — proyecta un punto 3D a píxeles de pantalla
//  · pick_gizmo_axis  — detecta el eje del gizmo más cercano al cursor
//  · drag_gizmo       — arrastra una entidad sobre un eje 3D
//  · update_hover     — actualiza el hover de entidad y gizmo en modo 3D

pub(crate) mod camera_3d;
pub(crate) use camera_3d::Camera;

pub(crate) mod character_anchor;
pub(crate) mod play_character;
pub(crate) mod fps_camera;
pub(crate) mod editor_camera;
pub(crate) mod preview_editor;
pub(crate) mod play_controller;
pub(crate) mod entity_textures;
pub(crate) mod gltf_texture_load;
pub(crate) mod mesh_3d;
pub(crate) mod model_asset;
pub(crate) mod model_animation;
pub(crate) mod collision_overlay;
pub(crate) mod physics_3d;
pub(crate) mod quick_build;
pub(crate) mod plane_tools;
pub(crate) mod execution_areas_3d;
pub(crate) mod plane_tool_rotate_dbg;
pub(crate) mod static_model_cache;
pub(crate) mod world_bounds;
pub(crate) mod player_ui;
pub(crate) use world_bounds::WorldBounds3D;

pub(crate) fn is_gltf_model_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            e.eq_ignore_ascii_case("glb") || e.eq_ignore_ascii_case("gltf")
        })
}

/// Posición del cuerpo Rapier (centro del AABB en `transform.position`).
pub(crate) fn physics_body_position_for_model_path(
    _model_path: &str,
    transform_position: [f32; 3],
    _half: [f32; 3],
) -> [f32; 3] {
    transform_position
}

/// Centro del AABB local de la malla (espacio del mesh antes del transform de entidad).
pub(crate) fn physics_aabb_center_local(bounds: ([f32; 3], [f32; 3])) -> [f32; 3] {
    let (min, max) = bounds;
    [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ]
}

/// Posición del collider en mundo: centro del AABB escalado/rotado (GLB/GLTF).
pub(crate) fn physics_body_world_center(
    transform: &Transform,
    local_bounds: Option<([f32; 3], [f32; 3])>,
    _model_path: &str,
    _half: [f32; 3],
) -> [f32; 3] {
    let Some(bounds) = local_bounds else {
        return transform.position.to_array();
    };
    let center_local = glam::Vec3::from_array(physics_aabb_center_local(bounds));
    let offset = transform.rotation * (transform.scale * center_local);
    (transform.position + offset).to_array()
}

/// Semieje del collider: AABB local de la malla × escala del transform.
pub(crate) fn physics_half_extents_for_model(
    scale: [f32; 3],
    local_bounds: Option<([f32; 3], [f32; 3])>,
) -> [f32; 3] {
    if let Some((min, max)) = local_bounds {
        let sx = scale[0].abs();
        let sy = scale[1].abs();
        let sz = scale[2].abs();
        return [
            ((max[0] - min[0]).abs() * 0.5 * sx).max(0.01),
            ((max[1] - min[1]).abs() * 0.5 * sy).max(0.01),
            ((max[2] - min[2]).abs() * 0.5 * sz).max(0.01),
        ];
    }
    [
        (scale[0].abs() * 0.5).max(0.01),
        (scale[1].abs() * 0.5).max(0.01),
        (scale[2].abs() * 0.5).max(0.01),
    ]
}

pub(crate) mod directional_light;

use crate::ipc::{AxisValue, RotationEulerDelta};

/// Resuelve un cambio de eje individual sobre un vector actual.
/// Si `axis_value` es `Some`, reemplaza solo ese eje; en caso contrario aplica el `vec` completo.
pub(crate) fn resolve_axis_update(
    current: glam::Vec3,
    vec: Option<[f32; 3]>,
    axis_value: Option<AxisValue>,
) -> Option<glam::Vec3> {
    if let Some(av) = axis_value {
        let mut next = current;
        match av.axis {
            0 => next.x = av.value,
            1 => next.y = av.value,
            2 => next.z = av.value,
            _ => {}
        }
        return Some(next);
    }
    vec.map(glam::Vec3::from_array)
}

/// Resuelve rotación de `set_transform`: quaternion explícito, delta Euler o Euler absoluto.
///
/// El front trabaja con índices semánticos: `axis 0 = pitch (X)`, `axis 1 = yaw (Y)`, `axis 2 = roll (Z)`.
///
/// **Deltas (`rotation_euler_delta`)**: se aplican como giro local en quaternion (`current * axis_quat`),
/// idéntico para X/Y/Z y al `quatRotateLocalAxis` del front. NO usar `to_euler` + suma + `from_euler`:
/// en YXZ el eje central (X/pitch) queda acotado a ~±90° al descomponer y el slider “se congela”
/// mientras Y/Z siguen circulares.
///
/// **Euler absoluto (`rotation_euler_degrees`)**: `from_euler(YXZ, yaw, pitch, roll)`.
pub(crate) fn resolve_set_transform_rotation(
    current: glam::Quat,
    rotation: Option<[f32; 4]>,
    euler_delta: Option<RotationEulerDelta>,
    euler_degrees: Option<[f32; 3]>,
) -> Option<glam::Quat> {
    if let Some(d) = euler_delta {
        let delta = d.degrees.to_radians();
        let axis_quat = match d.axis {
            0 => glam::Quat::from_rotation_x(delta),
            1 => glam::Quat::from_rotation_y(delta),
            2 => glam::Quat::from_rotation_z(delta),
            _ => return Some(current),
        };
        return Some((current * axis_quat).normalize());
    }
    if let Some([pitch_deg, yaw_deg, roll_deg]) = euler_degrees {
        return Some(glam::Quat::from_euler(
            glam::EulerRot::YXZ,
            yaw_deg.to_radians(),   // Y
            pitch_deg.to_radians(), // X
            roll_deg.to_radians(),  // Z
        ));
    }
    rotation.map(|r| glam::Quat::from_xyzw(r[0], r[1], r[2], r[3]))
}

use std::path::Path;

use glam::Vec3 as GlamVec3;

use crate::config_3d::character_anchor::{
    PLAY_CHARACTER_BODY_HEIGHT,
};
use crate::config_shared::point_to_segment_2d;
use crate::ecs::{EntityId, MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
    pub(crate) fn entity_model_local_bounds(&self, id: EntityId) -> Option<([f32; 3], [f32; 3])> {
        let path = self.entity_asset_path_for_bounds(id)?;
        self.cached_static_model_parts(&path)
            .and_then(|parts| parts.first())
            .map(|p| p.local_bounds)
    }

    pub(crate) fn set_entity_physics_from_mesh_aabb(&mut self, id: EntityId, body_type: &str) {
        let Some(t) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let model_path = self
            .entity_asset_path_for_bounds(id)
            .unwrap_or_default();
        let bounds = self.entity_model_local_bounds(id);
        let half = physics_half_extents_for_model(t.scale.abs().to_array(), bounds);
        let body_pos = physics_body_world_center(&t, bounds, model_path.as_str(), half);
        self.physics
            .set_entity_physics(id, true, body_type, body_pos, half);
    }

    /// Colisión de placeholders (`[EditorBox]`, cubos FP): caja estática según escala del transform.
    pub(crate) fn set_entity_physics_from_transform_box(&mut self, id: EntityId, body_type: &str) {
        let Some(t) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let scale = t.scale.abs().to_array();
        let half = [
            scale[0] * 0.5,
            scale[1] * 0.5,
            scale[2] * 0.5,
        ];
        let pos = t.position.to_array();
        self.physics
            .set_entity_physics(id, true, body_type, pos, half);
    }

    /// Colisión esférica para marcador `[Ball]`.
    pub(crate) fn entity_uses_sphere_physics(&self, id: EntityId) -> bool {
        let Some(meta) = self.save_registry.meta.get(&id) else {
            return false;
        };
        crate::entity_save_meta::entity_path_marker(&meta.path) == Some("[Ball]")
    }

    /// `colision` con AABB de archivo `.glb`/`.gltf`; marcadores de plantilla usan caja del transform.
    pub(crate) fn uses_mesh_file_collision(&self, id: EntityId) -> bool {
        let Some(meta) = self.save_registry.meta.get(&id) else {
            return false;
        };
        let path = meta
            .visual_model_path
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or(meta.path.as_str());
        if crate::entity_save_meta::entity_path_marker(path).is_some() {
            return false;
        }
        let lower = path.to_ascii_lowercase();
        lower.ends_with(".glb") || lower.ends_with(".gltf")
    }

    /// Recrea el collider Rapier alineado al AABB de la malla (posición + escala actuales).
    pub(crate) fn sync_entity_physics_collider(&mut self, id: EntityId) {
        if !self.physics.has_physics(id) {
            return;
        }
        if self.is_plane_wall_entity(id) && self.collider_entities.contains(&id) {
            self.sync_plane_wall_physics(id);
            return;
        }
        let body_type = self.physics.get_body_type(id).to_string();
        self.set_entity_physics_from_mesh_aabb(id, &body_type);
    }

    /// Aplica `colision` tras spawn/carga o cambio de modelo según categoría y tipo de mesh.
    pub(crate) fn reconcile_entity_physics_with_mesh(&mut self, id: EntityId) {
        if self.is_plane_wall_entity(id) {
            if self.collider_entities.contains(&id)
                && self.entity_colision.get(&id).copied().unwrap_or(true)
            {
                self.sync_plane_wall_physics(id);
            }
            return;
        }
        if !self.entity_colision.get(&id).copied().unwrap_or(true) {
            if self.play_character_entity != Some(id) {
                self.physics.remove_entity_body(id);
            }
            return;
        }

        if self.play_character_entity == Some(id) {
            self.ensure_play_character_kinematic_only();
            return;
        }

        if self.ground_entity_id() == Some(id) {
            self.physics.remove_entity_body(id);
            return;
        }

        if self.sun_entity == Some(id) {
            self.physics.remove_entity_body(id);
            return;
        }

        if self.editor_camera_entity == Some(id) {
            self.physics.remove_entity_body(id);
            return;
        }

        let category = self
            .save_registry
            .meta
            .get(&id)
            .and_then(|m| m.entity_category.as_deref());

        if crate::entity_save_meta::entity_category_uses_character_capsule(category) {
            self.physics.remove_entity_body(id);
            return;
        }

        if !self.uses_mesh_file_collision(id) {
            let body_type = if self.physics.has_physics(id) {
                self.physics.get_body_type(id).to_string()
            } else {
                "static".to_string()
            };
            if self.entity_uses_sphere_physics(id) {
                let Some(t) = self.world.get::<Transform>(id).cloned() else {
                    return;
                };
                let radius = t.scale.x.abs().max(0.01) * 0.5;
                let pos = t.position.to_array();
                self.physics
                    .set_entity_sphere_physics(id, true, &body_type, pos, radius);
            } else {
                self.set_entity_physics_from_transform_box(id, &body_type);
            }
            return;
        }

        let body_type = if self.physics.has_physics(id) {
            self.physics.get_body_type(id).to_string()
        } else {
            "static".to_string()
        };

        if crate::entity_save_meta::entity_category_uses_mesh_collision(category) {
            self.set_entity_physics_from_mesh_aabb(id, &body_type);
            return;
        }

        if self.physics.has_physics(id) {
            self.set_entity_physics_from_mesh_aabb(id, &body_type);
        }
    }

    /// Tras `replace_entity_model` en entidades que no son el jugador principal.
    pub(crate) fn reconcile_entity_physics_after_model_replace(&mut self, id: EntityId) {
        self.reconcile_entity_physics_with_mesh(id);
    }

    /// AABB en mundo para hover/click: misma caja que Rapier (`local_bounds` + transform).
    fn entity_world_pick_aabb(&self, id: EntityId, transform: &Transform) -> (GlamVec3, GlamVec3) {
        if self.play_character_entity == Some(id) {
            if let Some((center, half)) = self.play_character_world_pick_aabb() {
                return (center, half);
            }
        }
        let model_path = self
            .entity_asset_path_for_bounds(id)
            .unwrap_or_default();
        let bounds = self.entity_model_local_bounds(id);
        let half = physics_half_extents_for_model(transform.scale.abs().to_array(), bounds);
        let center = physics_body_world_center(transform, bounds, model_path.as_str(), half);
        (GlamVec3::from_array(center), GlamVec3::from_array(half))
    }

    /// Instancia un modelo 3D en la escena (reutiliza caché estática si existe).
    pub(crate) fn load_model(&mut self, path: &str, entity_category: Option<&str>, kind: &str) {
        if self.queue_load_model_if_preloading(path, entity_category, false, kind) {
            return;
        }
        if let Err(e) = self.ensure_static_model_cached(path) {
            log::error!("Error cargando modelo: {e}");
            send_event(&EngineEvent::Error { message: e });
            return;
        }
        let key = self.model_path_key(path);
        let parts: Vec<_> = self
            .static_model_cache
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let count = parts.len();
        for part in parts {
            self.spawn_model_from_cached_part(part, &key, entity_category, kind);
        }
        log::info!("Modelo instanciado desde caché: {key} ({count} malla/s)");
    }

    /// Sustituye el mesh visual de una entidad existente (mismo id, sin recrear entidad).
    ///
    /// Orden: cola si precarga en curso → caché GPU (clave `::play_character` para el jugador) →
    /// carga desde disco. No llamar `sync_player_rotation_from_look` en editor al asignar mesh:
    /// la rotación guardada llega con `set_play_character_view` tras `entity_model_replaced`.
    pub(crate) fn replace_entity_model(&mut self, id: EntityId, path: &str) {
        if self.world.get::<Transform>(id).is_none() {
            send_event(&EngineEvent::Error {
                message: format!("Entidad {id} no encontrada para reemplazar modelo"),
            });
            return;
        }

        let is_play_character = self.play_character_entity == Some(id);
        if self.queue_entity_model_replace_if_preloading(id, path, is_play_character) {
            return;
        }
        if self.replace_entity_model_from_static_cache(id, path, is_play_character) {
            return;
        }

        let normalize = if is_play_character {
            Some(PLAY_CHARACTER_BODY_HEIGHT)
        } else {
            None
        };
        let path_buf = Path::new(path);
        let is_gltf = path_buf
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("glb") || e.eq_ignore_ascii_case("gltf"));
        let gltf_file = if is_gltf {
            match model_asset::import_gltf(path_buf) {
                Ok(f) => Some(f),
                Err(e) => {
                    send_event(&EngineEvent::Error { message: e });
                    return;
                }
            }
        } else {
            None
        };
        self.replace_entity_model_inner(id, path, gltf_file, is_play_character, normalize);
    }

    /// Reutiliza malla/capa de textura ya en GPU (precarga o carga previa).
    fn replace_entity_model_from_static_cache(
        &mut self,
        id: EntityId,
        path: &str,
        is_play_character: bool,
    ) -> bool {
        let cache_key = self.model_cache_key(path);
        let library_path = self.model_library_path_for(path);
        let mesh_cache_key = if is_play_character {
            if let Err(e) = self.ensure_play_character_model_cached(path) {
                log::debug!(
                    "[replace_entity_model] sin caché jugador para {path}: {e}"
                );
                return false;
            }
            crate::config_3d::static_model_cache::play_character_cache_key(&cache_key)
        } else if let Err(e) = self.ensure_static_model_cached(path) {
            log::debug!("[replace_entity_model] sin caché reutilizable para {path}: {e}");
            return false;
        } else {
            cache_key.clone()
        };
        let Some(part) = self
            .static_model_cache
            .get(&mesh_cache_key)
            .and_then(|parts| parts.first())
            .copied()
        else {
            return false;
        };

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

        if is_play_character {
            if self.install_play_character_visual_from_path(id, path).is_err() {
                return false;
            }
            if self.restoring_save_manifest {
                log::info!(
                    "Mesh jugador instalado desde caché (restore manifest, sin auto-escala): {mesh_cache_key}"
                );
                return true;
            }
            if self.should_apply_play_character_mesh_collision(id, path) {
                self.apply_play_character_model_placement_after_load(
                    id,
                    path,
                    part.local_bounds,
                );
                self.sync_play_character_body_rotation_after_mesh_assign();
            }
            if self.should_apply_play_character_mesh_collision(id, path) {
                self.finish_play_character_model_replace(id, path);
            }
            self.emit_entity_model_replaced_for_play_character(id, &library_path);
            log::info!(
                "Modelo reemplazado desde caché en entidad {id}: {mesh_cache_key}"
            );
            return true;
        }

        self.register_or_update_visual_model_meta(id, &library_path, false);
        self.try_bind_model_animations_with_gltf(id, &cache_key, None);
        self.reconcile_entity_physics_after_model_replace(id);
        let (position, rotation, scale) = match self.world.get::<Transform>(id) {
            Some(t) => (
                Some(t.position.to_array()),
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
            path: library_path.clone(),
            position,
            rotation,
            scale,
        });
        log::info!(
            "Modelo reemplazado desde caché en entidad {id}: {mesh_cache_key}"
        );
        true
    }

    fn replace_entity_model_inner(
        &mut self,
        id: EntityId,
        path: &str,
        gltf_file: Option<std::sync::Arc<model_asset::GltfFile>>,
        is_play_character: bool,
        normalize: Option<f32>,
    ) {
        let path_buf = Path::new(path);
        let loaded = match (gltf_file.as_deref(), normalize) {
            (Some(file), Some(extent)) => {
                match mesh_3d::load_gltf_preview_from_file(&self.device, file, extent) {
                    Ok(parts) => parts,
                    Err(e) => {
                        send_event(&EngineEvent::Error { message: e });
                        return;
                    }
                }
            }
            _ => match mesh_3d::load_model_file(&self.device, path_buf, normalize) {
                Ok(parts) => parts,
                Err(e) => {
                    send_event(&EngineEvent::Error { message: e });
                    return;
                }
            },
        };

        let Some(part) = loaded.into_iter().next() else {
            send_event(&EngineEvent::Error {
                message: "El archivo no contiene mallas".to_string(),
            });
            return;
        };

        let mesh_idx = self.meshes.len();
        let tex_idx = self.tex_layers.len();
        self.meshes.push(part.mesh);
        let layer = self
            .texture_array
            .pack(&self.queue, &part.rgba, part.width, part.height);
        self.tex_layers.push(layer);

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

        if is_play_character {
            self.register_or_update_visual_model_meta(id, path, true);
            self.play_character_mesh_forward_xz = part.forward_xz;
            self.physics.remove_entity_body(id);
        } else {
            self.register_or_update_visual_model_meta(id, path, false);
        }

        if is_play_character && self.restoring_save_manifest {
            self.play_character_mesh_extents =
                Some(crate::config_3d::character_anchor::PlayCharacterMeshExtents::from_local_bounds(
                    part.local_bounds.0,
                    part.local_bounds.1,
                ));
            self.model_assets.remove(path);
            self.try_bind_model_animations_with_gltf(id, path, gltf_file.as_deref());
            if let Some(asset) = self.model_assets.get(path) {
                if is_gltf_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_gltf_play_character_forward_xz(asset);
                }
            }
            let asset_key = self.model_path_key(path);
            let play_key =
                crate::config_3d::static_model_cache::play_character_cache_key(&asset_key);
            if !self.static_model_cache.contains_key(&play_key) {
                self.static_model_cache.insert(
                    play_key,
                    vec![crate::config_3d::static_model_cache::CachedStaticModelPart {
                        mesh_idx,
                        tex_idx,
                        local_bounds: part.local_bounds,
                        forward_xz: part.forward_xz,
                    }],
                );
            }
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
            log::info!("[restore] mesh jugador cargado sin pipeline editor: {path}");
            return;
        }

        if self.should_apply_play_character_mesh_collision(id, path) {
            self.apply_play_character_model_placement_after_load(id, path, part.local_bounds);
            self.sync_play_character_body_rotation_after_mesh_assign();
        } else if !is_play_character {
            self.reconcile_entity_physics_after_model_replace(id);
        }

        let (position, rotation, scale) = match self.world.get::<Transform>(id) {
            Some(t) => (
                Some(
                    if is_play_character {
                        self.play_character_feet_position().to_array()
                    } else {
                        t.position.to_array()
                    },
                ),
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

        // Forzar recarga del asset (orientación/normalize pueden cambiar entre versiones del motor).
        self.model_assets.remove(path);
        self.try_bind_model_animations_with_gltf(id, path, gltf_file.as_deref());

        if is_play_character {
            if let Some(asset) = self.model_assets.get(path) {
                if is_gltf_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_gltf_play_character_forward_xz(asset);
                }
            }
            self.sync_play_character_body_rotation_after_mesh_assign();
        }
        if self.should_apply_play_character_mesh_collision(id, path) {
            self.finish_play_character_model_replace(id, path);
        } else if !is_play_character {
            self.reconcile_entity_physics_after_model_replace(id);
        }

        if is_play_character {
            let asset_key = self.model_path_key(path);
            let play_key =
                crate::config_3d::static_model_cache::play_character_cache_key(&asset_key);
            if !self.static_model_cache.contains_key(&play_key) {
                self.static_model_cache.insert(
                    play_key,
                    vec![crate::config_3d::static_model_cache::CachedStaticModelPart {
                        mesh_idx,
                        tex_idx,
                        local_bounds: part.local_bounds,
                        forward_xz: part.forward_xz,
                    }],
                );
            }
        }

        send_event(&EngineEvent::EntityModelReplaced {
            id,
            path: path.to_string(),
            position,
            rotation,
            scale,
        });
        log::info!("Modelo reemplazado en entidad {id}: {path}");
    }

    fn ray_intersects_aabb(
        origin: GlamVec3,
        dir: GlamVec3,
        center: GlamVec3,
        half: GlamVec3,
    ) -> Option<f32> {
        let min = center - half;
        let max = center + half;
        let mut tmin = f32::NEG_INFINITY;
        let mut tmax = f32::INFINITY;

        let oa = origin.to_array();
        let da = dir.to_array();
        let mna = min.to_array();
        let mxa = max.to_array();
        for i in 0..3 {
            let o = oa[i];
            let d = da[i];
            let mn = mna[i];
            let mx = mxa[i];
            if d.abs() < 1e-8 {
                if o < mn || o > mx {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let mut t1 = (mn - o) * inv;
                let mut t2 = (mx - o) * inv;
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                tmin = tmin.max(t1);
                tmax = tmax.min(t2);
                if tmax < tmin {
                    return None;
                }
            }
        }

        let t = if tmin >= 0.0 { tmin } else { tmax };
        if t >= 0.0 { Some(t) } else { None }
    }

    fn ray_cast(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        use glam::Vec4;

        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

        let ndc_x = (2.0 * pixel_x / w) - 1.0;
        let ndc_y = -(2.0 * pixel_y / h) + 1.0;

        let inv_proj = self.camera.proj_matrix(aspect).inverse();
        let inv_view = self.camera_view_matrix().inverse();

        let clip_dir = Vec4::new(ndc_x, ndc_y, -1.0, 0.0);
        let view_dir = inv_proj * clip_dir;
        let view_dir = Vec4::new(view_dir.x, view_dir.y, -1.0, 0.0);
        let world_dir = (inv_view * view_dir).truncate().normalize();
        let ray_origin = self.camera_world_position();

        let mut closest: Option<(f32, EntityId)> = None;
        for &entity in self.world.entities() {
            if self.world.get::<NonSelectable>(entity).is_some()
                || self.world.get::<MeshComponent>(entity).is_none()
            {
                continue;
            }
            let Some(transform) = self.world.get::<Transform>(entity) else {
                continue;
            };
            let (center, half) = self.entity_world_pick_aabb(entity, transform);
            if let Some(t) = Self::ray_intersects_aabb(ray_origin, world_dir, center, half)
            {
                if closest.map_or(true, |(ct, _)| t < ct) {
                    closest = Some((t, entity));
                }
            }
        }
        closest.map(|(_, id)| id)
    }

    pub fn pick_entity(&mut self, pixel_x: f32, pixel_y: f32) {
        if self.player_ui_edit_active {
            return;
        }
        match self.ray_cast(pixel_x, pixel_y) {
            Some(entity) => {
                if self.ctrl_held {
                    if let Some(idx) = self.selected_entities.iter().position(|&e| e == entity) {
                        self.selected_entities.swap_remove(idx);
                        if self.selected_entity == Some(entity) {
                            self.selected_entity = self.selected_entities.last().copied();
                        }
                        if self.selected_entities.is_empty() {
                            self.selected_entity = None;
                            send_event(&EngineEvent::EntityDeselected);
                        } else if let Some(active_id) = self.selected_entity {
                            self.send_entity_selected_event(active_id);
                        }
                        send_event(&EngineEvent::MultiSelectChanged {
                            ids: self.selected_entities.clone(),
                        });
                        self.sync_editor_camera_focus();
                        return;
                    } else {
                        self.selected_entities.push(entity);
                        self.selected_entity = Some(entity);
                    }
                } else {
                    if self.selected_entity == Some(entity)
                        && self.selected_entities.len() == 1
                        && self.selected_entities[0] == entity
                    {
                        return;
                    }
                    self.selected_entities.clear();
                    self.selected_entities.push(entity);
                    self.selected_entity = Some(entity);
                }
                self.send_entity_selected_event(entity);
                if self.editor_camera_entity == Some(entity) {
                    self.sync_editor_viewport_from_camera_entity();
                }
                if self.ctrl_held && self.selected_entities.len() > 1 {
                    send_event(&EngineEvent::MultiSelectChanged {
                        ids: self.selected_entities.clone(),
                    });
                }
                self.sync_editor_camera_focus();
            }
            None => {
                if !self.ctrl_held
                    && (self.selected_entity.is_some() || !self.selected_entities.is_empty())
                {
                    self.selected_entity = None;
                    self.selected_entities.clear();
                    send_event(&EngineEvent::EntityDeselected);
                    self.sync_editor_camera_focus();
                }
            }
        }
    }

    pub(crate) fn project_to_screen(&self, p: GlamVec3) -> Option<(f32, f32)> {
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let vp = self.camera.proj_matrix(w / h) * self.camera_view_matrix();
        let c = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 0.0 {
            return None;
        }
        Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
    }

    pub fn pick_gizmo_axis(&self, pixel_x: f32, pixel_y: f32) -> Option<usize> {
        let origin = self.selection_center()?;
        let so = self.project_to_screen(origin)?;

        const LEN: f32 = 1.2;
        const THRESH: f32 = 16.0;
        let dirs = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z];

        let mut best: Option<(f32, usize)> = None;
        for (i, &dir) in dirs.iter().enumerate() {
            if let Some(tip) = self.project_to_screen(origin + dir * LEN) {
                let d = point_to_segment_2d(pixel_x, pixel_y, so.0, so.1, tip.0, tip.1);
                if d < THRESH && best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    pub fn drag_gizmo(
        &mut self,
        pixel_x: f32,
        pixel_y: f32,
        last_x: f32,
        last_y: f32,
        axis_idx: usize,
    ) {
        let selected_ids: Vec<EntityId> = if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        };
        if selected_ids.is_empty() {
            return;
        }

        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

        let mut sum = GlamVec3::ZERO;
        let mut count = 0usize;
        for &id in &selected_ids {
            if let Some(t) = self.world.get::<Transform>(id) {
                sum += t.position;
                count += 1;
            }
        }
        if count == 0 {
            return;
        }
        let origin = sum / count as f32;

        let vp = self.camera.proj_matrix(aspect) * self.camera_view_matrix();
        let axis_world = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z][axis_idx];

        let project = |p: GlamVec3| -> Option<(f32, f32)> {
            let c = vp * glam::Vec4::new(p.x, p.y, p.z, 1.0);
            if c.w <= 0.0 {
                return None;
            }
            Some(((c.x / c.w + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
        };

        let (s0x, s0y) = match project(origin) {
            Some(p) => p,
            None => return,
        };
        let (s1x, s1y) = match project(origin + axis_world) {
            Some(p) => p,
            None => return,
        };

        let ax = s1x - s0x;
        let ay = s1y - s0y;
        let axis_len = (ax * ax + ay * ay).sqrt();
        if axis_len < 1e-4 {
            return;
        }

        let dx = pixel_x - last_x;
        let dy = pixel_y - last_y;
        let world_delta = (dx * ax + dy * ay) / (axis_len * axis_len);

        for &sel_id in &selected_ids {
            if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
                t.position += axis_world * world_delta;
            }
            if let Some(t) = self.world.get::<Transform>(sel_id).cloned() {
                if self.is_plane_wall_entity(sel_id) && self.collider_entities.contains(&sel_id) {
                    self.sync_plane_wall_physics(sel_id);
                } else if self.physics.has_physics(sel_id) {
                    let half = [
                        (t.scale.x * 0.5).max(0.01),
                        (t.scale.y * 0.5).max(0.01),
                        (t.scale.z * 0.5).max(0.01),
                    ];
                    let pos = t.position.to_array();
                    let model_path = self
                        .save_registry
                        .meta
                        .get(&sel_id)
                        .map(|m| m.path.as_str())
                        .unwrap_or("");
                    let body_pos =
                        physics_body_position_for_model_path(model_path, pos, half);
                    self.physics
                        .sync_entity_physics_from_transform(sel_id, body_pos, half);
                }
            }
        }

        if selected_ids
            .iter()
            .any(|id| self.sun_entity == Some(*id))
        {
            self.sync_directional_light_from_sun();
        }
        if !self.is_play_controller_active() {
            self.sync_editor_camera_focus();
        }

        if let Some(player_id) = self.play_character_entity {
            if selected_ids.contains(&player_id) {
                self.emit_play_character_view_changed(false);
            }
        }

        let lead_id = self.selected_entity.or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id {
            if self.world.get::<Transform>(sel_id).is_some() {
                self.send_entity_selected_event(sel_id);
            }
        }
    }

    pub fn update_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        if self.player_ui_edit_active {
            return;
        }
        let prev_hover = self.hovered_entity;
        self.hovered_entity = self.ray_cast(pixel_x, pixel_y);
        self.hovered_gizmo_axis = self.pick_gizmo_axis(pixel_x, pixel_y);
        match (prev_hover, self.hovered_entity) {
            (None, Some(id)) => crate::ipc::send_event(&crate::ipc::EngineEvent::EntityHovered { id }),
            (Some(_), None) => crate::ipc::send_event(&crate::ipc::EngineEvent::EntityUnhovered),
            (Some(a), Some(b)) if a != b => {
                crate::ipc::send_event(&crate::ipc::EngineEvent::EntityHovered { id: b })
            }
            _ => {}
        }
    }
}
