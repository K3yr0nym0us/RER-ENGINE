//! UI del editor egui.

mod accordion;
mod bottom_bar;
mod frame;
mod model_import_modal;
mod number_draft;
pub(crate) mod sidebar_left;
pub(crate) mod sidebar_right;
mod top_bar;
mod widgets;

use crate::editor::app::EditorApp;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    frame::show(ui, app);
    // Modal flotante (backdrop sobre toda la ventana): tras los paneles.
    let ctx = ui.ctx().clone();
    model_import_modal::show(&ctx, app);
}
