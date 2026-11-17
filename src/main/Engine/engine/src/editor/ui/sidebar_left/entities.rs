use super::*;

/// Categoría de biblioteca que corresponde a cada acordeón de «Crear entidad».
/// Debe coincidir con `create_entity` (`mod.rs`) y con
/// `normalize_model_library_category` del motor.
fn category_for_entity_kind(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Character => "character",
        EntityKind::Environment => "environment",
        EntityKind::Object => "object",
        EntityKind::Weapon => "weapon",
        EntityKind::Projectile => "projectile",
    }
}

/// `true` si el modelo debe ofrecerse en el acordeón de `kind`.
/// Los modelos importados antes de la modal nativa (o con una categoría que el
/// motor no normaliza) se tratan como `object`, igual que hace el selector del
/// renderer (`CreateEntityFromModelModalBody`).
fn asset_matches_entity_kind(
    asset: &crate::editor::assets::AssetEntry,
    kind: EntityKind,
) -> bool {
    let expected = category_for_entity_kind(kind);
    match crate::engine3d::ipc::normalize_model_library_category(asset.category.as_deref()) {
        Some(category) => category == expected,
        None => expected == "object",
    }
}

pub(super) fn entities_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    assets: &AssetLists,
    context: ProjectContext,
    locale: Locale,
    active_scene_id: u64,
    actions: &mut Vec<LeftAction>,
) {
    let mut kinds = vec![
        (
            EntityKind::Character,
            icons::USER,
            crate::editor::i18n::tr(locale, "Personajes", "Characters"),
            crate::editor::theme::SUCCESS,
        ),
        (
            EntityKind::Environment,
            icons::TREE,
            crate::editor::i18n::tr(locale, "Escenario", "Environment"),
            crate::editor::theme::INFO,
        ),
        (
            EntityKind::Object,
            icons::CUBE,
            crate::editor::i18n::tr(locale, "Objetos", "Objects"),
            crate::editor::theme::WARNING,
        ),
    ];
    if context.is_3d() {
        kinds.push((
            EntityKind::Weapon,
            icons::SHIELD,
            crate::editor::i18n::tr(locale, "Armas", "Weapons"),
            crate::editor::theme::DANGER,
        ));
    }
    kinds.push((
        EntityKind::Projectile,
        icons::CIRCLE,
        crate::editor::i18n::tr(locale, "Proyectiles", "Projectiles"),
        crate::editor::theme::INFO,
    ));

    for (kind, icon, title, color) in kinds {
        entity_section(
            ui,
            state,
            kind,
            icon,
            title,
            color,
            assets,
            context,
            locale,
            active_scene_id,
            actions,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn entity_section(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    kind: EntityKind,
    icon: &str,
    title: &str,
    color: egui::Color32,
    assets: &AssetLists,
    context: ProjectContext,
    locale: Locale,
    active_scene_id: u64,
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
        // Solo modelos globales o de la escena activa (los de otras escenas se
        // globalizan desde «Cargar desde otra escena»). En 3D, además, cada
        // acordeón ofrece únicamente los modelos de su categoría: la modal de
        // importación guarda la categoría en el recurso y el motor la reporta
        // en `models_list`. En 2D los sprites no tienen categoría y se listan
        // todos, como antes.
        let list: Vec<&crate::editor::assets::AssetEntry> = assets
            .assets(asset_kind)
            .iter()
            .filter(|asset| asset.is_visible_in_scene(active_scene_id))
            .filter(|asset| !context.is_3d() || asset_matches_entity_kind(asset, kind))
            .collect();
        if list.is_empty() {
            widgets::note(
                ui,
                crate::editor::i18n::tr(
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
            create_tooltip(kind, context, locale),
            color,
        ) && let Some(asset) = selected.clone()
        {
            actions.push(LeftAction::CreateEntity { kind, asset });
        }
    });
}

pub(super) fn create_label(
    kind: EntityKind,
    context: ProjectContext,
    locale: Locale,
) -> &'static str {
    if context.is_3d() {
        match kind {
            EntityKind::Character => crate::editor::i18n::tr(
                locale,
                "Crear personaje desde modelo",
                "Create character from model",
            ),
            EntityKind::Environment => crate::editor::i18n::tr(
                locale,
                "Crear escenario desde modelo",
                "Create environment from model",
            ),
            EntityKind::Object => crate::editor::i18n::tr(
                locale,
                "Crear objeto desde modelo",
                "Create object from model",
            ),
            EntityKind::Weapon => crate::editor::i18n::tr(
                locale,
                "Crear arma desde modelo",
                "Create weapon from model",
            ),
            EntityKind::Projectile => crate::editor::i18n::tr(
                locale,
                "Crear proyectil desde modelo",
                "Create projectile from model",
            ),
        }
    } else {
        match kind {
            EntityKind::Character => {
                crate::editor::i18n::tr(locale, "Crear personaje", "Create character")
            }
            EntityKind::Environment => {
                crate::editor::i18n::tr(locale, "Añadir escenario", "Add environment")
            }
            EntityKind::Object => crate::editor::i18n::tr(locale, "Añadir objeto", "Add object"),
            EntityKind::Weapon => crate::editor::i18n::tr(locale, "Crear arma", "Create weapon"),
            EntityKind::Projectile => {
                crate::editor::i18n::tr(locale, "Añadir proyectil", "Add projectile")
            }
        }
    }
}

fn create_tooltip(kind: EntityKind, context: ProjectContext, locale: Locale) -> &'static str {
    if context.is_3d() {
        return crate::editor::i18n::tr(
            locale,
            "Crea la entidad en la escena con el modelo seleccionado",
            "Spawn the entity in the scene using the selected model",
        );
    }
    match kind {
        EntityKind::Projectile => crate::editor::i18n::tr(
            locale,
            "Crea el proyectil con el sprite seleccionado",
            "Create the projectile from the selected sprite",
        ),
        _ => crate::editor::i18n::tr(
            locale,
            "Crea la entidad con el sprite seleccionado",
            "Create the entity from the selected sprite",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(category: Option<&str>) -> crate::editor::assets::AssetEntry {
        crate::editor::assets::AssetEntry {
            name: "Modelo".into(),
            path: "model_test".into(),
            model_id: Some("model_test".into()),
            asset: None,
            category: category.map(str::to_string),
            state: Some("ready".into()),
            scene_id: None,
        }
    }

    const CATEGORIES: [EntityKind; 5] = [
        EntityKind::Character,
        EntityKind::Environment,
        EntityKind::Object,
        EntityKind::Weapon,
        EntityKind::Projectile,
    ];

    #[test]
    fn each_accordion_lists_only_its_own_category() {
        for kind in CATEGORIES {
            for other in CATEGORIES {
                let belongs = kind == other;
                assert_eq!(
                    asset_matches_entity_kind(
                        &model(Some(category_for_entity_kind(other))),
                        kind
                    ),
                    belongs,
                    "acordeón {:?} con modelo {:?}",
                    kind,
                    other
                );
            }
        }
    }

    #[test]
    fn uncategorized_or_invalid_models_are_objects_only() {
        for category in [None, Some("desconocida")] {
            assert!(asset_matches_entity_kind(&model(category), EntityKind::Object));
            for kind in [EntityKind::Character, EntityKind::Environment, EntityKind::Weapon, EntityKind::Projectile] {
                assert!(!asset_matches_entity_kind(&model(category), kind));
            }
        }
    }

    #[test]
    fn base_player_model_is_only_offered_as_character() {
        // `bundled.rs` registra el jugador base con category = "character".
        let player = model(Some("character"));
        assert!(asset_matches_entity_kind(&player, EntityKind::Character));
        for kind in [EntityKind::Environment, EntityKind::Object, EntityKind::Weapon, EntityKind::Projectile] {
            assert!(!asset_matches_entity_kind(&player, kind));
        }
    }
}
