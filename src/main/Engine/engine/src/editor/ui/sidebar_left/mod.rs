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
    /// Importación de modelo pendiente: la modal nativa pide categoría (y nombre).
    pub(crate) model_import: Option<ModelImportDraft>,
    /// Globalización pendiente: la modal nativa confirma antes de enviar el comando.
    pub(crate) globalize_model: Option<GlobalizeModelDraft>,
    /// Modal nativa «Cargar desde otra escena» abierta (lista de recursos ajenos).
    pub(crate) load_from_scene_open: bool,
    /// Cambio de escena pendiente de confirmar en la modal nativa.
    pub(crate) pending_scene_switch: Option<SceneSwitchDraft>,
    /// «Limpiar escena» pendiente de confirmar en la modal nativa.
    pub(crate) pending_clear_scene: bool,
}

/// Escena destino del cambio pendiente; la modal nativa confirma antes de cargar.
pub(crate) struct SceneSwitchDraft {
    pub id: u64,
    pub name: String,
}

/// Modelo que el usuario quiere convertir en global; la modal nativa explica
/// que pasará a estar disponible en todas las escenas.
pub(crate) struct GlobalizeModelDraft {
    pub path: String,
    pub name: String,
}

/// Borrador de importación de modelo 3D: la modal nativa (egui) elige la
/// categoría antes de registrar el `.rerasset` en el motor.
pub(crate) struct ModelImportDraft {
    pub path: String,
    pub name: String,
    /// `character | environment | object | weapon | projectile`
    /// (`normalize_model_library_category` en el motor).
    pub category: &'static str,
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
    /// Resources → modelos: modal «Cargar desde otra escena» (referencia por path).
    OpenLoadFromScene,
    /// Resources → modelos: convierte el modelo de la escena activa en global.
    GlobalizeModel {
        path: String,
        name: String,
    },
    /// Escenas → «Limpiar escena» (modal nativa de confirmación).
    ClearScene,
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
            next_locale = footer(ui, locale, &mut actions);
        });
    if let Some(next) = next_locale {
        app.cli.locale = next;
        crate::editor::bridge::notify_locale(next);
        app.send_engine(json!({
            "cmd": "set_locale",
            "locale": next.as_str(),
        }));
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
                        |ui, state| scenes_body(ui, state, &scenes, context, locale, &mut actions),
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
                                scenes.active_scene_id,
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
                                scenes.active_scene_id,
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
                        |ui, state| ui_body(ui, state, &ui_screens, locale, &mut actions),
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
                                quick_build_active.as_deref(),
                                &mut actions,
                            )
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::Controls,
                        icons::GAME_CONTROLLER,
                        crate::editor::i18n::tr(locale, "Controles", "Controls"),
                        |ui, _state| controls_body(ui, locale, controls.as_ref(), &mut actions),
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
            // La modal nativa confirma antes de cargar la escena.
            app.left_sidebar.pending_scene_switch = Some(SceneSwitchDraft {
                id,
                name: app.scenes.name_for(id),
            });
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
        LeftAction::OpenLoadFromScene => {
            // Lista nativa de modelos de otras escenas; al elegir uno se abre la
            // confirmación de globalización (misma lógica que el botón globo).
            // Refresco para tener el `scene_id` al día antes de listar.
            app.send_engine(json!({ "cmd": "get_models_list" }));
            app.left_sidebar.load_from_scene_open = true;
        }
        LeftAction::GlobalizeModel { path, name } => {
            // La modal nativa pide confirmación antes de enviar el comando.
            app.left_sidebar.globalize_model = Some(GlobalizeModelDraft { path, name });
        }
        LeftAction::ClearScene => {
            // La modal nativa pide confirmación antes de vaciar los DEMO.
            app.left_sidebar.pending_clear_scene = true;
        }
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

    // Los modelos deben quedar categorizados: la modal nativa pide la categoría
    // antes de enviar `load_model_asset` (si no, no aparecen en los selectores
    // filtrados por categoría, p. ej. «cambiar modelo» de un personaje).
    if kind == AssetKind::Model {
        app.left_sidebar.model_import = Some(ModelImportDraft {
            path: path_text,
            name,
            category: "object",
        });
        return;
    }

    app.send_engine(json!({
        "cmd": kind.load_command(),
        "path": path_text,
        "name": name,
    }));
    app.push_log(format!("[Recursos] Importando «{name}»…"), false);
}

/// Comando para iniciar la colocación con ghost de una entidad nueva.
/// Reutiliza el mismo mecanismo que Construcción rápida (`quick_build_place`),
/// pero en modo `preview_place_once`: el modelo sigue al cursor y se coloca
/// con un click, tras el cual la herramienta se desactiva.
fn begin_entity_placement_command(kind: EntityKind, asset: String) -> serde_json::Value {
    let (entity_kind, category) = match kind {
        EntityKind::Character => ("character", "character"),
        EntityKind::Environment => ("scenario", "environment"),
        EntityKind::Object => ("model", "object"),
        EntityKind::Weapon => ("model", "weapon"),
        EntityKind::Projectile => ("projectile", "projectile"),
    };
    json!({
        "cmd": "set_active_tool",
        "tool": "quick_build_place",
        "preview_path": asset,
        "preview_kind": "model",
        "preview_scale": [1.0, 1.0, 1.0],
        "preview_entity_category": category,
        "preview_spawn_kind": entity_kind,
        "preview_place_once": true,
    })
}

fn create_entity(app: &mut EditorApp, kind: EntityKind, asset: String) {
    if app.cli.context.is_3d() {
        app.send_engine(begin_entity_placement_command(kind, asset));
        // El motor emitirá `quick_build_ghost_ready`, pero mostramos el hint
        // ya mismo para que no haya que adivinar qué hacer en el viewport.
        app.placement_hint = Some(crate::editor::i18n::tr(
            app.cli.locale,
            "Mueve el cursor sobre el mundo y haz click para colocar la entidad",
            "Move the cursor over the world and click to place the entity",
        ).to_string());
        app.push_log(
            format!("[Entidad] Mueve el cursor y haz click para colocar {:?}…", kind),
            false,
        );
        return;
    }
    let command = match kind {
        EntityKind::Character => "load_character",
        EntityKind::Environment | EntityKind::Object | EntityKind::Weapon => "load_scenario",
        EntityKind::Projectile => "load_projectile",
    };
    app.send_engine(json!({ "cmd": command, "path": asset }));
    app.push_log(
        format!("[Entidad] Creando {:?} desde el recurso seleccionado…", kind),
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn footer(ui: &mut egui::Ui, locale: Locale, actions: &mut Vec<LeftAction>) -> Option<Locale> {
    let width = ui.available_width();
    if widgets::wide_button_click_sized(
        ui,
        width,
        icons::PUZZLE_PIECE,
        crate::editor::i18n::tr(locale, "Plugins", "Plugins"),
        crate::editor::i18n::tr(
            locale,
            "Abrir catálogo de plugins",
            "Open the plugins catalog",
        ),
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
            crate::editor::i18n::tr(locale, "Abrir la guía de usuario", "Open the user guide"),
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

    #[test]
    fn create_entity_uses_one_shot_ghost_placement() {
        let cmd = begin_entity_placement_command(EntityKind::Character, "model_hero".into());

        assert_eq!(cmd["cmd"], "set_active_tool");
        assert_eq!(cmd["tool"], "quick_build_place");
        assert_eq!(cmd["preview_path"], "model_hero");
        assert_eq!(cmd["preview_kind"], "model");
        assert_eq!(cmd["preview_entity_category"], "character");
        assert_eq!(cmd["preview_spawn_kind"], "character");
        assert_eq!(cmd["preview_place_once"], true);
    }

    #[test]
    fn each_entity_kind_keeps_its_spawn_kind_and_category() {
        let cases = [
            (EntityKind::Character, "character", "character"),
            (EntityKind::Environment, "scenario", "environment"),
            (EntityKind::Object, "model", "object"),
            (EntityKind::Weapon, "model", "weapon"),
            (EntityKind::Projectile, "projectile", "projectile"),
        ];
        for (kind, spawn_kind, category) in cases {
            let cmd = begin_entity_placement_command(kind, "asset".into());
            assert_eq!(cmd["preview_spawn_kind"], spawn_kind);
            assert_eq!(cmd["preview_entity_category"], category);
        }
    }
}
