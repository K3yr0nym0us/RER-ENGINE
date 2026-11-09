//! Controles visuales reutilizables de las sidebars.
//!
//! Todos están deshabilitados salvo el selector de idioma: la funcionalidad
//! del motor llega en la Fase 5. Los botones anchos se pintan a mano para
//! centrar icono + texto, como hace Bootstrap con `.btn` (egui alinea a la
//! izquierda cuando el botón se estira).

use egui::{Align2, Color32, FontId, RichText, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icons;

pub fn caption(ui: &mut egui::Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .size(12.0)
            .strong()
            .color(crate::editor::theme::TEXT_MUTED),
    );
}

pub fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(12.0)
            .italics()
            .color(crate::editor::theme::TEXT_MUTED),
    );
}

/// Nota pequeña del sidebar derecho (React: 0.72rem).
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(10.0)
            .color(crate::editor::theme::TEXT_MUTED),
    );
}

/// Nota aún más pequeña (React: 0.68rem).
pub fn tiny(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(9.5)
            .color(crate::editor::theme::TEXT_MUTED),
    );
}

/// Fila `label ... valor` con el valor a la derecha en color info.
pub fn label_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .size(12.0)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .size(12.0)
                    .color(crate::editor::theme::INFO),
            );
        });
    });
}

/// Etiquetas extremas de un range (p. ej. Nítido / Suave).
pub fn range_end_labels(ui: &mut egui::Ui, left: &str, right: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(left)
                .size(9.5)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(right)
                    .size(9.5)
                    .color(crate::editor::theme::TEXT_MUTED),
            );
        });
    });
}

/// Range de Bootstrap interactivo. Devuelve `true` si el valor cambió.
pub fn range_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    accent: Color32,
) -> bool {
    let enabled = ui.is_enabled();
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let mut changed = false;
    if enabled
        && (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let t = ((pointer.x - rect.left() - 6.0) / (rect.width() - 12.0)).clamp(0.0, 1.0);
        let raw = min + t * (max - min);
        let snapped = if step > 0.0 {
            (raw / step).round() * step
        } else {
            raw
        };
        let next = snapped.clamp(min, max);
        if (next - *value).abs() > 1e-6 {
            *value = next;
            changed = true;
        }
    }

    let alpha = if enabled { 1.0 } else { 0.5 };
    let painter = ui.painter();
    let y = rect.center().y;
    let track = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 6.0, y - 2.0),
        egui::pos2(rect.right() - 6.0, y + 2.0),
    );
    painter.rect_filled(
        track,
        egui::CornerRadius::same(2),
        crate::editor::theme::RANGE_TRACK.linear_multiply(alpha),
    );

    let t = if (max - min).abs() > f32::EPSILON {
        ((*value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let x = rect.left() + 6.0 + (rect.width() - 12.0) * t;
    painter.circle_filled(egui::pos2(x, y), 6.5, accent.linear_multiply(alpha));

    let _ = response.on_hover_text(format!("{value:.2}"));
    changed
}

pub fn select_placeholder(ui: &mut egui::Ui, value: &str, hint: &str) {
    select_placeholder_sized(ui, ui.available_width(), value, hint);
}

pub fn select_placeholder_sized(ui: &mut egui::Ui, width: f32, value: &str, hint: &str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::hover());

    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        crate::editor::theme::PANEL_BG,
        Stroke::new(1.0, crate::editor::theme::BORDER),
        StrokeKind::Inside,
    );
    painter.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        value,
        FontId::proportional(13.0),
        crate::editor::theme::TEXT,
    );
    painter.text(
        rect.right_center() - egui::vec2(8.0, 0.0),
        Align2::RIGHT_CENTER,
        icons::CARET_DOWN,
        FontId::proportional(13.0),
        crate::editor::theme::TEXT_MUTED,
    );

    response.on_hover_text(hint);
}

/// Botón pequeño de acción (check) con borde info.
pub fn small_icon_button(ui: &mut egui::Ui, icon: &str, hint: &str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::hover());

    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        Color32::TRANSPARENT,
        Stroke::new(1.0, crate::editor::theme::INFO),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(15.0),
        crate::editor::theme::INFO,
    );

    response.on_hover_text(hint);
}

pub fn wide_button(ui: &mut egui::Ui, icon: &str, label: &str, hint: &str, color: Color32) {
    wide_button_sized(ui, ui.available_width(), icon, label, hint, color);
}

pub fn wide_button_sized(
    ui: &mut egui::Ui,
    width: f32,
    icon: &str,
    label: &str,
    hint: &str,
    color: Color32,
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::hover());

    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        Color32::TRANSPARENT,
        Stroke::new(1.0, color),
        StrokeKind::Inside,
    );

    let text = if icon.is_empty() {
        label.to_owned()
    } else {
        format!("{icon}  {label}")
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(13.0),
        color,
    );

    response.on_hover_text(format!("{label} — {hint}"));
}

/// Tarjeta 64×64 de la caja de herramientas (3 líneas: texto, icono, texto).
pub fn tool_card(
    ui: &mut egui::Ui,
    icon: &str,
    top: &str,
    bottom: &str,
    label: &str,
    color: Color32,
    hint: &str,
) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), Sense::hover());

    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        Color32::TRANSPARENT,
        Stroke::new(1.0, color),
        StrokeKind::Inside,
    );

    let center = rect.center();
    painter.text(
        center - egui::vec2(0.0, 18.0),
        Align2::CENTER_CENTER,
        top,
        FontId::proportional(9.0),
        color,
    );
    painter.text(
        center,
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(20.0),
        color,
    );
    painter.text(
        center + egui::vec2(0.0, 18.0),
        Align2::CENTER_CENTER,
        bottom,
        FontId::proportional(9.0),
        color,
    );

    response.on_hover_text(format!("{label} — {hint}"));
}

/// Botón compacto del idioma (muestra el idioma al que se cambiaría).
pub fn language_button(ui: &mut egui::Ui, width: f32, label: &str, tooltip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    let hovered = response.hovered();
    let fill = Color32::from_rgb(0x0f, 0x11, 0x20);
    let border = if hovered {
        Color32::from_rgb(0x47, 0x55, 0x69)
    } else {
        crate::editor::theme::BORDER
    };
    let text_color = if hovered {
        Color32::from_rgb(0xcb, 0xd5, 0xe1)
    } else {
        Color32::from_rgb(0x94, 0xa3, 0xb8)
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(6),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        text_color,
    );

    let response = response.on_hover_text(tooltip);
    response.clicked()
}

// ---------------------------------------------------------------------------
// Controles interactivos (barra derecha funcional)
// ---------------------------------------------------------------------------

/// Switch de Bootstrap interactivo. Devuelve `true` si cambió.
pub fn switch_value(ui: &mut egui::Ui, value: &mut bool, hint: &str) -> bool {
    let enabled = ui.is_enabled();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 18.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let mut changed = false;
    if enabled && response.clicked() {
        *value = !*value;
        changed = true;
    }

    let alpha = if enabled { 1.0 } else { 0.55 };
    let painter = ui.painter();
    let fill = if *value {
        crate::editor::theme::CONTROL_BLUE
    } else {
        crate::editor::theme::SWITCH_OFF
    };
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(9),
        fill.linear_multiply(alpha),
    );

    let cx = if *value {
        rect.right() - 9.0
    } else {
        rect.left() + 9.0
    };
    painter.circle_filled(
        egui::pos2(cx, rect.center().y),
        7.0,
        Color32::WHITE.linear_multiply(alpha),
    );

    let _ = response.on_hover_text(hint);
    changed
}

/// Icono cuadrado interactivo. Devuelve `true` si se pulsó.
pub fn icon_toggle_value(ui: &mut egui::Ui, icon: &str, value: &mut bool, hint: &str) -> bool {
    let enabled = ui.is_enabled();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 26.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let mut changed = false;
    if enabled && response.clicked() {
        *value = !*value;
        changed = true;
    }

    let (fill, stroke, color) = if *value {
        (
            crate::editor::theme::INFO,
            crate::editor::theme::INFO,
            crate::editor::theme::WARNING_TEXT,
        )
    } else {
        (
            Color32::TRANSPARENT,
            crate::editor::theme::OUTLINE_SECONDARY,
            crate::editor::theme::OUTLINE_SECONDARY,
        )
    };
    let alpha = if enabled { 1.0 } else { 0.55 };

    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        fill.linear_multiply(alpha),
        Stroke::new(1.0, stroke.linear_multiply(alpha)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(14.0),
        color.linear_multiply(alpha),
    );

    let _ = response.on_hover_text(hint);
    changed
}

/// Pastilla interactiva. Devuelve `true` si se pulsó.
pub fn pill_button_click(ui: &mut egui::Ui, label: &str, active: bool, hint: &str) -> bool {
    let enabled = ui.is_enabled();
    let font = FontId::proportional(12.0);
    let text_color = if active {
        crate::editor::theme::WARNING_TEXT
    } else {
        crate::editor::theme::OUTLINE_SECONDARY
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), text_color);
    let width = galley.size().x + 16.0;

    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let (fill, stroke) = if active {
        (crate::editor::theme::INFO, crate::editor::theme::INFO)
    } else {
        (
            Color32::TRANSPARENT,
            crate::editor::theme::OUTLINE_SECONDARY,
        )
    };
    let alpha = if enabled { 1.0 } else { 0.55 };

    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        fill.linear_multiply(alpha),
        Stroke::new(1.0, stroke.linear_multiply(alpha)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        text_color.linear_multiply(alpha),
    );

    let clicked = enabled && response.clicked();
    let _ = response.on_hover_text(hint);
    clicked
}

/// Fila de pastillas interactiva: devuelve el índice pulsado.
pub fn tier_row_choice(ui: &mut egui::Ui, tiers: &[(&str, bool)], hint: &str) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (index, (label, active)) in tiers.iter().enumerate() {
            if pill_button_click(ui, label, *active, hint) {
                clicked = Some(index);
            }
        }
    });
    clicked
}

/// Botón pequeño de acción interactivo. Devuelve `true` si se pulsó.
pub fn small_icon_button_click(ui: &mut egui::Ui, icon: &str, hint: &str) -> bool {
    let enabled = ui.is_enabled();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let alpha = if enabled { 1.0 } else { 0.55 };
    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        Color32::TRANSPARENT,
        Stroke::new(1.0, crate::editor::theme::INFO.linear_multiply(alpha)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(15.0),
        crate::editor::theme::INFO.linear_multiply(alpha),
    );

    let clicked = enabled && response.clicked();
    let _ = response.on_hover_text(hint);
    clicked
}

/// Botón ancho interactivo. Devuelve `true` si se pulsó.
pub fn wide_button_click_sized(
    ui: &mut egui::Ui,
    width: f32,
    icon: &str,
    label: &str,
    hint: &str,
    color: Color32,
) -> bool {
    let enabled = ui.is_enabled();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let alpha = if enabled { 1.0 } else { 0.55 };
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        Color32::TRANSPARENT,
        Stroke::new(1.0, color.linear_multiply(alpha)),
        StrokeKind::Inside,
    );

    let text = if icon.is_empty() {
        label.to_owned()
    } else {
        format!("{icon}  {label}")
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(13.0),
        color.linear_multiply(alpha),
    );

    let clicked = enabled && response.clicked();
    let _ = response.on_hover_text(format!("{label} — {hint}"));
    clicked
}

/// Input numérico. Devuelve `true` al confirmar con Enter o al perder el foco.
pub fn number_input(ui: &mut egui::Ui, width: f32, text: &mut String) -> bool {
    let response = ui.add(egui::TextEdit::singleline(text).desired_width(width));
    response.lost_focus()
}

/// Icono de acción compacto (filas de escenas). Devuelve `true` si se pulsó.
pub fn icon_action(ui: &mut egui::Ui, icon: &str, size: f32, color: Color32, hint: &str) -> bool {
    let enabled = ui.is_enabled();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let alpha = if enabled { 1.0 } else { 0.4 };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(size * 0.78),
        color.linear_multiply(alpha),
    );

    let clicked = enabled && response.clicked();
    let _ = response.on_hover_text(hint);
    clicked
}

/// Combo de texto. Devuelve `true` si cambió la selección.
pub fn combo_text(
    ui: &mut egui::Ui,
    id: &str,
    width: f32,
    options: &[(&'static str, &str)],
    selected: &mut &'static str,
) -> bool {
    let selected_label = options
        .iter()
        .find(|(value, _)| *value == *selected)
        .map(|(_, label)| *label)
        .unwrap_or(*selected);

    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for (value, label) in options {
                if ui.selectable_label(*value == *selected, *label).clicked() {
                    *selected = *value;
                    changed = true;
                }
            }
        });
    changed
}

/// Combo numérico (p. ej. límite de FPS). Devuelve `true` si cambió.
pub fn combo_u32(
    ui: &mut egui::Ui,
    id: &str,
    width: f32,
    options: &[u32],
    selected: &mut u32,
) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(format!("{} FPS", *selected))
        .show_ui(ui, |ui| {
            for value in options {
                if ui
                    .selectable_label(*selected == *value, format!("{value} FPS"))
                    .clicked()
                {
                    *selected = *value;
                    changed = true;
                }
            }
        });
    changed
}

/// Combo con opciones dinámicas `(valor, etiqueta)`. Devuelve `true` si cambió.
pub fn combo_string(
    ui: &mut egui::Ui,
    id: &str,
    width: f32,
    options: &[(String, String)],
    selected: &mut Option<String>,
) -> bool {
    let selected_label = selected
        .as_ref()
        .and_then(|value| {
            options
                .iter()
                .find(|(path, _)| path == value)
                .map(|(_, label)| label.clone())
        })
        .unwrap_or_else(|| "—".to_string());

    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for (value, label) in options {
                let is_selected = selected.as_deref() == Some(value.as_str());
                if ui.selectable_label(is_selected, label).clicked() {
                    *selected = Some(value.clone());
                    changed = true;
                }
            }
        });
    changed
}

/// Tarjeta de herramienta clicable. Devuelve `true` si se pulsó.
pub fn tool_card_click(
    ui: &mut egui::Ui,
    icon: &str,
    top: &str,
    bottom: &str,
    label: &str,
    color: Color32,
    active: bool,
    hint: &str,
) -> bool {
    let enabled = ui.is_enabled();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let alpha = if enabled { 1.0 } else { 0.5 };
    let fill = if active {
        color.linear_multiply(0.22)
    } else if response.hovered() && enabled {
        color.linear_multiply(0.10)
    } else {
        Color32::TRANSPARENT
    };

    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(5),
        fill,
        Stroke::new(1.0, color.linear_multiply(alpha)),
        StrokeKind::Inside,
    );

    let center = rect.center();
    painter.text(
        center - egui::vec2(0.0, 18.0),
        Align2::CENTER_CENTER,
        top,
        FontId::proportional(9.0),
        color.linear_multiply(alpha),
    );
    painter.text(
        center,
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(20.0),
        color.linear_multiply(alpha),
    );
    painter.text(
        center + egui::vec2(0.0, 18.0),
        Align2::CENTER_CENTER,
        bottom,
        FontId::proportional(9.0),
        color.linear_multiply(alpha),
    );

    let clicked = enabled && response.clicked();
    let _ = response.on_hover_text(format!("{label} — {hint}"));
    clicked
}
