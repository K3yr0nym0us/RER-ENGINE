// ── Escenas base 3D por código ────────────────────────────────────────────────

use crate::config_3d::first_person::FIRST_PERSON_BODY_HEIGHT;
use crate::config_3d::first_person::FIRST_PERSON_COLLIDER_RADIUS;
use crate::config_3d::physics_3d::PhysicsWorld;
use crate::config_3d::{Camera, WorldBounds3D};
use crate::ecs::EntityId;
use crate::config_compat::{ActiveTool, PhysicsWorld2D};
use crate::ecs::{MeshComponent, NonSelectable, Transform};
use crate::engine::State;
use crate::gizmo;
use crate::ipc::{send_event, EngineEvent};
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
        self.pivot_edit_mode = None;
        self.logical_area_mode = None;
        self.script_engine = ScriptEngine::new()
            .expect("Error al reinicializar el motor de scripting Lua");
        self.control_bindings_by_entity.clear();
        self.clear_first_person_script_frame();
        self.first_person_player_entity = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_applying_undo = false;
        self.world_bounds_3d = WorldBounds3D::default();
        self.sync_world_bounds_3d_runtime();
    }

    /// Escena base del modo first-person: suelo checker y cámara a ras de editor 3D.
    pub(crate) fn setup_first_person(&mut self) {
        self.reset_runtime_scene_3d();

        let ground_plane = crate::config_3d::mesh_3d::create_ground_plane(&self.device);
        self.meshes.push(ground_plane);

        let checker_pixels = {
            const S: u32 = 128;
            const TILE: u32 = 8;
            let mut px: Vec<u8> = Vec::with_capacity((S * S * 4) as usize);
            for y in 0..S {
                for x in 0..S {
                    let light = ((x / TILE + y / TILE) % 2) == 0;
                    let (r, g, b): (u8, u8, u8) = if light { (58, 61, 80) } else { (30, 32, 48) };
                    px.extend_from_slice(&[r, g, b, 255]);
                }
            }
            px
        };
        let checker_uv = self.atlas.pack(&self.queue, &checker_pixels, 128, 128);
        self.uv_rects.push(checker_uv);

        let plane_id = self.world.spawn(Some("Ground"));
        self.world.insert(
            plane_id,
            MeshComponent {
                mesh_idx: 0,
                tex_idx: 0,
            },
        );
        self.world.insert(plane_id, NonSelectable);
        if let Some(t) = self.world.get_mut::<Transform>(plane_id) {
            t.position = glam::Vec3::new(0.0, 0.0, 0.0);
            t.scale = glam::Vec3::new(20.0, 0.02, 20.0);
        }
        self.physics.add_static_ground();

        let block_mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let white_px = [235u8, 240, 255, 255];
        let block_tex_idx = self.uv_rects.len();
        let block_uv = self.atlas.pack(&self.queue, &white_px, 1, 1);
        self.uv_rects.push(block_uv);

        let mut spawn_block =
            |name: &str, position: [f32; 3], scale: [f32; 3]| {
                let id = self.world.spawn(Some(name));
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
                self.send_model_loaded_event(id, name);
            };

        // Escenario base visible al frente para que first-person no arranque en vacío.
        spawn_block("Wall_Left", [-6.0, 2.0, 18.0], [1.2, 4.0, 18.0]);
        spawn_block("Wall_Right", [6.0, 2.0, 18.0], [1.2, 4.0, 18.0]);
        spawn_block("Wall_Back", [0.0, 2.0, 27.0], [12.0, 4.0, 1.2]);
        spawn_block("Crate_A", [-2.5, 0.75, 11.0], [1.5, 1.5, 1.5]);
        spawn_block("Crate_B", [2.0, 1.25, 15.0], [2.0, 2.5, 2.0]);
        spawn_block("Pillar", [0.0, 2.5, 21.0], [1.8, 5.0, 1.8]);

        // Límites del mundo: el wireframe es centrado en el origen (z ∈ [-depth/2, depth/2]).
        // Los muros del placeholder llegan hasta z≈28; depth 36 dejaba max z=18 y el render los ocultaba.
        self.set_world_bounds_3d_size(28.0, 14.0, Some(56.0));

        self.camera = Camera::new();
        self.camera.target = glam::Vec3::new(
            0.0,
            crate::config_3d::first_person::FIRST_PERSON_GROUND_REST_Y,
            5.0,
        );
        self.camera.pitch = 0.25;
        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
        self.camera.distance =
            crate::config_3d::first_person::FIRST_PERSON_EDITOR_ORBIT_DISTANCE;
        self.clamp_first_person_camera_to_bounds();
        self.clear_color = wgpu::Color {
            r: 0.06,
            g: 0.06,
            b: 0.10,
            a: 1.0,
        };

        self.spawn_first_person_player();
        self.sync_first_person_camera_mode();

        log::info!("Escena first-person cargada: arena base del editor 3D");
    }

    /// Cuerpo placeholder del jugador FP (cubo). La posición del transform = pies (= cámara).
    fn attach_first_person_player_body(&mut self, id: EntityId) {
        let mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let body_px = [180u8, 200, 255, 255];
        let tex_idx = self.uv_rects.len();
        let body_uv = self.atlas.pack(&self.queue, &body_px, 1, 1);
        self.uv_rects.push(body_uv);

        self.world.insert(
            id,
            MeshComponent {
                mesh_idx,
                tex_idx,
            },
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            let w = FIRST_PERSON_COLLIDER_RADIUS * 2.0;
            t.scale = glam::Vec3::new(w, FIRST_PERSON_BODY_HEIGHT, w);
        }
    }

    fn setup_first_person_player_entity(&mut self, id: EntityId, feet: glam::Vec3) {
        self.character_entities.push(id);
        self.first_person_player_entity = Some(id);
        self.attach_first_person_player_body(id);
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = crate::config_3d::first_person::player_body_center_from_feet(feet);
        }
        self.camera.target = feet;
        self.sync_player_rotation_from_look();
        self.sync_first_person_camera_mode();
    }

    fn spawn_first_person_player(&mut self) {
        let feet = self.camera.target;
        let id = self.world.spawn(Some("Player"));
        self.setup_first_person_player_entity(id, feet);
        send_event(&EngineEvent::CharacterLoaded {
            id,
            path: "[Player]".to_string(),
        });
    }

    /// Escena 3D vacía: solo para abrir un `.save` (sin plantilla first-person).
    pub(crate) fn setup_empty_3d(&mut self) {
        self.reset_runtime_scene_3d();
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
        let block_mesh_idx = self.meshes.len();
        self.meshes.push(mesh::create_cube(&self.device));
        let white_px = [235u8, 240, 255, 255];
        let block_tex_idx = self.uv_rects.len();
        let block_uv = self.atlas.pack(&self.queue, &white_px, 1, 1);
        self.uv_rects.push(block_uv);

        let id = self.world.spawn(Some(name));
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
        self.send_model_loaded_event(id, name);
    }

    pub(crate) fn load_character(&mut self, path: &str) {
        let is_player = path == "[Player]" || path.ends_with("[Player]");
        let id = self.world.spawn(Some(
            if is_player {
                "Player"
            } else {
                path.split(['/', '\\']).next_back().unwrap_or(path)
            },
        ));
        if is_player {
            let feet = self.camera.target;
            self.setup_first_person_player_entity(id, feet);
        } else {
            self.character_entities.push(id);
        }
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
