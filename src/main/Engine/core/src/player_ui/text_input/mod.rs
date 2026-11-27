//! Macro: impl `State` para edición de texto HUD (parametrizado por motor).

pub mod hit_test;
pub mod types;

pub use types::PlayerUiTextDrag;

mod methods;

/// Texto editable directamente en el HUD. Los valores localizados se editan en catálogos.
#[doc(hidden)]
#[must_use]
pub fn editable_text(value: &crate::i18n::TextValue) -> Option<&str> {
    value.as_literal()
}

/// Aplica una edición solo a valores literales; los localizados permanecen intactos.
#[doc(hidden)]
#[must_use]
pub fn mutate_literal_text(
    value: &mut crate::i18n::TextValue,
    edit: impl FnOnce(&mut String),
) -> bool {
    let Some(literal) = value.as_literal_mut() else {
        return false;
    };
    edit(literal);
    true
}

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

#[cfg(test)]
mod tests {
    use super::{editable_text, mutate_literal_text};
    use crate::i18n::{LocalizedText, TextId, TextValue};

    #[test]
    fn editable_text_is_only_available_for_literals() {
        let literal = TextValue::literal("Texto");
        let localized = TextValue::Localized(LocalizedText::new(
            TextId::new("player_ui", "title"),
            "Título",
        ));

        assert_eq!(editable_text(&literal), Some("Texto"));
        assert_eq!(editable_text(&localized), None);
    }

    #[test]
    fn literal_text_can_be_edited_but_localized_text_is_a_noop() {
        let mut literal = TextValue::literal("Texto");
        assert_eq!(literal.source_text(), "Texto");
        assert_eq!(literal.as_literal(), Some("Texto"));
        assert!(!literal.is_localized());
        assert!(mutate_literal_text(&mut literal, |text| {
            text.push_str(" editado");
        }));
        assert_eq!(literal.source_text(), "Texto editado");

        let mut localized = TextValue::Localized(LocalizedText::new(
            TextId::new("player_ui", "title"),
            "Título",
        ));
        let before = localized.clone();
        assert_eq!(localized.source_text(), "Título");
        assert_eq!(localized.as_literal(), None);
        assert!(localized.is_localized());
        assert!(!mutate_literal_text(&mut localized, |text| {
            text.push_str(" editado");
        }));
        assert_eq!(localized, before);
    }
}
