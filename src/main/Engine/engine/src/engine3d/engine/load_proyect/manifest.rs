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
        "[load-pack] v{} {} {} | {} modelos, {} escenas, {} entidades | {}",
        project.version,
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
    let project: ProjectSaveData = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if project.r#type.is_empty() || project.gameStyle.is_empty() {
        return Err("manifest.json inválido (falta type o gameStyle)".to_string());
    }

    Ok(project)
}
