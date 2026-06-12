//! Registro de modelos importados por `model_id`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rer_engine_shared::assets::AssetState;

#[derive(Clone, Debug)]
pub struct ImportedModelEntry {
    pub model_id:          String,
    pub name:              String,
    pub category:          Option<String>,
    pub state:             AssetState,
    pub rerasset_path:     PathBuf,
    pub source_path:       String,
    pub source_size:       u64,
    pub source_mtime_secs: u64,
    pub importer_version:  u16,
}

#[derive(Default)]
pub struct ImportedModelRegistry {
    by_id:     HashMap<String, ImportedModelEntry>,
    path_to_id: HashMap<String, String>,
}

impl ImportedModelRegistry {
    pub fn get(&self, model_id: &str) -> Option<&ImportedModelEntry> {
        self.by_id.get(model_id)
    }

    pub fn get_by_source_path(&self, path: &str) -> Option<&ImportedModelEntry> {
        let key = normalize_source_key(path);
        self.path_to_id
            .get(&key)
            .and_then(|id| self.by_id.get(id))
    }

    pub fn model_id_for_path(&self, path: &str) -> Option<String> {
        self.get_by_source_path(path).map(|e| e.model_id.clone())
    }

    pub fn insert(&mut self, entry: ImportedModelEntry) {
        let key = normalize_source_key(&entry.source_path);
        self.path_to_id.insert(key, entry.model_id.clone());
        self.by_id.insert(entry.model_id.clone(), entry);
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

    pub fn remove_by_source_path(&mut self, path: &str) -> Option<ImportedModelEntry> {
        let key = normalize_source_key(path);
        let id = self.path_to_id.remove(&key)?;
        self.by_id.remove(&id)
    }

    pub fn ready_entries(&self) -> impl Iterator<Item = &ImportedModelEntry> {
        self.by_id
            .values()
            .filter(|e| e.state == AssetState::Ready)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ImportedModelEntry> {
        self.by_id.values()
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
        .unwrap_or_else(|| std::env::temp_dir().join("rer-engine-imported").join("models"))
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
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(32)
        .collect();
    let slug = if slug.is_empty() { "model".to_string() } else { slug };

    let mut hasher = DefaultHasher::new();
    normalize_source_key(source_path).hash(&mut hasher);
    let hash6 = format!("{:06x}", hasher.finish() & 0x00FF_FFFF);
    format!("model_{slug}_{hash6}")
}

pub fn rerasset_path_for_id(model_id: &str) -> PathBuf {
    imported_models_dir().join(format!("{model_id}.rerasset"))
}

pub fn source_models_dir() -> PathBuf {
    std::env::var("RER_PROJECT_EXTRACT_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .map(|p| p.join("source").join("models"))
        .unwrap_or_else(|| std::env::temp_dir().join("rer-engine-source").join("models"))
}

/// Copia el archivo fuente a `source/models/` (solo sesión de editor con extract dir).
pub fn mirror_source_file(source_path: &Path) -> Option<PathBuf> {
    let extract = std::env::var("RER_PROJECT_EXTRACT_DIR").ok().filter(|s| !s.is_empty())?;
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
