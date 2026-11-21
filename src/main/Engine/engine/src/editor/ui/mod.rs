//! UI del editor egui.

mod accordion;
mod bottom_bar;
mod clear_scene_modal;
/// Cuerpo de la ventana nativa de configuración de controles.
pub(crate) mod control_bindings_modal;
/// Cuerpo de la ventana nativa de creación de escena.
pub(crate) mod create_scene_modal;
/// Cuerpo de la ventana nativa de creación de pantalla UI.
pub(crate) mod create_ui_screen_modal;
pub(crate) mod delete_interaction_tooltip_modal;
/// Cuerpo de la ventana nativa de confirmación de borrado de escena.
pub(crate) mod delete_scene_modal;
/// Cuerpo de la ventana nativa de confirmación de borrado de pantalla UI.
pub(crate) mod delete_ui_screen_modal;
/// Cuerpo de la ventana nativa de propiedades de entidad.
pub(crate) mod entity_properties_modal;
/// Cuerpo de la ventana nativa de confirmación de salida del editor.
pub(crate) mod exit_confirm_modal;
mod frame;
mod globalize_model_modal;
/// Cuerpo de la ventana nativa del editor de HUD.
pub(crate) mod hud_editor_modal;
pub(crate) mod interaction_tooltip_modal;
mod load_from_scene_modal;
/// Overlay de métricas sobre el viewport del motor.
pub(crate) mod metrics_overlay;
mod number_draft;
/// Modal flotante del editor de emisores de partículas (F5), dentro del viewport.
mod particles_config_modal;
/// Cuerpo de la ventana nativa del catálogo de plugins.
pub(crate) mod plugins_modal;
/// Cuerpo de la ventana nativa del selector de blueprints.
pub(crate) mod quick_build_modal;
/// Cuerpo de la ventana nativa de confirmación para borrar un recurso.
pub(crate) mod remove_asset_modal;
pub(crate) mod remove_tooltip_script_modal;
/// Cuerpo de la ventana nativa de renombrado de menú UI.
pub(crate) mod rename_ui_screen_modal;
/// Cuerpo de la ventana nativa del script Rhai de escena.
pub(crate) mod scene_script_modal;
pub(crate) mod sidebar_left;
pub(crate) mod sidebar_right;
/// Cuerpo de la ventana nativa de configuración de sockets.
pub(crate) mod socket_config_modal;
mod switch_scene_modal;
pub(crate) mod tooltip_script_editor_modal;
mod top_bar;
/// Cuerpo de la ventana nativa de la guía de usuario.
pub(crate) mod user_guide_modal;
/// Cuerpo de la ventana nativa del editor visual de lógica (nodos).
pub(crate) mod visual_script_modal;
mod widgets;
/// Cuerpo de la ventana nativa del generador de mundo.
pub(crate) mod worldgen_modal;

use crate::editor::app::EditorApp;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    frame::show(ui, app);
    // Modal flotante (backdrop sobre toda la ventana): tras los paneles.
    let ctx = ui.ctx().clone();
    load_from_scene_modal::show(&ctx, app);
    globalize_model_modal::show(&ctx, app);
    switch_scene_modal::show(&ctx, app);
    clear_scene_modal::show(&ctx, app);
    particles_config_modal::show(&ctx, app);
}
