use super::*;

impl State {
    pub(super) fn cmd_world(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Common(EngineCommandCommon::SetWorldSize {
                width,
                height,
                depth,
            }) => {
                let old_bounds = self.world_bounds_3d;
                self.set_world_bounds_3d_size(width, height, depth);
                self.clamp_play_character_camera_to_bounds();
                self.worldgen_world_bounds_changed(old_bounds);
            }
            EngineCommand::Only3d(EngineCommand3dOnly::SetWorldBounds {
                north,
                south,
                east,
                west,
                height,
            }) => {
                if let Some(height) = height {
                    self.world_bounds_3d.height = height.clamp(
                        crate::engine3d::MIN_WORLD_HEIGHT_3D,
                        crate::engine3d::MAX_WORLD_HEIGHT_3D,
                    );
                }
                let old_bounds = self.world_bounds_3d;
                self.set_world_bounds_3d_faces(north, south, east, west);
                self.clamp_play_character_camera_to_bounds();
                self.worldgen_world_bounds_changed(old_bounds);
            }
            EngineCommand::Common(EngineCommandCommon::SetGravity { gravity }) => {
                let magnitude = gravity.abs();
                self.physics.set_gravity(-magnitude);
                log::info!("[physics] Gravedad actualizada: {:.2} m/s²", magnitude);
            }
            EngineCommand::Common(EngineCommandCommon::SetGridVisible { visible }) => {
                self.grid_config.visible = visible;
            }
            EngineCommand::Common(EngineCommandCommon::SetGridCellSize { size }) => {
                self.grid_config.cell_size = size.clamp(0.05, 100.0);
                self.refresh_ground_checker_uv();
            }
            EngineCommand::Common(EngineCommandCommon::SetPhysics {
                id,
                enabled,
                body_type,
            }) => {
                if self.play_character_entity == Some(id) || self.editor_camera_entity == Some(id) {
                    self.physics.remove_entity_body(id);
                } else if enabled {
                    self.set_entity_physics_from_mesh_aabb(id, &body_type);
                } else {
                    self.physics.remove_entity_body(id);
                }

                send_event(&EngineEvent::PhysicsChanged {
                    entity_id: id,
                    enabled,
                    body_type,
                });
            }
            other => return Some(other),
        }
        None
    }
}
