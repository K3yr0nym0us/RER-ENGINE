use super::*;

impl State {
    pub(super) fn cmd_system(&mut self, cmd: EngineCommand) -> Option<EngineCommand> {
        match cmd {
            EngineCommand::Common(EngineCommandCommon::Ping) => {
                send_event(&EngineEvent::Pong);
            }
            EngineCommand::Common(EngineCommandCommon::SetClearColor { r, g, b }) => {
                self.clear_color = [r as f32, g as f32, b as f32, 1.0f32];
            }
            EngineCommand::Common(EngineCommandCommon::Resize { width, height }) => {
                if !self.embedded {
                    self.resize(PhysicalSize::new(width, height));
                }
            }
            EngineCommand::Common(EngineCommandCommon::PlayAudio { path, loop_ }) => {
                let decoded = std::fs::read(&path).ok().and_then(|b| {
                    let cursor = std::io::Cursor::new(b);
                    rodio::Decoder::new(cursor).ok().map(|dec| {
                        let ch = dec.channels().get();
                        let sr = dec.sample_rate().get();
                        let s: Vec<f32> = dec.collect();
                        Arc::new(DecodedAudio {
                            samples: s,
                            channels: ch,
                            sample_rate: sr,
                        })
                    })
                });
                match decoded {
                    Some(audio) => self.play_audio_internal(audio, loop_),
                    None => log::error!("[audio] no se pudo cargar o decodificar: {path}"),
                }
            }
            EngineCommand::Common(EngineCommandCommon::StopAudio) => {
                self.stop_audio_internal();
                log::info!("[audio] detenido por comando externo");
            }
            EngineCommand::Common(EngineCommandCommon::SetTargetFps { fps }) => {
                self.target_fps = fps.clamp(1, 1000);
                log::info!("[render] Límite de FPS actualizado: {}", self.target_fps);
            }
            EngineCommand::Common(EngineCommandCommon::SetCtrlHeld { held }) => {
                self.ctrl_held = held;
            }
            EngineCommand::Common(EngineCommandCommon::Undo) => {
                self.apply_undo();
            }
            EngineCommand::Common(EngineCommandCommon::Redo) => {
                self.apply_redo();
            }
            EngineCommand::Common(EngineCommandCommon::SetLocale { locale }) => {
                log::info!("[IPC] SetLocale: {}", locale);
                self.snap_locale = locale;
            }
            EngineCommand::Common(EngineCommandCommon::SetAutosave { enabled }) => {
                self.autosave_enabled = enabled;
                self.autosave_last_tick = Instant::now();
                log::info!(
                    "[autosave] {}",
                    if enabled { "activado" } else { "desactivado" }
                );
            }
            EngineCommand::Common(EngineCommandCommon::ExportSaveSnapshot) => {
                self.export_save_snapshot();
            }
            EngineCommand::Common(EngineCommandCommon::ResendAllModelClips) => {
                self.resend_all_model_clips_ready();
            }
            EngineCommand::Common(EngineCommandCommon::Shutdown) => {}
            other => return Some(other),
        }
        None
    }
}
