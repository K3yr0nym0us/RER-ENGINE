//! Estado de la ventana nativa del generador de mundo.
//!
//! Sustituye al `WorldGenModalBody` del renderer. Toda la configuración vive en
//! Rust: límites de la UI, reparto de biomas, JSON camelCase de `WorldGenConfig`
//! y seguimiento de los eventos del motor (`world_gen_*`).

use serde_json::{Value, json};

use crate::editor::app::EditorApp;

/// Diámetro del disco en metros (la UI usa diámetro, el motor radio).
/// Espejo de `MIN_WORLD_FACE_3D * 2` / `MAX_WORLD_FACE_3D * 2` del motor y de
/// `MIN/MAX_WORLDGEN_RADIUS` del plugin (`worldgen/src/scale.rs`).
pub const SIZE_MIN: u32 = 64;
pub const SIZE_MAX: u32 = 2000;
pub const SIZE_DEFAULT: u32 = 256;
pub const MIN_BIOME_SHARE: u32 = 10;

/// Orden de prioridad de la UI (distinto del índice canónico del crate worldgen).
pub const BIOME_IDS: [&str; 5] = ["grassland", "forest", "snow", "desert", "volcanic"];

pub fn size_to_radius(size: u32) -> f64 {
    f64::from(size) * 0.5
}

pub fn height_for_radius(radius: f64) -> u32 {
    (radius * 0.55).clamp(48.0, 256.0).round() as u32
}

pub fn max_biome_slots(radius: f64) -> usize {
    if radius < 80.0 {
        2
    } else if radius < 160.0 {
        3
    } else if radius < 280.0 {
        4
    } else {
        5
    }
}

fn max_sites_in_disk(radius: f64, spacing: f64) -> u32 {
    const SITE_EDGE: f64 = 24.0;
    let usable = (radius - SITE_EDGE).max(0.0);
    let space = spacing.max(8.0);
    if usable < space * 0.35 {
        return u32::from(usable > 8.0);
    }
    let area = std::f64::consts::PI * usable * usable;
    let cell = std::f64::consts::PI * (space * 0.5).powi(2);
    ((area / cell) * 0.45).floor().max(0.0) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureCaps {
    pub mountains: u32,
    pub hills: u32,
    pub valleys: u32,
    pub plains: u32,
    pub cliffs: u32,
    pub lakes: u32,
    pub rivers: u32,
    pub caves: u32,
    pub waterfalls: u32,
}

pub fn feature_caps(radius: f64) -> FeatureCaps {
    let cap = |fraction: f64| max_sites_in_disk(radius, radius * fraction).max(1);
    FeatureCaps {
        mountains: cap(0.62),
        hills: cap(0.28),
        valleys: cap(0.36),
        plains: cap(0.32),
        cliffs: cap(0.30),
        lakes: cap(0.38),
        rivers: cap(0.55),
        caves: ((radius / 22.0).floor() as u32).clamp(1, 16),
        waterfalls: cap(0.70),
    }
}

fn feature_sizes(radius: f64) -> serde_json::Map<String, Value> {
    let round = |value: f64| serde_json::Value::from(value.round() as i64);
    let mut map = serde_json::Map::new();
    map.insert(
        "mountains".into(),
        json!({
            "minSize": round(radius * 0.10), "maxSize": round(radius * 0.22),
            "minSeparation": round(radius * 0.62),
            "maxHeight": round((radius * 0.22).min(64.0)), "sharpness": 1.2,
        }),
    );
    map.insert(
        "hills".into(),
        json!({
            "minSize": round(radius * 0.08), "maxSize": round(radius * 0.22),
            "minSeparation": round(radius * 0.28),
            "maxHeight": round((radius * 0.12).min(24.0)),
        }),
    );
    map.insert(
        "valleys".into(),
        json!({
            "minSize": round(radius * 0.12), "maxSize": round(radius * 0.3),
            "minSeparation": round(radius * 0.36),
            "maxDepth": round((radius * 0.14).min(30.0)),
        }),
    );
    map.insert(
        "plains".into(),
        json!({
            "minSize": round(radius * 0.16), "maxSize": round(radius * 0.4),
            "minSeparation": round(radius * 0.32), "maxRoughness": 1.5,
        }),
    );
    map.insert(
        "cliffs".into(),
        json!({
            "minSize": round(radius * 0.08), "maxSize": round(radius * 0.22),
            "minSeparation": round(radius * 0.3), "dropHeight": 18,
        }),
    );
    map.insert(
        "lakes".into(),
        json!({
            "minSize": round(radius * 0.07), "maxSize": round(radius * 0.2),
            "minSeparation": round(radius * 0.38), "maxDepth": 10,
        }),
    );
    map.insert(
        "rivers".into(),
        json!({
            "lengthMin": round((radius * 0.45 * 0.45).max(20.0)),
            "lengthMax": round((radius * 1.5).max(40.0)),
            "width": 4, "depth": 3,
        }),
    );
    map
}

fn distribute_integer(total: i64, weights: &[i64]) -> Vec<i64> {
    if weights.is_empty() {
        return Vec::new();
    }
    let sum: i64 = weights.iter().sum();
    let safe: Vec<i64> = if sum > 0 {
        weights.to_vec()
    } else {
        vec![1; weights.len()]
    };
    let safe_sum = if sum > 0 { sum } else { weights.len() as i64 };
    let exact: Vec<f64> = safe
        .iter()
        .map(|weight| total as f64 * *weight as f64 / safe_sum as f64)
        .collect();
    let mut result: Vec<i64> = exact.iter().map(|value| value.floor() as i64).collect();
    let mut left = total - result.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..exact.len()).collect();
    order.sort_by(|a, b| {
        let rest_a = exact[*a] - exact[*a].floor();
        let rest_b = exact[*b] - exact[*b].floor();
        rest_b
            .partial_cmp(&rest_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for index in order {
        if left <= 0 {
            break;
        }
        result[index] += 1;
        left -= 1;
    }
    result
}

fn distribute_with_minimum(total: i64, wanted: &[u32]) -> Vec<u32> {
    let extra = distribute_integer(
        total - MIN_BIOME_SHARE as i64 * wanted.len() as i64,
        &wanted
            .iter()
            .map(|value| (*value).saturating_sub(MIN_BIOME_SHARE) as i64)
            .collect::<Vec<_>>(),
    );
    extra
        .into_iter()
        .map(|value| (value + MIN_BIOME_SHARE as i64).max(0) as u32)
        .collect()
}

fn enabled_indices(enabled: &[bool; 5]) -> Vec<usize> {
    (0..5).filter(|index| enabled[*index]).collect()
}

fn shares_from(indices: &[usize], values: Vec<u32>) -> [u32; 5] {
    let mut shares = [0u32; 5];
    for (index, value) in indices.iter().zip(values) {
        shares[*index] = value;
    }
    shares
}

pub fn normalize_biome_shares(shares: &[u32; 5], enabled: &[bool; 5]) -> [u32; 5] {
    let indices = enabled_indices(enabled);
    shares_from(
        &indices,
        distribute_with_minimum(
            100,
            &indices
                .iter()
                .map(|index| shares[*index])
                .collect::<Vec<_>>(),
        ),
    )
}

pub fn biome_share_range(enabled_count: usize) -> (u32, u32) {
    if enabled_count <= 1 {
        (100, 100)
    } else {
        (
            MIN_BIOME_SHARE,
            100 - MIN_BIOME_SHARE * (enabled_count as u32 - 1),
        )
    }
}

pub fn rebalance_biome_shares(
    shares: &[u32; 5],
    enabled: &[bool; 5],
    changed: usize,
    value: u32,
) -> [u32; 5] {
    if !enabled[changed] {
        return normalize_biome_shares(shares, enabled);
    }
    let others: Vec<usize> = enabled_indices(enabled)
        .into_iter()
        .filter(|index| *index != changed)
        .collect();
    if others.is_empty() {
        return shares_from(&[changed], vec![100]);
    }
    let (min, max) = biome_share_range(others.len() + 1);
    let fixed = value.clamp(min, max);
    let rest = distribute_with_minimum(
        (100 - fixed) as i64,
        &others
            .iter()
            .map(|index| shares[*index])
            .collect::<Vec<_>>(),
    );
    let mut indices = vec![changed];
    indices.extend(others);
    let mut values = vec![fixed];
    values.extend(rest);
    shares_from(&indices, values)
}

pub fn toggle_biome_share(shares: &[u32; 5], enabled: &[bool; 5], toggled: usize) -> [u32; 5] {
    if !enabled[toggled] {
        let mut copy = *shares;
        copy[toggled] = 0;
        return normalize_biome_shares(&copy, enabled);
    }
    let count = enabled_indices(enabled).len();
    rebalance_biome_shares(
        shares,
        enabled,
        toggled,
        (100.0 / count as f64).round() as u32,
    )
}

pub fn clamp_biomes(biomes: &[bool; 5], radius: f64) -> [bool; 5] {
    let slots = max_biome_slots(radius);
    let mut next = *biomes;
    let mut kept = 0;
    for enabled in &mut next {
        if *enabled {
            if kept >= slots {
                *enabled = false;
            } else {
                kept += 1;
            }
        }
    }
    if !(0..5).any(|index| next[index]) {
        next[0] = true;
    }
    next
}

/// Panel principal: valores por defecto equivalentes a `DEFAULT_PANEL`.
#[derive(Debug, Clone)]
pub struct PanelState {
    pub seed: u32,
    pub size: u32,
    pub voxel_size: f32,
    pub mountains: u32,
    pub hills: u32,
    pub valleys: u32,
    pub plains: u32,
    pub cliffs: u32,
    pub lakes: u32,
    pub rivers: u32,
    pub caves: u32,
    pub waterfalls: u32,
    pub biomes: [bool; 5],
    pub biome_shares: [u32; 5],
    pub replace_world: bool,
}

fn random_seed() -> u32 {
    let mut bytes = [0u8; 4];
    if getrandom::fill(&mut bytes).is_ok() {
        u32::from_le_bytes(bytes)
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as u32)
            .unwrap_or(1)
    }
}

impl Default for PanelState {
    fn default() -> Self {
        let biomes = clamp_biomes(&[true; 5], size_to_radius(SIZE_DEFAULT));
        let shares = normalize_biome_shares(&[0; 5], &biomes);
        Self {
            seed: random_seed(),
            size: SIZE_DEFAULT,
            voxel_size: 1.0,
            mountains: 4,
            hills: 2,
            valleys: 3,
            plains: 2,
            cliffs: 2,
            lakes: 2,
            rivers: 2,
            caves: 3,
            waterfalls: 2,
            biomes,
            biome_shares: shares,
            replace_world: false,
        }
    }
}

impl PanelState {
    pub fn clamp_to_size(mut self, size: u32) -> Self {
        self.size = size.clamp(SIZE_MIN, SIZE_MAX);
        let caps = feature_caps(size_to_radius(self.size));
        self.mountains = self.mountains.min(caps.mountains);
        self.hills = self.hills.min(caps.hills);
        self.valleys = self.valleys.min(caps.valleys);
        self.plains = self.plains.min(caps.plains);
        self.cliffs = self.cliffs.min(caps.cliffs);
        self.lakes = self.lakes.min(caps.lakes);
        self.rivers = self.rivers.min(caps.rivers);
        self.caves = self.caves.min(caps.caves);
        self.waterfalls = self.waterfalls.min(caps.waterfalls);
        self.biomes = clamp_biomes(&self.biomes, size_to_radius(self.size));
        self.biome_shares = normalize_biome_shares(&self.biome_shares, &self.biomes);
        self
    }

    /// JSON camelCase de `WorldGenConfig` esperado por el motor.
    pub fn config_json(&self) -> String {
        let radius = size_to_radius(self.size);
        let sizes = feature_sizes(radius);
        let feature = |count: u32, key: &str| {
            let mut object = json!({ "enabled": count > 0, "maxCount": count })
                .as_object()
                .unwrap()
                .clone();
            if let Some(Value::Object(extra)) = sizes.get(key) {
                for (name, value) in extra {
                    object.insert(name.clone(), value.clone());
                }
            }
            Value::Object(object)
        };
        let mut biome_object = serde_json::Map::new();
        for (index, id) in BIOME_IDS.iter().enumerate() {
            biome_object.insert((*id).to_string(), json!(self.biomes[index]));
        }
        let mut shares = serde_json::Map::new();
        for (index, id) in BIOME_IDS.iter().enumerate() {
            shares.insert((*id).to_string(), json!(self.biome_shares[index]));
        }
        biome_object.insert("shares".into(), Value::Object(shares));

        json!({
            "seed": self.seed,
            "bounds": { "width": self.size, "length": self.size, "height": height_for_radius(radius) },
            "resolution": { "voxelSize": self.voxel_size, "lodEnabled": true },
            "mountains": feature(self.mountains, "mountains"),
            "hills": feature(self.hills, "hills"),
            "valleys": feature(self.valleys, "valleys"),
            "plains": feature(self.plains, "plains"),
            "cliffs": feature(self.cliffs, "cliffs"),
            "lakes": feature(self.lakes, "lakes"),
            "rivers": feature(self.rivers, "rivers"),
            "caves": {
                "enabled": self.caves > 0, "maxCount": self.caves,
                "depthMin": 10, "depthMax": 50, "radius": 3,
                "roomChance": 0.2, "segments": 18,
            },
            "waterfalls": {
                "enabled": self.waterfalls > 0, "maxCount": self.waterfalls, "minDrop": 6,
            },
            "biomes": Value::Object(biome_object),
        })
        .to_string()
    }
}

/// Reglas de distribución de un modelo (scatter).
#[derive(Debug, Clone)]
pub struct ScatterRules {
    pub max_instances: u32,
    pub density: u32,
    pub min_distance: f32,
    pub altitude_min: f32,
    pub altitude_max: f32,
    pub max_slope_deg: f32,
    pub scale_min: u32,
    pub scale_max: u32,
    pub random_rotation: bool,
    pub cluster_enabled: bool,
    pub cluster_size: u32,
}

impl Default for ScatterRules {
    fn default() -> Self {
        Self {
            max_instances: 200,
            density: 50,
            min_distance: 4.0,
            altitude_min: 0.0,
            altitude_max: 200.0,
            max_slope_deg: 35.0,
            scale_min: 80,
            scale_max: 130,
            random_rotation: true,
            cluster_enabled: false,
            cluster_size: 12,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScatterEntry {
    pub id: String,
    pub model_path: String,
    pub rules: ScatterRules,
}

impl ScatterEntry {
    pub fn blueprint_id(&self) -> String {
        format!("worldgen:{}", self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationPhase {
    Scan,
    Mesh,
}

/// Progreso de generación mostrado en el botón principal.
#[derive(Debug, Clone, Copy)]
pub struct Generation {
    pub completed: u32,
    pub total: u32,
    pub phase: GenerationPhase,
}

/// Estado completo de la ventana nativa.
#[derive(Debug, Clone, Default)]
pub struct WorldGenState {
    pub open: bool,
    /// `true` mientras exista un mundo generado (habilita el botón Clear).
    pub world_generated: bool,
    pub panel: PanelState,
    pub scatter_entries: Vec<ScatterEntry>,
    pub open_scatter_id: Option<String>,
    pub selected_model_path: String,
    pub status: String,
    pub generation: Option<Generation>,
    pub distributing: bool,
    pub library_path: Option<String>,
    pub pending_scatter: u32,
    pub finish_handled: bool,
    pub commands: Vec<Value>,
}

impl WorldGenState {
    pub fn inputs_locked(&self) -> bool {
        self.generation.is_some() || self.distributing
    }

    /// Aplica los eventos `world_gen_*` del motor para actualizar la UI.
    pub fn apply_event(&mut self, name: &str, event: &Value) {
        match name {
            "world_gen_progress" => {
                let phase = if event.get("phase").and_then(Value::as_str) == Some("scan") {
                    GenerationPhase::Scan
                } else {
                    GenerationPhase::Mesh
                };
                let completed = event.get("completed").and_then(Value::as_u64).unwrap_or(0) as u32;
                let total = event.get("total").and_then(Value::as_u64).unwrap_or(0) as u32;
                self.generation = Some(Generation {
                    completed,
                    total,
                    phase,
                });
                if phase == GenerationPhase::Mesh && total > 0 && completed >= total {
                    self.generation = None;
                    self.world_generated = true;
                    if !self.finish_handled {
                        self.finish_handled = true;
                        self.start_distribution();
                    }
                }
            }
            "world_gen_error" => {
                self.finish_handled = true;
                self.pending_scatter = 0;
                self.generation = None;
                self.distributing = false;
                self.status = event
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("World generation failed")
                    .to_string();
            }
            "world_gen_completed" => {
                self.generation = None;
                self.world_generated = true;
                if !self.finish_handled {
                    self.finish_handled = true;
                    self.start_distribution();
                }
            }
            "world_gen_status" => {
                if event.get("state").and_then(Value::as_str) == Some("complete") {
                    self.generation = None;
                    self.world_generated = true;
                    if !self.finish_handled {
                        self.finish_handled = true;
                        self.start_distribution();
                    }
                } else if event.get("state").and_then(Value::as_str) == Some("library_error") {
                    self.finish_handled = true;
                    self.pending_scatter = 0;
                    self.generation = None;
                    self.distributing = false;
                    self.status = "World generation failed".to_string();
                }
            }
            "world_gen_scatter_done" => {
                self.pending_scatter = self.pending_scatter.saturating_sub(1);
                let count = event.get("count").and_then(Value::as_u64).unwrap_or(0);
                if self.pending_scatter == 0 {
                    self.distributing = false;
                    self.status = "World ready".to_string();
                } else {
                    self.status = format!("Distributing blueprints… ({count})");
                }
            }
            _ => {}
        }
    }

    /// Encola los comandos de scatter de cada modelo añadido.
    pub fn start_distribution(&mut self) {
        self.generation = None;
        if self.scatter_entries.is_empty() {
            self.distributing = false;
            self.status = "World ready".to_string();
            return;
        }
        self.pending_scatter = self.scatter_entries.len() as u32;
        self.distributing = true;
        self.status = "Distributing blueprints…".to_string();
        for entry in &self.scatter_entries {
            let rules = &entry.rules;
            self.commands.push(json!({
                "cmd": "register_blueprint",
                "blueprint": {
                    "blueprint_id": entry.blueprint_id(),
                    "model": entry.model_path,
                    "template_name": entry.model_path,
                    "category": "object",
                    "colision": false,
                    "physics_type": "dynamic",
                }
            }));
            self.commands.push(json!({
                "cmd": "world_gen_scatter",
                "rules_json": json!({
                    "blueprintId": entry.blueprint_id(),
                    "biomes": [],
                    "altitudeMin": rules.altitude_min,
                    "altitudeMax": rules.altitude_max,
                    "maxSlopeDeg": rules.max_slope_deg,
                    "maxInstances": rules.max_instances,
                    "density": rules.density as f32 / 100.0,
                    "scaleMin": rules.scale_min as f32 / 100.0,
                    "scaleMax": rules.scale_max as f32 / 100.0,
                    "randomRotation": rules.random_rotation,
                    "minDistance": rules.min_distance,
                    "cluster": { "enabled": rules.cluster_enabled, "size": rules.cluster_size, "radius": 0 },
                })
                .to_string(),
            }));
        }
    }
}

/// Envía todos los comandos pendientes del estado al motor.
pub fn flush_commands(app: &mut EditorApp) {
    let commands = std::mem::take(&mut app.worldgen.commands);
    for command in commands {
        app.send_engine(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_panel_keeps_three_biomes_with_shares_summing_100() {
        let panel = PanelState::default();
        assert_eq!(panel.biomes.iter().filter(|enabled| **enabled).count(), 3);
        assert_eq!(panel.biome_shares.iter().sum::<u32>(), 100);
        assert!(
            panel
                .biome_shares
                .iter()
                .zip(panel.biomes)
                .filter(|(_, enabled)| *enabled)
                .all(|(share, _)| *share >= MIN_BIOME_SHARE)
        );
    }

    #[test]
    fn biome_slots_grow_with_radius() {
        assert_eq!(max_biome_slots(64.0), 2);
        assert_eq!(max_biome_slots(128.0), 3);
        assert_eq!(max_biome_slots(1000.0), 5);
    }

    #[test]
    fn rebalance_keeps_total_at_100() {
        let panel = PanelState::default();
        let shares = rebalance_biome_shares(&panel.biome_shares, &panel.biomes, 1, 60);
        assert_eq!(shares.iter().sum::<u32>(), 100);
        assert_eq!(shares[1], 60);
    }

    #[test]
    fn config_json_has_expected_shape() {
        let config: Value = serde_json::from_str(&PanelState::default().config_json()).unwrap();
        assert_eq!(config["bounds"]["width"], json!(SIZE_DEFAULT));
        assert_eq!(config["resolution"]["lodEnabled"], json!(true));
        assert!(config["biomes"]["shares"].is_object());
        assert!(config["mountains"].is_object());
    }

    #[test]
    fn scatter_distribution_enqueues_register_and_scatter() {
        let mut state = WorldGenState::default();
        state.scatter_entries.push(ScatterEntry {
            id: "wg_1".into(),
            model_path: "models/hero.glb".into(),
            rules: ScatterRules::default(),
        });
        state.start_distribution();
        assert!(state.distributing);
        assert_eq!(state.commands.len(), 2);
        assert_eq!(state.commands[0]["cmd"], json!("register_blueprint"));
        assert_eq!(state.commands[1]["cmd"], json!("world_gen_scatter"));
    }
}
