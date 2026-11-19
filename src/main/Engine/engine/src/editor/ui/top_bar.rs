//! Barra superior del editor.
//!
//! Play/Debug controlan el motor directamente en Rust. No hay botón de Stop:
//! en modo juego el motor captura el cursor, así que se sale con ESC.
//! Guardar/Auto se ejecutan también en Rust: el editor pide el estado del
//! proyecto a la UI React y escribe el `.save` (manifiesto + assets + ZIP).

use egui::{Align, Color32, Layout, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::{EditorApp, EditorStatus};
use crate::editor::i18n::Text;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let text = app.text();
    let compact = ui.available_width() < 600.0;

    ui.columns(3, |columns| {
        columns[0].with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.add_space(8.0);
            left_group(ui, app, &text);
        });

        columns[1].with_layout(Layout::left_to_right(Align::Center), |ui| {
            // El `horizontal` limita la altura al de una fila; sin él el badge
            // se estiraría para ocupar toda la altura de la barra.
            ui.horizontal(|ui| center_group(ui, app, compact));
        });

        columns[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            right_group(ui, app, compact, &text);
        });
    });
}

fn left_group(ui: &mut egui::Ui, app: &mut EditorApp, text: &Text) {
    let can_use = app.status == EditorStatus::Ready;

    // En juego el cursor está capturado: este botón solo sirve para entrar,
    // para salir se usa ESC.
    if labeled_button(
        ui,
        icons::PLAY,
        text.play_game,
        None,
        can_use && !app.preview_playing,
        app.preview_playing,
        crate::editor::theme::PLAY_BG,
    ) {
        app.send_engine(json!({ "cmd": "set_preview_playing", "playing": true }));
        app.preview_playing = true;
    }

    if labeled_button(
        ui,
        icons::BUG,
        text.debug,
        Some(text.debug_hint),
        can_use,
        app.debug_mode,
        crate::editor::theme::DEBUG_BG,
    ) {
        app.debug_mode = !app.debug_mode;
        app.send_engine(json!({ "cmd": "set_debug_mode", "show": app.debug_mode }));
    }
}

fn center_group(ui: &mut egui::Ui, app: &EditorApp, compact: bool) {
    let project_type = app.cli.context.project_type.as_str();

    let title_font = egui::FontId::new(16.0, egui::FontFamily::Name("rer-bold".into()));

    // `ui.horizontal` ocuparía todo el ancho de la columna, así que medimos el
    // contenido y añadimos el espacio izquierdo necesario para centrarlo.
    let title_width = if compact {
        0.0
    } else {
        ui.ctx().fonts_mut(|fonts| {
            fonts
                .layout_no_wrap("RER-ENGINE".to_owned(), title_font.clone(), Color32::WHITE)
                .rect
                .width()
        })
    };
    let badge_width = ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                project_type.to_owned(),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            )
            .rect
            .width()
    }) + 16.0; // márgenes del badge (7+7) + borde (1+1)
    let content_width = if compact {
        badge_width
    } else {
        title_width + ui.spacing().item_spacing.x + badge_width
    };
    ui.add_space(((ui.available_width() - content_width) * 0.5).max(0.0));

    if !compact {
        ui.label(
            RichText::new("RER-ENGINE")
                .font(title_font)
                .color(crate::editor::theme::ACCENT),
        );
    }

    badge(
        ui,
        project_type,
        crate::editor::theme::BADGE_GREEN_BG,
        crate::editor::theme::BADGE_GREEN_TEXT,
        Some(crate::editor::theme::BADGE_GREEN_BORDER),
    );
}

fn right_group(ui: &mut egui::Ui, app: &mut EditorApp, compact: bool, text: &Text) {
    // El layout va de derecha a izquierda: primero Auto (extremo derecho), luego Save.
    auto_button(ui, app, compact, text);
    save_button(ui, app, compact, text);
}

/// Botón con icono y texto y fondo sólido. El fondo se oscurece al activarse y
/// un poco más mientras se mantiene pulsado; el borde es un tono claro del
/// propio color, no blanco.
fn labeled_button(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    tooltip: Option<&str>,
    enabled: bool,
    active: bool,
    fill: Color32,
) -> bool {
    // egui resuelve el estado visual con la respuesta del frame anterior;
    // leemos esa misma respuesta para reforzar el efecto al pulsar.
    let id = ui.next_auto_id();
    let pressed = ui
        .ctx()
        .read_response(id)
        .is_some_and(|response| response.is_pointer_button_down_on());

    let current_fill = if pressed {
        fill.lerp_to_gamma(Color32::BLACK, 0.40)
    } else if active {
        fill.lerp_to_gamma(Color32::BLACK, 0.22)
    } else {
        fill
    };
    let border = if active || pressed {
        fill.lerp_to_gamma(Color32::WHITE, 0.45)
    } else {
        fill
    };

    let text = RichText::new(format!("{}  {}", icon, label))
        .size(14.0)
        .color(Color32::WHITE);
    let button = egui::Button::new(text)
        .min_size(egui::vec2(0.0, 30.0))
        .fill(current_fill)
        .stroke(egui::Stroke::new(1.0, border));

    let response = ui.add_enabled(enabled, button);
    let response = if let Some(tooltip) = tooltip {
        response.on_hover_text(tooltip)
    } else {
        response
    };
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    enabled && response.clicked()
}

fn save_button(ui: &mut egui::Ui, app: &mut EditorApp, compact: bool, text: &Text) {
    let label = if compact {
        icons::FLOPPY_DISK.to_owned()
    } else {
        format!("{}  {}", icons::FLOPPY_DISK, text.save_project)
    };

    // Si el último guardado falló, el botón queda marcado en rojo.
    let failed = app.save_error.is_some();
    let color = if failed {
        crate::editor::theme::DANGER
    } else {
        crate::editor::theme::TEXT
    };
    let stroke = if failed {
        crate::editor::theme::DANGER
    } else {
        crate::editor::theme::PRIMARY_OUTLINE
    };

    let button = egui::Button::new(RichText::new(label).size(14.0).color(color))
        .min_size(egui::vec2(0.0, 28.0))
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, stroke));

    let enabled = app.can_save();
    let response = ui.add_enabled(enabled, button);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        let reason = if app.saving {
            text.saving_project
        } else {
            text.save_unavailable
        };
        response.on_disabled_hover_text(format!("{} — {reason}", text.save))
    };

    if enabled && response.clicked() {
        app.begin_save(false);
    }
}

fn auto_button(ui: &mut egui::Ui, app: &mut EditorApp, compact: bool, text: &Text) {
    let label = if compact {
        icons::CLOCK.to_owned()
    } else {
        format!(
            "{}  {}",
            icons::CLOCK,
            if app.auto_save_enabled {
                text.auto_on
            } else {
                text.auto_off
            }
        )
    };

    let color = if app.auto_save_enabled {
        crate::editor::theme::WARNING_TEXT
    } else {
        crate::editor::theme::OUTLINE_SECONDARY
    };
    let fill = if app.auto_save_enabled {
        crate::editor::theme::WARNING
    } else {
        Color32::TRANSPARENT
    };

    let button = egui::Button::new(RichText::new(label).size(14.0).color(color))
        .min_size(egui::vec2(0.0, 28.0))
        .fill(fill)
        .stroke(egui::Stroke::new(
            1.0,
            crate::editor::theme::OUTLINE_SECONDARY,
        ));

    let enabled = app.has_saved_once && !app.saving;
    let response = ui.add_enabled(enabled, button);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        let reason = if !app.has_saved_once {
            text.save_first
        } else {
            text.saving_project
        };
        response.on_disabled_hover_text(reason)
    };

    if enabled && response.clicked() {
        app.toggle_auto_save();
    }
}

fn badge(
    ui: &mut egui::Ui,
    label: &str,
    fill: Color32,
    text_color: Color32,
    border: Option<Color32>,
) {
    let mut frame = egui::Frame::new()
        .fill(fill)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(7, 1));

    if let Some(border) = border {
        frame = frame.stroke(egui::Stroke::new(1.0, border));
    }

    frame.show(ui, |ui| {
        ui.label(RichText::new(label).size(12.0).color(text_color));
    });
}
