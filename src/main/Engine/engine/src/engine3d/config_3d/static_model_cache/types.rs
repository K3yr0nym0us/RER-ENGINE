//! Tipos, claves y canal de precarga del caché de modelos estáticos.

use super::*;

#[derive(Clone, Copy)]
pub(crate) struct CachedStaticModelPart {
    pub mesh_idx: usize,
    pub tex_idx: usize,
    pub local_bounds: ([f32; 3], [f32; 3]),
    pub forward_xz: glam::Vec2,
    pub roughness: f32,
    pub metallic: f32,
    pub ior: f32,
}

pub(crate) struct ModelPreloadCpuResult {
    pub path: String,
    pub parts: Vec<CpuModelMeshPart>,
    pub anim_asset: Option<Arc<model_asset::ModelAsset>>,
    /// Variante jugador (`::play_character`); no aplica a props/entorno.
    pub warm_play_character: bool,
    /// Mallas jugador parseadas en el hilo de precarga (sin re-leer disco al warm GPU).
    pub play_character_parts: Option<Vec<CpuModelMeshPart>>,
}

pub(crate) type ModelPreloadRx = mpsc::Receiver<Result<ModelPreloadCpuResult, (String, String)>>;

pub(crate) type ModelPreloadTx = mpsc::Sender<Result<ModelPreloadCpuResult, (String, String)>>;

pub(crate) type StaticModelCache = HashMap<String, Vec<CachedStaticModelPart>>;

/// `load_model` recibido mientras la precarga GPU del path sigue en curso.
pub(crate) struct PendingLoadModel {
    pub path: String,
    pub entity_category: Option<String>,
    pub single_instance: bool,
    pub kind: String,
}

/// `replace_entity_model` recibido mientras la precarga GPU del path sigue en curso.
pub(crate) struct PendingEntityModelReplace {
    pub id: EntityId,
    pub path: String,
}

/// CPU listo; subida a GPU pendiente (no bloquear el event loop de winit).
pub(crate) struct PendingGpuModelPreload {
    pub(crate) path: String,
    pub(crate) parts: Vec<CpuModelMeshPart>,
    pub(crate) uploaded: Vec<CachedStaticModelPart>,
    pub(crate) anim_asset: Option<Arc<model_asset::ModelAsset>>,
    /// Variante jugador (`::play_character`); no aplica a props/entorno.
    pub(crate) warm_play_character: bool,
    /// Malla en GPU; textura de la parte actual pendiente (presupuesto en pasos).
    pub(crate) pending_part_mesh_idx: Option<usize>,
    /// Tras este job, ejecutar flush de `load_model` / replace en cola.
    pub(crate) defer_flush_path: Option<String>,
    /// Solo calienta caché `::play_character` (sin repetir `ModelAssetLoaded`).
    pub(crate) play_character_warm_only: bool,
    /// Variante jugador ya parseada en CPU (precarga Recursos con warm).
    pub(crate) play_character_parts: Option<Vec<CpuModelMeshPart>>,
}

/// Pasos GPU (malla o textura) por frame durante precarga en editor (event loop / IPC).
pub(crate) const MODEL_GPU_PARTS_PER_FRAME: usize = 12;

/// Presupuesto mayor al cargar `.save` o esperar precarga (sin redibujar ventana; ver `c51c943`).
pub(crate) const MODEL_GPU_PARTS_DURING_SAVE_LOAD: usize = 64;

pub(crate) fn create_model_preload_channel() -> (ModelPreloadTx, ModelPreloadRx) {
    mpsc::channel()
}

pub(crate) fn normalize_model_path(path: &str) -> String {
    // Normalizamos la "cache key" de forma determinista para evitar misses
    // cuando el mismo archivo llega con variantes de string (p.ej. `\\?\` vs
    // rutas normales o separadores `\` vs `/`).
    let canonical = Path::new(path)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string());

    let mut s = canonical.replace('\\', "/");

    // Convertir rutas device-prefix para que todas compartan la misma key.
    // Ej: `\\?\C:\x\y.glb` -> `C:/x/y.glb`
    // Ej: `\\?\UNC\server\share\...` -> `//server/share/...`
    if let Some(rest) = s.strip_prefix("//?/UNC/") {
        s = format!("//{rest}");
    } else if let Some(rest) = s.strip_prefix("//?/") {
        s = rest.to_string();
    }

    s
}

pub(crate) fn play_character_cache_key(canonical_path: &str) -> String {
    format!("{canonical_path}::play_character")
}

/// Clave de `model_assets` (skinning/clips); distinta de la malla estática del editor.
pub(crate) fn model_asset_cache_key(canonical_path: &str, normalize: Option<f32>) -> String {
    use crate::engine3d::config_3d::character_anchor::PLAY_CHARACTER_BODY_HEIGHT;
    match normalize {
        None => canonical_path.to_string(),
        Some(h) if (h - PLAY_CHARACTER_BODY_HEIGHT).abs() < 0.05 => {
            play_character_cache_key(canonical_path)
        }
        Some(h) => format!("{canonical_path}::norm_{h:.3}"),
    }
}
