// ── Lógica exclusiva del modo 3D ─────────────────────────────────────────────
//
// Contiene:
//  · camera_3d        — Camera (órbita) + CameraUniform
//  · character_anchor / play_character / fps_camera / play_controller
//  · load_model       — carga un .glb/.gltf/.fbx y añade mallas a la escena
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
pub(crate) mod fbx_facing;
pub(crate) mod mesh_3d;
pub(crate) mod model_asset;
pub(crate) mod model_animation;
pub(crate) mod physics_3d;
pub(crate) mod quick_build;
pub(crate) mod static_model_cache;
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
        .is_some_and(|e| {
            e.eq_ignore_ascii_case("glb") || e.eq_ignore_ascii_case("gltf")
        })
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
    PLAY_CHARACTER_COLLIDER_RADIUS,
};
use crate::config_shared::point_to_segment_2d;
use crate::ecs::{EntityId, MeshComponent, NonSelectable, Transform};
use crate::entity_save_meta::EntitySaveMeta;
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
    /// Instancia un modelo 3D en la escena (reutiliza caché estática si existe).
    pub(crate) fn load_model(&mut self, path: &str, entity_category: Option<&str>) {
        if self.queue_load_model_if_preloading(path, entity_category, false) {
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
            self.spawn_model_from_cached_part(part.mesh_idx, part.tex_idx, &key, entity_category);
        }
        log::info!("Modelo instanciado desde caché: {key} ({count} malla/s)");
    }

    /// Sustituye el mesh visual de una entidad existente (mismo id, sin recrear entidad).
    pub(crate) fn replace_entity_model(&mut self, id: EntityId, path: &str) {
        if self.world.get::<Transform>(id).is_none() {
            send_event(&EngineEvent::Error {
                message: format!("Entidad {id} no encontrada para reemplazar modelo"),
            });
            return;
        }

        let is_play_character = self.play_character_entity == Some(id);
        let normalize = if is_play_character {
            Some(
                self.world
                    .get::<Transform>(id)
                    .map(|t| (t.scale.y * PLAY_CHARACTER_BODY_HEIGHT).max(0.1))
                    .unwrap_or(PLAY_CHARACTER_BODY_HEIGHT),
            )
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

    fn replace_entity_model_inner(
        &mut self,
        id: EntityId,
        path: &str,
        gltf_file: Option<model_asset::GltfFile>,
        is_play_character: bool,
        normalize: Option<f32>,
    ) {
        let path_buf = Path::new(path);
        let loaded = match (gltf_file.as_ref(), normalize) {
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
        let tex_idx = self.uv_rects.len();
        self.meshes.push(part.mesh);
        let uv = self
            .atlas
            .pack(&self.queue, &part.rgba, part.width, part.height);
        self.uv_rects.push(uv);

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
            if let Some(m) = self.save_registry.meta.get_mut(&id) {
                m.visual_model_path = Some(path.to_string());
            } else {
                self.save_registry.register_meta(
                    id,
                    EntitySaveMeta {
                        kind: "character".to_string(),
                        path: "[Player]".to_string(),
                        visual_model_path: Some(path.to_string()),
                        points: None,
                    },
                );
            }
            self.play_character_mesh_forward_xz = part.forward_xz;
            if is_fbx_model_path(path) {
                if let Some(skin_fwd) = model_asset::fbx_skinned_play_forward_xz(
                    Path::new(path),
                    PLAY_CHARACTER_BODY_HEIGHT,
                ) {
                    self.play_character_mesh_forward_xz = skin_fwd;
                }
            }
            let feet = self.play_character_feet_position();
            let w = PLAY_CHARACTER_COLLIDER_RADIUS * 2.0;
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                // La malla ya viene normalizada a BODY_HEIGHT; scale.y=1 evita doble altura.
                t.scale = glam::Vec3::new(w, 1.0, w);
                t.position = glam::Vec3::new(
                    feet.x,
                    feet.y + PLAY_CHARACTER_BODY_HEIGHT * 0.5,
                    feet.z,
                );
                let (_, _, yaw) = t.rotation.to_euler(glam::EulerRot::YXZ);
                t.rotation = glam::Quat::from_rotation_y(yaw);
            }
            self.sync_player_rotation_from_look();
            // El jugador FP usa solo la cápsula cinemática; el cuerpo Rapier estático bloquea queries.
            self.physics.remove_entity_body(id);
            self.emit_play_character_view_changed(false);
        } else {
            if let Some(m) = self.save_registry.meta.get_mut(&id) {
                m.path = path.to_string();
                m.visual_model_path = Some(path.to_string());
            }
        }
        if !is_play_character && self.physics.has_physics(id) {
            if let Some(t) = self.world.get::<Transform>(id) {
                let half = [
                    (t.scale.x * 0.5).max(0.01),
                    (t.scale.y * 0.5).max(0.01),
                    (t.scale.z * 0.5).max(0.01),
                ];
                let pos = t.position.to_array();
                let body_type = self.physics.get_body_type(id).to_string();
                self.physics
                    .set_entity_physics(id, true, &body_type, pos, half);
            }
        }

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

        // Forzar recarga del asset (orientación/normalize pueden cambiar entre versiones del motor).
        self.model_assets.remove(path);
        self.try_bind_model_animations_with_gltf(id, path, gltf_file.as_ref());

        if is_play_character && self.model_animation_bindings.contains_key(&id) {
            if let Some(asset) = self.model_assets.get(path) {
                if is_fbx_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_fbx_play_character_forward_xz(asset);
                } else if is_gltf_model_path(path) {
                    self.play_character_mesh_forward_xz =
                        model_asset::resolve_gltf_play_character_forward_xz(asset);
                }
                self.sync_player_rotation_from_look();
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
            let half = transform.scale * 0.5;
            if let Some(t) =
                Self::ray_intersects_aabb(ray_origin, world_dir, transform.position, half)
            {
                if closest.map_or(true, |(ct, _)| t < ct) {
                    closest = Some((t, entity));
                }
            }
        }
        closest.map(|(_, id)| id)
    }

    pub fn pick_entity(&mut self, pixel_x: f32, pixel_y: f32) {
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
                if self.physics.has_physics(sel_id) {
                    let half = [
                        (t.scale.x * 0.5).max(0.01),
                        (t.scale.y * 0.5).max(0.01),
                        (t.scale.z * 0.5).max(0.01),
                    ];
                    self.physics.sync_entity_physics_from_transform(
                        sel_id,
                        t.position.to_array(),
                        half,
                    );
                }
            }
        }

        if selected_ids
            .iter()
            .any(|id| self.sun_entity == Some(*id))
        {
            self.sync_directional_light_from_sun();
        }
        if !self.is_play_controller_active() && self.camera_2d.is_none() {
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
