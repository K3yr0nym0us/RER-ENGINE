//! Sección «Entorno y partículas» del sidebar derecho (F5).
//!
//! Dropdown de clima global (ninguno/lluvia/nieve/niebla) + intensidad, y lista de
//! emisores de escena con alta/edición (modal nativa `particles_config_modal`) y baja.
//! Sigue el patrón del sidebar: el cuerpo solo acumula [`WorldAction`] y `apply_action`
//! envía los comandos al motor cuando termina de dibujar. Los sliders usan el widget
//! pintado a mano de la app (`widgets::range_slider`), igual que el resto de secciones.

use egui::{RichText, Ui};
use egui_phosphor::regular as icons;
use serde_json::json;

use crate::editor::cli::Locale;
use crate::editor::i18n::tr;
use crate::editor::theme;
use crate::editor::ui::number_draft::NumberDraft;
use crate::editor::ui::widgets;

use super::WorldAction;

/// Base de ids asignados por la UI: altos para no chocar con los automáticos del
/// motor (0, 1, 2…) ni con el id reservado del clima (u64::MAX).
const UI_ID_BASE: u64 = 0x6000_0000_0000_0000;

/// Opciones de clima del dropdown (wire del comando `set_particle_environment`).
const WEATHER_OPTIONS: [(&str, &str, &str); 4] = [
    ("none", "Ninguno", "None"),
    ("rain", "Lluvia", "Rain"),
    ("snow", "Nieve", "Snow"),
    ("fog", "Niebla", "Fog"),
];

/// Materiales para la alta manual de emisores.
pub(crate) const MATERIAL_OPTIONS: [(&str, &str, &str); 5] = [
    ("fire", "Fuego", "Fire"),
    ("smoke", "Humo", "Smoke"),
    ("fog", "Niebla", "Fog"),
    ("sand", "Arena", "Arena"),
    ("water", "Agua", "Water"),
];

/// Valores planos de un emisor (preset o snapshot). Los borradores de la UI se
/// sincronizan desde aquí y al confirmar se vuelven a leer de los drafts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EmitterValues {
    pub position: [f32; 3],
    pub rate: f32,
    pub size: f32,
    pub life: f32,
    pub gravity: f32,
    pub drag: f32,
    pub turbulence: f32,
    pub cohesion: f32,
    pub ground_y: f32,
    pub wind: [f32; 2],
    pub ground_collision: bool,
    pub accumulate: bool,
    pub splash: bool,
}

/// Fila de un emisor de escena y borrador de la modal (una sola estructura: la modal
/// edita un clon y al confirmar reemplaza la fila). Los `NumberDraft` son la fuente
/// única del texto; el valor se lee con `values()` al confirmar.
#[derive(Clone)]
pub(crate) struct EmitterRow {
    pub(crate) id: u64,
    pub(crate) material: &'static str,
    pub(crate) position: [NumberDraft; 3],
    pub(crate) rate: NumberDraft,
    pub(crate) size: NumberDraft,
    pub(crate) life: NumberDraft,
    pub(crate) gravity: NumberDraft,
    pub(crate) drag: NumberDraft,
    pub(crate) turbulence: NumberDraft,
    pub(crate) cohesion: NumberDraft,
    pub(crate) ground_y: NumberDraft,
    pub(crate) wind: [NumberDraft; 2],
    pub(crate) ground_collision: bool,
    pub(crate) accumulate: bool,
    pub(crate) splash: bool,
}

impl EmitterRow {
    fn from_values(values: &EmitterValues) -> Self {
        let mut row = Self {
            id: 0,
            material: "fire",
            position: [
                NumberDraft::default(),
                NumberDraft::default(),
                NumberDraft::default(),
            ],
            rate: NumberDraft::default(),
            size: NumberDraft::default(),
            life: NumberDraft::default(),
            gravity: NumberDraft::default(),
            drag: NumberDraft::default(),
            turbulence: NumberDraft::default(),
            cohesion: NumberDraft::default(),
            ground_y: NumberDraft::default(),
            wind: [NumberDraft::default(), NumberDraft::default()],
            ground_collision: values.ground_collision,
            accumulate: values.accumulate,
            splash: values.splash,
        };
        row.apply_values(values);
        row
    }

    /// Sincroniza los drafts con valores externos (preset o snapshot) sin borrar el
    /// texto que el usuario esté editando.
    pub(crate) fn apply_values(&mut self, values: &EmitterValues) {
        for (index, value) in values.position.iter().enumerate() {
            self.position[index].sync(*value);
        }
        self.rate.sync(values.rate);
        self.size.sync(values.size);
        self.life.sync(values.life);
        self.gravity.sync(values.gravity);
        self.drag.sync(values.drag);
        self.turbulence.sync(values.turbulence);
        self.cohesion.sync(values.cohesion);
        self.ground_y.sync(values.ground_y);
        self.wind[0].sync(values.wind[0]);
        self.wind[1].sync(values.wind[1]);
        self.ground_collision = values.ground_collision;
        self.accumulate = values.accumulate;
        self.splash = values.splash;
    }

    /// Fija los campos que el kernel ignora para este material al valor del preset.
    pub(crate) fn pin_inapplicable_fields(&mut self) {
        let preset = preset_values(self.material);
        let mask = field_mask(self.material);
        if !mask.gravity {
            self.gravity.force(preset.gravity);
        }
        if !mask.drag {
            self.drag.force(preset.drag);
        }
        if !mask.turbulence {
            self.turbulence.force(preset.turbulence);
        }
        if !mask.cohesion {
            self.cohesion.force(preset.cohesion);
        }
        if !mask.ground_y {
            self.ground_y.force(preset.ground_y);
        }
        if !mask.wind {
            self.wind[0].force(preset.wind[0]);
            self.wind[1].force(preset.wind[1]);
        }
        if !mask.ground_collision {
            self.ground_collision = preset.ground_collision;
        }
        if !mask.accumulate {
            self.accumulate = preset.accumulate;
        }
        if !mask.splash {
            self.splash = preset.splash;
        }
    }

    /// Valores actuales (lo escrito en los drafts, con respaldo si están vacíos).
    fn values(&self) -> EmitterValues {
        let parse = |draft: &NumberDraft, fallback: f32| draft.parsed().unwrap_or(fallback);
        EmitterValues {
            position: [
                parse(&self.position[0], 0.0),
                parse(&self.position[1], 0.0),
                parse(&self.position[2], 0.0),
            ],
            rate: parse(&self.rate, 100.0),
            size: parse(&self.size, 0.3),
            life: parse(&self.life, 3.0),
            gravity: parse(&self.gravity, -9.8),
            drag: parse(&self.drag, 0.5),
            turbulence: parse(&self.turbulence, 0.0),
            cohesion: parse(&self.cohesion, 0.0),
            ground_y: parse(&self.ground_y, 0.0),
            wind: [parse(&self.wind[0], 0.0), parse(&self.wind[1], 0.0)],
            ground_collision: self.ground_collision,
            accumulate: self.accumulate,
            splash: self.splash,
        }
    }
}

#[derive(Default)]
pub struct ParticlesUiState {
    /// Clima activo según la última selección (o snapshot): `None` = ninguno.
    weather: Option<&'static str>,
    /// Intensidad 1–10 (densidad: capacidad+tasa en lluvia/nieve, espesor en niebla).
    weather_intensity: f32,
    /// Multiplicador de velocidad de caída (lluvia/nieve).
    weather_fall: f32,
    /// Viento horizontal X (m/s): negativo = hacia la izquierda.
    weather_wind: f32,
    /// Emisores de escena creados desde la UI (o restaurados del snapshot).
    pub(crate) emitters: Vec<EmitterRow>,
    next_ui_id: u64,
    /// Modal de alta/edición abierta: un clon de la fila (o preset nuevo).
    pub(crate) modal: Option<EmitterRow>,
    /// `true` = alta nueva; `false` = edición del emisor con ese id.
    pub(crate) modal_is_new: bool,
}

/// Preset de valores por material para la alta nueva (coincide con los presets del
/// motor: `EmitterConfig::{fire_demo,smoke,fog,rain,snow,sand,water}`).
pub(crate) fn preset_values(material: &'static str) -> EmitterValues {
    let (position, rate, size, life, gravity, drag, turbulence, cohesion) = match material {
        "smoke" => ([0.0, 0.3, 0.0], 90.0, 0.9, 3.8, 1.35, 0.55, 1.2, 0.0),
        "fog" => ([0.0, 1.2, 0.0], 36.0, 2.2, 10.0, 0.0, 0.0, 0.0, 0.0),
        "sand" => ([0.0, 2.0, 0.0], 320.0, 0.05, 4.0, -9.8, 0.6, 0.3, 0.1),
        "water" => ([0.0, 2.0, 0.0], 380.0, 0.06, 3.5, -9.8, 0.35, 0.2, 0.5),
        // fuego
        _ => ([0.0, 0.35, 0.0], 160.0, 0.55, 1.1, -1.9, 0.9, 2.4, 0.0),
    };
    EmitterValues {
        position,
        rate,
        size,
        life,
        gravity,
        drag,
        turbulence,
        cohesion,
        ground_y: 0.0,
        wind: if material == "fog" {
            [0.2, 0.12]
        } else {
            [0.0, 0.0]
        },
        ground_collision: matches!(material, "sand" | "water"),
        accumulate: false,
        splash: material == "water",
    }
}

/// Qué campos de la modal el kernel usa de verdad para este material.
/// El resto se muestra bloqueado en el valor del preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmitterFieldMask {
    pub gravity: bool,
    pub drag: bool,
    pub turbulence: bool,
    pub cohesion: bool,
    pub ground_y: bool,
    pub wind: bool,
    pub ground_collision: bool,
    pub accumulate: bool,
    pub splash: bool,
}

pub(crate) fn field_mask(material: &str) -> EmitterFieldMask {
    match material {
        "fog" => EmitterFieldMask {
            gravity: false,
            drag: false,
            turbulence: false,
            cohesion: false,
            ground_y: true,
            wind: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
        },
        "sand" => EmitterFieldMask {
            gravity: true,
            drag: true,
            turbulence: false,
            cohesion: true,
            ground_y: true,
            wind: false,
            ground_collision: true,
            accumulate: false,
            splash: false,
        },
        "water" => EmitterFieldMask {
            gravity: true,
            drag: true,
            turbulence: false,
            cohesion: true,
            ground_y: true,
            wind: false,
            ground_collision: true,
            accumulate: false,
            splash: true,
        },
        // fuego y humo: flotabilidad, drag, curl y viento. Sin suelo ni cohesión.
        _ => EmitterFieldMask {
            gravity: true,
            drag: true,
            turbulence: true,
            cohesion: false,
            ground_y: false,
            wind: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
        },
    }
}

impl ParticlesUiState {
    /// Aplica `world.particles` del snapshot del motor (sincronización al cargar).
    pub(crate) fn apply_snapshot(&mut self, particles: &serde_json::Value) {
        self.emitters.clear();
        self.weather = None;
        self.weather_intensity = 1.0;
        self.weather_fall = 1.0;
        self.weather_wind = 0.0;
        let Some(map) = particles.as_object() else {
            return;
        };
        if let Some(weather) = map.get("weather") {
            let kind = weather.get("kind").and_then(serde_json::Value::as_str);
            let intensity = weather
                .get("intensity")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0) as f32;
            if let Some(option) = WEATHER_OPTIONS
                .iter()
                .find(|(wire, _, _)| Some(*wire) == kind)
            {
                self.weather = Some(option.0);
            }
            self.weather_intensity = intensity.clamp(1.0, 10.0);
            self.weather_fall = weather
                .get("fall_speed")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0) as f32;
            self.weather_wind = weather
                .get("wind_x")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
        }
        if let Some(list) = map.get("emitters").and_then(serde_json::Value::as_array) {
            for saved in list {
                let material = saved.get("material").and_then(serde_json::Value::as_str);
                let Some(material) = MATERIAL_OPTIONS
                    .iter()
                    .find(|(wire, _, _)| Some(*wire) == material)
                    .map(|option| option.0)
                else {
                    continue;
                };
                let get_f = |key: &str, fallback: f32| {
                    saved
                        .get(key)
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(fallback as f64) as f32
                };
                let get_bool = |key: &str| {
                    saved
                        .get(key)
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                };
                let position = saved
                    .get("position")
                    .and_then(serde_json::Value::as_array)
                    .map(|axis| {
                        [
                            axis.first()
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or(0.0) as f32,
                            axis.get(1)
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or(0.0) as f32,
                            axis.get(2)
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or(0.0) as f32,
                        ]
                    })
                    .unwrap_or([0.0; 3]);
                let wind = saved
                    .get("wind")
                    .and_then(serde_json::Value::as_array)
                    .map(|axis| {
                        [
                            axis.first()
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or(0.0) as f32,
                            axis.get(1)
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or(0.0) as f32,
                        ]
                    })
                    .unwrap_or([0.0, 0.0]);
                let mut row = EmitterRow::from_values(&EmitterValues {
                    position,
                    rate: get_f("rate", 100.0),
                    size: get_f("size", 0.3),
                    life: get_f("life", 3.0),
                    gravity: get_f("gravity", -9.8),
                    drag: get_f("drag", 0.5),
                    turbulence: get_f("turbulence", 0.0),
                    cohesion: get_f("cohesion", 0.0),
                    ground_y: get_f("ground_y", 0.0),
                    wind,
                    ground_collision: get_bool("ground_collision"),
                    accumulate: get_bool("accumulate"),
                    splash: get_bool("splash"),
                });
                row.id = saved
                    .get("id")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                row.material = material;
                row.pin_inapplicable_fields();
                self.emitters.push(row);
            }
        }
    }

    /// Valor del comando `add_particle_emitter` (upsert: alta o edición del mismo id).
    pub(crate) fn emitter_command(&self, row: &EmitterRow) -> serde_json::Value {
        let mut row = row.clone();
        row.pin_inapplicable_fields();
        let values = row.values();
        json!({
            "cmd": "add_particle_emitter",
            "id": row.id,
            "material": row.material,
            "position": values.position,
            "rate": values.rate,
            "size": values.size,
            "life": values.life,
            "gravity": values.gravity,
            "drag": values.drag,
            "turbulence": values.turbulence,
            "cohesion": values.cohesion,
            "ground_y": values.ground_y,
            "wind": values.wind,
            "ground_collision": values.ground_collision,
            "accumulate": values.accumulate,
            "splash": values.splash,
        })
    }
}

pub(super) fn particles_body(
    ui: &mut Ui,
    state: &mut ParticlesUiState,
    locale: Locale,
    actions: &mut Vec<WorldAction>,
) {
    widgets::caption(ui, tr(locale, "Clima", "Weather"));
    ui.add_space(2.0);

    // Dropdown del clima (el "dropdown" pedido del sidebar derecho).
    let current = state
        .weather
        .and_then(|wire| WEATHER_OPTIONS.iter().find(|option| option.0 == wire));
    let label = current
        .map(|option| tr(locale, option.1, option.2))
        .unwrap_or_else(|| tr(locale, "Ninguno", "None"));
    let mut next_weather = state.weather;
    egui::ComboBox::from_id_salt("particles_weather_combo")
        .width(ui.available_width())
        .selected_text(RichText::new(label).size(12.5).color(theme::TEXT))
        .show_ui(ui, |ui| {
            for (wire, es, en) in WEATHER_OPTIONS {
                ui.selectable_value(
                    &mut next_weather,
                    Some(wire),
                    RichText::new(tr(locale, es, en)).size(12.5),
                );
            }
        });
    if next_weather != state.weather {
        state.weather = next_weather;
        actions.push(WorldAction::SetParticleWeather {
            kind: state.weather,
            intensity: state.weather_intensity,
            fall: state.weather_fall,
            wind: state.weather_wind,
        });
    }

    // Intensidad 1–10: densidad de lluvia/nieve (capacidad+tasa) o espesor de niebla.
    widgets::label_value(
        ui,
        tr(locale, "Intensidad", "Intensity"),
        &format!("{:.0}", state.weather_intensity),
    );
    if widgets::range_slider(
        ui,
        &mut state.weather_intensity,
        1.0,
        10.0,
        1.0,
        theme::CONTROL_BLUE,
    ) {
        actions.push(WorldAction::SetParticleWeather {
            kind: state.weather,
            intensity: state.weather_intensity,
            fall: state.weather_fall,
            wind: state.weather_wind,
        });
    }

    // Dirección de caída + velocidad (lluvia/nieve): el viento empuja la caída hacia
    // un lado y el multiplicador acelera la velocidad terminal del preset.
    let falling = state
        .weather
        .is_some_and(|wire| *wire == *"rain" || *wire == *"snow");
    if falling {
        ui.add_space(2.0);
        widgets::label_value(
            ui,
            tr(locale, "Dirección de caída", "Fall direction"),
            match state.weather_wind {
                x if x < -0.05 => tr(locale, "Izquierda", "Left"),
                x if x > 0.05 => tr(locale, "Derecha", "Right"),
                _ => tr(locale, "Recta", "Straight"),
            },
        );
        let mut next_wind = state.weather_wind;
        egui::ComboBox::from_id_salt("particles_wind_combo")
            .width(ui.available_width())
            .selected_text(
                RichText::new(match state.weather_wind {
                    x if x < -0.05 => tr(locale, "Izquierda", "Left"),
                    x if x > 0.05 => tr(locale, "Derecha", "Right"),
                    _ => tr(locale, "Recta", "Straight"),
                })
                .size(12.5)
                .color(theme::TEXT),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut next_wind,
                    0.0,
                    RichText::new(tr(locale, "Recta", "Straight")).size(12.5),
                );
                ui.selectable_value(
                    &mut next_wind,
                    -4.0,
                    RichText::new(tr(locale, "Izquierda", "Left")).size(12.5),
                );
                ui.selectable_value(
                    &mut next_wind,
                    4.0,
                    RichText::new(tr(locale, "Derecha", "Right")).size(12.5),
                );
            });
        if next_wind != state.weather_wind {
            state.weather_wind = next_wind;
            actions.push(WorldAction::SetParticleWeather {
                kind: state.weather,
                intensity: state.weather_intensity,
                fall: state.weather_fall,
                wind: state.weather_wind,
            });
        }

        ui.add_space(2.0);
        widgets::label_value(
            ui,
            tr(locale, "Velocidad de caída", "Fall speed"),
            &format!("×{:.1}", state.weather_fall),
        );
        if widgets::range_slider(
            ui,
            &mut state.weather_fall,
            0.5,
            3.0,
            0.1,
            theme::CONTROL_BLUE,
        ) {
            actions.push(WorldAction::SetParticleWeather {
                kind: state.weather,
                intensity: state.weather_intensity,
                fall: state.weather_fall,
                wind: state.weather_wind,
            });
        }
    }

    ui.add_space(4.0);
    widgets::caption(ui, tr(locale, "Emisores de escena", "Scene emitters"));
    ui.add_space(2.0);

    if widgets::wide_button_click_sized(
        ui,
        ui.available_width(),
        icons::PLUS,
        tr(locale, "Añadir emisor", "Add emitter"),
        tr(
            locale,
            "Abrir el editor de emisores",
            "Open the emitter editor",
        ),
        theme::OUTLINE_SECONDARY,
    ) {
        let mut row = EmitterRow::from_values(&preset_values("fire"));
        row.material = "fire";
        row.pin_inapplicable_fields();
        state.modal = Some(row);
        state.modal_is_new = true;
    }

    ui.add_space(3.0);
    for row in &mut state.emitters {
        let material_label = MATERIAL_OPTIONS
            .iter()
            .find(|(wire, _, _)| *wire == row.material)
            .map(|option| tr(locale, option.1, option.2))
            .unwrap_or(row.material);
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .corner_radius(4)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name_width = (ui.available_width() - 46.0).max(0.0);
                    ui.add_sized(
                        [name_width, 18.0],
                        egui::Label::new(
                            RichText::new(format!("{material_label} · id {}", row.id % UI_ID_BASE))
                                .size(12.0)
                                .color(theme::TEXT),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if widgets::icon_action(
                            ui,
                            icons::TRASH,
                            18.0,
                            theme::DANGER,
                            tr(locale, "Eliminar", "Remove"),
                        ) {
                            actions.push(WorldAction::RemoveParticleEmitter(row.id));
                        }
                        if widgets::icon_action(
                            ui,
                            icons::PENCIL,
                            18.0,
                            theme::WARNING,
                            tr(locale, "Editar", "Edit"),
                        ) {
                            let mut draft = row.clone();
                            draft.pin_inapplicable_fields();
                            state.modal = Some(draft);
                            state.modal_is_new = false;
                        }
                    });
                });
            });
    }
    if state.emitters.is_empty() {
        widgets::tiny(
            ui,
            tr(locale, "Sin emisores de escena.", "No scene emitters."),
        );
    }
}

/// Asigna el siguiente id de UI al confirmar una alta nueva (la modal lo invoca).
pub(crate) fn next_ui_id(state: &mut ParticlesUiState) -> u64 {
    let id = UI_ID_BASE + state.next_ui_id;
    state.next_ui_id += 1;
    id
}

#[cfg(test)]
mod tests {
    use super::{EmitterRow, field_mask, preset_values};

    #[test]
    fn fire_locks_ground_flags_and_cohesion() {
        let mask = field_mask("fire");
        assert!(mask.gravity && mask.drag && mask.turbulence && mask.wind);
        assert!(!mask.cohesion && !mask.ground_y && !mask.ground_collision);
        assert!(!mask.accumulate && !mask.splash);
    }

    #[test]
    fn fog_locks_ballistics_and_keeps_wind() {
        let mask = field_mask("fog");
        assert!(mask.wind && mask.ground_y);
        assert!(!mask.gravity && !mask.drag && !mask.turbulence && !mask.cohesion);
        assert!(!mask.ground_collision && !mask.splash);
    }

    #[test]
    fn sand_locks_wind_and_splash() {
        let mask = field_mask("sand");
        assert!(mask.gravity && mask.drag && mask.cohesion && mask.ground_collision);
        assert!(!mask.turbulence && !mask.wind && !mask.splash && !mask.accumulate);
    }

    #[test]
    fn water_keeps_splash_and_locks_wind() {
        let mask = field_mask("water");
        assert!(mask.splash && mask.ground_collision && mask.cohesion);
        assert!(!mask.turbulence && !mask.wind && !mask.accumulate);
    }

    #[test]
    fn pin_resets_inapplicable_fire_fields_to_preset() {
        let mut row = EmitterRow::from_values(&preset_values("fire"));
        row.material = "fire";
        row.cohesion.force(0.9);
        row.ground_collision = true;
        row.splash = true;
        row.accumulate = true;
        row.pin_inapplicable_fields();
        assert_eq!(row.cohesion.parsed(), Some(0.0));
        assert!(!row.ground_collision);
        assert!(!row.splash);
        assert!(!row.accumulate);
        assert_eq!(row.gravity.parsed(), Some(-1.9));
    }

    #[test]
    fn pin_does_not_overwrite_editable_water_fields() {
        let mut row = EmitterRow::from_values(&preset_values("water"));
        row.material = "water";
        row.gravity.force(-4.0);
        row.splash = false;
        row.turbulence.force(9.0);
        row.pin_inapplicable_fields();
        assert_eq!(row.gravity.parsed(), Some(-4.0));
        assert!(!row.splash);
        assert_eq!(row.turbulence.parsed(), Some(0.2));
    }
}
