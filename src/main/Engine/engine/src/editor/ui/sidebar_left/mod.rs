//! Barra lateral izquierda: escenas funcionales, cámara y secciones visuales.

use egui::{Color32, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::app::{ControlSummary, EditorApp, EditorStatus, PlayView};
use crate::editor::assets::{AssetKind, AssetLists};
use crate::editor::cli::Locale;
use crate::editor::context::{GameStyle, ProjectContext};
use crate::editor::control_bindings::{self, ControlDeviceMode};
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
    InteractionTooltips,
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
            Self::InteractionTooltips => "interaction_tooltips",
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
    cam3_clip: NumberDraft,
    selected_sprite: Option<String>,
    selected_model: Option<String>,
    pub(crate) active_tool: Option<&'static str>,
    pub(crate) active_ui_screen: Option<String>,
    /// Sub-acordeón abierto dentro de Recursos → Modelos (una categoría).
    pub(crate) open_model_category: Option<&'static str>,
    /// Globalización pendiente: la modal nativa confirma antes de enviar el comando.
    pub(crate) globalize_model: Option<GlobalizeModelDraft>,
    /// Modal nativa «Cargar desde otra escena» abierta (lista de recursos ajenos).
    pub(crate) load_from_scene_open: bool,
    /// Cambio de escena pendiente de confirmar en la modal nativa.
    pub(crate) pending_scene_switch: Option<SceneSwitchDraft>,
    /// «Limpiar escena» pendiente de confirmar en la modal nativa.
    pub(crate) pending_clear_scene: bool,
    /// Recurso pendiente de borrar; la modal nativa pide confirmación.
    pub(crate) pending_remove_asset: Option<RemoveAssetDraft>,
    /// Escena pendiente de borrar; la modal nativa confirma (o avisa si es la única).
    pub(crate) pending_delete_scene: Option<DeleteSceneDraft>,
    /// Creación de escena pendiente; la ventana nativa pide el nombre.
    pub(crate) pending_create_scene: Option<CreateSceneDraft>,
    /// Pantalla UI pendiente de borrar; la ventana nativa pide confirmación.
    pub(crate) pending_delete_ui_screen: Option<DeleteUiScreenDraft>,
    /// Creación de pantalla UI pendiente; la ventana nativa pide el nombre.
    pub(crate) pending_create_ui_screen: Option<CreateUiScreenDraft>,
    /// Renombrado de menú UI pendiente; la ventana nativa pide el nombre.
    pub(crate) pending_rename_ui_screen: Option<RenameUiScreenDraft>,
    /// Borrador del tooltip en edición en el accordion de interacción.
    pub(crate) tooltip_draft: interaction_tooltips::TooltipDraft,
    /// Abre la modal nativa con el formulario del tooltip.
    pub(crate) tooltip_modal_open: bool,
    /// Confirmación de borrado de un tooltip con nombre.
    pub(crate) pending_delete_interaction_tooltip: Option<DeleteInteractionTooltipDraft>,
    /// Editor nativo del script asignado al tooltip (nombre, fuente).
    pub(crate) tooltip_script_editor: Option<(String, String)>,
    /// Confirmación de quitar el script/nodo del tooltip.
    pub(crate) pending_remove_tooltip_script: bool,
    /// Guía de usuario abierta en su ventana nativa.
    pub(crate) user_guide_open: bool,
    /// Selector de blueprints abierto en su ventana nativa.
    pub(crate) quick_build_open: bool,
    /// Pestaña activa del selector de blueprints (persiste entre frames).
    pub(crate) quick_build_tab: usize,
    /// Borrado de blueprint pendiente de confirmar en la ventana nativa.
    pub(crate) quick_build_delete: Option<QuickBuildDeleteDraft>,
    /// Script de escena abierto en su ventana nativa (id de escena).
    pub(crate) scene_script_open: Option<u64>,
    /// Código del editor; `None` hasta que llega el `project_state` con el script.
    pub(crate) scene_script_source: Option<String>,
}

/// Pantalla UI que el usuario quiere borrar (HUD o menú).
pub(crate) struct DeleteUiScreenDraft {
    pub scope: String,
    pub id: String,
    pub name: String,
}

/// Nombre inicial para la pantalla UI nueva (HUD o menú).
pub(crate) struct CreateUiScreenDraft {
    pub scope: String,
    pub name: String,
    pub default_name: String,
}

/// Menú UI que se va a renombrar en la ventana nativa.
pub(crate) struct RenameUiScreenDraft {
    pub id: String,
    pub name: String,
}

/// Blueprint a borrar ( datos sin serializar ) y nº de entidades vinculadas.
pub(crate) struct QuickBuildDeleteDraft {
    pub blueprint: serde_json::Value,
    pub linked_count: usize,
}

/// Nombre inicial para la escena nueva; la ventana nativa lo deja editar.
pub(crate) struct CreateSceneDraft {
    pub name: String,
    pub default_name: String,
}

/// Escena que el usuario quiere borrar. `blocked` indica que es la única del
/// proyecto: la ventana nativa muestra el aviso en vez de la confirmación.
pub(crate) struct DeleteSceneDraft {
    pub id: u64,
    pub name: String,
    pub blocked: bool,
}

/// Recurso que el usuario quiere borrar; la modal nativa confirma antes de
/// enviar el comando `remove_*` al motor.
pub(crate) struct RemoveAssetDraft {
    pub kind: AssetKind,
    pub path: String,
    pub name: String,
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
    SetPlayCameraClipRadius(f32),
    OpenSockets,
    /// Abre el catálogo de plugins en su ventana nativa.
    OpenPlugins,
    OpenLegacyWindow(&'static str),
    /// Abre la modal React de bindings para `keyboard_mouse` o `gamepad`.
    OpenControlBindings(&'static str),
    /// Crea una pantalla HUD/menú en la ventana nativa.
    CreateUiScreen(&'static str),
    /// Abre el editor HUD (aún React) o renombra un menú en la ventana nativa.
    OpenUiEditor {
        scope: &'static str,
        screen_id: String,
    },
    /// Elimina una pantalla HUD/menú (con confirmación en la ventana nativa).
    DeleteUiScreen {
        scope: &'static str,
        id: String,
        name: String,
    },
    /// Activa/desactiva la pantalla HUD que se muestra en play.
    SetActiveUiScreen(Option<String>),
    ImportAsset(AssetKind),
    /// Importa un modelo 3D directamente en la categoría elegida en el
    /// sub-acordeón de Recursos → Modelos (sin modal intermedia).
    ImportModelCategory(&'static str),
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
    /// Abre la modal del tooltip (None = crear, Some = editar).
    OpenTooltipModal(Option<interaction_tooltips::TooltipDraft>),
    /// Borra un tooltip de interacción del proyecto.
    DeleteInteractionTooltip {
        id: String,
        name: String,
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
mod interaction_tooltips;
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
pub(crate) use interaction_tooltips::{
    DEMO_TOOLTIP_IDS, DeleteInteractionTooltipDraft, TooltipDraft, demo_tooltips, is_demo_tooltip,
    sync_command,
};

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
    let interaction_tooltips = app.project_state.interaction_tooltips.clone();
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
                    section(
                        ui,
                        state,
                        Section::InteractionTooltips,
                        icons::CHAT,
                        crate::editor::i18n::tr(
                            locale,
                            "Tooltips de interacción",
                            "Interaction tooltips",
                        ),
                        |ui, state| {
                            interaction_tooltips::body(
                                ui,
                                state,
                                &interaction_tooltips,
                                locale,
                                &mut actions,
                            )
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
            // La ventana nativa pide el nombre; el motor asigna el id al crear.
            let next_id = app
                .scenes
                .scenes
                .iter()
                .map(|scene| scene.id)
                .max()
                .unwrap_or(0)
                + 1;
            let default_name = rer_engine_core::editor_defaults::default_scene_name(
                u32::try_from(next_id).unwrap_or(1),
            );
            app.left_sidebar.pending_create_scene = Some(CreateSceneDraft {
                name: default_name.clone(),
                default_name,
            });
        }
        LeftAction::OpenDeleteScene(id) => {
            // La ventana nativa confirma el borrado (o avisa si es la única escena).
            let name = app.scenes.name_for(id);
            let blocked = !app.scenes.can_delete();
            app.left_sidebar.pending_delete_scene = Some(DeleteSceneDraft { id, name, blocked });
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
        LeftAction::SetPlayCameraClipRadius(radius) => {
            app.send_engine(json!({
                "cmd": "set_play_character_view",
                "camera_only": true,
                "camera_clip_radius": radius,
            }));
        }
        LeftAction::OpenPlugins => {
            app.plugins_modal.open = true;
            crate::editor::plugins::request_snapshot(app);
            app.push_log("[Plugins] Abriendo catálogo…", false);
        }
        LeftAction::OpenSockets => {
            // La ventana nativa de sockets toma la entidad seleccionada del viewport.
            let target = app.selected_entity.clone();
            app.socket_config.open = true;
            app.socket_config.target_id = target.as_ref().map(|entity| entity.id);
            app.socket_config.target_name = target.map(|entity| entity.name).unwrap_or_default();
            if let Some(id) = app.socket_config.target_id {
                app.send_engine(json!({ "cmd": "list_entity_sockets", "entity_id": id }));
                app.send_engine(json!({ "cmd": "list_entity_bones", "entity_id": id }));
            }
        }
        LeftAction::OpenLegacyWindow(kind) => {
            if kind == "user_guide" {
                app.left_sidebar.user_guide_open = true;
                return;
            }
            let scene_id = app.scenes.active_scene_id;
            if kind == "scene_visual_script" {
                // El editor de nodos lee el grafo del estado nativo del proyecto.
                app.visual_script.context = crate::editor::visual_script::GraphContext::Scene;
                app.visual_script.scene_id = Some(scene_id);
                app.visual_script.scene_name = app.scenes.name_for(scene_id);
                app.visual_script.entity_id = None;
                app.visual_script.error = None;
                app.visual_script.selected = None;
                app.visual_script.loaded = false;
                app.visual_script.open = true;
                return;
            }
            if kind == "scene_script" {
                // El script se lee del estado nativo del proyecto.
                app.left_sidebar.scene_script_open = Some(scene_id);
                app.left_sidebar.scene_script_source = None;
                return;
            }
            // Todos los `kind` conocidos se abren nativamente arriba; no queda
            // ninguna ventana legacy que delegar a Electron.
            app.push_log(
                format!("[Editor] Ventana desconocida «{kind}» (escena {scene_id})"),
                true,
            );
        }
        LeftAction::OpenControlBindings(mode) => {
            // Los bindings se leen del `entity_meta` nativo del editor.
            // El personaje efectivo se elige igual que en React (jugador primero).
            app.control_bindings.mode = ControlDeviceMode::parse(mode);
            control_bindings::ensure_character(app);
            app.control_bindings.editor = None;
            app.control_bindings.open = true;
            app.push_log(
                format!("[Controles] Abriendo configuración ({mode})…"),
                false,
            );
        }
        LeftAction::CreateUiScreen(scope) => {
            let list = if scope == "menu" {
                &app.ui_screens.menu
            } else {
                &app.ui_screens.player
            };
            let default_name = format!(
                "{} {}",
                if scope == "menu" { "UI Menu" } else { "HUD" },
                list.len() + 1
            );
            app.left_sidebar.pending_create_ui_screen = Some(CreateUiScreenDraft {
                scope: scope.to_string(),
                name: default_name.clone(),
                default_name,
            });
        }
        LeftAction::OpenUiEditor { scope, screen_id } => {
            if scope == "menu" {
                // El renombrado de menús vive en la ventana nativa.
                let name = app
                    .ui_screens
                    .menu
                    .iter()
                    .find(|screen| screen.id == screen_id)
                    .map(|screen| screen.name.clone())
                    .unwrap_or_default();
                app.left_sidebar.pending_rename_ui_screen = Some(RenameUiScreenDraft {
                    id: screen_id,
                    name,
                });
            } else {
                // Editor de HUD 100 % nativo: se pide el snapshot de elementos
                // al motor y se entra en modo edición de la pantalla.
                crate::editor::hud_editor::begin_edit(app, &screen_id);
            }
        }
        LeftAction::DeleteUiScreen { scope, id, name } => {
            // La ventana nativa confirma y sincroniza el catálogo con el motor.
            app.left_sidebar.pending_delete_ui_screen = Some(DeleteUiScreenDraft {
                scope: scope.to_string(),
                id,
                name,
            });
        }
        LeftAction::SetActiveUiScreen(screen_id) => {
            // 100% nativo: el editor es la fuente de verdad del catálogo.
            for screen in &mut app.ui_screens.player {
                screen.active = screen_id.as_deref() == Some(screen.id.as_str());
            }
            app.left_sidebar.active_ui_screen = screen_id.clone();
            app.sync_player_ui_screens();
            app.send_engine(json!({
                "cmd": "set_active_player_ui_screen",
                "screen_id": screen_id,
            }));
        }
        LeftAction::ImportAsset(kind) => import_asset(app, kind),
        LeftAction::ImportModelCategory(category) => import_model_category(app, category),
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
            // La modal nativa confirma antes de enviar el comando de borrado.
            app.left_sidebar.pending_remove_asset = Some(RemoveAssetDraft { kind, path, name });
        }
        LeftAction::CreateEntity { kind, asset } => create_entity(app, kind, asset),
        LeftAction::SetActiveTool(tool) => {
            app.left_sidebar.active_tool = if tool.is_empty() { None } else { Some(tool) };
            app.send_engine(json!({ "cmd": "set_active_tool", "tool": tool }));
        }
        LeftAction::QuickBuild { active } => {
            if active {
                // Desactivar: se apaga el ghost directamente en el motor.
                app.quick_build_active = None;
                app.placement_hint = None;
                app.send_engine(json!({ "cmd": "set_active_tool", "tool": "" }));
                app.push_log("[Quick build] Desactivando…", false);
            } else {
                // Los blueprints ya están en el estado nativo del editor.
                app.left_sidebar.quick_build_open = true;
                app.push_log("[Quick build] Abriendo blueprints…", false);
            }
        }
        LeftAction::OpenTooltipModal(draft) => {
            app.left_sidebar.tooltip_draft = draft.unwrap_or_default();
            app.left_sidebar.tooltip_modal_open = true;
        }
        LeftAction::DeleteInteractionTooltip { id, name } => {
            app.left_sidebar.pending_delete_interaction_tooltip =
                Some(DeleteInteractionTooltipDraft { id, name });
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

/// Importa un modelo directamente a la categoría del sub-acordeón pulsado.
/// No hay modal intermedia: el botón ya dice en qué categoría se va a cargar.
fn import_model_category(app: &mut EditorApp, category: &'static str) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Modelo 3D", AssetKind::Model.extensions())
        .pick_file()
    else {
        return;
    };
    let path_text = path.to_string_lossy().to_string();
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("modelo")
        .to_string();
    app.send_engine(json!({
        "cmd": "load_model_asset",
        "path": path_text,
        "name": name,
        "category": category,
    }));
    app.push_log(
        format!("[Recursos] Importando «{name}» como {category}…"),
        false,
    );
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
        app.placement_hint = Some(
            crate::editor::i18n::tr(
                app.cli.locale,
                "Mueve el cursor sobre el mundo y haz click para colocar la entidad",
                "Move the cursor over the world and click to place the entity",
            )
            .to_string(),
        );
        app.push_log(
            format!(
                "[Entidad] Mueve el cursor y haz click para colocar {:?}…",
                kind
            ),
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
        format!(
            "[Entidad] Creando {:?} desde el recurso seleccionado…",
            kind
        ),
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
        actions.push(LeftAction::OpenPlugins);
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
