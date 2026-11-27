use super::*;

pub(crate) fn file_size_label(path: &Path) -> String {
    fs::metadata(path)
        .map(|m| format!("{} bytes", m.len()))
        .unwrap_or_else(|_| "tamaño desconocido".to_string())
}

pub(crate) fn log_load_manifest_summary(project: &ProjectSaveData, extract_dir: &Path) {
    let model_count = project
        .resources
        .as_ref()
        .map(|r| r.models.len())
        .unwrap_or(0);
    log::info!(
        "[load-pack] {} {} | {} modelos, {} escenas, {} entidades | {}",
        project.r#type,
        project.gameStyle,
        model_count,
        project.scenes.len(),
        project.entities.len(),
        extract_dir.display()
    );
    if let Some(resources) = &project.resources {
        for res in &resources.models {
            let disk = resolve_manifest_asset_path(&res.asset, extract_dir, &res.id);
            if disk.is_file() {
                log::info!(
                    "[load-pack]   «{}» {} → {} ({})",
                    res.name,
                    res.id,
                    disk.display(),
                    file_size_label(&disk)
                );
            } else {
                log::error!(
                    "[load-pack]   «{}» {} .rerasset FALTANTE: {}",
                    res.name,
                    res.id,
                    disk.display()
                );
            }
        }
    }
}

pub(crate) fn load_project_from_extract_dir(extract_path: &str) -> Result<ProjectSaveData, String> {
    let extract_dir = Path::new(extract_path);
    if extract_dir.is_file() {
        return Err(
            "se esperaba directorio extraído, no archivo .save (Electron ya descomprimió)"
                .to_string(),
        );
    }
    if !extract_dir.is_dir() {
        return Err("directorio de proyecto no encontrado".to_string());
    }

    let manifest_path = extract_dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Err("manifest.json no encontrado en directorio extraído".to_string());
    }

    let raw = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let manifest: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    crate::i18n::validate_project_manifest(&manifest)?;
    let project: ProjectSaveData = serde_json::from_value(manifest).map_err(|e| e.to_string())?;
    if project.r#type.is_empty() || project.gameStyle.is_empty() {
        return Err("manifest.json inválido (falta type o gameStyle)".to_string());
    }

    Ok(project)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::load_project_from_extract_dir;

    fn manifest_dir(tag: &str, manifest: Value) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("rer-load-3d-{tag}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create manifest test directory");
        std::fs::write(dir.join("manifest.json"), manifest.to_string())
            .expect("write manifest test file");
        dir
    }

    #[test]
    fn project_loader_rejects_legacy_or_missing_i18n_manifests() {
        let valid = manifest_dir(
            "valid",
            json!({
                "type": "3D",
                "gameStyle": "third-person",
                "i18n": {
                    "defaultLanguage": "en",
                    "fallbackLanguage": "en",
                    "catalogPaths": []
                }
            }),
        );
        assert!(load_project_from_extract_dir(&valid.to_string_lossy()).is_ok());
        let _ = std::fs::remove_dir_all(valid);

        for (tag, manifest) in [
            (
                "version",
                json!({
                    "type": "3D",
                    "gameStyle": "third-person",
                    "version": 1,
                    "i18n": { "defaultLanguage": "en", "fallbackLanguage": "en" }
                }),
            ),
            (
                "language",
                json!({
                    "type": "3D",
                    "gameStyle": "third-person",
                    "language": "es",
                    "i18n": { "defaultLanguage": "en", "fallbackLanguage": "en" }
                }),
            ),
            (
                "missing",
                json!({ "type": "3D", "gameStyle": "third-person" }),
            ),
            (
                "null",
                json!({ "type": "3D", "gameStyle": "third-person", "i18n": null }),
            ),
        ] {
            let dir = manifest_dir(tag, manifest);
            assert!(
                load_project_from_extract_dir(&dir.to_string_lossy()).is_err(),
                "el manifiesto {tag} debe rechazarse"
            );
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
