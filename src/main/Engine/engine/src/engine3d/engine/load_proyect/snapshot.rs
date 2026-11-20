use super::*;

pub(crate) fn is_3d_placeholder_saved_scene(scene: &SavedScene) -> bool {
    scene.player.is_some()
        && scene.models.is_empty()
        && scene.entities.iter().any(|e| e.category == "sun")
        && scene.entities.iter().any(|e| e.category == "ground")
}

pub(crate) fn active_view_from_saved_scene(scene: &SavedScene) -> ActiveSaveView {
    ActiveSaveView {
        world: scene.world.clone(),
        entities: scene.entities.clone(),
        player: scene.player.clone(),
        config_camera: scene.config_camera.clone(),
        sceneId: scene.id,
        sceneName: scene.name.clone(),
    }
}

pub(crate) fn saved_scene_from_active_view(view: &ActiveSaveView) -> SavedScene {
    SavedScene {
        id: view.sceneId,
        name: view.sceneName.clone(),
        world: view.world.clone(),
        backgroundPath: None,
        entities: view.entities.clone(),
        player: view.player.clone(),
        config_camera: view.config_camera.clone(),
        config_editor_camera: None,
        sprites: Vec::new(),
        models: Vec::new(),
    }
}

pub(crate) fn saved_scene_from_snapshot_payload(
    p: &crate::engine3d::ipc::SaveSceneSnapshotPayload,
    id: u32,
    name: &str,
) -> SavedScene {
    use crate::engine3d::ipc::SaveEntity3DSnapshot;

    fn map_entity(e: &SaveEntity3DSnapshot) -> SavedEntity3D {
        SavedEntity3D {
            id: e.id,
            name: e.name.clone(),
            category: e.category.clone(),
            model: e.model.clone(),
            model_id: e.model_id.clone(),
            position: e.position,
            rotation: Some(e.rotation),
            scale: e.scale,
            physics_type: e.physics_type.clone(),
            colision: e.colision,
            hardness: e.hardness,
            animations: None,
            scripts: None,
            controls: e.controls.as_ref().map(|c| SavedControlBindings {
                keyboard_mouse: c
                    .keyboard_mouse
                    .iter()
                    .map(|(k, s)| {
                        (
                            k.clone(),
                            SavedScript {
                                name: s.name.clone(),
                                source: s.source.clone(),
                            },
                        )
                    })
                    .collect(),
                gamepad: c
                    .gamepad
                    .iter()
                    .map(|(k, s)| {
                        (
                            k.clone(),
                            SavedScript {
                                name: s.name.clone(),
                                source: s.source.clone(),
                            },
                        )
                    })
                    .collect(),
            }),
            blueprint_id: e.blueprint_id.clone(),
            texture_lod: e.texture_lod.clone(),
            attach_parent_id: e.attach_parent_id,
            attach_local_position: e.attach_local_position,
            attach_local_rotation: e.attach_local_rotation,
            attach_local_scale: e.attach_local_scale,
            attach_socket_host_id: e.attach_socket_host_id,
            attach_socket_name: e.attach_socket_name.clone(),
            sockets: e.sockets.clone(),
            bone_physics: e.bone_physics.clone(),
            projectile: e.projectile.clone(),
            interactable: e.interactable.clone(),
        }
    }

    SavedScene {
        id,
        name: name.to_string(),
        world: SavedWorldConfig {
            worldWidth: p.world.world_width,
            worldHeight: p.world.world_height,
            worldDepth: Some(p.world.world_depth),
            worldRadius: Some(p.world.world_radius),
            worldCenterX: p.world.world_center_x,
            worldCenterZ: p.world.world_center_z,
            gridVisible: p.world.grid_visible,
            gridCellSize: p.world.grid_cell_size,
            gravity: Some(p.world.gravity),
            targetFps: p.world.target_fps as f64,
            lightAmbient: p.world.light_ambient,
            lightIntensity: p.world.light_intensity,
            shadowDarkness: p.world.shadow_darkness,
            sunCycleEnabled: p.world.sun_cycle_enabled,
            sunRiseCardinal: p.world.sun_rise_cardinal.clone(),
            sunCycleDay: p.world.sun_cycle_day,
            sunCycleNight: p.world.sun_cycle_night,
            sunCyclePhase: p.world.sun_cycle_phase,
            graphicsTextureTier: p.world.graphics_texture_tier.clone(),
            textureDetailDistance: p.world.texture_detail_distance_m,
            reflectionTier: p.world.reflection_tier.clone(),
            reflectionMode: p.world.reflection_mode.clone(),
            rtTier: p.world.rt_tier.clone(),
            reflectionRaytracing: p.world.reflection_raytracing,
            reflectionProbes: p.world.reflection_probes,
            shadowTier: p.world.shadow_tier.clone(),
            msaaTier: p.world.msaa_tier.clone(),
            ssaaTier: p.world.ssaa_tier.clone(),
            taaEnabled: p.world.taa_enabled,
            taaBlend: p.world.taa_blend,
            taaJitterScale: p.world.taa_jitter_scale,
            lodEnabled: p.world.model_lod_enabled,
            lod1Distance: p.world.model_lod1_distance_m,
            lod2Distance: p.world.model_lod2_distance_m,
            particles: p
                .world
                .particles
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|error| {
                    log::warn!("[save] partículas no serializables: {error}");
                    error
                })
                .ok()
                .flatten(),
        },
        backgroundPath: p.background_path.clone(),
        entities: p.entities.iter().map(map_entity).collect(),
        player: p.player.as_ref().map(map_entity),
        config_camera: p.config_camera.as_ref().map(|c| {
            use crate::engine3d::ipc::PlayCameraFollowMode;
            let follow = match c.camera_follow_mode {
                PlayCameraFollowMode::FollowCharacter => "follow_character",
                PlayCameraFollowMode::MoveWithCharacter => "move_with_character",
            };
            SavedConfigCamera {
                camera_eye_position: c.camera_eye_position,
                fps_camera_yaw: c.fps_camera_yaw,
                fps_camera_pitch: c.fps_camera_pitch,
                yaw: Some(c.yaw),
                pitch: Some(c.pitch),
                fov_y: Some(c.fov_y),
                frustum_distance: Some(c.frustum_distance),
                camera_clip_radius: c.camera_clip_radius,
                camera_follow_mode: Some(follow.to_string()),
            }
        }),
        config_editor_camera: p
            .config_editor_camera
            .as_ref()
            .map(|c| SavedConfigEditorCamera {
                position: c.position,
                rotation: c.rotation,
            }),
        sprites: p
            .sprites
            .iter()
            .map(|s| NamedPath {
                name: s.name.clone(),
                path: s.path.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        models: Vec::new(),
    }
}

/// Escena 3D recién creada: suelo, sol (luz), jugador y cámaras. Sin plantilla demo.
pub(crate) fn build_fp_placeholder_saved_scene(id: u32, name: &str) -> SavedScene {
    use crate::engine3d::config_3d::character_anchor::{
        PLAY_CHARACTER_EDITOR_ORBIT_PITCH, PLAY_CHARACTER_EDITOR_ORBIT_YAW,
    };

    let world = SavedWorldConfig {
        // Lado 64 m / radio 32 m: el mínimo que acepta el generador de mundos.
        worldWidth: 64.0,
        worldHeight: 56.0,
        worldDepth: Some(64.0),
        worldRadius: Some(32.0),
        worldCenterX: 0.0,
        worldCenterZ: 0.0,
        gridVisible: true,
        gridCellSize: 1.0,
        gravity: Some(15.0),
        targetFps: 60.0,
        lightAmbient: Some(DEFAULT_LIGHT_AMBIENT),
        lightIntensity: Some(DEFAULT_LIGHT_INTENSITY),
        shadowDarkness: Some(DEFAULT_SHADOW_DARKNESS),
        // Demo: sol en el punto más alto (mediodía), ciclo activo por defecto.
        sunCycleEnabled: Some(true),
        sunRiseCardinal: Some("east".to_string()),
        sunCycleDay: Some(1000.0),
        sunCycleNight: Some(1000.0),
        sunCyclePhase: Some(0.25),
        graphicsTextureTier: Some("medium".to_string()),
        textureDetailDistance: Some(
            crate::engine3d::config_3d::entity_textures::default_texture_detail_near_m(),
        ),
        reflectionTier: Some(
            crate::engine3d::config_3d::reflection_graphics::DEFAULT_REFLECTION_TIER
                .wire()
                .to_string(),
        ),
        reflectionMode: Some("off".to_string()),
        rtTier: Some("low".to_string()),
        reflectionRaytracing: None,
        reflectionProbes: None,
        shadowTier: Some(
            crate::engine3d::config_3d::shadow_graphics::DEFAULT_SHADOW_TIER
                .wire()
                .to_string(),
        ),
        msaaTier: Some(
            crate::engine3d::config_3d::msaa_graphics::DEFAULT_MSAA_TIER
                .wire()
                .to_string(),
        ),
        ssaaTier: Some(
            crate::engine3d::config_3d::ssaa_graphics::DEFAULT_SSAA_TIER
                .wire()
                .to_string(),
        ),
        taaEnabled: None,
        taaBlend: None,
        taaJitterScale: None,
        lodEnabled: Some(crate::engine3d::config_3d::model_lod::DEFAULT_MODEL_LOD_ENABLED),
        lod1Distance: Some(crate::engine3d::config_3d::model_lod::DEFAULT_MODEL_LOD1_DISTANCE_M),
        lod2Distance: Some(crate::engine3d::config_3d::model_lod::DEFAULT_MODEL_LOD2_DISTANCE_M),
        particles: Some(serde_json::json!({ "emitters": [] })),
    };

    let ground_scale = [32.0 / 40.0, 0.02, 64.0 / 40.0];
    let sun_position = {
        use crate::engine3d::config_3d::directional_light::{
            SunRiseCardinal, sun_position_from_bounds,
        };
        use crate::engine3d::config_3d::world_bounds::WorldBounds3D;
        let bounds = WorldBounds3D::with_faces(32.0, 32.0, 32.0, 32.0, world.worldHeight);
        sun_position_from_bounds(&bounds, 0.25, SunRiseCardinal::East).to_array()
    };
    let entities = vec![
        SavedEntity3D {
            id: 0,
            name: "Ground".to_string(),
            category: "ground".to_string(),
            model: "[Ground]".to_string(),
            position: [0.0, 0.0, 0.0],
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            scale: ground_scale,
            physics_type: Some("static".to_string()),
            colision: true,
            hardness: 0,
            animations: None,
            scripts: None,
            controls: None,
            blueprint_id: None,
            texture_lod: None,
            model_id: None,
            attach_parent_id: None,
            attach_local_position: None,
            attach_local_rotation: None,
            attach_local_scale: None,
            attach_socket_host_id: None,
            attach_socket_name: None,
            sockets: vec![],
            bone_physics: vec![],
            projectile: None,
            interactable: None,
        },
        SavedEntity3D {
            id: 0,
            name: "Sun".to_string(),
            category: "sun".to_string(),
            model: "[Sun]".to_string(),
            position: sun_position,
            rotation: Some([0.0, 0.0, 0.0, 1.0]),
            scale: [1.0, 1.0, 1.0],
            physics_type: Some("static".to_string()),
            colision: true,
            hardness: 0,
            animations: None,
            scripts: None,
            controls: None,
            blueprint_id: None,
            texture_lod: None,
            model_id: None,
            attach_parent_id: None,
            attach_local_position: None,
            attach_local_rotation: None,
            attach_local_scale: None,
            attach_socket_host_id: None,
            attach_socket_name: None,
            sockets: vec![],
            bone_physics: vec![],
            projectile: None,
            interactable: None,
        },
    ];

    let player = SavedEntity3D {
        id: 0,
        name: "Player".to_string(),
        category: "player".to_string(),
        model: "[Player]".to_string(),
        position: [0.0, 0.0, 5.0],
        rotation: Some([0.0, 0.0, 0.0, 1.0]),
        scale: [1.0, 1.0, 1.0],
        physics_type: Some("dynamic".to_string()),
        colision: true,
        hardness: 0,
        animations: None,
        scripts: None,
        controls: None,
        blueprint_id: None,
        texture_lod: None,
        model_id: Some(
            rer_engine_core::bundled_models::DEFAULT_PLAY_CHARACTER_MODEL_ID.to_string(),
        ),
        attach_parent_id: None,
        attach_local_position: None,
        attach_local_rotation: None,
        attach_local_scale: None,
        attach_socket_host_id: None,
        attach_socket_name: None,
        sockets: vec![],
        bone_physics: vec![],
        projectile: None,
        interactable: None,
    };

    SavedScene {
        id,
        name: name.to_string(),
        world,
        backgroundPath: None,
        entities,
        player: Some(player),
        config_camera: Some(SavedConfigCamera {
            camera_eye_position: None,
            fps_camera_yaw: None,
            fps_camera_pitch: None,
            yaw: Some(PLAY_CHARACTER_EDITOR_ORBIT_YAW),
            pitch: Some(PLAY_CHARACTER_EDITOR_ORBIT_PITCH),
            fov_y: None,
            frustum_distance: None,
            camera_clip_radius: None,
            camera_follow_mode: Some("move_with_character".to_string()),
        }),
        config_editor_camera: None,
        sprites: Vec::new(),
        models: Vec::new(),
    }
}

pub(crate) fn build_minimal_project_from_store(
    game_style: &str,
    scenes: &[SavedScene],
) -> ProjectSaveData {
    ProjectSaveData {
        version: 1,
        r#type: "3D".to_string(),
        gameStyle: game_style.to_string(),
        activeSceneId: None,
        world: None,
        backgroundPath: None,
        entities: Vec::new(),
        player: None,
        config_camera: None,
        config_editor_camera: None,
        sprites: Vec::new(),
        sounds: Vec::new(),
        fonts: Vec::new(),
        backgrounds: Vec::new(),
        scenes: scenes.to_vec(),
        blueprints: Vec::new(),
        language: None,
        playerUiScreens:
            crate::engine3d::config_3d::player_ui::defaults::default_3d_project_ui_screens(),
        menuUiScreens: Vec::new(),
        player_ui_text_boxes: Vec::new(),
        player_ui_buttons: Vec::new(),
        player_ui_images: Vec::new(),
        player_ui_objects:
            crate::engine3d::config_3d::player_ui::defaults::default_3d_project_ui_objects(),
        hud_images: Vec::new(),
        resources: None,
        worldGen3d: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fp_placeholder_is_empty_except_ground_sun_player_and_camera() {
        let scene = build_fp_placeholder_saved_scene(2, "Scene-02");
        assert!(is_3d_placeholder_saved_scene(&scene));
        assert_eq!(scene.id, 2);
        assert_eq!(scene.name, "Scene-02");
        assert_eq!(scene.entities.len(), 2);
        // Demo: sol en el punto más alto y ciclo activo por defecto.
        assert_eq!(scene.world.sunCyclePhase, Some(0.25));
        assert_eq!(scene.world.sunCycleEnabled, Some(true));
        assert_eq!(scene.world.sunRiseCardinal.as_deref(), Some("east"));
        let categories: Vec<&str> = scene
            .entities
            .iter()
            .map(|entity| entity.category.as_str())
            .collect();
        assert!(categories.contains(&"ground"));
        assert!(categories.contains(&"sun"));
        assert!(scene.entities.iter().all(|entity| entity.model != "[Ball]"
            && !entity.model.contains("MatVal")
            && !entity.model.contains("ParticleDemo")));
        let player = scene.player.as_ref().expect("player");
        assert_eq!(player.model, "[Player]");
        assert_eq!(
            player.model_id.as_deref(),
            Some(rer_engine_core::bundled_models::DEFAULT_PLAY_CHARACTER_MODEL_ID)
        );
        assert_eq!(player.scale, [1.0, 1.0, 1.0]);
        assert!(scene.config_camera.is_some());
        assert!(scene.models.is_empty());
        let particles = scene.world.particles.as_ref().expect("empty particles");
        let emitters = particles
            .get("emitters")
            .and_then(serde_json::Value::as_array)
            .expect("emitters array");
        assert!(emitters.is_empty());
    }
}
