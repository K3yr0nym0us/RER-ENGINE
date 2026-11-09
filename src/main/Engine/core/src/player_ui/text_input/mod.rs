//! Macro: impl `State` para edición de texto HUD (parametrizado por motor).

pub mod hit_test;
pub mod types;

pub use types::PlayerUiTextDrag;

mod methods;

/// Genera el bloque `impl State` de edición HUD.
///
/// `$pu` = módulo `player_ui` del motor; `$ipc` = módulo IPC; `$state` = ruta al
/// `State` del motor llamante. Así el macro no asume que `crate::engine` exista,
/// que solo era cierto cuando el motor 3D era un crate raíz.
#[macro_export]
macro_rules! impl_player_ui_text_input {
    ($pu:path, $ipc:path, $state:path) => {
        use std::time::Instant;

        use rer_engine_core::platform::{query_ctrl_held_os, query_shift_held_os};
        use rer_engine_core::player_ui::config::{
            self, BOX_STACK_GAP, HasUiHudRect, PlayerUiButton, PlayerUiImage, PlayerUiResizeHandle,
            PlayerUiTextBox, UiHudRect, apply_resize_free, apply_resize_locked_aspect,
        };
        use rer_engine_core::player_ui::grid::{
            player_ui_grid_steps, snap_ndc_point_to_grid, snap_ui_hud_rect_to_grid,
        };
        use rer_engine_core::player_ui::text_input::types::{
            CARET_BLINK_ON_MS, CARET_BLINK_PERIOD_MS, DEFAULT_BOX_H, DEFAULT_BOX_W, DOUBLE_CLICK,
            DRAG_THRESHOLD_PX, MAX_TEXT_CHARS, MIN_BOX_H, MIN_BOX_W, PlayerUiHitTarget,
            PlayerUiTextDrag as PuTextDrag,
        };
        use $ipc::{
            EngineEvent, PlayerUiButtonListItem, PlayerUiImageListItem, PlayerUiTextBoxListItem,
            SavePlayerUiTextBoxSnapshot, send_event,
        };
        use $pu::{button, image, object};
        use $state;
        impl State {
            $crate::__rer_player_ui_text_core!();
            $crate::__rer_player_ui_text_edit!();
            $crate::__rer_player_ui_text_drag!();
            $crate::__rer_player_ui_text_emit!();
        }
    };
}
