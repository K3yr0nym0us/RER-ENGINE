//! Barra lateral izquierda: escenas funcionales, cámara y secciones visuales.

use egui::{Color32, RichText};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::app::{ControlSummary, EditorApp, EditorStatus, PlayView};
use crate::assets::{AssetKind, AssetLists};
use crate::cli::Locale;
use crate::context::{GameStyle, ProjectContext};
use crate::i18n;
use crate::scenes::SceneList;
use crate::theme;
use crate::world::Camera2d;

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
                .fill(theme::SIDEBAR_BG)
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
                        i18n::tr(locale, "Escenas", "Scenes"),
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
                        i18n::tr(locale, "Cámara", "Camera"),
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
                        i18n::tr(locale, "Recursos", "Resources"),
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
                        i18n::tr(locale, "Crear entidad", "Create entity"),
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
                        i18n::tr(locale, "Interfaz de usuario", "User interface"),
                        |ui, state| ui_body(ui, state, &assets, locale, phase_note, &mut actions),
                    );
                    section(
                        ui,
                        state,
                        Section::Tools,
                        icons::WRENCH,
                        i18n::tr(locale, "Herramientas", "Tools"),
                        |ui, state| {
                            tools_body(ui, state, context.is_3d(), locale, phase_note, &mut actions)
                        },
                    );
                    section(
                        ui,
                        state,
                        Section::Controls,
                        icons::GAME_CONTROLLER,
                        i18n::tr(locale, "Controles", "Controls"),
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
            crate::bridge::request_legacy_window("create_scene", json!({}));
        }
        LeftAction::OpenDeleteScene(id) => {
            crate::bridge::request_legacy_window("delete_scene", json!({ "scene_id": id }));
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
            crate::bridge::request_legacy_window("socket_config", json!({}));
        }
        LeftAction::OpenLegacyWindow(kind) => {
            let scene_id = app.scenes.active_scene_id;
            crate::bridge::request_legacy_window(kind, json!({ "scene_id": scene_id }));
            app.push_log(
                format!("[Editor] Abriendo ventana legacy «{kind}» (escena {scene_id})…"),
                false,
            );
        }
        LeftAction::ImportAsset(kind) => import_asset(app, kind),
        LeftAction::OpenRemoveAsset { kind, path, name } => {
            crate::bridge::request_legacy_window(
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

fn scenes_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    scenes: &SceneList,
    context: ProjectContext,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    sub(
        ui,
        state,
        "scenes_programming",
        icons::CODE,
        i18n::tr(locale, "Programación", "Programming"),
        |ui| {
            let width = ui.available_width();
            if widgets::wide_button_click_sized(
                ui,
                width,
                icons::FILE_CODE,
                i18n::tr(locale, "Script de escena", "Scene script"),
                hint,
                theme::WARNING,
            ) {
                actions.push(LeftAction::OpenLegacyWindow("scene_script"));
            }
            if widgets::wide_button_click_sized(
                ui,
                width,
                icons::GRAPH,
                i18n::tr(locale, "Lógica de escena (nodos)", "Scene logic (nodes)"),
                hint,
                theme::INFO,
            ) {
                actions.push(LeftAction::OpenLegacyWindow("scene_visual_script"));
            }
        },
    );

    if !context.is_3d() {
        widgets::note(
            ui,
            i18n::tr(
                locale,
                "La gestión de escenas 2D sigue en la UI React.",
                "2D scene management still lives in the React UI.",
            ),
        );
        return;
    }

    let create_width = ui.available_width();
    if !scenes.loading
        && widgets::wide_button_click_sized(
            ui,
            create_width,
            icons::PLUS,
            i18n::tr(locale, "Crear nueva escena", "Create new scene"),
            i18n::tr(
                locale,
                "Crea una escena 3D vacía",
                "Create an empty 3D scene",
            ),
            theme::OUTLINE_SECONDARY,
        )
    {
        actions.push(LeftAction::OpenCreateScene);
    }
    ui.add_space(2.0);

    if scenes.loading && scenes.scenes.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new().size(16.0));
            ui.label(
                RichText::new(i18n::tr(locale, "Cargando escenas…", "Loading scenes..."))
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
            );
        });
        return;
    }

    if scenes.scenes.is_empty() {
        widgets::note(
            ui,
            i18n::tr(
                locale,
                "Sin escenas del motor todavía.",
                "No engine scenes yet.",
            ),
        );
        return;
    }

    for scene in &scenes.scenes {
        let is_active = scene.id == scenes.active_scene_id;
        let switching = scenes.switching_to == Some(scene.id);
        let fill = if is_active {
            theme::SCENE_ACTIVE_BG
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if is_active {
            egui::Stroke::new(1.0, theme::SCENE_ACTIVE_TEXT)
        } else {
            egui::Stroke::NONE
        };

        egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name_width = (ui.available_width() - 46.0).max(0.0);
                    let name_color = if is_active {
                        theme::SCENE_ACTIVE_TEXT
                    } else {
                        theme::TEXT
                    };
                    ui.add_sized(
                        [name_width, 18.0],
                        egui::Label::new(RichText::new(&scene.name).size(13.0).color(name_color))
                            .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let can_act = !scenes.loading;
                        if can_act
                            && scenes.can_delete()
                            && widgets::icon_action(
                                ui,
                                icons::TRASH,
                                20.0,
                                theme::DANGER,
                                i18n::tr(locale, "Eliminar escena", "Delete scene"),
                            )
                        {
                            actions.push(LeftAction::OpenDeleteScene(scene.id));
                        }
                        if !is_active {
                            if switching {
                                ui.add(egui::Spinner::new().size(14.0));
                            } else if can_act
                                && widgets::icon_action(
                                    ui,
                                    icons::SIGN_IN,
                                    20.0,
                                    theme::INFO,
                                    i18n::tr(locale, "Cargar escena", "Load scene"),
                                )
                            {
                                actions.push(LeftAction::SwitchScene(scene.id));
                            }
                        }
                    });
                });
            });
    }
}

fn camera_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    context: ProjectContext,
    camera: Option<Camera2d>,
    play_view: Option<PlayView>,
    ready: bool,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    if context.is_3d() {
        widgets::caption(ui, i18n::tr(locale, "Tipo de cámara", "Camera type"));
        widgets::select_placeholder(ui, context.game_style.as_str(), hint);
        ui.add_space(4.0);

        if context.game_style != GameStyle::FirstPerson {
            widgets::note(
                ui,
                i18n::tr(
                    locale,
                    "Los controles detallados para este modo aún no están disponibles.",
                    "Detailed camera controls for this mode are not available yet.",
                ),
            );
            return;
        }

        let Some(view) = play_view else {
            widgets::note(
                ui,
                i18n::tr(
                    locale,
                    "Aún no hay jugador en la escena.",
                    "No player in the scene yet.",
                ),
            );
            return;
        };
        if view.player_id.is_none() {
            widgets::note(
                ui,
                i18n::tr(
                    locale,
                    "Aún no hay jugador en la escena.",
                    "No player in the scene yet.",
                ),
            );
            return;
        }

        state.cam3_x.sync(view.eye[0]);
        state.cam3_y.sync(view.eye[1]);
        state.cam3_z.sync(view.eye[2]);
        state.cam3_yaw.sync(view.yaw.to_degrees());
        state.cam3_fov.sync(view.fov_y.to_degrees());
        state.cam3_frustum.sync(view.frustum_distance);

        widgets::caption(ui, i18n::tr(locale, "Cámara", "Camera"));
        let mut axis = None;
        ui.add_enabled_ui(ready, |ui| {
            ui.columns(3, |columns| {
                for (index, draft) in [
                    (0usize, &mut state.cam3_x),
                    (1, &mut state.cam3_y),
                    (2, &mut state.cam3_z),
                ] {
                    let width = columns[index].available_width();
                    columns[index].label(
                        RichText::new(["X", "Y", "Z"][index])
                            .size(11.5)
                            .color(theme::TEXT_MUTED),
                    );
                    if widgets::number_input(&mut columns[index], width, &mut draft.text)
                        && let Some(value) = draft.parsed()
                    {
                        axis = Some((index as u8, value));
                    }
                }
            });
        });
        if let Some((axis, value)) = axis {
            actions.push(LeftAction::SetPlayCameraAxis { axis, value });
        }

        let mut yaw = None;
        let mut fov = None;
        let mut frustum = None;
        ui.add_enabled_ui(ready, |ui| {
            ui.columns(3, |columns| {
                let labels = [
                    i18n::tr(locale, "Yaw (°)", "Yaw (°)"),
                    i18n::tr(locale, "FOV (°)", "FOV (°)"),
                    i18n::tr(locale, "Rango F. (m)", "F. Range (m)"),
                ];
                let drafts = [
                    &mut state.cam3_yaw,
                    &mut state.cam3_fov,
                    &mut state.cam3_frustum,
                ];
                for (index, (label, draft)) in labels.into_iter().zip(drafts).enumerate() {
                    let width = columns[index].available_width();
                    columns[index].label(RichText::new(label).size(11.5).color(theme::TEXT_MUTED));
                    if widgets::number_input(&mut columns[index], width, &mut draft.text)
                        && let Some(value) = draft.parsed()
                    {
                        match index {
                            0 => yaw = Some(value.to_radians()),
                            1 => fov = Some(value.to_radians()),
                            _ => frustum = Some(value.max(0.5)),
                        }
                    }
                }
            });
        });
        if let Some(value) = yaw {
            actions.push(LeftAction::SetPlayCameraYaw(value));
        }
        if let Some(value) = fov {
            actions.push(LeftAction::SetPlayCameraFov(value));
        }
        if let Some(value) = frustum {
            actions.push(LeftAction::SetPlayCameraFrustum(value));
        }
        return;
    }

    let camera = camera.unwrap_or(Camera2d {
        x: 0.0,
        y: 0.0,
        half_h: 10.0,
    });
    state.cam_x.sync(camera.x);
    state.cam_y.sync(camera.y);
    state.cam_zoom.sync(camera.half_h);

    widgets::caption(ui, i18n::tr(locale, "Cámara del editor", "Editor camera"));
    let mut commit = false;
    ui.columns(3, |columns| {
        let width = columns[0].available_width();
        columns[0].label(RichText::new("X").size(11.5).color(theme::TEXT_MUTED));
        if widgets::number_input(&mut columns[0], width, &mut state.cam_x.text) {
            commit = true;
        }

        let width = columns[1].available_width();
        columns[1].label(RichText::new("Y").size(11.5).color(theme::TEXT_MUTED));
        if widgets::number_input(&mut columns[1], width, &mut state.cam_y.text) {
            commit = true;
        }

        let width = columns[2].available_width();
        columns[2].label(
            RichText::new(i18n::tr(locale, "Zoom", "Zoom"))
                .size(11.5)
                .color(theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[2], width, &mut state.cam_zoom.text) {
            commit = true;
        }
    });

    if commit
        && let (Some(x), Some(y), Some(half_h)) = (
            state.cam_x.parsed(),
            state.cam_y.parsed(),
            state.cam_zoom.parsed(),
        )
    {
        actions.push(LeftAction::SetCamera2d {
            x,
            y,
            half_h: half_h.max(1.0),
        });
    }
}

fn resources_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    assets: &AssetLists,
    context: ProjectContext,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    if context.is_3d() {
        asset_section(
            ui,
            state,
            AssetKind::Model,
            icons::CUBE,
            i18n::tr(locale, "Modelos", "Models"),
            assets,
            locale,
            hint,
            actions,
        );
    } else {
        asset_section(
            ui,
            state,
            AssetKind::Sprite,
            icons::SPARKLE,
            i18n::tr(locale, "Sprites", "Sprites"),
            assets,
            locale,
            hint,
            actions,
        );
    }

    asset_section(
        ui,
        state,
        AssetKind::Sound,
        icons::SPEAKER_HIGH,
        i18n::tr(locale, "Sonidos", "Sounds"),
        assets,
        locale,
        hint,
        actions,
    );
    asset_section(
        ui,
        state,
        AssetKind::Font,
        icons::TEXT_AA,
        i18n::tr(locale, "Fuentes", "Fonts"),
        assets,
        locale,
        hint,
        actions,
    );
    asset_section(
        ui,
        state,
        AssetKind::HudImage,
        icons::IMAGE,
        i18n::tr(locale, "Imágenes", "Images"),
        assets,
        locale,
        hint,
        actions,
    );

    if !context.is_3d() {
        asset_section(
            ui,
            state,
            AssetKind::Background,
            icons::IMAGE_SQUARE,
            i18n::tr(locale, "Fondos", "Backgrounds"),
            assets,
            locale,
            hint,
            actions,
        );
    }
}

fn asset_section(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    kind: AssetKind,
    icon: &str,
    title: &str,
    assets: &AssetLists,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    sub(ui, state, kind.id(), icon, title, |ui| {
        let width = ui.available_width();
        if widgets::wide_button_click_sized(
            ui,
            width,
            icons::PLUS,
            import_label(kind, locale),
            hint,
            theme::ACCENT,
        ) {
            actions.push(LeftAction::ImportAsset(kind));
        }

        let list = assets.assets(kind);
        if list.is_empty() {
            widgets::tiny(
                ui,
                i18n::tr(locale, "Sin recursos cargados", "No assets loaded"),
            );
            return;
        }

        for asset in list {
            ui.horizontal(|ui| {
                let name_width = (ui.available_width() - 24.0).max(0.0);
                ui.add_sized(
                    [name_width, 16.0],
                    egui::Label::new(RichText::new(&asset.name).size(12.0).color(theme::TEXT))
                        .truncate(),
                );
                if widgets::icon_action(
                    ui,
                    icons::TRASH,
                    18.0,
                    theme::DANGER,
                    i18n::tr(locale, "Quitar recurso", "Remove asset"),
                ) {
                    actions.push(LeftAction::OpenRemoveAsset {
                        kind,
                        path: asset.path.clone(),
                        name: asset.name.clone(),
                    });
                }
            });
        }
    });
}

fn import_label(kind: AssetKind, locale: Locale) -> &'static str {
    match kind {
        AssetKind::Model => i18n::tr(locale, "+ Cargar modelo", "+ Load model"),
        AssetKind::Sprite => i18n::tr(locale, "+ Cargar sprite", "+ Load sprite"),
        AssetKind::Sound => i18n::tr(locale, "+ Cargar sonido", "+ Load sound"),
        AssetKind::Font => i18n::tr(locale, "+ Cargar fuente", "+ Load font"),
        AssetKind::HudImage => i18n::tr(locale, "+ Cargar imagen", "+ Load image"),
        AssetKind::Background => i18n::tr(locale, "+ Cargar fondo", "+ Load background"),
    }
}

fn entities_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    assets: &AssetLists,
    context: ProjectContext,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    let mut kinds = vec![
        (
            EntityKind::Character,
            icons::USER,
            i18n::tr(locale, "Personajes", "Characters"),
            theme::SUCCESS,
        ),
        (
            EntityKind::Environment,
            icons::TREE,
            i18n::tr(locale, "Escenario", "Environment"),
            theme::INFO,
        ),
        (
            EntityKind::Object,
            icons::CUBE,
            i18n::tr(locale, "Objetos", "Objects"),
            theme::WARNING,
        ),
    ];
    if context.is_3d() {
        kinds.push((
            EntityKind::Weapon,
            icons::SHIELD,
            i18n::tr(locale, "Armas", "Weapons"),
            theme::DANGER,
        ));
    }
    kinds.push((
        EntityKind::Projectile,
        icons::CIRCLE,
        i18n::tr(locale, "Proyectiles", "Projectiles"),
        theme::INFO,
    ));

    for (kind, icon, title, color) in kinds {
        entity_section(
            ui, state, kind, icon, title, color, assets, context, locale, hint, actions,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn entity_section(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    kind: EntityKind,
    icon: &str,
    title: &str,
    color: egui::Color32,
    assets: &AssetLists,
    context: ProjectContext,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    let selected = if context.is_3d() {
        &mut state.selected_model
    } else {
        &mut state.selected_sprite
    };
    sub_at(ui, &mut state.open_sub, kind.id(), icon, title, |ui| {
        let asset_kind = if context.is_3d() {
            AssetKind::Model
        } else {
            AssetKind::Sprite
        };
        let list = assets.assets(asset_kind);
        if list.is_empty() {
            widgets::note(
                ui,
                i18n::tr(
                    locale,
                    "Carga recursos primero en «Recursos».",
                    "Load assets first in “Resources”.",
                ),
            );
            return;
        }

        if selected
            .as_ref()
            .is_none_or(|path| !list.iter().any(|asset| &asset.path == path))
        {
            *selected = Some(list[0].path.clone());
        }

        let options: Vec<(String, String)> = list
            .iter()
            .map(|asset| (asset.path.clone(), asset.name.clone()))
            .collect();
        let width = ui.available_width();
        widgets::combo_string(ui, kind.id(), width, &options, selected);
        ui.add_space(2.0);

        if widgets::wide_button_click_sized(
            ui,
            width,
            icons::PLUS,
            create_label(kind, context, locale),
            hint,
            color,
        ) && let Some(asset) = selected.clone()
        {
            actions.push(LeftAction::CreateEntity { kind, asset });
        }
    });
}

fn create_label(kind: EntityKind, context: ProjectContext, locale: Locale) -> &'static str {
    if context.is_3d() {
        match kind {
            EntityKind::Character => i18n::tr(
                locale,
                "Crear personaje desde modelo",
                "Create character from model",
            ),
            EntityKind::Environment => i18n::tr(
                locale,
                "Crear escenario desde modelo",
                "Create environment from model",
            ),
            EntityKind::Object => i18n::tr(
                locale,
                "Crear objeto desde modelo",
                "Create object from model",
            ),
            EntityKind::Weapon => i18n::tr(
                locale,
                "Crear arma desde modelo",
                "Create weapon from model",
            ),
            EntityKind::Projectile => i18n::tr(
                locale,
                "Crear proyectil desde modelo",
                "Create projectile from model",
            ),
        }
    } else {
        match kind {
            EntityKind::Character => i18n::tr(locale, "Crear personaje", "Create character"),
            EntityKind::Environment => i18n::tr(locale, "Añadir escenario", "Add environment"),
            EntityKind::Object => i18n::tr(locale, "Añadir objeto", "Add object"),
            EntityKind::Weapon => i18n::tr(locale, "Crear arma", "Create weapon"),
            EntityKind::Projectile => i18n::tr(locale, "Añadir proyectil", "Add projectile"),
        }
    }
}

fn ui_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    assets: &AssetLists,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    let active_screen = state
        .active_ui_screen
        .clone()
        .unwrap_or_else(|| "player-hud-01".to_string());
    let screen_text = format!(
        "{}: {active_screen}",
        i18n::tr(locale, "Pantalla activa", "Active screen")
    );
    let font = assets.fonts.first().map(|asset| asset.path.clone());
    let image = assets.hud_images.first().map(|asset| asset.path.clone());

    sub(
        ui,
        state,
        "ui_player",
        icons::CURSOR_CLICK,
        i18n::tr(locale, "Interfaz del jugador", "Player UI"),
        |ui| {
            widgets::tiny(ui, &screen_text);
            let width = ui.available_width();

            if let Some(font) = &font {
                if widgets::wide_button_click_sized(
                    ui,
                    width,
                    icons::PLUS,
                    i18n::tr(locale, "+ Añadir texto", "+ Add text"),
                    hint,
                    theme::INFO,
                ) {
                    actions.push(LeftAction::AddUiText(font.clone()));
                }
            } else {
                widgets::note(
                    ui,
                    i18n::tr(
                        locale,
                        "Carga una fuente en Recursos para añadir texto.",
                        "Load a font in Resources to add text.",
                    ),
                );
            }

            if let Some(image) = &image {
                if widgets::wide_button_click_sized(
                    ui,
                    width,
                    icons::IMAGE,
                    i18n::tr(locale, "+ Añadir imagen", "+ Add image"),
                    hint,
                    theme::INFO,
                ) {
                    actions.push(LeftAction::AddUiImage(image.clone()));
                }
            } else {
                widgets::note(
                    ui,
                    i18n::tr(
                        locale,
                        "Carga una imagen HUD en Recursos para añadir imágenes.",
                        "Load a HUD image in Resources to add images.",
                    ),
                );
            }

            widgets::note(
                ui,
                i18n::tr(
                    locale,
                    "Añadir botones y el editor visual de HUD siguen en la UI React.",
                    "Adding buttons and the visual HUD editor still live in the React UI.",
                ),
            );
        },
    );

    sub(
        ui,
        state,
        "ui_menu",
        icons::LIST,
        i18n::tr(locale, "Menú de UI", "UI Menu"),
        |ui| {
            widgets::note(
                ui,
                i18n::tr(
                    locale,
                    "La lista de pantallas y menús se gestiona en la UI React por ahora.",
                    "The screen and menu list is managed in the React UI for now.",
                ),
            );
        },
    );
}

fn tools_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    is_3d: bool,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    // Tarjetas 64×64 como React; en 3D hay 4 y la cuarta cae a una segunda fila.
    type ToolCard = (
        Option<&'static str>,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        egui::Color32,
    );
    let mut cards: Vec<ToolCard> = vec![
        (
            Some("draw_collider"),
            icons::WALL,
            i18n::tr(locale, "Crear", "Create"),
            i18n::tr(locale, "Muro", "Wall"),
            i18n::tr(locale, "Colisionador", "Collider"),
            theme::INFO,
        ),
        (
            Some("draw_execution_area"),
            icons::CODE_BLOCK,
            i18n::tr(locale, "Crear", "Create"),
            "Trigger",
            i18n::tr(locale, "Área de ejecución", "Execution area"),
            theme::DANGER,
        ),
        (
            None,
            icons::WRENCH,
            i18n::tr(locale, "Construcción", "Construction"),
            i18n::tr(locale, "Rápida", "Quick"),
            i18n::tr(locale, "Quick build", "Quick build"),
            theme::WARNING,
        ),
    ];

    if is_3d {
        cards.push((
            Some("socket_config"),
            icons::TREE_STRUCTURE,
            "Socket",
            "Config",
            i18n::tr(locale, "Sockets", "Sockets"),
            theme::ACCENT,
        ));
    }

    for row in cards.chunks(3) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            let count = row.len();
            let gap = if count > 1 {
                ((ui.available_width() - 64.0 * count as f32) / (count - 1) as f32).max(4.0)
            } else {
                0.0
            };

            for (index, (tool, icon, top, bottom, label, color)) in row.iter().enumerate() {
                if index > 0 {
                    ui.add_space(gap);
                }
                match tool {
                    Some("socket_config") => {
                        if widgets::tool_card_click(
                            ui, icon, top, bottom, label, *color, false, hint,
                        ) {
                            actions.push(LeftAction::OpenSockets);
                        }
                    }
                    Some(tool) => {
                        let active = state.active_tool == Some(*tool);
                        if widgets::tool_card_click(
                            ui, icon, top, bottom, label, *color, active, hint,
                        ) {
                            let next = if active { "" } else { *tool };
                            actions.push(LeftAction::SetActiveTool(next));
                        }
                    }
                    None => widgets::tool_card(
                        ui,
                        icon,
                        top,
                        bottom,
                        label,
                        *color,
                        i18n::tr(
                            locale,
                            "Se configura desde la UI React por ahora",
                            "Configured from the React UI for now",
                        ),
                    ),
                }
            }
        });
    }

    widgets::tiny(
        ui,
        i18n::tr(
            locale,
            "Colisionador y trigger: activa y haz clic en el viewport. Quick build sigue en React; los sockets se configuran en esta ventana.",
            "Collider and trigger: activate and click in the viewport. Quick build still lives in React; sockets are configured in this window.",
        ),
    );
}

fn controls_body(
    ui: &mut egui::Ui,
    locale: Locale,
    _hint: &str,
    controls: Option<&ControlSummary>,
) {
    let Some(controls) = controls else {
        widgets::note(
            ui,
            i18n::tr(
                locale,
                "Aún no hay jugador en la escena.",
                "No player in the scene yet.",
            ),
        );
        return;
    };

    widgets::caption(
        ui,
        i18n::tr(locale, "Controles del jugador", "Player controls"),
    );
    if let Some(name) = &controls.player_name {
        widgets::tiny(ui, name);
    }

    widgets::label_value(
        ui,
        i18n::tr(locale, "Teclado + ratón", "Keyboard + mouse"),
        &format_controls(&controls.keyboard, locale),
    );
    widgets::label_value(
        ui,
        i18n::tr(locale, "Mandos", "Gamepads"),
        &format_controls(&controls.gamepad, locale),
    );
    widgets::note(
        ui,
        i18n::tr(
            locale,
            "Editar los bindings requiere el editor de scripts; de momento se hace desde la UI React.",
            "Editing bindings requires the script editor; for now it is done from the React UI.",
        ),
    );
}

fn format_controls(keys: &[String], locale: Locale) -> String {
    if keys.is_empty() {
        i18n::tr(locale, "Sin bindings", "No bindings").to_string()
    } else {
        keys.join(", ")
    }
}

fn footer(ui: &mut egui::Ui, locale: Locale, hint: &str) -> Option<Locale> {
    widgets::wide_button(
        ui,
        icons::PUZZLE_PIECE,
        i18n::tr(locale, "Plugins", "Plugins"),
        hint,
        theme::OUTLINE_SECONDARY,
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
            i18n::tr(locale, "Guia de usuario", "User guide"),
            hint,
            theme::INFO,
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
