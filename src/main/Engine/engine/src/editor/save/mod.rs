//! Guardado `.save` en Rust: ensamblado del manifiesto + empaquetado ZIP.

pub mod manifest;
pub mod pack;

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::editor::project_state::{ProjectState, is_saveable};

/// Resultado de un guardado correcto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSuccess {
    pub save_path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

impl SaveSuccess {
    pub fn file_name(&self) -> String {
        self.save_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project.save")
            .to_string()
    }
}

/// Ensambla y escribe el `.save` a partir del snapshot del motor y del estado
/// que respondió la UI React.
pub fn write_save(
    engine_scene: &Value,
    state: &ProjectState,
    project_type: &str,
    game_style: &str,
    save_path: &Path,
    extract_dir: Option<&Path>,
) -> Result<SaveSuccess, String> {
    if !is_saveable(state) {
        return Err("la UI no entregó el estado del proyecto".to_string());
    }

    let mut document = manifest::build_manifest(&manifest::ManifestInput {
        engine_scene,
        state,
        project_type,
        game_style,
    });
    let report = pack::pack_and_write(&mut document, extract_dir, save_path)?;

    Ok(SaveSuccess {
        save_path: save_path.to_path_buf(),
        files: report.files,
        bytes: report.bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::io::Read;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rer-save-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn engine_scene() -> Value {
        json!({
            "world": { "world_width": 10.0, "world_height": 10.0, "world_depth": 10.0,
                       "world_radius": 5.0, "grid_visible": true, "grid_cell_size": 1.0,
                       "gravity": 9.81, "target_fps": 60 },
            "entities": [], "sprites": [], "models": [], "sounds": [], "backgrounds": []
        })
    }

    #[test]
    fn writes_manifest_and_assets() {
        let dir = temp_dir("assets");
        let assets = dir.join("src");
        fs::create_dir_all(&assets).unwrap();
        let sound = assets.join("hit.wav");
        fs::write(&sound, b"RIFF").unwrap();

        let save_path = dir.join("project.save");
        let state = ProjectState {
            project_type: Some("2D".into()),
            game_style: Some("top-down".into()),
            locale: Some("es".into()),
            scenes: vec![
                json!({ "id": 1, "name": "Nivel", "entities": [], "player": null, "sprites": [] }),
            ],
            active_scene_id: Some(1),
            sounds: vec![json!({ "name": "hit", "path": sound.to_string_lossy() })],
            ..ProjectState::default()
        };

        let result = write_save(
            &engine_scene(),
            &state,
            "2D",
            "top-down",
            &save_path,
            Some(&dir),
        )
        .unwrap();
        assert!(result.files >= 2);

        let file = fs::File::open(&save_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut names = Vec::new();
        for index in 0..archive.len() {
            names.push(archive.by_index(index).unwrap().name().to_string());
        }
        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.contains(&"sounds/hit.wav".to_string()));

        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        let manifest: Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(manifest["sounds"][0]["path"], json!("sounds/hit.wav"));
        assert_eq!(manifest["language"], json!("es"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_project_state() {
        let dir = temp_dir("empty");
        let save_path = dir.join("empty.save");
        let error = write_save(
            &engine_scene(),
            &ProjectState::default(),
            "2D",
            "top-down",
            &save_path,
            Some(&dir),
        )
        .unwrap_err();
        assert!(error.contains("estado del proyecto"));
        let _ = fs::remove_dir_all(&dir);
    }
}
