//! Consola inferior del editor.
//!
//! Espejo de `LogConsole` en React: fondo `#03050d`, borde superior `#202545`,
//! monospace pequeño, verde para eventos y rojo para errores, con auto-scroll.
//! Arriba lleva una franja de capas para ajustar la verbosidad en caliente.

use egui::{Align, Layout, Rect, RichText, TextStyle, WidgetText};
use rer_engine_core::logging::LogLayer;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;

/// Bootstrap `small` sobre base 14 px (0.875em).
const LOG_FONT_SIZE: f32 = 12.5;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let text = app.text();

    // El `Frame` del Panel pinta su fondo solo hasta donde llega el contenido
    // (`min_rect`). Forzamos esa altura al rect completo para que el LOG_BG
    // cubra todo el panel aunque haya pocas líneas de log.
    let panel_height = ui.max_rect().height();
    ui.set_min_height(panel_height);

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
                    // Altura visible del panel: el `TextEdit` la usa como mínimo para
                    // que la consola llene el alto al arrastrar el divisor.
                    let fill_height = ui.available_height();
                    if app.log.is_empty() {
                        ui.label(
                            RichText::new(text.empty_log)
                                .monospace()
                                .size(LOG_FONT_SIZE)
                                .color(crate::editor::theme::TEXT_MUTED),
                        );
                        return;
                    }

                    // Un único `TextEdit` de solo lectura: egui no copia bien
                    // selecciones que cruzan varios `Label` independientes.
                    // `desired_width` finito = wrap al ancho real de la consola
                    // (INFINITY desactiva el wrap y corta las líneas largas).
                    let full = console_text(app);
                    let mut view: &str = &full;
                    let mut layouter =
                        |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                            let font = egui::TextStyle::Monospace.resolve(ui.style());
                            let mut job = egui::text::LayoutJob::default();
                            job.wrap.max_width = wrap_width;
                            // Las líneas muy largas sin espacios también deben partirse.
                            job.wrap.break_anywhere = true;
                            for line in buf.as_str().split_inclusive('\n') {
                                let color = if line.starts_with("[ERROR] ") {
                                    crate::editor::theme::DANGER
                                } else {
                                    crate::editor::theme::SUCCESS
                                };
                                job.append(
                                    line,
                                    0.0,
                                    egui::TextFormat {
                                        font_id: font.clone(),
                                        color,
                                        ..Default::default()
                                    },
                                );
                            }
                            ui.fonts_mut(|fonts| fonts.layout_job(job))
                        };
                    ui.add(
                        egui::TextEdit::multiline(&mut view)
                            .id_salt("editor_log_text")
                            .font(egui::TextStyle::Monospace)
                            .text_color(crate::editor::theme::SUCCESS)
                            .frame(egui::Frame::NONE)
                            .background_color(egui::Color32::TRANSPARENT)
                            .desired_width(ui.available_width())
                            .desired_rows(app.log.len().min(200))
                            .min_size(egui::vec2(ui.available_width(), fill_height))
                            .layouter(&mut layouter),
                    );
                });
        });

    // Botón flotante de copia: solo icono y borde, sobre los logs a la derecha.
    {
        let size = 26.0;
        let top = rect.top() + ui.spacing().interact_size.y + 14.0;
        let button_rect = Rect::from_min_size(
            egui::pos2(rect.right() - 16.0 - size, top),
            egui::vec2(size, size),
        );
        let response = ui.put(
            button_rect,
            egui::Button::new(RichText::new(egui_phosphor::regular::COPY).size(14.0)),
        );
        let clicked = response.clicked();
        // egui 0.36 trae `visuals.interact_cursor = None`: los botones no ponen
        // el cursor de mano por sí solos, hay que pedirlo explícitamente.
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(tr(app.cli.locale, "Copiar logs", "Copy logs"));
        if clicked {
            ui.ctx().copy_text(console_text(app));
        }
    }
}

/// Texto completo de la consola (una línea por entrada, errores prefijados).
fn console_text(app: &EditorApp) -> String {
    app.log_as_text()
}

/// Franja superior: capas activas para la consola y el stderr del motor.
///
/// La franja se reparte en tres zonas dentro de todo el ancho disponible:
/// el título a la izquierda, los filtros centrados y el check de métricas
/// pegado a la derecha.
fn log_layer_strip(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let is_3d = app.cli.context.is_3d();

    let title = tr(locale, "Filtros de log", "Log filters");
    let metrics = tr(locale, "Métricas de uso", "Usage metrics");
    // `.strong()` no funciona aquí: el tema registra la negrita como familia
    // propia (`rer-bold`), no como estilo Bold de la familia proporcional.
    let title_font = bold_font(12.0);
    let layers: Vec<LogLayer> = LogLayer::ALL
        .iter()
        .copied()
        .filter(|layer| layer_visible(is_3d, *layer))
        .collect();

    egui::Frame::new()
        .fill(crate::editor::theme::ACCORDION_HEADER_BG)
        .inner_margin(egui::Margin::symmetric(14, 3))
        .show(ui, |ui| {
            const CHECK_SPACING: f32 = 10.0;

            let strip = Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
            );

            // Medimos cada zona para repartir el ancho sin recortar textos.
            let title_w = text_width(
                ui,
                WidgetText::from(RichText::new(title).font(title_font.clone())),
                TextStyle::Body,
            ) + 8.0;
            let metrics_w = text_width(
                ui,
                WidgetText::from(RichText::new(metrics).size(10.5)),
                TextStyle::Body,
            ) + ui.spacing().icon_width + 8.0;
            let checks_w: f32 = layers
                .iter()
                .map(|layer| {
                    text_width(
                        ui,
                        WidgetText::from(RichText::new(layer_label(locale, *layer)).size(10.5)),
                        TextStyle::Body,
                    ) + ui.spacing().icon_width
                })
                .sum::<f32>()
                + (layers.len().saturating_sub(1) as f32) * CHECK_SPACING;

            let title_w = title_w.min(strip.width() * 0.3);
            let metrics_w = metrics_w.min(strip.width() * 0.3);
            let title_rect = Rect::from_min_size(strip.min, egui::vec2(title_w, strip.height()));
            let metrics_rect = Rect::from_min_size(
                egui::pos2(strip.right() - metrics_w, strip.top()),
                egui::vec2(metrics_w, strip.height()),
            );
            let checks_rect = Rect::from_min_size(
                egui::pos2(title_rect.right(), strip.top()),
                egui::vec2(
                    (metrics_rect.left() - title_rect.right()).max(0.0),
                    strip.height(),
                ),
            );

            // Checks centrados en el espacio sobrante (con wrap en ventanas estrechas).
            let mut checks_child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(checks_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            checks_child.spacing_mut().item_spacing = egui::vec2(CHECK_SPACING, 2.0);
            if checks_w < checks_rect.width() {
                // Centramos el grupo respecto a toda la franja, sin invadir el botón.
                let desired_left = strip.center().x - checks_w * 0.5;
                let space =
                    (desired_left - checks_rect.left()).clamp(0.0, checks_rect.width() - checks_w);
                checks_child.add_space(space);
            }
            checks_child.horizontal_wrapped(|ui| {
                for &layer in &layers {
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
            let content_h = checks_child.min_rect().height().max(strip.height());
            drop(checks_child);

            // Título a la izquierda, con fuente algo mayor y en negrita.
            {
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(Rect::from_min_size(
                            title_rect.min,
                            egui::vec2(title_rect.width(), content_h),
                        ))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                child.label(
                    RichText::new(title)
                        .font(title_font)
                        .color(crate::editor::theme::TEXT_MUTED),
                );
            }

            // Check de métricas en el extremo derecho (donde estaba el botón de copia).
            {
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(Rect::from_min_size(
                            metrics_rect.min,
                            egui::vec2(metrics_rect.width(), content_h),
                        ))
                        .layout(Layout::right_to_left(Align::Center)),
                );
                let mut show_metrics = app.show_metrics;
                let response = child
                    .checkbox(
                        &mut show_metrics,
                        RichText::new(metrics)
                            .size(10.5)
                            .color(crate::editor::theme::TEXT_MUTED),
                    )
                    .on_hover_text(tr(
                        locale,
                        "Métricas de uso sobre el viewport del motor",
                        "Usage metrics over the engine viewport",
                    ));
                if response.changed() {
                    app.show_metrics = show_metrics;
                }
            }

            // El fondo de la franja cubre todo el ancho y toda la altura usada.
            ui.allocate_rect(
                Rect::from_min_size(strip.min, egui::vec2(strip.width(), content_h)),
                egui::Sense::hover(),
            );
        });

    let y = ui.cursor().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        y,
        egui::Stroke::new(1.0, crate::editor::theme::METRICS_BORDER),
    );
    ui.add_space(2.0);
}

/// Fuente en negrita del tema (registrada como familia propia en `theme::apply`).
fn bold_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("rer-bold".into()))
}

/// Ancho de un texto con la fuente indicada, sin envolver.
fn text_width(ui: &egui::Ui, text: WidgetText, fallback: TextStyle) -> f32 {
    text.into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        fallback,
    )
    .size()
    .x
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
        LogLayer::Vulkan => tr(locale, "Vulkan", "Vulkan"),
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
        LogLayer::Vulkan => tr(
            locale,
            "Vulkan: validación, dispositivo, pipelines y asignaciones (ruidoso)",
            "Vulkan: validation, device, pipelines and allocations (noisy)",
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

    #[test]
    fn log_text_is_one_line_per_entry_with_error_prefix() {
        use crate::editor::app::LogEntry;
        let entries = [
            LogEntry {
                text: "listo".into(),
                is_error: false,
            },
            LogEntry {
                text: "fallo".into(),
                is_error: true,
            },
        ];
        let format = crate::editor::app::format_log_entries;
        assert_eq!(format(&entries), "listo\n[ERROR] fallo\n");
        assert_eq!(format(&[]), "");
    }

    #[test]
    fn long_lines_are_kept_whole_for_wrapping() {
        use crate::editor::app::LogEntry;
        let long = "x".repeat(4000);
        let entries = [LogEntry {
            text: long.clone(),
            is_error: false,
        }];
        let out = crate::editor::app::format_log_entries(&entries);
        assert!(
            out.contains(&long),
            "el log debe conservar la línea completa"
        );
        assert_eq!(out, format!("{long}\n"));
    }
}
