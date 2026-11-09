use super::*;

pub(crate) fn entity_path_marker(p: &str) -> Option<&'static str> {
    let marker = p.split(['/', '\\']).next_back().unwrap_or(p);
    ENTITY_MARKERS.iter().copied().find(|m| *m == marker)
}

pub(crate) fn is_editor_box_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[EditorBox]")
}

pub(crate) fn is_ball_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Ball]")
}

pub(crate) fn is_collider_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[Colisionador]")
}

pub(crate) fn is_execution_area_path(p: &str) -> bool {
    entity_path_marker(p) == Some("[ExecutionArea]")
}

pub(crate) fn is_model_3d_path(p: &str) -> bool {
    let lower = p.to_ascii_lowercase();
    lower.ends_with(".glb") || lower.ends_with(".gltf") || lower.ends_with(".fbx")
}

pub(crate) fn is_3d_model_file_entity(entity: &SavedEntity3D) -> bool {
    if entity.model_id.as_deref().is_some_and(|id| !id.is_empty()) {
        return entity_path_marker(&entity.model).is_none()
            && !matches!(entity.category.as_str(), "sun" | "ground" | "player");
    }
    if !is_model_3d_path(&entity.model) {
        return false;
    }
    entity_path_marker(&entity.model).is_none()
        && !matches!(entity.category.as_str(), "sun" | "ground" | "player")
}

/// Clave GPU (`model_id`) para spawn / caché al abrir `.save`.
pub(crate) fn entity_model_cache_lookup(
    state: &State,
    entity: &SavedEntity3D,
) -> Result<String, String> {
    if let Some(id) = entity.model_id.as_deref().filter(|s| !s.is_empty())
        && state
            .imported_model_registry
            .get(id)
            .is_some_and(|e| e.state == AssetState::Ready)
    {
        return Ok(id.to_string());
    }
    if let Some(id) = state
        .imported_model_registry
        .model_id_for_path(&entity.model)
        && state
            .imported_model_registry
            .get(&id)
            .is_some_and(|e| e.state == AssetState::Ready)
    {
        return Ok(id);
    }
    let basename = path_basename_lower(&entity.model);
    for entry in state.imported_model_registry.iter() {
        if entry.state != AssetState::Ready {
            continue;
        }
        if path_basename_lower(&entry.source_path) == basename {
            return Ok(entry.model_id.clone());
        }
    }
    Err(format!(
        "Sin asset importado para «{}» (requiere model_id + .rerasset en resources.models)",
        entity.model
    ))
}

pub(crate) fn entity_library_category(category: &str) -> Option<String> {
    match category {
        "environment" | "object" | "character" | "weapon" | "projectile" => {
            Some(category.to_string())
        }
        _ => None,
    }
}

pub(crate) fn path_basename_lower(p: &str) -> String {
    p.split(['/', '\\'])
        .next_back()
        .unwrap_or(p)
        .to_ascii_lowercase()
}

pub(crate) fn paths_match_for_burst(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    path_basename_lower(a) == path_basename_lower(b)
}

/// `model_*` del manifest v2 no es una ruta en disco bajo extract_dir.
pub(crate) fn is_imported_model_id(p: &str) -> bool {
    p.starts_with("model_")
}

/// Resuelve paths de assets en el `.save`; preserva marcadores y `model_id`.
pub(crate) fn resolve_asset_path(p: &str, extracted_dir: &Path) -> String {
    if p.is_empty() {
        return p.to_string();
    }
    if entity_path_marker(p).is_some() || is_imported_model_id(p) {
        return p.to_string();
    }
    resolve_path(p, extracted_dir)
}

/// Misma referencia de asset importado (model_id o alias en registro).
pub(crate) fn models_refer_to_same_asset(state: &State, a: &str, b: &str) -> bool {
    if paths_match_for_burst(a, b) {
        return true;
    }
    state.model_cache_key(a) == state.model_cache_key(b)
}

pub(crate) fn source_path_for_imported_model(
    project: &ProjectSaveData,
    model_id: &str,
    extract_dir: &Path,
) -> String {
    for scene in &project.scenes {
        for entity in scene.entities.iter().chain(scene.player.iter()) {
            if entity.model_id.as_deref() == Some(model_id) && !entity.model.is_empty() {
                if is_imported_model_id(&entity.model) {
                    continue;
                }
                if is_model_3d_path(&entity.model) || entity.model.contains('/') {
                    return resolve_asset_path(&entity.model, extract_dir);
                }
            }
        }
    }
    for entity in project.entities.iter().chain(project.player.iter()) {
        if entity.model_id.as_deref() == Some(model_id) && !entity.model.is_empty() {
            if is_imported_model_id(&entity.model) {
                continue;
            }
            if is_model_3d_path(&entity.model) || entity.model.contains('/') {
                return resolve_asset_path(&entity.model, extract_dir);
            }
        }
    }
    model_id.to_string()
}

pub(crate) fn resolve_path(p: &str, extracted_dir: &Path) -> String {
    if p.is_empty() {
        return p.to_string();
    }
    if entity_path_marker(p).is_some() {
        return entity_path_marker(p).unwrap().to_string();
    }
    if Path::new(p).is_absolute() {
        log::error!("[load] manifest path must be relative to .save extract dir: {p}");
    }
    let normalized = p.replace('/', std::path::MAIN_SEPARATOR_STR);
    extracted_dir
        .join(normalized)
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn resolve_optional_path(p: &Option<String>, extracted_dir: &Path) -> Option<String> {
    p.as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| resolve_path(s, extracted_dir))
}

pub(crate) fn resolve_script_source(source: &str, extracted_dir: &Path) -> String {
    if source.is_empty() {
        return String::new();
    }
    // Rhai embebido en manifest (sin referencia @file:scripting/...).
    if !source.starts_with(SCRIPT_FILE_PREFIX) {
        return source.to_string();
    }
    let rel = &source[SCRIPT_FILE_PREFIX.len()..];
    let normalized = rel.replace('/', std::path::MAIN_SEPARATOR_STR);
    let abs = extracted_dir.join(normalized);
    if !abs.is_file() {
        log::warn!("script referenciado no encontrado en save: {rel}");
        return String::new();
    }
    fs::read_to_string(&abs).unwrap_or_default()
}

pub(crate) fn resolve_scripts(
    scripts: &Option<Vec<SavedScript>>,
    extracted_dir: &Path,
) -> Option<Vec<SavedScript>> {
    scripts.as_ref().map(|list| {
        list.iter()
            .map(|s| SavedScript {
                name: s.name.clone(),
                source: resolve_script_source(&s.source, extracted_dir),
            })
            .collect()
    })
}

pub(crate) fn resolve_control_bindings(
    bindings: &Option<SavedControlBindings>,
    extracted_dir: &Path,
) -> Option<SavedControlBindings> {
    bindings.as_ref().map(|b| SavedControlBindings {
        keyboard_mouse: b
            .keyboard_mouse
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    SavedScript {
                        name: s.name.clone(),
                        source: resolve_script_source(&s.source, extracted_dir),
                    },
                )
            })
            .collect(),
        gamepad: b
            .gamepad
            .iter()
            .map(|(k, s)| {
                (
                    k.clone(),
                    SavedScript {
                        name: s.name.clone(),
                        source: resolve_script_source(&s.source, extracted_dir),
                    },
                )
            })
            .collect(),
    })
}

pub(crate) fn resolve_entity_3d(entity: &SavedEntity3D, extracted_dir: &Path) -> SavedEntity3D {
    SavedEntity3D {
        model: resolve_asset_path(&entity.model, extracted_dir),
        scripts: resolve_scripts(&entity.scripts, extracted_dir),
        controls: resolve_control_bindings(&entity.controls, extracted_dir),
        ..entity.clone()
    }
}

pub(crate) fn resolve_player_entity(
    player: &Option<SavedEntity3D>,
    extracted_dir: &Path,
) -> Option<SavedEntity3D> {
    player.as_ref().map(|p| resolve_entity_3d(p, extracted_dir))
}

pub(crate) fn resolve_loaded_paths(project: &mut ProjectSaveData, extracted_dir: &Path) {
    let has_scenes = !project.scenes.is_empty();

    project.sounds = project
        .sounds
        .iter()
        .map(|s| NamedPath {
            name: s.name.clone(),
            path: resolve_path(&s.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.fonts = project
        .fonts
        .iter()
        .map(|f| NamedPath {
            name: f.name.clone(),
            path: resolve_path(&f.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.backgrounds = project
        .backgrounds
        .iter()
        .map(|b| NamedPath {
            name: b.name.clone(),
            path: resolve_path(&b.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.hud_images = project
        .hud_images
        .iter()
        .map(|h| NamedPath {
            name: h.name.clone(),
            path: resolve_path(&h.path, extracted_dir),
            category: None,
            model_id: None,
            asset: None,
        })
        .collect();
    project.player_ui_text_boxes = project
        .player_ui_text_boxes
        .iter()
        .map(|b| crate::engine3d::ipc::SavePlayerUiTextBoxSnapshot {
            font_path: resolve_path(&b.font_path, extracted_dir),
            ..b.clone()
        })
        .collect();
    project.player_ui_buttons = project
        .player_ui_buttons
        .iter()
        .map(|b| crate::engine3d::ipc::SavePlayerUiButtonSnapshot {
            font_path: resolve_path(&b.font_path, extracted_dir),
            texture_path: resolve_optional_path(&b.texture_path, extracted_dir),
            ..b.clone()
        })
        .collect();
    project.player_ui_images = project
        .player_ui_images
        .iter()
        .map(|img| crate::engine3d::ipc::SavePlayerUiImageSnapshot {
            image_path: resolve_path(&img.image_path, extracted_dir),
            ..img.clone()
        })
        .collect();
    project.player_ui_objects = project
        .player_ui_objects
        .iter()
        .map(|obj| crate::engine3d::ipc::SavePlayerUiObjectSnapshot {
            texture_path: resolve_optional_path(&obj.texture_path, extracted_dir),
            ..obj.clone()
        })
        .collect();

    if !has_scenes {
        project.player = resolve_player_entity(&project.player, extracted_dir);
        project.entities = project
            .entities
            .iter()
            .map(|e| resolve_entity_3d(e, extracted_dir))
            .collect();
    } else {
        project.entities.clear();
        project.player = None;
        project.config_camera = None;
        project.config_editor_camera = None;
        project.backgroundPath = None;
        project.sprites.clear();
    }

    project.scenes = project
        .scenes
        .iter()
        .map(|scene| SavedScene {
            backgroundPath: resolve_optional_path(&scene.backgroundPath, extracted_dir),
            models: Vec::new(),
            entities: scene
                .entities
                .iter()
                .map(|e| resolve_entity_3d(e, extracted_dir))
                .collect(),
            player: resolve_player_entity(&scene.player, extracted_dir),
            ..scene.clone()
        })
        .collect();

    project.blueprints = project
        .blueprints
        .iter()
        .map(|bp| SavedBlueprint {
            model: resolve_asset_path(&bp.model, extracted_dir),
            scripts: resolve_scripts(&bp.scripts, extracted_dir),
            ..bp.clone()
        })
        .collect();
}
