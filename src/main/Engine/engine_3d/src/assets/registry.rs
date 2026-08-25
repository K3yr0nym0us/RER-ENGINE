//! Registro de modelos importados por `model_id`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rer_engine_shared::assets::AssetState;

#[derive(Clone, Debug)]
pub struct ImportedModelEntry {
    pub model_id: String,
    pub name: String,
    pub category: Option<String>,
    pub state: AssetState,
    pub rerasset_path: PathBuf,
    pub source_path: String,
    pub source_size: u64,
    pub source_mtime_secs: u64,
    pub importer_version: u16,
}

#[derive(Default)]
pub struct ImportedModelRegistry {
    by_id: HashMap<String, ImportedModelEntry>,
    path_to_id: HashMap<String, String>,
}

impl ImportedModelRegistry {
    pub fn get(&self, model_id: &str) -> Option<&ImportedModelEntry> {
        self.by_id.get(model_id)
    }

    pub fn get_by_source_path(&self, path: &str) -> Option<&ImportedModelEntry> {
        let key = normalize_source_key(path);
        self.path_to_id.get(&key).and_then(|id| self.by_id.get(id))
    }

    pub fn model_id_for_path(&self, path: &str) -> Option<String> {
        self.get_by_source_path(path).map(|e| e.model_id.clone())
    }

    pub fn insert(&mut self, entry: ImportedModelEntry) {
        let model_id = entry.model_id.clone();
        let source_path = entry.source_path.clone();
        self.by_id.insert(model_id.clone(), entry);
        self.link_imported_model_aliases(&model_id, &source_path);
    }

    /// Registra alias de path → `model_id` (basename, `source/models/…`, id).
    pub fn link_imported_model_aliases(&mut self, model_id: &str, source_path: &str) {
        if !self.by_id.contains_key(model_id) {
            return;
        }
        self.link_alias(model_id, source_path);
        self.link_alias(model_id, model_id);
        if let Some(base) = Path::new(source_path).file_name().and_then(|n| n.to_str()) {
            self.link_alias(model_id, base);
            self.link_alias(model_id, &relative_source_manifest_path(base));
        }
    }

    fn link_alias(&mut self, model_id: &str, alias_path: &str) {
        if alias_path.is_empty() {
            return;
        }
        self.path_to_id
            .insert(normalize_source_key(alias_path), model_id.to_string());
    }

    pub fn set_state(&mut self, model_id: &str, state: AssetState) {
        if let Some(e) = self.by_id.get_mut(model_id) {
            e.state = state;
        }
    }

    pub fn mark_ready(&mut self, model_id: &str, rerasset_path: PathBuf, importer_version: u16) {
        if let Some(e) = self.by_id.get_mut(model_id) {
            e.state = AssetState::Ready;
            e.rerasset_path = rerasset_path;
            e.importer_version = importer_version;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &ImportedModelEntry> {
        self.by_id.values()
    }

    /// Quita un modelo importado y todos sus alias de path.
    pub fn remove(&mut self, model_id: &str) -> Option<ImportedModelEntry> {
        let entry = self.by_id.remove(model_id)?;
        self.path_to_id.retain(|_, id| id != model_id);
        Some(entry)
    }
}

pub fn normalize_source_key(path: &str) -> String {
    path.replace('\\', "/")
}

pub fn imported_models_dir() -> PathBuf {
    std::env::var("RER_PROJECT_EXTRACT_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .map(|p| p.join("imported").join("models"))
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("rer-engine-imported")
                .join("models")
        })
}

pub fn source_fingerprint(path: &Path) -> (u64, u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (size, mtime)
}

pub fn generate_model_id(display_name: &str, source_path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let slug: String = display_name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(32)
        .collect();
    let slug = if slug.is_empty() {
        "model".to_string()
    } else {
        slug
    };

    let mut hasher = DefaultHasher::new();
    normalize_source_key(source_path).hash(&mut hasher);
    let hash6 = format!("{:06x}", hasher.finish() & 0x00FF_FFFF);
    format!("model_{slug}_{hash6}")
}

pub fn relative_rerasset_manifest_path(model_id: &str) -> String {
    format!("imported/models/{model_id}.rerasset")
}

pub fn relative_source_manifest_path(basename: &str) -> String {
    format!("source/models/{basename}")
}

pub fn rerasset_path_for_id(model_id: &str) -> PathBuf {
    imported_models_dir().join(format!("{model_id}.rerasset"))
}

pub fn resolve_rerasset_on_disk(extract_dir: &Path, model_id: &str) -> PathBuf {
    if extract_dir.as_os_str().is_empty() {
        rerasset_path_for_id(model_id)
    } else {
        extract_dir.join(relative_rerasset_manifest_path(model_id))
    }
}

/// Resuelve `resources.models[].asset` (relativo al extract dir) a path en disco.
pub fn resolve_manifest_asset_path(asset: &str, extract_dir: &Path, model_id: &str) -> PathBuf {
    let normalized = normalize_source_key(asset);
    if !asset.is_empty() && !Path::new(asset).is_absolute() && normalized.starts_with("imported/") {
        if extract_dir.as_os_str().is_empty() {
            rerasset_path_for_id(model_id)
        } else {
            extract_dir.join(asset.replace('/', std::path::MAIN_SEPARATOR_STR))
        }
    } else if Path::new(asset).is_absolute() {
        PathBuf::from(asset)
    } else {
        resolve_rerasset_on_disk(extract_dir, model_id)
    }
}

pub fn source_models_dir() -> PathBuf {
    std::env::var("RER_PROJECT_EXTRACT_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .map(|p| p.join("source").join("models"))
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("rer-engine-source")
                .join("models")
        })
}

/// Copia el archivo fuente a `source/models/` (solo sesión de editor con extract dir).
pub fn mirror_source_file(source_path: &Path) -> Option<PathBuf> {
    let extract = std::env::var("RER_PROJECT_EXTRACT_DIR")
        .ok()
        .filter(|s| !s.is_empty())?;
    if extract.is_empty() {
        return None;
    }
    let dir = source_models_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let name = source_path.file_name()?.to_os_string();
    let dest = dir.join(name);
    std::fs::copy(source_path, &dest).ok()?;
    Some(dest)
}
