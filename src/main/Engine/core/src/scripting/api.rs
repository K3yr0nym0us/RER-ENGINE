use std::sync::{Arc, Mutex};

use rhai::{Dynamic, Engine, INT, Map};

use crate::i18n::{LocaleId, TextArguments};

use super::i18n::ScriptI18nState;
use super::interaction::InteractionState;
use super::play_script_input::PlayScriptInput;
use super::script_cmd::ScriptCmd;

/// Dirección XY estándar para una tecla de control (2D / side-scroller).
fn control_key_move_dir(key: &str) -> (f32, f32) {
    match key {
        "A" | "D-LEFT" => (-1.0, 0.0),
        "D" | "D-RIGHT" => (1.0, 0.0),
        "W" | "D-UP" => (0.0, 1.0),
        "S" | "D-DOWN" => (0.0, -1.0),
        _ => (0.0, 0.0),
    }
}

macro_rules! register_num_fn1 {
    ($engine:expr, $ctx:expr, $name:expr, |$p:ident| $cmd:expr) => {{
        {
            let c = $ctx.clone();
            $engine.register_fn($name, move |$p: f64| {
                c.clone().push($cmd);
            });
        }
        {
            let c = $ctx.clone();
            $engine.register_fn($name, move |$p: INT| {
                c.clone().push($cmd);
            });
        }
    }};
}

macro_rules! register_id_num2 {
    ($engine:expr, $ctx:expr, $name:expr, |$id:ident, $a:ident, $b:ident| $cmd:expr) => {{
        {
            let c = $ctx.clone();
            $engine.register_fn($name, move |$id: i64, $a: f64, $b: f64| {
                c.clone().push($cmd);
            });
        }
        {
            let c = $ctx.clone();
            $engine.register_fn($name, move |$id: i64, $a: INT, $b: INT| {
                c.clone().push($cmd);
            });
        }
        {
            let c = $ctx.clone();
            $engine.register_fn($name, move |$id: i64, $a: INT, $b: f64| {
                c.clone().push($cmd);
            });
        }
        {
            let c = $ctx.clone();
            $engine.register_fn($name, move |$id: i64, $a: f64, $b: INT| {
                c.clone().push($cmd);
            });
        }
    }};
}

macro_rules! register_id_num3 {
    ($engine:expr, $ctx:expr, $name:expr, |$id:ident, $a:ident, $b:ident, $c:ident| $cmd:expr) => {{
        macro_rules! reg3 {
            ($t1:ty, $t2:ty, $t3:ty) => {{
                let api = $ctx.clone();
                $engine.register_fn($name, move |$id: i64, $a: $t1, $b: $t2, $c: $t3| {
                    api.clone().push($cmd);
                });
            }};
        }
        reg3!(f64, f64, f64);
        reg3!(INT, INT, INT);
        reg3!(INT, INT, f64);
        reg3!(INT, f64, INT);
        reg3!(INT, f64, f64);
        reg3!(f64, INT, INT);
        reg3!(f64, INT, f64);
        reg3!(f64, f64, INT);
    }};
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptEngineProfile {
    Engine2d,
    Engine3d,
}

#[derive(Clone)]
pub struct ScriptApiContext {
    pub cmds: Arc<Mutex<Vec<ScriptCmd>>>,
    pub player_ui_active_screen: Arc<Mutex<Option<String>>>,
    pub graphics_texture_tier: Arc<Mutex<String>>,
    pub reflection_tier: Arc<Mutex<String>>,
    /// Tecla del binding activo en scripts de control 2D (`move_control`).
    pub control_binding_key: Arc<Mutex<String>>,
    /// Input de juego (ratón / cruceta) para scripts de control en play.
    pub play_input: Arc<Mutex<PlayScriptInput>>,
    /// Estado de interacción (objetivo bajo la mira y pulsación), writable por el motor.
    pub interaction: Arc<Mutex<InteractionState>>,
    /// Catálogos y selección de idioma compartidos por todas las funciones Rhai.
    pub i18n: Arc<Mutex<ScriptI18nState>>,
    pub profile: ScriptEngineProfile,
}

impl ScriptApiContext {
    pub fn new(profile: ScriptEngineProfile) -> Self {
        Self {
            cmds: Arc::new(Mutex::new(Vec::new())),
            player_ui_active_screen: Arc::new(Mutex::new(None)),
            graphics_texture_tier: Arc::new(Mutex::new("medium".to_string())),
            reflection_tier: Arc::new(Mutex::new("off".to_string())),
            control_binding_key: Arc::new(Mutex::new(String::new())),
            play_input: Arc::new(Mutex::new(PlayScriptInput::default())),
            interaction: Arc::new(Mutex::new(InteractionState::default())),
            i18n: Arc::new(Mutex::new(ScriptI18nState::default())),
            profile,
        }
    }

    pub fn drain_cmds(&self) -> Vec<ScriptCmd> {
        self.cmds
            .lock()
            .map(|mut g| g.drain(..).collect())
            .unwrap_or_default()
    }

    fn push(&self, cmd: ScriptCmd) {
        if let Ok(mut g) = self.cmds.lock() {
            g.push(cmd);
        }
    }
}

fn rhai_value_to_json(value: &Dynamic) -> Result<serde_json::Value, String> {
    if value.is_unit() {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(value) = value.as_int() {
        return Ok(serde_json::Value::from(value));
    }
    if let Ok(value) = value.as_float() {
        return serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| format!("non-finite float {value} cannot be converted to JSON"));
    }
    if let Ok(value) = value.as_bool() {
        return Ok(serde_json::Value::Bool(value));
    }
    if let Ok(value) = value.as_immutable_string_ref() {
        return Ok(serde_json::Value::String(value.to_string()));
    }
    if let Ok(values) = value.as_array_ref() {
        return values
            .iter()
            .map(rhai_value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array);
    }
    if let Ok(values) = value.as_map_ref() {
        return values
            .iter()
            .map(|(key, value)| rhai_value_to_json(value).map(|value| (key.to_string(), value)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(serde_json::Value::Object);
    }

    Err(format!(
        "unsupported Rhai value type: {}",
        value.type_name()
    ))
}

fn rhai_map_to_arguments(values: Map) -> Result<TextArguments, String> {
    values
        .into_iter()
        .map(|(key, value)| Ok((key.to_string(), rhai_value_to_json(&value)?)))
        .collect()
}

fn decimal_count_json(count: f64) -> Result<serde_json::Value, String> {
    serde_json::Number::from_f64(count)
        .map(serde_json::Value::Number)
        .ok_or_else(|| format!("count must be a finite decimal, got {count}"))
}

fn register_i18n_api(engine: &mut Engine, ctx: &ScriptApiContext) {
    let language_ctx = ctx.i18n.clone();
    engine.register_fn(
        "__engine_set_language",
        move |language: String| match LocaleId::parse(&language) {
            Ok(language) => language_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .set_language(language),
            Err(error) => log::warn!("[scripting] set_language({language:?}): {error}"),
        },
    );

    let locale_ctx = ctx.i18n.clone();
    engine.register_fn(
        "__engine_set_locale",
        move |locale: String| match LocaleId::parse(&locale) {
            Ok(locale) => locale_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .set_locale(locale),
            Err(error) => log::warn!("[scripting] set_locale({locale:?}): {error}"),
        },
    );

    let language_ctx = ctx.i18n.clone();
    engine.register_fn("__engine_get_language", move || {
        language_ctx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active_language
            .to_string()
    });

    let locale_ctx = ctx.i18n.clone();
    engine.register_fn("__engine_get_locale", move || {
        locale_ctx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .formatting_locale
            .to_string()
    });

    for name in ["__engine_tr", "tr"] {
        let i18n_ctx = ctx.i18n.clone();
        engine.register_fn(name, move |key: String| {
            i18n_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .translate(&key, None)
        });

        let i18n_ctx = ctx.i18n.clone();
        engine.register_fn(name, move |key: String, source: String| {
            i18n_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .translate(&key, Some(&source))
        });
    }

    let i18n_ctx = ctx.i18n.clone();
    engine.register_fn("tr_n", move |key: String, count: INT| {
        i18n_ctx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .translate_plural(&key, serde_json::Value::from(count), None)
    });

    let i18n_ctx = ctx.i18n.clone();
    engine.register_fn(
        "tr_n",
        move |key: String, count: f64| match decimal_count_json(count) {
            Ok(count) => i18n_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .translate_plural(&key, count, None),
            Err(error) => {
                log::warn!("[scripting] tr_n({key:?}, {count}): {error}");
                String::new()
            }
        },
    );

    let i18n_ctx = ctx.i18n.clone();
    engine.register_fn("tr_n", move |key: String, count: INT, source: String| {
        i18n_ctx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .translate_plural(&key, serde_json::Value::from(count), Some(&source))
    });

    let i18n_ctx = ctx.i18n.clone();
    engine.register_fn("tr_n", move |key: String, count: f64, source: String| {
        match decimal_count_json(count) {
            Ok(count) => i18n_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .translate_plural(&key, count, Some(&source)),
            Err(error) => {
                log::warn!("[scripting] tr_n({key:?}, {count}): {error}");
                String::new()
            }
        }
    });

    let i18n_ctx = ctx.i18n.clone();
    engine.register_fn(
        "tr_fmt",
        move |key: String, args: Map| match rhai_map_to_arguments(args) {
            Ok(arguments) => i18n_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .translate_with_arguments(&key, &arguments, None),
            Err(error) => {
                log::warn!("[scripting] tr_fmt({key:?}): {error}");
                String::new()
            }
        },
    );

    let i18n_ctx = ctx.i18n.clone();
    engine.register_fn("tr_fmt", move |key: String, args: Map, source: String| {
        match rhai_map_to_arguments(args) {
            Ok(arguments) => i18n_ctx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .translate_with_arguments(&key, &arguments, Some(&source)),
            Err(error) => {
                log::warn!("[scripting] tr_fmt({key:?}): {error}");
                String::new()
            }
        }
    });
}

pub fn register_native_api(engine: &mut Engine, ctx: &ScriptApiContext) {
    register_i18n_api(engine, ctx);

    let c = ctx.clone();
    engine.register_fn("__engine_log", move |msg: String| {
        c.push(ScriptCmd::Log { message: msg });
    });

    let play_input_ctx = ctx.play_input.clone();
    engine.register_fn("__engine_is_play_mode", move || {
        play_input_ctx.lock().map(|g| g.is_play).unwrap_or(false)
    });

    register_id_num2!(engine, ctx, "__engine_move_to", |id, x, y| {
        ScriptCmd::SetPosition {
            id: id as u32,
            x: x as f32,
            y: y as f32,
        }
    });

    register_id_num2!(engine, ctx, "__engine_translate", |id, dx, dy| {
        ScriptCmd::Translate {
            id: id as u32,
            dx: dx as f32,
            dy: dy as f32,
        }
    });

    register_id_num2!(engine, ctx, "__engine_set_scale", |id, sx, sy| {
        ScriptCmd::SetScale {
            id: id as u32,
            sx: sx as f32,
            sy: sy as f32,
        }
    });

    let c = ctx.clone();
    engine.register_fn("__engine_play_animation", move |id: i64, name: String| {
        c.push(ScriptCmd::PlayAnimation {
            id: id as u32,
            name,
        });
    });

    let c = ctx.clone();
    engine.register_fn("__engine_play_next_animation", move |id: i64| {
        c.push(ScriptCmd::PlayNextAnimation { id: id as u32 });
    });

    let c = ctx.clone();
    engine.register_fn(
        "__engine_set_default_animation",
        move |id: i64, name: String| {
            c.push(ScriptCmd::SetDefaultAnimation {
                id: id as u32,
                name,
            });
        },
    );

    let c = ctx.clone();
    engine.register_fn("__engine_stop_animation", move |id: i64| {
        c.push(ScriptCmd::StopAnimation { id: id as u32 });
    });

    let c = ctx.clone();
    engine.register_fn(
        "__engine_set_physics",
        move |id: i64, enabled: bool, body_type: String| {
            c.push(ScriptCmd::SetPhysics {
                id: id as u32,
                enabled,
                body_type,
            });
        },
    );

    register_id_num3!(
        engine,
        ctx,
        "__engine_move_entity",
        |id, speed, dir_x, dir_y| ScriptCmd::MoveEntity {
            id: id as u32,
            speed: speed as f32,
            dir_x: dir_x as f32,
            dir_y: dir_y as f32,
        }
    );

    register_id_num3!(
        engine,
        ctx,
        "__engine_move_entity_facing",
        |id, speed, amount_x, dir_y| ScriptCmd::MoveEntityFacing {
            id: id as u32,
            speed: speed as f32,
            amount_x: amount_x as f32,
            dir_y: dir_y as f32,
        }
    );

    let c = ctx.clone();
    engine.register_fn("__engine_set_vsync", move |enabled: bool| {
        c.push(ScriptCmd::SetVsync { enabled });
    });

    if ctx.profile == ScriptEngineProfile::Engine2d {
        register_id_num3!(
            engine,
            ctx,
            "__engine_apply_kinematic_gravity",
            |id, speed_x, jump_speed_y, gravity| ScriptCmd::ApplyKinematicGravity {
                id: id as u32,
                speed_x: speed_x as f32,
                jump_speed_y: jump_speed_y as f32,
                gravity: gravity as f32,
            }
        );

        register_id_num3!(
            engine,
            ctx,
            "__engine_apply_kinematic_impulse",
            |id, dir_x, dir_y, impulse| ScriptCmd::ApplyKinematicImpulse {
                id: id as u32,
                dir_x: dir_x as f32,
                dir_y: dir_y as f32,
                impulse: impulse as f32,
            }
        );

        register_id_num3!(
            engine,
            ctx,
            "__engine_move_entity_slide",
            |id, dx, dy, speed| ScriptCmd::SlideEntity {
                id: id as u32,
                dx: dx as f32,
                dy: dy as f32,
                speed: speed as f32,
            }
        );

        let c = ctx.clone();
        engine.register_fn("__engine_move_control", move |id: i64, speed: f64| {
            let key = c
                .control_binding_key
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            let (dir_x, dir_y) = control_key_move_dir(&key);
            if dir_x.abs() + dir_y.abs() > f32::EPSILON {
                c.push(ScriptCmd::MoveEntity {
                    id: id as u32,
                    speed: speed as f32,
                    dir_x,
                    dir_y,
                });
            }
        });
        let c = ctx.clone();
        engine.register_fn("__engine_move_control", move |id: i64, speed: INT| {
            let key = c
                .control_binding_key
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            let (dir_x, dir_y) = control_key_move_dir(&key);
            if dir_x.abs() + dir_y.abs() > f32::EPSILON {
                c.push(ScriptCmd::MoveEntity {
                    id: id as u32,
                    speed: speed as f32,
                    dir_x,
                    dir_y,
                });
            }
        });

        let play_input_ctx = ctx.play_input.clone();
        engine.register_fn("__engine_mouse_world_x", move || {
            play_input_ctx
                .lock()
                .ok()
                .and_then(|g| g.mouse_world_2d)
                .map(|(x, _)| x as f64)
                .unwrap_or(0.0)
        });
        let play_input_ctx = ctx.play_input.clone();
        engine.register_fn("__engine_mouse_world_y", move || {
            play_input_ctx
                .lock()
                .ok()
                .and_then(|g| g.mouse_world_2d)
                .map(|(_, y)| y as f64)
                .unwrap_or(0.0)
        });

        let c = ctx.clone();
        engine.register_fn("__engine_toggle_demo_fire", move || {
            c.push(ScriptCmd::ToggleDemoFire);
        });

        let c = ctx.clone();
        engine.register_fn(
            "__engine_fire_projectile",
            move |template_id: i64, from_id: i64, dir_x: f64, dir_y: f64, dir_z: f64| {
                c.push(ScriptCmd::FireProjectile {
                    template_id: template_id as u32,
                    from_id: from_id as u32,
                    dir_x: dir_x as f32,
                    dir_y: dir_y as f32,
                    dir_z: dir_z as f32,
                });
            },
        );
        let c = ctx.clone();
        engine.register_fn(
            "__engine_fire_projectile",
            move |template_id: INT, from_id: INT, dir_x: INT, dir_y: INT, dir_z: INT| {
                c.push(ScriptCmd::FireProjectile {
                    template_id: template_id as u32,
                    from_id: from_id as u32,
                    dir_x: dir_x as f32,
                    dir_y: dir_y as f32,
                    dir_z: dir_z as f32,
                });
            },
        );
    }

    if ctx.profile == ScriptEngineProfile::Engine3d {
        let c = ctx.clone();
        engine.register_fn("__engine_fp_press_key", move |key: String| {
            c.push(ScriptCmd::PlayControllerPressKey { key });
        });

        let c = ctx.clone();
        engine.register_fn("__engine_fp_jump", move || {
            c.push(ScriptCmd::PlayControllerJump);
        });

        register_num_fn1!(engine, ctx, "__engine_fp_set_walk_speed", |speed| {
            ScriptCmd::PlayControllerSetWalkSpeed(speed as f32)
        });

        register_num_fn1!(engine, ctx, "__engine_fp_set_sprint_multiplier", |mult| {
            ScriptCmd::PlayControllerSetSprintMultiplier(mult as f32)
        });

        register_num_fn1!(engine, ctx, "__engine_fp_set_jump_speed", |speed| {
            ScriptCmd::PlayControllerSetJumpSpeed(speed as f32)
        });

        let c = ctx.clone();
        engine.register_fn("__engine_set_taa", move |enabled: bool| {
            c.push(ScriptCmd::SetTaa { enabled });
        });

        let c = ctx.clone();
        engine.register_fn("__engine_set_active_player_ui", move |screen_id: String| {
            c.push(ScriptCmd::SetActivePlayerUiScreen { screen_id });
        });

        let c = ctx.clone();
        engine.register_fn(
            "__engine_set_active_player_ui_by_name",
            move |name: String| {
                c.push(ScriptCmd::SetActivePlayerUiScreenByName { name });
            },
        );

        let c = ctx.clone();
        engine.register_fn("__engine_clear_active_player_ui", move || {
            c.push(ScriptCmd::ClearActivePlayerUiScreen);
        });

        let screen_ctx = ctx.player_ui_active_screen.clone();
        engine.register_fn("__engine_get_active_player_ui", move || {
            screen_ctx
                .lock()
                .ok()
                .and_then(|g| g.clone())
                .unwrap_or_default()
        });

        let c = ctx.clone();
        engine.register_fn("__engine_set_graphics_texture_tier", move |tier: String| {
            c.push(ScriptCmd::SetGraphicsTextureTier { tier });
        });

        let tier_ctx = ctx.graphics_texture_tier.clone();
        engine.register_fn("__engine_get_graphics_texture_tier", move || {
            tier_ctx
                .lock()
                .ok()
                .map(|g| g.clone())
                .unwrap_or_else(|| "medium".to_string())
        });

        let c = ctx.clone();
        engine.register_fn("__engine_set_reflection_tier", move |tier: String| {
            c.push(ScriptCmd::SetReflectionTier { tier });
        });

        let c = ctx.clone();
        engine.register_fn("__engine_set_rt_tier", move |tier: String| {
            c.push(ScriptCmd::SetRtTier { tier });
        });

        let c = ctx.clone();
        engine.register_fn("__engine_set_reflection_mode", move |mode: String| {
            c.push(ScriptCmd::SetReflectionMode { mode });
        });

        let refl_ctx = ctx.reflection_tier.clone();
        engine.register_fn("__engine_get_reflection_tier", move || {
            refl_ctx
                .lock()
                .ok()
                .map(|g| g.clone())
                .unwrap_or_else(|| "off".to_string())
        });

        let play_input_ctx = ctx.play_input.clone();
        engine.register_fn("__engine_play_aim_dir_x", move || {
            play_input_ctx
                .lock()
                .ok()
                .and_then(|g| g.play_aim_dir_3d)
                .map(|(x, _, _)| x as f64)
                .unwrap_or(0.0)
        });
        let play_input_ctx = ctx.play_input.clone();
        engine.register_fn("__engine_play_aim_dir_y", move || {
            play_input_ctx
                .lock()
                .ok()
                .and_then(|g| g.play_aim_dir_3d)
                .map(|(_, y, _)| y as f64)
                .unwrap_or(0.0)
        });
        let play_input_ctx = ctx.play_input.clone();
        engine.register_fn("__engine_play_aim_dir_z", move || {
            play_input_ctx
                .lock()
                .ok()
                .and_then(|g| g.play_aim_dir_3d)
                .map(|(_, _, z)| z as f64)
                .unwrap_or(-1.0)
        });

        // ── Sistema de interacción (tooltip sobre entidades) ─────────────────────
        let interaction_ctx = ctx.interaction.clone();
        engine.register_fn("__engine_interact_target", move || {
            interaction_ctx
                .lock()
                .ok()
                .and_then(|g| g.target)
                .map(|id| id as f64)
                .unwrap_or(-1.0)
        });
        let interaction_ctx = ctx.interaction.clone();
        engine.register_fn("__engine_interact_pressed", move || {
            interaction_ctx.lock().map(|g| g.pressed).unwrap_or(false)
        });
        let interaction_ctx = ctx.interaction.clone();
        engine.register_fn("__engine_interact_distance", move || {
            interaction_ctx
                .lock()
                .map(|g| g.distance as f64)
                .unwrap_or(0.0)
        });

        let c = ctx.clone();
        engine.register_fn(
            "__engine_set_interactable",
            move |id: i64, enabled: bool| {
                c.push(ScriptCmd::SetInteractable {
                    id: id as u32,
                    enabled,
                });
            },
        );
        let c = ctx.clone();
        engine.register_fn("__engine_despawn_entity", move |id: i64| {
            c.push(ScriptCmd::DespawnEntity { id: id as u32 });
        });

        let c = ctx.clone();
        engine.register_fn(
            "__engine_fire_projectile",
            move |template_id: i64, from_id: i64, dir_x: f64, dir_y: f64, dir_z: f64| {
                c.push(ScriptCmd::FireProjectile {
                    template_id: template_id as u32,
                    from_id: from_id as u32,
                    dir_x: dir_x as f32,
                    dir_y: dir_y as f32,
                    dir_z: dir_z as f32,
                });
            },
        );
        let c = ctx.clone();
        engine.register_fn(
            "__engine_fire_projectile",
            move |template_id: INT, from_id: INT, dir_x: INT, dir_y: INT, dir_z: INT| {
                c.push(ScriptCmd::FireProjectile {
                    template_id: template_id as u32,
                    from_id: from_id as u32,
                    dir_x: dir_x as f32,
                    dir_y: dir_y as f32,
                    dir_z: dir_z as f32,
                });
            },
        );
    }
}

pub fn api_preamble(profile: ScriptEngineProfile) -> &'static str {
    match profile {
        ScriptEngineProfile::Engine2d => API_PREAMBLE_2D,
        ScriptEngineProfile::Engine3d => API_PREAMBLE_3D,
    }
}

const API_PREAMBLE_2D: &str = r#"
let engine = #{
    log: |msg| { __engine_log(msg); },
    set_language: |language| { __engine_set_language(language); },
    set_locale: |locale| { __engine_set_locale(locale); },
    get_language: || { __engine_get_language() },
    get_locale: || { __engine_get_locale() },
    move_to: |id, x, y| { __engine_move_to(id, x, y); },
    translate: |id, dx, dy| { __engine_translate(id, dx, dy); },
    set_scale: |id, sx, sy| { __engine_set_scale(id, sx, sy); },
    play_animation: |id, name| { __engine_play_animation(id, name); },
    set_default_animation: |id, name| { __engine_set_default_animation(id, name); },
    stop_animation: |id| { __engine_stop_animation(id); },
    set_physics: |id, enabled, body_type| { __engine_set_physics(id, enabled, body_type); },
    move_entity: |id, speed, dir_x, dir_y| { __engine_move_entity(id, speed, dir_x, dir_y); },
    move_entity_facing: |id, speed, amount_x, dir_y| { __engine_move_entity_facing(id, speed, amount_x, dir_y); },
    apply_kinematic_gravity: |id, speed_x, jump_speed_y, gravity| { __engine_apply_kinematic_gravity(id, speed_x, jump_speed_y, gravity); },
    apply_kinematic_impulse: |id, dir_x, dir_y, impulse| { __engine_apply_kinematic_impulse(id, dir_x, dir_y, impulse); },
    move_entity_slide: |id, dx, dy, speed| { __engine_move_entity_slide(id, dx, dy, speed); },
    move_control: |id, speed| { __engine_move_control(id, speed); },
    set_vsync: |enabled| { __engine_set_vsync(enabled); },
    is_play_mode: || { __engine_is_play_mode() },
    mouse_world_x: || { __engine_mouse_world_x() },
    mouse_world_y: || { __engine_mouse_world_y() },
    fire_projectile: |template_id, from_id, dir_x, dir_y, dir_z| { __engine_fire_projectile(template_id, from_id, dir_x, dir_y, dir_z); },
};
"#;

const API_PREAMBLE_3D: &str = r#"
let engine = #{
    log: |msg| { __engine_log(msg); },
    set_language: |language| { __engine_set_language(language); },
    set_locale: |locale| { __engine_set_locale(locale); },
    get_language: || { __engine_get_language() },
    get_locale: || { __engine_get_locale() },
    move_to: |id, x, y| { __engine_move_to(id, x, y); },
    translate: |id, dx, dy| { __engine_translate(id, dx, dy); },
    set_scale: |id, sx, sy| { __engine_set_scale(id, sx, sy); },
    play_animation: |id, name| { __engine_play_animation(id, name); },
    play_next_animation: |id| { __engine_play_next_animation(id); },
    set_default_animation: |id, name| { __engine_set_default_animation(id, name); },
    stop_animation: |id| { __engine_stop_animation(id); },
    set_physics: |id, enabled, body_type| { __engine_set_physics(id, enabled, body_type); },
    move_entity: |id, speed, dir_x, dir_y| { __engine_move_entity(id, speed, dir_x, dir_y); },
    move_entity_facing: |id, speed, amount_x, dir_y| { __engine_move_entity_facing(id, speed, amount_x, dir_y); },
    set_vsync: |enabled| { __engine_set_vsync(enabled); },
    is_play_mode: || { __engine_is_play_mode() },
    set_taa: |enabled| { __engine_set_taa(enabled); },
    fp_press_key: |key| { __engine_fp_press_key(key); },
    fp_jump: || { __engine_fp_jump(); },
    fp_set_walk_speed: |speed| { __engine_fp_set_walk_speed(speed); },
    fp_set_sprint_multiplier: |mult| { __engine_fp_set_sprint_multiplier(mult); },
    fp_set_jump_speed: |speed| { __engine_fp_set_jump_speed(speed); },
    set_active_player_ui: |screen_id| { __engine_set_active_player_ui(screen_id); },
    set_active_player_ui_by_name: |name| { __engine_set_active_player_ui_by_name(name); },
    get_active_player_ui: || { __engine_get_active_player_ui() },
    clear_active_player_ui: || { __engine_clear_active_player_ui(); },
    set_graphics_texture_tier: |tier| { __engine_set_graphics_texture_tier(tier); },
    get_graphics_texture_tier: || { __engine_get_graphics_texture_tier() },
    set_reflection_tier: |tier| { __engine_set_reflection_tier(tier); },
    get_reflection_tier: || { __engine_get_reflection_tier() },
    set_rt_tier: |tier| { __engine_set_rt_tier(tier); },
    set_reflection_mode: |mode| { __engine_set_reflection_mode(mode); },
    play_aim_dir_x: || { __engine_play_aim_dir_x() },
    play_aim_dir_y: || { __engine_play_aim_dir_y() },
    play_aim_dir_z: || { __engine_play_aim_dir_z() },
    interact_target: || { __engine_interact_target() },
    interact_pressed: || { __engine_interact_pressed() },
    interact_distance: || { __engine_interact_distance() },
    set_interactable: |id, enabled| { __engine_set_interactable(id, enabled); },
    despawn_entity: |id| { __engine_despawn_entity(id); },
    toggle_demo_fire: || { __engine_toggle_demo_fire(); },
    fire_projectile: |template_id, from_id, dir_x, dir_y, dir_z| { __engine_fire_projectile(template_id, from_id, dir_x, dir_y, dir_z); },
};
"#;

pub fn wrap_user_source(profile: ScriptEngineProfile, user_source: &str) -> String {
    format!("{}\n{}", api_preamble(profile), user_source)
}

pub fn scene_script_preamble(profile: ScriptEngineProfile) -> &'static str {
    match profile {
        ScriptEngineProfile::Engine3d => SCENE_SCRIPT_PREAMBLE_3D,
        ScriptEngineProfile::Engine2d => SCENE_SCRIPT_PREAMBLE_2D,
    }
}

const SCENE_SCRIPT_PREAMBLE_2D: &str = r#"
let engine = #{
    log: |msg| { __engine_log(msg); },
    set_language: |language| { __engine_set_language(language); },
    set_locale: |locale| { __engine_set_locale(locale); },
    get_language: || { __engine_get_language() },
    get_locale: || { __engine_get_locale() },
};
"#;

const SCENE_SCRIPT_PREAMBLE_3D: &str = r#"
let engine = #{
    log: |msg| { __engine_log(msg); },
    set_language: |language| { __engine_set_language(language); },
    set_locale: |locale| { __engine_set_locale(locale); },
    get_language: || { __engine_get_language() },
    get_locale: || { __engine_get_locale() },
    set_graphics_texture_tier: |tier| { __engine_set_graphics_texture_tier(tier); },
    get_graphics_texture_tier: || { __engine_get_graphics_texture_tier() },
    set_reflection_tier: |tier| { __engine_set_reflection_tier(tier); },
    get_reflection_tier: || { __engine_get_reflection_tier() },
    set_rt_tier: |tier| { __engine_set_rt_tier(tier); },
    set_reflection_mode: |mode| { __engine_set_reflection_mode(mode); },
};
"#;

pub fn wrap_scene_source(profile: ScriptEngineProfile, user_source: &str) -> String {
    format!("{}\n{}", scene_script_preamble(profile), user_source)
}
