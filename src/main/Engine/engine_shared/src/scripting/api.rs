use std::sync::{Arc, Mutex};

use rhai::Engine;

use super::script_cmd::ScriptCmd;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptEngineProfile {
    Engine2d,
    Engine3d,
}

#[derive(Clone)]
pub struct ScriptApiContext {
    pub cmds: Arc<Mutex<Vec<ScriptCmd>>>,
    pub player_ui_active_screen: Arc<Mutex<Option<String>>>,
    pub profile: ScriptEngineProfile,
}

impl ScriptApiContext {
    pub fn new(profile: ScriptEngineProfile) -> Self {
        Self {
            cmds: Arc::new(Mutex::new(Vec::new())),
            player_ui_active_screen: Arc::new(Mutex::new(None)),
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

    let c = ctx.clone();
    engine.register_fn(
        "__engine_move_to",
        move |id: i64, x: f64, y: f64| {
            c.push(ScriptCmd::SetPosition {
                id: id as u32,
                x: x as f32,
                y: y as f32,
            });
        },
    );

    let c = ctx.clone();
    engine.register_fn(
        "__engine_translate",
        move |id: i64, dx: f64, dy: f64| {
            c.push(ScriptCmd::Translate {
                id: id as u32,
                dx: dx as f32,
                dy: dy as f32,
            });
        },
    );

    let c = ctx.clone();
    engine.register_fn(
        "__engine_set_scale",
        move |id: i64, sx: f64, sy: f64| {
            c.push(ScriptCmd::SetScale {
                id: id as u32,
                sx: sx as f32,
                sy: sy as f32,
            });
        },
    );

    let c = ctx.clone();
    engine.register_fn(
        "__engine_play_animation",
        move |id: i64, name: String| {
            c.push(ScriptCmd::PlayAnimation {
                id: id as u32,
                name,
            });
        },
    );

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

    let c = ctx.clone();
    engine.register_fn(
        "__engine_move_entity",
        move |id: i64, speed: f64, dir_x: f64, dir_y: f64| {
            c.push(ScriptCmd::MoveEntity {
                id: id as u32,
                speed: speed as f32,
                dir_x: dir_x as f32,
                dir_y: dir_y as f32,
            });
        },
    );

    let c = ctx.clone();
    engine.register_fn(
        "__engine_move_entity_facing",
        move |id: i64, speed: f64, amount_x: f64, dir_y: f64| {
            c.push(ScriptCmd::MoveEntityFacing {
                id: id as u32,
                speed: speed as f32,
                amount_x: amount_x as f32,
                dir_y: dir_y as f32,
            });
        },
    );

    let c = ctx.clone();
    engine.register_fn("__engine_set_vsync", move |enabled: bool| {
        c.push(ScriptCmd::SetVsync { enabled });
    });

    if ctx.profile == ScriptEngineProfile::Engine2d {
        let c = ctx.clone();
        engine.register_fn(
            "__engine_apply_kinematic_gravity",
            move |id: i64, speed_x: f64, jump_speed_y: f64, gravity: f64| {
                c.push(ScriptCmd::ApplyKinematicGravity {
                    id: id as u32,
                    speed_x: speed_x as f32,
                    jump_speed_y: jump_speed_y as f32,
                    gravity: gravity as f32,
                });
            },
        );

        let c = ctx.clone();
        engine.register_fn(
            "__engine_apply_kinematic_impulse",
            move |id: i64, dir_x: f64, dir_y: f64, impulse: f64| {
                c.push(ScriptCmd::ApplyKinematicImpulse {
                    id: id as u32,
                    dir_x: dir_x as f32,
                    dir_y: dir_y as f32,
                    impulse: impulse as f32,
                });
            },
        );

        let c = ctx.clone();
        engine.register_fn(
            "__engine_move_entity_slide",
            move |id: i64, dx: f64, dy: f64, speed: f64| {
                c.push(ScriptCmd::SlideEntity {
                    id: id as u32,
                    dx: dx as f32,
                    dy: dy as f32,
                    speed: speed as f32,
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

        let c = ctx.clone();
        engine.register_fn("__engine_fp_set_walk_speed", move |speed: f64| {
            c.push(ScriptCmd::PlayControllerSetWalkSpeed(speed as f32));
        });

        let c = ctx.clone();
        engine.register_fn("__engine_fp_set_sprint_multiplier", move |mult: f64| {
            c.push(ScriptCmd::PlayControllerSetSprintMultiplier(mult as f32));
        });

        let c = ctx.clone();
        engine.register_fn("__engine_fp_set_jump_speed", move |speed: f64| {
            c.push(ScriptCmd::PlayControllerSetJumpSpeed(speed as f32));
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
        engine.register_fn("__engine_set_active_player_ui_by_name", move |name: String| {
            c.push(ScriptCmd::SetActivePlayerUiScreenByName { name });
        });

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
    log: fn(msg) { __engine_log(msg); },
    move_to: fn(id, x, y) { __engine_move_to(id, x, y); },
    translate: fn(id, dx, dy) { __engine_translate(id, dx, dy); },
    set_scale: fn(id, sx, sy) { __engine_set_scale(id, sx, sy); },
    play_animation: fn(id, name) { __engine_play_animation(id, name); },
    set_default_animation: fn(id, name) { __engine_set_default_animation(id, name); },
    stop_animation: fn(id) { __engine_stop_animation(id); },
    set_physics: fn(id, enabled, body_type) { __engine_set_physics(id, enabled, body_type); },
    move_entity: fn(id, speed, dir_x, dir_y) { __engine_move_entity(id, speed, dir_x, dir_y); },
    move_entity_facing: fn(id, speed, amount_x, dir_y) { __engine_move_entity_facing(id, speed, amount_x, dir_y); },
    apply_kinematic_gravity: fn(id, speed_x, jump_speed_y, gravity) { __engine_apply_kinematic_gravity(id, speed_x, jump_speed_y, gravity); },
    apply_kinematic_impulse: fn(id, dir_x, dir_y, impulse) { __engine_apply_kinematic_impulse(id, dir_x, dir_y, impulse); },
    move_entity_slide: fn(id, dx, dy, speed) { __engine_move_entity_slide(id, dx, dy, speed); },
    set_vsync: fn(enabled) { __engine_set_vsync(enabled); },
};
"#;

const API_PREAMBLE_3D: &str = r#"
let engine = #{
    log: fn(msg) { __engine_log(msg); },
    move_to: fn(id, x, y) { __engine_move_to(id, x, y); },
    translate: fn(id, dx, dy) { __engine_translate(id, dx, dy); },
    set_scale: fn(id, sx, sy) { __engine_set_scale(id, sx, sy); },
    play_animation: fn(id, name) { __engine_play_animation(id, name); },
    set_default_animation: fn(id, name) { __engine_set_default_animation(id, name); },
    stop_animation: fn(id) { __engine_stop_animation(id); },
    set_physics: fn(id, enabled, body_type) { __engine_set_physics(id, enabled, body_type); },
    move_entity: fn(id, speed, dir_x, dir_y) { __engine_move_entity(id, speed, dir_x, dir_y); },
    move_entity_facing: fn(id, speed, amount_x, dir_y) { __engine_move_entity_facing(id, speed, amount_x, dir_y); },
    set_vsync: fn(enabled) { __engine_set_vsync(enabled); },
    set_taa: fn(enabled) { __engine_set_taa(enabled); },
    fp_press_key: fn(key) { __engine_fp_press_key(key); },
    fp_jump: fn() { __engine_fp_jump(); },
    fp_set_walk_speed: fn(speed) { __engine_fp_set_walk_speed(speed); },
    fp_set_sprint_multiplier: fn(mult) { __engine_fp_set_sprint_multiplier(mult); },
    fp_set_jump_speed: fn(speed) { __engine_fp_set_jump_speed(speed); },
    play_character_press_key: fn(key) { __engine_fp_press_key(key); },
    play_character_jump: fn() { __engine_fp_jump(); },
    play_character_set_walk_speed: fn(speed) { __engine_fp_set_walk_speed(speed); },
    play_character_set_sprint_multiplier: fn(mult) { __engine_fp_set_sprint_multiplier(mult); },
    play_character_set_jump_speed: fn(speed) { __engine_fp_set_jump_speed(speed); },
    set_active_player_ui: fn(screen_id) { __engine_set_active_player_ui(screen_id); },
    set_active_player_ui_by_name: fn(name) { __engine_set_active_player_ui_by_name(name); },
    get_active_player_ui: fn() { __engine_get_active_player_ui() },
    clear_active_player_ui: fn() { __engine_clear_active_player_ui(); },
};
"#;

pub fn wrap_user_source(profile: ScriptEngineProfile, user_source: &str) -> String {
    format!("{}\n{}", api_preamble(profile), user_source)
}

pub fn scene_script_preamble() -> &'static str {
    r#"
let engine = #{
    log: fn(msg) { __engine_log(msg); },
};
"#
}

pub fn wrap_scene_source(user_source: &str) -> String {
    format!("{}\n{}", scene_script_preamble(), user_source)
}
