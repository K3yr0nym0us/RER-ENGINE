//! Sección "Generador de mundos": configuración de terreno procedural y
//! distribución de blueprints mediante el plugin `world-gen` (DLL nativa).
//!
//! Los comandos que envía son los mismos `world_gen_*` del contrato del
//! engine; la DLL se selecciona una vez por sesión con un diálogo de archivo.

use super::*;

/// Estado de una feature del terreno (activada + cuota máxima).
#[derive(Debug, Clone)]
pub(crate) struct WorldGenFeature {
    pub(crate) enabled: bool,
    pub(crate) max_count: u32,
}

impl WorldGenFeature {
    fn new(enabled: bool, max_count: u32) -> Self {
        Self { enabled, max_count }
    }
}

/// Estado persistente del panel dentro de la sesión del editor.
#[derive(Debug, Clone)]
pub(crate) struct WorldGenPanelState {
    /// DLL instalada seleccionada (una vez por sesión).
    pub(crate) dll_path: Option<String>,
    pub(crate) seed: u32,
    pub(crate) width: f32,
    pub(crate) length: f32,
    pub(crate) height: f32,
    pub(crate) voxel: f32,
    pub(crate) mountains: WorldGenFeature,
    pub(crate) hills: WorldGenFeature,
    pub(crate) valleys: WorldGenFeature,
    pub(crate) plains: WorldGenFeature,
    pub(crate) cliffs: WorldGenFeature,
    pub(crate) lakes: WorldGenFeature,
    pub(crate) rivers: u32,
    pub(crate) caves: u32,
    pub(crate) waterfalls: u32,
    pub(crate) biome_grassland: bool,
    pub(crate) biome_forest: bool,
    pub(crate) biome_desert: bool,
    pub(crate) biome_snow: bool,
    pub(crate) biome_volcanic: bool,
    // Scatter.
    pub(crate) scatter_model: Option<String>,
    pub(crate) scatter_max_instances: u32,
    pub(crate) scatter_density_pct: f32,
    pub(crate) scatter_min_distance: f32,
    pub(crate) scatter_max_slope_deg: f32,
    pub(crate) scatter_altitude_min: f32,
    pub(crate) scatter_altitude_max: f32,
    pub(crate) scatter_scale_min_pct: f32,
    pub(crate) scatter_scale_max_pct: f32,
    pub(crate) scatter_random_rotation: bool,
    pub(crate) scatter_cluster_enabled: bool,
    pub(crate) scatter_cluster_size: u32,
}

impl Default for WorldGenPanelState {
    fn default() -> Self {
        Self {
            dll_path: None,
            seed: 1,
            width: 512.0,
            length: 512.0,
            height: 128.0,
            voxel: 1.0,
            mountains: WorldGenFeature::new(true, 15),
            hills: WorldGenFeature::new(true, 30),
            valleys: WorldGenFeature::new(true, 8),
            plains: WorldGenFeature::new(true, 10),
            cliffs: WorldGenFeature::new(false, 12),
            lakes: WorldGenFeature::new(true, 8),
            rivers: 5,
            caves: 20,
            waterfalls: 3,
            biome_grassland: true,
            biome_forest: true,
            biome_desert: false,
            biome_snow: false,
            biome_volcanic: false,
            scatter_model: None,
            scatter_max_instances: 200,
            scatter_density_pct: 50.0,
            scatter_min_distance: 4.0,
            scatter_max_slope_deg: 35.0,
            scatter_altitude_min: 0.0,
            scatter_altitude_max: 200.0,
            scatter_scale_min_pct: 80.0,
            scatter_scale_max_pct: 130.0,
            scatter_random_rotation: true,
            scatter_cluster_enabled: false,
            scatter_cluster_size: 12,
        }
    }
}

impl WorldGenPanelState {
    /// JSON camelCase de `WorldGenConfig` del crate worldgen.
    fn config_json(&self) -> String {
        serde_json::json!({
            "seed": self.seed,
            "bounds": {
                "width": self.width,
                "length": self.length,
                "height": self.height,
            },
            "resolution": { "voxelSize": self.voxel, "lodEnabled": true },
            "mountains": {
                "enabled": self.mountains.enabled,
                "maxCount": self.mountains.max_count,
                "minSize": 60.0,
                "maxSize": 180.0,
                "minSeparation": 120.0,
                "maxHeight": 90.0,
                "sharpness": 3.0,
            },
            "hills": {
                "enabled": self.hills.enabled,
                "maxCount": self.hills.max_count,
                "minSize": 40.0,
                "maxSize": 120.0,
                "minSeparation": 80.0,
                "maxHeight": 24.0,
            },
            "valleys": {
                "enabled": self.valleys.enabled,
                "maxCount": self.valleys.max_count,
                "minSize": 80.0,
                "maxSize": 200.0,
                "minSeparation": 150.0,
                "maxDepth": 30.0,
            },
            "plains": {
                "enabled": self.plains.enabled,
                "maxCount": self.plains.max_count,
                "minSize": 100.0,
                "maxSize": 260.0,
                "minSeparation": 120.0,
                "maxRoughness": 1.5,
            },
            "cliffs": {
                "enabled": self.cliffs.enabled,
                "maxCount": self.cliffs.max_count,
                "minSize": 30.0,
                "maxSize": 120.0,
                "minSeparation": 100.0,
                "dropHeight": 18.0,
            },
            "lakes": {
                "enabled": self.lakes.enabled,
                "maxCount": self.lakes.max_count,
                "minSize": 20.0,
                "maxSize": 90.0,
                "minSeparation": 100.0,
                "maxDepth": 12.0,
            },
            "rivers": {
                "enabled": self.rivers > 0,
                "maxCount": self.rivers,
                "lengthMin": 150.0,
                "lengthMax": 600.0,
                "width": 4.0,
                "depth": 3.0,
            },
            "caves": {
                "enabled": self.caves > 0,
                "maxCount": self.caves,
                "depthMin": 10.0,
                "depthMax": 50.0,
                "radius": 3.0,
                "roomChance": 0.2,
                "segments": 18,
            },
            "waterfalls": {
                "enabled": self.waterfalls > 0,
                "maxCount": self.waterfalls,
                "minDrop": 6.0,
            },
            "biomes": {
                "grassland": self.biome_grassland,
                "forest": self.biome_forest,
                "desert": self.biome_desert,
                "snow": self.biome_snow,
                "volcanic": self.biome_volcanic,
            },
        })
        .to_string()
    }

    /// Reglas de scatter (camelCase de `ScatterRules` + `blueprintId`).
    fn scatter_rules_json(&self, blueprint_id: &str) -> String {
        serde_json::json!({
            "blueprintId": blueprint_id,
            "biomes": [],
            "altitudeMin": self.scatter_altitude_min,
            "altitudeMax": self.scatter_altitude_max,
            "maxSlopeDeg": self.scatter_max_slope_deg,
            "maxInstances": self.scatter_max_instances,
            "density": self.scatter_density_pct / 100.0,
            "scaleMin": self.scatter_scale_min_pct / 100.0,
            "scaleMax": self.scatter_scale_max_pct / 100.0,
            "randomRotation": self.scatter_random_rotation,
            "minDistance": self.scatter_min_distance,
            "cluster": {
                "enabled": self.scatter_cluster_enabled,
                "size": self.scatter_cluster_size,
                "radius": 0.0,
            },
        })
        .to_string()
    }
}

/// Id estable de la blueprint sintetizada para un modelo del proyecto.
pub(crate) fn scatter_blueprint_id(model_path: &str) -> String {
    format!("worldgen:{model_path}")
}

pub(super) fn worldgen_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    models: &[crate::editor::assets::AssetEntry],
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    let panel = &mut state.worldgen;

    // --- DLL del plugin ------------------------------------------------------
    let dll_label = match &panel.dll_path {
        Some(path) => format!("DLL: {}", file_name_of(path)),
        None => crate::editor::i18n::tr(locale, "Cargar DLL del plugin…", "Load plugin DLL…")
            .to_string(),
    };
    if widgets::wide_button_click_sized(
        ui,
        ui.available_width(),
        icons::CUBE,
        &dll_label,
        hint,
        crate::editor::theme::OUTLINE_SECONDARY,
    ) && let Some(path) = rfd::FileDialog::new()
        .add_filter("DLL", &["dll"])
        .pick_file()
        && let Some(text) = path.to_str()
    {
        panel.dll_path = Some(text.to_string());
        actions.push(LeftAction::WorldGenSetLibraryPath(text.to_string()));
    }
    ui.add_space(6.0);

    // --- Terreno -------------------------------------------------------------
    widgets::caption(ui, crate::editor::i18n::tr(locale, "Terreno", "Terrain"));
    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(locale, "Semilla", "Seed"));
        ui.add(
            egui::DragValue::new(&mut panel.seed)
                .range(0..=u32::MAX)
                .speed(1.0),
        );
    });
    ui.horizontal(|ui| {
        for (label, value, range) in [
            ("Ancho", &mut panel.width, 16.0..=4096.0),
            ("Largo", &mut panel.length, 16.0..=4096.0),
            ("Alto", &mut panel.height, 16.0..=1024.0),
        ] {
            ui.label(crate::editor::i18n::tr(locale, label, label));
            ui.add(
                egui::DragValue::new(value)
                    .range(range)
                    .speed(8.0)
                    .suffix(" m"),
            );
        }
    });
    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(locale, "Voxel", "Voxel"));
        ui.add(
            egui::DragValue::new(&mut panel.voxel)
                .range(0.25..=4.0)
                .speed(0.25)
                .suffix(" m"),
        );
        widgets::hint(
            ui,
            crate::editor::i18n::tr(
                locale,
                "menor = más detalle (más lento)",
                "smaller = more detail (slower)",
            ),
        );
    });

    let feature_row = |ui: &mut egui::Ui,
                       label: &str,
                       feature: &mut WorldGenFeature,
                       actions: &mut Vec<LeftAction>| {
        ui.horizontal(|ui| {
            widgets::switch_value(ui, &mut feature.enabled, label);
            ui.label(
                egui::RichText::new(label)
                    .size(12.0)
                    .color(crate::editor::theme::TEXT_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let _ = actions;
                ui.add(
                    egui::DragValue::new(&mut feature.max_count)
                        .range(0..=500)
                        .speed(1.0),
                );
            });
        });
    };
    feature_row(
        ui,
        crate::editor::i18n::tr(locale, "Montañas", "Mountains"),
        &mut panel.mountains,
        actions,
    );
    feature_row(
        ui,
        crate::editor::i18n::tr(locale, "Colinas", "Hills"),
        &mut panel.hills,
        actions,
    );
    feature_row(
        ui,
        crate::editor::i18n::tr(locale, "Valles", "Valleys"),
        &mut panel.valleys,
        actions,
    );
    feature_row(
        ui,
        crate::editor::i18n::tr(locale, "Llanuras", "Plains"),
        &mut panel.plains,
        actions,
    );
    feature_row(
        ui,
        crate::editor::i18n::tr(locale, "Acantilados", "Cliffs"),
        &mut panel.cliffs,
        actions,
    );
    feature_row(
        ui,
        crate::editor::i18n::tr(locale, "Lagos", "Lakes"),
        &mut panel.lakes,
        actions,
    );

    for (label, value) in [
        ("Ríos", &mut panel.rivers),
        ("Cuevas", &mut panel.caves),
        ("Cascadas", &mut panel.waterfalls),
    ] {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(crate::editor::i18n::tr(locale, label, label))
                    .size(12.0)
                    .color(crate::editor::theme::TEXT_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::DragValue::new(value).range(0..=200).speed(1.0));
            });
        });
    }

    widgets::caption(ui, crate::editor::i18n::tr(locale, "Biomas", "Biomes"));
    ui.horizontal_wrapped(|ui| {
        for (label, value) in [
            ("Pradera", &mut panel.biome_grassland),
            ("Bosque", &mut panel.biome_forest),
            ("Desierto", &mut panel.biome_desert),
            ("Nieve", &mut panel.biome_snow),
            ("Volcánico", &mut panel.biome_volcanic),
        ] {
            widgets::switch_value(ui, value, crate::editor::i18n::tr(locale, label, label));
            ui.label(
                egui::RichText::new(crate::editor::i18n::tr(locale, label, label))
                    .size(11.0)
                    .color(crate::editor::theme::TEXT_MUTED),
            );
            ui.add_space(4.0);
        }
    });

    ui.add_space(8.0);
    let width = ui.available_width();
    ui.horizontal(|ui| {
        let generate = widgets::wide_button_click_sized(
            ui,
            width * 0.5,
            icons::PLAY,
            crate::editor::i18n::tr(locale, "Generar mundo", "Generate world"),
            hint,
            crate::editor::theme::ACCENT,
        );
        ui.add_space(4.0);
        let stop = widgets::wide_button_click_sized(
            ui,
            (width * 0.5 - 4.0) * 0.5,
            icons::STOP,
            crate::editor::i18n::tr(locale, "Detener", "Stop"),
            hint,
            crate::editor::theme::OUTLINE_SECONDARY,
        );
        let clear = widgets::wide_button_click_sized(
            ui,
            (width * 0.5 - 4.0) * 0.5,
            icons::TRASH,
            crate::editor::i18n::tr(locale, "Limpiar", "Clear"),
            hint,
            crate::editor::theme::DANGER,
        );
        if generate {
            actions.push(LeftAction::WorldGenGenerate {
                config_json: panel.config_json(),
            });
        }
        if stop {
            actions.push(LeftAction::WorldGenStop);
        }
        if clear {
            actions.push(LeftAction::WorldGenReset);
        }
    });

    // --- Scatter de blueprints ----------------------------------------------
    ui.add_space(8.0);
    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Distribuir blueprints", "Distribute blueprints"),
    );
    if models.is_empty() {
        widgets::note(
            ui,
            crate::editor::i18n::tr(
                locale,
                "Importa modelos al proyecto primero (sección Recursos).",
                "Import models into the project first (Resources section).",
            ),
        );
        return;
    }
    let options: Vec<(String, String)> = models
        .iter()
        .map(|entry| (entry.path.clone(), entry.name.clone()))
        .collect();
    if widgets::combo_string(
        ui,
        "worldgen_scatter_model",
        ui.available_width(),
        &options,
        &mut panel.scatter_model,
    ) {
        // Cambio de modelo: las colocaciones previas de la vista quedan
        // descartadas (el scatter genera una lista nueva cada vez).
    }

    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(
            locale,
            "Máx. instancias",
            "Max instances",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(&mut panel.scatter_max_instances)
                    .range(0..=200_000)
                    .speed(10.0),
            );
        });
    });
    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(
            locale,
            "Distancia mín. (m)",
            "Min distance (m)",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(&mut panel.scatter_min_distance)
                    .range(0.5..=50.0)
                    .speed(0.5),
            );
        });
    });
    widgets::range_slider(
        ui,
        &mut panel.scatter_density_pct,
        0.0,
        100.0,
        5.0,
        crate::editor::theme::INFO,
    );
    widgets::range_end_labels(
        ui,
        crate::editor::i18n::tr(locale, "Densidad", "Density"),
        &format!("{:.0}%", panel.scatter_density_pct),
    );
    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(
            locale,
            "Pendiente máx. (°)",
            "Max slope (°)",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(&mut panel.scatter_max_slope_deg)
                    .range(0.0..=90.0)
                    .speed(1.0),
            );
        });
    });
    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(
            locale,
            "Altitud mín.",
            "Min altitude",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(&mut panel.scatter_altitude_min)
                    .range(0.0..=4000.0)
                    .speed(5.0)
                    .suffix(" m"),
            );
        });
    });
    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(
            locale,
            "Altitud máx.",
            "Max altitude",
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(&mut panel.scatter_altitude_max)
                    .range(0.0..=4000.0)
                    .speed(5.0)
                    .suffix(" m"),
            );
        });
    });
    ui.horizontal(|ui| {
        ui.label(crate::editor::i18n::tr(locale, "Escala (%)", "Scale (%)"));
        ui.add(
            egui::DragValue::new(&mut panel.scatter_scale_min_pct)
                .range(1.0..=500.0)
                .speed(5.0),
        );
        ui.label("–");
        ui.add(
            egui::DragValue::new(&mut panel.scatter_scale_max_pct)
                .range(1.0..=500.0)
                .speed(5.0),
        );
    });
    ui.horizontal(|ui| {
        widgets::switch_value(
            ui,
            &mut panel.scatter_random_rotation,
            crate::editor::i18n::tr(locale, "Rotación aleatoria", "Random rotation"),
        );
        ui.label(
            egui::RichText::new(crate::editor::i18n::tr(
                locale,
                "Rotación aleatoria",
                "Random rotation",
            ))
            .size(11.0)
            .color(crate::editor::theme::TEXT_MUTED),
        );
        ui.add_space(8.0);
        widgets::switch_value(
            ui,
            &mut panel.scatter_cluster_enabled,
            crate::editor::i18n::tr(locale, "Clústeres", "Clusters"),
        );
        ui.label(
            egui::RichText::new(crate::editor::i18n::tr(locale, "Clústeres", "Clusters"))
                .size(11.0)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        if panel.scatter_cluster_enabled {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::DragValue::new(&mut panel.scatter_cluster_size)
                        .range(2..=200)
                        .speed(1.0),
                );
            });
        }
    });

    ui.add_space(4.0);
    if widgets::wide_button_click_sized(
        ui,
        ui.available_width(),
        icons::TREE_STRUCTURE,
        crate::editor::i18n::tr(locale, "Distribuir en el mundo", "Distribute in world"),
        hint,
        crate::editor::theme::INFO,
    ) && let Some(model) = panel.scatter_model.clone()
    {
        let blueprint_id = scatter_blueprint_id(&model);
        let rules_json = panel.scatter_rules_json(&blueprint_id);
        actions.push(LeftAction::WorldGenScatter {
            blueprint_id,
            model,
            rules_json,
        });
    }
}

fn file_name_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}
