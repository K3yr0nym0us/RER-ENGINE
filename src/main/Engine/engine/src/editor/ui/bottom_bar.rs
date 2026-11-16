//! Consola inferior del editor.
//!
//! Espejo de `LogConsole` en React: fondo `#03050d`, borde superior `#202545`,
//! monospace pequeño, verde para eventos y rojo para errores, con auto-scroll.
//! Arriba lleva una franja de capas para ajustar la verbosidad en caliente.

use egui::RichText;
use rer_engine_core::logging::LogLayer;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;

/// Bootstrap `small` sobre base 14 px (0.875em).
const LOG_FONT_SIZE: f32 = 12.5;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let text = app.text();

    log_layer_strip(ui, app);

    // `border-top: 1px solid #202545` del `.console-panel`, por encima de la franja.
    let rect = ui.max_rect();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(1.0, crate::editor::theme::METRICS_BORDER),
    );

    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 7))
        .show(ui, |ui| {
            // Sin separación extra: React apila los <div> con el line-height.
            ui.spacing_mut().item_spacing.y = 0.0;

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if app.log.is_empty() {
                        ui.label(
                            RichText::new(text.empty_log)
                                .monospace()
                                .size(LOG_FONT_SIZE)
                                .color(crate::editor::theme::TEXT_MUTED),
                        );
                        return;
                    }

                    for entry in &app.log {
                        let color = if entry.is_error {
                            crate::editor::theme::DANGER
                        } else {
                            crate::editor::theme::SUCCESS
                        };
                        ui.label(
                            RichText::new(&entry.text)
                                .monospace()
                                .size(LOG_FONT_SIZE)
                                .color(color),
                        );
                    }
                });
        });
}

/// Franja superior: capas activas para la consola y el stderr del motor.
fn log_layer_strip(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let is_3d = app.cli.context.is_3d();
    egui::Frame::new()
        .fill(crate::editor::theme::ACCORDION_HEADER_BG)
        .inner_margin(egui::Margin::symmetric(14, 3))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 2.0);
                ui.label(
                    RichText::new(tr(locale, "Filtros de log", "Log filters"))
                        .size(10.0)
                        .strong()
                        .color(crate::editor::theme::TEXT_MUTED),
                );
                for layer in LogLayer::ALL {
                    if !layer_visible(is_3d, layer) {
                        continue;
                    }
                    let mut enabled = app.log_layers.contains(layer);
                    let response = ui
                        .checkbox(
                            &mut enabled,
                            RichText::new(layer_label(locale, layer))
                                .size(10.5)
                                .color(crate::editor::theme::TEXT_MUTED),
                        )
                        .on_hover_text(layer_hint(locale, layer));
                    if response.changed() {
                        app.log_layers.set(layer, enabled);
                        rer_engine_core::logging::set_layers(app.log_layers);
                    }
                }
            });
        });

    let y = ui.cursor().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        y,
        egui::Stroke::new(1.0, crate::editor::theme::METRICS_BORDER),
    );
    ui.add_space(2.0);
}

/// El check de escena corresponde solo al motor activo.
fn layer_visible(is_3d: bool, layer: LogLayer) -> bool {
    match layer {
        LogLayer::World3d => is_3d,
        LogLayer::World2d => !is_3d,
        _ => true,
    }
}

fn layer_label(locale: crate::editor::cli::Locale, layer: LogLayer) -> &'static str {
    match layer {
        LogLayer::Engine => tr(locale, "Motor", "Engine"),
        LogLayer::Editor => tr(locale, "Editor", "Editor"),
        LogLayer::Loading => tr(locale, "Carga", "Loading"),
        LogLayer::World3d => tr(locale, "Escena 3D", "3D scene"),
        LogLayer::World2d => tr(locale, "Escena 2D", "2D scene"),
        LogLayer::Physics => tr(locale, "Física", "Physics"),
        LogLayer::Worldgen => tr(locale, "Generación", "Worldgen"),
        LogLayer::Assets => tr(locale, "Recursos", "Assets"),
        LogLayer::Runtime => tr(locale, "Depuración", "Debug"),
    }
}

fn layer_hint(locale: crate::editor::cli::Locale, layer: LogLayer) -> &'static str {
    match layer {
        LogLayer::Engine => tr(
            locale,
            "Arranque, parada y errores del motor",
            "Engine start, stop and errors",
        ),
        LogLayer::Editor => tr(
            locale,
            "Escenas, entidades y acciones del editor",
            "Editor scenes, entities and actions",
        ),
        LogLayer::Loading => tr(
            locale,
            "Progreso de carga de proyecto y escena",
            "Project and scene loading progress",
        ),
        LogLayer::World3d => tr(
            locale,
            "Escena 3D: modelos, luces, sombras, reflejos y render",
            "3D scene: models, lights, shadows, reflections and render",
        ),
        LogLayer::World2d => tr(
            locale,
            "Escena 2D: sprites, fondos, cámara y dibujo",
            "2D scene: sprites, backgrounds, camera and drawing",
        ),
        LogLayer::Physics => tr(
            locale,
            "Cuerpos, colisiones y triggers",
            "Bodies, collisions and triggers",
        ),
        LogLayer::Worldgen => tr(
            locale,
            "Generación de mundo y plugin worldgen",
            "World generation and worldgen plugin",
        ),
        LogLayer::Assets => tr(
            locale,
            "Recursos cargados y listas internas (ruidoso)",
            "Loaded assets and internal lists (noisy)",
        ),
        LogLayer::Runtime => tr(
            locale,
            "Depuración: métricas, selección, play y eventos internos (ruidoso)",
            "Debug: metrics, selection, play and internal events (noisy)",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_engine_shows_only_its_scene_layer() {
        assert!(layer_visible(true, LogLayer::World3d));
        assert!(!layer_visible(true, LogLayer::World2d));
        assert!(layer_visible(false, LogLayer::World2d));
        assert!(!layer_visible(false, LogLayer::World3d));
        assert!(layer_visible(true, LogLayer::Engine));
        assert!(layer_visible(false, LogLayer::Editor));
    }
}
