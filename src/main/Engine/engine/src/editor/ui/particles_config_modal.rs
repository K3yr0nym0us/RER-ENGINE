//! Cuerpo del editor de emisores de partículas (F5), pintado como modal flotante
//! de egui dentro de la ventana principal del editor.
//!
//! Dos modos:
//! - Alta nueva (`modal_is_new`): solo elige el material; al confirmar se activa
//!   el ghost y el click del viewport CREA el elemento y abre su modal de
//!   propiedades.
//! - Propiedades (`!modal_is_new`): el botón «Aplicar» envía el comando
//!   (`add_particle_emitter` o `add_water_source`) con el id de UI —el motor hace
//!   upsert— y deja la ventana abierta. No hay botón cancelar.

use egui::RichText;
use egui_phosphor::regular as icons;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::number_draft::NumberDraft;
use crate::editor::ui::sidebar_right::particles::{
    EmitterRow, FieldRange, MATERIAL_OPTIONS, field_mask, field_ranges, next_ui_id, preset_values,
};

use super::widgets;

/// Ancho reservado a la etiqueta de cada slider; el resto es pista + valor.
const SLIDER_LABEL_WIDTH: f32 = 124.0;

/// Dibuja el editor como modal flotante de egui sobre la ventana principal.
/// Los popups (material/color) viven en el mismo viewport, así que pueden
/// desplegarse fuera del rect de la modal sin recortarse.
pub(crate) fn show(ctx: &egui::Context, app: &mut EditorApp) {
    if app.right_sidebar.particles.modal.is_none() {
        return;
    }
    let locale = app.cli.locale;
    let id = egui::Id::new("editor_particle_emitter_modal");
    // Area propia en vez de `Modal::default_area`: ese área usa `anchor(CENTER_CENTER)`,
    // que fija la posición. Con `pivot` + `default_pos` + `movable` la modal se puede
    // arrastrar, empezando centrada en el viewport.
    let area = egui::Area::new(id)
        .kind(egui::UiKind::Modal)
        .order(egui::Order::Foreground)
        .interactable(true)
        .movable(true)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center());
    let modal = egui::Modal::new(id)
        .area(area)
        // Sin el velo negro sobre el viewport del motor; el backdrop sigue
        // capturando clics para cerrar, pero no oscurece la escena.
        .backdrop_color(egui::Color32::TRANSPARENT)
        .frame(
            egui::Frame::popup(&ctx.style_of(egui::Theme::Dark))
                // Mismo fondo que las ventanas modales nativas (PANEL_BG / clear_color),
                // no el SIDEBAR_BG oscuro de los paneles laterales.
                .fill(theme::PANEL_BG)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(430.0);
            ui.label(
                RichText::new(tr(
                    locale,
                    "Editor de emisor de partículas",
                    "Particle emitter editor",
                ))
                .size(15.0)
                .strong()
                .color(theme::TEXT),
            );
            ui.add_space(8.0);
            show_body(ui, app);
        });
    if modal.should_close() {
        // ESC/clic fuera: el body ya conserva el draft si no hubo acción.
        app.right_sidebar.particles.modal = None;
    }
}

fn show_body(ui: &mut egui::Ui, app: &mut EditorApp) {
    let Some(mut draft) = app.right_sidebar.particles.modal.clone() else {
        return;
    };
    let is_new = app.right_sidebar.particles.modal_is_new;
    let is_3d = app.cli.context.is_3d();
    // Alta nueva en 3D: esta modal SOLO elige el material; el ghost coloca el
    // elemento y entonces se abre la modal de propiedades.
    let select_only = is_new && is_3d;
    let locale = app.cli.locale;
    let mut confirm = false;
    let mut cancel = false;
    let mut apply = false;

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

    if !select_only {
        draft.pin_inapplicable_fields();
        let mask = field_mask(draft.material);
        let ranges = field_ranges(draft.material);
        // Los campos que el material no usa NO se renderizan (antes se
        // mostraban bloqueados): así la modal solo enseña lo que aplica.
        if mask.cell_size {
            // Cubo de líquidos: el backend de rejilla solo añade el tamaño de
            // celda; el resto de propiedades líquidas se comparten abajo.
            slider_row(
                ui,
                locale,
                "Tamaño de celda (m)",
                "Cell size (m)",
                &mut draft.cell_size,
                ranges.cell_size,
                0.5,
            );
        } else {
            if mask.rate {
                slider_row(
                    ui,
                    locale,
                    "Emisión (part/s)",
                    "Emission (part/s)",
                    &mut draft.rate,
                    ranges.rate,
                    100.0,
                );
            }
            slider_row(
                ui,
                locale,
                "Tamaño (m)",
                "Size (m)",
                &mut draft.size,
                ranges.size,
                0.3,
            );
            slider_row(
                ui,
                locale,
                "Vida (s)",
                "Life (s)",
                &mut draft.life,
                ranges.life,
                3.0,
            );
            if mask.gravity {
                slider_row(
                    ui,
                    locale,
                    "Gravedad (m/s²)",
                    "Gravity (m/s²)",
                    &mut draft.gravity,
                    ranges.gravity,
                    -9.8,
                );
            }
            if mask.drag {
                slider_row(
                    ui,
                    locale,
                    "Arrastre",
                    "Drag",
                    &mut draft.drag,
                    ranges.drag,
                    0.5,
                );
            }
            if mask.turbulence {
                slider_row(
                    ui,
                    locale,
                    "Turbulencia",
                    "Turbulence",
                    &mut draft.turbulence,
                    ranges.turbulence,
                    0.0,
                );
            }
            if mask.cohesion && mask.color {
                // Líquidos GPU: el mismo parámetro se presenta como tensión
                // superficial (el shader lo usa como cohesión de flujo).
                slider_row(
                    ui,
                    locale,
                    "Tensión superficial",
                    "Surface tension",
                    &mut draft.cohesion,
                    ranges.cohesion,
                    0.15,
                );
            } else if mask.cohesion {
                slider_row(
                    ui,
                    locale,
                    "Cohesión",
                    "Cohesion",
                    &mut draft.cohesion,
                    ranges.cohesion,
                    0.0,
                );
            }
            if mask.ground_y {
                slider_row(
                    ui,
                    locale,
                    "Suelo (m)",
                    "Ground (m)",
                    &mut draft.ground_y,
                    ranges.ground_y,
                    0.0,
                );
            }
            if mask.wind {
                slider_row(
                    ui,
                    locale,
                    "Viento X (m/s)",
                    "Wind X (m/s)",
                    &mut draft.wind[0],
                    ranges.wind,
                    0.0,
                );
                slider_row(
                    ui,
                    locale,
                    "Viento Z (m/s)",
                    "Wind Z (m/s)",
                    &mut draft.wind[1],
                    ranges.wind,
                    0.0,
                );
            }

            if mask.ground_collision || mask.accumulate || mask.splash {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if mask.ground_collision {
                        checkbox_tr(
                            ui,
                            locale,
                            "Colisión suelo",
                            "Ground collision",
                            &mut draft.ground_collision,
                        );
                    }
                    if mask.accumulate {
                        checkbox_tr(ui, locale, "Acumular", "Accumulate", &mut draft.accumulate);
                    }
                    if mask.splash {
                        checkbox_tr(ui, locale, "Salpicar", "Splash", &mut draft.splash);
                    }
                });
            }
        }
        if mask.viscosity {
            slider_row(
                ui,
                locale,
                "Viscosidad",
                "Viscosity",
                &mut draft.viscosity,
                ranges.viscosity,
                0.15,
            );
        }
        // Propiedades compartidas por todos los líquidos, con backend de
        // rejilla (cubo) o GPU (emisor de agua). Va al final, tras los sliders.
        if mask.color {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(tr(locale, "Liquid color", "Liquid color"));
                ui.color_edit_button_rgb(&mut draft.color);
            });
        }
    }

    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let half = (ui.available_width() - ui.spacing().item_spacing.x) * 0.5;
            if !is_new {
                // Modal de propiedades: aplicar sin cerrar, o cancelar y cerrar.
                if widgets::modal_button(
                    ui,
                    icons::CHECK,
                    tr(locale, "Aplicar", "Apply"),
                    widgets::ModalButtonStyle::Primary,
                    half,
                    true,
                ) {
                    apply = true;
                }
                if widgets::modal_button(
                    ui,
                    icons::X,
                    tr(locale, "Cancelar", "Cancel"),
                    widgets::ModalButtonStyle::Secondary,
                    half,
                    true,
                ) {
                    cancel = true;
                }
            } else {
                let confirm_label = if is_3d {
                    tr(locale, "Posicionar en el mundo", "Place in world")
                } else {
                    tr(locale, "Confirmar", "Confirm")
                };
                let confirm_icon = if is_3d {
                    icons::CROSSHAIR
                } else {
                    icons::CHECK
                };
                if widgets::modal_button(
                    ui,
                    confirm_icon,
                    confirm_label,
                    widgets::ModalButtonStyle::Primary,
                    half,
                    true,
                ) {
                    confirm = true;
                }
                if widgets::modal_button(
                    ui,
                    icons::X,
                    tr(locale, "Cancelar", "Cancel"),
                    widgets::ModalButtonStyle::Secondary,
                    half,
                    true,
                ) {
                    cancel = true;
                }
            }
        });
    });
    ui.add_space(6.0);

    if apply {
        // El borrador es un clon con los valores recién editados: se guarda de
        // vuelta, se aplica al motor y la modal sigue abierta.
        app.right_sidebar.particles.modal = Some(draft.clone());
        apply_properties(app, &draft);
        return;
    }
    if cancel {
        app.right_sidebar.particles.modal = None;
        return;
    }
    if !confirm {
        // Sin acción: conserva lo escrito en los drafts para el próximo frame.
        app.right_sidebar.particles.modal = Some(draft);
        return;
    }

    // Alta nueva: id de UI asignado aquí; el click del ghost creará el elemento
    // con este id y abrirá su modal de propiedades.
    app.right_sidebar.particles.modal = None;
    draft.id = next_ui_id(&mut app.right_sidebar.particles);
    let emitter_id = draft.id;
    let material = draft.material;
    let kind_label = MATERIAL_OPTIONS
        .iter()
        .find(|(wire, _, _)| *wire == material)
        .map(|option| option.1.to_string())
        .unwrap_or_else(|| material.to_string());

    // En 2D no hay ghost de viewport: se aplica directamente, como antes.
    if !is_3d {
        apply_properties(app, &draft);
        app.right_sidebar.particles.modal = None;
        app.push_log(
            format!("[Partículas] Emisor «{kind_label}» confirmado (id {emitter_id})"),
            false,
        );
        return;
    }

    app.right_sidebar.particles.awaiting_config = true;
    app.right_sidebar.particles.pending_placement = Some(draft);
    app.push_log(
        format!(
            "[Partículas] Mueve el cursor y haz click en el mundo para colocar «{kind_label}» (id {emitter_id})…"
        ),
        false,
    );
}

/// Envía los valores actuales al motor (upsert por id) y actualiza la fila del
/// sidebar. No toca `modal`: la ventana se queda abierta para seguir editando.
fn apply_properties(app: &mut EditorApp, draft: &EmitterRow) {
    let command = app.right_sidebar.particles.emitter_command(draft);
    app.send_engine(command);
    let emitters = &mut app.right_sidebar.particles.emitters;
    match emitters.iter_mut().find(|existing| existing.id == draft.id) {
        Some(existing) => *existing = draft.clone(),
        None => emitters.push(draft.clone()),
    }
    let material = MATERIAL_OPTIONS
        .iter()
        .find(|(wire, _, _)| *wire == draft.material)
        .map(|option| option.1.to_string())
        .unwrap_or_else(|| draft.material.to_string());
    app.push_log(
        format!(
            "[Partículas] Cambios aplicados a «{material}» (id {})",
            draft.id
        ),
        false,
    );
}

/// Fila de slider: etiqueta a la izquierda, valor actual a la derecha y el
/// control ocupando el resto. El borrador se escribe con los decimales del paso.
fn slider_row(
    ui: &mut egui::Ui,
    locale: Locale,
    es: &'static str,
    en: &'static str,
    draft: &mut NumberDraft,
    range: FieldRange,
    fallback: f32,
) {
    let decimals = range.decimals();
    let parsed = draft.parsed().unwrap_or(fallback);
    let mut value = range.clamp(parsed);
    if (value - parsed).abs() > 1e-6 {
        draft.force_precise(value, decimals);
    }

    ui.horizontal(|ui| {
        ui.add_sized(
            [SLIDER_LABEL_WIDTH, 18.0],
            egui::Label::new(field_label(locale, es, en)).truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{value:.decimals$}"))
                    .size(11.5)
                    .color(theme::INFO),
            );
            ui.add_space(6.0);
            if widgets::range_slider(
                ui,
                &mut value,
                range.min,
                range.max,
                range.step,
                theme::CONTROL_BLUE,
            ) {
                draft.force_precise(value, decimals);
            }
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
