//! Barra lateral derecha: configuración del mundo funcional y métricas.
//!
//! Los controles envían comandos `set_*` directamente al motor (el editor es su
//! dueño). Los valores se sincronizan desde `scene.world` del snapshot y desde
//! los eventos de confirmación, de modo que React y egui no se desalinean.

use egui::{Align, Layout, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::app::{EditorApp, EditorStatus};
use crate::cli::Locale;
use crate::context::ProjectContext;
use crate::i18n;
use crate::theme;
use crate::world::{
    MsaaTier, REFLECTION_DEBUG_VIEWS, ReflectionTier, ShadowTier, TextureTier, WorldSettings,
    normalize_debug_view,
};

use super::number_draft::NumberDraft;
use super::{accordion, widgets};

#[derive(Default)]
pub struct SidebarRightState {
    open: Option<&'static str>,
    grid_locked: bool,
    width: NumberDraft,
    height: NumberDraft,
    radius: NumberDraft,
    grid_cell: NumberDraft,
    fps_selection: u32,
    fps_dirty: bool,
}

impl SidebarRightState {
    #[cfg(test)]
    fn toggle(&mut self, id: &'static str) {
        self.open = if self.open == Some(id) {
            None
        } else {
            Some(id)
        };
    }
}

/// Acción pendiente de aplicar al motor tras dibujar la UI.
enum WorldAction {
    SetSize {
        width: f32,
        height: f32,
    },
    SetRadius(f32),
    SetGridVisible(bool),
    SetGridCellSize(f32),
    SetGravity(f32),
    SetTargetFps(u32),
    SetLight {
        ambient: Option<f32>,
        intensity: Option<f32>,
        shadow_darkness: Option<f32>,
    },
    SetTextureTier(TextureTier),
    SetTextureDetailDistance(f32),
    SetReflectionTier(ReflectionTier),
    SetReflectionRaytracing(bool),
    SetReflectionProbes(bool),
    SpawnReflectionProbe,
    SetReflectionDebugView(&'static str),
    SetShadowTier(ShadowTier),
    SetMsaaTier(MsaaTier),
    SetTaa {
        enabled: bool,
        blend: f32,
        jitter_scale: f32,
    },
}

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let context = app.cli.context;
    let enabled = app.status == EditorStatus::Ready;
    let world = app.world;
    let mut actions = Vec::new();

    // Métricas fijas al fondo (React: .metrics-panel, 120 px).
    egui::Panel::bottom("editor_right_metrics")
        .exact_size(120.0)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(theme::METRICS_BG))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.x_range(),
                rect.top(),
                egui::Stroke::new(1.0, theme::METRICS_BORDER),
            );
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(7, 7))
                .show(ui, |ui| metrics(ui, app));
        });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 0,
                    top: 8,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(i18n::tr(
                                locale,
                                "CONFIGURACIÓN DEL MUNDO",
                                "WORLD SETTINGS",
                            ))
                            .size(14.0)
                            .strong()
                            .color(theme::TEXT_MUTED),
                        );
                    });
                    ui.add_space(6.0);

                    let SidebarRightState {
                        open,
                        grid_locked,
                        width,
                        height,
                        radius,
                        grid_cell,
                        fps_selection,
                        fps_dirty,
                    } = &mut app.right_sidebar;

                    // Sincroniza los borradores con el estado del motor.
                    width.sync(world.world_width);
                    height.sync(world.world_height);
                    radius.sync(world.world_radius);
                    grid_cell.sync(world.grid_cell_size);
                    if !*fps_dirty {
                        *fps_selection = world.target_fps;
                    }

                    sub(
                        ui,
                        open,
                        "world_workspace",
                        icons::FRAME_CORNERS,
                        i18n::tr(locale, "Espacio de trabajo", "Workspace"),
                        enabled,
                        |ui| {
                            if context.is_3d() {
                                workspace_3d(ui, radius, world, locale, &mut actions);
                            } else {
                                workspace_2d(ui, width, height, locale, &mut actions);
                            }
                        },
                    );

                    if !context.is_3d() {
                        sub(
                            ui,
                            open,
                            "world_background",
                            icons::IMAGE,
                            i18n::tr(locale, "Fondo del mundo", "World background"),
                            enabled,
                            |ui| {
                                background_body(
                                    ui,
                                    locale,
                                    i18n::tr(
                                        locale,
                                        "Selector pendiente: llega con la gestión de recursos",
                                        "Picker pending: coming with asset management",
                                    ),
                                )
                            },
                        );
                    }

                    sub(
                        ui,
                        open,
                        "world_grid",
                        icons::GRID_FOUR,
                        i18n::tr(locale, "Rejilla", "Grid"),
                        enabled,
                        |ui| {
                            grid_body(
                                ui,
                                grid_locked,
                                grid_cell,
                                context,
                                world,
                                locale,
                                &mut actions,
                            )
                        },
                    );

                    if context.is_3d() {
                        sub(
                            ui,
                            open,
                            "world_lighting",
                            icons::SUN,
                            i18n::tr(locale, "Sol e iluminación", "Sun and lighting"),
                            enabled,
                            |ui| lighting_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_textures",
                            icons::IMAGE,
                            i18n::tr(locale, "Texturas GLB", "GLB textures"),
                            enabled,
                            |ui| textures_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_reflections",
                            icons::SPARKLE,
                            i18n::tr(locale, "Reflejos", "Reflections"),
                            enabled,
                            |ui| reflections_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_shadows",
                            icons::MOON,
                            i18n::tr(locale, "Sombras", "Shadows"),
                            enabled,
                            |ui| shadows_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_msaa",
                            icons::COLUMNS,
                            "MSAA",
                            enabled,
                            |ui| msaa_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_taa",
                            icons::SHIELD_CHECK,
                            "TAA",
                            enabled,
                            |ui| taa_body(ui, world, locale, &mut actions),
                        );
                    }

                    sub(
                        ui,
                        open,
                        "world_gravity",
                        icons::ARROW_CIRCLE_DOWN,
                        i18n::tr(locale, "Gravedad", "Gravity"),
                        enabled,
                        |ui| gravity_body(ui, world, locale, &mut actions),
                    );
                    sub(
                        ui,
                        open,
                        "world_fps",
                        icons::SPEEDOMETER,
                        i18n::tr(locale, "Límite de FPS", "FPS limit"),
                        enabled,
                        |ui| fps_body(ui, fps_selection, fps_dirty, world, locale, &mut actions),
                    );
                });
        });

    for action in actions {
        apply_action(app, action);
    }
}

fn sub(
    ui: &mut egui::Ui,
    open: &mut Option<&'static str>,
    id: &'static str,
    icon: &str,
    title: &str,
    enabled: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let is_open = *open == Some(id);
    let response = accordion::item(
        ui,
        egui::Id::new(("editor_acc_right", id)),
        1,
        is_open,
        |ui, openness| accordion::header(ui, icon, title, is_open, openness, 1),
        |ui| {
            ui.add_enabled_ui(enabled, |ui| body(ui));
        },
    );

    if response.clicked() {
        *open = if is_open { None } else { Some(id) };
    }

    ui.add_space(2.0);
}

fn workspace_3d(
    ui: &mut egui::Ui,
    radius: &mut NumberDraft,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::label_value(
        ui,
        i18n::tr(locale, "Radio de esfera (u)", "Sphere radius (u)"),
        &format!("{:.1} u", world.world_radius),
    );

    let mut value = world.world_radius;
    if widgets::range_slider(ui, &mut value, 5.0, 500.0, 1.0, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetRadius(value));
    }
    ui.add_space(2.0);

    ui.label(
        RichText::new(i18n::tr(locale, "Radio (u)", "Radius (u)"))
            .size(11.5)
            .color(theme::TEXT_MUTED),
    );
    let width = ui.available_width();
    let committed = widgets::number_input(ui, width, &mut radius.text);
    if committed && let Some(value) = radius.parsed() {
        actions.push(WorldAction::SetRadius(value));
    }
}

fn workspace_2d(
    ui: &mut egui::Ui,
    width_draft: &mut NumberDraft,
    height_draft: &mut NumberDraft,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    let mut commit = false;
    ui.columns(2, |columns| {
        let label_width = columns[0].available_width();
        columns[0].label(
            RichText::new(i18n::tr(locale, "Ancho (u)", "Width (u)"))
                .size(11.5)
                .color(theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[0], label_width, &mut width_draft.text) {
            commit = true;
        }

        let label_height = columns[1].available_width();
        columns[1].label(
            RichText::new(i18n::tr(locale, "Alto (u)", "Height (u)"))
                .size(11.5)
                .color(theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[1], label_height, &mut height_draft.text) {
            commit = true;
        }
    });

    if commit
        && let (Some(width), Some(height)) = (width_draft.parsed(), height_draft.parsed())
        && width > 0.0
        && height > 0.0
    {
        actions.push(WorldAction::SetSize { width, height });
    }
}

fn background_body(ui: &mut egui::Ui, locale: Locale, hint: &str) {
    let width = (ui.available_width() - 34.0).max(0.0);
    let unavailable = i18n::tr(locale, "Sin fondos cargados", "No backgrounds loaded");

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        widgets::select_placeholder_sized(ui, width, unavailable, hint);
        widgets::small_icon_button(ui, icons::CHECK, hint);
    });
}

fn grid_body(
    ui: &mut egui::Ui,
    locked: &mut bool,
    cell: &mut NumberDraft,
    context: ProjectContext,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    if context.is_3d() {
        widgets::hint(
            ui,
            i18n::tr(
                locale,
                "El tamaño de celda alinea el checker del suelo y el snap (Ctrl).",
                "Cell size aligns ground checker and object placement snap (Ctrl).",
            ),
        );
    } else {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut visible = world.grid_visible;
            let icon = if world.grid_visible {
                icons::EYE
            } else {
                icons::EYE_SLASH
            };
            if widgets::icon_toggle_value(
                ui,
                icon,
                &mut visible,
                i18n::tr(locale, "Mostrar/ocultar rejilla", "Show/hide grid"),
            ) {
                actions.push(WorldAction::SetGridVisible(visible));
            }
        });
    }

    let mut commit_cell = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(i18n::tr(locale, "Tamaño de celda", "Cell size"))
                .size(12.0)
                .color(theme::TEXT_MUTED),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let icon = if *locked {
                icons::LOCK
            } else {
                icons::LOCK_OPEN
            };
            let mut lock_state = *locked;
            if widgets::icon_toggle_value(
                ui,
                icon,
                &mut lock_state,
                i18n::tr(locale, "Bloquear tamaño de rejilla", "Lock grid size"),
            ) {
                *locked = lock_state;
            }
            ui.add_enabled_ui(!*locked, |ui| {
                commit_cell = widgets::number_input(ui, 55.0, &mut cell.text);
            });
        });
    });
    ui.add_space(2.0);

    ui.add_enabled_ui(!*locked, |ui| {
        let mut value = world.grid_cell_size;
        if widgets::range_slider(ui, &mut value, 0.25, 10.0, 0.25, theme::CONTROL_BLUE) {
            actions.push(WorldAction::SetGridCellSize(value));
        }
    });

    if !*locked
        && commit_cell
        && let Some(value) = cell.parsed()
        && value > 0.0
    {
        actions.push(WorldAction::SetGridCellSize(value));
    }
}

fn lighting_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        i18n::tr(
            locale,
            "Mueve el gizmo del sol en el viewport. Estos valores ajustan el brillo.",
            "Move the sun gizmo in the viewport. These values tune brightness.",
        ),
    );

    widgets::label_value(
        ui,
        i18n::tr(locale, "Luz ambiental", "Ambient light"),
        &format!("{:.2}", world.light_ambient),
    );
    let mut ambient = world.light_ambient;
    if widgets::range_slider(ui, &mut ambient, 0.0, 0.45, 0.01, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetLight {
            ambient: Some(ambient),
            intensity: None,
            shadow_darkness: None,
        });
    }
    ui.add_space(2.0);

    widgets::label_value(
        ui,
        i18n::tr(locale, "Intensidad de luz", "Light intensity"),
        &format!("{:.2}", world.light_intensity),
    );
    let mut intensity = world.light_intensity;
    if widgets::range_slider(ui, &mut intensity, 0.2, 2.5, 0.05, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetLight {
            ambient: None,
            intensity: Some(intensity),
            shadow_darkness: None,
        });
    }
}

fn textures_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        i18n::tr(
            locale,
            "El nivel alto mejora la nitidez de las texturas GLB a costa de VRAM.",
            "Higher tiers improve GLB texture sharpness at the cost of VRAM.",
        ),
    );
    widgets::caption(
        ui,
        i18n::tr(
            locale,
            "Nivel de texturas gráficas",
            "Graphics texture tier",
        ),
    );

    if let Some(index) = tier_choice(
        ui,
        locale,
        &TextureTier::ALL,
        world.graphics_texture_tier,
        TextureTier::as_str,
        i18n::tr(locale, "Nivel de texturas GLB", "GLB texture tier"),
    ) {
        actions.push(WorldAction::SetTextureTier(TextureTier::ALL[index]));
    }
    ui.add_space(4.0);

    widgets::label_value(
        ui,
        i18n::tr(locale, "Distancia de detalle", "Texture detail distance"),
        &format!("{:.0} m", world.texture_detail_distance),
    );
    let mut distance = world.texture_detail_distance;
    if widgets::range_slider(ui, &mut distance, 5.0, 80.0, 1.0, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetTextureDetailDistance(distance));
    }
    widgets::tiny(
        ui,
        i18n::tr(
            locale,
            "Más distancia = texturas nítidas más lejos.",
            "Longer distance = sharp textures further away.",
        ),
    );
}

fn reflections_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    if let Some(effective) = world.reflection_tier_effective
        && effective != world.reflection_tier
        && matches!(
            world.reflection_tier,
            ReflectionTier::High | ReflectionTier::Ultra
        )
    {
        ui.label(
            RichText::new(i18n::tr(
                locale,
                "La GPU no soporta el tier pedido; el motor usa un nivel menor.",
                "The GPU does not support the requested tier; the engine uses a lower one.",
            ))
            .size(10.0)
            .color(theme::WARNING_BADGE),
        );
    }

    let reflections_active = !world.reflection_tier.is_off();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(i18n::tr(locale, "Nivel de reflejos", "Reflection tier"))
                .size(12.0)
                .strong()
                .color(theme::TEXT_MUTED),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(
                RichText::new("RT ON")
                    .size(9.5)
                    .color(if world.reflection_raytracing {
                        theme::INFO
                    } else {
                        theme::TEXT_MUTED
                    }),
            );
            ui.add_enabled_ui(reflections_active, |ui| {
                let mut enabled = world.reflection_raytracing;
                if widgets::switch_value(
                    ui,
                    &mut enabled,
                    i18n::tr(locale, "Ray tracing de reflejos", "Reflection ray tracing"),
                ) {
                    actions.push(WorldAction::SetReflectionRaytracing(enabled));
                }
            });
            ui.label(
                RichText::new("RT OFF")
                    .size(9.5)
                    .color(if world.reflection_raytracing {
                        theme::TEXT_MUTED
                    } else {
                        theme::INFO
                    }),
            );
        });
    });

    if let Some(index) = tier_choice(
        ui,
        locale,
        &ReflectionTier::ALL,
        world.reflection_tier,
        ReflectionTier::as_str,
        i18n::tr(
            locale,
            "Nivel de reflejos (Off desactiva SSR y composición)",
            "Reflection tier (Off disables SSR and composite)",
        ),
    ) {
        actions.push(WorldAction::SetReflectionTier(ReflectionTier::ALL[index]));
    }
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.add_enabled_ui(reflections_active, |ui| {
            let mut probes = world.reflection_probes;
            if widgets::switch_value(
                ui,
                &mut probes,
                i18n::tr(
                    locale,
                    "Reflection probes en el render",
                    "Reflection probes in the render",
                ),
            ) {
                actions.push(WorldAction::SetReflectionProbes(probes));
            }
        });
        let width = ui.available_width();
        if widgets::wide_button_click_sized(
            ui,
            width,
            "",
            i18n::tr(
                locale,
                "Insertar sonda de reflexión",
                "Insert reflection probe",
            ),
            i18n::tr(
                locale,
                "Coloca una sonda en el centro del viewport",
                "Place a probe at the viewport center",
            ),
            theme::INFO,
        ) {
            actions.push(WorldAction::SpawnReflectionProbe);
        }
    });
    ui.add_space(4.0);

    widgets::caption(
        ui,
        i18n::tr(locale, "Vista de depuración", "Reflection debug view"),
    );
    let mut view = world.reflection_debug_view;
    let width = ui.available_width();
    if widgets::combo_text(
        ui,
        "editor_reflection_debug_view",
        width,
        &REFLECTION_DEBUG_VIEWS,
        &mut view,
    ) {
        actions.push(WorldAction::SetReflectionDebugView(normalize_debug_view(
            view,
        )));
    }
}

fn shadows_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        i18n::tr(
            locale,
            "El nivel de sombras define resolución y suavizado.",
            "Shadow tier sets resolution and softness.",
        ),
    );
    widgets::caption(ui, i18n::tr(locale, "Nivel de sombras", "Shadow tier"));

    if let Some(index) = tier_choice(
        ui,
        locale,
        &ShadowTier::ALL,
        world.shadow_tier,
        ShadowTier::as_str,
        i18n::tr(locale, "Nivel de sombras", "Shadow tier"),
    ) {
        actions.push(WorldAction::SetShadowTier(ShadowTier::ALL[index]));
    }

    widgets::label_value(
        ui,
        i18n::tr(locale, "Oscuridad de sombra", "Shadow darkness"),
        &format!("{:.2}", world.shadow_darkness),
    );
    let mut darkness = world.shadow_darkness;
    if widgets::range_slider(ui, &mut darkness, 0.02, 0.85, 0.01, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetLight {
            ambient: None,
            intensity: None,
            shadow_darkness: Some(darkness),
        });
    }
}

fn msaa_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        i18n::tr(
            locale,
            "Más muestras suavizan bordes a costa de rendimiento.",
            "More samples smooth edges at the cost of performance.",
        ),
    );
    widgets::caption(ui, i18n::tr(locale, "Nivel MSAA", "MSAA tier"));

    if let Some(index) = tier_choice(
        ui,
        locale,
        &MsaaTier::ALL,
        world.msaa_tier,
        MsaaTier::as_str,
        i18n::tr(locale, "Nivel MSAA", "MSAA tier"),
    ) {
        actions.push(WorldAction::SetMsaaTier(MsaaTier::ALL[index]));
    }
}

fn taa_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::hint(
        ui,
        i18n::tr(
            locale,
            "El antialiasing temporal suaviza bordes y estabiliza las sombras entre frames.",
            "Temporal antialiasing smooths edges and stabilizes shadows across frames.",
        ),
    );
    widgets::caption(ui, "TAA");

    let options = [
        (i18n::tr(locale, "On", "On"), world.taa_enabled),
        (i18n::tr(locale, "Off", "Off"), !world.taa_enabled),
    ];
    if let Some(index) = widgets::tier_row_choice(ui, &options, i18n::tr(locale, "TAA", "TAA")) {
        actions.push(WorldAction::SetTaa {
            enabled: index == 0,
            blend: world.taa_blend,
            jitter_scale: world.taa_jitter_scale,
        });
    }
    ui.add_space(4.0);

    widgets::label_value(
        ui,
        i18n::tr(locale, "Mezcla", "Blend"),
        &format!("{:.2}", world.taa_blend),
    );
    let mut blend = world.taa_blend;
    if widgets::range_slider(ui, &mut blend, 0.0, 1.0, 0.01, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetTaa {
            enabled: world.taa_enabled,
            blend,
            jitter_scale: world.taa_jitter_scale,
        });
    }
    widgets::range_end_labels(
        ui,
        i18n::tr(locale, "Nítido", "Sharp"),
        i18n::tr(locale, "Suave", "Smooth"),
    );
    ui.add_space(4.0);

    widgets::label_value(ui, "Jitter", &format!("{:.2}", world.taa_jitter_scale));
    let mut jitter = world.taa_jitter_scale;
    if widgets::range_slider(ui, &mut jitter, 0.0, 2.0, 0.05, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetTaa {
            enabled: world.taa_enabled,
            blend: world.taa_blend,
            jitter_scale: jitter,
        });
    }
    widgets::range_end_labels(
        ui,
        i18n::tr(locale, "Menos", "Less"),
        i18n::tr(locale, "Más", "More"),
    );
}

fn gravity_body(
    ui: &mut egui::Ui,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::label_value(
        ui,
        i18n::tr(locale, "Gravedad", "Gravity"),
        &format!("{:.2} u/s²", world.gravity),
    );
    let mut gravity = world.gravity;
    if widgets::range_slider(ui, &mut gravity, 0.0, 50.0, 0.5, theme::CONTROL_BLUE) {
        actions.push(WorldAction::SetGravity(gravity));
    }
}

fn fps_body(
    ui: &mut egui::Ui,
    selection: &mut u32,
    dirty: &mut bool,
    world: WorldSettings,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    let mut options = vec![60u32, 120, 144, 240];
    if !options.contains(selection) {
        options.push(*selection);
    }

    let width = (ui.available_width() - 34.0).max(0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if widgets::combo_u32(ui, "editor_target_fps", width, &options, selection) {
            *dirty = true;
        }
        if widgets::small_icon_button_click(
            ui,
            icons::CHECK,
            i18n::tr(locale, "Aplicar límite de FPS", "Apply FPS limit"),
        ) {
            actions.push(WorldAction::SetTargetFps(*selection));
            *dirty = false;
        }
    });

    widgets::tiny(
        ui,
        &format!(
            "{} {}",
            i18n::tr(locale, "Actual:", "Current:"),
            world.target_fps
        ),
    );
}

fn tier_choice<T: Copy + PartialEq, F: Fn(T) -> &'static str>(
    ui: &mut egui::Ui,
    locale: Locale,
    all: &[T],
    active: T,
    key: F,
    hint: &str,
) -> Option<usize> {
    let options: Vec<(&'static str, bool)> = all
        .iter()
        .map(|tier| (tier_label(locale, key(*tier)), *tier == active))
        .collect();
    widgets::tier_row_choice(ui, &options, hint)
}

fn tier_label(locale: Locale, key: &'static str) -> &'static str {
    match key {
        "off" => i18n::tr(locale, "Off", "Off"),
        "low" => i18n::tr(locale, "Bajo", "Low"),
        "medium" => i18n::tr(locale, "Medio", "Medium"),
        "high" => i18n::tr(locale, "Alto", "High"),
        "ultra" => "Ultra",
        _ => key,
    }
}

fn apply_action(app: &mut EditorApp, action: WorldAction) {
    match action {
        WorldAction::SetSize { width, height } => {
            app.world.world_width = width;
            app.world.world_height = height;
            app.send_engine(json!({
                "cmd": "set_world_size",
                "width": width,
                "height": height,
            }));
        }
        WorldAction::SetRadius(radius) => {
            let radius = radius.clamp(5.0, 500.0);
            app.world.world_radius = radius;
            app.world.world_width = radius * 2.0;
            app.world.world_height = radius * 2.0;
            app.world.world_depth = radius * 2.0;
            app.send_engine(json!({ "cmd": "set_world_radius", "radius": radius }));
        }
        WorldAction::SetGridVisible(visible) => {
            app.world.grid_visible = visible;
            app.send_engine(json!({ "cmd": "set_grid_visible", "visible": visible }));
        }
        WorldAction::SetGridCellSize(size) => {
            app.world.grid_cell_size = size;
            app.send_engine(json!({ "cmd": "set_grid_cell_size", "size": size }));
        }
        WorldAction::SetGravity(gravity) => {
            app.world.gravity = gravity;
            app.send_engine(json!({ "cmd": "set_gravity", "gravity": gravity }));
        }
        WorldAction::SetTargetFps(fps) => {
            app.world.target_fps = fps;
            app.send_engine(json!({ "cmd": "set_target_fps", "fps": fps }));
            app.push_log(format!("[Rendimiento] Límite de FPS: {fps}"), false);
        }
        WorldAction::SetLight {
            ambient,
            intensity,
            shadow_darkness,
        } => {
            if let Some(value) = ambient {
                app.world.light_ambient = value;
            }
            if let Some(value) = intensity {
                app.world.light_intensity = value;
            }
            if let Some(value) = shadow_darkness {
                app.world.shadow_darkness = value;
            }
            app.send_engine(WorldSettings::directional_light_command(
                ambient,
                intensity,
                shadow_darkness,
            ));
        }
        WorldAction::SetTextureTier(tier) => {
            app.world.graphics_texture_tier = tier;
            app.send_engine(json!({ "cmd": "set_graphics_texture_tier", "tier": tier.as_str() }));
            app.push_log(format!("[Texturas] Nivel: {}", tier.as_str()), false);
        }
        WorldAction::SetTextureDetailDistance(distance) => {
            app.world.texture_detail_distance = distance;
            app.send_engine(
                json!({ "cmd": "set_texture_detail_distance", "distance_m": distance }),
            );
        }
        WorldAction::SetReflectionTier(tier) => {
            app.world.reflection_tier = tier;
            app.world.reflection_tier_effective = None;
            app.send_engine(json!({ "cmd": "set_reflection_tier", "tier": tier.as_str() }));
            app.push_log(
                format!("[Reflejos] Nivel solicitado: {}", tier.as_str()),
                false,
            );
        }
        WorldAction::SetReflectionRaytracing(enabled) => {
            app.world.reflection_raytracing = enabled;
            app.send_engine(json!({ "cmd": "set_reflection_raytracing", "enabled": enabled }));
            app.push_log(
                format!("[Reflejos] Ray tracing: {}", on_off(enabled)),
                false,
            );
        }
        WorldAction::SetReflectionProbes(enabled) => {
            app.world.reflection_probes = enabled;
            app.send_engine(json!({ "cmd": "set_reflection_probes", "enabled": enabled }));
            app.push_log(format!("[Reflejos] Probes: {}", on_off(enabled)), false);
        }
        WorldAction::SpawnReflectionProbe => {
            app.send_engine(json!({ "cmd": "spawn_reflection_probe" }));
            app.push_log("[Reflejos] Insertando sonda de reflexión…", false);
        }
        WorldAction::SetReflectionDebugView(view) => {
            app.world.reflection_debug_view = view;
            app.send_engine(json!({ "cmd": "set_reflection_debug_view", "view": view }));
            app.push_log(format!("[Reflejos] Vista debug: {view}"), false);
        }
        WorldAction::SetShadowTier(tier) => {
            app.world.shadow_tier = tier;
            app.send_engine(json!({ "cmd": "set_shadow_tier", "tier": tier.as_str() }));
            app.push_log(format!("[Sombras] Nivel: {}", tier.as_str()), false);
        }
        WorldAction::SetMsaaTier(tier) => {
            app.world.msaa_tier = tier;
            app.send_engine(json!({ "cmd": "set_msaa_tier", "tier": tier.as_str() }));
            app.push_log(format!("[MSAA] Nivel: {}", tier.as_str()), false);
        }
        WorldAction::SetTaa {
            enabled,
            blend,
            jitter_scale,
        } => {
            app.world.taa_enabled = enabled;
            app.world.taa_blend = blend;
            app.world.taa_jitter_scale = jitter_scale;
            app.send_engine(json!({
                "cmd": "set_taa",
                "enabled": enabled,
                "blend": blend,
                "jitter_scale": jitter_scale,
            }));
        }
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

const METRIC_GAP: f32 = 3.5;
const METRIC_FONT_SIZE: f32 = 12.0;
const METRIC_LINE_HEIGHT: f32 = 18.6;

fn metrics(ui: &mut egui::Ui, app: &EditorApp) {
    let locale = app.cli.locale;

    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(i18n::tr(locale, "Métricas de uso", "Usage metrics"))
                .size(13.0)
                .strong()
                .color(theme::TEXT_MUTED),
        );
    });
    ui.add_space(7.0);

    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(1.0, theme::METRICS_BORDER),
    );
    ui.add_space(7.0);

    let Some(metrics) = app.metrics else {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(i18n::tr(
                    locale,
                    "Esperando métricas…",
                    "Waiting for metrics...",
                ))
                .size(12.0)
                .color(theme::TEXT_MUTED),
            );
        });
        return;
    };

    let width = ui.available_width();
    let col_left = width * 5.0 / 12.0;
    let col_right = (width - col_left - METRIC_GAP).max(0.0);

    egui::Grid::new("editor_right_metrics_grid")
        .num_columns(2)
        .spacing([METRIC_GAP, 0.0])
        .show(ui, |ui| {
            metric_cell(
                ui,
                col_left,
                "FPS",
                &format!("{:.1}", metrics.fps),
                Some(fps_color(metrics.fps)),
            );
            metric_cell(
                ui,
                col_right,
                "Frame Time",
                &format!("{:.1} ms", metrics.frame_time_ms),
                None,
            );
            ui.end_row();

            metric_cell(
                ui,
                col_left,
                "Draw Calls",
                &metrics.draw_calls.to_string(),
                None,
            );
            metric_cell(
                ui,
                col_right,
                "Physics Bodies",
                &metrics.physics_bodies.to_string(),
                None,
            );
            ui.end_row();

            metric_cell(
                ui,
                col_left,
                "CPU",
                &format_percent(metrics.cpu_percent),
                None,
            );
            metric_cell(
                ui,
                col_right,
                "GPU",
                &format_percent(metrics.gpu_percent),
                None,
            );
            ui.end_row();
        });
}

fn metric_cell(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    value: &str,
    value_color: Option<egui::Color32>,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, METRIC_LINE_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.x = METRIC_GAP;
            ui.label(
                RichText::new(label)
                    .monospace()
                    .size(METRIC_FONT_SIZE)
                    .color(theme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(value)
                    .monospace()
                    .size(METRIC_FONT_SIZE)
                    .color(value_color.unwrap_or(theme::TEXT)),
            );
        },
    );
}

fn format_percent(value: Option<f32>) -> String {
    match value {
        Some(value) if value.is_finite() => format!("{value:.1}%"),
        _ => "—".to_string(),
    }
}

fn fps_color(fps: f32) -> egui::Color32 {
    if fps >= 55.0 {
        theme::METRIC_FPS_GOOD
    } else if fps >= 30.0 {
        theme::METRIC_FPS_WARN
    } else {
        theme::METRIC_FPS_BAD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_one_accordion_at_a_time() {
        let mut state = SidebarRightState::default();

        state.toggle("world_workspace");
        assert_eq!(state.open, Some("world_workspace"));

        state.toggle("world_grid");
        assert_eq!(state.open, Some("world_grid"));

        state.toggle("world_grid");
        assert_eq!(state.open, None);
    }

    #[test]
    fn fps_colors_follow_react_thresholds() {
        assert_eq!(fps_color(60.0), theme::METRIC_FPS_GOOD);
        assert_eq!(fps_color(55.0), theme::METRIC_FPS_GOOD);
        assert_eq!(fps_color(45.0), theme::METRIC_FPS_WARN);
        assert_eq!(fps_color(30.0), theme::METRIC_FPS_WARN);
        assert_eq!(fps_color(20.0), theme::METRIC_FPS_BAD);
    }

    #[test]
    fn percent_format_matches_react() {
        assert_eq!(format_percent(Some(12.34)), "12.3%");
        assert_eq!(format_percent(Some(0.0)), "0.0%");
        assert_eq!(format_percent(None), "—");
        assert_eq!(format_percent(Some(f32::NAN)), "—");
    }

    #[test]
    fn tier_labels_follow_locale() {
        assert_eq!(tier_label(Locale::Es, "low"), "Bajo");
        assert_eq!(tier_label(Locale::En, "low"), "Low");
        assert_eq!(tier_label(Locale::Es, "ultra"), "Ultra");
    }
}
