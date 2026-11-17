//! UI del editor egui.

mod accordion;
mod bottom_bar;
mod clear_scene_modal;
mod frame;
mod globalize_model_modal;
mod load_from_scene_modal;
mod model_import_modal;
mod number_draft;
/// Modal nativa del editor de emisores de partículas (F5).
mod particles_config_modal;
pub(crate) mod sidebar_left;
pub(crate) mod sidebar_right;
mod switch_scene_modal;
mod top_bar;
mod widgets;

use crate::editor::app::EditorApp;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    frame::show(ui, app);
    // Modal flotante (backdrop sobre toda la ventana): tras los paneles.
    let ctx = ui.ctx().clone();
    model_import_modal::show(&ctx, app);
    load_from_scene_modal::show(&ctx, app);
    globalize_model_modal::show(&ctx, app);
    switch_scene_modal::show(&ctx, app);
    clear_scene_modal::show(&ctx, app);
}

/// Cuerpo del editor de emisores: vive en una ventana nativa winit aparte.
pub(crate) fn show_particle_editor(ui: &mut egui::Ui, app: &mut EditorApp) {
    particles_config_modal::show(ui, app);
}
