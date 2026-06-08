//! Wrapper del motor 2D: atlas compartido + instancia GPU local (`mesh::InstanceData`).

use glam::Mat4;

pub use rer_engine_shared::player_ui::screen_hud_atlas::{ScreenHudAtlas, ScreenHudPackedImage};

use crate::mesh;

/// Instancia para `screen_hud_pipeline`. `tex_layer_pad` = UV rect.
pub fn build_screen_hud_instance(
    packed: ScreenHudPackedImage,
    model: Mat4,
    alpha: f32,
) -> mesh::InstanceData {
    let mut inst = mesh::InstanceData::new(model, 0.0, [0.0; 4]);
    inst.uv_rect = packed.uv_rect;
    inst.flag_pad[1] = alpha;
    inst
}
