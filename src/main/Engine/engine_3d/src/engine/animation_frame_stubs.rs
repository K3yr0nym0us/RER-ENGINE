//! Animaciones por frames PNG (spritesheet): pipeline del motor 2D.
//! En 3D las entidades animadas usan clips del `.rerasset`; estas rutas son no-op.

use super::State;

impl State {
    pub(crate) fn preload_anim_frame_with_rect(
        &mut self,
        _path: &str,
        _src_rect: Option<(u32, u32, u32, u32)>,
    ) {
    }

    pub(crate) fn play_animation_frame(
        &mut self,
        _id: u32,
        _path: &str,
        _pivot_x: f32,
        _pivot_y: f32,
        _logical_w: u32,
        _logical_h: u32,
        _src_rect: Option<(u32, u32, u32, u32)>,
        _flip_horizontal: bool,
    ) {
    }

    pub(crate) fn restore_animation_frame(&mut self, _id: u32) {}
}
