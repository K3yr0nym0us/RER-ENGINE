use super::*;

pub(super) fn resources_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    assets: &AssetLists,
    context: ProjectContext,
    locale: Locale,
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
    actions: &mut Vec<LeftAction>,
) {
    sub(ui, state, kind.id(), icon, title, |ui| {
        let width = ui.available_width();
        if widgets::wide_button_click_sized(
            ui,
            width,
            icons::PLUS,
            import_label(kind, locale),
            import_tooltip(kind, locale),
            crate::editor::theme::ACCENT,
        ) {
            actions.push(LeftAction::ImportAsset(kind));
        }

        let list = assets.assets(kind);
        if list.is_empty() {
            widgets::tiny(
                ui,
                crate::editor::i18n::tr(locale, "Sin recursos cargados", "No assets loaded"),
            );
            return;
        }

        for asset in list {
            ui.horizontal(|ui| {
                let name_width = (ui.available_width() - 24.0).max(0.0);
                ui.add_sized(
                    [name_width, 16.0],
                    egui::Label::new(
                        RichText::new(&asset.name)
                            .size(12.0)
                            .color(crate::editor::theme::TEXT),
                    )
                    .truncate(),
                );
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
            });
        }
    });
}

pub(super) fn import_label(kind: AssetKind, locale: Locale) -> &'static str {
    match kind {
        AssetKind::Model => crate::editor::i18n::tr(locale, "+ Cargar modelo", "+ Load model"),
        AssetKind::Sprite => crate::editor::i18n::tr(locale, "+ Cargar sprite", "+ Load sprite"),
        AssetKind::Sound => crate::editor::i18n::tr(locale, "+ Cargar sonido", "+ Load sound"),
        AssetKind::Font => crate::editor::i18n::tr(locale, "+ Cargar fuente", "+ Load font"),
        AssetKind::HudImage => crate::editor::i18n::tr(locale, "+ Cargar imagen", "+ Load image"),
        AssetKind::Background => {
            crate::editor::i18n::tr(locale, "+ Cargar fondo", "+ Load background")
        }
    }
}

fn import_tooltip(kind: AssetKind, locale: Locale) -> &'static str {
    match kind {
        AssetKind::Model => crate::editor::i18n::tr(
            locale,
            "Importar modelo al proyecto",
            "Import model into project",
        ),
        AssetKind::Sprite => crate::editor::i18n::tr(
            locale,
            "Importar sprite al proyecto",
            "Import sprite into project",
        ),
        AssetKind::Sound => crate::editor::i18n::tr(
            locale,
            "Importar sonido al proyecto",
            "Import sound into project",
        ),
        AssetKind::Font => crate::editor::i18n::tr(
            locale,
            "Importar fuente al proyecto",
            "Import font into project",
        ),
        AssetKind::HudImage => crate::editor::i18n::tr(
            locale,
            "Importar imagen al proyecto",
            "Import image into project",
        ),
        AssetKind::Background => crate::editor::i18n::tr(
            locale,
            "Importar fondo al proyecto",
            "Import background into project",
        ),
    }
}
