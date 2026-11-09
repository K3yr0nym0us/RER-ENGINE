//! Parseo de skeleton, malla skinned y clips de animación (paso aparte de `mesh_3d`).

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::engine3d::config_3d::skin_diag;
use crate::engine3d::mesh::SkinnedVertex;

mod animation;
mod asset;
mod import;
mod material;
mod play;
mod skeleton;
mod types;

pub(crate) use self::animation::*;
pub(crate) use self::asset::*;
pub(crate) use self::import::*;
pub(crate) use self::material::*;
pub(crate) use self::play::*;
pub(crate) use self::skeleton::*;
pub(crate) use self::types::*;

pub(crate) use crate::engine3d::config_3d::model_asset_fbx::fbx_skinned_play_forward_xz;
