use crate::ipc::{
    EngineCommand, EngineCommand2dOnly, EngineCommandCommon, EngineEvent, ImportSceneCamera2d,
    ImportSceneEntity, ImportScenePayload, send_event,
};

use super::State;

impl State {
    pub(crate) fn import_scene(&mut self, payload: ImportScenePayload) {
        self.suppress_scene_setup_logs = true;
        match payload.scene.as_str() {
            "2D" => self.setup_2d_platformer(),
            "scratch" => self.setup_scratch(),
            other => {
                log::warn!("[import_scene] escena '{other}' no reconocida, usando 2D");
                self.setup_2d_platformer();
            }
        }

        self.handle_command(EngineCommand::Common(EngineCommandCommon::SetWorldSize {
            width: payload.world.world_width,
            height: payload.world.world_height,
            depth: None,
        }));
        self.handle_command(EngineCommand::Common(EngineCommandCommon::SetGridVisible {
            visible: payload.world.grid_visible,
        }));
        self.handle_command(EngineCommand::Common(
            EngineCommandCommon::SetGridCellSize {
                size: payload.world.grid_cell_size,
            },
        ));
        self.handle_command(EngineCommand::Common(EngineCommandCommon::SetTargetFps {
            fps: payload.world.target_fps,
        }));
        let gravity = payload
            .world
            .gravity
            .unwrap_or(rer_engine_shared::DEFAULT_GRAVITY_MAGNITUDE);
        self.handle_command(EngineCommand::Common(EngineCommandCommon::SetGravity {
            gravity,
        }));

        if let Some(ImportSceneCamera2d { x, y, half_h }) = payload.camera2d {
            self.handle_command(EngineCommand::Only2d(EngineCommand2dOnly::SetCamera2d {
                x,
                y,
                half_h,
            }));
        }

        for sprite in &payload.sprites {
            self.handle_command(EngineCommand::Common(EngineCommandCommon::LoadSprite {
                path: sprite.path.clone(),
                name: sprite.name.clone(),
            }));
        }

        match payload.background_path.as_deref() {
            Some(path) if !path.trim().is_empty() => {
                self.background_path = Some(path.to_owned());
                self.load_background(path);
            }
            _ => {
                self.background_path = None;
                self.clear_background();
            }
        }

        let entity_count = payload.entities.len();
        for ent in &payload.entities {
            self.import_scene_entity(ent);
        }

        send_event(&EngineEvent::SceneImported {
            entity_count: entity_count as u32,
        });
        self.suppress_scene_setup_logs = false;
    }

    fn import_scene_entity(&mut self, ent: &ImportSceneEntity) {
        let id = ent.id;
        let is_player = ent.path == "[Player]";
        let omit_scale = ent.omit_scale || is_player;
        let skip_transform = ent.skip_transform;
        let apply_initial = ent.apply_initial_animation_frame.unwrap_or(true);

        let created = match ent.kind.as_str() {
            "scenario" => self
                .insert_scenario_at(&ent.path, Some(id), ent.name.as_deref())
                .then_some(id),
            "character" => self
                .insert_character_at(&ent.path, Some(id), ent.name.as_deref())
                .then_some(id),
            "collider" => ent.points.and_then(|pts| {
                self.create_collision_box_from_points_at(&pts, Some(id), ent.name.as_deref(), false)
            }),
            "execution_area" => ent.points.and_then(|pts| {
                self.create_execution_area_from_points_at(
                    &pts,
                    Some(id),
                    ent.name.as_deref(),
                    false,
                )
            }),
            other => {
                log::warn!("[import_scene] kind '{other}' ignorado para entidad {id}");
                None
            }
        };

        let Some(entity_id) = created else {
            return;
        };

        self.apply_entity_restore_inner(
            entity_id,
            ent.name.clone(),
            &ent.transform,
            ent.physics.as_ref(),
            ent.animations.as_deref(),
            ent.scripts.as_deref(),
            ent.control_bindings.as_ref(),
            omit_scale,
            skip_transform,
            apply_initial,
        );
    }
}
