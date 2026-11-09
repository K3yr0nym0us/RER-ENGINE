//! Contrato IPC compartido entre `rer_engine_2d` y `rer_engine_3d`.
//!
//! Comandos y tipos de payload usados por ambos binarios. Cada motor extiende con
//! `EngineCommand2dOnly` / `EngineCommand3dOnly` en su propio `ipc.rs`.

pub mod commands;
pub mod types;

pub use commands::{AxisValue, EngineCommandCommon, RotationEulerDelta};
pub use types::*;
