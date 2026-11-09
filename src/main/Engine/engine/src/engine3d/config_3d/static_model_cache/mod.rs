// ── Precarga asíncrona y caché GPU de modelos 3D ─────────────────────────────
//
// Dos entradas por path canónico:
//   · clave normal — mesh tal cual (Recursos / props / `load_model`).
//   · `{path}::play_character` — mesh normalizado (~PLAY_CHARACTER_BODY_HEIGHT) para
//     `replace_entity_model` del jugador. No reutilizar la entrada normal en el player:
//     el pivote/escala difieren y forzar `sync_player_rotation_from_look` en editor
//     rompe la restauración del `.save`.
//
// La implementación está partida por responsabilidad en los submódulos siguientes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::engine3d::assets::load::{load_rerasset_cpu, material_texture_chunk_map};
use crate::engine3d::config_3d::mesh_3d::{
    CpuModelMeshPart, prepare_cpu_parts_textures_for_gpu, vertex_local_bounds,
};
use crate::engine3d::config_3d::model_asset;
use crate::engine3d::config_3d::{
    physics_body_world_center, physics_half_extents_for_model, transform_position_for_visual_center,
};
use crate::engine3d::ecs::{EntityId, MeshComponent, SurfacePbr, Transform};
use crate::engine3d::engine::State;
use crate::engine3d::entity_save_meta::EntitySaveMeta;
use crate::engine3d::ipc::{EngineEvent, send_event, send_load_progress};
use rer_engine_core::assets::read_rerasset;
use rer_engine_core::editor_defaults::entity_label_for_spawn;

mod preload;
mod registry;
mod spawn;
mod types;

pub(crate) use self::types::*;
