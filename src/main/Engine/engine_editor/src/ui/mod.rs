//! UI del editor egui.

mod accordion;
mod bottom_bar;
mod frame;
mod number_draft;
pub(crate) mod sidebar_left;
pub(crate) mod sidebar_right;
mod top_bar;
mod widgets;

use crate::app::EditorApp;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    frame::show(ui, app);
}
