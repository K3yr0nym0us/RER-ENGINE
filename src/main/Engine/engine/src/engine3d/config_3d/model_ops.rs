//! Carga y reemplazo de modelos, física derivada de la malla y AABB de picking.

use super::*;

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
        crate::engine3d::entity_save_meta::entity_path_marker(&meta.path) == Some("[Ball]")
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
        if crate::engine3d::entity_save_meta::entity_path_marker(path).is_some() {
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

        if crate::engine3d::entity_save_meta::entity_category_uses_character_capsule(category) {
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

        if crate::engine3d::entity_save_meta::entity_category_uses_mesh_collision(category) {
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
    pub(crate) fn replace_entity_model_from_static_cache(
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
            crate::engine3d::config_3d::static_model_cache::play_character_cache_key(&cache_key)
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

    pub(crate) fn replace_entity_model_inner(
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
                match mesh_3d::load_gltf_preview_from_file(&self.vulkan, file, extent) {
                    Ok(parts) => parts,
                    Err(e) => {
                        send_event(&EngineEvent::Error { message: e });
                        return;
                    }
                }
            }
            _ => match mesh_3d::load_model_file(&self.vulkan, path_buf, normalize) {
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
            .pack(&self.vulkan, &part.rgba, part.width, part.height)
            .unwrap();
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
                    crate::engine3d::config_3d::model_asset_fbx::fbx_skinned_play_forward_xz(
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
                crate::engine3d::config_3d::character_anchor::PlayCharacterMeshExtents::from_local_bounds(
                    part.local_bounds.0,
                    part.local_bounds.1,
                ),
            );
            self.model_assets.remove(
                &crate::engine3d::config_3d::static_model_cache::model_asset_cache_key(
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
            let play_key = crate::engine3d::config_3d::static_model_cache::play_character_cache_key(
                &asset_key,
            );
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.static_model_cache.entry(play_key)
            {
                e.insert(vec![
                    crate::engine3d::config_3d::static_model_cache::CachedStaticModelPart {
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
                &crate::engine3d::config_3d::static_model_cache::model_asset_cache_key(
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
            let play_key = crate::engine3d::config_3d::static_model_cache::play_character_cache_key(
                &asset_key,
            );
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.static_model_cache.entry(play_key)
            {
                e.insert(vec![
                    crate::engine3d::config_3d::static_model_cache::CachedStaticModelPart {
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

    pub(crate) fn ray_intersects_aabb(
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
}
