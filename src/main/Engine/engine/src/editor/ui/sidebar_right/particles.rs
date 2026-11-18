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

/// Materiales para la alta manual de emisores. «Agua (bloques)» no es un
/// emisor GPU: es el autómata celular estilo Minecraft del módulo `water`.
pub(crate) const MATERIAL_OPTIONS: [(&str, &str, &str); 6] = [
    ("fire", "Fuego", "Fire"),
    ("smoke", "Humo", "Smoke"),
    ("fog", "Niebla", "Fog"),
    ("sand", "Arena", "Arena"),
    ("water", "Agua", "Water"),
    ("water_block", "Agua (bloques)", "Water blocks"),
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
    /// Solo agua de bloques: lado de la celda (m).
    pub cell_size: f32,
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
    pub(crate) cell_size: NumberDraft,
}

impl EmitterRow {
    /// Borrador nuevo con el preset del material (botón «Añadir emisor»).
    pub(crate) fn new_draft(material: &'static str) -> Self {
        let mut row = Self::from_values(&preset_values(material));
        row.material = material;
        row.pin_inapplicable_fields();
        row
    }

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
            cell_size: NumberDraft::default(),
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
        self.cell_size.sync(values.cell_size);
    }

    /// Aplica una posición elegida con el ghost del viewport a los drafts X/Y/Z.
    pub(crate) fn set_position(&mut self, position: [f32; 3]) {
        for (draft, value) in self.position.iter_mut().zip(position) {
            draft.force(value);
        }
    }

    /// Fija los campos que el kernel ignora para este material al valor del preset
    /// y deja el resto dentro de los límites seguros del slider.
    pub(crate) fn pin_inapplicable_fields(&mut self) {
        let preset = preset_values(self.material);
        let mask = field_mask(self.material);
        if !mask.rate {
            self.rate.force(preset.rate);
        }
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
        if !mask.cell_size {
            self.cell_size.force(preset.cell_size);
        }
        self.clamp_fields();
    }

    /// Reajusta los drafts a los rangos seguros del material. Cubre snapshots
    /// viejos o valores escritos antes de que existieran los sliders.
    pub(crate) fn clamp_fields(&mut self) {
        let ranges = field_ranges(self.material);
        let clamp = |draft: &mut NumberDraft, range: FieldRange| {
            let Some(value) = draft.parsed() else {
                return;
            };
            let clamped = range.clamp(value);
            if (clamped - value).abs() > 1e-6 {
                draft.force(clamped);
            }
        };
        clamp(&mut self.rate, ranges.rate);
        clamp(&mut self.size, ranges.size);
        clamp(&mut self.life, ranges.life);
        clamp(&mut self.gravity, ranges.gravity);
        clamp(&mut self.drag, ranges.drag);
        clamp(&mut self.turbulence, ranges.turbulence);
        clamp(&mut self.cohesion, ranges.cohesion);
        clamp(&mut self.ground_y, ranges.ground_y);
        clamp(&mut self.wind[0], ranges.wind);
        clamp(&mut self.wind[1], ranges.wind);
        clamp(&mut self.cell_size, ranges.cell_size);
    }

    /// Valores actuales (lo escrito en los drafts, con respaldo si están vacíos),
    /// ya acotados a los rangos seguros del material.
    fn values(&self) -> EmitterValues {
        let ranges = field_ranges(self.material);
        let parse = |draft: &NumberDraft, fallback: f32, range: FieldRange| {
            range.clamp(draft.parsed().unwrap_or(fallback))
        };
        EmitterValues {
            position: [
                parse(&self.position[0], 0.0, UNBOUNDED),
                parse(&self.position[1], 0.0, UNBOUNDED),
                parse(&self.position[2], 0.0, UNBOUNDED),
            ],
            rate: parse(&self.rate, 100.0, ranges.rate),
            size: parse(&self.size, 0.3, ranges.size),
            life: parse(&self.life, 3.0, ranges.life),
            gravity: parse(&self.gravity, -9.8, ranges.gravity),
            drag: parse(&self.drag, 0.5, ranges.drag),
            turbulence: parse(&self.turbulence, 0.0, ranges.turbulence),
            cohesion: parse(&self.cohesion, 0.0, ranges.cohesion),
            ground_y: parse(&self.ground_y, 0.0, ranges.ground_y),
            wind: [
                parse(&self.wind[0], 0.0, ranges.wind),
                parse(&self.wind[1], 0.0, ranges.wind),
            ],
            ground_collision: self.ground_collision,
            accumulate: self.accumulate,
            splash: self.splash,
            cell_size: parse(&self.cell_size, 0.5, ranges.cell_size),
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
    /// Borrador confirmado en la modal, esperando a elegir posición en el
    /// viewport con el ghost. `Some` mantiene el ghost activo.
    pub(crate) pending_placement: Option<EmitterRow>,
    /// `true` tras confirmar la modal de SOLO selección (alta nueva en 3D):
    /// el ghost coloca el emisor y el click abre la modal de propiedades
    /// (que al confirmar aplica al motor) en vez de crearlo directamente.
    pub(crate) awaiting_config: bool,
    /// `true` cuando la modal abierta es el paso de configuración posterior a
    /// la colocación: al confirmar se envía el comando al motor directamente
    /// (la posición ya la puso el ghost), sin reabrir el ghost.
    pub(crate) modal_configure: bool,
}

/// Preset de valores por material para la alta nueva. Los emisores coinciden
/// con los presets del motor; `water_block` solo usa posición y tamaño de celda.
pub(crate) fn preset_values(material: &'static str) -> EmitterValues {
    let (position, rate, size, life, gravity, drag, turbulence, cohesion) = match material {
        "smoke" => ([0.0, 0.3, 0.0], 90.0, 0.9, 3.8, 1.35, 0.55, 1.2, 0.0),
        "fog" => ([0.0, 1.2, 0.0], 36.0, 2.2, 10.0, 0.0, 0.0, 0.0, 0.0),
        "sand" => ([0.0, 2.0, 0.0], 320.0, 0.05, 4.0, -9.8, 0.6, 0.3, 0.1),
        "water" => ([0.0, 2.0, 0.0], 380.0, 0.06, 3.5, -9.8, 0.35, 0.2, 0.5),
        "water_block" => ([0.0, 0.0, 0.0], 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0),
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
        cell_size: 0.5,
    }
}

/// Qué campos de la modal usa de verdad este material. El resto no se
/// renderiza en la UI (queda fijado al preset del kernel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmitterFieldMask {
    pub rate: bool,
    pub gravity: bool,
    pub drag: bool,
    pub turbulence: bool,
    pub cohesion: bool,
    pub ground_y: bool,
    pub wind: bool,
    pub ground_collision: bool,
    pub accumulate: bool,
    pub splash: bool,
    pub cell_size: bool,
}

pub(crate) fn field_mask(material: &str) -> EmitterFieldMask {
    match material {
        "water_block" => EmitterFieldMask {
            rate: false,
            gravity: false,
            drag: false,
            turbulence: false,
            cohesion: false,
            ground_y: false,
            wind: false,
            ground_collision: false,
            accumulate: false,
            splash: false,
            cell_size: true,
        },
        "fog" => EmitterFieldMask {
            rate: true,
            gravity: false,
            drag: false,
            turbulence: false,
            cohesion: false,
            ground_y: true,
            wind: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
            cell_size: false,
        },
        "sand" => EmitterFieldMask {
            rate: true,
            gravity: true,
            drag: true,
            turbulence: false,
            cohesion: true,
            ground_y: true,
            wind: false,
            ground_collision: true,
            accumulate: false,
            splash: false,
            cell_size: false,
        },
        "water" => EmitterFieldMask {
            rate: true,
            gravity: true,
            drag: true,
            turbulence: false,
            cohesion: true,
            ground_y: true,
            wind: false,
            ground_collision: true,
            accumulate: false,
            splash: true,
            cell_size: false,
        },
        // fuego y humo: flotabilidad, drag, curl y viento. Sin suelo ni cohesión.
        _ => EmitterFieldMask {
            rate: true,
            gravity: true,
            drag: true,
            turbulence: true,
            cohesion: false,
            ground_y: false,
            wind: true,
            ground_collision: false,
            accumulate: false,
            splash: false,
            cell_size: false,
        },
    }
}

/// Límites de un campo numérico de la modal. Los sliders no pueden salir del
/// rango y los comandos se acotan al confirmar aunque el borrador venga de un
/// snapshot antiguo o manipulado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FieldRange {
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl FieldRange {
    pub(crate) fn clamp(self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }

    /// Decimales que muestra el slider para este paso (0.005 → 3).
    pub(crate) fn decimals(self) -> usize {
        if self.step >= 1.0 {
            0
        } else if self.step >= 0.1 {
            1
        } else if self.step >= 0.01 {
            2
        } else {
            3
        }
    }
}

/// Posición del emisor: sin límites (el ghost puede colocar en cualquier sitio).
const UNBOUNDED: FieldRange = FieldRange {
    min: f32::NEG_INFINITY,
    max: f32::INFINITY,
    step: 0.0,
};

/// Rangos seguros por material para los sliders de la modal. La cantidad no
/// sube la capacidad del pool (la fija el preset del motor), pero el tamaño sí
/// dispara el overdraw: por eso los topes de `size` son los más estrictos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EmitterRanges {
    pub rate: FieldRange,
    pub size: FieldRange,
    pub life: FieldRange,
    pub gravity: FieldRange,
    pub drag: FieldRange,
    pub turbulence: FieldRange,
    pub cohesion: FieldRange,
    pub ground_y: FieldRange,
    pub wind: FieldRange,
    pub cell_size: FieldRange,
}

pub(crate) fn field_ranges(material: &str) -> EmitterRanges {
    let (rate, size, life) = match material {
        "smoke" => (
            FieldRange {
                min: 20.0,
                max: 180.0,
                step: 5.0,
            },
            FieldRange {
                min: 0.3,
                max: 1.5,
                step: 0.05,
            },
            FieldRange {
                min: 0.5,
                max: 8.0,
                step: 0.1,
            },
        ),
        "fog" => (
            FieldRange {
                min: 10.0,
                max: 70.0,
                step: 2.0,
            },
            FieldRange {
                min: 0.5,
                max: 3.0,
                step: 0.1,
            },
            FieldRange {
                min: 2.0,
                max: 24.0,
                step: 0.5,
            },
        ),
        "sand" => (
            FieldRange {
                min: 40.0,
                max: 600.0,
                step: 10.0,
            },
            FieldRange {
                min: 0.01,
                max: 0.1,
                step: 0.005,
            },
            FieldRange {
                min: 0.5,
                max: 8.0,
                step: 0.1,
            },
        ),
        "water" => (
            FieldRange {
                min: 40.0,
                max: 600.0,
                step: 10.0,
            },
            FieldRange {
                min: 0.01,
                max: 0.1,
                step: 0.005,
            },
            FieldRange {
                min: 0.5,
                max: 6.0,
                step: 0.1,
            },
        ),
        // Agua de bloques: el autómata no tiene emisión ni tamaño de partícula;
        // sus campos se fijan al preset y solo se edita `cell_size`.
        "water_block" => (
            FieldRange {
                min: 0.0,
                max: 0.0,
                step: 0.0,
            },
            FieldRange {
                min: 0.5,
                max: 0.5,
                step: 0.0,
            },
            FieldRange {
                min: 0.0,
                max: 0.0,
                step: 0.0,
            },
        ),
        // fuego
        _ => (
            FieldRange {
                min: 40.0,
                max: 400.0,
                step: 5.0,
            },
            FieldRange {
                min: 0.15,
                max: 1.0,
                step: 0.05,
            },
            FieldRange {
                min: 0.3,
                max: 3.0,
                step: 0.05,
            },
        ),
    };
    EmitterRanges {
        rate,
        size,
        life,
        gravity: FieldRange {
            min: -20.0,
            max: 10.0,
            step: 0.1,
        },
        drag: FieldRange {
            min: 0.0,
            max: 3.0,
            step: 0.05,
        },
        turbulence: FieldRange {
            min: 0.0,
            max: 6.0,
            step: 0.1,
        },
        cohesion: FieldRange {
            min: 0.0,
            max: 1.0,
            step: 0.01,
        },
        ground_y: FieldRange {
            min: -10.0,
            max: 10.0,
            step: 0.1,
        },
        wind: FieldRange {
            min: -6.0,
            max: 6.0,
            step: 0.1,
        },
        cell_size: FieldRange {
            min: 0.25,
            max: 2.0,
            step: 0.05,
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
                    cell_size: get_f("cell_size", 0.5),
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

    /// Comando de alta para la fila: emisores GPU o fuente de agua de bloques
    /// (upsert por id en ambos casos).
    pub(crate) fn emitter_command(&self, row: &EmitterRow) -> serde_json::Value {
        let mut row = row.clone();
        row.pin_inapplicable_fields();
        let values = row.values();
        if row.material == "water_block" {
            return json!({
                "cmd": "add_water_source",
                "id": row.id,
                "position": values.position,
                "cell_size": values.cell_size,
            });
        }
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

    /// Quita la fila demo del sidebar al «Limpiar escena».
    pub(crate) fn remove_demo_showcase(&mut self) {
        let is_demo = |row: &EmitterRow| {
            let x = row.position[0].parsed().unwrap_or(0.0);
            let z = row.position[2].parsed().unwrap_or(0.0);
            crate::engine3d::particles::is_demo_showcase_wire(row.material, [x, 0.0, z], row.id)
        };
        if self.modal.as_ref().is_some_and(is_demo) {
            self.modal = None;
        }
        self.emitters.retain(|row| !is_demo(row));
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
        state.modal = Some(EmitterRow::new_draft("fire"));
        state.modal_is_new = true;
    }

    ui.add_space(3.0);
    // Nombre-número estable: cada material numera sus emisores por orden de la
    // lista («Fuego 1», «Agua 2»…). El id real es interno (botón de copiar).
    let mut numbers: std::collections::HashMap<&'static str, u32> = Default::default();
    for row in &mut state.emitters {
        let material_label = MATERIAL_OPTIONS
            .iter()
            .find(|(wire, _, _)| *wire == row.material)
            .map(|option| tr(locale, option.1, option.2))
            .unwrap_or(row.material);
        let number = numbers
            .entry(row.material)
            .and_modify(|n| *n += 1)
            .or_insert(1);
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .corner_radius(4)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name_width = (ui.available_width() - 64.0).max(0.0);
                    ui.add_sized(
                        [name_width, 18.0],
                        egui::Label::new(
                            RichText::new(format!("{material_label} {number}"))
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
                            if row.material == "water_block" {
                                actions.push(WorldAction::RemoveWaterSource(row.id));
                            } else {
                                actions.push(WorldAction::RemoveParticleEmitter(row.id));
                            }
                        }
                        if widgets::icon_action(
                            ui,
                            icons::COPY,
                            18.0,
                            theme::INFO,
                            tr(locale, "Copiar id", "Copy id"),
                        ) {
                            ui.ctx().copy_text(row.id.to_string());
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
    use super::{EmitterRow, FieldRange, field_mask, field_ranges, preset_values};

    fn assert_inside(label: &str, value: f32, range: FieldRange) {
        assert!(
            value >= range.min && value <= range.max,
            "{label}: {value} fuera de [{}, {}]",
            range.min,
            range.max
        );
    }

    #[test]
    fn ghost_position_overwrites_the_position_fields() {
        let values = preset_values("smoke");
        let mut row = EmitterRow::from_values(&values);
        row.material = "smoke";
        row.set_position([3.0, 0.5, -2.0]);
        assert_eq!(row.position[0].parsed(), Some(3.0));
        assert_eq!(row.position[1].parsed(), Some(0.5));
        assert_eq!(row.position[2].parsed(), Some(-2.0));
        // El resto de parámetros no se tocan.
        assert_eq!(row.material, "smoke");
    }

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
    fn water_block_replaces_the_water_cube_splasher() {
        let options = super::MATERIAL_OPTIONS;
        assert!(options.iter().any(|(wire, es, en)| *wire == "water_block"
            && *es == "Agua (bloques)"
            && *en == "Water blocks"));
        assert!(!options.iter().any(|(wire, _, _)| *wire == "water_cube"));
        let mask = field_mask("water_block");
        assert!(mask.cell_size);
        assert!(
            !mask.rate && !mask.gravity && !mask.splash && !mask.ground_collision && !mask.cohesion
        );
        let values = preset_values("water_block");
        assert_eq!(values.cell_size, 0.5);
        assert_eq!(values.rate, 0.0);
    }

    #[test]
    fn water_block_row_sends_the_water_source_command() {
        let state = super::ParticlesUiState::default();
        let row = EmitterRow::new_draft("water_block");
        let command = state.emitter_command(&row);
        assert_eq!(command["cmd"], "add_water_source");
        assert_eq!(command["cell_size"], 0.5);
        assert_eq!(command["position"], serde_json::json!([0.0, 0.0, 0.0]));
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

    #[test]
    fn safe_ranges_contain_every_preset() {
        for (material, _, _) in super::MATERIAL_OPTIONS {
            let values = preset_values(material);
            let ranges = field_ranges(material);
            assert_inside(material, values.rate, ranges.rate);
            assert_inside(material, values.size, ranges.size);
            assert_inside(material, values.life, ranges.life);
            assert_inside(material, values.gravity, ranges.gravity);
            assert_inside(material, values.drag, ranges.drag);
            assert_inside(material, values.turbulence, ranges.turbulence);
            assert_inside(material, values.cohesion, ranges.cohesion);
            assert_inside(material, values.ground_y, ranges.ground_y);
            assert_inside(material, values.wind[0], ranges.wind);
            assert_inside(material, values.wind[1], ranges.wind);
            assert_inside(material, values.cell_size, ranges.cell_size);
        }
    }

    #[test]
    fn command_clamps_out_of_range_drafts() {
        let state = super::ParticlesUiState::default();
        let mut row = EmitterRow::new_draft("smoke");
        row.size.force(50.0);
        row.rate.force(100_000.0);
        row.life.force(1_000.0);
        row.turbulence.force(-5.0);

        let ranges = field_ranges("smoke");
        let command = state.emitter_command(&row);
        assert_eq!(command["size"].as_f64(), Some(ranges.size.max as f64));
        assert_eq!(command["rate"].as_f64(), Some(ranges.rate.max as f64));
        assert_eq!(command["life"].as_f64(), Some(ranges.life.max as f64));
        assert_eq!(
            command["turbulence"].as_f64(),
            Some(ranges.turbulence.min as f64)
        );
    }

    #[test]
    fn water_block_command_clamps_cell_size() {
        let state = super::ParticlesUiState::default();
        let mut row = EmitterRow::new_draft("water_block");
        row.cell_size.force(999.0);
        let command = state.emitter_command(&row);
        assert_eq!(
            command["cell_size"].as_f64(),
            Some(field_ranges("water_block").cell_size.max as f64)
        );
    }
}
