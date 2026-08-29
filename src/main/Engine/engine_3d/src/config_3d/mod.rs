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

pub(crate) mod bone_physics;
pub(crate) mod bone_physics_pick;
pub(crate) mod character_anchor;
pub(crate) mod collision_overlay;
pub(crate) mod editor_camera;
pub(crate) mod editor_viewport_controls;
pub(crate) mod entity_attachments;
pub(crate) mod entity_sockets;
pub(crate) mod entity_textures;
pub(crate) mod execution_areas_3d;
pub(crate) mod fbx_facing;
pub(crate) mod fps_camera;
pub(crate) mod gltf_texture_load;
pub(crate) mod material_validation;
pub(crate) mod mesh_3d;
pub(crate) mod mesh_3d_fbx;
pub(crate) mod model_animation;
pub(crate) mod model_asset;
pub(crate) mod model_asset_fbx;
pub(crate) mod msaa_graphics;
pub(crate) mod msaa_settings;
pub(crate) mod pbr_presets;
pub(crate) mod physics_3d;
pub(crate) mod plane_tool_rotate_dbg;
pub(crate) mod plane_tools;
pub(crate) mod play_character;
pub(crate) mod play_controller;
pub(crate) mod player_ui;
pub(crate) mod preview_editor;
pub(crate) mod projectiles;
pub(crate) mod quick_build;
pub(crate) mod reflection_graphics;
pub(crate) mod reflection_settings;
pub(crate) mod shadow_graphics;
pub(crate) mod shadow_settings;
pub(crate) mod skeleton_debug;
pub(crate) mod skin_diag;
pub(crate) mod socket_bone_pick;
pub(crate) mod socket_debug;
pub(crate) mod static_model_cache;
pub(crate) mod texture_graphics;
pub(crate) mod transform_gizmo;
pub(crate) mod world_bounds;
pub(crate) use world_bounds::WorldBounds3D;

pub(crate) fn is_fbx_model_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("fbx"))
}

pub(crate) fn is_gltf_model_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("glb") || e.eq_ignore_ascii_case("gltf"))
}

/// Posición del cuerpo Rapier: pies en `transform.position` para FBX; centro en el resto.
pub(crate) fn physics_body_position_for_model_path(
    model_path: &str,
    transform_position: [f32; 3],
    half: [f32; 3],
) -> [f32; 3] {
    if is_fbx_model_path(model_path) {
        physics_3d::physics_center_from_feet_position(transform_position, half)
    } else {
        transform_position
    }
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

/// Posición del collider en mundo: centro del AABB escalado/rotado (GLB/GLTF);
/// FBX mantiene convención de pies en `transform.position`.
pub(crate) fn physics_body_world_center(
    transform: &Transform,
    local_bounds: Option<([f32; 3], [f32; 3])>,
    model_path: &str,
    half: [f32; 3],
) -> [f32; 3] {
    if is_fbx_model_path(model_path) {
        return physics_body_position_for_model_path(
            model_path,
            transform.position.to_array(),
            half,
        );
    }
    let Some(bounds) = local_bounds else {
        return transform.position.to_array();
    };
    let center_local = glam::Vec3::from_array(physics_aabb_center_local(bounds));
    let offset = transform.rotation * (transform.scale * center_local);
    (transform.position + offset).to_array()
}

/// Posición del `Transform` para que el centro visual del mesh quede en `desired_center`.
pub(crate) fn transform_position_for_visual_center(
    desired_center: glam::Vec3,
    rotation: glam::Quat,
    scale: glam::Vec3,
    model_path: &str,
    local_bounds: Option<([f32; 3], [f32; 3])>,
) -> glam::Vec3 {
    let probe = Transform {
        position: glam::Vec3::ZERO,
        rotation,
        scale,
    };
    let half = physics_half_extents_for_model(scale.abs().to_array(), local_bounds);
    let visual = glam::Vec3::from_array(physics_body_world_center(
        &probe,
        local_bounds,
        model_path,
        half,
    ));
    desired_center - visual
}

/// Aplica rotación alrededor de `group_pivot` manteniendo el centro visual de la malla
/// (mismo criterio que `entity_world_pick_aabb` / gizmo), no `Transform.position`.
pub(crate) fn rotate_entity_transform_about_visual_center(
    start_transform: &Transform,
    start_visual_center: glam::Vec3,
    group_pivot: glam::Vec3,
    delta_quat: glam::Quat,
    model_path: &str,
    local_bounds: Option<([f32; 3], [f32; 3])>,
) -> (glam::Vec3, glam::Quat) {
    let new_visual = group_pivot + delta_quat * (start_visual_center - group_pivot);
    let new_rot = (delta_quat * start_transform.rotation).normalize();
    let new_pos = transform_position_for_visual_center(
        new_visual,
        new_rot,
        start_transform.scale,
        model_path,
        local_bounds,
    );
    (new_pos, new_rot)
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

use crate::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT;
use crate::config_3d::transform_gizmo::TransformGizmoMode;
use crate::config_shared::point_to_segment_2d;
use crate::ecs::{EntityId, MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

impl State {
    pub(crate) fn entity_model_local_bounds(&self, id: EntityId) -> Option<([f32; 3], [f32; 3])> {
        let asset_path = self.entity_asset_path_for_bounds(id)?;

        // `static_model_cache` está indexada por `model_id` para assets importados.
        // En el editor el `EntitySaveMeta` puede apuntar a la ruta “fuente” (GLB/GLTF/FBX),
        // así que intentamos primero con `asset_path` y si no hay caché, resolvemos
        // `model_id` para encontrar los bounds correctos.
        if let Some(bounds) = self
            .cached_static_model_parts(&asset_path)
            .and_then(|parts| parts.first())
            .map(|p| p.local_bounds)
        {
            return Some(bounds);
        }

        if let Some(model_id) = self.imported_model_registry.model_id_for_path(&asset_path)
            && let Some(bounds) = self
                .cached_static_model_parts(&model_id)
                .and_then(|parts| parts.first())
                .map(|p| p.local_bounds)
        {
            return Some(bounds);
        }

        self.play_character_visual_local_bounds(&asset_path)
    }

    /// Bounds locales de la malla visible (estática, skinned o caché del jugador).
    pub(crate) fn resolve_entity_visual_local_bounds(
        &self,
        id: EntityId,
    ) -> Option<([f32; 3], [f32; 3])> {
        if let Some(bounds) = self.entity_model_local_bounds(id) {
            return Some(bounds);
        }
        if self.play_character_entity == Some(id)
            && let Some(ext) = self.play_character_mesh_extents
        {
            return Some((ext.local_min, ext.local_max));
        }
        None
    }

    pub(crate) fn set_entity_physics_from_mesh_aabb(&mut self, id: EntityId, body_type: &str) {
        let Some(t) = self.world.get::<Transform>(id).cloned() else {
            return;
        };
        let model_path = self.entity_asset_path_for_bounds(id).unwrap_or_default();
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
        let half = [scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5];
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
        lower.ends_with(".glb") || lower.ends_with(".gltf") || lower.ends_with(".fbx")
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
    pub(crate) fn entity_world_pick_aabb(
        &self,
        id: EntityId,
        transform: &Transform,
    ) -> (GlamVec3, GlamVec3) {
        if self.play_character_entity == Some(id)
            && let Some((center, half)) = self.play_character_world_pick_aabb()
        {
            return (center, half);
        }
        let model_path = self.entity_asset_path_for_bounds(id).unwrap_or_default();
        let bounds = self.resolve_entity_visual_local_bounds(id);
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
            if self.ensure_play_character_model_cached(path).is_err() {
                return false;
            }
            crate::config_3d::static_model_cache::play_character_cache_key(&cache_key)
        } else if self.ensure_static_model_cached(path).is_err() {
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
            if self
                .install_play_character_visual_from_path(id, path)
                .is_err()
            {
                return false;
            }
            if self.restoring_save_manifest {
                log::info!(
                    "Mesh jugador instalado desde caché (restore manifest, sin auto-escala): {mesh_cache_key}"
                );
                return true;
            }
            if self.should_apply_play_character_mesh_collision(id, path) {
                self.apply_play_character_model_placement_after_load(id, path, part.local_bounds);
                self.sync_play_character_body_rotation_after_mesh_assign();
            }
            if self.should_apply_play_character_mesh_collision(id, path) {
                self.finish_play_character_model_replace(id, path);
            }
            self.emit_entity_model_replaced_for_play_character(id, &library_path);
            log::info!("Modelo reemplazado desde caché en entidad {id}: {mesh_cache_key}");
            return true;
        }

        self.register_or_update_visual_model_meta(id, &library_path, false);
        self.try_bind_model_animations_with_gltf(id, &cache_key, None);
        self.reconcile_entity_physics_after_model_replace(id);
        let (position, rotation, scale) = match self.world.get::<Transform>(id) {
            Some(t) => (
                Some(t.position.to_array()),
                Some([t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]),
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
        log::info!("Modelo reemplazado desde caché en entidad {id}: {mesh_cache_key}");
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
            self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        }

        if is_play_character {
            self.register_or_update_visual_model_meta(id, path, true);
            self.play_character_mesh_forward_xz = part.forward_xz;
            if is_fbx_model_path(path)
                && let Some(skin_fwd) =
                    crate::config_3d::model_asset_fbx::fbx_skinned_play_forward_xz(
                        Path::new(path),
                        PLAY_CHARACTER_BODY_HEIGHT,
                    )
            {
                self.play_character_mesh_forward_xz = skin_fwd;
            }
            self.physics.remove_entity_body(id);
        } else {
            self.register_or_update_visual_model_meta(id, path, false);
        }

        if is_play_character && self.restoring_save_manifest {
            self.play_character_mesh_extents = Some(
                crate::config_3d::character_anchor::PlayCharacterMeshExtents::from_local_bounds(
                    part.local_bounds.0,
                    part.local_bounds.1,
                ),
            );
            self.model_assets.remove(
                &crate::config_3d::static_model_cache::model_asset_cache_key(
                    &self.model_cache_key(path),
                    Some(PLAY_CHARACTER_BODY_HEIGHT),
                ),
            );
            self.model_assets.remove(&self.model_cache_key(path));
            self.try_bind_model_animations_with_gltf(id, path, gltf_file.as_deref());
            if let Some(asset) = self.get_model_asset_for_entity(path, id) {
                if is_fbx_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_fbx_play_character_forward_xz(&asset);
                } else if is_gltf_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_gltf_play_character_forward_xz(&asset);
                }
            }
            let asset_key = self.model_path_key(path);
            let play_key =
                crate::config_3d::static_model_cache::play_character_cache_key(&asset_key);
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.static_model_cache.entry(play_key)
            {
                e.insert(vec![
                    crate::config_3d::static_model_cache::CachedStaticModelPart {
                        mesh_idx,
                        tex_idx,
                        local_bounds: part.local_bounds,
                        forward_xz: part.forward_xz,
                        roughness: -1.0,
                        metallic: 0.0,
                        ior: 0.0,
                    },
                ]);
            }
            let (position, rotation, scale) = match self.world.get::<Transform>(id) {
                Some(t) => (
                    Some(self.play_character_feet_position().to_array()),
                    Some([t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]),
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
                Some(if is_play_character {
                    self.play_character_feet_position().to_array()
                } else {
                    t.position.to_array()
                }),
                Some([t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]),
                Some(t.scale.to_array()),
            ),
            None => (None, None, None),
        };

        // Forzar recarga del asset (orientación/normalize pueden cambiar entre versiones del motor).
        if is_play_character {
            self.model_assets.remove(
                &crate::config_3d::static_model_cache::model_asset_cache_key(
                    &self.model_cache_key(path),
                    Some(PLAY_CHARACTER_BODY_HEIGHT),
                ),
            );
        }
        self.model_assets.remove(&self.model_cache_key(path));
        self.try_bind_model_animations_with_gltf(id, path, gltf_file.as_deref());

        if is_play_character {
            if let Some(asset) = self.get_model_asset_for_entity(path, id) {
                if is_fbx_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_fbx_play_character_forward_xz(&asset);
                } else if is_gltf_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_gltf_play_character_forward_xz(&asset);
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
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.static_model_cache.entry(play_key)
            {
                e.insert(vec![
                    crate::config_3d::static_model_cache::CachedStaticModelPart {
                        mesh_idx,
                        tex_idx,
                        local_bounds: part.local_bounds,
                        forward_xz: part.forward_xz,
                        roughness: -1.0,
                        metallic: 0.0,
                        ior: 0.0,
                    },
                ]);
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

    pub(crate) fn entity_at_pixel(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        self.ray_cast(pixel_x, pixel_y)
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
            if self.world.get::<NonSelectable>(entity).is_some() {
                continue;
            }
            let Some(transform) = self.world.get::<Transform>(entity) else {
                continue;
            };
            let (center, half) = if self.is_reflection_probe_entity(entity) {
                self.reflection_probe_pick_aabb(transform)
            } else if self.world.get::<MeshComponent>(entity).is_none() {
                continue;
            } else {
                self.entity_world_pick_aabb(entity, transform)
            };
            if let Some(t) = Self::ray_intersects_aabb(ray_origin, world_dir, center, half)
                && closest.is_none_or(|(ct, _)| t < ct)
            {
                closest = Some((t, entity));
            }
        }
        closest.map(|(_, id)| id)
    }

    pub fn pick_entity(&mut self, pixel_x: f32, pixel_y: f32) {
        if self.player_ui_edit_active {
            return;
        }
        if self.socket_bone_pick_entity.is_some() {
            let _ = self.try_pick_socket_bone_click(pixel_x, pixel_y);
            return;
        }
        if self.bone_physics_pick_entity.is_some() {
            let _ = self.try_pick_bone_physics_click(pixel_x, pixel_y);
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
        if self.socket_bone_pick_entity.is_some() || self.bone_physics_pick_entity.is_some() {
            return None;
        }
        let origin = self.selection_center()?;
        match self.transform_gizmo_mode {
            TransformGizmoMode::Translate => {
                self.pick_translate_gizmo_axis(origin, pixel_x, pixel_y)
            }
            TransformGizmoMode::Rotate => self.pick_rotate_gizmo_axis(origin, pixel_x, pixel_y),
        }
    }

    fn pick_translate_gizmo_axis(
        &self,
        origin: GlamVec3,
        pixel_x: f32,
        pixel_y: f32,
    ) -> Option<usize> {
        let so = self.project_to_screen(origin)?;

        let center_dist = ((pixel_x - so.0).powi(2) + (pixel_y - so.1).powi(2)).sqrt();
        if center_dist < editor_viewport_controls::GIZMO_CENTER_PICK_RADIUS_PX {
            return Some(editor_viewport_controls::GIZMO_CENTER_AXIS);
        }

        let gizmo_scale = self.transform_gizmo_world_scale().unwrap_or(1.0);
        let len = rer_engine_shared::gizmo::axis_world_length(gizmo_scale);
        let axis_starts = self
            .transform_gizmo_axis_start_mesh()
            .unwrap_or([0.0, 0.0, 0.0]);
        const THRESH: f32 = 16.0;
        let dirs = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z];

        let mut best: Option<(f32, usize)> = None;
        for (i, &dir) in dirs.iter().enumerate() {
            let start_world = origin + dir * axis_starts[i] * gizmo_scale;
            if let (Some(base), Some(tip)) = (
                self.project_to_screen(start_world),
                self.project_to_screen(start_world + dir * len),
            ) {
                let d = point_to_segment_2d(pixel_x, pixel_y, base.0, base.1, tip.0, tip.1);
                if d < THRESH && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    fn pick_rotate_gizmo_axis(
        &self,
        origin: GlamVec3,
        pixel_x: f32,
        pixel_y: f32,
    ) -> Option<usize> {
        let gizmo_scale = self.transform_gizmo_rotation_world_scale().unwrap_or(1.0);
        let radius = crate::gizmo::GIZMO_ROTATION_RING_RADIUS * gizmo_scale;
        const THRESH: f32 = 18.0;
        const SEGMENTS: u32 = 48;

        let mut best: Option<(f32, usize)> = None;
        for axis in 0..3 {
            let mut prev_screen = None;
            for seg in 0..=SEGMENTS {
                let t = std::f32::consts::TAU * seg as f32 / SEGMENTS as f32;
                let local = crate::gizmo::rotation_ring_point(axis, radius, t);
                let world = origin + GlamVec3::from_array(local);
                if let Some(screen) = self.project_to_screen(world) {
                    if let Some((px, py)) = prev_screen {
                        let d = point_to_segment_2d(pixel_x, pixel_y, px, py, screen.0, screen.1);
                        if d < THRESH && best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, axis));
                        }
                    }
                    prev_screen = Some(screen);
                }
            }
        }
        best.map(|(_, i)| i)
    }

    pub(crate) fn is_entity_selected(&self, id: EntityId) -> bool {
        self.selected_entities.contains(&id) || self.selected_entity == Some(id)
    }

    pub(crate) fn selected_entity_at_pixel(&self, pixel_x: f32, pixel_y: f32) -> Option<EntityId> {
        let hit = self.ray_cast(pixel_x, pixel_y)?;
        if self.is_entity_selected(hit) {
            Some(hit)
        } else {
            None
        }
    }

    pub(crate) fn selected_entity_ids(&self) -> Vec<EntityId> {
        if !self.selected_entities.is_empty() {
            self.selected_entities.clone()
        } else {
            self.selected_entity.into_iter().collect()
        }
    }

    fn apply_selection_translation_delta(
        &mut self,
        selected_ids: &[EntityId],
        delta: GlamVec3,
        snap: bool,
        sync_camera_focus: bool,
    ) {
        if selected_ids.is_empty() || delta.length_squared() < 1e-12 {
            return;
        }

        let cell = if snap {
            self.grid_config.cell_size.max(0.05)
        } else {
            0.0
        };

        for &sel_id in selected_ids {
            if let Some(t) = self.world.get_mut::<Transform>(sel_id) {
                t.position += delta;
                if cell > 0.0 {
                    t.position = editor_viewport_controls::snap_vec3_to_grid(t.position, cell);
                }
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
                    let body_pos = physics_body_position_for_model_path(model_path, pos, half);
                    self.physics
                        .sync_entity_physics_from_transform(sel_id, body_pos, half);
                }
            }
        }

        if selected_ids.iter().any(|id| self.sun_entity == Some(*id)) {
            self.sync_directional_light_from_sun();
        }
        if !self.is_play_controller_active() && sync_camera_focus {
            self.sync_editor_camera_focus();
        }

        if let Some(player_id) = self.play_character_entity
            && selected_ids.contains(&player_id)
        {
            self.emit_play_character_view_changed(false);
        }

        let lead_id = self
            .selected_entity
            .or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id
            && self.world.get::<Transform>(sel_id).is_some()
        {
            self.send_entity_selected_event(sel_id);
        }

        self.handle_entity_attachment_after_transform(selected_ids);
        self.notify_reflection_probe_transform_changed(selected_ids);
    }

    pub(crate) fn apply_selection_rotation_from_snapshots(
        &mut self,
        pivot: GlamVec3,
        snapshots: &[crate::engine::types::EntityTransformSnapshot],
        delta_quat: glam::Quat,
    ) {
        if snapshots.is_empty() {
            return;
        }

        let selected_ids: Vec<EntityId> = snapshots.iter().map(|(id, _, _, _)| *id).collect();
        for &(id, start_pos, start_rot, start_scl) in snapshots {
            if self.editor_camera_entity == Some(id) {
                continue;
            }
            let start_pos = GlamVec3::from_array(start_pos);
            let start_rot =
                glam::Quat::from_xyzw(start_rot[0], start_rot[1], start_rot[2], start_rot[3]);
            let start_scale = GlamVec3::from_array(start_scl);
            let start_transform = Transform {
                position: start_pos,
                rotation: start_rot,
                scale: start_scale,
            };
            let start_visual = self.entity_world_pick_aabb(id, &start_transform).0;
            let model_path = self.entity_asset_path_for_bounds(id).unwrap_or_default();
            let bounds = self.resolve_entity_visual_local_bounds(id);
            let (new_pos, new_rot) = rotate_entity_transform_about_visual_center(
                &start_transform,
                start_visual,
                pivot,
                delta_quat,
                model_path.as_str(),
                bounds,
            );

            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = new_pos;
                t.rotation = new_rot;
            }
            if self.is_plane_wall_entity(id) && self.collider_entities.contains(&id) {
                self.sync_plane_wall_physics(id);
            } else if self.physics.has_physics(id) {
                self.sync_entity_physics_collider(id);
            }
            if self.sun_entity == Some(id) {
                self.sync_directional_light_from_sun();
            }
        }

        let lead_id = self
            .selected_entity
            .or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id
            && self.world.get::<Transform>(sel_id).is_some()
        {
            self.send_entity_selected_event(sel_id);
        }

        self.handle_entity_attachment_after_transform(&selected_ids);
        self.notify_reflection_probe_transform_changed(&selected_ids);
    }

    pub(crate) fn apply_selection_translation_from_snapshots(
        &mut self,
        snapshots: &[crate::engine::types::EntityTransformSnapshot],
        delta: GlamVec3,
        snap: bool,
    ) {
        if snapshots.is_empty() || delta.length_squared() < 1e-12 {
            return;
        }

        let cell = if snap {
            self.grid_config.cell_size.max(0.05)
        } else {
            0.0
        };

        let selected_ids: Vec<EntityId> = snapshots.iter().map(|(id, _, _, _)| *id).collect();
        for &(id, start_pos, start_rot, start_scl) in snapshots {
            if self.editor_camera_entity == Some(id) {
                continue;
            }
            let mut new_pos = GlamVec3::from_array(start_pos) + delta;
            if cell > 0.0 {
                new_pos = editor_viewport_controls::snap_vec3_to_grid(new_pos, cell);
            }
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = new_pos;
                t.rotation =
                    glam::Quat::from_xyzw(start_rot[0], start_rot[1], start_rot[2], start_rot[3]);
                t.scale = GlamVec3::from_array(start_scl);
            }
            if self.is_plane_wall_entity(id) && self.collider_entities.contains(&id) {
                self.sync_plane_wall_physics(id);
            } else if self.physics.has_physics(id) {
                self.sync_entity_physics_collider(id);
            }
            if self.sun_entity == Some(id) {
                self.sync_directional_light_from_sun();
            }
        }

        if let Some(player_id) = self.play_character_entity
            && selected_ids.contains(&player_id)
        {
            self.emit_play_character_view_changed(false);
        }

        let lead_id = self
            .selected_entity
            .or_else(|| selected_ids.last().copied());
        if let Some(sel_id) = lead_id
            && self.world.get::<Transform>(sel_id).is_some()
        {
            self.send_entity_selected_event(sel_id);
        }

        self.handle_entity_attachment_after_transform(&selected_ids);
        self.notify_reflection_probe_transform_changed(&selected_ids);
    }

    pub fn drag_entity_free(
        &mut self,
        pixel_x: f32,
        pixel_y: f32,
        plane_point: GlamVec3,
        plane_normal: GlamVec3,
        last_world: &mut GlamVec3,
        shift_held: bool,
        ctrl_held: bool,
    ) {
        let selected_ids = self.selected_entity_ids();
        if selected_ids.is_empty() {
            return;
        }
        let Some(hit) = self.free_drag_world_point(pixel_x, pixel_y, plane_point, plane_normal)
        else {
            return;
        };

        let mut delta = hit - *last_world;
        let max_delta = self.editor_viewport_drag_max_delta();
        if delta.length() > max_delta {
            // Resincronizar sin teletransportar (p. ej. tras perder frames de intersección).
            *last_world = hit;
            return;
        }

        if shift_held {
            delta *= editor_viewport_controls::DRAG_PRECISION_FACTOR;
            *last_world += delta;
        } else {
            *last_world = hit;
        }

        delta = editor_viewport_controls::clamp_drag_delta(delta, max_delta);

        self.apply_selection_translation_delta(&selected_ids, delta, ctrl_held, false);
    }

    pub fn drag_gizmo(
        &mut self,
        pixel_x: f32,
        pixel_y: f32,
        last_x: f32,
        last_y: f32,
        axis_idx: usize,
        shift_held: bool,
        ctrl_held: bool,
    ) {
        let selected_ids = self.selected_entity_ids();
        if selected_ids.is_empty() || axis_idx > 2 {
            return;
        }

        let origin = match self.selection_center() {
            Some(c) => c,
            None => return,
        };

        let w = self.size.width as f32;
        let h = self.size.height as f32;
        let aspect = w / h;

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
        let mut world_delta = (dx * ax + dy * ay) / (axis_len * axis_len);
        if shift_held {
            world_delta *= editor_viewport_controls::DRAG_PRECISION_FACTOR;
        }

        let axis_delta = axis_world * world_delta;
        let max_delta = self.editor_viewport_drag_max_delta();
        let clamped = editor_viewport_controls::clamp_drag_delta(axis_delta, max_delta);

        self.apply_selection_translation_delta(&selected_ids, clamped, ctrl_held, false);
    }

    pub fn drag_gizmo_rotate(
        &mut self,
        pivot: GlamVec3,
        snapshots: &[crate::engine::types::EntityTransformSnapshot],
        start_mouse: (f32, f32),
        current_mouse: (f32, f32),
        axis_idx: usize,
        plane_u: GlamVec3,
        plane_v: GlamVec3,
        shift_held: bool,
        ctrl_held: bool,
    ) {
        if snapshots.is_empty() || axis_idx > 2 {
            return;
        }
        let axis_world = [GlamVec3::X, GlamVec3::Y, GlamVec3::Z][axis_idx];
        let Some(mut angle) = self.rotation_angle_on_axis_plane(
            pivot,
            axis_world,
            start_mouse,
            current_mouse,
            plane_u,
            plane_v,
        ) else {
            return;
        };
        if shift_held {
            angle *= editor_viewport_controls::DRAG_PRECISION_FACTOR;
        }
        let mut delta = glam::Quat::from_axis_angle(axis_world, angle);
        if ctrl_held {
            delta = editor_viewport_controls::snap_rotation_quat(delta);
        }
        self.apply_selection_rotation_from_snapshots(pivot, snapshots, delta);
    }

    fn rotation_angle_on_axis_plane(
        &self,
        pivot: GlamVec3,
        axis: GlamVec3,
        start_mouse: (f32, f32),
        current_mouse: (f32, f32),
        plane_u: GlamVec3,
        plane_v: GlamVec3,
    ) -> Option<f32> {
        let (ro0, rd0) = self.viewport_ray(start_mouse.0, start_mouse.1)?;
        let (ro1, rd1) = self.viewport_ray(current_mouse.0, current_mouse.1)?;
        editor_viewport_controls::rotation_drag_angle(
            ro0, rd0, ro1, rd1, pivot, axis, plane_u, plane_v,
        )
    }

    pub fn toggle_transform_gizmo_mode(&mut self) {
        self.transform_gizmo_mode = self.transform_gizmo_mode.toggle();
        self.hovered_gizmo_axis = None;
        self.active_gizmo_axis = None;
        log::info!(
            "[gizmo] modo {}",
            match self.transform_gizmo_mode {
                TransformGizmoMode::Translate => "traslación",
                TransformGizmoMode::Rotate => "rotación",
            }
        );
    }

    pub fn update_hover(&mut self, pixel_x: f32, pixel_y: f32) {
        if self.player_ui_edit_active {
            return;
        }
        if self.socket_bone_pick_entity.is_some() {
            self.update_socket_bone_pick_hover(pixel_x, pixel_y);
            if self.hovered_entity.is_some() {
                self.hovered_entity = None;
                crate::ipc::send_event(&crate::ipc::EngineEvent::EntityUnhovered);
            }
            self.hovered_gizmo_axis = None;
            return;
        }
        if self.bone_physics_pick_entity.is_some() {
            self.update_bone_physics_pick_hover(pixel_x, pixel_y);
            if self.hovered_entity.is_some() {
                self.hovered_entity = None;
                crate::ipc::send_event(&crate::ipc::EngineEvent::EntityUnhovered);
            }
            self.hovered_gizmo_axis = None;
            return;
        }
        let prev_hover = self.hovered_entity;
        self.hovered_entity = self.ray_cast(pixel_x, pixel_y);
        self.hovered_gizmo_axis = self.pick_gizmo_axis(pixel_x, pixel_y);
        match (prev_hover, self.hovered_entity) {
            (None, Some(id)) => {
                crate::ipc::send_event(&crate::ipc::EngineEvent::EntityHovered { id })
            }
            (Some(_), None) => crate::ipc::send_event(&crate::ipc::EngineEvent::EntityUnhovered),
            (Some(a), Some(b)) if a != b => {
                crate::ipc::send_event(&crate::ipc::EngineEvent::EntityHovered { id: b })
            }
            _ => {}
        }
    }
}
