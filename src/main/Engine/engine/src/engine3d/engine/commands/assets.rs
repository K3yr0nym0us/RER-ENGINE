use super::*;

impl State {
    pub(super) fn cmd_assets(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Common(EngineCommandCommon::LoadModel {
                path,
                single_instance,
                entity_category,
                kind,
            }) => {
                let category = entity_category.as_deref();
                let kind = kind.as_deref().unwrap_or("model");
                if single_instance.unwrap_or(false) {
                    self.load_model_single(&path, category, kind);
                } else {
                    self.load_model(&path, category, kind);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::LoadModelAsset {
                path,
                name,
                category,
            }) => {
                self.register_model_asset(&path, &name, category.as_deref());
            }
            EngineCommand::Only3d(EngineCommand3dOnly::RemoveModelAsset { path }) => {
                if let Some(removed) = self.remove_model_from_library(&path) {
                    send_event(&EngineEvent::ModelAssetRemoved {
                        path: removed.clone(),
                    });
                    log::info!("[model] eliminado de recursos: {}", removed);
                } else {
                    log::warn!("[model] intento de eliminar modelo inexistente: {}", path);
                }
            }
            EngineCommand::Only3d(EngineCommand3dOnly::GetModelsList) => {
                let models: Vec<crate::engine3d::ipc::ModelInfo> = self
                    .model_store
                    .iter()
                    .map(|(path, entry)| {
                        let state = entry.model_id.as_ref().and_then(|id| {
                            self.imported_model_registry.get(id).map(|e| {
                                match e.state {
                                    rer_engine_core::assets::AssetState::Importing => "importing",
                                    rer_engine_core::assets::AssetState::Ready => "ready",
                                    rer_engine_core::assets::AssetState::Failed => "failed",
                                }
                                .to_string()
                            })
                        });
                        crate::engine3d::ipc::ModelInfo {
                            path: path.clone(),
                            name: entry.name.clone(),
                            category: entry.category.clone(),
                            model_id: entry.model_id.clone(),
                            asset: entry.rerasset_path.clone(),
                            state,
                        }
                    })
                    .collect();
                let count = models.len();
                send_event(&EngineEvent::ModelsList { models });
                log::info!("[model] lista enviada: {} modelos", count);
            }
            EngineCommand::Common(EngineCommandCommon::ReloadAsset { path }) => {
                log::info!("[IPC] ReloadAsset: {}", path);
                let key = self.model_path_key(&path);
                if self.static_model_cache.remove(&key).is_some() {
                    self.model_assets.remove(&key);
                    crate::engine3d::config_3d::model_asset::invalidate_gltf_import_cache(
                        std::path::Path::new(&key),
                    );
                    log::info!(
                        "[hot-reload] Caché de modelo invalidada; recarga al volver a instanciar: {}",
                        path
                    );
                } else {
                    log::warn!("[hot-reload] Path no está en caché de modelos 3D: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::LoadSprite { path, name }) => {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        use image::ImageReader;
                        match ImageReader::new(std::io::Cursor::new(&bytes))
                            .with_guessed_format()
                            .map_err(|e| e.to_string())
                            .and_then(|r| r.decode().map_err(|e| e.to_string()))
                        {
                            Ok(img) => {
                                let img = img.to_rgba8();
                                let (w, h) = img.dimensions();
                                self.sprite_store.insert(path.clone(), (name.clone(), w, h));
                                send_event(&EngineEvent::SpriteLoaded {
                                    path,
                                    name,
                                    width: w,
                                    height: h,
                                });
                            }
                            Err(e) => {
                                log::error!("[sprite] error decodificando {}: {}", path, e);
                                send_event(&EngineEvent::Error {
                                    message: format!("Error al decodificar sprite: {e}"),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[sprite] error leyendo {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("No se pudo leer el sprite: {e}"),
                        });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemoveSprite { path }) => {
                if self.sprite_store.remove(&path).is_some() {
                    send_event(&EngineEvent::SpriteRemoved { path: path.clone() });
                    log::info!("[sprite] eliminado: {}", path);
                } else {
                    log::warn!("[sprite] intento de eliminar sprite inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetSpritesList) => {
                let sprites: Vec<crate::engine3d::ipc::SpriteInfo> = self
                    .sprite_store
                    .iter()
                    .map(
                        |(path, &(ref name, w, h))| crate::engine3d::ipc::SpriteInfo {
                            path: path.clone(),
                            name: name.clone(),
                            width: w,
                            height: h,
                        },
                    )
                    .collect();
                let count = sprites.len();
                send_event(&EngineEvent::SpritesList { sprites });
                log::info!("[sprite] lista enviada: {} sprites", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadSound { path, name }) => {
                self.sound_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::SoundLoaded {
                    path: path.clone(),
                    name: name.clone(),
                });
            }
            EngineCommand::Common(EngineCommandCommon::RemoveSound { path }) => {
                if self.sound_store.remove(&path).is_some() {
                    send_event(&EngineEvent::SoundRemoved { path: path.clone() });
                    log::info!("[sound] eliminado: {}", path);
                } else {
                    log::warn!("[sound] intento de eliminar sonido inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetSoundsList) => {
                let sounds: Vec<crate::engine3d::ipc::SoundInfo> = self
                    .sound_store
                    .iter()
                    .map(|(path, name)| crate::engine3d::ipc::SoundInfo {
                        path: path.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let count = sounds.len();
                send_event(&EngineEvent::SoundsList { sounds });
                log::info!("[sound] lista enviada: {} sonidos", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadFont { path, name }) => {
                match validate_font_file(&path) {
                    Ok(()) => {
                        self.font_store.insert(path.clone(), name.clone());
                        send_event(&EngineEvent::FontLoaded {
                            path: path.clone(),
                            name: name.clone(),
                        });
                    }
                    Err(e) => {
                        log::error!("[font] error cargando {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("Error al cargar fuente: {e}"),
                        });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemoveFont { path }) => {
                if self.font_store.remove(&path).is_some() {
                    send_event(&EngineEvent::FontRemoved { path: path.clone() });
                    log::info!("[font] eliminada: {}", path);
                } else {
                    log::warn!("[font] intento de eliminar fuente inexistente: {}", path);
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetFontsList) => {
                let fonts: Vec<crate::engine3d::ipc::FontInfo> = self
                    .font_store
                    .iter()
                    .map(|(path, name)| crate::engine3d::ipc::FontInfo {
                        path: path.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let count = fonts.len();
                send_event(&EngineEvent::FontsList { fonts });
                log::info!("[font] lista enviada: {} fuentes", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadHudImage { path, name }) => {
                match crate::engine3d::hud_image_asset::validate_hud_image_file(&path) {
                    Ok((width_px, height_px)) => {
                        self.hud_image_store.insert(
                            path.clone(),
                            crate::engine3d::hud_image_asset::HudImageAssetMeta {
                                name: name.clone(),
                                width_px,
                                height_px,
                            },
                        );
                        send_event(&EngineEvent::HudImageLoaded {
                            path: path.clone(),
                            name: name.clone(),
                            width: width_px,
                            height: height_px,
                        });
                    }
                    Err(e) => {
                        log::error!("[hud-image] error cargando {}: {}", path, e);
                        send_event(&EngineEvent::Error {
                            message: format!("Error al cargar imagen HUD: {e}"),
                        });
                    }
                }
            }
            EngineCommand::Common(EngineCommandCommon::RemoveHudImage { path }) => {
                if self.hud_image_store.remove(&path).is_some() {
                    send_event(&EngineEvent::HudImageRemoved { path: path.clone() });
                    log::info!("[hud-image] eliminada: {}", path);
                } else {
                    log::warn!(
                        "[hud-image] intento de eliminar imagen inexistente: {}",
                        path
                    );
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetHudImagesList) => {
                let images: Vec<crate::engine3d::ipc::HudImageInfo> = self
                    .hud_image_store
                    .iter()
                    .map(|(path, meta)| crate::engine3d::ipc::HudImageInfo {
                        path: path.clone(),
                        name: meta.name.clone(),
                        width: meta.width_px,
                        height: meta.height_px,
                    })
                    .collect();
                let count = images.len();
                send_event(&EngineEvent::HudImagesList { images });
                log::info!("[hud-image] lista enviada: {} imágenes", count);
            }
            EngineCommand::Common(EngineCommandCommon::LoadBackgroundAsset { path, name }) => {
                self.background_store.insert(path.clone(), name.clone());
                send_event(&EngineEvent::BackgroundAssetLoaded {
                    path: path.clone(),
                    name: name.clone(),
                });
            }
            EngineCommand::Common(EngineCommandCommon::RemoveBackgroundAsset { path }) => {
                if self.background_store.remove(&path).is_some() {
                    send_event(&EngineEvent::BackgroundAssetRemoved { path: path.clone() });
                    log::info!("[background] eliminado: {}", path);
                } else {
                    log::warn!(
                        "[background] intento de eliminar fondo inexistente: {}",
                        path
                    );
                }
            }
            EngineCommand::Common(EngineCommandCommon::GetBackgroundsList) => {
                let backgrounds: Vec<crate::engine3d::ipc::BackgroundInfo> = self
                    .background_store
                    .iter()
                    .map(|(path, name)| crate::engine3d::ipc::BackgroundInfo {
                        path: path.clone(),
                        name: name.clone(),
                    })
                    .collect();
                let count = backgrounds.len();
                send_event(&EngineEvent::BackgroundsList { backgrounds });
                log::info!("[background] lista enviada: {} fondos", count);
            }
            other => return Some(other),
        }
        None
    }
}
