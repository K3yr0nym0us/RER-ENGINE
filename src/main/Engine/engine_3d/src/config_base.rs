// ── Escenas base 3D por código ────────────────────────────────────────────────

use crate::config_3d::physics_3d::PhysicsWorld;
use crate::config_3d::mesh_3d::GROUND_PLANE_MESH_EXTENT;
use crate::config_3d::{Camera, WorldBounds3D};
use crate::config_compat::{ActiveTool, PhysicsWorld2D};
use crate::ecs::{MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::gizmo;
use crate::ipc::{send_event, EngineEvent};
use crate::entity_save_meta::EntitySaveMeta;
use crate::mesh;
use crate::scripting::ScriptEngine;

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
        });
    }

    /// Limpieza compartida para cualquier escena base del binario 3D.
    pub(crate) fn reset_runtime_scene_3d(&mut self) {
        self.stop_audio_internal();
        self.physics = PhysicsWorld::new();
        self.physics_2d = PhysicsWorld2D::new();
        self.world.clear();
        self.meshes.clear();
        self.uv_rects.clear();
        self.static_tex_cache.clear();
        self.anim_texture_cache.clear();
        self.atlas.reset(&self.queue);
        self.reload_snap_hint_assets();
        self.anim_overrides.clear();
        self.animations.clear();
        self.active_animations.clear();
        self.default_animation_by_entity.clear();
        self.anim_saved_transforms.clear();
        self.anim_flip_overrides.clear();
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
        self.camera_2d = None;
        self.active_tool = ActiveTool::None;
        self.quick_build_ghost_id = None;
        self.quick_build_preview_path = None;
        self.quick_build_preview_kind = None;
        self.quick_build_preview_scale = None;
        self.tool_overlay_buffer = gizmo::build_from_vertices(&self.device, &[]);
        self.show_snap_hint = false;
        self.snap_hint_alpha = 0.0;
        self.fps_exit_hint_alpha = 0.0;
        self.pivot_edit_mode = None;
        self.logical_area_mode = None;
        self.script_engine = ScriptEngine::new()
            .expect("Error al reinicializar el motor de scripting Lua");
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

    fn find_ground_entity_id(&self) -> Option<crate::ecs::EntityId> {
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
        let checker_uv = self.atlas.pack(&self.queue, &px, S, S);
        let tex_idx = self.uv_rects.len();
        self.uv_rects.push(checker_uv);
        tex_idx
    }

    /// Suelo checker + colisión estática. Idempotente (también tras `setup_empty_3d` al cargar `.save`).
    pub(crate) fn ensure_ground_plane(&mut self) {
        if self.find_ground_entity_id().is_some() {
            return;
        }

        let ground_mesh_idx = self.meshes.len();
        self.meshes
            .push(crate::config_3d::mesh_3d::create_ground_plane(&self.device));
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
                points: None,
            },
        );
        self.send_model_loaded_event(plane_id, "Ground");
    }

    pub(crate) fn spawn_ground_plane(&mut self, position: [f32; 3], _scale: [f32; 3]) {
        self.ensure_ground_plane();
        let Some(id) = self.find_ground_entity_id() else {
            return;
        };
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = glam::Vec3::from_array(position);
            t.position.y = 0.0;
        }
        self.sync_ground_plane_to_world_bounds();
        self.send_model_loaded_event(id, "Ground");
    }

    /// Escala el mesh del suelo (40×40 local) al cuadro de límites del accordion World.
    pub(crate) fn sync_ground_plane_to_world_bounds(&mut self) {
        if self.camera_2d.is_some() {
            return;
        }
        let Some(id) = self.find_ground_entity_id() else {
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
    }

    /// Escena base del modo first-person: suelo checker y cámara a ras de editor 3D.
    pub(crate) fn setup_default_3d_scene(&mut self) {
        self.reset_runtime_scene_3d();
        self.ensure_ground_plane();

        let block_mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let white_px = [255u8, 255, 255, 255];
        let block_tex_idx = self.uv_rects.len();
        let block_uv = self.atlas.pack(&self.queue, &white_px, 1, 1);
        self.uv_rects.push(block_uv);

        let mut spawn_block = |position: [f32; 3], scale: [f32; 3]| {
            let label = self.next_numbered_entity_name(rer_engine_shared::editor_defaults::entity_label::SCENARIO);
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
            self.scenario_entities.push(id);
            self.save_registry.register_meta(
                id,
                EntitySaveMeta {
                    kind: "model".to_string(),
                    path: "[EditorBox]".to_string(),
                    visual_model_path: None,
                    points: None,
                },
            );
            self.send_model_loaded_event(id, &label);
        };

        // Escenario base visible al frente para que first-person no arranque en vacío.
        spawn_block([-6.0, 2.0, 18.0], [1.2, 4.0, 18.0]);
        spawn_block([6.0, 2.0, 18.0], [1.2, 4.0, 18.0]);
        spawn_block([0.0, 2.0, 27.0], [12.0, 4.0, 1.2]);
        spawn_block([-2.5, 0.75, 11.0], [1.5, 1.5, 1.5]);
        spawn_block([2.0, 1.25, 15.0], [2.0, 2.5, 2.0]);
        spawn_block([0.0, 2.5, 21.0], [1.8, 5.0, 1.8]);

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

        self.spawn_play_character();
        self.sync_fps_camera_mode();
        self.ensure_default_sun();

        log::info!("Escena 3D por defecto cargada: arena base del editor");
    }

    /// Escena 3D vacía: solo para abrir un `.save` (sin plantilla first-person).
    pub(crate) fn setup_empty_3d(&mut self) {
        self.reset_runtime_scene_3d();
        self.ensure_ground_plane();
        self.camera = Camera::new();
        self.clear_color = wgpu::Color {
            r: 0.06,
            g: 0.06,
            b: 0.10,
            a: 1.0,
        };
        log::info!("Escena 3D vacía — contenido desde guardado");
    }

    /// Cubo del editor (muros/cajas de plantilla) sin archivo `.glb`.
    pub(crate) fn spawn_editor_box(&mut self, name: &str, position: [f32; 3], scale: [f32; 3]) {
        let label = self.resolve_entity_display_name(
            name,
            rer_engine_shared::editor_defaults::entity_label::BOX,
        );
        let block_mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let white_px = [255u8, 255, 255, 255];
        let block_tex_idx = self.uv_rects.len();
        let block_uv = self.atlas.pack(&self.queue, &white_px, 1, 1);
        self.uv_rects.push(block_uv);

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
        self.scenario_entities.push(id);
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "model".to_string(),
                path: "[EditorBox]".to_string(),
                visual_model_path: None,
                points: None,
            },
        );
        self.send_model_loaded_event(id, &label);
        self.push_remove_entity_undo(id);
    }

    pub(crate) fn load_character(&mut self, path: &str) {
        let is_player = path == "[Player]" || path.ends_with("[Player]");
        let label = if is_player {
            rer_engine_shared::editor_defaults::entity_label::PLAYER.to_string()
        } else {
            self.next_numbered_entity_name(rer_engine_shared::editor_defaults::entity_label::CHARACTER)
        };
        let id = self.world.spawn(Some(&label));
        if is_player {
            let feet = self.camera.target;
            self.setup_play_character_entity(id, feet);
        } else {
            self.character_entities.push(id);
        }
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "character".to_string(),
                path: path.to_string(),
                visual_model_path: None,
                points: None,
            },
        );
        send_event(&EngineEvent::CharacterLoaded {
            id,
            path: path.to_string(),
        });
    }

    /// Inicializa la escena scratch: un cubo de referencia con cámara orbital.
    pub(crate) fn setup_scratch(&mut self) {
        self.reset_runtime_scene_3d();

        // Cubo central con textura blanca (fallback)
        let cube = mesh::create_cube(&self.device);
        self.meshes.push(cube);
        let white_px = [255u8, 255, 255, 255];
        let uv = self.atlas.pack(&self.queue, &white_px, 1, 1);
        let tex_idx = self.uv_rects.len();
        self.uv_rects.push(uv);
        let cube_id = self.world.spawn(Some("Cube"));
        self.world.insert(cube_id, MeshComponent { mesh_idx: 0, tex_idx });

        // Cámara orbital por defecto mirando el cubo
        self.camera = Camera::new();
        self.clear_color = wgpu::Color { r: 0.06, g: 0.06, b: 0.10, a: 1.0 };

        log::info!("Escena BASE cargada: cubo de referencia");
    }
}
