//! Barra lateral derecha: configuración del mundo funcional y métricas.
//!
//! Los controles envían comandos `set_*` directamente al motor (el editor es su
//! dueño). Los valores se sincronizan desde `scene.world` del snapshot y desde
//! los eventos de confirmación, de modo que React y egui no se desalinean.

use egui::{Align, Layout, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::{EditorApp, EditorStatus};
use crate::editor::cli::Locale;
use crate::editor::context::ProjectContext;
use crate::editor::world::{
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
    /// Fondo 2D activo: `Some(path)` carga la imagen, `None` lo quita.
    SetBackground(Option<String>),
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
    /// LOD de modelos: `None` = mantener el valor actual (actualizaciones parciales).
    SetModelLod {
        enabled: Option<bool>,
        lod1_distance: Option<f32>,
        lod2_distance: Option<f32>,
    },
}

mod fps;
mod gravity;
mod grid;
mod lighting;
mod lod;
mod metrics;
mod msaa;
mod reflections;
mod shadows;
mod taa;
mod textures;
mod tiers;
mod workspace;
use self::fps::*;
use self::gravity::*;
use self::grid::*;
use self::lighting::*;
use self::lod::*;
use self::metrics::*;
use self::msaa::*;
use self::reflections::*;
use self::shadows::*;
use self::taa::*;
use self::textures::*;
use self::tiers::*;
use self::workspace::*;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let context = app.cli.context;
    let enabled = app.status == EditorStatus::Ready;
    let world = app.world;
    let backgrounds = app.assets.backgrounds.clone();
    let background_path = app.background_path.clone();
    let mut actions = Vec::new();

    // Métricas fijas al fondo (React: .metrics-panel, 120 px).
    egui::Panel::bottom("editor_right_metrics")
        .exact_size(120.0)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(crate::editor::theme::METRICS_BG))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.x_range(),
                rect.top(),
                egui::Stroke::new(1.0, crate::editor::theme::METRICS_BORDER),
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
                            RichText::new(crate::editor::i18n::tr(
                                locale,
                                "CONFIGURACIÓN DEL MUNDO",
                                "WORLD SETTINGS",
                            ))
                            .size(14.0)
                            .strong()
                            .color(crate::editor::theme::TEXT_MUTED),
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
                        crate::editor::i18n::tr(locale, "Espacio de trabajo", "Workspace"),
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
                            crate::editor::i18n::tr(locale, "Fondo del mundo", "World background"),
                            enabled,
                            |ui| {
                                background_body(
                                    ui,
                                    locale,
                                    crate::editor::i18n::tr(
                                        locale,
                                        "Selecciona la imagen de fondo del mundo",
                                        "Select the world background image",
                                    ),
                                    &backgrounds,
                                    background_path.as_deref(),
                                    &mut actions,
                                )
                            },
                        );
                    }

                    sub(
                        ui,
                        open,
                        "world_grid",
                        icons::GRID_FOUR,
                        crate::editor::i18n::tr(locale, "Rejilla", "Grid"),
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
                            crate::editor::i18n::tr(
                                locale,
                                "Sol e iluminación",
                                "Sun and lighting",
                            ),
                            enabled,
                            |ui| lighting_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_textures",
                            icons::IMAGE,
                            crate::editor::i18n::tr(locale, "Texturas GLB", "GLB textures"),
                            enabled,
                            |ui| textures_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_reflections",
                            icons::SPARKLE,
                            crate::editor::i18n::tr(locale, "Reflejos", "Reflections"),
                            enabled,
                            |ui| reflections_body(ui, world, locale, &mut actions),
                        );
                        sub(
                            ui,
                            open,
                            "world_shadows",
                            icons::MOON,
                            crate::editor::i18n::tr(locale, "Sombras", "Shadows"),
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
                        sub(
                            ui,
                            open,
                            "world_lod",
                            icons::STACK,
                            "LOD",
                            enabled,
                            |ui| lod_body(ui, world, locale, &mut actions),
                        );
                    }

                    sub(
                        ui,
                        open,
                        "world_gravity",
                        icons::ARROW_CIRCLE_DOWN,
                        crate::editor::i18n::tr(locale, "Gravedad", "Gravity"),
                        enabled,
                        |ui| gravity_body(ui, world, locale, &mut actions),
                    );
                    sub(
                        ui,
                        open,
                        "world_fps",
                        icons::SPEEDOMETER,
                        crate::editor::i18n::tr(locale, "Límite de FPS", "FPS limit"),
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
        WorldAction::SetBackground(path) => {
            match path.as_deref() {
                Some(path) => {
                    app.send_engine(json!({ "cmd": "load_background", "path": path }));
                    app.push_log(format!("[Fondo] Aplicando {path}…"), false);
                }
                None => {
                    app.send_engine(json!({ "cmd": "clear_background" }));
                    app.push_log("[Fondo] Fondo eliminado".to_string(), false);
                }
            }
            app.background_path = path;
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
            app.world.msaa_sample_count = tier.desired_sample_count();
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
        WorldAction::SetModelLod {
            enabled,
            lod1_distance,
            lod2_distance,
        } => {
            if let Some(value) = enabled {
                app.world.model_lod_enabled = value;
            }
            if let Some(value) = lod1_distance {
                app.world.model_lod1_distance = value;
            }
            if let Some(value) = lod2_distance {
                app.world.model_lod2_distance = value;
            }
            app.send_engine(json!({
                "cmd": "set_model_lod",
                "enabled": app.world.model_lod_enabled,
                "lod1_distance_m": app.world.model_lod1_distance,
                "lod2_distance_m": app.world.model_lod2_distance,
            }));
            app.push_log(
                format!(
                    "[LOD] {} · LOD1 {:.0} m · LOD2 {:.0} m",
                    on_off(app.world.model_lod_enabled),
                    app.world.model_lod1_distance,
                    app.world.model_lod2_distance,
                ),
                false,
            );
        }
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

const METRIC_GAP: f32 = 3.5;
const METRIC_FONT_SIZE: f32 = 12.0;
const METRIC_LINE_HEIGHT: f32 = 18.6;

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
        assert_eq!(fps_color(60.0), crate::editor::theme::METRIC_FPS_GOOD);
        assert_eq!(fps_color(55.0), crate::editor::theme::METRIC_FPS_GOOD);
        assert_eq!(fps_color(45.0), crate::editor::theme::METRIC_FPS_WARN);
        assert_eq!(fps_color(30.0), crate::editor::theme::METRIC_FPS_WARN);
        assert_eq!(fps_color(20.0), crate::editor::theme::METRIC_FPS_BAD);
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
