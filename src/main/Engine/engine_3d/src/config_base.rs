// ── Escenas base 3D por código ────────────────────────────────────────────────

use crate::config_3d::physics_3d::PhysicsWorld;
use crate::config_3d::mesh_3d::GROUND_PLANE_MESH_EXTENT;
use crate::config_3d::{Camera, WorldBounds3D};
use crate::config_compat::ActiveTool;
use crate::ecs::{EntityId, MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::gizmo;
use crate::ipc::{send_event, send_load_progress, EngineEvent};
use crate::entity_save_meta::EntitySaveMeta;
use crate::mesh;
use crate::scripting::{ScriptEngine, ScriptEngineProfile};

impl State {
    pub(crate) fn send_model_loaded_event(&self, id: crate::ecs::EntityId, name: &str) {
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
        let transform = self
            .world
            .get::<Transform>(id)
            .cloned()
            .unwrap_or_default();
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
        });
    }

    /// Limpieza compartida para cualquier escena base del binario 3D.
    pub(crate) fn reset_runtime_scene_3d(&mut self) {
        self.stop_audio_internal();
        self.physics = PhysicsWorld::new();
        self.world.clear();
        self.meshes.clear();
        self.tex_layers.clear();
        self.editor_box_mesh_idx = None;
        self.editor_box_tex_idx = None;
        self.plane_tool_wall_mesh_idx = None;
        self.static_model_cache.clear();
        self.model_assets.clear();
        self.model_store.clear();
        self.imported_model_registry = crate::assets::ImportedModelRegistry::default();
        self.model_preload_inflight.clear();
        self.model_preload_gpu_queue.clear();
        self.pending_load_models.clear();
        self.pending_entity_model_replaces.clear();
        self.texture_array.reset(&self.queue);
        self.texture_path_layers.clear();
        self.glb_texture_catalog_cache.clear();
        self.entity_texture_effective_cap.clear();
        self.reload_screen_hud_images();
        self.animations.clear();
        self.active_animations.clear();
        self.default_animation_by_entity.clear();
        self.anim_saved_transforms.clear();
        self.entity_facing_right.clear();
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
        self.quick_build_blueprint = None;
        self.blueprint_registry.clear();
        self.entity_blueprint_ids.clear();
        self.entity_colision.clear();
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
        self.fps_exit_hint_alpha = 0.0;
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
        self.play_camera_follow_mode = crate::ipc::PlayCameraFollowMode::MoveWithCharacter;
        self.play_camera_follow_offset = glam::Vec3::ZERO;
        self.play_camera_follow_offset_local = glam::Vec3::ZERO;
        self.sun_entity = None;
        self.sun_icon_mesh_idx = None;
        self.sun_icon_tex_idx = None;
        self.directional_light_dir =
            crate::config_3d::directional_light::DEFAULT_LIGHT_DIR.normalize();
        self.directional_light_color =
            crate::config_3d::directional_light::DEFAULT_LIGHT_COLOR;
        self.directional_light_ambient =
            crate::config_3d::directional_light::DEFAULT_LIGHT_AMBIENT;
        self.light_intensity = crate::config_3d::directional_light::DEFAULT_LIGHT_INTENSITY;
        self.shadow_darkness = crate::config_3d::directional_light::DEFAULT_SHADOW_DARKNESS;
        self.play_character_mesh_forward_xz = glam::Vec2::new(0.0, 1.0);
        self.clear_preview_editor_snapshots();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_applying_undo = false;
        self.world_bounds_3d = WorldBounds3D::default();
        self.sync_world_bounds_3d_runtime();
    }

    pub(crate) fn ground_entity_id(&self) -> Option<crate::ecs::EntityId> {
        self.world
            .query::<crate::ecs::NameComponent>()
            .find(|(_, c)| c.name.eq_ignore_ascii_case("ground"))
            .map(|(id, _)| id)
    }

    fn pack_scene_checker_texture(&mut self) -> usize {
        const S: u32 = 128;
        const TILE: u32 = 8;
        let mut px: Vec<u8> = Vec::with_capacity((S * S * 4) as usize);
        for y in 0..S {
            for x in 0..S {
                let light = ((x / TILE + y / TILE) % 2) == 0;
                let (r, g, b): (u8, u8, u8) = if light {
                    (58, 61, 80)
                } else {
                    (30, 32, 48)
                };
                px.extend_from_slice(&[r, g, b, 255]);
            }
        }
        let checker_layer = self.texture_array.pack(&self.queue, &px, S, S);
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
        self.meshes.push(crate::config_3d::mesh_3d::create_ground_plane(
            &self.device,
            b.width,
            b.depth,
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

    /// Escala el mesh del suelo (40×40 local) al cuadro de límites del accordion World.
    pub(crate) fn sync_ground_plane_to_world_bounds(&mut self) {
        let Some(id) = self.ground_entity_id() else {
            return;
        };
        let b = self.world_bounds_3d;
        let sx = (b.width / GROUND_PLANE_MESH_EXTENT).max(0.01);
        let sz = (b.depth / GROUND_PLANE_MESH_EXTENT).max(0.01);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position.x = 0.0;
            t.position.z = 0.0;
            t.position.y = 0.0;
            t.scale = glam::Vec3::new(sx, 0.02, sz);
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
            .get::<crate::ecs::MeshComponent>(id)
            .map(|mc| mc.mesh_idx)
        else {
            return;
        };
        let b = self.world_bounds_3d;
        let cell = self.grid_config.cell_size;
        self.meshes[mesh_idx] = crate::config_3d::mesh_3d::create_ground_plane(
            &self.device,
            b.width,
            b.depth,
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

        let (block_mesh_idx, block_tex_idx) = self.ensure_editor_box_gpu_assets();

        let mut spawn_block =
            |position: [f32; 3], scale: [f32; 3], entity_category: Option<&str>| {
            let base_label = rer_engine_shared::editor_defaults::entity_label_for_category(entity_category);
            let label = self.next_numbered_entity_name(base_label);
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
                    entity_category: entity_category.map(str::to_string),
                },
            );
            self.send_model_loaded_event(id, &label);
        };

        // Placeholder plantilla 3D: 3 muros (environment) + 3 cubos (object).
        send_load_progress("Insertando Paredes (3)", None, None);
        spawn_block([-6.0, 2.0, 18.0], [1.2, 4.0, 18.0], Some("environment"));
        spawn_block([6.0, 2.0, 18.0], [1.2, 4.0, 18.0], Some("environment"));
        spawn_block([0.0, 2.0, 27.0], [12.0, 4.0, 1.2], Some("environment"));
        send_load_progress("Insertando Cubos (3)", None, None);
        spawn_block([-2.5, 0.75, 11.0], [1.5, 1.5, 1.5], Some("object"));
        spawn_block([2.0, 1.25, 15.0], [2.0, 2.5, 2.0], Some("object"));
        spawn_block([0.0, 2.5, 21.0], [1.8, 5.0, 1.8], Some("object"));

        // Límites del mundo: el wireframe es centrado en el origen (z ∈ [-depth/2, depth/2]).
        // Los muros del placeholder llegan hasta z≈28; depth 36 dejaba max z=18 y el render los ocultaba.
        self.set_world_bounds_3d_size(28.0, 14.0, Some(56.0));

        self.camera = Camera::new();
        let spawn_xz = (0.0_f32, 5.0_f32);
        let ground_y = self
            .physics
            .find_ground_y_at(spawn_xz.0, spawn_xz.1, 10.0, 20.0)
            .unwrap_or(0.0);
        self.camera.target = glam::Vec3::new(spawn_xz.0, ground_y, spawn_xz.1);
        self.camera.pitch =
            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_PITCH;
        self.camera.yaw =
            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_YAW;
        self.camera.distance =
            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE;
        self.editor_viewport_yaw = self.camera.yaw;
        self.editor_viewport_pitch = self.camera.pitch;
        self.editor_viewport_distance = self.camera.distance;
        self.clamp_play_character_camera_to_bounds();
        self.clear_color = wgpu::Color {
            r: 0.06,
            g: 0.06,
            b: 0.10,
            a: 1.0,
        };

        send_load_progress("Insertando Character (Player)", None, None);
        self.apply_3d_placeholder_sun_and_player();

        send_load_progress("Plantilla 3D lista", None, None);
        log::info!("Plantilla 3D por defecto lista");
        let scene_name =
            rer_engine_shared::editor_defaults::default_scene_name(1);
        self.editor_scenes_init_from_boot(&scene_name);
        send_event(&EngineEvent::Ready {
            gravity: self.physics.gravity_magnitude(),
        });
    }

    pub(crate) fn apply_empty_3d_editor_defaults(&mut self) {
        self.set_world_bounds_3d_size(28.0, 14.0, Some(56.0));
        self.camera = Camera::new();
        let spawn_xz = (0.0_f32, 5.0_f32);
        let ground_y = self
            .physics
            .find_ground_y_at(spawn_xz.0, spawn_xz.1, 10.0, 20.0)
            .unwrap_or(0.0);
        self.camera.target = glam::Vec3::new(spawn_xz.0, ground_y, spawn_xz.1);
        self.camera.pitch =
            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_PITCH;
        self.camera.yaw =
            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_YAW;
        self.camera.distance =
            crate::config_3d::character_anchor::PLAY_CHARACTER_EDITOR_ORBIT_DISTANCE;
        self.editor_viewport_yaw = self.camera.yaw;
        self.editor_viewport_pitch = self.camera.pitch;
        self.editor_viewport_distance = self.camera.distance;
        self.clamp_play_character_camera_to_bounds();
        self.clear_color = wgpu::Color {
            r: 0.06,
            g: 0.06,
            b: 0.10,
            a: 1.0,
        };
    }

    /// Suelo checker + sol + jugador + pelota de prueba de la plantilla FP (sin muros ni cubos).
    pub(crate) fn apply_3d_placeholder_sun_and_player(&mut self) {
        self.ensure_ground_plane();
        self.ensure_default_sun();
        self.ensure_default_physics_ball();
        if self.play_character_entity.is_none() {
            self.spawn_play_character();
        }
        self.ensure_default_3d_player_ui();
        self.sync_fps_camera_mode();
    }

    fn has_physics_ball(&self) -> bool {
        self.save_registry
            .meta
            .values()
            .any(|m| crate::entity_save_meta::entity_path_marker(&m.path) == Some("[Ball]"))
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

    /// Tras cargar escena FP placeholder (switch sin guardar): alinear sol, luz y cámara orbital del editor.
    pub(crate) fn finalize_3d_placeholder_editor_scene(&mut self) {
        use crate::config_3d::character_anchor::{
            PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
        };
        use crate::config_3d::directional_light::{
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
            self.world.get::<Transform>(id).map(|t| {
                [t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]
            })
        });
        let body_scale = self.play_character_entity.and_then(|id| {
            self.world.get::<Transform>(id).map(|t| t.scale.to_array())
        });
        self.apply_play_character_view(
            feet,
            PLAY_CHARACTER_EDITOR_ORBIT_YAW,
            PLAY_CHARACTER_EDITOR_ORBIT_PITCH,
            None,
            None,
            Some(crate::ipc::PlayCameraFollowMode::MoveWithCharacter),
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
        if let Some(id) = self.play_character_entity {
            if self.world.get::<crate::ecs::Transform>(id).is_none() {
                self.play_character_entity = None;
            }
        }
        if let Some(id) = self.sun_entity {
            if self.world.get::<crate::ecs::Transform>(id).is_none() {
                self.sun_entity = None;
            }
        }
        if let Some(id) = self.editor_camera_entity {
            if self.world.get::<crate::ecs::Transform>(id).is_none() {
                self.editor_camera_entity = None;
            }
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
        self.quick_build_blueprint = None;
        self.blueprint_registry.clear();
        self.entity_blueprint_ids.clear();
        self.entity_colision.clear();
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
        self.fps_exit_hint_alpha = 0.0;
        self.pivot_edit_mode = None;
        self.logical_area_mode = None;
        self.control_bindings_by_entity.clear();
        self.clear_play_controller_script_frame();
        self.play_character_entity = None;
        self.editor_camera_entity = None;
        self.play_camera_eye_position = glam::Vec3::ZERO;
        self.play_camera_follow_mode = crate::ipc::PlayCameraFollowMode::MoveWithCharacter;
        self.play_camera_follow_offset = glam::Vec3::ZERO;
        self.play_camera_follow_offset_local = glam::Vec3::ZERO;
        self.sun_entity = None;
        self.sun_icon_mesh_idx = None;
        self.sun_icon_tex_idx = None;
        self.clear_preview_editor_snapshots();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_applying_undo = false;
    }

    /// Mesh + textura 1×1 compartidos para cajas del editor (`[EditorBox]`).
    pub(crate) fn ensure_editor_box_gpu_assets(&mut self) -> (usize, usize) {
        if let (Some(mesh_idx), Some(tex_idx)) =
            (self.editor_box_mesh_idx, self.editor_box_tex_idx)
        {
            return (mesh_idx, tex_idx);
        }
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let white_px = [255u8, 255, 255, 255];
        let tex_idx = self.tex_layers.len();
        let block_layer = self.texture_array.pack(&self.queue, &white_px, 1, 1);
        self.tex_layers.push(block_layer);
        self.editor_box_mesh_idx = Some(mesh_idx);
        self.editor_box_tex_idx = Some(tex_idx);
        (mesh_idx, tex_idx)
    }

    /// Cubo del editor (muros/cajas de plantilla) sin archivo `.glb`.
    pub(crate) fn spawn_editor_box(&mut self, name: &str, position: [f32; 3], scale: [f32; 3]) {
        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::BOX,
        );
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
    }

    pub(crate) fn load_character(&mut self, path: &str) {
        let is_player = path == "[Player]" || path.ends_with("[Player]");
        if !is_player {
            log::error!("[load_character] en 3D solo aplica a [Player]; use load_model para modelos: {path}");
            return;
        }
        if let Some(id) = self.play_character_entity {
            if self.world.get::<Transform>(id).is_some() {
                log::info!(
                    "[load_character] jugador ya existe (id={id}); no se crea entidad nueva"
                );
                send_event(&EngineEvent::CharacterLoaded {
                    id,
                    path: path.to_string(),
                });
                return;
            }
        }
        let label = rer_engine_shared::editor_defaults::entity_label::PLAYER.to_string();
        let id = self.world.spawn(Some(&label));
        log::info!("[load_character] jugador placeholder creado (id={id})");
        let feet = self.camera.target;
        self.setup_play_character_entity(id, feet);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "character".to_string(),
                path: path.to_string(),
                visual_model_path: None,
                entity_category: Some("player".to_string()),
            },
        );
        send_event(&EngineEvent::CharacterLoaded {
            id,
            path: path.to_string(),
        });
    }
}
