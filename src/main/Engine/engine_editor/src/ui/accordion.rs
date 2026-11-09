//! Acordeón reutilizable de las sidebars.
//!
//! El encabezado y el cuerpo viven dentro del mismo marco (como el
//! `.accordion-item` de Bootstrap) y el despliegue se anima con el estado de
//! `CollapsingState`.

use egui::{Align2, FontId, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icons;

use crate::theme;

/// Ítem completo: marco exterior + header + cuerpo animado.
///
/// Devuelve la respuesta del header para que el sidebar actualice su estado
/// de "solo uno abierto".
pub fn item(
    ui: &mut egui::Ui,
    id: egui::Id,
    depth: u8,
    open: bool,
    add_header: impl FnOnce(&mut egui::Ui, f32) -> egui::Response,
    add_body: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let mut state = egui::containers::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        false,
    );
    state.set_open(open);
    let openness = state.openness(ui.ctx());

    let radius = if depth == 0 { 6 } else { 4 };
    let frame = egui::Frame::new()
        .fill(theme::ACCORDION_BG)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(radius))
        .inner_margin(egui::Margin::ZERO);

    frame
        .show(ui, |ui| {
            // Header y cuerpo deben tocarse: el cuerpo es parte del mismo ítem.
            ui.spacing_mut().item_spacing.y = 0.0;

            let header_response = add_header(ui, openness);

            state.show_body_unindented(ui, |ui| {
                let rect = ui.max_rect();
                ui.painter()
                    .hline(rect.x_range(), rect.top(), Stroke::new(1.0, theme::BORDER));

                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(6, 8))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        add_body(ui);
                    });
            });

            header_response
        })
        .inner
}

/// Encabezado clicable. `depth` 0 = acordeón principal, 1 = sub-acordeón.
pub fn header(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    open: bool,
    openness: f32,
    depth: u8,
) -> egui::Response {
    let height = if depth == 0 { 36.0 } else { 32.0 };
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    let visual_open = openness > 0.0;
    let fill = if visual_open {
        theme::ACCORDION_OPEN_BG
    } else if response.hovered() {
        theme::ACCORDION_HEADER_HOVER
    } else {
        theme::ACCORDION_HEADER_BG
    };

    let radius = if depth == 0 { 6 } else { 4 };
    let corners = if visual_open {
        egui::CornerRadius {
            nw: radius,
            ne: radius,
            sw: 0,
            se: 0,
        }
    } else {
        egui::CornerRadius::same(radius)
    };

    let painter = ui.painter();
    painter.rect(rect, corners, fill, Stroke::NONE, StrokeKind::Inside);

    let text_color = if visual_open {
        theme::ACCENT
    } else {
        theme::TEXT
    };
    painter.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        format!("{icon}  {title}"),
        FontId::proportional(if depth == 0 { 14.5 } else { 14.0 }),
        text_color,
    );
    painter.text(
        rect.right_center() - egui::vec2(10.0, 0.0),
        Align2::RIGHT_CENTER,
        if open {
            icons::CARET_DOWN
        } else {
            icons::CARET_RIGHT
        },
        FontId::proportional(14.0),
        theme::TEXT_MUTED,
    );

    response
}
