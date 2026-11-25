//! Contenido de la ventana nativa del catálogo de plugins.
//!
//! Réplica del `PluginsModalBody` de React: tarjetas cuadradas de 200 px con
//! icono 88 px, badge de versión y tamaño, acciones de instalación con barra de
//! progreso, tarjetas «Coming soon» y panel de modelo/conexión del asistente.

use egui::{Align, Color32, Layout, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::plugins::{self, CATALOG, DownloadProgress, PluginCatalogEntry};
use crate::editor::theme;

const CARD_WIDTH: f32 = 200.0;
const CARD_INSTALL_WIDTH: f32 = 220.0;
const MIN_CARD_HEIGHT: f32 = 200.0;
const ICON_SIZE: f32 = 88.0;
const BLUE: Color32 = Color32::from_rgb(0x0d, 0x6e, 0xfd);
const GREEN: Color32 = Color32::from_rgb(0x19, 0x87, 0x54);
const RED: Color32 = Color32::from_rgb(0xdc, 0x35, 0x45);
const OUTLINE: Color32 = Color32::from_rgb(0x6c, 0x75, 0x7d);
const BADGE_BG: Color32 = Color32::from_rgb(0x6c, 0x75, 0x7d);
const DANGER_BG: Color32 = Color32::from_rgb(0x84, 0x1c, 0x24);
const WARNING_BG: Color32 = Color32::from_rgb(0xff, 0xf3, 0xcd);
const WARNING_TEXT: Color32 = Color32::from_rgb(0x66, 0x4d, 0x00);

#[derive(Clone, Copy)]
enum ButtonKind {
    Primary,
    Success,
    OutlineDanger,
    OutlineSecondary,
}

fn styled_button(
    ui: &mut egui::Ui,
    label: &str,
    kind: ButtonKind,
    width: f32,
    enabled: bool,
) -> bool {
    let old = ui.style().as_ref().clone();
    {
        let style = ui.style_mut();
        let inactive = &mut style.visuals.widgets.inactive;
        let hovered = &mut style.visuals.widgets.hovered;
        let active = &mut style.visuals.widgets.active;
        let (fill, hover, text, stroke) = match kind {
            ButtonKind::Primary => (BLUE, BLUE, Color32::WHITE, BLUE),
            ButtonKind::Success => (GREEN, GREEN, Color32::WHITE, GREEN),
            ButtonKind::OutlineDanger => (
                Color32::TRANSPARENT,
                Color32::from_rgba_premultiplied(220, 53, 69, 30),
                RED,
                RED,
            ),
            ButtonKind::OutlineSecondary => (
                Color32::TRANSPARENT,
                Color32::from_rgba_premultiplied(108, 117, 125, 30),
                Color32::from_rgb(0xde, 0xe2, 0xf0),
                OUTLINE,
            ),
        };
        for state in [inactive, hovered, active] {
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.bg_stroke = egui::Stroke::new(1.0, stroke);
            state.fg_stroke.color = text;
            state.corner_radius = egui::CornerRadius::same(4);
        }
        hovered.weak_bg_fill = hover;
        hovered.bg_fill = hover;
        active.weak_bg_fill = hover;
        active.bg_fill = hover;
    }
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).size(12.0).strong())
            .min_size(egui::vec2(width, 26.0)),
    );
    *ui.style_mut() = old;
    response.clicked()
}

fn badge(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(BADGE_BG)
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(Color32::WHITE));
        });
}

/// Borde punteado de `.plugin-square-card--soon` (Bootstrap `border dashed`).
/// egui no tiene trazo discontinuo en `Stroke`, así que se pinta a mano.
fn paint_dashed_border(ui: &egui::Ui, rect: egui::Rect, color: Color32) {
    let path = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    for shape in egui::Shape::dashed_line(&path, egui::Stroke::new(1.0, color), 5.0, 4.0) {
        ui.painter().add(shape);
    }
}

/// Espejo de `pluginInstallStatusLabel` del renderer, por fase de descarga.
fn install_status_label(progress: Option<&DownloadProgress>, locale: Locale) -> String {
    match progress.map(|item| item.phase.as_str()) {
        Some("opencode") => {
            tr(locale, "Descargando OpenCode…", "Downloading OpenCode…").to_string()
        }
        Some("extracting") => tr(locale, "Instalando…", "Installing…").to_string(),
        Some("preparing") => tr(
            locale,
            "Preparando el espacio de trabajo…",
            "Preparing assistant workspace…",
        )
        .to_string(),
        Some("artifact") => tr(locale, "Descargando…", "Downloading…").to_string(),
        _ => tr(locale, "Preparando la descarga…", "Preparing download…").to_string(),
    }
}

fn load_assistant_icon(ui: &mut egui::Ui, app: &mut EditorApp) {
    if app.plugins_modal.icon.is_some() {
        return;
    }
    let bytes = include_bytes!("../../../../../../resources/RER-AI.png");
    if let Ok(image) = image::load_from_memory(bytes) {
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            rgba.as_raw(),
        );
        let texture = ui
            .ctx()
            .load_texture("rer_ai_icon", color, egui::TextureOptions::LINEAR);
        app.plugins_modal.icon = Some(texture);
    }
}

fn is_installing(app: &EditorApp, id: &str) -> bool {
    app.plugins_modal.busy_id.as_deref() == Some(id)
        || (app.plugins_modal.llm.status == "downloading" && id == "local-ai-assistant")
}

fn plugin_card(ui: &mut egui::Ui, app: &mut EditorApp, entry: &PluginCatalogEntry) {
    let locale = app.cli.locale;
    let id = entry.id;
    let installed = app.plugins_modal.plugins.is_installed(id);
    let enabled = app.plugins_modal.plugins.is_enabled(id);
    let installing = is_installing(app, id);
    let install_available = crate::editor::plugin_runtime::plugin_install_available(id);
    let card_width = if installing {
        CARD_INSTALL_WIDTH
    } else {
        CARD_WIDTH
    };

    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(6)
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_width(card_width - 26.0);
            // CSS: `.plugin-square-card { min-height: 200px }`. El Frame suma
            // 8 px arriba + 12 abajo, así que el contenido necesita 180 px.
            ui.set_min_height(MIN_CARD_HEIGHT - 20.0);
            ui.vertical_centered(|ui| {
                if id == "world-gen" {
                    ui.label(
                        RichText::new(icons::GLOBE)
                            .size(ICON_SIZE)
                            .color(theme::INFO),
                    );
                } else if let Some(texture) = app.plugins_modal.icon.clone() {
                    ui.image((texture.id(), egui::vec2(ICON_SIZE, ICON_SIZE)));
                } else {
                    ui.label(
                        RichText::new(icons::ROBOT)
                            .size(ICON_SIZE)
                            .color(theme::INFO),
                    );
                }
                ui.label(
                    RichText::new(plugin_title(entry, locale))
                        .size(13.6)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.horizontal(|ui| {
                    badge(ui, &format!("v{}", entry.version));
                    ui.label(
                        RichText::new(
                            entry
                                .size_label
                                .split(" (")
                                .next()
                                .unwrap_or(entry.size_label),
                        )
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                    );
                });
            });

            ui.add_space(8.0);
            if installing {
                let progress = app
                    .plugins_modal
                    .progress
                    .as_ref()
                    .filter(|item| item.plugin_id == id)
                    .cloned();
                let indeterminate = progress
                    .as_ref()
                    .is_some_and(|item| item.phase == "extracting" || item.phase == "preparing");
                let percent = progress.as_ref().map(|p| p.overall_percent).unwrap_or(0);
                // React: `.plugin-install-progress` coloca la fase a la
                // izquierda y los bytes a la derecha en la misma fila sobre la
                // barra; el porcentaje se escribe dentro de la barra.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(install_status_label(progress.as_ref(), locale))
                            .size(12.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(progress) = &progress {
                            ui.label(
                                RichText::new(format!(
                                    "{} / {}",
                                    format_bytes(progress.bytes_received),
                                    format_bytes(progress.bytes_total)
                                ))
                                .size(10.5)
                                .color(theme::TEXT_MUTED),
                            );
                        }
                    });
                });
                let bar = ui.available_width();
                let bar_text = if indeterminate {
                    "…".to_string()
                } else {
                    format!("{percent}%")
                };
                ui.add(
                    egui::ProgressBar::new(if indeterminate {
                        0.35
                    } else {
                        percent as f32 / 100.0
                    })
                    .desired_width(bar)
                    .text(
                        RichText::new(bar_text)
                            .size(10.4)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .animate(indeterminate),
                );
                ui.add_space(8.0);
                if styled_button(
                    ui,
                    tr(locale, "Cancel", "Cancel"),
                    ButtonKind::OutlineSecondary,
                    ui.available_width(),
                    !app.plugins_modal.cancelling,
                ) {
                    plugins::action(app, "cancel", json!({}));
                }
            } else {
                ui.vertical(|ui| {
                    let width = ui.available_width();
                    if !installed {
                        if install_available {
                            if styled_button(
                                ui,
                                tr(locale, "Instalar", "Install"),
                                ButtonKind::Primary,
                                width,
                                true,
                            ) {
                                app.plugins_modal.error = None;
                                app.plugins_modal.cancelling = false;
                                app.plugins_modal.progress = None;
                                app.plugins_modal.busy_id = Some(id.to_string());
                                plugins::action(app, "install", json!({ "plugin_id": id }));
                            }
                        } else {
                            let pending =
                                tr(locale, "Pendiente de publicación", "Publication pending");
                            let _ = styled_button(
                                ui,
                                pending,
                                ButtonKind::OutlineSecondary,
                                width,
                                false,
                            );
                            ui.label(
                                RichText::new(tr(
                                    locale,
                                    "El artefacto Linux aún no está publicado.",
                                    "The Linux artifact is not published yet.",
                                ))
                                .size(10.5)
                                .color(theme::TEXT_MUTED),
                            );
                        }
                    } else {
                        ui.vertical_centered(|ui| {
                            if id != "world-gen" {
                                // React: `.form-check.form-switch` (pastilla + label).
                                let mut enabled_value = enabled;
                                let label = tr(locale, "Enabled", "Enabled");
                                super::widgets::top_row(ui, |ui| {
                                    if super::widgets::switch_value(ui, &mut enabled_value, label) {
                                        plugins::action(
                                            app,
                                            "set_enabled",
                                            json!({ "plugin_id": id, "enabled": enabled_value }),
                                        );
                                    }
                                    ui.label(RichText::new(label).size(12.0).color(theme::TEXT));
                                });
                            }
                            if id == "world-gen"
                                && styled_button(
                                    ui,
                                    tr(locale, "Ejecutar plugin", "Run plugin"),
                                    ButtonKind::Success,
                                    width,
                                    true,
                                )
                            {
                                // El generador es nativo: se abre con la ruta de la DLL
                                // que el propio editor lee del state.json. Sustituye
                                // a la modal de plugins (como el `openWorldGenPluginModal`
                                // original): si la dejamos abierta, al cerrar WorldGen
                                // vuelve a aparecer Plugins porque `sync_modal_window`
                                // prioriza WorldGen pero no limpia el flag anterior.
                                app.plugins_modal.open = false;
                                app.worldgen.library_path =
                                    crate::editor::plugin_runtime::worldgen_library_path();
                                app.worldgen.open = true;
                            }
                            let uninstall_label = if id == "world-gen" {
                                tr(locale, "Eliminar plugin", "Delete plugin")
                            } else {
                                tr(locale, "Desinstalar", "Uninstall")
                            };
                            if styled_button(
                                ui,
                                uninstall_label,
                                ButtonKind::OutlineDanger,
                                width,
                                true,
                            ) {
                                app.plugins_modal.error = None;
                                app.plugins_modal.busy_id = Some(id.to_string());
                                plugins::action(app, "uninstall", json!({ "plugin_id": id }));
                            }
                        });
                    }
                });
            }
        });
}

fn plugin_title(entry: &PluginCatalogEntry, locale: Locale) -> String {
    if entry.id == "local-ai-assistant" {
        tr(locale, "Asistente local RER-AI", "RER-AI local assistant").to_string()
    } else {
        entry.name.to_string()
    }
}

/// Espejo de `formatDownloadBytes` del renderer: — / GB / MB / KB.
fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "—".to_string();
    }
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else if mb >= 1.0 {
        format!("{:.1} MB", mb)
    } else {
        format!("{} KB", ((bytes as f64 / 1024.0).round() as u64).max(1))
    }
}

fn coming_soon_card(ui: &mut egui::Ui, locale: Locale, title: Option<&str>) {
    // React `.plugin-square-card--soon`: opacidad 0.72 y borde dashed.
    let response = egui::Frame::new()
        .corner_radius(6)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(CARD_WIDTH - 26.0);
            ui.set_min_height(MIN_CARD_HEIGHT - 24.0);
            ui.set_opacity(0.72);
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label(
                    RichText::new(icons::HOURGLASS)
                        .size(if title.is_some() { 40.0 } else { 44.0 })
                        .color(theme::TEXT_MUTED),
                );
                if let Some(title) = title {
                    ui.label(RichText::new(title).size(13.6).strong().color(theme::TEXT));
                }
                ui.add_space(4.0);
                badge(ui, tr(locale, "Muy pronto", "Coming soon"));
                ui.add_space(16.0);
            });
        });
    paint_dashed_border(
        ui,
        response.response.rect,
        Color32::from_rgba_unmultiplied(0x6c, 0x75, 0x7d, 184),
    );
}

fn assistant_panel(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    if !app.plugins_modal.llm.server_running() {
        return;
    }
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr(locale, "Modelo y conexión", "Model and connection"))
                    .size(13.6)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr(locale, "Modelo", "Model"))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
            });
            let selected = app
                .plugins_modal
                .settings
                .selected_model
                .clone()
                .unwrap_or_default();
            let current_name = if selected.is_empty() {
                tr(
                    locale,
                    "Automático (primer modelo gratuito)",
                    "Automatic (first free model)",
                )
                .to_string()
            } else {
                app.plugins_modal
                    .models
                    .iter()
                    .find(|model| model.reference == selected)
                    .map(|model| model.name.clone())
                    .unwrap_or_else(|| selected.clone())
            };
            egui::ComboBox::from_id_salt("plugins_assistant_model")
                .selected_text(current_name)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    let mut value = selected.clone();
                    ui.selectable_value(
                        &mut value,
                        String::new(),
                        tr(
                            locale,
                            "Automático (primer modelo gratuito)",
                            "Automatic (first free model)",
                        )
                        .to_string(),
                    );
                    for (group, label) in [
                        (true, tr(locale, "Modelos gratuitos", "Free models")),
                        (
                            false,
                            tr(locale, "Modelos OpenCode Go", "OpenCode Go models"),
                        ),
                    ] {
                        let models: Vec<_> = app
                            .plugins_modal
                            .models
                            .iter()
                            .filter(|model| model.free == group)
                            .collect();
                        if models.is_empty() {
                            continue;
                        }
                        ui.label(
                            RichText::new(label)
                                .size(11.0)
                                .strong()
                                .color(theme::TEXT_MUTED),
                        );
                        for model in models {
                            ui.selectable_value(&mut value, model.reference.clone(), &model.name);
                        }
                    }
                    if value != selected {
                        app.plugins_modal.settings.selected_model = if value.is_empty() {
                            None
                        } else {
                            Some(value.clone())
                        };
                        plugins::action(app, "set_model", json!({ "model": value }));
                    }
                });
            ui.add_space(10.0);

            if app.plugins_modal.settings.go_connected {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("OpenCode Go")
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    badge(ui, tr(locale, "Conectado", "Connected"));
                    if styled_button(
                        ui,
                        tr(locale, "Desconectar", "Disconnect"),
                        ButtonKind::OutlineSecondary,
                        110.0,
                        !app.plugins_modal.assistant_busy,
                    ) {
                        app.plugins_modal.assistant_busy = true;
                        plugins::action(app, "disconnect_go", json!({}));
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr(locale, "Token de OpenCode Go", "OpenCode Go token"))
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    let token = ui.add(
                        egui::TextEdit::singleline(&mut app.plugins_modal.token)
                            .password(true)
                            .hint_text(tr(
                                locale,
                                "Pega tu clave API de OpenCode Go",
                                "Paste your OpenCode Go API key",
                            ))
                            .desired_width(ui.available_width() - 90.0),
                    );
                    let _ = token;
                    if styled_button(
                        ui,
                        tr(locale, "Conectar", "Connect"),
                        ButtonKind::Primary,
                        80.0,
                        !app.plugins_modal.assistant_busy
                            && app.plugins_modal.token.trim().len() >= 8,
                    ) {
                        app.plugins_modal.assistant_busy = true;
                        plugins::action(
                            app,
                            "connect_go",
                            json!({ "api_key": app.plugins_modal.token.trim() }),
                        );
                    }
                });
            }

            ui.add_space(6.0);
            ui.label(
                RichText::new(tr(
                    locale,
                    "El token se guarda en el perfil aislado de OpenCode, nunca en RER-ENGINE.",
                    "The token is stored in the isolated OpenCode profile, never in RER-ENGINE.",
                ))
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );

            if !app.plugins_modal.models.iter().any(|model| model.free) {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr(
                        locale,
                        "No hay modelos gratuitos. Conecta tu cuenta de OpenCode.",
                        "No free models available. Connect your OpenCode account.",
                    ))
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
                );
                if styled_button(
                    ui,
                    tr(
                        locale,
                        "Conectar cuenta OpenCode",
                        "Connect OpenCode account",
                    ),
                    ButtonKind::OutlineSecondary,
                    200.0,
                    !app.plugins_modal.assistant_busy,
                ) {
                    plugins::action(app, "start_login", json!({}));
                }
            }

            if let Some(error) = &app.plugins_modal.models_error {
                ui.label(RichText::new(error).size(11.0).color(theme::WARNING));
            }
            if let Some(error) = &app.plugins_modal.assistant_error {
                ui.label(RichText::new(error).size(11.0).color(theme::DANGER));
            }
        });
}

/// Dibuja el cuerpo en la ventana modal externa.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    load_assistant_icon(ui, app);

    // El panel del asistente se hidrata cuando el servidor está encendido.
    if app.plugins_modal.llm.server_running() && !app.plugins_modal.assistant_bootstrap_requested {
        app.plugins_modal.assistant_bootstrap_requested = true;
        plugins::action(app, "assistant_bootstrap", json!({}));
    }

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("plugins_scroll")
                .show(ui, |ui| {
                    if let Some(error) = &app.plugins_modal.error {
                        alert(ui, DANGER_BG, Color32::WHITE, error);
                        ui.add_space(8.0);
                    }
                    if let Some(error) = &app.plugins_modal.llm.error {
                        alert(ui, WARNING_BG, WARNING_TEXT, error);
                        ui.add_space(8.0);
                    }

                    // React: `.plugins-modal-grid` = flex + space-evenly con
                    // tarjetas de 200 px. `horizontal()` centra cada hijo en un
                    // row que crece hacia abajo (efecto escalera); con
                    // `horizontal_top` todas comparten la misma línea base.
                    super::widgets::top_row(ui, |ui| {
                        let gap = 8.0;
                        let widths: Vec<f32> = CATALOG
                            .iter()
                            .map(|entry| {
                                if is_installing(app, entry.id) {
                                    CARD_INSTALL_WIDTH
                                } else {
                                    CARD_WIDTH
                                }
                            })
                            .chain([CARD_WIDTH, CARD_WIDTH])
                            .collect();
                        let row_width: f32 =
                            widths.iter().sum::<f32>() + gap * (widths.len() - 1) as f32;
                        let leading = (ui.available_width() - row_width) * 0.5;
                        if leading > 0.0 {
                            ui.add_space(leading);
                        }
                        let mut widths = widths.into_iter();
                        for (index, entry) in CATALOG.iter().enumerate() {
                            if index > 0 {
                                ui.add_space(gap);
                            }
                            let width = widths.next().unwrap_or(CARD_WIDTH);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 0.0),
                                Layout::top_down(Align::Min),
                                |ui| plugin_card(ui, app, entry),
                            );
                        }
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            egui::vec2(widths.next().unwrap_or(CARD_WIDTH), 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                coming_soon_card(
                                    ui,
                                    locale,
                                    Some(tr(
                                        locale,
                                        "Generación de fotogramas 2D con IA",
                                        "2D frame generation AI",
                                    )),
                                )
                            },
                        );
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            egui::vec2(widths.next().unwrap_or(CARD_WIDTH), 0.0),
                            Layout::top_down(Align::Min),
                            |ui| coming_soon_card(ui, locale, None),
                        );
                    });

                    if app.plugins_modal.has_assistant() {
                        ui.add_space(12.0);
                        assistant_panel(ui, app);
                    }
                });
        });
}

fn alert(ui: &mut egui::Ui, background: Color32, text: Color32, message: &str) {
    egui::Frame::new()
        .fill(background)
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.label(RichText::new(message).size(12.0).color(text));
        });
}
