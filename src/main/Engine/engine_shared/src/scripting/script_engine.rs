use std::collections::HashMap;

use rhai::{AST, Dynamic, Engine, INT, Map, Scope};

use super::api::{
    ScriptApiContext, ScriptEngineProfile, register_native_api, wrap_scene_source, wrap_user_source,
};
use super::control::ControlScriptDispatch;
use super::entity_snapshot::EntitySnapshot;
use super::script_cmd::ScriptCmd;

pub type ScriptResult<T> = Result<T, String>;

struct AttachedScript {
    path: String,
    user_source: String,
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
    scene_user_source: Option<String>,
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
            scene_user_source: None,
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
                user_source: source.to_string(),
                ast,
                started: false,
            });

        Ok(())
    }

    pub fn detach_entity(&mut self, entity_id: u32) {
        if let Some(scripts) = self.scripts.remove(&entity_id) {
            for s in scripts {
                self.run_entity_method(
                    &s.path,
                    &s.user_source,
                    &s.ast,
                    "on_stop",
                    "on_stop!(entity);",
                    entity_id,
                    None,
                );
            }
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
                        user_source: s.user_source.clone(),
                        ast: s.ast.clone(),
                        started: s.started,
                    })
                    .collect()
            })
            .unwrap_or_default();

        for s in &anim {
            self.run_entity_method(
                &s.path,
                &s.user_source,
                &s.ast,
                "on_stop",
                "on_stop!(entity);",
                entity_id,
                None,
            );
        }

        if let Some(scripts) = self.scripts.get_mut(&entity_id) {
            scripts.retain(|s| !s.path.starts_with("$anim$::"));
        }
    }

    pub fn entity_ids(&self) -> Vec<u32> {
        self.scripts.keys().copied().collect()
    }

    pub fn entity_has_scripts(&self, entity_id: u32) -> bool {
        self.scripts
            .get(&entity_id)
            .is_some_and(|scripts| !scripts.is_empty())
    }

    pub fn clear_control_script_cache(&mut self) {
        self.control_script_cache.clear();
    }

    pub fn sync_graphics_texture_tier_readback(&self, tier: &str) {
        if self.profile != ScriptEngineProfile::Engine3d {
            return;
        }
        if let Ok(mut guard) = self.api_ctx.graphics_texture_tier.lock() {
            *guard = tier.to_string();
        }
    }

    pub fn sync_reflection_tier_readback(&self, tier: &str) {
        if self.profile != ScriptEngineProfile::Engine3d {
            return;
        }
        if let Ok(mut guard) = self.api_ctx.reflection_tier.lock() {
            *guard = tier.to_string();
        }
    }

    pub fn load_scene_script(&mut self, scene_id: u32, source: &str) -> ScriptResult<()> {
        self.scene_id = scene_id;
        self.scene_started = false;
        if source.trim().is_empty() {
            self.scene_ast = None;
            self.scene_user_source = None;
            return Ok(());
        }
        self.scene_user_source = Some(source.to_string());
        let wrapped = wrap_scene_source(self.profile, source);
        let ast = self
            .engine
            .compile(&wrapped)
            .map_err(|e| format!("Error compilando script de escena: {e}"))?;
        self.scene_ast = Some(ast);
        Ok(())
    }

    pub fn clear_scene_script(&mut self) {
        self.scene_ast = None;
        self.scene_user_source = None;
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
        let user_source = self.scene_user_source.clone().unwrap_or_default();
        self.run_scene_method(
            &user_source,
            &ast,
            "on_scene_start",
            "on_scene_start!();",
            None,
        );
        self.api_ctx.drain_cmds()
    }

    pub fn tick_scene_script(&mut self, dt: f32) -> Vec<ScriptCmd> {
        let Some(ast) = self.scene_ast.clone() else {
            return vec![];
        };
        let user_source = self.scene_user_source.clone().unwrap_or_default();
        self.run_scene_method(
            &user_source,
            &ast,
            "on_scene_tick",
            "on_scene_tick!(dt);",
            Some(dt),
        );
        self.api_ctx.drain_cmds()
    }

    fn source_defines_fn(source: &str, method: &str) -> bool {
        source.contains(&format!("fn {method}(")) || source.contains(&format!("fn {method} ("))
    }

    fn invoke_script_ast<F>(
        &mut self,
        cache_key: &str,
        user_source: &str,
        invoke_suffix: &str,
        wrap: F,
    ) -> ScriptResult<AST>
    where
        F: Fn(&str) -> String,
    {
        if let Some(ast) = self.control_script_cache.get(cache_key) {
            return Ok(ast.clone());
        }
        let exec_source = if invoke_suffix.is_empty() {
            user_source.to_string()
        } else {
            format!("{user_source}\n\n{invoke_suffix}")
        };
        let wrapped = wrap(&exec_source);
        let ast = self
            .engine
            .compile(&wrapped)
            .map_err(|e| format!("Error compilando script '{cache_key}': {e}"))?;
        self.control_script_cache
            .insert(cache_key.to_string(), ast.clone());
        Ok(ast)
    }

    fn control_ast(
        &mut self,
        path: &str,
        source: &str,
        method: &str,
        invoke_callback: bool,
    ) -> ScriptResult<AST> {
        let cache_key = if invoke_callback {
            format!("{path}::{method}::invoke")
        } else {
            path.to_string()
        };
        let invoke_suffix = if invoke_callback {
            format!("{method}!(entity, control_key);")
        } else {
            String::new()
        };
        let profile = self.profile;
        self.invoke_script_ast(&cache_key, source, &invoke_suffix, move |s| {
            wrap_user_source(profile, s)
        })
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
        let method = match dispatch {
            ControlScriptDispatch::WhileHeld => "on_keep",
            ControlScriptDispatch::JustPressed => "on_press",
        };
        let has_callback = Self::source_defines_fn(source, method);
        let ast = self.control_ast(path, source, method, has_callback)?;
        let entity = entity_to_dynamic(entity_id, snapshot);
        let mut scope = Scope::new();
        scope.push("entity", entity);
        scope.push("control_key", control_key.to_string());

        if let Ok(mut guard) = self.api_ctx.control_binding_key.lock() {
            *guard = control_key.to_string();
        }

        // Lua-style: ejecutar chunk (preámbulo + statements, o fn + `on_keep!(entity, control_key)`).
        let run_result = self.engine.run_ast_with_scope(&mut scope, &ast);

        if let Ok(mut guard) = self.api_ctx.control_binding_key.lock() {
            guard.clear();
        }

        if let Err(e) = run_result {
            return Err(format!("Error ejecutando control script '{path}': {e}"));
        }

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
            let scripts: Vec<_> = scripts
                .iter()
                .map(|s| (s.path.clone(), s.user_source.clone(), s.ast.clone()))
                .collect();
            for (path, user_source, ast) in scripts {
                if Self::source_defines_fn(&user_source, "on_trigger_enter") {
                    let cache_key = format!("{path}::on_trigger_enter::invoke");
                    let profile = self.profile;
                    match self.invoke_script_ast(
                        &cache_key,
                        &user_source,
                        "on_trigger_enter!(trigger, actor);",
                        move |s| wrap_user_source(profile, s),
                    ) {
                        Ok(ast) => {
                            let mut scope = Scope::new();
                            scope.push("trigger", trigger.clone());
                            scope.push("actor", actor.clone());
                            if let Err(e) = self.engine.run_ast_with_scope(&mut scope, &ast) {
                                log::warn!("[scripting] on_trigger_enter '{path}': {e}");
                            }
                        }
                        Err(e) => {
                            log::warn!("[scripting] on_trigger_enter compile '{path}': {e}");
                        }
                    }
                } else {
                    let mut scope = Scope::new();
                    if let Err(e) = self.engine.call_fn::<()>(
                        &mut scope,
                        &ast,
                        "on_trigger_enter",
                        (trigger.clone(), actor.clone()),
                    ) {
                        log::warn!("[scripting] on_trigger_enter '{path}': {e}");
                    }
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
        if self.profile == ScriptEngineProfile::Engine3d
            && let Ok(mut guard) = self.api_ctx.player_ui_active_screen.lock()
        {
            *guard = player_ui_active_screen.map(str::to_string);
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
            let (path, user_source, ast) = {
                let script = &self.scripts.get(&entity_id).unwrap()[idx];
                (
                    script.path.clone(),
                    script.user_source.clone(),
                    script.ast.clone(),
                )
            };
            self.run_entity_method(
                &path,
                &user_source,
                &ast,
                "on_start",
                "on_start!(entity);",
                entity_id,
                snapshot,
            );
        }

        for entity_id in entity_ids {
            let snapshot = snapshots.get(&entity_id);
            let entity = entity_to_dynamic(entity_id, snapshot);

            let scripts: Vec<_> = self
                .scripts
                .get(&entity_id)
                .map(|scripts| {
                    scripts
                        .iter()
                        .map(|s| (s.path.clone(), s.user_source.clone(), s.ast.clone()))
                        .collect()
                })
                .unwrap_or_default();

            for (path, user_source, ast) in scripts {
                if Self::source_defines_fn(&user_source, "update") {
                    let cache_key = format!("{path}::update::invoke");
                    let profile = self.profile;
                    match self.invoke_script_ast(
                        &cache_key,
                        &user_source,
                        "update!(entity, dt);",
                        move |s| wrap_user_source(profile, s),
                    ) {
                        Ok(ast) => {
                            let mut scope = Scope::new();
                            scope.push("entity", entity.clone());
                            scope.push("dt", dt as f64);
                            if let Err(e) = self.engine.run_ast_with_scope(&mut scope, &ast) {
                                log::warn!("[scripting] update '{path}': {e}");
                            }
                        }
                        Err(e) => log::warn!("[scripting] update compile '{path}': {e}"),
                    }
                } else {
                    let mut scope = Scope::new();
                    if let Err(e) = self.engine.call_fn::<()>(
                        &mut scope,
                        &ast,
                        "update",
                        (entity.clone(), dt as f64),
                    ) {
                        log::warn!("[scripting] update '{path}': {e}");
                    }
                }
            }
        }

        self.api_ctx.drain_cmds()
    }

    fn run_entity_method(
        &mut self,
        path: &str,
        user_source: &str,
        base_ast: &AST,
        method: &str,
        invoke_suffix: &str,
        entity_id: u32,
        snapshot: Option<&EntitySnapshot>,
    ) {
        let entity = entity_to_dynamic(entity_id, snapshot);
        if Self::source_defines_fn(user_source, method) {
            let cache_key = format!("{path}::{method}::invoke");
            let profile = self.profile;
            match self.invoke_script_ast(&cache_key, user_source, invoke_suffix, move |s| {
                wrap_user_source(profile, s)
            }) {
                Ok(ast) => {
                    let mut scope = Scope::new();
                    scope.push("entity", entity);
                    if let Err(e) = self.engine.run_ast_with_scope(&mut scope, &ast) {
                        log::warn!("[scripting] {method} '{path}': {e}");
                    }
                }
                Err(e) => log::warn!("[scripting] compile {method} '{path}': {e}"),
            }
        } else {
            let mut scope = Scope::new();
            if let Err(e) = self
                .engine
                .call_fn::<()>(&mut scope, base_ast, method, (entity,))
            {
                log::warn!("[scripting] {method}: {e}");
            }
        }
    }

    fn run_scene_method(
        &mut self,
        user_source: &str,
        base_ast: &AST,
        method: &str,
        invoke_suffix: &str,
        dt: Option<f32>,
    ) {
        let cache_key = format!("scene::{method}::invoke");
        if Self::source_defines_fn(user_source, method) {
            let profile = self.profile;
            match self.invoke_script_ast(&cache_key, user_source, invoke_suffix, |src| {
                wrap_scene_source(profile, src)
            }) {
                Ok(ast) => {
                    let mut scope = Scope::new();
                    if let Some(dt) = dt {
                        scope.push("dt", dt as f64);
                    }
                    if let Err(e) = self.engine.run_ast_with_scope(&mut scope, &ast) {
                        log::warn!("[scripting] {method}: {e}");
                    }
                }
                Err(e) => log::warn!("[scripting] compile {method}: {e}"),
            }
        } else {
            let mut scope = Scope::new();
            let result = if let Some(dt) = dt {
                self.engine
                    .call_fn::<()>(&mut scope, base_ast, method, (dt as f64,))
            } else {
                self.engine.call_fn::<()>(&mut scope, base_ast, method, ())
            };
            if let Err(e) = result {
                log::warn!("[scripting] {method}: {e}");
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripting::api::{ScriptApiContext, register_native_api, wrap_user_source};

    fn compile_control(profile: ScriptEngineProfile, source: &str) -> Result<(), String> {
        let mut engine = ScriptEngine::new(profile).unwrap();
        engine
            .control_ast("test_control", source, "on_keep", false)
            .map(|_| ())
    }

    #[test]
    fn preamble_only_compiles() {
        let wrapped = wrap_user_source(ScriptEngineProfile::Engine3d, "");
        let mut engine = Engine::new();
        register_native_api(
            &mut engine,
            &ScriptApiContext::new(ScriptEngineProfile::Engine3d),
        );
        engine.compile(&wrapped).expect("preamble only");
    }

    #[test]
    fn default_fp_move_control_compiles() {
        let source = "let WALK_SPEED = 4;\nengine.fp_set_walk_speed(WALK_SPEED);\nengine.fp_press_key(\"W\");\n";
        compile_control(ScriptEngineProfile::Engine3d, source).expect("fp move control");
    }

    #[test]
    fn control_bare_body_produces_fp_press_key() {
        use crate::scripting::ScriptCmd;
        let source = "let WALK_SPEED = 4.0;\nengine.fp_set_walk_speed(WALK_SPEED);\nengine.fp_press_key(\"W\");\n";
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine3d).unwrap();
        let cmds = se
            .run_control_script_while_held(1, "W", "fp_move_forward", source, None)
            .expect("run bare body");
        assert!(
            cmds.iter()
                .any(|c| matches!(c, ScriptCmd::PlayControllerPressKey { key } if key == "W")),
            "expected fp_press_key cmd, got {cmds:?}"
        );
    }

    #[test]
    fn control_bare_body_int_walk_speed_produces_fp_press_key() {
        use crate::scripting::ScriptCmd;
        let source = "let WALK_SPEED = 4;\nengine.fp_set_walk_speed(WALK_SPEED);\nengine.fp_press_key(\"S\");\n";
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine3d).unwrap();
        let cmds = se
            .run_control_script_while_held(1, "S", "fp_move_back", source, None)
            .expect("run bare body with int walk speed");
        assert!(
            cmds.iter().any(|c| matches!(
                c,
                ScriptCmd::PlayControllerSetWalkSpeed(s) if (*s - 4.0).abs() < f32::EPSILON
            )),
            "expected walk speed cmd, got {cmds:?}"
        );
        assert!(
            cmds.iter()
                .any(|c| matches!(c, ScriptCmd::PlayControllerPressKey { key } if key == "S")),
            "expected fp_press_key cmd, got {cmds:?}"
        );
    }

    #[test]
    fn entity_on_start_callback_sees_engine() {
        use crate::scripting::ScriptCmd;
        let source = r#"fn on_start(entity) {
    engine.log("started");
}"#;
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine3d).unwrap();
        se.attach_script(1, "test_entity", source).expect("attach");
        let cmds = se.tick(0.016, &HashMap::new(), None);
        assert!(
            cmds.iter()
                .any(|c| matches!(c, ScriptCmd::Log { message } if message == "started")),
            "expected log cmd from on_start, got {cmds:?}"
        );
    }

    #[test]
    fn control_on_keep_callback_produces_fp_press_key() {
        use crate::scripting::ScriptCmd;
        let source = r#"fn on_keep(entity, control_key) {
    engine.fp_press_key("W");
}"#;
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine3d).unwrap();
        let cmds = se
            .run_control_script_while_held(1, "W", "fp_move_callback", source, None)
            .expect("run on_keep callback");
        assert!(
            cmds.iter()
                .any(|c| matches!(c, ScriptCmd::PlayControllerPressKey { key } if key == "W")),
            "expected fp_press_key cmd, got {cmds:?}"
        );
    }

    #[test]
    fn entity_update_int_move_entity_produces_cmd() {
        use crate::scripting::ScriptCmd;
        let source = r#"fn update(entity, dt) {
    engine.move_entity(entity.id, 4, 1, 0);
}"#;
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine2d).unwrap();
        se.attach_script(1, "move_right", source).expect("attach");
        let cmds = se.tick(0.016, &HashMap::new(), None);
        assert!(
            cmds.iter().any(|c| matches!(
                c,
                ScriptCmd::MoveEntity { id: 1, speed, dir_x, dir_y }
                    if *speed == 4.0 && *dir_x == 1.0 && *dir_y == 0.0
            )),
            "expected move_entity cmd, got {cmds:?}"
        );
    }

    #[test]
    fn control_move_control_maps_binding_key() {
        use crate::scripting::ScriptCmd;
        let source = r#"fn on_keep(entity, control_key) {
    engine.move_control(entity.id, 7.0);
    engine.play_animation(entity.id, "Run");
}"#;
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine2d).unwrap();
        let cmds = se
            .run_control_script_while_held(1, "D", "move_right", source, None)
            .expect("run move_control");
        assert!(
            cmds.iter().any(|c| matches!(
                c,
                ScriptCmd::MoveEntity { id: 1, speed, dir_x, dir_y }
                    if (*speed - 7.0).abs() < f32::EPSILON && *dir_x == 1.0 && *dir_y == 0.0
            )),
            "expected move_entity from move_control, got {cmds:?}"
        );
    }

    #[test]
    fn scene_on_scene_start_callback_sees_engine() {
        use crate::scripting::ScriptCmd;
        let source = r#"fn on_scene_start() {
    engine.log("scene ready");
}"#;
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine3d).unwrap();
        se.load_scene_script(1, source).expect("load scene");
        let cmds = se.on_scene_play_start();
        assert!(
            cmds.iter()
                .any(|c| matches!(c, ScriptCmd::Log { message } if message == "scene ready")),
            "expected log cmd from on_scene_start, got {cmds:?}"
        );
    }

    #[test]
    fn trigger_on_trigger_enter_callback_sees_engine() {
        use crate::scripting::ScriptCmd;
        let source = r#"fn on_trigger_enter(trigger, actor) {
    engine.log("triggered");
}"#;
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine2d).unwrap();
        se.attach_script(10, "trigger_script", source)
            .expect("attach");
        let cmds = se
            .run_trigger_enter_hook(10, 20, None, None)
            .expect("trigger hook");
        assert!(
            cmds.iter()
                .any(|c| matches!(c, ScriptCmd::Log { message } if message == "triggered")),
            "expected log cmd from on_trigger_enter, got {cmds:?}"
        );
    }

    #[test]
    fn control_on_press_callback_produces_cmd() {
        use crate::scripting::ScriptCmd;
        let source = r#"fn on_press(entity, control_key) {
    engine.fp_press_key("SPACE");
}"#;
        let mut se = ScriptEngine::new(ScriptEngineProfile::Engine3d).unwrap();
        let cmds = se
            .run_control_script_just_pressed(1, "SPACE", "fp_jump_press", source, None)
            .expect("run on_press callback");
        assert!(
            cmds.iter()
                .any(|c| matches!(c, ScriptCmd::PlayControllerPressKey { key } if key == "SPACE")),
            "expected fp_press_key cmd, got {cmds:?}"
        );
    }

    /// Compiles Rhai control scripts from the embedded 2D demo (and optional 3D DEMO save).
    #[test]
    fn demo_save_rhai_sources_compile() {
        use std::io::Read;
        use std::path::{Path, PathBuf};

        fn collect_rhai_dir(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_rhai_dir(&path, out);
                } else if path.extension().is_some_and(|e| e == "rhai")
                    && let Ok(source) = std::fs::read_to_string(&path)
                {
                    out.push((path, source));
                }
            }
        }

        fn collect_rhai_zip(save: &Path, out: &mut Vec<(PathBuf, String)>) {
            let Ok(file) = std::fs::File::open(save) else {
                return;
            };
            let Ok(mut archive) = zip::read::ZipArchive::new(file) else {
                return;
            };
            for i in 0..archive.len() {
                let Ok(mut entry) = archive.by_index(i) else {
                    continue;
                };
                let name = entry.name().to_string();
                if !name.ends_with(".rhai") {
                    continue;
                }
                let mut source = String::new();
                if entry.read_to_string(&mut source).is_ok() {
                    out.push((PathBuf::from(name), source));
                }
            }
        }

        let engine_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let repo = engine_root.join("../..");
        let mut sources: Vec<(ScriptEngineProfile, PathBuf, String)> = Vec::new();

        let save_2d = engine_root.join("engine_2d/assets/DEMO_2d.save");
        let save_3d = repo.join("DEMO_3d_FIRST_PERSON.save");
        let tmp_2d = repo.join(".tmp_demo2d");
        let tmp_3d = repo.join(".tmp_demo3d");

        let mut from_2d = Vec::new();
        if tmp_2d.is_dir() {
            collect_rhai_dir(&tmp_2d, &mut from_2d);
        } else if save_2d.is_file() {
            collect_rhai_zip(&save_2d, &mut from_2d);
        }
        let from_2d_count = from_2d.len();
        for (path, source) in from_2d {
            sources.push((ScriptEngineProfile::Engine2d, path, source));
        }

        let mut from_3d = Vec::new();
        if tmp_3d.is_dir() {
            collect_rhai_dir(&tmp_3d, &mut from_3d);
        } else if save_3d.is_file() {
            collect_rhai_zip(&save_3d, &mut from_3d);
        }
        for (path, source) in from_3d {
            sources.push((ScriptEngineProfile::Engine3d, path, source));
        }

        assert!(
            from_2d_count >= 5,
            "expected at least 5 .rhai sources from embedded DEMO_2d.save, got {from_2d_count}"
        );

        for (profile, path, source) in sources {
            let mut engine = Engine::new();
            register_native_api(&mut engine, &ScriptApiContext::new(profile));
            let wrapped = wrap_user_source(profile, &source);
            engine
                .compile(&wrapped)
                .unwrap_or_else(|e| panic!("compile {}: {e}", path.display()));
        }
    }
}
