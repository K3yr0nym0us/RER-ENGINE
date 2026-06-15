//! Ensamblaje del contrato IPC 2D: comandos comunes + extensión exclusiva.

use serde::Deserialize;

pub use rer_engine_ipc_common::EngineCommandCommon;

use crate::ipc::ImportScenePayload;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EngineCommand {
    Common(EngineCommandCommon),
    Only2d(EngineCommand2dOnly),
}

/// Comandos solo en `rer_engine_2d` (rechazados en main si projectType es 3D).
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum EngineCommand2dOnly {
    LoadScenario {
        path: String,
        #[serde(default)]
        track_undo: Option<bool>,
    },
    SetScenarioScale { id: u32, scale: f32 },
    LoadCharacter {
        path: String,
        #[serde(default)]
        track_undo: Option<bool>,
    },
    SetCharacterScale { id: u32, scale: f32 },
    ClearBackground,
    PlayAnimationFrame {
        id:        u32,
        path:      String,
        #[serde(default)]
        pivot_x:   Option<f32>,
        #[serde(default)]
        pivot_y:   Option<f32>,
        logical_w: u32,
        logical_h: u32,
        #[serde(default)]
        src_x:     Option<u32>,
        #[serde(default)]
        src_y:     Option<u32>,
        #[serde(default)]
        src_w:     Option<u32>,
        #[serde(default)]
        src_h:     Option<u32>,
    },
    RestoreAnimationFrame { id: u32 },
    SetCamera2d { x: f32, y: f32, half_h: f32 },
    LoadBackground { path: String },
    SetPivotEditMode {
        id: u32,
        frame_path: String,
        pivot_x: f32,
        pivot_y: f32,
    },
    CancelPivotEditMode,
    SetLogicalAreaMode { id: u32, w: u32, h: u32 },
    CancelLogicalAreaMode,
    CreateColliderFromPoints {
        points: [[f32; 2]; 4],
        #[serde(default)]
        track_undo: Option<bool>,
    },
    CreateExecutionAreaFromPoints {
        points: [[f32; 2]; 4],
        #[serde(default)]
        track_undo: Option<bool>,
    },
    SetVsync { enabled: bool },
    ImportScene(ImportScenePayload),
}
