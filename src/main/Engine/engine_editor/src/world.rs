//! Ajustes de mundo de la escena activa (barra lateral derecha).
//!
//! El motor es la fuente de verdad: al arrancar/cambiar de escena se pide un
//! `export_save_snapshot` y se leen sus `world`. Cada control envía el comando
//! `set_*` correspondiente y actualiza el estado de forma optimista.

use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureTier {
    Low,
    Medium,
    High,
    Ultra,
}

impl TextureTier {
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "low" => Self::Low,
            "high" => Self::High,
            "ultra" => Self::Ultra,
            _ => Self::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflectionTier {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl ReflectionTier {
    pub const ALL: [Self; 5] = [Self::Off, Self::Low, Self::Medium, Self::High, Self::Ultra];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "low" => Self::Low,
            "medium" | "medio" => Self::Medium,
            "high" => Self::High,
            "ultra" => Self::Ultra,
            _ => Self::Off,
        }
    }

    pub fn is_off(self) -> bool {
        self == Self::Off
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowTier {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl ShadowTier {
    pub const ALL: [Self; 5] = [Self::Off, Self::Low, Self::Medium, Self::High, Self::Ultra];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "off" | "apagado" => Self::Off,
            "medium" | "medio" => Self::Medium,
            "high" | "alto" => Self::High,
            "ultra" => Self::Ultra,
            _ => Self::Low,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsaaTier {
    Low,
    Medium,
    High,
    Ultra,
}

impl MsaaTier {
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "medium" | "medio" => Self::Medium,
            "high" | "alto" => Self::High,
            "ultra" => Self::Ultra,
            _ => Self::Low,
        }
    }
}

/// Vistas de depuración de reflejos (mismas claves que el motor).
pub const REFLECTION_DEBUG_VIEWS: [(&str, &str); 12] = [
    ("final", "Final"),
    ("ssr_debug", "SSR debug"),
    ("ssr_miss_green", "SSR miss (verde)"),
    ("ssr_exit_reason", "SSR motivo de salida"),
    ("ssr_vector_rgb", "SSR vector RGB"),
    ("ssr_hit_class", "SSR clase de impacto"),
    ("ssr_path_px", "SSR recorrido (px)"),
    ("ssr_march_refl_dir", "SSR dirección reflejada"),
    ("ssr_hit_uv", "SSR UV de impacto"),
    ("ssr_hit_sample_color", "SSR color muestreado"),
    ("ssr_proj_depth_delta", "SSR delta de profundidad"),
    ("ssr_ray_overlay", "SSR overlay de rayos"),
];

pub fn normalize_debug_view(value: &str) -> &'static str {
    let value = value.trim().to_lowercase();
    REFLECTION_DEBUG_VIEWS
        .iter()
        .map(|(key, _)| *key)
        .find(|key| *key == value)
        .unwrap_or("final")
}

/// Cámara del editor 2D.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera2d {
    pub x: f32,
    pub y: f32,
    pub half_h: f32,
}

/// Valores de la escena activa que edita la barra derecha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldSettings {
    pub world_width: f32,
    pub world_height: f32,
    pub world_depth: f32,
    pub world_radius: f32,
    pub grid_visible: bool,
    pub grid_cell_size: f32,
    pub gravity: f32,
    pub target_fps: u32,
    pub light_ambient: f32,
    pub light_intensity: f32,
    pub shadow_darkness: f32,
    pub graphics_texture_tier: TextureTier,
    pub texture_detail_distance: f32,
    pub reflection_tier: ReflectionTier,
    pub reflection_tier_effective: Option<ReflectionTier>,
    pub reflection_raytracing: bool,
    pub reflection_probes: bool,
    pub reflection_debug_view: &'static str,
    pub shadow_tier: ShadowTier,
    pub msaa_tier: MsaaTier,
    pub taa_enabled: bool,
    pub taa_blend: f32,
    pub taa_jitter_scale: f32,
    /// Cámara del editor 2D (`None` en 3D o si el motor aún no la reportó).
    pub camera2d: Option<Camera2d>,
}

impl Default for WorldSettings {
    fn default() -> Self {
        Self {
            world_width: 100.0,
            world_height: 50.0,
            world_depth: 100.0,
            world_radius: 50.0,
            grid_visible: true,
            grid_cell_size: 1.0,
            gravity: 15.0,
            target_fps: 60,
            light_ambient: 0.06,
            light_intensity: 1.0,
            shadow_darkness: 0.22,
            graphics_texture_tier: TextureTier::Medium,
            texture_detail_distance: 10.0,
            reflection_tier: ReflectionTier::Off,
            reflection_tier_effective: None,
            reflection_raytracing: false,
            reflection_probes: false,
            reflection_debug_view: "final",
            shadow_tier: ShadowTier::Low,
            msaa_tier: MsaaTier::Low,
            taa_enabled: true,
            taa_blend: 0.62,
            taa_jitter_scale: 1.0,
            camera2d: None,
        }
    }
}

impl WorldSettings {
    /// Lee `scene.world` del snapshot del motor.
    pub fn apply_snapshot(&mut self, scene: &Value) {
        let Some(world) = scene.get("world") else {
            return;
        };

        if let Some(value) = number(world, "world_width") {
            self.world_width = value as f32;
        }
        if let Some(value) = number(world, "world_height") {
            self.world_height = value as f32;
        }
        if let Some(value) = number(world, "world_depth") {
            self.world_depth = value as f32;
        }
        if let Some(value) = number(world, "world_radius") {
            self.world_radius = value as f32;
        }
        if let Some(value) = boolean(world, "grid_visible") {
            self.grid_visible = value;
        }
        if let Some(value) = number(world, "grid_cell_size") {
            self.grid_cell_size = value as f32;
        }
        if let Some(value) = number(world, "gravity") {
            self.gravity = value as f32;
        }
        if let Some(value) = world.get("target_fps").and_then(Value::as_u64) {
            self.target_fps = value as u32;
        }
        if let Some(value) = number(world, "light_ambient") {
            self.light_ambient = value as f32;
        }
        if let Some(value) = number(world, "light_intensity") {
            self.light_intensity = value as f32;
        }
        if let Some(value) = number(world, "shadow_darkness") {
            self.shadow_darkness = value as f32;
        }
        if let Some(value) = world.get("graphics_texture_tier").and_then(Value::as_str) {
            self.graphics_texture_tier = TextureTier::parse(value);
        }
        if let Some(value) = number(world, "texture_detail_distance_m") {
            self.texture_detail_distance = value as f32;
        }
        if let Some(value) = world.get("reflection_tier").and_then(Value::as_str) {
            self.reflection_tier = ReflectionTier::parse(value);
        }
        if let Some(value) = boolean(world, "reflection_raytracing") {
            self.reflection_raytracing = value;
        }
        if let Some(value) = boolean(world, "reflection_probes") {
            self.reflection_probes = value;
        }
        if let Some(value) = world.get("shadow_tier").and_then(Value::as_str) {
            self.shadow_tier = ShadowTier::parse(value);
        }
        if let Some(value) = world.get("msaa_tier").and_then(Value::as_str) {
            self.msaa_tier = MsaaTier::parse(value);
        }

        self.camera2d = scene.get("camera2d").and_then(|camera| {
            Some(Camera2d {
                x: number(camera, "x")? as f32,
                y: number(camera, "y")? as f32,
                half_h: number(camera, "half_h")? as f32,
            })
        });
    }

    /// Aplica un `engine_command` que React envió al motor, para mantener la UI
    /// de egui sincronizada cuando se edita desde la UI React.
    pub fn apply_command(&mut self, command: &Value) {
        let name = command.get("cmd").and_then(Value::as_str).unwrap_or("");
        match name {
            "set_world_size" => {
                if let Some(value) = number(command, "width") {
                    self.world_width = value as f32;
                }
                if let Some(value) = number(command, "height") {
                    self.world_height = value as f32;
                }
                if let Some(value) = number(command, "depth") {
                    self.world_depth = value as f32;
                }
            }
            "set_world_radius" => {
                if let Some(value) = number(command, "radius") {
                    let radius = (value as f32).clamp(5.0, 500.0);
                    self.world_radius = radius;
                    self.world_width = radius * 2.0;
                    self.world_height = radius * 2.0;
                    self.world_depth = radius * 2.0;
                }
            }
            "set_grid_visible" => {
                if let Some(value) = boolean(command, "visible") {
                    self.grid_visible = value;
                }
            }
            "set_grid_cell_size" => {
                if let Some(value) = number(command, "size") {
                    self.grid_cell_size = value as f32;
                }
            }
            "set_gravity" => {
                if let Some(value) = number(command, "gravity") {
                    self.gravity = value as f32;
                }
            }
            "set_target_fps" => {
                if let Some(value) = command.get("fps").and_then(Value::as_u64) {
                    self.target_fps = value as u32;
                }
            }
            "set_directional_light" => {
                if let Some(value) = number(command, "ambient") {
                    self.light_ambient = value as f32;
                }
                if let Some(value) = number(command, "intensity") {
                    self.light_intensity = value as f32;
                }
                if let Some(value) = number(command, "shadow_darkness") {
                    self.shadow_darkness = value as f32;
                }
            }
            "set_graphics_texture_tier" => {
                if let Some(value) = command.get("tier").and_then(Value::as_str) {
                    self.graphics_texture_tier = TextureTier::parse(value);
                }
            }
            "set_texture_detail_distance" => {
                if let Some(value) = number(command, "distance_m") {
                    self.texture_detail_distance = value as f32;
                }
            }
            "set_reflection_tier" => {
                if let Some(value) = command.get("tier").and_then(Value::as_str) {
                    self.reflection_tier = ReflectionTier::parse(value);
                }
            }
            "set_reflection_raytracing" => {
                if let Some(value) = boolean(command, "enabled") {
                    self.reflection_raytracing = value;
                }
            }
            "set_reflection_probes" => {
                if let Some(value) = boolean(command, "enabled") {
                    self.reflection_probes = value;
                }
            }
            "set_reflection_debug_view" => {
                if let Some(value) = command.get("view").and_then(Value::as_str) {
                    self.reflection_debug_view = normalize_debug_view(value);
                }
            }
            "set_shadow_tier" => {
                if let Some(value) = command.get("tier").and_then(Value::as_str) {
                    self.shadow_tier = ShadowTier::parse(value);
                }
            }
            "set_msaa_tier" => {
                if let Some(value) = command.get("tier").and_then(Value::as_str) {
                    self.msaa_tier = MsaaTier::parse(value);
                }
            }
            "set_taa" => {
                if let Some(value) = boolean(command, "enabled") {
                    self.taa_enabled = value;
                }
                if let Some(value) = number(command, "blend") {
                    self.taa_blend = value as f32;
                }
                if let Some(value) = number(command, "jitter_scale") {
                    self.taa_jitter_scale = value as f32;
                }
            }
            "set_camera_2d" => {
                if let (Some(x), Some(y), Some(half_h)) = (
                    number(command, "x"),
                    number(command, "y"),
                    number(command, "half_h"),
                ) {
                    self.camera2d = Some(Camera2d {
                        x: x as f32,
                        y: y as f32,
                        half_h: half_h as f32,
                    });
                }
            }
            _ => {}
        }
    }

    /// Aplica eventos de confirmación del motor (`*_changed`).
    pub fn apply_event(&mut self, event: &Value) {
        let name = event.get("event").and_then(Value::as_str).unwrap_or("");
        match name {
            "graphics_texture_tier_changed" => {
                if let Some(value) = event.get("tier").and_then(Value::as_str) {
                    self.graphics_texture_tier = TextureTier::parse(value);
                }
            }
            "texture_detail_distance_changed" => {
                if let Some(value) = number(event, "distance_m") {
                    self.texture_detail_distance = value as f32;
                }
            }
            "reflection_tier_changed" => {
                if let Some(value) = event.get("tier").and_then(Value::as_str) {
                    self.reflection_tier = ReflectionTier::parse(value);
                }
            }
            "reflection_tier_effective" => {
                if let Some(value) = event.get("effective").and_then(Value::as_str) {
                    self.reflection_tier_effective = Some(ReflectionTier::parse(value));
                }
            }
            "reflection_raytracing_changed" => {
                if let Some(value) = boolean(event, "enabled") {
                    self.reflection_raytracing = value;
                }
            }
            "reflection_probes_changed" => {
                if let Some(value) = boolean(event, "enabled") {
                    self.reflection_probes = value;
                }
            }
            "reflection_debug_view_changed" => {
                if let Some(value) = event.get("view").and_then(Value::as_str) {
                    self.reflection_debug_view = normalize_debug_view(value);
                }
            }
            "shadow_tier_changed" => {
                if let Some(value) = event.get("tier").and_then(Value::as_str) {
                    self.shadow_tier = ShadowTier::parse(value);
                }
            }
            "msaa_tier_changed" => {
                if let Some(value) = event.get("tier").and_then(Value::as_str) {
                    self.msaa_tier = MsaaTier::parse(value);
                }
            }
            "taa_changed" => {
                if let Some(value) = boolean(event, "enabled") {
                    self.taa_enabled = value;
                }
                if let Some(value) = number(event, "blend") {
                    self.taa_blend = value as f32;
                }
                if let Some(value) = number(event, "jitter_scale") {
                    self.taa_jitter_scale = value as f32;
                }
            }
            "camera_2d_updated" => {
                if let (Some(x), Some(y), Some(half_h)) = (
                    number(event, "x"),
                    number(event, "y"),
                    number(event, "half_h"),
                ) {
                    self.camera2d = Some(Camera2d {
                        x: x as f32,
                        y: y as f32,
                        half_h: half_h as f32,
                    });
                }
            }
            _ => {}
        }
    }

    /// Comando `set_directional_light` con solo los campos indicados.
    pub fn directional_light_command(
        ambient: Option<f32>,
        intensity: Option<f32>,
        shadow_darkness: Option<f32>,
    ) -> Value {
        let mut fields = Map::new();
        fields.insert("cmd".into(), Value::String("set_directional_light".into()));
        if let Some(value) = ambient {
            fields.insert("ambient".into(), Value::from(value as f64));
        }
        if let Some(value) = intensity {
            fields.insert("intensity".into(), Value::from(value as f64));
        }
        if let Some(value) = shadow_darkness {
            fields.insert("shadow_darkness".into(), Value::from(value as f64));
        }
        Value::Object(fields)
    }
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn boolean(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_world_from_3d_snapshot() {
        let mut world = WorldSettings::default();
        world.apply_snapshot(&json!({
            "world": {
                "world_width": 100.0, "world_height": 100.0, "world_depth": 100.0,
                "world_radius": 50.0, "grid_visible": false, "grid_cell_size": 2.5,
                "gravity": 9.81, "target_fps": 144,
                "light_ambient": 0.1, "light_intensity": 1.4, "shadow_darkness": 0.3,
                "graphics_texture_tier": "high", "texture_detail_distance_m": 42.0,
                "reflection_tier": "medium", "reflection_raytracing": true,
                "reflection_probes": true, "shadow_tier": "ultra", "msaa_tier": "high"
            }
        }));
        assert!(!world.grid_visible);
        assert_eq!(world.grid_cell_size, 2.5);
        assert_eq!(world.target_fps, 144);
        assert_eq!(world.graphics_texture_tier, TextureTier::High);
        assert_eq!(world.reflection_tier, ReflectionTier::Medium);
        assert!(world.reflection_raytracing);
        assert_eq!(world.shadow_tier, ShadowTier::Ultra);
        assert_eq!(world.msaa_tier, MsaaTier::High);
        assert_eq!(world.texture_detail_distance, 42.0);
    }

    #[test]
    fn applies_react_commands() {
        let mut world = WorldSettings::default();
        world.apply_command(&json!({ "cmd": "set_world_radius", "radius": 10.0 }));
        assert_eq!(world.world_radius, 10.0);
        assert_eq!(world.world_width, 20.0);

        world.apply_command(&json!({ "cmd": "set_directional_light", "ambient": 0.2 }));
        assert_eq!(world.light_ambient, 0.2);
        assert_eq!(world.light_intensity, 1.0);

        world.apply_command(&json!({ "cmd": "set_taa", "enabled": false, "blend": 0.4 }));
        assert!(!world.taa_enabled);
        assert_eq!(world.taa_blend, 0.4);
    }

    #[test]
    fn applies_engine_confirmations() {
        let mut world = WorldSettings::default();
        world.apply_event(&json!({ "event": "reflection_tier_effective", "effective": "medium" }));
        assert_eq!(
            world.reflection_tier_effective,
            Some(ReflectionTier::Medium)
        );
        world.apply_event(&json!({ "event": "msaa_tier_changed", "tier": "ultra" }));
        assert_eq!(world.msaa_tier, MsaaTier::Ultra);
        world
            .apply_event(&json!({ "event": "taa_changed", "enabled": false, "jitter_scale": 0.5 }));
        assert!(!world.taa_enabled);
        assert_eq!(world.taa_jitter_scale, 0.5);
    }

    #[test]
    fn directional_light_only_includes_given_fields() {
        let command = WorldSettings::directional_light_command(Some(0.1), None, Some(0.4));
        assert_eq!(command["cmd"], json!("set_directional_light"));
        assert!((command["ambient"].as_f64().unwrap() - 0.1).abs() < 1e-6);
        assert!(command.get("intensity").is_none());
        assert!((command["shadow_darkness"].as_f64().unwrap() - 0.4).abs() < 1e-6);
    }
}
