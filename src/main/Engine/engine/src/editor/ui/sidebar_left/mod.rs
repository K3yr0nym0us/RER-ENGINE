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
    AddUiText(String),
    AddUiImage(String),
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
use self::camera::*;
use self::controls::*;
use self::entities::*;
use self::interface::*;
use self::resources::*;
use self::scenes::*;
use self::tools::*;

pub fn show(ui: &mut egui::Ui, app: &mut EditorApp) {
    let locale = app.cli.locale;
    let context = app.cli.context;
    let phase_note = app.text().phase_note;
    let scenes = app.scenes.clone();
    let camera = app.world.camera2d;
    let play_view = app.play_view;
    let ready = app.status == EditorStatus::Ready;
    let assets = app.assets.clone();
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
            next_locale = footer(ui, locale, phase_note);
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
                        |ui, state| ui_body(ui, state, &assets, locale, phase_note, &mut actions),
                    );
                    section(
                        ui,
                        state,
                        Section::Tools,
                        icons::WRENCH,
                        crate::editor::i18n::tr(locale, "Herramientas", "Tools"),
                        |ui, state| {
                            tools_body(ui, state, context.is_3d(), locale, phase_note, &mut actions)
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::Controls,
                        icons::GAME_CONTROLLER,
                        crate::editor::i18n::tr(locale, "Controles", "Controls"),
                        |ui, _state| controls_body(ui, locale, phase_note, controls.as_ref()),
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
        LeftAction::AddUiText(font_path) => {
            let screen = app
                .left_sidebar
                .active_ui_screen
                .clone()
                .unwrap_or_else(|| "player-hud-01".to_string());
            app.send_engine(json!({
                "cmd": "set_player_ui_edit_mode",
                "active": true,
                "scope": "player",
                "screen_id": screen,
            }));
            app.send_engine(json!({
                "cmd": "add_player_ui_text_box",
                "font_path": font_path,
            }));
            app.push_log("[UI] Añadiendo cuadro de texto…", false);
        }
        LeftAction::AddUiImage(image_path) => {
            let screen = app
                .left_sidebar
                .active_ui_screen
                .clone()
                .unwrap_or_else(|| "player-hud-01".to_string());
            app.send_engine(json!({
                "cmd": "set_player_ui_edit_mode",
                "active": true,
                "scope": "player",
                "screen_id": screen,
            }));
            app.send_engine(json!({
                "cmd": "add_player_ui_image",
                "image_path": image_path,
            }));
            app.push_log("[UI] Añadiendo imagen HUD…", false);
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
fn footer(ui: &mut egui::Ui, locale: Locale, hint: &str) -> Option<Locale> {
    widgets::wide_button(
        ui,
        icons::PUZZLE_PIECE,
        crate::editor::i18n::tr(locale, "Plugins", "Plugins"),
        hint,
        crate::editor::theme::OUTLINE_SECONDARY,
    );
    ui.add_space(2.0);

    let mut next = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let language_width = 38.0;
        let gap = 8.0;
        let guide_width = (ui.available_width() - language_width - gap).max(0.0);

        widgets::wide_button_sized(
            ui,
            guide_width,
            icons::INFO,
            crate::editor::i18n::tr(locale, "Guia de usuario", "User guide"),
            hint,
            crate::editor::theme::INFO,
        );
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
