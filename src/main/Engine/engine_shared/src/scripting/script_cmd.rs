/// Comandos que los scripts Rhai pueden solicitar al motor host.
#[derive(Debug, Clone)]
pub enum ScriptCmd {
    SetPosition {
        id: u32,
        x: f32,
        y: f32,
    },
    Translate {
        id: u32,
        dx: f32,
        dy: f32,
    },
    SetScale {
        id: u32,
        sx: f32,
        sy: f32,
    },
    PlayAnimation {
        id: u32,
        name: String,
    },
    SetDefaultAnimation {
        id: u32,
        name: String,
    },
    StopAnimation {
        id: u32,
    },
    SetPhysics {
        id: u32,
        enabled: bool,
        body_type: String,
    },
    MoveEntity {
        id: u32,
        speed: f32,
        dir_x: f32,
        dir_y: f32,
    },
    MoveEntityFacing {
        id: u32,
        speed: f32,
        amount_x: f32,
        dir_y: f32,
    },
    ApplyKinematicGravity {
        id: u32,
        speed_x: f32,
        jump_speed_y: f32,
        gravity: f32,
    },
    ApplyKinematicImpulse {
        id: u32,
        dir_x: f32,
        dir_y: f32,
        impulse: f32,
    },
    SlideEntity {
        id: u32,
        dx: f32,
        dy: f32,
        speed: f32,
    },
    Log {
        message: String,
    },
    PlayControllerPressKey {
        key: String,
    },
    PlayControllerJump,
    PlayControllerSetWalkSpeed(f32),
    PlayControllerSetSprintMultiplier(f32),
    PlayControllerSetJumpSpeed(f32),
    SetVsync {
        enabled: bool,
    },
    SetTaa {
        enabled: bool,
    },
    SetActivePlayerUiScreen {
        screen_id: String,
    },
    ClearActivePlayerUiScreen,
    SetActivePlayerUiScreenByName {
        name: String,
    },
    SetGraphicsTextureTier {
        tier: String,
    },
    SetReflectionTier {
        tier: String,
    },
}
