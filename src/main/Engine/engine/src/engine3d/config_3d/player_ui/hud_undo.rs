//! Undo/redo del HUD Player UI (snapshot por pantalla en edición).

use crate::engine3d::engine::UndoAction;

rer_engine_core::player_ui_hud_undo_impl!(
    crate::engine3d::engine::State,
    UndoAction,
    super::object::PlayerUiObjectDrawSession,
    on_push = sync_editor_scenes_undo_dirty_to_renderer,
);
