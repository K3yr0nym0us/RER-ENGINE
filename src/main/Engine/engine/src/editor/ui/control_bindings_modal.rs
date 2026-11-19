//! Contenido de la ventana nativa de configuración de controles.
//!
//! Réplica del `ControlBindingsModalBody` de React: cabecera con personaje y
//! badge, layout de teclado (8/12) + mouse (4/12) o layout de mando, aviso sin
//! asignaciones y pie de información. El doble click sobre una tecla abre un
//! editor Rhai anidado con el mismo aspecto que el `ScriptEditorModalBody`.

use std::collections::BTreeMap;

use egui::{Align, Color32, Layout, RichText};
use egui_code_editor::CodeEditor;
use egui_phosphor::regular as icons;

use crate::editor::app::EditorApp;
use crate::editor::control_bindings::{
    self, ControlDeviceMode, bindings, character_label, set_binding,
};
use crate::editor::i18n::tr;
use crate::editor::theme;

use super::scene_script_modal::{RER_THEME, rhai_syntax};
use super::widgets;

// Colores exactos del CSS del renderer (`index.css`, bloque "Control mapping modal").
const KEY_BORDER: Color32 = Color32::from_rgb(0x4a, 0x4f, 0x78);
const KEY_BG: Color32 = Color32::from_rgb(0x1b, 0x20, 0x36);
const KEY_TEXT: Color32 = Color32::from_rgb(0xdd, 0xe3, 0xf1);
const KEY_HOVER_BG: Color32 = Color32::from_rgb(0x25, 0x2c, 0x4a);
const KEY_ASSIGNED_BORDER: Color32 = Color32::from_rgb(0x2e, 0xa8, 0x6f);
const KEY_ASSIGNED_BG: Color32 = Color32::from_rgb(0x1d, 0x3a, 0x33);
const KEY_DOT: Color32 = Color32::from_rgb(0x47, 0xd1, 0x8f);
const SECTION_TITLE: Color32 = Color32::from_rgb(0x88, 0x90, 0xb5);
const BADGE_BG: Color32 = Color32::from_rgb(0x6c, 0x75, 0x7d);

fn key_label(key: &str) -> &str {
    match key {
        "MOUSE_LEFT" => "MOUSE LEFT",
        "MOUSE_RIGHT" => "MOUSE RIGHT",
        "MOUSE_MIDDLE" => "MOUSE MIDDLE",
        other => other,
    }
}

/// Botón de tecla: borde redondeado, texto 11px en negrita y punto verde si tiene script.
fn key_button(ui: &mut egui::Ui, label: &str, assigned: bool, wide: bool) -> bool {
    let old_style = ui.style().as_ref().clone();
    {
        let style = ui.style_mut();
        let states = [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ];
        for state in states {
            state.bg_fill = if assigned { KEY_ASSIGNED_BG } else { KEY_BG };
            state.weak_bg_fill = state.bg_fill;
            state.bg_stroke = egui::Stroke::new(
                1.0,
                if assigned {
                    KEY_ASSIGNED_BORDER
                } else {
                    KEY_BORDER
                },
            );
            state.fg_stroke.color = KEY_TEXT;
            state.corner_radius = egui::CornerRadius::same(8);
        }
        style.visuals.widgets.hovered.bg_fill = KEY_HOVER_BG;
        style.visuals.widgets.hovered.weak_bg_fill = KEY_HOVER_BG;
        style.visuals.widgets.active.bg_fill = KEY_HOVER_BG;
        style.visuals.widgets.active.weak_bg_fill = KEY_HOVER_BG;
    }

    let width =
        (label.chars().count() as f32 * 7.0 + 18.0 + if assigned { 10.0 } else { 0.0 }).max(34.0);
    let response = if wide {
        ui.add_sized(
            [ui.available_width().max(80.0), 34.0],
            egui::Button::new(RichText::new(label).size(11.0).strong().color(KEY_TEXT)),
        )
    } else {
        ui.add_sized(
            [width, 34.0],
            egui::Button::new(RichText::new(label).size(11.0).strong().color(KEY_TEXT)),
        )
    };
    if assigned {
        let dot = egui::pos2(response.rect.right() - 9.0, response.rect.center().y);
        ui.painter().circle_filled(dot, 3.0, KEY_DOT);
    }
    let clicked = response.double_clicked();
    *ui.style_mut() = old_style;
    clicked
}

fn row(
    ui: &mut egui::Ui,
    keys: &[&str],
    bindings: &BTreeMap<String, (String, String)>,
    opened: &mut Option<String>,
    wide_last: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (index, key) in keys.iter().enumerate() {
            if key_button(
                ui,
                key_label(key),
                bindings.contains_key(*key),
                wide_last && index + 1 == keys.len(),
            ) {
                *opened = Some((*key).to_string());
            }
        }
    });
}

/// Línea fina equivalente al `<hr class="ckb-section-hr">` del renderer.
fn hr(ui: &mut egui::Ui, color: Color32) {
    let rect = ui.available_rect_before_wrap();
    let y = rect.top() + 0.5;
    ui.allocate_space(egui::vec2(rect.width(), 1.0));
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(1.0, color),
    );
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.label(
        RichText::new(title.to_uppercase())
            .size(11.0)
            .strong()
            .color(SECTION_TITLE),
    );
    ui.add_space(4.0);
    hr(ui, KEY_BORDER);
    ui.add_space(8.0);
}

fn keyboard_layout(
    ui: &mut egui::Ui,
    bindings: &BTreeMap<String, (String, String)>,
    opened: &mut Option<String>,
) {
    let total = ui.available_width();
    let gap = 24.0;
    let left = (total - gap) * 8.0 / 12.0;
    let right = (total - gap) * 4.0 / 12.0;

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(left, 0.0), Layout::top_down(Align::Min), |ui| {
            section_title(ui, "Teclado");
            row(
                ui,
                &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
                bindings,
                opened,
                false,
            );
            ui.add_space(6.0);
            row(ui, &["Q", "W", "E", "R"], bindings, opened, false);
            ui.add_space(6.0);
            row(ui, &["A", "S", "D", "F"], bindings, opened, false);
            ui.add_space(6.0);
            row(ui, &["Z", "X", "C"], bindings, opened, false);
            ui.add_space(6.0);
            row(
                ui,
                &["SHIFT", "CTRL", "ALT", "SPACE"],
                bindings,
                opened,
                true,
            );
        });
        ui.add_space(gap);
        ui.allocate_ui_with_layout(egui::vec2(right, 0.0), Layout::top_down(Align::Min), |ui| {
            section_title(ui, "Mouse");
            row(ui, &["MOUSE_LEFT"], bindings, opened, false);
            ui.add_space(6.0);
            row(ui, &["MOUSE_RIGHT"], bindings, opened, false);
            ui.add_space(6.0);
            row(ui, &["MOUSE_MIDDLE"], bindings, opened, false);
        });
    });
}

fn gamepad_layout(
    ui: &mut egui::Ui,
    bindings: &BTreeMap<String, (String, String)>,
    opened: &mut Option<String>,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 10.0;

        // Gatillos y bumpers: izquierda LT/LB, derecha RB/RT.
        ui.horizontal(|ui| {
            let half = ui.available_width() / 2.0;
            ui.allocate_ui_with_layout(egui::vec2(half, 0.0), Layout::top_down(Align::Min), |ui| {
                row(ui, &["LT", "LB"], bindings, opened, false)
            });
            ui.allocate_ui_with_layout(egui::vec2(half, 0.0), Layout::top_down(Align::Max), |ui| {
                row(ui, &["RB", "RT"], bindings, opened, false)
            });
        });

        // D-pad | centro | botones de cara.
        ui.horizontal(|ui| {
            let third = ui.available_width() / 3.0;
            ui.allocate_ui_with_layout(
                egui::vec2(third, 0.0),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    row(ui, &["D-UP"], bindings, opened, false);
                    row(ui, &["D-LEFT", "D-RIGHT"], bindings, opened, false);
                    row(ui, &["D-DOWN"], bindings, opened, false);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(third, 0.0),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for key in ["BACK", "L3", "R3", "START"] {
                            if key_button(ui, key, bindings.contains_key(key), false) {
                                *opened = Some(key.to_string());
                            }
                        }
                    });
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(third, 0.0),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    row(ui, &["Y"], bindings, opened, false);
                    row(ui, &["X", "B"], bindings, opened, false);
                    row(ui, &["A"], bindings, opened, false);
                },
            );
        });
    });
}

/// Badge gris `bg-secondary` con el número de asignaciones.
fn count_badge(ui: &mut egui::Ui, count: usize) {
    let text = format!("{count} asignaciones");
    egui::Frame::new()
        .fill(BADGE_BG)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(Color32::WHITE));
        });
}

/// Editor Rhai anidado, con la misma distribución que `ScriptEditorModalBody`.
fn script_editor_modal(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let label = character_label(app);
    let mode = app.control_bindings.mode;
    let Some(mut draft) = app.control_bindings.editor.take() else {
        return;
    };

    let mut save = false;
    let mut cancel = false;
    let syntax = rhai_syntax();
    let title = format!("Script para {} ({})", draft.control_key, label);

    let modal = egui::Modal::new(egui::Id::new("control_bindings_script_editor"))
        .frame(
            egui::Frame::popup(&ui.ctx().style_of(egui::Theme::Dark))
                .fill(theme::PANEL_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(14)),
        )
        .show(ui, |ui| {
            ui.set_width(720.0);
            ui.label(RichText::new(title).size(14.0).strong().color(theme::TEXT));
            ui.add_space(8.0);

            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .desired_width(f32::INFINITY)
                    .hint_text(tr(locale, "Nombre del script", "Script name")),
            );
            ui.add_space(8.0);

            let editor_width = ui.available_width();
            CodeEditor::default()
                .id_source("control_bindings_rhai_editor")
                .with_rows(18)
                .with_fontsize(13.0)
                .with_theme(RER_THEME)
                .with_numlines(true)
                .vscroll(false)
                .desired_width(editor_width)
                .show(ui, &mut draft.source, &syntax);

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::modal_button(
                        ui,
                        icons::FLOPPY_DISK,
                        tr(locale, "Guardar script", "Save script"),
                        widgets::ModalButtonStyle::Primary,
                        130.0,
                        !draft.name.trim().is_empty(),
                    ) {
                        save = true;
                    }
                    ui.add_space(6.0);
                    if widgets::modal_button(
                        ui,
                        icons::X,
                        tr(locale, "Cancelar", "Cancel"),
                        widgets::ModalButtonStyle::Secondary,
                        100.0,
                        true,
                    ) {
                        cancel = true;
                    }
                });
            });
        });

    if modal.should_close() {
        cancel = true;
    }
    if save {
        set_binding(
            app,
            mode,
            &draft.control_key,
            draft.name.trim(),
            &draft.source,
        );
    }
    if !cancel && !save {
        app.control_bindings.editor = Some(draft);
    }
}

/// Dibuja el cuerpo en la ventana modal externa.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    // El snapshot de React puede llegar después de abrir la ventana: en cuanto
    // contenga entidades, seleccionamos el personaje efectivo (jugador primero).
    control_bindings::ensure_character(app);
    let locale = app.cli.locale;
    let mode = app.control_bindings.mode;
    let bindings_for_mode = bindings(app, mode);
    let count = bindings_for_mode.len();
    let label = character_label(app);
    let mut opened: Option<String> = None;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Personaje seleccionado:").size(12.0).color(theme::TEXT_MUTED));
                ui.label(RichText::new(&label).size(12.0).strong().color(theme::TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    count_badge(ui, count);
                });
            });
            ui.add_space(16.0);

            if mode == ControlDeviceMode::KeyboardMouse {
                keyboard_layout(ui, &bindings_for_mode, &mut opened);
            } else {
                gamepad_layout(ui, &bindings_for_mode, &mut opened);
            }

            if count == 0 {
                ui.add_space(14.0);
                ui.label(
                    RichText::new("No hay scripts asignados todavia para este modo.")
                        .size(12.0)
                        .italics()
                        .color(theme::TEXT_MUTED),
                );
            }

            ui.add_space(14.0);
            hr(ui, theme::BORDER);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(icons::INFO).size(14.0).color(theme::INFO));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Haz doble click sobre cualquier tecla/boton para abrir la modal de script y asignarlo.",
                    )
                    .size(12.0)
                    .strong()
                    .color(theme::INFO),
                );
            });
        });

    if let Some(key) = opened {
        let existing = bindings_for_mode.get(&key);
        let is_3d = app.cli.context.is_3d();
        let name = existing
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| format!("{}_{}", mode.json_key(), key.to_lowercase()));
        let source = existing
            .map(|(_, source)| source.clone())
            .unwrap_or_else(|| control_bindings::default_script(locale, &key, is_3d));
        app.control_bindings.editor = Some(crate::editor::control_bindings::ScriptDraft {
            control_key: key,
            name,
            source,
        });
    }

    script_editor_modal(ui, app);
}
