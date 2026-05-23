use glam::Vec3;

use crate::ecs::{EditorCamera, EntityId, NonSelectable, Transform};
use crate::engine::State;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{send_event, EngineEvent};

impl State {
    /// Crea la entidad de cámara orbital del editor (separada del jugador FP).
    pub(crate) fn ensure_editor_camera_entity(&mut self) {
        if self.camera_2d.is_some() || self.editor_camera_entity.is_some() {
            return;
        }
        let id = self.world.spawn(Some("EditorCamera"));
        self.editor_camera_entity = Some(id);
        self.world.insert(id, NonSelectable);
        self.world.insert(
            id,
            EditorCamera {
                yaw: self.editor_viewport_yaw,
                pitch: self.editor_viewport_pitch,
                distance: self.editor_viewport_distance,
            },
        );
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = self.editor_orbit_target;
            t.rotation = glam::Quat::IDENTITY;
            t.scale = Vec3::ONE;
        }
        self.save_registry.register_meta(
            id,
            EntitySaveMeta {
                kind: "editor_camera".to_string(),
                path: "[EditorCamera]".to_string(),
                visual_model_path: None,
                points: None,
            },
        );
        send_event(&EngineEvent::CharacterLoaded {
            id,
            path: "[EditorCamera]".to_string(),
        });
    }

    /// Copia el estado orbital del editor a la entidad ECS.
    pub(crate) fn sync_editor_camera_entity_from_viewport(&mut self) {
        let Some(id) = self.editor_camera_entity else {
            return;
        };
        if let Some(cam) = self.world.get_mut::<EditorCamera>(id) {
            cam.yaw = self.editor_viewport_yaw;
            cam.pitch = self.editor_viewport_pitch;
            cam.distance = self.editor_viewport_distance;
        }
        if let Some(t) = self.world.get_mut::<Transform>(id) {
            t.position = self.editor_orbit_target;
        }
    }

    /// Lee la entidad ECS y actualiza los campos legacy del viewport.
    pub(crate) fn sync_editor_viewport_from_camera_entity(&mut self) {
        let Some(id) = self.editor_camera_entity else {
            return;
        };
        if let Some(cam) = self.world.get::<EditorCamera>(id) {
            self.editor_viewport_yaw = cam.yaw;
            self.editor_viewport_pitch = cam.pitch;
            self.editor_viewport_distance = cam.distance;
        }
        if let Some(t) = self.world.get::<Transform>(id) {
            self.editor_orbit_target = t.position;
        }
    }

    /// Aplica transform del panel Propiedades a la cámara de editor (solo orbit target).
    pub(crate) fn apply_editor_camera_transform(
        &mut self,
        id: EntityId,
        position: Option<[f32; 3]>,
    ) -> bool {
        if self.editor_camera_entity != Some(id) {
            return false;
        }
        if let Some(p) = position {
            self.editor_orbit_target = Vec3::from_array(p);
        }
        self.sync_editor_camera_entity_from_viewport();
        true
    }
}
