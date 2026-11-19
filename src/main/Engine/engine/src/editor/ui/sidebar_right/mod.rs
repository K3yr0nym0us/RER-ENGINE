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
    MsaaTier, REFLECTION_DEBUG_VIEWS, ReflectionTier, ShadowTier, SsaaTier, TextureTier,
    WorldSettings, normalize_debug_view,
};

use super::number_draft::NumberDraft;
use super::{accordion, widgets};

#[derive(Default)]
pub struct SidebarRightState {
    open: Option<&'static str>,
    grid_locked: bool,
    width: NumberDraft,
    height: NumberDraft,
    north: NumberDraft,
    south: NumberDraft,
    east: NumberDraft,
    west: NumberDraft,
    grid_cell: NumberDraft,
    /// Duración del día / la noche en unidades UI (1000 cada uno = 2000).
    sun_cycle_day: NumberDraft,
    sun_cycle_night: NumberDraft,
    fps_selection: u32,
    fps_dirty: bool,
    /// Partículas GPU-driven (F5): clima + emisores de escena gestionados desde la UI.
    pub particles: particles::ParticlesUiState,
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
    /// Caras del mundo (m): norte (-Z), sur (+Z), este (+X), oeste (-X).
    SetBounds {
        north: f32,
        south: f32,
        east: f32,
        west: f32,
    },
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
        sun_cycle_enabled: Option<bool>,
        sun_rise_cardinal: Option<&'static str>,
        sun_cycle_day: Option<f32>,
        sun_cycle_night: Option<f32>,
        sun_cycle_phase: Option<f32>,
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
    SetSsaaTier(SsaaTier),
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
    /// Partículas (F5/F6-bis): clima global del dropdown (None = ninguno), con
    /// multiplicador de velocidad de caída y viento X (lluvia/nieve).
    SetParticleWeather {
        kind: Option<&'static str>,
        intensity: f32,
        fall: f32,
        wind: f32,
    },
    /// Partículas (F5): baja de un emisor de escena creado por la UI.
    RemoveParticleEmitter(u64),
    /// Cubo de líquidos (F5): baja de una fuente creada desde el sidebar.
    RemoveWaterSource(u64),
}

mod fps;
mod gravity;
mod grid;
mod lighting;
mod lod;
mod msaa;
/// Partículas GPU-driven (F5): dropdown de clima + emisores de escena.
pub(crate) mod particles;
mod reflections;
mod shadows;
mod ssaa;
mod taa;
mod textures;
mod tiers;
mod workspace;
use self::fps::*;
use self::gravity::*;
use self::grid::*;
use self::lighting::*;
use self::lod::*;
use self::msaa::*;
use self::particles::*;
use self::reflections::*;
use self::shadows::*;
use self::ssaa::*;
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

    // Las métricas de uso viven ahora en el viewport del motor
    // (`ui::metrics_overlay`), no en un panel fijo de esta barra lateral.

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
                        north,
                        south,
                        east,
                        west,
                        grid_cell,
                        sun_cycle_day,
                        sun_cycle_night,
                        fps_selection,
                        fps_dirty,
                        particles,
                    } = &mut app.right_sidebar;

                    // Sincroniza los borradores con el estado del motor.
                    width.sync(world.world_width);
                    height.sync(world.world_height);
                    north.sync(world.north);
                    south.sync(world.south);
                    east.sync(world.east);
                    west.sync(world.west);
                    grid_cell.sync(world.grid_cell_size);
                    sun_cycle_day.sync(world.sun_cycle_day);
                    sun_cycle_night.sync(world.sun_cycle_night);
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
                                workspace_3d(
                                    ui,
                                    north,
                                    south,
                                    east,
                                    west,
                                    world,
                                    locale,
                                    &mut actions,
                                );
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
                            |ui| {
                                lighting_body(
                                    ui,
                                    world,
                                    locale,
                                    sun_cycle_day,
                                    sun_cycle_night,
                                    &mut actions,
                                )
                            },
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
                            "world_ssaa",
                            icons::ARROWS_OUT,
                            "SSAA",
                            enabled,
                            |ui| ssaa_body(ui, world, locale, &mut actions),
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
                        sub(ui, open, "world_lod", icons::STACK, "LOD", enabled, |ui| {
                            lod_body(ui, world, locale, &mut actions)
                        });
                        sub(
                            ui,
                            open,
                            "world_particles",
                            icons::SPARKLE,
                            crate::editor::i18n::tr(
                                locale,
                                "Entorno y partículas",
                                "Environment & particles",
                            ),
                            enabled,
                            |ui| particles_body(ui, particles, locale, &mut actions),
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
        WorldAction::SetBounds {
            north,
            south,
            east,
            west,
        } => {
            app.world.set_faces(north, south, east, west);
            app.send_engine(json!({
                "cmd": "set_world_bounds",
                "north": app.world.north,
                "south": app.world.south,
                "east": app.world.east,
                "west": app.world.west,
                "height": app.world.world_height,
            }));
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
            sun_cycle_enabled,
            sun_rise_cardinal,
            sun_cycle_day,
            sun_cycle_night,
            sun_cycle_phase,
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
            if let Some(value) = sun_cycle_enabled {
                app.world.sun_cycle_enabled = value;
            }
            if let Some(value) = sun_rise_cardinal {
                app.world.sun_rise_cardinal = value;
            }
            if let Some(value) = sun_cycle_day {
                app.world.sun_cycle_day = value;
            }
            if let Some(value) = sun_cycle_night {
                app.world.sun_cycle_night = value;
            }
            if let Some(value) = sun_cycle_phase {
                app.world.sun_cycle_phase = value;
            }
            app.send_engine(WorldSettings::directional_light_command(
                ambient,
                intensity,
                shadow_darkness,
                sun_cycle_enabled,
                sun_rise_cardinal,
                sun_cycle_day,
                sun_cycle_night,
                sun_cycle_phase,
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
        WorldAction::SetSsaaTier(tier) => {
            app.world.ssaa_tier = tier;
            app.world.ssaa_scale = tier.desired_scale();
            app.send_engine(json!({ "cmd": "set_ssaa_tier", "tier": tier.as_str() }));
            app.push_log(format!("[SSAA] Nivel: {}", tier.as_str()), false);
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
        WorldAction::SetParticleWeather {
            kind,
            intensity,
            fall,
            wind,
        } => {
            // El estado ya lo fijó el body; aquí solo viaja el comando.
            let wire = kind.unwrap_or("none");
            app.send_engine(json!({
                "cmd": "set_particle_environment",
                "kind": wire,
                "intensity": intensity,
                "fall_speed": fall,
                "wind_x": wind,
            }));
            app.push_log(
                match kind {
                    Some("rain") => format!(
                        "[Clima] Lluvia (intensidad {intensity:.1}, caída ×{fall:.1}, viento {wind:+.1})"
                    ),
                    Some("snow") => format!(
                        "[Clima] Nieve (intensidad {intensity:.1}, caída ×{fall:.1}, viento {wind:+.1})"
                    ),
                    Some("fog") => format!("[Clima] Niebla (intensidad {intensity:.1})"),
                    _ => "[Clima] Desactivado".to_string(),
                },
                false,
            );
        }
        WorldAction::RemoveParticleEmitter(id) => {
            app.send_engine(json!({ "cmd": "remove_particle_emitter", "id": id }));
            app.right_sidebar
                .particles
                .emitters
                .retain(|row| row.id != id);
            app.push_log(format!("[Partículas] Emisor {id} eliminado"), false);
        }
        WorldAction::RemoveWaterSource(id) => {
            app.send_engine(json!({ "cmd": "remove_water_source", "id": id }));
            app.right_sidebar
                .particles
                .emitters
                .retain(|row| row.id != id);
            app.push_log(format!("[Agua] Fuente {id} eliminada"), false);
        }
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
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
    fn tier_labels_follow_locale() {
        assert_eq!(tier_label(Locale::Es, "low"), "Bajo");
        assert_eq!(tier_label(Locale::En, "low"), "Low");
        assert_eq!(tier_label(Locale::Es, "ultra"), "Ultra");
    }
}
