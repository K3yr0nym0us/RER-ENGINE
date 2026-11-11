use super::*;

/// Lista de biblioteca para el editor desde `resources.models` (`path` = `model_id`).
pub(crate) fn editor_models_from_manifest(project: &ProjectSaveData) -> Vec<ImportSceneSprite> {
    project
        .resources
        .as_ref()
        .map(|resources| {
            resources
                .models
                .iter()
                .map(|res| ImportSceneSprite {
                    path: res.id.clone(),
                    name: res.name.clone(),
                    category: crate::engine3d::ipc::normalize_model_library_category(Some(
                        res.model_type.as_str(),
                    )),
                    model_id: Some(res.id.clone()),
                    asset: Some(res.asset.clone()),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Tras `switch_editor_scene`: lista de escenas del registro del editor (incluye creadas sin guardar).
pub(crate) fn send_project_loaded_3d_with_editor_scenes(
    project: &ProjectSaveData,
    view: &ActiveSaveView,
    editor_scenes: &[crate::engine3d::ipc::EditorSceneListItem],
) {
    send_project_loaded_3d(project, view, Some(editor_scenes));
}

pub(crate) fn send_project_loaded_3d(
    project: &ProjectSaveData,
    view: &ActiveSaveView,
    editor_scenes: Option<&[crate::engine3d::ipc::EditorSceneListItem]>,
) {
    let scenes: Vec<ProjectLoaded3dSceneTab> = if let Some(items) = editor_scenes {
        items
            .iter()
            .map(|s| ProjectLoaded3dSceneTab {
                id: s.id,
                name: s.name.clone(),
            })
            .collect()
    } else if project.scenes.is_empty() {
        vec![ProjectLoaded3dSceneTab {
            id: view.sceneId,
            name: view.sceneName.clone(),
        }]
    } else {
        project
            .scenes
            .iter()
            .map(|s| ProjectLoaded3dSceneTab {
                id: s.id,
                name: s.name.clone(),
            })
            .collect()
    };

    let world = ProjectLoaded3dWorld {
        worldWidth: view.world.worldWidth,
        worldHeight: view.world.worldHeight,
        worldDepth: view.world.worldDepth,
        worldRadius: view.world.worldRadius.or_else(|| {
            let d = view.world.worldDepth.unwrap_or(50.0);
            Some(view.world.worldWidth.min(view.world.worldHeight).min(d) * 0.5)
        }),
        gridVisible: view.world.gridVisible,
        gridCellSize: view.world.gridCellSize,
        gravity: view.world.gravity,
        targetFps: view.world.targetFps,
        lightAmbient: view.world.lightAmbient,
        lightIntensity: view.world.lightIntensity,
        shadowDarkness: view.world.shadowDarkness,
        graphicsTextureTier: view
            .world
            .graphicsTextureTier
            .clone()
            .or_else(|| Some("medium".to_string())),
        textureDetailDistance: view.world.textureDetailDistance.or_else(|| {
            Some(crate::engine3d::config_3d::entity_textures::default_texture_detail_near_m())
        }),
        reflectionTier: view.world.reflectionTier.clone().or_else(|| {
            Some(
                crate::engine3d::config_3d::reflection_graphics::DEFAULT_REFLECTION_TIER
                    .wire()
                    .to_string(),
            )
        }),
        reflectionRaytracing: view.world.reflectionRaytracing,
        reflectionProbes: view.world.reflectionProbes,
        shadowTier: view.world.shadowTier.clone().or_else(|| {
            Some(
                crate::engine3d::config_3d::shadow_graphics::DEFAULT_SHADOW_TIER
                    .wire()
                    .to_string(),
            )
        }),
        msaaTier: view.world.msaaTier.clone().or_else(|| {
            Some(
                crate::engine3d::config_3d::msaa_graphics::DEFAULT_MSAA_TIER
                    .wire()
                    .to_string(),
            )
        }),
        taaEnabled: view.world.taaEnabled,
        taaBlend: view.world.taaBlend,
        taaJitterScale: view.world.taaJitterScale,
    };

    let models = editor_models_from_manifest(project);

    let blueprints =
        serde_json::to_value(&project.blueprints).unwrap_or(serde_json::Value::Array(vec![]));

    let player = view
        .player
        .as_ref()
        .and_then(|p| serde_json::to_value(p).ok());
    let config_camera = view
        .config_camera
        .as_ref()
        .and_then(|c| serde_json::to_value(c).ok());

    send_project_loaded_3d_event(&ProjectLoaded3dEvent {
        event: "project_loaded_3d",
        activeSceneId: view.sceneId,
        sceneName: view.sceneName.clone(),
        entityCount: view.entities.len() as u32,
        scenes,
        language: project.language.clone(),
        models,
        sounds: project
            .sounds
            .iter()
            .map(|s| ImportSceneSprite {
                path: s.path.clone(),
                name: s.name.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        fonts: project
            .fonts
            .iter()
            .map(|f| ImportSceneSprite {
                path: f.path.clone(),
                name: f.name.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        backgrounds: project
            .backgrounds
            .iter()
            .map(|b| ImportSceneSprite {
                path: b.path.clone(),
                name: b.name.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        hud_images: project
            .hud_images
            .iter()
            .map(|img| ImportSceneSprite {
                path: img.path.clone(),
                name: img.name.clone(),
                category: None,
                model_id: None,
                asset: None,
            })
            .collect(),
        blueprints,
        world,
        player,
        config_camera,
        playerUiScreens: project.playerUiScreens.clone(),
        menuUiScreens: project.menuUiScreens.clone(),
    });
}
