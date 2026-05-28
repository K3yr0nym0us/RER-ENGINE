use glam::{Quat, Vec3};

use crate::config_3d::character_anchor::{center_from_feet, feet_from_transform};
use crate::ecs::{EntityId, Transform};
use crate::engine::State;
use crate::ipc::PlayCameraFollowMode;

#[derive(Clone, Copy)]
pub(crate) struct PreviewEntityTransform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

#[derive(Clone, Copy)]
pub(crate) struct PreviewFpEditorView {
    pub play_camera_eye_position: Vec3,
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub fov_y: f32,
    pub frustum_distance: f32,
    pub follow_mode: PlayCameraFollowMode,
    pub editor_orbit_target: Vec3,
    pub editor_viewport_yaw: f32,
    pub editor_viewport_pitch: f32,
    pub editor_viewport_distance: f32,
}

impl State {
    pub(crate) fn capture_preview_editor_snapshots(&mut self) {
        self.preview_entity_transform_snapshots.clear();
        for &id in self.world.entities() {
            let Some(t) = self.world.get::<Transform>(id) else {
                continue;
            };
            self.preview_entity_transform_snapshots.insert(
                id,
                PreviewEntityTransform {
                    position: t.position,
                    rotation: t.rotation,
                    scale: t.scale,
                },
            );
        }

        self.preview_fp_view_snapshot = if self.has_play_character() {
            Some(PreviewFpEditorView {
                play_camera_eye_position: self.play_camera_eye_position,
                camera_yaw: self.camera.yaw,
                camera_pitch: self.camera.pitch,
                fov_y: self.camera.fov_y,
                frustum_distance: self.fps_editor_frustum_distance,
                follow_mode: self.play_camera_follow_mode,
                editor_orbit_target: self.editor_orbit_target,
                editor_viewport_yaw: self.editor_viewport_yaw,
                editor_viewport_pitch: self.editor_viewport_pitch,
                editor_viewport_distance: self.editor_viewport_distance,
            })
        } else {
            None
        };
    }

    pub(crate) fn restore_preview_editor_snapshots_on_enter(&mut self) {
        let ids: Vec<EntityId> = self.preview_entity_transform_snapshots.keys().copied().collect();
        for id in ids {
            let Some(snap) = self.preview_entity_transform_snapshots.get(&id).copied() else {
                continue;
            };
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                t.position = snap.position;
                t.rotation = snap.rotation;
                t.scale = snap.scale;
            }
        }

        if let Some(fp) = self.preview_fp_view_snapshot {
            self.play_camera_eye_position = fp.play_camera_eye_position;
            self.camera.yaw = fp.camera_yaw;
            self.camera.pitch = fp.camera_pitch;
            self.camera.fov_y = fp.fov_y;
            self.fps_editor_frustum_distance = fp.frustum_distance;
            self.play_camera_follow_mode = fp.follow_mode;
            self.editor_orbit_target = fp.editor_orbit_target;
            self.editor_viewport_yaw = fp.editor_viewport_yaw;
            self.editor_viewport_pitch = fp.editor_viewport_pitch;
            self.editor_viewport_distance = fp.editor_viewport_distance;
            self.sync_editor_camera_entity_from_viewport();
            self.capture_play_camera_follow_offset();
        }

        if self.play_character_entity.is_some() {
            self.ensure_play_character_kinematic_only();
        }
    }

    pub(crate) fn commit_play_session_to_editor(&mut self) {
        if self.has_play_character() {
            self.editor_viewport_yaw = self.camera.yaw;
            self.editor_viewport_pitch = self.camera.pitch;

            if let Some(id) = self.play_character_entity {
                let snap = self.preview_entity_transform_snapshots.get(&id).copied();
                if let (Some(snap), Some(t)) = (snap, self.world.get_mut::<Transform>(id)) {
                    let feet = if self.play_character_mesh_extents.is_some() {
                        t.position
                    } else {
                        feet_from_transform(
                            t.position,
                            t.scale.y,
                            t.rotation,
                            None,
                        )
                    };
                    t.scale = snap.scale;
                    t.position = if self.play_character_mesh_extents.is_some() {
                        feet
                    } else {
                        center_from_feet(feet, snap.scale.y, t.rotation, None)
                    };
                    self.editor_orbit_target = t.position;
                } else if let Some(t) = self.world.get::<Transform>(id) {
                    self.editor_orbit_target = t.position;
                }
            }

            self.capture_play_camera_follow_offset();
            self.sync_editor_camera_entity_from_viewport();
            self.ensure_play_character_kinematic_only();
        }

        let entity_ids: Vec<EntityId> = self.world.entities().to_vec();
        for id in entity_ids {
            self.sync_entity_physics_collider(id);
        }
    }

    pub(crate) fn clear_preview_editor_snapshots(&mut self) {
        self.preview_entity_transform_snapshots.clear();
        self.preview_fp_view_snapshot = None;
        self.play_session_body_yaw_baseline = 0.0;
        self.play_session_camera_yaw_baseline = 0.0;
    }
}
