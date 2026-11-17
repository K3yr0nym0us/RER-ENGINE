//! Cuerpo del editor de emisores de partículas (F5), pintado en una ventana
//! nativa winit aparte (`particle_window`). Al confirmar envía
//! `add_particle_emitter` con id de UI: el motor hace upsert (alta o reemplazo
//! si el id ya existe) y el sidebar actualiza la misma fila; cancelar o cerrar
//! la ventana OS no tocan el motor.

use egui::RichText;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::sidebar_right::particles::{
    EmitterRow, MATERIAL_OPTIONS, field_mask, preset_values,
};

use super::widgets;

/// Dibuja el formulario a pantalla completa de la ventana nativa del emisor.
pub(crate) fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    if app.right_sidebar.particles.modal.is_none() {
        return;
    }
    let is_new = app.right_sidebar.particles.modal_is_new;
    let locale = app.cli.locale;
    let mut confirm = false;
    let mut cancel = false;

    egui::Frame::new()
        .fill(theme::SIDEBAR_BG)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            let Some(draft) = app.right_sidebar.particles.modal.as_mut() else {
                return;
            };
            ui.set_width(ui.available_width());
            // Material: solo se elige en la alta nueva (editar mantiene el del emisor).
            if is_new {
                ui.label(field_label(locale, "Material", "Material"));
                egui::ComboBox::from_id_salt("particles_material_combo")
                    .width(ui.available_width())
                    .selected_text(
                        RichText::new(
                            MATERIAL_OPTIONS
                                .iter()
                                .find(|(wire, _, _)| *wire == draft.material)
                                .map(|option| tr(locale, option.1, option.2))
                                .unwrap_or(draft.material),
                        )
                        .size(12.5),
                    )
                    .show_ui(ui, |ui| {
                        let mut next = draft.material;
                        for (wire, es, en) in MATERIAL_OPTIONS {
                            ui.selectable_value(
                                &mut next,
                                wire,
                                RichText::new(tr(locale, es, en)).size(12.5),
                            );
                        }
                        if next != draft.material {
                            // Cambiar material recarga el preset del sidebar.
                            draft.apply_values(&preset_values(next));
                            draft.material = next;
                            draft.pin_inapplicable_fields();
                        }
                    });
            } else {
                let material = MATERIAL_OPTIONS
                    .iter()
                    .find(|(wire, _, _)| *wire == draft.material)
                    .map(|option| tr(locale, option.1, option.2))
                    .unwrap_or(draft.material);
                widgets::tiny(ui, material);
            }
            ui.add_space(6.0);

            draft.pin_inapplicable_fields();
            let mask = field_mask(draft.material);

            position_row(ui, locale, draft);
            ui.add_enabled_ui(mask.rate, |ui| {
                number_row(
                    ui,
                    locale,
                    "Emisión (part/s)",
                    "Emission (part/s)",
                    &mut draft.rate,
                );
            });
            number_row(ui, locale, "Tamaño (m)", "Size (m)", &mut draft.size);
            number_row(ui, locale, "Vida (s)", "Life (s)", &mut draft.life);
            ui.add_enabled_ui(mask.gravity, |ui| {
                number_row(
                    ui,
                    locale,
                    "Gravedad (m/s²)",
                    "Gravity (m/s²)",
                    &mut draft.gravity,
                );
            });
            ui.add_enabled_ui(mask.drag, |ui| {
                number_row(ui, locale, "Arrastre", "Drag", &mut draft.drag);
            });
            ui.add_enabled_ui(mask.turbulence, |ui| {
                number_row(
                    ui,
                    locale,
                    "Turbulencia",
                    "Turbulence",
                    &mut draft.turbulence,
                );
            });
            ui.add_enabled_ui(mask.cohesion, |ui| {
                number_row(ui, locale, "Cohesión", "Cohesion", &mut draft.cohesion);
            });
            ui.add_enabled_ui(mask.ground_y, |ui| {
                number_row(ui, locale, "Suelo (m)", "Ground (m)", &mut draft.ground_y);
            });
            ui.add_enabled_ui(mask.wind, |ui| {
                number_row(
                    ui,
                    locale,
                    "Viento X (m/s)",
                    "Wind X (m/s)",
                    &mut draft.wind[0],
                );
                number_row(
                    ui,
                    locale,
                    "Viento Z (m/s)",
                    "Wind Z (m/s)",
                    &mut draft.wind[1],
                );
            });

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(mask.ground_collision, |ui| {
                    checkbox_tr(
                        ui,
                        locale,
                        "Colisión suelo",
                        "Ground collision",
                        &mut draft.ground_collision,
                    );
                });
                ui.add_enabled_ui(mask.accumulate, |ui| {
                    checkbox_tr(ui, locale, "Acumular", "Accumulate", &mut draft.accumulate);
                });
                ui.add_enabled_ui(mask.splash, |ui| {
                    checkbox_tr(ui, locale, "Salpicar", "Splash", &mut draft.splash);
                });
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(
                        RichText::new(tr(locale, "Confirmar", "Confirm"))
                            .size(13.0)
                            .color(theme::TEXT),
                    ))
                    .clicked()
                {
                    confirm = true;
                }
                if ui
                    .add(egui::Button::new(
                        RichText::new(tr(locale, "Cancelar", "Cancel"))
                            .size(13.0)
                            .color(theme::TEXT_MUTED),
                    ))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });

    if !(confirm || cancel) {
        return;
    }
    // Libera el préstamo del borrador antes de tocar el estado.
    let mut draft = app
        .right_sidebar
        .particles
        .modal
        .take()
        .expect("borrador abierto");
    if !confirm {
        return;
    }

    // Confirmar: id de UI para altas nuevas (edición conserva el id) + fila del sidebar.
    if is_new {
        draft.id = crate::editor::ui::sidebar_right::particles::next_ui_id(
            &mut app.right_sidebar.particles,
        );
    }
    let command = app.right_sidebar.particles.emitter_command(&draft);
    app.send_engine(command);
    let emitters = &mut app.right_sidebar.particles.emitters;
    match emitters.iter_mut().find(|existing| existing.id == draft.id) {
        Some(existing) => {
            *existing = draft.clone();
        }
        None => {
            emitters.push(draft.clone());
        }
    }
    let material = draft.material;
    let kind_label = MATERIAL_OPTIONS
        .iter()
        .find(|(wire, _, _)| *wire == material)
        .map(|option| option.1.to_string())
        .unwrap_or_else(|| material.to_string());
    app.push_log(
        format!(
            "[Partículas] Emisor «{kind_label}» confirmado (id {})",
            draft.id
        ),
        false,
    );
}

/// Fila de posición X/Y/Z en columnas (patrón de la sección Cámara del sidebar).
fn position_row(ui: &mut egui::Ui, locale: crate::editor::cli::Locale, draft: &mut EmitterRow) {
    ui.label(field_label(locale, "Posición (m)", "Position (m)"));
    ui.columns(3, |columns| {
        for (index, axis) in ["X", "Y", "Z"].iter().enumerate() {
            let width = columns[index].available_width();
            columns[index].label(RichText::new(*axis).size(11.5).color(theme::TEXT_MUTED));
            widgets::number_input(&mut columns[index], width, &mut draft.position[index].text);
        }
    });
}

fn number_row(
    ui: &mut egui::Ui,
    locale: crate::editor::cli::Locale,
    es: &'static str,
    en: &'static str,
    draft: &mut crate::editor::ui::number_draft::NumberDraft,
) {
    ui.horizontal(|ui| {
        ui.label(field_label(locale, es, en));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let width = (ui.available_width() - 8.0).max(60.0);
            widgets::number_input(ui, width, &mut draft.text);
        });
    });
}

fn checkbox_tr(
    ui: &mut egui::Ui,
    locale: crate::editor::cli::Locale,
    es: &'static str,
    en: &'static str,
    value: &mut bool,
) {
    ui.checkbox(value, RichText::new(tr(locale, es, en)).size(12.0));
}

fn field_label(
    locale: crate::editor::cli::Locale,
    es: &'static str,
    en: &'static str,
) -> egui::RichText {
    RichText::new(tr(locale, es, en))
        .size(11.5)
        .color(theme::TEXT_MUTED)
}
