//! Contenido de la ventana nativa del generador de mundo.
//!
//! Réplica del `WorldGenModalBody` de React: configuración de terreno (tamaño,
//! elementos y biomas), distribución de blueprints por modelo y acciones de
//! generación. Los comandos `world_gen_*` los envía el propio editor.

use egui::{Color32, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::EditorApp;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::worldgen;

use super::{accordion, widgets};

const DANGER_BG: Color32 = Color32::from_rgb(0x84, 0x1c, 0x24);
const DANGER_TEXT: Color32 = Color32::from_rgb(0xff, 0xd8, 0xda);
/// `h-100`: los dos fieldsets de la fila comparten la altura del más alto.
const FIELDSET_PAIR_MIN_HEIGHT: f32 = 244.0;
/// Los dos paneles de la fila (terreno / blueprints) comparten altura.
const SIDE_PANEL_MIN_HEIGHT: f32 = 520.0;

// Colores Bootstrap del `WorldGenModalBody` original (root font 14 px).
const BTN_PRIMARY: Color32 = Color32::from_rgb(0x0d, 0x6e, 0xfd);
const BTN_PRIMARY_HOVER: Color32 = Color32::from_rgb(0x0b, 0x5e, 0xd7);
const BTN_SECONDARY: Color32 = Color32::from_rgb(0x6c, 0x75, 0x7d);
const BTN_DANGER: Color32 = Color32::from_rgb(0xdc, 0x35, 0x45);
const BTN_INFO: Color32 = Color32::from_rgb(0x0d, 0xca, 0xf0);

#[derive(Clone, Copy)]
enum Btn {
    /// `.btn-primary` — Generar mundo.
    Primary,
    /// `.btn-outline-secondary` — Detener.
    OutlineSecondary,
    /// `.btn-outline-danger` — Limpiar.
    OutlineDanger,
    /// `.btn-outline-info` — Añadir modelo.
    OutlineInfo,
}

/// Spinner circular tipo Bootstrap (`spinner-border-sm`), animado con el
/// tiempo de la app. Se usa en lugar del icono de play mientras se genera.
fn draw_spinner(ui: &egui::Ui, center: egui::Pos2, radius: f32, color: Color32) {
    // Sin repeticiones el spinner se quedaría quieto: pedimos repintado.
    ui.ctx().request_repaint();
    let time = ui.ctx().input(|input| input.time) as f32;
    let angle = time * 5.0;
    let painter = ui.painter();
    painter.circle(
        center,
        radius,
        Color32::TRANSPARENT,
        egui::Stroke::new(2.0, color.linear_multiply(0.3)),
    );
    let sweep = std::f32::consts::FRAC_PI_2 * 1.4;
    let steps = 14;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|index| {
            let a = angle + sweep * (index as f32 / steps as f32);
            center + egui::vec2(a.cos() * radius, a.sin() * radius)
        })
        .collect();
    painter.line(points, egui::Stroke::new(2.0, color));
}

/// Botones del modal original. Se pintan aquí para no cambiar el estilo
/// `Primary` verde que `widgets::modal_button` usa en las demás modales egui.
fn bootstrap_button(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    kind: Btn,
    width: f32,
    enabled: bool,
    spinner: bool,
) -> bool {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), sense);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hovered = enabled && response.hovered();
    let alpha = if enabled { 1.0 } else { 0.5 };

    let (fill, hover_fill, stroke, text) = match kind {
        Btn::Primary => (BTN_PRIMARY, BTN_PRIMARY_HOVER, BTN_PRIMARY, Color32::WHITE),
        Btn::OutlineSecondary => (
            Color32::TRANSPARENT,
            BTN_SECONDARY,
            BTN_SECONDARY,
            BTN_SECONDARY,
        ),
        Btn::OutlineDanger => (Color32::TRANSPARENT, BTN_DANGER, BTN_DANGER, BTN_DANGER),
        Btn::OutlineInfo => (Color32::TRANSPARENT, BTN_INFO, BTN_INFO, BTN_INFO),
    };
    // Bootstrap: los outline se rellenan y pasan a texto blanco en hover.
    let (fill, text) = if hovered {
        (hover_fill, Color32::WHITE)
    } else {
        (fill, text)
    };

    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(4),
        fill.linear_multiply(alpha),
        egui::Stroke::new(1.0, stroke.linear_multiply(alpha)),
        egui::StrokeKind::Inside,
    );
    let font = egui::FontId::proportional(13.0);
    let text_color = text.linear_multiply(alpha);
    if spinner {
        // Igual que el React: `spinner-border spinner-border-sm me-1` antes
        // del texto mientras se escanean columnas / se generan chunks.
        let radius = 6.5;
        let gap = 6.0;
        let text_width = painter
            .layout_no_wrap(label.to_owned(), font.clone(), text_color)
            .size()
            .x;
        let start_x = rect.center().x - (radius * 2.0 + gap + text_width) * 0.5;
        draw_spinner(
            ui,
            egui::pos2(start_x + radius, rect.center().y),
            radius,
            text_color,
        );
        painter.text(
            egui::pos2(
                start_x + radius * 2.0 + gap + text_width * 0.5,
                rect.center().y,
            ),
            egui::Align2::CENTER_CENTER,
            label,
            font,
            text_color,
        );
    } else {
        let content = if icon.is_empty() {
            label.to_owned()
        } else {
            format!("{icon}  {label}")
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            content,
            font,
            text_color,
        );
    }

    enabled && response.clicked()
}

/// Barra de progreso con porcentaje y contador para las dos fases de la
/// generación: escaneo de columnas y creación de chunks. El motor emite
/// `world_gen_progress` en ambas, así que siempre hay algo que mostrar.
fn generation_progress(
    ui: &mut egui::Ui,
    locale: crate::editor::cli::Locale,
    generation: worldgen::Generation,
) {
    let (label, done, total) = match generation.phase {
        worldgen::GenerationPhase::Scan => (
            t(locale, "Revisando columnas…", "Scanning columns…"),
            generation.completed,
            generation.total,
        ),
        worldgen::GenerationPhase::Mesh => (
            t(locale, "Generando mundo…", "Generating world…"),
            generation.completed,
            generation.total,
        ),
    };
    let ratio = if total > 0 {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let detail = if total > 0 {
        format!(
            "{label} · {done}/{total} ({}%)",
            (ratio * 100.0).round() as u32
        )
    } else {
        label
    };
    ui.label(RichText::new(detail).size(11.0).color(theme::TEXT_MUTED));
    ui.add_space(3.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(4), theme::BORDER);
    if ratio > 0.0 {
        let mut filled = rect;
        filled.max.x = rect.min.x + rect.width() * ratio;
        painter.rect_filled(filled, egui::CornerRadius::same(4), theme::CONTROL_BLUE);
    }
}

fn t(locale: crate::editor::cli::Locale, es: &'static str, en: &'static str) -> String {
    tr(locale, es, en).to_string()
}

fn localize_toast(locale: crate::editor::cli::Locale, message: &str) -> String {
    match message {
        "World ready" => t(locale, "Mundo listo", "World ready"),
        "World generation failed" => t(
            locale,
            "Falló la generación del mundo",
            "World generation failed",
        ),
        other => other.to_string(),
    }
}

/// Marco con borde y leyenda equivalente a `<fieldset><legend>`.
fn fieldset(
    ui: &mut egui::Ui,
    legend: &str,
    centered: bool,
    min_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // React: `<fieldset class="col-6 h-100">` es block-level: ocupa
            // todo el ancho que le toca. egui encoge el Frame a su contenido
            // salvo que se fuerce el ancho del ui interior.
            ui.set_width(ui.available_width());
            if min_height > 0.0 {
                ui.set_min_height(min_height);
            }
            // React: `<legend class="fs-5 fw-semibold text-uppercase">`; con
            // root de 14 px, `fs-5` son 17.5 px. Centrado con el ancho real
            // del texto en vez del offset fijo que usaba la migra inicial.
            let title = legend.to_uppercase();
            if centered {
                // `add_space` en un layout top_down suma espacio VERTICAL:
                // tiene que ir dentro de una fila horizontal para centrar.
                widgets::top_row(ui, |ui| {
                    let width = ui
                        .painter()
                        .layout_no_wrap(
                            title.clone(),
                            egui::FontId::proportional(17.5),
                            theme::TEXT,
                        )
                        .size()
                        .x;
                    let space = ((ui.available_width() - width) * 0.5).max(0.0);
                    ui.add_space(space);
                    ui.label(
                        RichText::new(title.clone())
                            .size(17.5)
                            .strong()
                            .color(theme::TEXT),
                    );
                });
            } else {
                ui.label(RichText::new(title).size(17.5).strong().color(theme::TEXT));
            }
            ui.add_space(2.0);
            add_contents(ui);
        });
}

/// Altura fija para todos los labels: así un texto de dos líneas no desplaza
/// su input respecto a los de la misma fila.
const FIELD_LABEL_HEIGHT: f32 = 30.0;

fn uint_field(ui: &mut egui::Ui, label: &str, value: &mut u32, min: u32, max: u32) {
    uint_field_with_icon(ui, "", label, value, min, max);
}

/// Igual que `uint_field`, pero con un icono Phosphor delante del label.
fn uint_field_with_icon(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    value: &mut u32,
    min: u32,
    max: u32,
) {
    ui.vertical(|ui| {
        ui.add_space(6.0);
        let width = ui.available_width();
        let text = if icon.is_empty() {
            label.to_owned()
        } else {
            format!("{icon}  {label}")
        };
        ui.add_sized(
            egui::vec2(width, FIELD_LABEL_HEIGHT),
            egui::Label::new(RichText::new(text).size(12.0).color(theme::TEXT_MUTED)).wrap(),
        );
        ui.add_sized(
            egui::vec2(width, 24.0),
            egui::DragValue::new(value)
                .range(min..=max)
                .speed(1.0)
                .custom_formatter(|value, _| format!("{:.0}", value)),
        );
    });
}

fn float_field(ui: &mut egui::Ui, label: &str, value: &mut f32, min: f32, max: f32, step: f32) {
    ui.vertical(|ui| {
        ui.add_space(6.0);
        let width = ui.available_width();
        ui.add_sized(
            egui::vec2(width, FIELD_LABEL_HEIGHT),
            egui::Label::new(RichText::new(label).size(12.0).color(theme::TEXT_MUTED)).wrap(),
        );
        ui.add_sized(
            egui::vec2(width, 24.0),
            egui::DragValue::new(value)
                .range(min..=max)
                .speed(step)
                .custom_formatter(|value, _| format!("{value:.1}")),
        );
    });
}

fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    suffix: &str,
) {
    ui.add_space(10.0);
    ui.label(
        RichText::new(format!("{label}: {value:.2}{suffix}"))
            .size(12.0)
            .color(theme::TEXT_MUTED),
    );
    // Mismo range que el resto del editor; ocupa todo el ancho disponible.
    widgets::range_slider(ui, value, min, max, step, theme::CONTROL_BLUE);
}

fn terrain_configuration(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let panel = &mut app.worldgen.panel;
    let caps = worldgen::feature_caps(worldgen::size_to_radius(panel.size));

    fieldset(
        ui,
        &t(locale, "Configuración del terreno", "Terrain configuration"),
        true,
        SIDE_PANEL_MIN_HEIGHT,
        |ui| {
            widgets::top_row(ui, |ui| {
                // `col-6` cada uno: 50 % del ancho del panel, con el gap
                // descontado para que no quede hueco a la derecha.
                let gap = ui.spacing().item_spacing.x;
                let half = (ui.available_width() - gap) / 2.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        fieldset(
                            ui,
                            &t(locale, "Configuración de tamaños", "Size settings"),
                            false,
                            FIELDSET_PAIR_MIN_HEIGHT,
                            |ui| {
                                ui.label(
                                    RichText::new(t(locale, "Semilla", "Seed"))
                                        .size(11.0)
                                        .color(theme::TEXT_MUTED),
                                );
                                widgets::top_row(ui, |ui| {
                                    ui.add_sized(
                                        egui::vec2(112.0, 24.0),
                                        egui::DragValue::new(&mut panel.seed)
                                            .range(0..=u32::MAX)
                                            .speed(1.0),
                                    );
                                    if ui
                                        .button(icons::ARROWS_CLOCKWISE)
                                        .on_hover_text(t(
                                            locale,
                                            "Nueva semilla aleatoria",
                                            "New random seed",
                                        ))
                                        .clicked()
                                    {
                                        let mut bytes = [0u8; 4];
                                        if getrandom::fill(&mut bytes).is_ok() {
                                            panel.seed = u32::from_le_bytes(bytes);
                                        }
                                    }
                                });
                                let mut size = panel.size as f32;
                                slider(
                                    ui,
                                    &t(locale, "Tamaño del mundo (m)", "World size (m)"),
                                    &mut size,
                                    worldgen::SIZE_MIN as f32,
                                    worldgen::SIZE_MAX as f32,
                                    8.0,
                                    " m",
                                );
                                let new_size = size.round() as u32;
                                if new_size != panel.size {
                                    let current = std::mem::take(panel);
                                    *panel = current.clamp_to_size(new_size);
                                }
                                let mut voxel = panel.voxel_size;
                                slider(
                                    ui,
                                    &t(locale, "Tamaño de voxel (m)", "Voxel size (m)"),
                                    &mut voxel,
                                    0.25,
                                    4.0,
                                    0.25,
                                    " m",
                                );
                                panel.voxel_size = voxel;
                            },
                        );
                    },
                );
                ui.add_space(gap);
                ui.allocate_ui_with_layout(
                    egui::vec2(half, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        fieldset(
                            ui,
                            &t(locale, "Elementos del terreno", "Terrain elements"),
                            false,
                            FIELDSET_PAIR_MIN_HEIGHT,
                            |ui| {
                                let fields: [(&str, &str, &mut u32, u32); 9] = [
                                    (
                                        icons::MOUNTAINS,
                                        &t(locale, "Montañas", "Mountains"),
                                        &mut panel.mountains,
                                        caps.mountains,
                                    ),
                                    (
                                        icons::TREE_EVERGREEN,
                                        &t(locale, "Colinas", "Hills"),
                                        &mut panel.hills,
                                        caps.hills,
                                    ),
                                    (
                                        icons::CHART_LINE_DOWN,
                                        &t(locale, "Valles", "Valleys"),
                                        &mut panel.valleys,
                                        caps.valleys,
                                    ),
                                    (
                                        icons::SUN_HORIZON,
                                        &t(locale, "Llanuras", "Plains"),
                                        &mut panel.plains,
                                        caps.plains,
                                    ),
                                    (
                                        icons::BARRICADE,
                                        &t(locale, "Acantilados", "Cliffs"),
                                        &mut panel.cliffs,
                                        caps.cliffs,
                                    ),
                                    (
                                        icons::DROP_SIMPLE,
                                        &t(locale, "Lagos", "Lakes"),
                                        &mut panel.lakes,
                                        caps.lakes,
                                    ),
                                    (
                                        icons::WAVES,
                                        &t(locale, "Ríos", "Rivers"),
                                        &mut panel.rivers,
                                        caps.rivers,
                                    ),
                                    (
                                        icons::FALLOUT_SHELTER,
                                        &t(locale, "Cuevas", "Caves"),
                                        &mut panel.caves,
                                        caps.caves,
                                    ),
                                    (
                                        icons::CLOUD_RAIN,
                                        &t(locale, "Cascadas", "Waterfalls"),
                                        &mut panel.waterfalls,
                                        caps.waterfalls,
                                    ),
                                ];
                                let mut iter = fields.into_iter();
                                for _row in 0..3 {
                                    widgets::top_row(ui, |ui| {
                                        // Calcular la columna una sola vez y
                                        // descontar el espaciado entre columnas:
                                        // si no, 3 columnas ocupan más que el
                                        // ancho disponible y se corta el lado.
                                        let gap = ui.spacing().item_spacing.x;
                                        let col = (ui.available_width() - gap * 2.0) / 3.0;
                                        for _col in 0..3 {
                                            if let Some((icon, label, value, max)) = iter.next() {
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(col, 0.0),
                                                    egui::Layout::top_down(egui::Align::Min),
                                                    |ui| {
                                                        uint_field_with_icon(
                                                            ui, icon, label, value, 0, max,
                                                        )
                                                    },
                                                );
                                            }
                                        }
                                    });
                                }
                            },
                        );
                    },
                );
            });

            let radius = worldgen::size_to_radius(panel.size);
            let biome_slots = worldgen::max_biome_slots(radius);
            ui.add_space(10.0);
            ui.label(
                RichText::new(t(locale, "Biomas", "Biomes"))
                    .size(17.5)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.label(
                RichText::new(
                    t(
                        locale,
                        "Este tamaño admite hasta {n} biomas",
                        "This size fits up to {n} biomes",
                    )
                    .replace("{n}", &biome_slots.to_string()),
                )
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(6.0);
            let enabled_count = panel.biomes.iter().filter(|enabled| **enabled).count();
            let names = [
                t(locale, "Pradera", "Grassland"),
                t(locale, "Bosque", "Forest"),
                t(locale, "Nieve", "Snow"),
                t(locale, "Desierto", "Desert"),
                t(locale, "Volcánico", "Volcanic"),
            ];
            let mut biomes = panel.biomes;
            let mut shares = panel.biome_shares;
            widgets::top_row(ui, |ui| {
                let gap = ui.spacing().item_spacing.x;
                let column = (ui.available_width() - gap * 4.0) / 5.0;
                for index in 0..5 {
                    ui.allocate_ui_with_layout(
                        egui::vec2(column, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let at_cap = !biomes[index] && enabled_count >= biome_slots;
                            // Solo los controles deshabilitados llevan tooltip,
                            // explicando el motivo.
                            let switch_hint = if at_cap {
                                t(
                                    locale,
                                    "Límite de biomas alcanzado para este tamaño de mundo",
                                    "Biome limit reached for this world size",
                                )
                            } else {
                                String::new()
                            };
                            ui.add_enabled_ui(!at_cap, |ui| {
                                // React: `<form-check form-switch style="font-size:1.2rem">`
                                // = switch de pastilla + etiqueta al lado, no un botón.
                                widgets::top_row(ui, |ui| {
                                    if widgets::switch_value(ui, &mut biomes[index], &switch_hint) {
                                        let (min, _max) = worldgen::biome_share_range(
                                            biomes.iter().filter(|enabled| **enabled).count(),
                                        );
                                        let _ = min;
                                        shares =
                                            worldgen::toggle_biome_share(&shares, &biomes, index);
                                    }
                                    ui.label(
                                        RichText::new(&names[index]).size(16.0).color(theme::TEXT),
                                    );
                                });
                            });
                            let share_locked = !biomes[index] || enabled_count <= 1;
                            let slider_hint = if share_locked {
                                if !biomes[index] {
                                    t(
                                        locale,
                                        "Activa el bioma para ajustar su reparto",
                                        "Enable the biome to adjust its share",
                                    )
                                } else {
                                    t(
                                        locale,
                                        "Con un solo bioma activo el reparto es fijo",
                                        "With only one biome enabled the share is fixed",
                                    )
                                }
                            } else {
                                String::new()
                            };
                            // React: slider y `%` en la misma fila (`d-flex
                            // align-items-center gap-2`, valor min-width 2.6rem).
                            let share_row = widgets::top_row(ui, |ui| {
                                let slider_width = (ui.available_width() - 38.0).max(20.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(slider_width, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.add_enabled_ui(!share_locked, |ui| {
                                            // El mismo range que usan las sidebars:
                                            // pista gris y tirador azul de Bootstrap.
                                            let mut share = shares[index] as f32;
                                            if widgets::range_slider_no_tooltip(
                                                ui,
                                                &mut share,
                                                0.0,
                                                100.0,
                                                1.0,
                                                theme::CONTROL_BLUE,
                                            ) && share.round() as u32 != shares[index]
                                            {
                                                shares = worldgen::rebalance_biome_shares(
                                                    &shares,
                                                    &biomes,
                                                    index,
                                                    share.round() as u32,
                                                );
                                            }
                                        });
                                    },
                                );
                                ui.label(
                                    RichText::new(format!("{}%", shares[index]))
                                        .size(11.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                            if !slider_hint.is_empty() {
                                let _ = share_row.response.on_hover_text(slider_hint);
                            }
                        },
                    );
                }
            });
            panel.biomes = biomes;
            panel.biome_shares = shares;
            ui.label(
                RichText::new(
                    t(
                        locale,
                        "Los biomas activos siempre suman el 100 % del mundo, con al menos un {n} % cada uno",
                        "Enabled biomes always add up to 100% of the world, at least {n}% each",
                    )
                    .replace("{n}", &worldgen::MIN_BIOME_SHARE.to_string()),
                )
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );

            // React: `<div class="form-check mt-2">` dentro del bloque de
            // biomas, separado por una línea horizontal.
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            widgets::top_row(ui, |ui| {
                widgets::switch_value(ui, &mut panel.replace_world, "");
                ui.label(
                    RichText::new(t(
                        locale,
                        "Reemplazar mundo actual",
                        "Replace current world",
                    ))
                    .size(12.0)
                    .color(theme::TEXT),
                );
            });
        },
    );
}

fn scatter_fields(
    ui: &mut egui::Ui,
    locale: crate::editor::cli::Locale,
    rules: &mut worldgen::ScatterRules,
) {
    // Máximo 3 campos por fila, todos con la misma anchura de columna.
    widgets::top_row(ui, |ui| {
        let gap = ui.spacing().item_spacing.x;
        let col = (ui.available_width() - gap * 2.0) / 3.0;
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                uint_field(
                    ui,
                    &t(locale, "Máx. instancias", "Max instances"),
                    &mut rules.max_instances,
                    0,
                    u32::MAX,
                )
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                float_field(
                    ui,
                    &t(locale, "Distancia mín. (m)", "Min distance (m)"),
                    &mut rules.min_distance,
                    0.0,
                    1000.0,
                    0.5,
                )
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                uint_field(
                    ui,
                    &t(locale, "Densidad %", "Density %"),
                    &mut rules.density,
                    0,
                    100,
                )
            },
        );
    });
    ui.add_space(6.0);
    widgets::top_row(ui, |ui| {
        let gap = ui.spacing().item_spacing.x;
        let col = (ui.available_width() - gap * 2.0) / 3.0;
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                float_field(
                    ui,
                    &t(locale, "Pendiente máx. (°)", "Max slope (°)"),
                    &mut rules.max_slope_deg,
                    0.0,
                    90.0,
                    1.0,
                )
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                float_field(
                    ui,
                    &t(locale, "Altitud mín. (m)", "Min altitude (m)"),
                    &mut rules.altitude_min,
                    -1000.0,
                    10000.0,
                    1.0,
                )
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                float_field(
                    ui,
                    &t(locale, "Altitud máx. (m)", "Max altitude (m)"),
                    &mut rules.altitude_max,
                    -1000.0,
                    10000.0,
                    1.0,
                )
            },
        );
    });
    ui.add_space(6.0);
    widgets::top_row(ui, |ui| {
        let gap = ui.spacing().item_spacing.x;
        let col = (ui.available_width() - gap * 2.0) / 3.0;
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                uint_field(
                    ui,
                    &t(locale, "Escala mín. %", "Min scale %"),
                    &mut rules.scale_min,
                    0,
                    1000,
                )
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(col, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                uint_field(
                    ui,
                    &t(locale, "Escala máx. %", "Max scale %"),
                    &mut rules.scale_max,
                    0,
                    1000,
                )
            },
        );
    });
    ui.add_space(6.0);
    widgets::top_row(ui, |ui| {
        widgets::switch_value(ui, &mut rules.random_rotation, "");
        ui.label(
            RichText::new(t(locale, "Rotación aleatoria", "Random rotation"))
                .size(12.0)
                .color(theme::TEXT),
        );
        ui.add_space(8.0);
        widgets::switch_value(ui, &mut rules.cluster_enabled, "");
        ui.label(
            RichText::new(t(locale, "Clústeres", "Clusters"))
                .size(12.0)
                .color(theme::TEXT),
        );
    });
    if rules.cluster_enabled {
        ui.add_space(6.0);
        widgets::top_row(ui, |ui| {
            ui.label(
                RichText::new(t(locale, "Tamaño de clúster", "Cluster size"))
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.add(
                egui::DragValue::new(&mut rules.cluster_size)
                    .range(2..=1000)
                    .speed(1.0),
            );
        });
    }
}

fn distribute_blueprints(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    fieldset(
        ui,
        &t(locale, "Distribuir blueprints", "Distribute blueprints"),
        false,
        SIDE_PANEL_MIN_HEIGHT,
        |ui| {
            if app.assets.models.is_empty() {
                ui.label(
                    RichText::new(t(
                        locale,
                        "Aún no hay modelos en el proyecto",
                        "No models in the project yet",
                    ))
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
                );
            } else {
                widgets::top_row(ui, |ui| {
                    let combo_width = ui.available_width() - 110.0;
                    egui::ComboBox::from_id_salt("worldgen_model_combo")
                        .width(combo_width.max(120.0))
                        .selected_text(if app.worldgen.selected_model_path.is_empty() {
                            t(locale, "Selecciona modelo…", "Select model…")
                        } else {
                            app.assets
                                .models
                                .iter()
                                .find(|model| model.path == app.worldgen.selected_model_path)
                                .map(|model| model.name.clone())
                                .unwrap_or_else(|| app.worldgen.selected_model_path.clone())
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.worldgen.selected_model_path,
                                String::new(),
                                t(locale, "Selecciona modelo…", "Select model…"),
                            );
                            for model in &app.assets.models {
                                ui.selectable_value(
                                    &mut app.worldgen.selected_model_path,
                                    model.path.clone(),
                                    &model.name,
                                );
                            }
                        });
                    let can_add = !app.worldgen.selected_model_path.is_empty();
                    if bootstrap_button(
                        ui,
                        icons::PLUS,
                        &t(locale, "Agregar modelo", "Add model"),
                        Btn::OutlineInfo,
                        100.0,
                        can_add,
                        false,
                    ) {
                        let path = app.worldgen.selected_model_path.clone();
                        let id = format!("wg_{}", app.worldgen.scatter_entries.len() + 1);
                        app.worldgen.scatter_entries.push(worldgen::ScatterEntry {
                            id: id.clone(),
                            model_path: path,
                            rules: worldgen::ScatterRules::default(),
                        });
                        app.worldgen.open_scatter_id = Some(id);
                        app.worldgen.selected_model_path.clear();
                    }
                });
            }

            if !app.worldgen.scatter_entries.is_empty() {
                ui.add_space(6.0);
                let entries = std::mem::take(&mut app.worldgen.scatter_entries);
                let mut kept: Vec<worldgen::ScatterEntry> = Vec::new();
                let mut removed = false;
                for mut entry in entries {
                    let open = app.worldgen.open_scatter_id.as_deref() == Some(entry.id.as_str());
                    let name = app
                        .assets
                        .models
                        .iter()
                        .find(|model| model.path == entry.model_path)
                        .map(|model| model.name.clone())
                        .unwrap_or_else(|| entry.model_path.clone());
                    let mut remove = false;
                    // El mismo acordeón que usan las sidebars del editor.
                    let response = accordion::item(
                        ui,
                        egui::Id::new(("worldgen_scatter", entry.id.clone())),
                        1,
                        open,
                        |ui, openness| {
                            let mut header_response = None;
                            ui.horizontal(|ui| {
                                let header_width = (ui.available_width() - 30.0).max(80.0);
                                header_response = Some(
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(header_width, 32.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            accordion::header(
                                                ui,
                                                icons::CUBE,
                                                &name,
                                                open,
                                                openness,
                                                1,
                                            )
                                        },
                                    )
                                    .inner,
                                );
                                if widgets::icon_action(
                                    ui,
                                    icons::TRASH,
                                    20.0,
                                    theme::DANGER,
                                    &t(locale, "Quitar modelo", "Remove model"),
                                ) {
                                    remove = true;
                                }
                            });
                            header_response.expect("accordion header response")
                        },
                        |ui| scatter_fields(ui, locale, &mut entry.rules),
                    );
                    if response.clicked() {
                        app.worldgen.open_scatter_id =
                            if open { None } else { Some(entry.id.clone()) };
                    }
                    if remove {
                        removed = true;
                    } else {
                        kept.push(entry);
                    }
                }
                app.worldgen.scatter_entries = kept;
                if removed {
                    app.worldgen.open_scatter_id = None;
                }
            }
        },
    );
}

/// Dibuja el cuerpo en la ventana modal externa.
pub(crate) fn show_in_window(ui: &mut egui::Ui, app: &mut EditorApp) {
    if app.worldgen.library_path.is_none() {
        app.worldgen.library_path = crate::editor::plugin_runtime::worldgen_library_path();
    }
    let locale = app.cli.locale;

    // Controles más grandes: `DragValue` y `Slider` toman su altura de
    // `interact_size.y`, y el grosor del rail de `slider_rail_height`.
    ui.spacing_mut().interact_size.y = 26.0;
    ui.spacing_mut().slider_rail_height = 6.0;

    egui::Frame::new()
        .fill(theme::PANEL_BG)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("worldgen_scroll")
                .show(ui, |ui| {
                    if app.worldgen.library_path.is_none() {
                        egui::Frame::new()
                            .fill(DANGER_BG)
                            .corner_radius(4)
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.label(RichText::new(t(locale, "Instala primero el plugin Generador de mundos",
                                    "Install the World Generator plugin first")).size(12.0).color(DANGER_TEXT));
                            });
                        ui.add_space(8.0);
                    }

                    // Los dos paneles van en la misma fila, como en el diseño
                    // original: terreno a la izquierda, blueprints a la derecha.
                    widgets::top_row(ui, |ui| {
                        let gap = ui.spacing().item_spacing.x;
                        let total = ui.available_width() - gap - 12.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(total * 0.694, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| terrain_configuration(ui, app),
                        );
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            egui::vec2(total * 0.306, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| distribute_blueprints(ui, app),
                        );
                    });

                    if let Some(generation) = app.worldgen.generation {
                        ui.add_space(6.0);
                        generation_progress(ui, locale, generation);
                    }

                    ui.add_space(10.0);
                    widgets::top_row(ui, |ui| {
                        let label = if app.worldgen.distributing {
                            t(locale, "Distribuyendo blueprints…", "Distributing blueprints…")
                        } else if let Some(generation) = app.worldgen.generation {
                            match generation.phase {
                                worldgen::GenerationPhase::Scan => {
                                    if generation.total > 0 {
                                        format!("{} {}/{}", t(locale, "Revisando columnas…", "Scanning columns…"), generation.completed, generation.total)
                                    } else {
                                        t(locale, "Revisando columnas…", "Scanning columns…")
                                    }
                                }
                                worldgen::GenerationPhase::Mesh => format!(
                                    "{} {}/{}", t(locale, "Generando mundo…", "Generating world…"), generation.completed, generation.total),
                            }
                        } else {
                            t(locale, "Generar mundo y distribuir blueprints", "Generate world and distribute blueprints")
                        };
                        let busy = app.worldgen.inputs_locked();
                        // React: `btn-primary btn-sm flex-grow-1` + Stop/Clear fijos.
                        let side_width = 90.0;
                        let gap = ui.spacing().item_spacing.x;
                        let generate_width =
                            (ui.available_width() - side_width * 2.0 - gap * 2.0 - 12.0).max(160.0);
                        if bootstrap_button(ui, icons::PLAY, &label, Btn::Primary, generate_width, !busy, busy)
                        {
                            match app.worldgen.library_path.clone() {
                                None => app.worldgen.show_toast(
                                    worldgen::WorldGenToastKind::Error,
                                    t(locale, "Instala primero el plugin Generador de mundos",
                                        "Install the World Generator plugin first"),
                                ),
                                Some(path) => {
                                    app.worldgen.finish_handled = false;
                                    app.worldgen.pending_scatter = 0;
                                    app.worldgen.distributing = false;
                                    app.worldgen.toast = None;
                                    app.worldgen.generation = Some(worldgen::Generation {
                                        completed: 0, total: 0, phase: worldgen::GenerationPhase::Scan,
                                    });
                                    app.send_engine(json!({ "cmd": "world_gen_set_library_path", "path": path }));
                                    let config = app.worldgen.panel.config_json();
                                    app.send_engine(json!({ "cmd": "world_gen_set_config", "config_json": config }));
                                    app.send_engine(json!({
                                        "cmd": "world_gen_start",
                                        "replace_world": app.worldgen.panel.replace_world,
                                    }));
                                }
                            }
                        }
                        if bootstrap_button(ui, icons::STOP, &t(locale, "Detener", "Stop"),
                            Btn::OutlineSecondary, side_width, busy, false)
                        {
                            app.worldgen.finish_handled = true;
                            app.worldgen.pending_scatter = 0;
                            app.worldgen.distributing = false;
                            app.worldgen.generation = None;
                            app.worldgen.show_toast(
                                worldgen::WorldGenToastKind::Info,
                                t(locale, "Generación cancelada", "Generation cancelled"),
                            );
                            app.send_engine(json!({ "cmd": "world_gen_stop" }));
                        }
                        if bootstrap_button(ui, icons::TRASH, &t(locale, "Limpiar", "Clear"),
                            Btn::OutlineDanger, side_width,
                            app.worldgen.world_generated && !busy, false)
                        {
                            app.worldgen.world_generated = false;
                            app.worldgen.show_toast(
                                worldgen::WorldGenToastKind::Info,
                                t(locale, "Mundo eliminado", "World cleared"),
                            );
                            app.send_engine(json!({ "cmd": "world_gen_reset" }));
                        }
                    });
                });
        });

    let toast_expired = if let Some(toast) = app.worldgen.toast.as_mut() {
        let kind = match toast.kind {
            worldgen::WorldGenToastKind::Success => widgets::ToastKind::Success,
            worldgen::WorldGenToastKind::Error => widgets::ToastKind::Error,
            worldgen::WorldGenToastKind::Info => widgets::ToastKind::Info,
        };
        let message = localize_toast(locale, &toast.message);
        !widgets::floating_toast(ui, kind, &message, &mut toast.start_time)
    } else {
        false
    };
    if toast_expired {
        app.worldgen.toast = None;
    }

    worldgen::flush_commands(app);
}
