use std::sync::{Arc, Mutex};

use rhai::{Engine, INT};

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

pub fn register_native_api(engine: &mut Engine, ctx: &ScriptApiContext) {
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
    move_to: |id, x, y| { __engine_move_to(id, x, y); },
    translate: |id, dx, dy| { __engine_translate(id, dx, dy); },
    set_scale: |id, sx, sy| { __engine_set_scale(id, sx, sy); },
    play_animation: |id, name| { __engine_play_animation(id, name); },
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
    play_aim_dir_x: || { __engine_play_aim_dir_x() },
    play_aim_dir_y: || { __engine_play_aim_dir_y() },
    play_aim_dir_z: || { __engine_play_aim_dir_z() },
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
};
"#;

const SCENE_SCRIPT_PREAMBLE_3D: &str = r#"
let engine = #{
    log: |msg| { __engine_log(msg); },
    set_graphics_texture_tier: |tier| { __engine_set_graphics_texture_tier(tier); },
    get_graphics_texture_tier: || { __engine_get_graphics_texture_tier() },
    set_reflection_tier: |tier| { __engine_set_reflection_tier(tier); },
    get_reflection_tier: || { __engine_get_reflection_tier() },
};
"#;

pub fn wrap_scene_source(profile: ScriptEngineProfile, user_source: &str) -> String {
    format!("{}\n{}", scene_script_preamble(profile), user_source)
}
