//! Overlay de play: PNG «Esc para salir» / «Esc to exit» según `snap_locale`.

use rer_engine_core::player_ui::{
    ScreenHudBottomLeftLayout, ndc_transform_bottom_left, pick_localized_screen_hud,
};

use super::State;

impl State {
    pub(crate) fn pack_fps_exit_hints(&mut self) {
        let gpu = self.vulkan.clone();
        self.fps_exit_hint_es = self
            .player_ui_text_atlas
            .pack_png_from_engine_assets(&gpu, "tooltip-btn-esc-salir.png");
        self.fps_exit_hint_en = self
            .player_ui_text_atlas
            .pack_png_from_engine_assets(&gpu, "tooltip-btn-esc-exit.png");
        if self.fps_exit_hint_es.is_none() && self.fps_exit_hint_en.is_none() {
            log::warn!("[hud] tooltips Esc (salir/exit) no encontrados en Engine/assets");
        }
    }

    pub(crate) fn update_fps_exit_hint_alpha(&mut self) {
        let target = if self.preview_playing {
            1.0_f32
        } else {
            0.0_f32
        };
        let k = if target > self.fps_exit_hint_alpha {
            4.2_f32
        } else {
            3.4_f32
        };
        let blend = 1.0 - (-k * self.delta_time.max(0.0)).exp();
        self.fps_exit_hint_alpha += (target - self.fps_exit_hint_alpha) * blend;
        if (self.fps_exit_hint_alpha - target).abs() < 0.001 {
            self.fps_exit_hint_alpha = target;
        }
    }

    pub(crate) fn build_fps_exit_hint_instance(
        &self,
    ) -> Option<crate::engine3d::mesh::InstanceData> {
        if self.fps_exit_hint_alpha <= 0.003 {
            return None;
        }
        let packed = pick_localized_screen_hud(
            &self.snap_locale,
            self.fps_exit_hint_en,
            self.fps_exit_hint_es,
        )?;
        let alpha = self.fps_exit_hint_alpha.clamp(0.0, 1.0);
        let eased = alpha * alpha * (3.0 - 2.0 * alpha);
        let model = ndc_transform_bottom_left(
            self.size.width as f32,
            self.size.height as f32,
            packed,
            ScreenHudBottomLeftLayout::default(),
            eased,
        )?;
        Some(crate::engine3d::screen_hud_image::build_screen_hud_instance(packed, model, eased))
    }
}
