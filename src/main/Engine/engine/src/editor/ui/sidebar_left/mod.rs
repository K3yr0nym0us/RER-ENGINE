//! Barra lateral izquierda: escenas funcionales, cámara y secciones visuales.

use egui::{Color32, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::{ControlSummary, EditorApp, EditorStatus, PlayView};
use crate::editor::assets::{AssetKind, AssetLists};
use crate::editor::cli::Locale;
use crate::editor::context::{GameStyle, ProjectContext};
use crate::editor::scenes::SceneList;
use crate::editor::world::Camera2d;

use super::number_draft::NumberDraft;
use super::{accordion, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Scenes,
    Camera,
    Resources,
    Entities,
    UserInterface,
    Tools,
    WorldGen,
    Controls,
}

impl Section {
    fn id(self) -> &'static str {
        match self {
            Self::Scenes => "scenes",
            Self::Camera => "camera",
            Self::Resources => "resources",
            Self::Entities => "entities",
            Self::UserInterface => "ui",
            Self::Tools => "tools",
            Self::WorldGen => "worldgen",
            Self::Controls => "controls",
        }
    }
}

#[derive(Default)]
pub struct SidebarLeftState {
    open_section: Option<Section>,
    open_sub: Option<&'static str>,
    cam_x: NumberDraft,
    cam_y: NumberDraft,
    cam_zoom: NumberDraft,
    cam3_x: NumberDraft,
    cam3_y: NumberDraft,
    cam3_z: NumberDraft,
    cam3_yaw: NumberDraft,
    cam3_fov: NumberDraft,
    cam3_frustum: NumberDraft,
    selected_sprite: Option<String>,
    selected_model: Option<String>,
    pub(crate) active_tool: Option<&'static str>,
    pub(crate) active_ui_screen: Option<String>,
    pub(crate) worldgen: WorldGenPanelState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityKind {
    Character,
    Environment,
    Object,
    Weapon,
    Projectile,
}

impl EntityKind {
    fn id(self) -> &'static str {
        match self {
            Self::Character => "ent_characters",
            Self::Environment => "ent_environment",
            Self::Object => "ent_objects",
            Self::Weapon => "ent_weapons",
            Self::Projectile => "ent_projectiles",
        }
    }
}

/// Acción pendiente de aplicar al motor tras dibujar la UI.
enum LeftAction {
    OpenCreateScene,
    OpenDeleteScene(u64),
    SwitchScene(u64),
    SetCamera2d {
        x: f32,
        y: f32,
        half_h: f32,
    },
    SetPlayCameraAxis {
        axis: u8,
        value: f32,
    },
    SetPlayCameraYaw(f32),
    SetPlayCameraFov(f32),
    SetPlayCameraFrustum(f32),
    OpenSockets,
    OpenLegacyWindow(&'static str),
    /// Abre la modal React de bindings para `keyboard_mouse` o `gamepad`.
    OpenControlBindings(&'static str),
    /// Crea una pantalla HUD/menú y la registra en React.
    CreateUiScreen(&'static str),
    /// Abre el editor de una pantalla HUD o el rename de un menú.
    OpenUiEditor {
        scope: &'static str,
        screen_id: String,
    },
    /// Elimina una pantalla HUD/menú (con confirmación en React).
    DeleteUiScreen {
        scope: &'static str,
        id: String,
        name: String,
    },
    /// Activa/desactiva la pantalla HUD que se muestra en play.
    SetActiveUiScreen(Option<String>),
    ImportAsset(AssetKind),
    OpenRemoveAsset {
        kind: AssetKind,
        path: String,
        name: String,
    },
    CreateEntity {
        kind: EntityKind,
        asset: String,
    },
    SetActiveTool(&'static str),
    /// Abre o desactiva la construcción rápida por blueprint.
    QuickBuild {
        active: bool,
    },
    /// Plugin worldgen: registra la DLL instalada en el engine.
    WorldGenSetLibraryPath(String),
    /// Plugin worldgen: aplica config y lanza la generación.
    WorldGenGenerate {
        config_json: String,
    },
    /// Plugin worldgen: cancela la cola de chunks.
    WorldGenStop,
    /// Plugin worldgen: limpia el mundo de la escena.
    WorldGenReset,
    /// Plugin worldgen: distribuye un modelo del proyecto sobre el terreno.
    WorldGenScatter {
        blueprint_id: String,
        model: String,
        rules_json: String,
    },
}

impl SidebarLeftState {
    fn toggle_section(&mut self, section: Section) {
        if self.open_section == Some(section) {
            self.open_section = None;
        } else {
            self.open_section = Some(section);
            self.open_sub = None;
        }
    }

    #[cfg(test)]
    fn toggle_sub(&mut self, id: &'static str) {
        self.open_sub = if self.open_sub == Some(id) {
            None
        } else {
            Some(id)
        };
    }
}

mod camera;
mod controls;
mod entities;
mod interface;
mod resources;
mod scenes;
mod tools;
mod worldgen;
use self::camera::*;
use self::controls::*;
use self::entities::*;
use self::interface::*;
use self::resources::*;
use self::scenes::*;
use self::tools::*;
use self::worldgen::*;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let context = app.cli.context;
    let phase_note = app.text().phase_note;
    let scenes = app.scenes.clone();
    let camera = app.world.camera2d;
    let play_view = app.play_view;
    let ready = app.status == EditorStatus::Ready;
    let assets = app.assets.clone();
    let ui_screens = app.ui_screens.clone();
    let quick_build_active = app.quick_build_active.clone();
    let controls = app.player_controls.clone();
    let mut actions = Vec::new();

    let mut next_locale = None;
    egui::Panel::bottom("editor_left_sidebar_footer")
        .exact_size(96.0)
        .show_separator_line(true)
        .frame(
            egui::Frame::new()
                .fill(crate::editor::theme::SIDEBAR_BG)
                .inner_margin(egui::Margin::symmetric(4, 8)),
        )
        .show(ui, |ui| {
            next_locale = footer(ui, locale, phase_note, &mut actions);
        });
    if let Some(next) = next_locale {
        app.cli.locale = next;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(4, 8))
                .show(ui, |ui| {
                    let state = &mut app.left_sidebar;

                    section(
                        ui,
                        state,
                        Section::Scenes,
                        icons::STACK,
                        crate::editor::i18n::tr(locale, "Escenas", "Scenes"),
                        |ui, state| {
                            scenes_body(
                                ui,
                                state,
                                &scenes,
                                context,
                                locale,
                                phase_note,
                                &mut actions,
                            )
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::Camera,
                        icons::VIDEO_CAMERA,
                        crate::editor::i18n::tr(locale, "Cámara", "Camera"),
                        |ui, state| {
                            camera_body(
                                ui,
                                state,
                                context,
                                camera,
                                play_view,
                                ready,
                                locale,
                                phase_note,
                                &mut actions,
                            )
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::Resources,
                        icons::PACKAGE,
                        crate::editor::i18n::tr(locale, "Recursos", "Resources"),
                        |ui, state| {
                            resources_body(
                                ui,
                                state,
                                &assets,
                                context,
                                locale,
                                phase_note,
                                &mut actions,
                            )
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::Entities,
                        icons::USERS,
                        crate::editor::i18n::tr(locale, "Crear entidad", "Create entity"),
                        |ui, state| {
                            entities_body(
                                ui,
                                state,
                                &assets,
                                context,
                                locale,
                                phase_note,
                                &mut actions,
                            )
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::UserInterface,
                        icons::BROWSER,
                        crate::editor::i18n::tr(locale, "Interfaz de usuario", "User interface"),
                        |ui, state| {
                            ui_body(ui, state, &ui_screens, locale, phase_note, &mut actions)
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::Tools,
                        icons::WRENCH,
                        crate::editor::i18n::tr(locale, "Herramientas", "Tools"),
                        |ui, state| {
                            tools_body(
                                ui,
                                state,
                                context.is_3d(),
                                locale,
                                phase_note,
                                quick_build_active.as_deref(),
                                &mut actions,
                            )
                        },
                    );
                    if context.is_3d() {
                        section(
                            ui,
                            state,
                            Section::WorldGen,
                            icons::GLOBE,
                            crate::editor::i18n::tr(
                                locale,
                                "Generador de mundos",
                                "World Generator",
                            ),
                            |ui, state| {
                                worldgen_body(
                                    ui,
                                    state,
                                    &assets.models,
                                    locale,
                                    phase_note,
                                    &mut actions,
                                )
                            },
                        );
                    }
                    section(
                        ui,
                        state,
                        Section::Controls,
                        icons::GAME_CONTROLLER,
                        crate::editor::i18n::tr(locale, "Controles", "Controls"),
                        |ui, _state| {
                            controls_body(ui, locale, phase_note, controls.as_ref(), &mut actions)
                        },
                    );
                });
        });

    for action in actions {
        apply_action(app, action);
    }
}

fn section(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    section: Section,
    icon: &str,
    title: &str,
    body: impl FnOnce(&mut egui::Ui, &mut SidebarLeftState),
) {
    let is_open = state.open_section == Some(section);
    let response = accordion::item(
        ui,
        egui::Id::new(("editor_acc_section", section.id())),
        0,
        is_open,
        |ui, openness| accordion::header(ui, icon, title, is_open, openness, 0),
        |ui| body(ui, state),
    );

    if response.clicked() {
        state.toggle_section(section);
    }

    ui.add_space(2.0);
}

fn sub(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    id: &'static str,
    icon: &str,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    sub_at(ui, &mut state.open_sub, id, icon, title, body);
}

fn sub_at(
    ui: &mut egui::Ui,
    open: &mut Option<&'static str>,
    id: &'static str,
    icon: &str,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    let is_open = *open == Some(id);
    let response = accordion::item(
        ui,
        egui::Id::new(("editor_acc_sub", id)),
        1,
        is_open,
        |ui, openness| accordion::header(ui, icon, title, is_open, openness, 1),
        body,
    );

    if response.clicked() {
        *open = if is_open { None } else { Some(id) };
    }

    ui.add_space(2.0);
}

fn apply_action(app: &mut EditorApp, action: LeftAction) {
    match action {
        LeftAction::OpenCreateScene => {
            crate::editor::bridge::request_legacy_window("create_scene", json!({}));
        }
        LeftAction::OpenDeleteScene(id) => {
            crate::editor::bridge::request_legacy_window("delete_scene", json!({ "scene_id": id }));
        }
        LeftAction::SwitchScene(id) => {
            let name = app.scenes.name_for(id);
            app.scenes.begin_switch(id);
            app.send_engine(json!({ "cmd": "switch_editor_scene", "scene_id": id }));
            app.push_log(format!("[Escenas] Cargando «{name}»…"), false);
        }
        LeftAction::SetCamera2d { x, y, half_h } => {
            app.world.camera2d = Some(Camera2d { x, y, half_h });
            app.send_engine(json!({ "cmd": "set_camera_2d", "x": x, "y": y, "half_h": half_h }));
        }
        LeftAction::SetPlayCameraAxis { axis, value } => {
            app.send_engine(json!({
                "cmd": "set_play_character_view",
                "camera_only": true,
                "position_axis": { "axis": axis, "value": value },
            }));
        }
        LeftAction::SetPlayCameraYaw(yaw) => {
            app.send_engine(json!({
                "cmd": "set_play_character_view",
                "camera_only": true,
                "yaw": yaw,
            }));
        }
        LeftAction::SetPlayCameraFov(fov) => {
            app.send_engine(json!({
                "cmd": "set_play_character_view",
                "camera_only": true,
                "fov_y": fov,
            }));
        }
        LeftAction::SetPlayCameraFrustum(distance) => {
            app.send_engine(json!({
                "cmd": "set_play_character_view",
                "camera_only": true,
                "frustum_distance": distance,
            }));
        }
        LeftAction::OpenSockets => {
            crate::editor::bridge::request_legacy_window("socket_config", json!({}));
        }
        LeftAction::OpenLegacyWindow(kind) => {
            let scene_id = app.scenes.active_scene_id;
            crate::editor::bridge::request_legacy_window(kind, json!({ "scene_id": scene_id }));
            app.push_log(
                format!("[Editor] Abriendo ventana legacy «{kind}» (escena {scene_id})…"),
                false,
            );
        }
        LeftAction::OpenControlBindings(mode) => {
            crate::editor::bridge::request_legacy_window(
                "control_bindings",
                json!({ "mode": mode }),
            );
            app.push_log(
                format!("[Controles] Abriendo configuración ({mode})…"),
                false,
            );
        }
        LeftAction::CreateUiScreen(scope) => {
            crate::editor::bridge::request_legacy_window(
                "ui_create_screen",
                json!({ "scope": scope }),
            );
        }
        LeftAction::OpenUiEditor { scope, screen_id } => {
            crate::editor::bridge::request_legacy_window(
                "ui_edit_screen",
                json!({ "scope": scope, "screen_id": screen_id }),
            );
        }
        LeftAction::DeleteUiScreen { scope, id, name } => {
            crate::editor::bridge::request_legacy_window(
                "ui_delete_screen",
                json!({ "scope": scope, "screen_id": id, "name": name }),
            );
        }
        LeftAction::SetActiveUiScreen(screen_id) => {
            crate::editor::bridge::request_legacy_window(
                "ui_set_active_screen",
                json!({ "screen_id": screen_id }),
            );
        }
        LeftAction::ImportAsset(kind) => import_asset(app, kind),
        LeftAction::OpenRemoveAsset { kind, path, name } => {
            crate::editor::bridge::request_legacy_window(
                "remove_asset",
                json!({
                    "asset_kind": kind.legacy_id(),
                    "path": path,
                    "name": name,
                }),
            );
        }
        LeftAction::CreateEntity { kind, asset } => create_entity(app, kind, asset),
        LeftAction::SetActiveTool(tool) => {
            app.left_sidebar.active_tool = if tool.is_empty() { None } else { Some(tool) };
            app.send_engine(json!({ "cmd": "set_active_tool", "tool": tool }));
        }
        LeftAction::QuickBuild { active } => {
            let action = if active { "toggle" } else { "open" };
            crate::editor::bridge::request_legacy_window(
                "quick_build",
                json!({ "action": action }),
            );
            app.push_log(
                if active {
                    "[Quick build] Desactivando…"
                } else {
                    "[Quick build] Abriendo blueprints…"
                },
                false,
            );
        }
        LeftAction::WorldGenSetLibraryPath(path) => {
            app.send_engine(json!({
                "cmd": "world_gen_set_library_path",
                "path": path,
            }));
            app.push_log("[WorldGen] DLL del plugin registrada.", false);
        }
        LeftAction::WorldGenGenerate { config_json } => {
            if app.left_sidebar.worldgen.dll_path.is_none() {
                app.push_log(
                    "[WorldGen] Selecciona primero la DLL del plugin (botón «Cargar DLL»).",
                    true,
                );
                return;
            }
            // La DLL ya registrada no se vuelve a enviar (el engine la retiene),
            // pero re-enviarla es idempotente y cubre reinicios del mundo.
            if let Some(path) = app.left_sidebar.worldgen.dll_path.clone() {
                app.send_engine(json!({
                    "cmd": "world_gen_set_library_path",
                    "path": path,
                }));
            }
            app.send_engine(json!({
                "cmd": "world_gen_set_config",
                "config_json": config_json,
            }));
            app.send_engine(json!({ "cmd": "world_gen_start" }));
            app.push_log("[WorldGen] Generando mundo…", false);
        }
        LeftAction::WorldGenStop => {
            app.send_engine(json!({ "cmd": "world_gen_stop" }));
            app.push_log("[WorldGen] Generación cancelada.", false);
        }
        LeftAction::WorldGenReset => {
            app.send_engine(json!({ "cmd": "world_gen_reset" }));
            app.push_log("[WorldGen] Mundo eliminado de la escena.", false);
        }
        LeftAction::WorldGenScatter {
            blueprint_id,
            model,
            rules_json,
        } => {
            // Sintetiza el registro de blueprint para que el scatter use la vía
            // existente (caché de modelos compartida por todas las instancias).
            let name = std::path::Path::new(&model)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("modelo")
                .to_string();
            app.send_engine(json!({
                "cmd": "register_blueprint",
                "blueprint": {
                    "blueprint_id": blueprint_id,
                    "model": model,
                    "template_name": name,
                    "category": "object",
                    "colision": false,
                    "physics_type": "dynamic",
                },
            }));
            app.send_engine(json!({
                "cmd": "world_gen_scatter",
                "rules_json": rules_json,
            }));
            app.push_log("[WorldGen] Distribuyendo blueprints…", false);
        }
    }
}

fn import_asset(app: &mut EditorApp, kind: AssetKind) {
    let filter_name = match kind {
        AssetKind::Model => "Modelo 3D",
        AssetKind::Sprite | AssetKind::HudImage | AssetKind::Background => "Imagen",
        AssetKind::Sound => "Audio",
        AssetKind::Font => "Fuente",
    };
    let Some(path) = rfd::FileDialog::new()
        .add_filter(filter_name, kind.extensions())
        .pick_file()
    else {
        return;
    };
    let path_text = path.to_string_lossy().to_string();
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("recurso")
        .to_string();
    app.send_engine(json!({
        "cmd": kind.load_command(),
        "path": path_text,
        "name": name,
    }));
    app.push_log(format!("[Recursos] Importando «{name}»…"), false);
}

fn create_entity(app: &mut EditorApp, kind: EntityKind, asset: String) {
    if app.cli.context.is_3d() {
        let (entity_kind, category) = match kind {
            EntityKind::Character => ("character", "character"),
            EntityKind::Environment => ("scenario", "environment"),
            EntityKind::Object => ("model", "object"),
            EntityKind::Weapon => ("model", "weapon"),
            EntityKind::Projectile => ("projectile", "projectile"),
        };
        app.send_engine(json!({
            "cmd": "load_model",
            "path": asset,
            "single_instance": true,
            "kind": entity_kind,
            "entity_category": category,
        }));
    } else {
        let command = match kind {
            EntityKind::Character => "load_character",
            EntityKind::Environment | EntityKind::Object | EntityKind::Weapon => "load_scenario",
            EntityKind::Projectile => "load_projectile",
        };
        app.send_engine(json!({ "cmd": command, "path": asset }));
    }
    app.push_log(
        format!(
            "[Entidad] Creando {:?} desde el recurso seleccionado…",
            kind
        ),
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn footer(
    ui: &mut egui::Ui,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) -> Option<Locale> {
    let width = ui.available_width();
    if widgets::wide_button_click_sized(
        ui,
        width,
        icons::PUZZLE_PIECE,
        crate::editor::i18n::tr(locale, "Plugins", "Plugins"),
        hint,
        crate::editor::theme::OUTLINE_SECONDARY,
    ) {
        actions.push(LeftAction::OpenLegacyWindow("plugins"));
    }
    ui.add_space(2.0);

    let mut next = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let language_width = 38.0;
        let gap = 8.0;
        let guide_width = (ui.available_width() - language_width - gap).max(0.0);

        if widgets::wide_button_click_sized(
            ui,
            guide_width,
            icons::INFO,
            crate::editor::i18n::tr(locale, "Guia de usuario", "User guide"),
            hint,
            crate::editor::theme::INFO,
        ) {
            actions.push(LeftAction::OpenLegacyWindow("user_guide"));
        }
        ui.add_space(gap);

        let label = if locale == Locale::En { "ES" } else { "EN" };
        let tooltip = if locale == Locale::En {
            "Switch to Spanish"
        } else {
            "Cambiar a inglés"
        };

        if widgets::language_button(ui, language_width, label, tooltip) {
            next = Some(if locale == Locale::En {
                Locale::Es
            } else {
                Locale::En
            });
        }
    });

    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_toggles_open_and_closed() {
        let mut state = SidebarLeftState::default();

        state.toggle_section(Section::Scenes);
        assert_eq!(state.open_section, Some(Section::Scenes));

        state.toggle_section(Section::Scenes);
        assert_eq!(state.open_section, None);
    }

    #[test]
    fn sub_accordion_toggles_open_and_closed() {
        let mut state = SidebarLeftState::default();

        state.toggle_sub("scenes_programming");
        assert_eq!(state.open_sub, Some("scenes_programming"));

        state.toggle_sub("scenes_programming");
        assert_eq!(state.open_sub, None);
    }

    #[test]
    fn switching_section_resets_open_sub() {
        let mut state = SidebarLeftState::default();

        state.toggle_section(Section::Scenes);
        state.toggle_sub("scenes_programming");
        state.toggle_section(Section::Camera);

        assert_eq!(state.open_section, Some(Section::Camera));
        assert_eq!(state.open_sub, None);
    }
}
