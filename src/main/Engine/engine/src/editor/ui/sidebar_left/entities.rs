use super::*;

pub(super) fn entities_body(
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
            ui, state, kind, icon, title, color, assets, context, locale, hint, actions,
        );
    }
}

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
            hint,
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
