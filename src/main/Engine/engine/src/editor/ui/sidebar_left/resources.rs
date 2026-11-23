use super::*;

/// Categorías de la biblioteca de modelos. Cada una es un sub-acordeón dentro
/// de Recursos → Modelos con su propio botón de importación directa.
const MODEL_CATEGORIES: [(&str, &str, &str, &str, &str); 7] = [
    (
        "character",
        "res_models_character",
        icons::USER,
        "Personajes",
        "Characters",
    ),
    (
        "environment",
        "res_models_environment",
        icons::TREE,
        "Entorno",
        "Environment",
    ),
    (
        "object",
        "res_models_object",
        icons::CUBE,
        "Objetos",
        "Objects",
    ),
    (
        "weapon",
        "res_models_weapon",
        icons::SHIELD,
        "Armas",
        "Weapons",
    ),
    (
        "projectile",
        "res_models_projectile",
        icons::CIRCLE,
        "Proyectiles",
        "Projectiles",
    ),
    (
        "animal",
        "res_models_animal",
        icons::PAW_PRINT,
        "Animales",
        "Animals",
    ),
    (
        "creature",
        "res_models_creature",
        icons::ALIEN,
        "Criaturas",
        "Creatures",
    ),
];

pub(super) fn resources_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    assets: &AssetLists,
    context: ProjectContext,
    locale: Locale,
    active_scene_id: u64,
    actions: &mut Vec<LeftAction>,
) {
    if context.is_3d() {
        asset_section(
            ui,
            state,
            AssetKind::Model,
            icons::CUBE,
            crate::editor::i18n::tr(locale, "Modelos", "Models"),
            assets,
            locale,
            active_scene_id,
            actions,
        );
    } else {
        asset_section(
            ui,
            state,
            AssetKind::Sprite,
            icons::SPARKLE,
            crate::editor::i18n::tr(locale, "Sprites", "Sprites"),
            assets,
            locale,
            active_scene_id,
            actions,
        );
    }

    asset_section(
        ui,
        state,
        AssetKind::Sound,
        icons::SPEAKER_HIGH,
        crate::editor::i18n::tr(locale, "Sonidos", "Sounds"),
        assets,
        locale,
        active_scene_id,
        actions,
    );
    asset_section(
        ui,
        state,
        AssetKind::Font,
        icons::TEXT_AA,
        crate::editor::i18n::tr(locale, "Fuentes", "Fonts"),
        assets,
        locale,
        active_scene_id,
        actions,
    );
    asset_section(
        ui,
        state,
        AssetKind::HudImage,
        icons::IMAGE,
        crate::editor::i18n::tr(locale, "Imágenes", "Images"),
        assets,
        locale,
        active_scene_id,
        actions,
    );

    if !context.is_3d() {
        asset_section(
            ui,
            state,
            AssetKind::Background,
            icons::IMAGE_SQUARE,
            crate::editor::i18n::tr(locale, "Fondos", "Backgrounds"),
            assets,
            locale,
            active_scene_id,
            actions,
        );
    }
}

pub(super) fn asset_section(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    kind: AssetKind,
    icon: &str,
    title: &str,
    assets: &AssetLists,
    locale: Locale,
    active_scene_id: u64,
    actions: &mut Vec<LeftAction>,
) {
    // Préstamos disjuntos: el acordeón de recursos usa `open_sub` y los
    // sub-acordeones de categoría usan `open_model_category`.
    let SidebarLeftState {
        open_sub,
        open_model_category,
        ..
    } = state;
    sub_at(ui, open_sub, kind.id(), icon, title, |ui| {
        let width = ui.available_width();

        if kind == AssetKind::Model {
            // Referencia un recurso de otra escena por path, sin reimportar.
            if widgets::wide_button_click_sized(
                ui,
                width,
                icons::STACK,
                crate::editor::i18n::tr(
                    locale,
                    "Cargar desde otra escena",
                    "Load from another scene",
                ),
                "",
                crate::editor::theme::OUTLINE_SECONDARY,
            ) {
                actions.push(LeftAction::OpenLoadFromScene);
            }
            ui.add_space(4.0);

            // Un acordeón por categoría: el botón carga directamente en ella,
            // sin pedir la categoría en una modal intermedia.
            for (category, id, cat_icon, title_es, title_en) in MODEL_CATEGORIES {
                let cat_title = crate::editor::i18n::tr(locale, title_es, title_en);
                sub_at(ui, open_model_category, id, cat_icon, cat_title, |ui| {
                    if widgets::wide_button_click_sized(
                        ui,
                        ui.available_width(),
                        icons::PLUS,
                        cat_title,
                        "",
                        crate::editor::theme::ACCENT,
                    ) {
                        actions.push(LeftAction::ImportModelCategory(category));
                    }
                    ui.add_space(4.0);

                    let list: Vec<&crate::editor::assets::AssetEntry> = assets
                        .assets(kind)
                        .iter()
                        .filter(|asset| asset.is_visible_in_scene(active_scene_id))
                        .filter(|asset| {
                            crate::engine3d::ipc::normalize_model_library_category(
                                asset.category.as_deref(),
                            )
                            .unwrap_or_else(|| "object".to_string())
                                == category
                        })
                        .collect();
                    asset_rows(ui, list, kind, locale, active_scene_id, actions);
                });
            }
            return;
        }

        if widgets::wide_button_click_sized(
            ui,
            width,
            icons::PLUS,
            import_label(kind, locale),
            "",
            crate::editor::theme::ACCENT,
        ) {
            actions.push(LeftAction::ImportAsset(kind));
        }

        // Solo modelos globales o de la escena activa: los de otras escenas se
        // traen desde «Cargar desde otra escena» (los globaliza).
        let list: Vec<&crate::editor::assets::AssetEntry> = assets
            .assets(kind)
            .iter()
            .filter(|asset| asset.is_visible_in_scene(active_scene_id))
            .collect();
        asset_rows(ui, list, kind, locale, active_scene_id, actions);
    });
}

/// Fila de recurso compartida por el listado plano y por los acordeones de
/// categoría de modelos: nombre, estado de carga, globalizar y borrar.
fn asset_rows(
    ui: &mut egui::Ui,
    list: Vec<&crate::editor::assets::AssetEntry>,
    kind: AssetKind,
    locale: Locale,
    active_scene_id: u64,
    actions: &mut Vec<LeftAction>,
) {
    if list.is_empty() {
        widgets::tiny(
            ui,
            crate::editor::i18n::tr(locale, "Sin recursos cargados", "No assets loaded"),
        );
        return;
    }

    for asset in list {
        let loading = asset.state.as_deref() == Some("importing");
        // «Globalizar»: modelo de la escena activa que aún está atado a ella.
        let can_globalize = !loading
            && kind == AssetKind::Model
            && asset.model_id.is_some()
            && asset
                .scene_id
                .is_some_and(|scene_id| u64::from(scene_id) == active_scene_id);
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .corner_radius(4)
            .stroke(egui::Stroke::new(1.0, crate::editor::theme::BORDER))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let trailing = if loading {
                        46.0
                    } else if can_globalize {
                        48.0
                    } else {
                        30.0
                    };
                    let name_width = (ui.available_width() - trailing).max(0.0);
                    ui.add_sized(
                        [name_width, 18.0],
                        egui::Label::new(
                            RichText::new(&asset.name)
                                .size(12.0)
                                .color(crate::editor::theme::TEXT),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if loading {
                            let _ = ui.add(egui::Spinner::new().size(14.0)).on_hover_text(
                                crate::editor::i18n::tr(
                                    locale,
                                    "Cargando modelo… espera antes de usarlo",
                                    "Loading model… wait before using it",
                                ),
                            );
                            // Botón deshabilitado: el recurso todavía no está listo.
                            ui.add_enabled_ui(false, |ui| {
                                let _ = widgets::icon_action(
                                    ui,
                                    icons::TRASH,
                                    18.0,
                                    crate::editor::theme::DANGER,
                                    crate::editor::i18n::tr(
                                        locale,
                                        "Espera a que el modelo termine de cargar",
                                        "Wait until the model finishes loading",
                                    ),
                                );
                            });
                        } else {
                            if widgets::icon_action(
                                ui,
                                icons::TRASH,
                                18.0,
                                crate::editor::theme::DANGER,
                                crate::editor::i18n::tr(locale, "Quitar recurso", "Remove asset"),
                            ) {
                                actions.push(LeftAction::OpenRemoveAsset {
                                    kind,
                                    path: asset.path.clone(),
                                    name: asset.name.clone(),
                                });
                            }
                            if can_globalize
                                && widgets::icon_action(
                                    ui,
                                    icons::GLOBE,
                                    18.0,
                                    crate::editor::theme::INFO,
                                    crate::editor::i18n::tr(
                                        locale,
                                        "Globalizar modelo",
                                        "Globalize model",
                                    ),
                                )
                            {
                                actions.push(LeftAction::GlobalizeModel {
                                    path: asset
                                        .model_id
                                        .clone()
                                        .unwrap_or_else(|| asset.path.clone()),
                                    name: asset.name.clone(),
                                });
                            }
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
}

pub(super) fn import_label(kind: AssetKind, locale: Locale) -> &'static str {
    match kind {
        AssetKind::Model => crate::editor::i18n::tr(locale, "Cargar modelo", "Load model"),
        AssetKind::Sprite => crate::editor::i18n::tr(locale, "Cargar sprite", "Load sprite"),
        AssetKind::Sound => crate::editor::i18n::tr(locale, "Cargar sonido", "Load sound"),
        AssetKind::Font => crate::editor::i18n::tr(locale, "Cargar fuente", "Load font"),
        AssetKind::HudImage => crate::editor::i18n::tr(locale, "Cargar imagen", "Load image"),
        AssetKind::Background => crate::editor::i18n::tr(locale, "Cargar fondo", "Load background"),
    }
}
