//! Barra superior del editor.
//!
//! Play/Stop/Debug controlan el motor directamente en Rust. Guardar/Auto se
//! ejecutan también en Rust: el editor pide el estado del proyecto a la UI
//! React y escribe el `.save` (manifiesto + assets + ZIP).

use egui::{Align, Color32, Layout, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::app::{EditorApp, EditorStatus};
use crate::i18n::Text;
use crate::theme;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let text = app.text();
    let compact = ui.available_width() < 600.0;

    ui.columns(3, |columns| {
        columns[0].with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.add_space(8.0);
            left_group(ui, app, &text);
        });

        columns[1].vertical_centered(|ui| {
            let vertical_pad = ((ui.available_height() - 28.0) * 0.5).max(0.0);
            ui.add_space(vertical_pad);
            ui.horizontal(|ui| center_group(ui, app, compact, &text));
        });

        columns[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            right_group(ui, app, compact, &text);
        });
    });
}

fn left_group(ui: &mut egui::Ui, app: &mut EditorApp, text: &Text) {
    let can_use = app.status == EditorStatus::Ready;
    let playing = app.preview_playing;

    if icon_button(
        ui,
        icons::PLAY,
        text.play,
        can_use && !playing,
        playing,
        if playing {
            theme::SUCCESS
        } else {
            Color32::TRANSPARENT
        },
        if playing {
            theme::SUCCESS
        } else {
            theme::PRIMARY_OUTLINE
        },
        if playing { Color32::WHITE } else { theme::TEXT },
    ) {
        app.send_engine(json!({ "cmd": "set_preview_playing", "playing": true }));
        app.preview_playing = true;
    }

    let stopped = !playing;
    if icon_button(
        ui,
        icons::STOP,
        text.stop,
        can_use && playing,
        stopped,
        if stopped {
            theme::DANGER
        } else {
            Color32::TRANSPARENT
        },
        if stopped {
            theme::DANGER
        } else {
            theme::PRIMARY_OUTLINE
        },
        if stopped { Color32::WHITE } else { theme::TEXT },
    ) {
        app.send_engine(json!({ "cmd": "set_preview_playing", "playing": false }));
        app.preview_playing = false;
    }

    let debug = app.debug_mode;
    if icon_button(
        ui,
        icons::BUG,
        text.debug,
        can_use,
        debug,
        if debug {
            theme::SUCCESS
        } else {
            Color32::TRANSPARENT
        },
        theme::SUCCESS,
        if debug {
            Color32::WHITE
        } else {
            theme::SUCCESS
        },
    ) {
        app.debug_mode = !app.debug_mode;
        app.send_engine(json!({ "cmd": "set_debug_mode", "show": app.debug_mode }));
    }
}

fn center_group(ui: &mut egui::Ui, app: &EditorApp, compact: bool, text: &Text) {
    if !compact {
        ui.label(
            RichText::new("RER-ENGINE")
                .strong()
                .size(16.0)
                .color(theme::ACCENT),
        );
    }

    badge(
        ui,
        app.cli.context.project_type.as_str(),
        theme::BADGE_GREEN_BG,
        theme::BADGE_GREEN_TEXT,
        Some(theme::BADGE_GREEN_BORDER),
    );

    match app.status {
        EditorStatus::Starting => badge(
            ui,
            text.starting,
            theme::WARNING_BADGE,
            theme::WARNING_TEXT,
            None,
        ),
        EditorStatus::Ready => badge(ui, "◉", theme::SUCCESS, Color32::WHITE, None),
        EditorStatus::Error => badge(ui, text.error, theme::DANGER, Color32::WHITE, None),
    }
}

fn right_group(ui: &mut egui::Ui, app: &mut EditorApp, compact: bool, text: &Text) {
    // El layout va de derecha a izquierda: primero Auto (extremo derecho), luego Save.
    auto_button(ui, app, compact, text);
    save_button(ui, app, compact, text);
}

#[allow(clippy::too_many_arguments)]
fn icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    enabled: bool,
    active: bool,
    fill: Color32,
    stroke: Color32,
    text_color: Color32,
) -> bool {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 30.0), sense);
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };

    let dim = !enabled && !active;
    let fill = if dim {
        fill.linear_multiply(0.55)
    } else {
        fill
    };
    let stroke = if dim {
        stroke.linear_multiply(0.55)
    } else {
        stroke
    };
    let text_color = if dim {
        text_color.linear_multiply(0.55)
    } else {
        text_color
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(4),
        fill,
        egui::Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(17.0),
        text_color,
    );

    let response = response.on_hover_text(label);
    enabled && response.clicked()
}

fn save_button(ui: &mut egui::Ui, app: &mut EditorApp, compact: bool, text: &Text) {
    let label = if compact {
        icons::FLOPPY_DISK.to_owned()
    } else {
        format!("{}  {}", icons::FLOPPY_DISK, text.save)
    };

    // Si el último guardado falló, el botón queda marcado en rojo.
    let failed = app.save_error.is_some();
    let color = if failed { theme::DANGER } else { theme::TEXT };
    let stroke = if failed {
        theme::DANGER
    } else {
        theme::PRIMARY_OUTLINE
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
        theme::WARNING_TEXT
    } else {
        theme::OUTLINE_SECONDARY
    };
    let fill = if app.auto_save_enabled {
        theme::WARNING
    } else {
        Color32::TRANSPARENT
    };

    let button = egui::Button::new(RichText::new(label).size(14.0).color(color))
        .min_size(egui::vec2(0.0, 28.0))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, theme::OUTLINE_SECONDARY));

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
        response.on_disabled_hover_text(format!("{} — {reason}", text.auto_off))
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
