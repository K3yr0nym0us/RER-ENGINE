use std::collections::HashMap;

use rhai::{Dynamic, Engine, Map, Scope, AST, INT};

use super::api::{
    register_native_api, wrap_scene_source, wrap_user_source, ScriptApiContext, ScriptEngineProfile,
};
use super::control::ControlScriptDispatch;
use super::entity_snapshot::EntitySnapshot;
use super::script_cmd::ScriptCmd;

pub type ScriptResult<T> = Result<T, String>;

struct AttachedScript {
    path: String,
    ast: AST,
    started: bool,
}

pub struct ScriptEngine {
    engine: Engine,
    api_ctx: ScriptApiContext,
    profile: ScriptEngineProfile,
    scripts: HashMap<u32, Vec<AttachedScript>>,
    control_script_cache: HashMap<String, AST>,
    scene_ast: Option<AST>,
    scene_id: u32,
    scene_started: bool,
}

impl ScriptEngine {
    pub fn new(profile: ScriptEngineProfile) -> ScriptResult<Self> {
        let api_ctx = ScriptApiContext::new(profile);
        let mut engine = Engine::new();
        register_native_api(&mut engine, &api_ctx);
        Ok(Self {
            engine,
            api_ctx,
            profile,
            scripts: HashMap::new(),
            control_script_cache: HashMap::new(),
            scene_ast: None,
            scene_id: 0,
            scene_started: false,
        })
    }

    pub fn profile(&self) -> ScriptEngineProfile {
        self.profile
    }

    pub fn attach_script(&mut self, entity_id: u32, path: &str, source: &str) -> ScriptResult<()> {
        let wrapped = wrap_user_source(self.profile, source);
        let ast = self
            .engine
            .compile(&wrapped)
            .map_err(|e| format!("Error compilando script '{path}': {e}"))?;
        self.scripts
            .entry(entity_id)
            .or_default()
            .push(AttachedScript {
                path: path.to_string(),
                ast,
                started: false,
            });
        log::debug!("[scripting] script Rhai '{path}' adjuntado a entidad {entity_id}");
        Ok(())
    }

    pub fn detach_entity(&mut self, entity_id: u32) {
        if let Some(scripts) = self.scripts.remove(&entity_id) {
            for s in scripts {
                self.call_lifecycle(&s.ast, "on_stop", entity_id, None);
            }
            log::debug!("[scripting] scripts de entidad {entity_id} removidos");
        }
    }

    pub fn detach_animation_scripts(&mut self, entity_id: u32) {
        let anim: Vec<AttachedScript> = self
            .scripts
            .get(&entity_id)
            .map(|scripts| {
                scripts
                    .iter()
                    .filter(|s| s.path.starts_with("$anim$::"))
                    .map(|s| AttachedScript {
                        path: s.path.clone(),
                        ast: s.ast.clone(),
                        started: s.started,
                    })
                    .collect()
            })
            .unwrap_or_default();

        for s in &anim {
            self.call_lifecycle(&s.ast, "on_stop", entity_id, None);
        }

        if let Some(scripts) = self.scripts.get_mut(&entity_id) {
            scripts.retain(|s| !s.path.starts_with("$anim$::"));
        }
        if anim.is_empty() {
            return;
        }
        log::debug!(
            "[scripting] {} script(s) de animación removidos de entidad {}",
            anim.len(),
            entity_id
        );
    }

    pub fn entity_ids(&self) -> Vec<u32> {
        self.scripts.keys().copied().collect()
    }

    pub fn clear_control_script_cache(&mut self) {
        self.control_script_cache.clear();
        log::debug!("[scripting] caché de control scripts limpiada");
    }

    pub fn load_scene_script(&mut self, scene_id: u32, source: &str) -> ScriptResult<()> {
        self.scene_id = scene_id;
        self.scene_started = false;
        if source.trim().is_empty() {
            self.scene_ast = None;
            return Ok(());
        }
        let wrapped = wrap_scene_source(source);
        let ast = self
            .engine
            .compile(&wrapped)
            .map_err(|e| format!("Error compilando script de escena: {e}"))?;
        self.scene_ast = Some(ast);
        Ok(())
    }

    pub fn clear_scene_script(&mut self) {
        self.scene_ast = None;
        self.scene_id = 0;
        self.scene_started = false;
    }

    pub fn reset_scene_play_state(&mut self) {
        self.scene_started = false;
    }

    pub fn scene_id(&self) -> u32 {
        self.scene_id
    }

    pub fn on_scene_play_start(&mut self) -> Vec<ScriptCmd> {
        if self.scene_started || self.scene_ast.is_none() {
            return vec![];
        }
        self.scene_started = true;
        let Some(ast) = self.scene_ast.clone() else {
            return vec![];
        };
        let mut scope = Scope::new();
        let _ = self
            .engine
            .call_fn::<()>(&mut scope, &ast, "on_scene_start", ());
        self.api_ctx.drain_cmds()
    }

    pub fn tick_scene_script(&mut self, dt: f32) -> Vec<ScriptCmd> {
        let Some(ast) = self.scene_ast.clone() else {
            return vec![];
        };
        let mut scope = Scope::new();
        let _ = self
            .engine
            .call_fn::<()>(&mut scope, &ast, "on_scene_tick", (dt as f64,));
        self.api_ctx.drain_cmds()
    }

    fn control_ast(&mut self, path: &str, source: &str) -> ScriptResult<AST> {
        if let Some(ast) = self.control_script_cache.get(path) {
            return Ok(ast.clone());
        }
        let wrapped = wrap_user_source(self.profile, source);
        let ast = self
            .engine
            .compile(&wrapped)
            .map_err(|e| format!("Error compilando control script '{path}': {e}"))?;
        self.control_script_cache
            .insert(path.to_string(), ast.clone());
        Ok(ast)
    }

    fn run_control_script_dispatch(
        &mut self,
        entity_id: u32,
        control_key: &str,
        path: &str,
        source: &str,
        snapshot: Option<&EntitySnapshot>,
        dispatch: ControlScriptDispatch,
    ) -> ScriptResult<Vec<ScriptCmd>> {
        let ast = self.control_ast(path, source)?;
        let entity = entity_to_dynamic(entity_id, snapshot);
        let mut scope = Scope::new();
        let method = match dispatch {
            ControlScriptDispatch::WhileHeld => "on_keep",
            ControlScriptDispatch::JustPressed => "on_press",
        };
        let _ = self.engine.call_fn::<()>(
            &mut scope,
            &ast,
            method,
            (entity, control_key.to_string()),
        );
        Ok(self.api_ctx.drain_cmds())
    }

    pub fn run_control_script_while_held(
        &mut self,
        entity_id: u32,
        control_key: &str,
        path: &str,
        source: &str,
        snapshot: Option<&EntitySnapshot>,
    ) -> ScriptResult<Vec<ScriptCmd>> {
        self.run_control_script_dispatch(
            entity_id,
            control_key,
            path,
            source,
            snapshot,
            ControlScriptDispatch::WhileHeld,
        )
    }

    pub fn run_control_script_just_pressed(
        &mut self,
        entity_id: u32,
        control_key: &str,
        path: &str,
        source: &str,
        snapshot: Option<&EntitySnapshot>,
    ) -> ScriptResult<Vec<ScriptCmd>> {
        self.run_control_script_dispatch(
            entity_id,
            control_key,
            path,
            source,
            snapshot,
            ControlScriptDispatch::JustPressed,
        )
    }

    pub fn run_control_script(
        &mut self,
        entity_id: u32,
        control_key: &str,
        path: &str,
        source: &str,
        snapshot: Option<&EntitySnapshot>,
    ) -> ScriptResult<Vec<ScriptCmd>> {
        self.run_control_script_while_held(entity_id, control_key, path, source, snapshot)
    }

    pub fn run_trigger_enter_hook(
        &mut self,
        trigger_id: u32,
        actor_id: u32,
        trigger_snapshot: Option<&EntitySnapshot>,
        actor_snapshot: Option<&EntitySnapshot>,
    ) -> ScriptResult<Vec<ScriptCmd>> {
        let trigger = entity_to_dynamic(trigger_id, trigger_snapshot);
        let actor = entity_to_dynamic(actor_id, actor_snapshot);

        if let Some(scripts) = self.scripts.get(&trigger_id) {
            for script in scripts {
                let mut scope = Scope::new();
                if let Err(e) = self.engine.call_fn::<()>(
                    &mut scope,
                    &script.ast,
                    "on_trigger_enter",
                    (trigger.clone(), actor.clone()),
                ) {
                    log::warn!(
                        "[scripting] on_trigger_enter '{}': {e}",
                        script.path
                    );
                }
            }
        } else {
            log::warn!(
                "[trigger] entrada detectada en trigger {trigger_id} pero no tiene scripts adjuntos"
            );
        }

        Ok(self.api_ctx.drain_cmds())
    }

    pub fn tick(
        &mut self,
        dt: f32,
        snapshots: &HashMap<u32, EntitySnapshot>,
        player_ui_active_screen: Option<&str>,
    ) -> Vec<ScriptCmd> {
        if self.profile == ScriptEngineProfile::Engine3d {
            if let Ok(mut guard) = self.api_ctx.player_ui_active_screen.lock() {
                *guard = player_ui_active_screen.map(str::to_string);
            }
        }

        let entity_ids: Vec<u32> = self.scripts.keys().copied().collect();

        let mut pending_starts: Vec<(u32, usize)> = Vec::new();
        for entity_id in &entity_ids {
            if let Some(scripts) = self.scripts.get_mut(entity_id) {
                for (idx, script) in scripts.iter_mut().enumerate() {
                    if !script.started {
                        script.started = true;
                        pending_starts.push((*entity_id, idx));
                    }
                }
            }
        }

        for (entity_id, idx) in pending_starts {
            let snapshot = snapshots.get(&entity_id);
            let ast = &self.scripts.get(&entity_id).unwrap()[idx].ast;
            self.call_lifecycle(ast, "on_start", entity_id, snapshot);
        }

        for entity_id in entity_ids {
            let snapshot = snapshots.get(&entity_id);
            let entity = entity_to_dynamic(entity_id, snapshot);

            if let Some(scripts) = self.scripts.get(&entity_id) {
                for script in scripts {
                    let mut scope = Scope::new();
                    if let Err(e) = self.engine.call_fn::<()>(
                        &mut scope,
                        &script.ast,
                        "update",
                        (entity.clone(), dt as f64),
                    ) {
                        log::warn!("[scripting] update '{}': {e}", script.path);
                    }
                }
            }
        }

        self.api_ctx.drain_cmds()
    }

    fn call_lifecycle(
        &self,
        ast: &AST,
        method: &str,
        entity_id: u32,
        snapshot: Option<&EntitySnapshot>,
    ) {
        let entity = entity_to_dynamic(entity_id, snapshot);
        let mut scope = Scope::new();
        if let Err(e) = self.engine.call_fn::<()>(&mut scope, ast, method, (entity,)) {
            log::warn!("[scripting] {method}: {e}");
        }
    }
}

fn entity_to_dynamic(entity_id: u32, snap: Option<&EntitySnapshot>) -> Dynamic {
    let mut map = Map::new();
    map.insert("id".into(), Dynamic::from(entity_id as INT));
    if let Some(s) = snap {
        map.insert("x".into(), Dynamic::from(s.x as f64));
        map.insert("y".into(), Dynamic::from(s.y as f64));
        map.insert("scale_x".into(), Dynamic::from(s.scale_x as f64));
        map.insert("scale_y".into(), Dynamic::from(s.scale_y as f64));
        map.insert("facing_right".into(), Dynamic::from(s.facing_right));
        map.insert("facing_sign".into(), Dynamic::from(s.facing_sign as f64));
        let anims: rhai::Array = s
            .animations
            .iter()
            .map(|n| Dynamic::from(n.clone()))
            .collect();
        map.insert("animations".into(), Dynamic::from(anims));
    }
    Dynamic::from_map(map)
}
