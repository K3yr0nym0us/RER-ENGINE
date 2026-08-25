//! Ensamblaje del contrato IPC 3D: comandos comunes + extensión exclusiva.

use serde::Deserialize;

pub use rer_engine_ipc_common::EngineCommandCommon;

use crate::ipc::{BlueprintPlacementMeta, PlayCameraFollowMode};

fn default_unit_quat() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn default_static_physics_type() -> String {
    "static".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EngineCommand {
    Common(EngineCommandCommon),
    Only3d(EngineCommand3dOnly),
}

/// Comandos solo en `rer_engine_3d` (rechazados en main si projectType es 2D).
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum EngineCommand3dOnly {
    SpawnCachedModel {
        path: String,
        #[serde(default)]
        name: Option<String>,
        position: [f32; 3],
        #[serde(default = "default_unit_quat")]
        rotation: [f32; 4],
        scale: [f32; 3],
        #[serde(default)]
        entity_category: Option<String>,
        #[serde(default)]
        blueprint_id: Option<String>,
        #[serde(default)]
        physics_enabled: bool,
        #[serde(default = "default_static_physics_type")]
        physics_type: String,
    },
    SpawnQuickBuildInstance {
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    PlaceQuickBuildAtCursor {
        #[serde(default)]
        pixel_x: Option<f32>,
        #[serde(default)]
        pixel_y: Option<f32>,
    },
    RegisterBlueprint {
        blueprint: BlueprintPlacementMeta,
    },
    LoadModelAsset {
        path: String,
        name: String,
        #[serde(default)]
        category: Option<String>,
    },
    RemoveModelAsset {
        path: String,
    },
    GetModelsList,
    SpawnEditorBox {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    SpawnSun {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    SpawnGround {
        position: [f32; 3],
        scale: [f32; 3],
    },
    SetDirectionalLight {
        #[serde(default)]
        ambient: Option<f32>,
        #[serde(default)]
        intensity: Option<f32>,
        #[serde(default)]
        shadow_darkness: Option<f32>,
    },
    #[serde(rename = "set_play_character_spawn")]
    SetPlayCharacterSpawn {
        position: [f32; 3],
        yaw: f32,
        pitch: f32,
    },
    #[serde(rename = "set_play_character_view")]
    SetPlayCharacterView {
        #[serde(default)]
        position: Option<[f32; 3]>,
        #[serde(default)]
        position_axis: Option<rer_engine_ipc_common::AxisValue>,
        #[serde(default)]
        yaw: Option<f32>,
        #[serde(default)]
        pitch: Option<f32>,
        #[serde(default)]
        fov_y: Option<f32>,
        #[serde(default)]
        frustum_distance: Option<f32>,
        #[serde(default)]
        camera_only: Option<bool>,
        #[serde(default)]
        camera_follow_mode: Option<PlayCameraFollowMode>,
        #[serde(default)]
        body_rotation: Option<[f32; 4]>,
        #[serde(default)]
        body_scale: Option<[f32; 3]>,
        #[serde(default)]
        camera_eye_position: Option<[f32; 3]>,
        #[serde(default)]
        fps_camera_yaw: Option<f32>,
        #[serde(default)]
        fps_camera_pitch: Option<f32>,
    },
    SetGraphicsTextureTier {
        tier: String,
    },
    SetTextureDetailDistance {
        distance_m: f32,
    },
    SetReflectionTier {
        tier: String,
    },
    SetReflectionRaytracing {
        enabled: bool,
    },
    SetReflectionProbes {
        enabled: bool,
    },
    SpawnReflectionProbe {
        #[serde(default)]
        position: Option<[f32; 3]>,
    },
    SetReflectionDebugView {
        view: String,
    },
    SetSsrDebugMode {
        enabled: bool,
    },
    SetShadowTier {
        tier: String,
    },
    SetWorldRadius {
        radius: f32,
    },
    SetTaa {
        enabled: bool,
        #[serde(default)]
        blend: Option<f32>,
        #[serde(default)]
        jitter_scale: Option<f32>,
    },
    SetCameraFov {
        fov_y: f32,
    },
    #[serde(rename = "set_play_editor_frustum_distance")]
    SetPlayEditorFrustumDistance {
        distance: f32,
    },
    SetEntityColision {
        id: u32,
        colision: bool,
    },
    LoadCharacter {
        path: String,
        #[serde(default)]
        track_undo: Option<bool>,
    },
    CreateEditorScene {
        name: String,
    },
    SwitchEditorScene {
        scene_id: u32,
    },
    DeleteEditorScene {
        scene_id: u32,
    },
    NotifyProjectSaved {
        extract_dir: String,
    },
    ClearEditorUndoRedo,
    MergeEntities {
        ids: Vec<u32>,
    },
    ListEntityBones {
        entity_id: u32,
    },
    ListEntitySockets {
        entity_id: u32,
    },
    UpsertEntitySocket {
        entity_id: u32,
        name: String,
        bone_name: String,
        #[serde(default)]
        local_position: [f32; 3],
        #[serde(default = "default_unit_quat")]
        local_rotation: [f32; 4],
    },
    RemoveEntitySocket {
        entity_id: u32,
        name: String,
    },
    AttachToSocket {
        child_ids: Vec<u32>,
        host_id: u32,
        socket_name: String,
    },
    DetachFromSocket {
        child_id: u32,
    },
    SetSocketBonePickMode {
        entity_id: u32,
        active: bool,
    },
    SetBonePhysicsEditorEntity {
        entity_id: u32,
        active: bool,
    },
    SetBonePhysicsPickMode {
        entity_id: u32,
        active: bool,
    },
    SetBonePhysics {
        entity_id: u32,
        bone_name: String,
        mode: String,
    },
    RemoveBonePhysics {
        entity_id: u32,
        bone_name: String,
    },
    ListEntityBonePhysics {
        entity_id: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_set_reflection_tier_command() {
        let json = r#"{"cmd":"set_reflection_tier","tier":"medium"}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_reflection_tier IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionTier { tier }) => {
                assert_eq!(tier, "medium");
            }
            other => panic!("expected Only3d SetReflectionTier, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_set_taa_command() {
        let json = r#"{"cmd":"set_taa","enabled":true}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_taa IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetTaa { enabled, .. }) => {
                assert!(enabled);
            }
            other => panic!("expected Only3d SetTaa, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_set_taa_command_with_params() {
        let json = r#"{"cmd":"set_taa","enabled":true,"blend":0.7,"jitter_scale":0.8}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_taa IPC with params");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetTaa {
                enabled,
                blend,
                jitter_scale,
            }) => {
                assert!(enabled);
                assert_eq!(blend, Some(0.7));
                assert_eq!(jitter_scale, Some(0.8));
            }
            other => panic!("expected Only3d SetTaa, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_set_reflection_debug_view_command() {
        let json = r#"{"cmd":"set_reflection_debug_view","view":"ssr_hits"}"#;
        let cmd: EngineCommand = serde_json::from_str(json).expect("set_reflection_debug_view IPC");
        match cmd {
            EngineCommand::Only3d(EngineCommand3dOnly::SetReflectionDebugView { view }) => {
                assert_eq!(view, "ssr_hits");
            }
            other => panic!("expected Only3d SetReflectionDebugView, got {other:?}"),
        }
    }
}
