use super::*;

/// Guardado en curso: esperando el estado de la UI y/o el snapshot del motor.
pub struct PendingSave {
    pub id: u64,
    pub save_path: PathBuf,
    pub state: Option<ProjectState>,
    pub silent: bool,
    /// El hilo de escritura ya arrancó (el timeout deja de aplicar).
    pub writing: bool,
}

fn pick_save_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("RER Save", &["save"])
        .set_file_name("RER-Project.save")
        .save_file()
}

impl EditorApp {
    pub fn can_save(&self) -> bool {
        self.status == EditorStatus::Ready && !self.saving
    }

    /// Inicia un guardado (con diálogo o silencioso para autosave).
    /// Devuelve el id de petición para poder armar el timeout.
    pub fn begin_save(&mut self, silent: bool) -> Option<u64> {
        if !self.can_save() {
            return None;
        }

        let save_path = match self.save_path.clone() {
            Some(path) => path,
            None if silent => return None,
            None => pick_save_path()?,
        };

        self.request_seq = self.request_seq.wrapping_add(1);
        let id = self.request_seq;
        self.saving = true;
        self.save_error = None;
        self.push_log(
            if silent {
                "[Autoguardado] guardando…".to_string()
            } else {
                format!("[Guardar] guardando en {}", save_path.display())
            },
            false,
        );
        self.pending_save = Some(PendingSave {
            id,
            save_path,
            state: None,
            silent,
            writing: false,
        });
        crate::editor::bridge::request_project_state(id);
        self.spawn_save_timeout(id);
        Some(id)
    }

    /// Recibe el estado de proyecto que envió la UI React.
    pub fn receive_project_state(&mut self, id: u64, state: &Value) -> bool {
        let Some(pending) = self.pending_save.as_mut() else {
            return false;
        };
        if pending.id != id || pending.state.is_some() {
            return false;
        }
        let parsed = ProjectState::from_value(state);
        pending.state = Some(parsed);
        self.send_engine(serde_json::json!({
            "cmd": "export_save_snapshot"
        }));
        true
    }

    /// Extrae el estado pendiente cuando llega el snapshot del motor.
    pub fn take_pending_save(&mut self) -> Option<(PathBuf, ProjectState)> {
        let pending = self.pending_save.as_mut()?;
        let state = pending.state.clone()?;
        pending.writing = true;
        let save_path = pending.save_path.clone();
        Some((save_path, state))
    }

    /// Finaliza el guardado en curso con el resultado del hilo de escritura.
    pub fn complete_save(&mut self, result: Result<SaveSuccess, String>) {
        let Some(pending) = self.pending_save.take() else {
            return;
        };
        self.saving = false;

        match result {
            Ok(success) => {
                self.save_path = Some(success.save_path.clone());
                self.has_saved_once = true;
                self.save_error = None;
                let prefix = if pending.silent {
                    "[Autoguardado]"
                } else {
                    "[Guardar]"
                };
                self.push_log(
                    format!(
                        "{prefix} {} ({} KB)",
                        success.file_name(),
                        success.bytes / 1024
                    ),
                    false,
                );
                self.notify_engine_project_saved();
                crate::editor::bridge::notify_project_saved(&success.save_path);
            }
            Err(message) => {
                self.save_error = Some(message.clone());
                self.push_log(format!("[Guardar] error: {message}"), true);
                crate::editor::bridge::notify_save_failed(&message);
            }
        }
    }

    /// Falla un guardado que excedió el tiempo de espera.
    pub fn timeout_save(&mut self, id: u64) {
        let Some(pending) = self.pending_save.as_ref() else {
            return;
        };
        if pending.id != id {
            return;
        }
        if pending.writing {
            return;
        }
        self.pending_save = None;
        self.saving = false;
        let message = self.text().save_timeout.to_string();
        self.save_error = Some(message.clone());
        self.push_log(message.clone(), true);
        crate::editor::bridge::notify_save_failed(&message);
    }

    /// Lanza un hilo que dispara el timeout si el guardado no arranca.
    fn spawn_save_timeout(&self, id: u64) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        std::thread::Builder::new()
            .name("save-timeout".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_secs(20));
                let _ = proxy.send_event(UserEvent::SaveTimeout(id));
            })
            .ok();
    }

    /// Autosave disparado por el motor (`autosave_tick`).
    pub fn on_autosave_tick(&mut self) -> Option<u64> {
        if !self.auto_save_enabled || !self.has_saved_once || self.saving {
            return None;
        }
        self.begin_save(true)
    }

    pub fn toggle_auto_save(&mut self) {
        if !self.has_saved_once {
            return;
        }
        self.auto_save_enabled = !self.auto_save_enabled;
        self.send_engine(serde_json::json!({
            "cmd": "set_autosave",
            "enabled": self.auto_save_enabled
        }));
    }

    pub fn project_type_string(&self, state: &ProjectState) -> String {
        state
            .project_type
            .clone()
            .unwrap_or_else(|| self.cli.context.project_type.as_str().to_string())
    }

    pub fn game_style_string(&self, state: &ProjectState) -> String {
        state
            .game_style
            .clone()
            .unwrap_or_else(|| self.cli.context.game_style.as_str().to_string())
    }

    fn notify_engine_project_saved(&self) {
        if !self.cli.context.is_3d() {
            return;
        }
        let Some(extract_dir) = self.extract_dir.as_ref() else {
            return;
        };
        // Solo si el motor ya está montando un proyecto extraído con manifiesto.
        if !extract_dir.join("manifest.json").is_file() {
            return;
        }
        self.send_engine(serde_json::json!({
            "cmd": "notify_project_saved",
            "extract_dir": extract_dir.to_string_lossy(),
        }));
    }
}
