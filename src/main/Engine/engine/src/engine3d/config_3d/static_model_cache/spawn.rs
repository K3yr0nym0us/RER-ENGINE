//! Spawn y reemplazo de entidades desde el caché, invalidación y limpieza.

use super::*;

impl State {
    pub(crate) fn cached_static_model_parts(&self, path: &str) -> Option<&[CachedStaticModelPart]> {
        let key = self.model_cache_key(path);
        self.static_model_cache.get(&key).map(|v| v.as_slice())
    }

    pub(crate) fn default_model_spawn_position(&self) -> [f32; 3] {
        let forward = self.camera.view_forward();
        let spawn = self.camera.target + forward * 2.5;
        [spawn.x, spawn.y.max(0.0), spawn.z]
    }

    /// Misma lógica que el acordeón Entidades (`load_model` → primera malla en caché),
    /// con transform y metadatos explícitos.
    pub(crate) fn spawn_cached_model_part_at(
        &mut self,
        mesh_idx: usize,
        tex_idx: usize,
        path: &str,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
        entity_name: &str,
        kind: &str,
        blueprint_id: Option<String>,
        entity_category: Option<String>,
        physics_enabled: bool,
        physics_type: &str,
        local_bounds: ([f32; 3], [f32; 3]),
        desired_id: Option<EntityId>,
        roughness: f32,
        metallic: f32,
        ior: f32,
        track_undo: bool,
    ) -> EntityId {
        let key = self.model_path_key(path);
        let resolved_category = entity_category.clone().or_else(|| {
            if kind == "character" {
                Some("character".to_string())
            } else {
                None
            }
        });
        let id = if let Some(desired) = desired_id.filter(|&d| d != 0) {
            if self.world.spawn_with_id(desired, Some(entity_name)) {
                desired
            } else {
                log::warn!("[restore] id guardado {desired} en uso; generando id nuevo");
                self.world.spawn(Some(entity_name))
            }
        } else {
            self.world.spawn(Some(entity_name))
        };
        self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        if roughness >= 0.0 {
            self.world.insert(
                id,
                SurfacePbr {
                    roughness,
                    metallic,
                    ior,
                    opacity: 1.0,
                },
            );
        }
        if kind == "character" && !self.character_entities.contains(&id) {
            self.character_entities.push(id);
        }
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.rotation = glam::Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
            t.scale = glam::Vec3::from_array(scale);
        }
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: kind.to_string(),
                path: key.clone(),
                visual_model_path: None,
                entity_category: resolved_category.clone(),
            },
        );
        if physics_enabled && self.play_character_entity != Some(id) {
            if let Some(t) = self.world.get::<Transform>(id).cloned() {
                let half =
                    physics_half_extents_for_model(t.scale.abs().to_array(), Some(local_bounds));
                let body_pos = physics_body_world_center(&t, Some(local_bounds), path, half);
                self.physics
                    .set_entity_physics(id, true, physics_type, body_pos, half);
            }
            send_event(&EngineEvent::PhysicsChanged {
                entity_id: id,
                enabled: true,
                body_type: physics_type.to_string(),
            });
        }
        if let Some(bp) = blueprint_id.clone() {
            self.register_entity_blueprint_id(id, bp);
        }
        if self.model_needs_skinned_bind(&key) {
            self.try_bind_model_animations_with_gltf(id, &key, None);
        }
        if track_undo {
            self.push_remove_entity_undo(id);
        }
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(entity_name.to_string()),
            position: Some(position),
            scale: Some(scale),
            rotation: Some(rotation),
            path: Some(key),
            kind: Some(kind.to_string()),
            blueprint_id,
            physics_enabled: Some(physics_enabled),
            physics_type: Some(physics_type.to_string()),
            entity_category: resolved_category,
        });
        id
    }

    pub(crate) fn spawn_cached_model_from_save(
        &mut self,
        path: &str,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
        entity_name: Option<&str>,
        entity_category: Option<String>,
        blueprint_id: Option<String>,
        physics_enabled: bool,
        physics_type: &str,
        desired_id: Option<EntityId>,
        track_undo: bool,
    ) -> Result<EntityId, String> {
        self.ensure_static_model_cached(path)?;
        let Some(part) = self
            .cached_static_model_parts(path)
            .and_then(|parts| parts.first())
            .copied()
        else {
            let key = self.model_cache_key(path);
            return Err(format!("Modelo vacío: {key}"));
        };
        let kind = if entity_category.as_deref() == Some("character") {
            "character"
        } else {
            "model"
        };
        let name = entity_name
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| {
                self.next_numbered_entity_name(entity_label_for_spawn(
                    kind,
                    entity_category.as_deref(),
                ))
            });
        Ok(self.spawn_cached_model_part_at(
            part.mesh_idx,
            part.tex_idx,
            path,
            position,
            rotation,
            scale,
            &name,
            kind,
            blueprint_id,
            entity_category,
            physics_enabled,
            physics_type,
            part.local_bounds,
            desired_id,
            part.roughness,
            part.metallic,
            part.ior,
            track_undo,
        ))
    }

    pub(crate) fn spawn_model_from_cached_part(
        &mut self,
        part: CachedStaticModelPart,
        path: &str,
        entity_category: Option<&str>,
        kind: &str,
    ) -> EntityId {
        let label = self.next_numbered_entity_name(entity_label_for_spawn(kind, entity_category));
        let desired = glam::Vec3::from_array(self.default_model_spawn_position());
        let rotation = glam::Quat::IDENTITY;
        let scale = glam::Vec3::ONE;
        let key = self.model_path_key(path);
        let position = transform_position_for_visual_center(
            desired,
            rotation,
            scale,
            &key,
            Some(part.local_bounds),
        )
        .to_array();
        self.spawn_cached_model_part_at(
            part.mesh_idx,
            part.tex_idx,
            path,
            position,
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            &label,
            kind,
            None,
            entity_category.map(str::to_string),
            false,
            "static",
            part.local_bounds,
            None,
            part.roughness,
            part.metallic,
            part.ior,
            true,
        )
    }

    pub(crate) fn invalidate_static_model_cache(&mut self, path: &str) {
        let key = self.model_cache_key(path);
        self.static_model_cache.remove(&key);
        self.static_model_cache
            .remove(&play_character_cache_key(&key));
        self.model_assets.remove(&key);
        self.model_preload_inflight.remove(&key);
        if let Some(source) = self.imported_model_registry.get(&key) {
            let source_key = self.model_path_key(&source.source_path);
            self.model_preload_inflight.remove(&source_key);
        }
    }

    /// Elimina un modelo de la biblioteca Resources (`model_store` + registro importado + caché GPU).
    /// Devuelve la clave canónica (`model_id`) si se eliminó algo.
    pub(crate) fn remove_model_from_library(&mut self, path: &str) -> Option<String> {
        let key = self.model_path_key(path);
        let model_id = if key.starts_with("model_") {
            Some(key.clone())
        } else {
            self.imported_model_registry.model_id_for_path(&key)
        };

        let store_keys: Vec<String> = self
            .model_store
            .iter()
            .filter(|(store_key, entry)| {
                **store_key == key
                    || entry
                        .model_id
                        .as_deref()
                        .is_some_and(|id| model_id.as_deref() == Some(id))
            })
            .map(|(store_key, _)| store_key.clone())
            .collect();

        if store_keys.is_empty() && model_id.is_none() {
            return None;
        }

        if let Some(ref id) = model_id {
            self.imported_model_registry.remove(id);
            self.invalidate_static_model_cache(id);
        }

        let canonical = model_id
            .or_else(|| store_keys.iter().find(|k| k.starts_with("model_")).cloned())
            .unwrap_or_else(|| key.clone());

        for store_key in store_keys {
            self.invalidate_static_model_cache(&store_key);
            self.model_store.remove(&store_key);
            self.model_preload_gpu_queue.retain(|p| p.path != store_key);
            self.drop_pending_load_models_for_path(&store_key);
        }

        Some(canonical)
    }
}
