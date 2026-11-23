// ── Escenas base 3D por código ────────────────────────────────────────────────

use crate::engine3d::config_3d::mesh_3d::GROUND_PLANE_MESH_EXTENT;
use crate::engine3d::config_3d::physics_3d::PhysicsWorld;
use crate::engine3d::config_3d::{Camera, WorldBounds3D};
use crate::engine3d::config_compat::ActiveTool;
use crate::engine3d::ecs::{EntityId, MeshComponent, NonSelectable, SurfacePbr, Transform};
use crate::engine3d::engine::State;
use crate::engine3d::entity_save_meta::EntitySaveMeta;
use crate::engine3d::ipc::{EngineEvent, send_event, send_load_progress};
use crate::engine3d::scripting::{ScriptEngine, ScriptEngineProfile};

impl State {
    pub(crate) fn send_model_loaded_event(&self, id: crate::engine3d::ecs::EntityId, name: &str) {
        let (position, scale) = match self.world.get::<Transform>(id) {
            Some(t) => (Some(t.position.to_array()), Some(t.scale.to_array())),
            None => (None, None),
        };
        send_event(&EngineEvent::ModelLoaded {
            id,
            name: Some(name.to_string()),
            position,
            scale,
            rotation: None,
            path: None,
            kind: None,
            blueprint_id: None,
            physics_enabled: None,
            physics_type: None,
            entity_category: None,
        });
    }

    pub(crate) fn register_entity_blueprint_id(&mut self, id: EntityId, blueprint_id: String) {
        self.entity_blueprint_ids.insert(id, blueprint_id);
    }

    pub(crate) fn send_entity_selected_event(&self, id: EntityId) {
        let name = self.world.name(id).unwrap_or("Entity").to_string();
        let transform = self.world.get::<Transform>(id).cloned().unwrap_or_default();
        let position = transform.position.to_array();
        let rotation = [
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
        ];
        let scale = transform.scale.to_array();
        let physics_enabled = self.physics.has_physics(id);
        let physics_type = self.physics.get_body_type(id).to_string();
        send_event(&EngineEvent::EntitySelected {
            id,
            name,
            position,
            rotation,
            scale,
            physics_enabled,
            physics_type,
            blueprint_id: self.entity_blueprint_ids.get(&id).cloned(),
            tooltip_id: self
                .world
                .get::<crate::engine3d::ecs::Interactable>(id)
                .and_then(|cfg| cfg.tooltip_id.clone()),
            interactable_enabled: self
                .world
                .get::<crate::engine3d::ecs::Interactable>(id)
                .is_some_and(|cfg| cfg.enabled),
        });
    }

    /// Limpieza compartida para cualquier escena base del binario 3D.
    pub(crate) fn reset_runtime_scene_3d(&mut self) {
        self.stop_audio_internal();
        self.physics = PhysicsWorld::new();
        self.world.clear();
        self.meshes.clear();
        self.tex_layers.clear();
        self.tex_layer_albedo.clear();
        self.editor_box_mesh_idx = None;
        self.editor_box_tex_idx = None;
        self.mat_val_label_mesh_idx = None;
        self.mat_val_texture_cache.clear();
        self.plane_tool_wall_mesh_idx = None;
        self.static_model_cache.clear();
        self.model_assets.clear();
        self.model_store.clear();
        self.imported_model_registry = crate::engine3d::assets::ImportedModelRegistry::default();
        self.model_preload_inflight.clear();
        self.model_preload_gpu_queue.clear();
        self.pending_load_models.clear();
        self.pending_entity_model_replaces.clear();
        let _ = self.texture_array.reset(&self.vulkan);
        self.worldgen.material_base = None;
        self.worldgen.fallback_tex_idx = None;
        self.worldgen.columns.clear();
        self.worldgen.entity_splat_tile.clear();
        self.worldgen.splat_atlas.reset();
        self.texture_path_layers.clear();
        self.glb_texture_catalog_cache.clear();
        self.entity_texture_effective_cap.clear();
        self.animations.clear();
        self.active_animations.clear();
        self.default_animation_by_entity.clear();
        self.anim_saved_transforms.clear();
        self.entity_facing_right.clear();
        self.script_snapshots.clear();
        self.script_anim_names.clear();
        self.scenario_entities.clear();
        self.character_entities.clear();
        self.collider_entities.clear();
        self.execution_area_entities.clear();
        self.execution_overlaps.clear();
        self.background_entity = None;
        self.background_path = None;
        self.save_registry.clear();
        self.selected_entity = None;
        self.selected_entities.clear();
        self.hovered_entity = None;
        self.hovered_gizmo_axis = None;
        self.active_gizmo_axis = None;
        self.ctrl_held = false;
        self.active_tool = ActiveTool::None;
        self.quick_build_ghost_id = None;
        self.plane_tool_ghost_id = None;
        self.plane_tool_preview_scale = None;
        self.quick_build_preview_path = None;
        self.quick_build_preview_kind = None;
        self.quick_build_preview_scale = None;
        self.quick_build_place_once = false;
        self.quick_build_spawn_kind = None;
        self.quick_build_blueprint = None;
        self.blueprint_registry.clear();
        self.entity_blueprint_ids.clear();
        self.entity_colision.clear();
        self.entity_hardness.clear();
        self.entity_attachments.clear();
        self.entity_sockets.clear();
        self.bone_index_cache.clear();
        self.socket_bone_pick_entity = None;
        self.socket_bone_pick_hovered_joint = None;
        self.bone_physics_editor_entity = None;
        self.bone_physics_pick_entity = None;
        self.bone_physics_pick_hovered_joint = None;
        self.entity_bone_physics.clear();
        self.entity_projectile_config.clear();
        self.active_projectiles.clear();
        self.bone_physics_sim.clear();
        self.bone_physics_entity_motion.clear();
        self.tool_overlay_buffer = crate::engine3d::gizmo::build_from_vertices(&self.vulkan, &[]);
        self.pivot_edit_mode = None;
        self.logical_area_mode = None;
        self.script_engine = ScriptEngine::new(ScriptEngineProfile::Engine3d)
            .expect("Error al reinicializar el motor de scripting Rhai");
        self.script_engine.clear_scene_script();
        self.control_bindings_by_entity.clear();
        self.clear_play_controller_script_frame();
        self.play_character_entity = None;
        self.editor_camera_entity = None;
        self.play_camera_eye_position = glam::Vec3::ZERO;
        self.play_camera_clip_radius =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_CAMERA_CLIP_RADIUS;
        self.play_camera_follow_mode =
            crate::engine3d::ipc::PlayCameraFollowMode::MoveWithCharacter;
        self.play_camera_follow_offset = glam::Vec3::ZERO;
        self.play_camera_follow_offset_local = glam::Vec3::ZERO;
        self.sun_entity = None;
        self.sun_icon_mesh_idx = None;
        self.sun_icon_tex_idx = None;
        self.probe_entity_slots.clear();
        self.last_probe_capture_ids = None;
        self.probe_capture_catchup = 0;
        self.directional_light_dir =
            crate::engine3d::config_3d::directional_light::DEFAULT_LIGHT_DIR.normalize();
        self.directional_light_color =
            crate::engine3d::config_3d::directional_light::DEFAULT_LIGHT_COLOR;
        self.directional_light_ambient =
            crate::engine3d::config_3d::directional_light::DEFAULT_LIGHT_AMBIENT;
        self.light_intensity =
            crate::engine3d::config_3d::directional_light::DEFAULT_LIGHT_INTENSITY;
        self.shadow_darkness =
            crate::engine3d::config_3d::directional_light::DEFAULT_SHADOW_DARKNESS;
        self.play_character_mesh_forward_xz = glam::Vec2::new(0.0, 1.0);
        self.clear_preview_editor_snapshots();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_applying_undo = false;
        self.world_bounds_3d = WorldBounds3D::default();
        self.sync_world_bounds_3d_runtime();
    }

    pub(crate) fn ground_entity_id(&self) -> Option<crate::engine3d::ecs::EntityId> {
        self.world
            .query::<crate::engine3d::ecs::NameComponent>()
            .find(|(_, c)| c.name.eq_ignore_ascii_case("ground"))
            .map(|(id, _)| id)
    }

    pub(crate) fn pack_scene_checker_texture(&mut self) -> usize {
        const S: u32 = 128;
        const TILE: u32 = 8;
        let mut px: Vec<u8> = Vec::with_capacity((S * S * 4) as usize);
        for y in 0..S {
            for x in 0..S {
                let light = (x / TILE + y / TILE).is_multiple_of(2);
                let (r, g, b): (u8, u8, u8) = if light { (58, 61, 80) } else { (30, 32, 48) };
                px.extend_from_slice(&[r, g, b, 255]);
            }
        }
        let checker_layer = self.texture_array.pack(&self.vulkan, &px, S, S).unwrap();
        let tex_idx = self.tex_layers.len();
        self.tex_layers.push(checker_layer);
        tex_idx
    }

    /// Suelo checker + colisión estática. Idempotente al restaurar desde el manifest `[Ground]`.
    pub(crate) fn ensure_ground_plane(&mut self) {
        if self.ground_entity_id().is_some() {
            return;
        }

        let ground_mesh_idx = self.meshes.len();
        let b = self.world_bounds_3d;
        let cell = self.grid_config.cell_size;
        self.meshes
            .push(crate::engine3d::config_3d::mesh_3d::create_ground_plane(
                &self.vulkan,
                b.width(),
                b.depth(),
                cell,
            ));
        let tex_idx = self.pack_scene_checker_texture();

        let plane_id = self.world.spawn(Some("Ground"));
        self.world.insert(
            plane_id,
            MeshComponent {
                mesh_idx: ground_mesh_idx,
                tex_idx,
            },
        );
        self.world.insert(plane_id, NonSelectable);
        // Suelo ligeramente reflectante para SSR (sin SurfacePbr → roughness 0.9, excluido del trace).
        self.world.insert(
            plane_id,
            SurfacePbr {
                roughness: 0.50,
                metallic: 0.0,
                ior: 0.0,
                opacity: 1.0,
            },
        );
        self.sync_ground_plane_to_world_bounds();
        self.physics.add_static_ground();
        self.save_registry.register_meta(
            plane_id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[Ground]".to_string(),
                visual_model_path: None,
                entity_category: None,
            },
        );
        self.send_model_loaded_event(plane_id, "Ground");
    }

    /// Quita el suelo checker (malla y semiespacio). Lo pide el generador al terminar
    /// si el usuario marcó reemplazar el mundo actual.
    pub(crate) fn remove_default_ground(&mut self) {
        self.physics.remove_static_ground();
        let Some(id) = self.ground_entity_id() else {
            return;
        };
        self.physics.remove_entity_body(id);
        self.save_registry.remove_entity(id);
        self.world.despawn(id);
        send_event(&EngineEvent::EntityRemoved {
            id,
            kind: "model".to_string(),
        });
    }

    pub(crate) fn spawn_ground_plane(&mut self, position: [f32; 3], _scale: [f32; 3]) {
        self.ensure_ground_plane();
        let Some(id) = self.ground_entity_id() else {
            return;
        };
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.position.y = 0.0;
        }
        self.sync_ground_plane_to_world_bounds();
    }

    /// Escala el mesh del suelo (40×40 local) a las caras del mundo.
    pub(crate) fn sync_ground_plane_to_world_bounds(&mut self) {
        let Some(id) = self.ground_entity_id() else {
            return;
        };
        if self.world.get::<SurfacePbr>(id).is_none() {
            self.world.insert(
                id,
                SurfacePbr {
                    roughness: 0.35,
                    metallic: 0.0,
                    ior: 0.0,
                    opacity: 1.0,
                },
            );
        }
        let b = self.world_bounds_3d;
        let s_x = (b.width() / GROUND_PLANE_MESH_EXTENT).max(0.01);
        let s_z = (b.depth() / GROUND_PLANE_MESH_EXTENT).max(0.01);
        let center = b.center();
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position.x = center.x;
            t.position.z = center.z;
            t.position.y = 0.0;
            t.scale = glam::Vec3::new(s_x, 0.02, s_z);
        }
        self.refresh_ground_checker_uv();
    }

    /// Reconstruye solo las UV del mesh del suelo para alinear el checker con `grid_config.cell_size`.
    pub(crate) fn refresh_ground_checker_uv(&mut self) {
        let Some(id) = self.ground_entity_id() else {
            return;
        };
        let Some(mesh_idx) = self
            .world
            .get::<crate::engine3d::ecs::MeshComponent>(id)
            .map(|mc| mc.mesh_idx)
        else {
            return;
        };
        let b = self.world_bounds_3d;
        let cell = self.grid_config.cell_size;
        self.meshes[mesh_idx] = crate::engine3d::config_3d::mesh_3d::create_ground_plane(
            &self.vulkan,
            b.width(),
            b.depth(),
            cell,
        );
    }

    /// Plantilla 3D por defecto (proyecto nuevo): suelo checker y cámara play character a ras de editor.
    pub(crate) fn setup_default_3d_scene(&mut self) {
        send_load_progress("Cargando plantilla 3D…", None, None);
        log::info!("Cargando plantilla 3D por defecto");
        self.reset_runtime_scene_3d();
        send_load_progress("Insertando suelo (Ground)", None, None);
        self.ensure_ground_plane();

        // Pista de obstáculos (parkour) rotada 90°: muro norte (z=14.4) y
        // laterales este/oeste (x=±6.75). Falta el frontal: se quita el muro
        // de z=5.4 para que el jugador, desde el centro mirando al norte
        // (yaw -π/2 → +Z), vea la pista sin obstrucción.
        send_load_progress("Insertando Paredes (3)", None, None);
        self.spawn_parkour_wall("ParkourWall · Norte", [0.0, 2.0, 14.4], [13.5, 4.0, 1.2]);
        self.spawn_parkour_wall("ParkourWall · Oeste", [-6.75, 2.0, 10.5], [1.2, 4.0, 9.0]);
        self.spawn_parkour_wall("ParkourWall · Este", [6.75, 2.0, 10.5], [1.2, 4.0, 9.0]);
        send_load_progress("Insertando Cilindros (3)", None, None);
        self.spawn_parkour_cylinder("ParkourCylinder · 1", [3.25, 0.75, 8.0], [1.5, 1.5, 1.5]);
        self.spawn_parkour_cylinder("ParkourCylinder · 2", [0.25, 1.25, 11.4], [2.0, 2.5, 2.0]);
        self.spawn_parkour_cylinder("ParkourCylinder · 3", [-4.25, 2.5, 9.9], [1.8, 5.0, 1.8]);

        // Límites lógicos del mundo: mínimo del motor, 30 m de lado (cara 15 m),
        // sincronizado con el generador y con la rosa de los vientos del editor.
        self.set_world_bounds_3d_radius(crate::engine3d::MIN_WORLD_RADIUS_3D);

        self.camera = Camera::new();
        // Jugador y cámara en el centro del mapa, con el mismo rumbo de siempre
        // (yaw -π/2 → +Z), mirando a la pista de obstáculos.
        let spawn_xz = (0.0_f32, 0.0_f32);
        let ground_y = self
            .physics
            .find_ground_y_at(spawn_xz.0, spawn_xz.1, 10.0, 20.0)
            .unwrap_or(0.0);
        self.camera.target = glam::Vec3::new(spawn_xz.0, ground_y, spawn_xz.1);
        self.camera.pitch =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_PITCH;
        self.camera.yaw =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_YAW;
        self.camera.distance =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE;
        self.editor_viewport_yaw = self.camera.yaw;
        self.editor_viewport_pitch = self.camera.pitch;
        self.editor_viewport_distance = self.camera.distance;
        self.clamp_play_character_camera_to_bounds();
        self.clear_color = [0.72, 0.86, 0.98, 1.0f32];

        send_load_progress("Insertando Character (Player)", None, None);
        let _ = crate::engine3d::assets::bundled::ensure_bundled_default_player_model(self);
        self.apply_3d_placeholder_sun_and_player();
        // Solo la demo que arranca sin `.save` siembra la fila de partículas.
        // Los proyectos `.save` restauran sus emisores desde el manifest.
        self.ensure_particle_demo_row();

        send_load_progress("Plantilla 3D lista", None, None);
        log::info!("Plantilla 3D por defecto lista");
        let scene_name = rer_engine_core::editor_defaults::default_scene_name(1);
        self.editor_scenes_init_from_boot(&scene_name);
        send_event(&EngineEvent::Ready {
            gravity: self.physics.gravity_magnitude(),
        });
    }

    pub(crate) fn apply_empty_3d_editor_defaults(&mut self) {
        self.set_world_bounds_3d_radius(crate::engine3d::MIN_WORLD_RADIUS_3D);
        self.camera = Camera::new();
        let spawn_xz = (0.0_f32, 5.0_f32);
        let ground_y = self
            .physics
            .find_ground_y_at(spawn_xz.0, spawn_xz.1, 10.0, 20.0)
            .unwrap_or(0.0);
        self.camera.target = glam::Vec3::new(spawn_xz.0, ground_y, spawn_xz.1);
        self.camera.pitch =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_PITCH;
        self.camera.yaw =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_YAW;
        self.camera.distance =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE;
        self.editor_viewport_yaw = self.camera.yaw;
        self.editor_viewport_pitch = self.camera.pitch;
        self.editor_viewport_distance = self.camera.distance;
        self.clamp_play_character_camera_to_bounds();
        self.clear_color = [0.72, 0.86, 0.98, 1.0f32];
    }

    /// Suelo checker + sol + jugador + pelota de prueba de la plantilla FP (sin muros ni cubos).
    pub(crate) fn apply_3d_placeholder_sun_and_player(&mut self) {
        self.ensure_ground_plane();
        self.ensure_default_sun();
        self.ensure_default_physics_ball();
        self.ensure_material_validation_demo();
        crate::engine3d::config_3d::interaction_demo::ensure_interaction_demo(self);
        if self.play_character_entity.is_none() {
            self.spawn_play_character();
        }
        self.ensure_default_3d_player_ui();
        self.sync_fps_camera_mode();
    }

    fn has_physics_ball(&self) -> bool {
        self.save_registry.meta.values().any(|m| {
            crate::engine3d::entity_save_meta::entity_path_marker(&m.path) == Some("[Ball]")
        })
    }

    fn ensure_default_physics_ball(&mut self) {
        if self.has_physics_ball() {
            return;
        }
        const RADIUS: f32 = 0.3;
        let position = [1.5_f32, RADIUS, 8.0];
        let diameter = RADIUS * 2.0;
        self.spawn_physics_ball("", position, [diameter, diameter, diameter], "dynamic");
    }

    pub(crate) fn is_reflection_probe_entity(&self, id: EntityId) -> bool {
        crate::engine3d::probe_registry::is_reflection_probe_entity(&self.save_registry, id)
    }

    /// Asigna ranura fija 0..MAX_PROBES-1; reutiliza la existente si ya estaba registrada.
    pub(crate) fn allocate_probe_slot(&mut self, id: EntityId) -> Option<usize> {
        crate::engine3d::probe_registry::allocate_probe_slot(&mut self.probe_entity_slots, id)
    }

    pub(crate) fn release_probe_entity_slot(&mut self, id: EntityId) {
        if crate::engine3d::probe_registry::release_probe_slot(&mut self.probe_entity_slots, id) {
            self.request_probe_capture_burst_if_reflections_active();
        }
    }

    /// Asegura ranura para cada probe registrado (cargas .save, plantilla FP, etc.).
    pub(crate) fn ensure_probe_slots_allocated(&mut self) {
        crate::engine3d::probe_registry::ensure_probe_slots_allocated(
            &self.save_registry,
            &mut self.probe_entity_slots,
        );
    }

    /// Probes activos: (entidad, centro, ranura cubemap). La ranura es estable por `EntityId`.
    pub(crate) fn reflection_probe_render_list(
        &self,
    ) -> Vec<(crate::engine3d::ecs::EntityId, glam::Vec3, usize)> {
        crate::engine3d::probe_registry::reflection_probe_render_list(
            &self.save_registry,
            &self.probe_entity_slots,
            |id| {
                self.world
                    .get::<crate::engine3d::ecs::Transform>(id)
                    .map(|t| t.position)
            },
        )
    }

    /// Sondas con centro, radio de influencia y ranura (para el IBL por instancia).
    pub(crate) fn reflection_probe_fields(
        &self,
    ) -> Vec<crate::engine3d::probe_registry::ProbeField> {
        crate::engine3d::probe_registry::reflection_probe_fields(
            &self.save_registry,
            &self.probe_entity_slots,
            |id| {
                self.world
                    .get::<crate::engine3d::ecs::Transform>(id)
                    .map(|t| (t.position, t.scale.x))
            },
        )
    }

    /// Si el conjunto de probes activos cambió, solicita burst de captura.
    pub(crate) fn sync_probe_capture_burst_for_entity_set(&mut self, probe_ids: &[EntityId]) {
        if crate::engine3d::probe_registry::sync_capture_burst_for_entity_set(
            &mut self.last_probe_capture_ids,
            probe_ids,
        ) {
            self.request_probe_capture_burst_if_reflections_active();
        }
    }

    /// AABB de pick/hover para probes sin malla (alineado al gizmo de editor).
    pub(crate) fn reflection_probe_pick_aabb(
        &self,
        transform: &crate::engine3d::ecs::Transform,
    ) -> (glam::Vec3, glam::Vec3) {
        let r = crate::engine3d::probe_registry::reflection_probe_editor_pick_half_extent();
        (transform.position, glam::Vec3::splat(r))
    }

    pub(crate) fn notify_reflection_probe_transform_changed(
        &mut self,
        ids: &[crate::engine3d::ecs::EntityId],
    ) {
        if ids.iter().any(|id| self.is_reflection_probe_entity(*id)) {
            self.request_probe_capture_burst_if_reflections_active();
        }
    }

    /// Esferas demo PBR (`[MatVal]`). Independiente del pipeline de reflejos / probes.
    pub(crate) fn ensure_material_validation_demo(&mut self) {
        crate::engine3d::config_3d::material_validation::ensure_material_comparison_demo(self);
    }

    /// Fila demo de partículas (x=10, paso 2 m en z desde -1) y letreros de pie.
    pub(crate) fn ensure_particle_demo_row(&mut self) {
        if self.particles.is_empty() {
            self.particles.seed_default_showcase();
        }
        crate::engine3d::config_3d::material_validation::ensure_particle_demo_labels(self);
    }

    /// Tras cargar escena FP placeholder (switch sin guardar): alinear sol, luz y cámara orbital del editor.
    pub(crate) fn finalize_3d_placeholder_editor_scene(&mut self) {
        use crate::engine3d::config_3d::character_anchor::{
            PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
        };
        use crate::engine3d::config_3d::directional_light::{
            DEFAULT_LIGHT_AMBIENT, DEFAULT_LIGHT_COLOR, DEFAULT_LIGHT_INTENSITY,
            DEFAULT_SHADOW_DARKNESS,
        };

        self.directional_light_color = DEFAULT_LIGHT_COLOR;
        self.apply_directional_light_settings(
            Some(DEFAULT_LIGHT_AMBIENT),
            Some(DEFAULT_LIGHT_INTENSITY),
            Some(DEFAULT_SHADOW_DARKNESS),
        );
        self.align_editor_sun_to_default_position();
        self.sync_fps_camera_mode();
        if !self.has_play_character() {
            return;
        }
        let feet = self.play_character_feet_position().to_array();
        let body_rotation = self.play_character_entity.and_then(|id| {
            self.world
                .get::<Transform>(id)
                .map(|t| [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w])
        });
        let body_scale = self
            .play_character_entity
            .and_then(|id| self.world.get::<Transform>(id).map(|t| t.scale.to_array()));
        self.apply_play_character_view(
            feet,
            PLAY_CHARACTER_EDITOR_ORBIT_YAW,
            PLAY_CHARACTER_EDITOR_ORBIT_PITCH,
            None,
            None,
            Some(crate::engine3d::ipc::PlayCameraFollowMode::MoveWithCharacter),
            body_rotation,
            body_scale,
            None,
            None,
            None,
        );
    }

    pub(crate) fn cancel_fp_baseline_defer(&mut self) {
        self.fp_baseline_defer_frames = 0;
    }

    pub(crate) fn tick_fp_baseline_defer(&mut self) {
        if self.fp_baseline_defer_frames == 0 {
            return;
        }
        self.fp_baseline_defer_frames -= 1;
        if self.fp_baseline_defer_frames == 0 {
            self.try_apply_3d_placeholder_sun_and_player();
        }
    }

    fn prune_stale_fp_entity_refs(&mut self) {
        if let Some(id) = self.play_character_entity
            && self
                .world
                .get::<crate::engine3d::ecs::Transform>(id)
                .is_none()
        {
            self.play_character_entity = None;
        }
        if let Some(id) = self.sun_entity
            && self
                .world
                .get::<crate::engine3d::ecs::Transform>(id)
                .is_none()
        {
            self.sun_entity = None;
        }
        if let Some(id) = self.editor_camera_entity
            && self
                .world
                .get::<crate::engine3d::ecs::Transform>(id)
                .is_none()
        {
            self.editor_camera_entity = None;
        }
    }

    fn try_apply_3d_placeholder_sun_and_player(&mut self) {
        self.prune_stale_fp_entity_refs();
        if self.play_character_entity.is_some() || self.sun_entity.is_some() {
            return;
        }
        if !self.scenario_entities.is_empty() || !self.character_entities.is_empty() {
            return;
        }
        log::info!("Escena FP vacía: insertando suelo, sol y jugador placeholder");
        self.apply_3d_placeholder_sun_and_player();
        self.cancel_fp_baseline_defer();
    }

    /// Arranque 3D al abrir `.save`: el ECS ya viene vacío de `State::new`; sin plantilla FP ni suelo.
    pub(crate) fn setup_empty_3d(&mut self) {
        self.mount_save_on_empty_world = true;
        log::info!("Motor 3D listo — montará escena desde .save (sin plantilla)");
    }

    /// Vacía entidades y estado de juego sin resetear GPU, texturas ni caché de modelos precargados.
    pub(crate) fn clear_scene_entities_for_save_load(&mut self) {
        self.stop_audio_internal();
        self.physics = PhysicsWorld::new();
        self.world.clear();
        self.pending_load_models.clear();
        self.pending_entity_model_replaces.clear();
        self.animations.clear();
        self.active_animations.clear();
        self.default_animation_by_entity.clear();
        self.anim_saved_transforms.clear();
        self.entity_facing_right.clear();
        self.script_snapshots.clear();
        self.script_anim_names.clear();
        self.scenario_entities.clear();
        self.character_entities.clear();
        self.collider_entities.clear();
        self.execution_area_entities.clear();
        self.execution_overlaps.clear();
        self.background_entity = None;
        self.background_path = None;
        self.save_registry.clear();
        self.selected_entity = None;
        self.selected_entities.clear();
        self.hovered_entity = None;
        self.hovered_gizmo_axis = None;
        self.active_gizmo_axis = None;
        self.ctrl_held = false;
        self.active_tool = ActiveTool::None;
        self.quick_build_ghost_id = None;
        self.plane_tool_ghost_id = None;
        self.plane_tool_preview_scale = None;
        self.quick_build_preview_path = None;
        self.quick_build_preview_kind = None;
        self.quick_build_preview_scale = None;
        self.quick_build_place_once = false;
        self.quick_build_spawn_kind = None;
        self.quick_build_blueprint = None;
        self.blueprint_registry.clear();
        self.entity_blueprint_ids.clear();
        self.entity_colision.clear();
        self.entity_hardness.clear();
        self.entity_attachments.clear();
        self.entity_sockets.clear();
        self.bone_index_cache.clear();
        self.socket_bone_pick_entity = None;
        self.socket_bone_pick_hovered_joint = None;
        self.bone_physics_editor_entity = None;
        self.bone_physics_pick_entity = None;
        self.bone_physics_pick_hovered_joint = None;
        self.entity_bone_physics.clear();
        self.entity_projectile_config.clear();
        self.active_projectiles.clear();
        self.bone_physics_sim.clear();
        self.bone_physics_entity_motion.clear();
        self.tool_overlay_buffer = crate::engine3d::gizmo::build_from_vertices(&self.vulkan, &[]);
        self.pivot_edit_mode = None;
        self.logical_area_mode = None;
        self.control_bindings_by_entity.clear();
        self.clear_play_controller_script_frame();
        self.play_character_entity = None;
        self.editor_camera_entity = None;
        self.play_camera_eye_position = glam::Vec3::ZERO;
        self.play_camera_clip_radius =
            crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_CAMERA_CLIP_RADIUS;
        self.play_camera_follow_mode =
            crate::engine3d::ipc::PlayCameraFollowMode::MoveWithCharacter;
        self.play_camera_follow_offset = glam::Vec3::ZERO;
        self.play_camera_follow_offset_local = glam::Vec3::ZERO;
        self.particles.clear();
        self.hovered_emitter = None;
        self.water_clear();
        self.sun_entity = None;
        self.sun_icon_mesh_idx = None;
        self.sun_icon_tex_idx = None;
        self.probe_entity_slots.clear();
        self.last_probe_capture_ids = None;
        self.probe_capture_catchup = 0;
        self.clear_preview_editor_snapshots();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_applying_undo = false;
    }

    /// Mesh + textura 1×1 compartidos para cajas del editor (`[EditorBox]`).
    pub(crate) fn ensure_editor_box_gpu_assets(&mut self) -> (usize, usize) {
        if let (Some(mesh_idx), Some(tex_idx)) = (self.editor_box_mesh_idx, self.editor_box_tex_idx)
        {
            return (mesh_idx, tex_idx);
        }
        let mesh_idx = self.meshes.len();
        self.meshes
            .push(crate::engine3d::mesh::create_cube(&self.vulkan));
        let white_px = [255u8, 255, 255, 255];
        let tex_idx = self.tex_layers.len();
        let block_layer = self
            .texture_array
            .pack(&self.vulkan, &white_px, 1, 1)
            .unwrap();
        self.tex_layers.push(block_layer);
        self.editor_box_mesh_idx = Some(mesh_idx);
        self.editor_box_tex_idx = Some(tex_idx);
        (mesh_idx, tex_idx)
    }

    /// Carpeta `assets` del motor (dev: `Engine/assets`; empaquetado: `…/assets`).
    /// Se busca subiendo desde el ejecutable: los tests corren desde
    /// `target/debug/deps`, así que la profundidad exacta no es fija.
    fn resolve_engine_assets_dir() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        for dir in exe.ancestors().take(6) {
            let assets = dir.join("assets");
            if assets.join("Brick_Wall_BaseColor.jpg").is_file() {
                return Some(assets);
            }
        }
        // Los binarios de test pueden vivir fuera del árbol del workspace.
        let manifest_assets =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if manifest_assets.join("Brick_Wall_BaseColor.jpg").is_file() {
            return Some(manifest_assets);
        }
        None
    }

    /// Sube `Brick_Wall_BaseColor.jpg` al array de texturas y devuelve su índice.
    pub(crate) fn ensure_brick_texture(&mut self) -> Option<usize> {
        if let Some(idx) = self.parkour_wall_tex_idx {
            return Some(idx);
        }
        // Tras «Limpiar escena» se descarta el índice, pero no la capa GPU:
        // la reutilizamos en vez de empaquetar la misma imagen otra vez.
        let layer = if let Some(layer) = self.parkour_wall_layer {
            layer
        } else {
            let path = Self::resolve_engine_assets_dir()?.join("Brick_Wall_BaseColor.jpg");
            let img = image::open(&path).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            let layer = self
                .texture_array
                .pack(&self.vulkan, img.as_raw(), w, h)
                .ok()?;
            self.parkour_wall_layer = Some(layer);
            layer
        };
        let idx = self
            .tex_layers
            .iter()
            .position(|&l| l == layer)
            .unwrap_or_else(|| {
                self.tex_layers.push(layer);
                self.tex_layers.len() - 1
            });
        self.parkour_wall_tex_idx = Some(idx);
        Some(idx)
    }

    /// Sube `FootBall_BaseColor.jpg` al array de texturas y devuelve su índice.
    pub(crate) fn ensure_football_texture(&mut self) -> Option<usize> {
        if let Some(idx) = self.ball_tex_idx {
            return Some(idx);
        }
        let layer = if let Some(layer) = self.ball_layer {
            layer
        } else {
            let path = Self::resolve_engine_assets_dir()?.join("FootBall_BaseColor.jpg");
            let img = image::open(&path).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            let layer = self
                .texture_array
                .pack(&self.vulkan, img.as_raw(), w, h)
                .ok()?;
            self.ball_layer = Some(layer);
            layer
        };
        let idx = self
            .tex_layers
            .iter()
            .position(|&l| l == layer)
            .unwrap_or_else(|| {
                self.tex_layers.push(layer);
                self.tex_layers.len() - 1
            });
        self.ball_tex_idx = Some(idx);
        Some(idx)
    }

    /// Malla cilíndrica compartida de los obstáculos.
    fn ensure_parkour_cylinder_mesh(&mut self) -> usize {
        if let Some(idx) = self.parkour_cylinder_mesh_idx {
            return idx;
        }
        let idx = self.meshes.len();
        self.meshes
            .push(crate::engine3d::mesh::create_cylinder(&self.vulkan, 20));
        self.parkour_cylinder_mesh_idx = Some(idx);
        idx
    }

    /// Cilindro de la pista de parkour: misma caja y física que una caja del editor.
    pub(crate) fn spawn_parkour_cylinder(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
    ) -> EntityId {
        let label = self
            .resolve_entity_display_name(name, rer_engine_core::editor_defaults::entity_label::BOX);
        let mesh_idx = self.ensure_parkour_cylinder_mesh();
        let tex_idx = self.ensure_editor_box_gpu_assets().1;
        let id = self.world.spawn(Some(&label));
        self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.scale = glam::Vec3::from_array(scale);
        }
        let half = [scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5];
        self.physics
            .set_entity_physics(id, true, "static", position, half);
        self.entity_colision.insert(id, true);
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[ParkourCylinder]".to_string(),
                visual_model_path: None,
                entity_category: Some("object".to_string()),
            },
        );
        self.send_model_loaded_event(id, &label);
        self.push_remove_entity_undo(id);
        id
    }

    /// Muro de la pista de parkour: caja del editor con textura de ladrillo.
    pub(crate) fn spawn_parkour_wall(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
    ) -> EntityId {
        let label = self
            .resolve_entity_display_name(name, rer_engine_core::editor_defaults::entity_label::BOX);
        let (mesh_idx, _) = self.ensure_editor_box_gpu_assets();
        let tex_idx = self.ensure_brick_texture().unwrap_or_else(|| {
            log::warn!("[parkour] textura de ladrillo no disponible; se usa el blanco");
            self.ensure_editor_box_gpu_assets().1
        });
        let id = self.world.spawn(Some(&label));
        self.world.insert(id, MeshComponent { mesh_idx, tex_idx });
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.scale = glam::Vec3::from_array(scale);
        }
        let half = [scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5];
        self.physics
            .set_entity_physics(id, true, "static", position, half);
        self.entity_colision.insert(id, true);
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[ParkourWall]".to_string(),
                visual_model_path: None,
                entity_category: Some("environment".to_string()),
            },
        );
        self.send_model_loaded_event(id, &label);
        self.push_remove_entity_undo(id);
        id
    }

    /// Cubo del editor (muros/cajas de plantilla) sin archivo `.glb`.
    pub(crate) fn spawn_editor_box(
        &mut self,
        name: &str,
        position: [f32; 3],
        scale: [f32; 3],
    ) -> EntityId {
        let label = self
            .resolve_entity_display_name(name, rer_engine_core::editor_defaults::entity_label::BOX);
        let (block_mesh_idx, block_tex_idx) = self.ensure_editor_box_gpu_assets();

        let id = self.world.spawn(Some(&label));
        self.world.insert(
            id,
            MeshComponent {
                mesh_idx: block_mesh_idx,
                tex_idx: block_tex_idx,
            },
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.scale = glam::Vec3::from_array(scale);
        }
        let half = [scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5];
        self.physics
            .set_entity_physics(id, true, "static", position, half);
        self.entity_colision.insert(id, true);
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[EditorBox]".to_string(),
                visual_model_path: None,
                entity_category: None,
            },
        );
        self.send_model_loaded_event(id, &label);
        self.push_remove_entity_undo(id);
        id
    }

    pub(crate) fn load_character(&mut self, path: &str) {
        let is_player = path == "[Player]" || path.ends_with("[Player]");
        if !is_player {
            log::error!(
                "[load_character] en 3D solo aplica a [Player]; use load_model para modelos: {path}"
            );
            return;
        }
        if let Some(id) = self.play_character_entity
            && self.world.get::<Transform>(id).is_some()
        {
            log::info!("[load_character] jugador ya existe (id={id}); no se crea entidad nueva");
            send_event(&EngineEvent::CharacterLoaded {
                id,
                path: path.to_string(),
            });
            return;
        }
        log::info!("[load_character] creando jugador con mesh empaquetado de la demo");
        self.spawn_play_character();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_textures_are_available_and_decodable() {
        let dir = State::resolve_engine_assets_dir()
            .unwrap_or_else(|| panic!("no se encontró la carpeta assets del motor"));
        for name in ["Brick_Wall_BaseColor.jpg", "FootBall_BaseColor.jpg"] {
            let path = dir.join(name);
            let img = image::open(&path)
                .unwrap_or_else(|error| panic!("textura ilegible {}: {error}", path.display()))
                .to_rgba8();
            let (w, h) = img.dimensions();
            assert!(w > 0 && h > 0, "{name} vacía");
            assert_eq!(img.as_raw().len(), (w * h * 4) as usize, "{name} RGBA");
        }
    }
}
