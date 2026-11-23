//! Undo/redo del HUD Player UI (snapshot por pantalla en edición).

use crate::engine2d::engine::UndoAction;

rer_engine_core::player_ui_hud_undo_impl!(
    crate::engine2d::engine::State,
    UndoAction,
    super::object::PlayerUiObjectDrawSession,
);
