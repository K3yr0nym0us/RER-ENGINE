//! Nivel de calidad de sombras (cascadas, resolución por cascada y filtro PCF).
//!
//! El motor usa un array de `MAX_SHADOW_CASCADES` capas de profundidad; el tier decide cuántas
//! se dibujan y el lado de cada mapa. `vulkan::FrameRenderer` reasigna el array cuando cambia el
//! tamaño y `cascade_params` fija el radio PCF en unidades de mundo, así que
//! low/medium/high/ultra se ven distinto (no solo cuestan distinto). Ultra extiende además el
//! alcance de sombras (`docs/yaml/rendering/Shadows_3D.yaml` → Sombras / Preset de calidad).

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ShadowTier {
    Off,
    #[default]
    Low,
    Medium,
    High,
    Ultra,
}

impl ShadowTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "off" | "apagado" => Some(Self::Off),
            "low" | "bajo" => Some(Self::Low),
            "medium" | "medio" => Some(Self::Medium),
            "high" | "alto" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// Cascadas dibujadas y muestreadas: Off 0, Low 2, Medium 3, High/Ultra 4.
    /// Con Off se calcula una cascada de relleno para que el array tenga matrices finitas,
    /// pero se reporta 0 y el forward no muestrea ninguna capa.
    pub const fn cascade_count(self) -> usize {
        match self {
            Self::Off => 0,
            Self::Low => 2,
            Self::Medium => 3,
            Self::High | Self::Ultra => 4,
        }
    }

    /// Multiplicador del radio de bounds que fija el borde lejano de la última cascada.
    /// Solo Ultra extiende el alcance (x1.45); el resto conserva x1.00.
    pub const fn shadow_distance_scale(self) -> f32 {
        match self {
            Self::Ultra => 1.45,
            Self::Off | Self::Low | Self::Medium | Self::High => 1.0,
        }
    }

    /// Resolución del shadow map **por cascada** (px por lado). Off usa 1×1 (mínimo,
    /// no se renderiza ni se muestrea). Con CSM el lado deja de subir a 8192: el array ya
    /// cuesta 4× un mapa único y el detalle cerca de la cámara lo da la primera cascada.
    pub fn shadow_map_size(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Low | Self::Medium => crate::engine3d::engine::SHADOW_MAP_SIZE,
            Self::High | Self::Ultra => 2048,
        }
    }

    /// Radio PCF pedido en unidades de mundo: el shader lo convierte a texeles de la cascada
    /// que toque, porque cada capa cubre un área distinta con el mismo lado de mapa.
    pub const fn cascade_soft_radius(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.045,
            Self::Medium => 0.055,
            Self::High => 0.065,
            Self::Ultra => 0.085,
        }
    }

    /// Fracción del borde de cada cascada (menos la última) donde la sombra se desvanece,
    /// para que el salto de densidad de texel entre capas no se lea como una línea dura.
    pub const fn cascade_fade(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low | Self::Medium => 0.10,
            Self::High | Self::Ultra => 0.12,
        }
    }

    /// Lado X/Z del volumen de sombras volumétricas (voxels). El eje Y es la mitad para
    /// mantener voxeles cúbicos con la caja de niebla `[26, 14, 26]`. Off desactiva el pase.
    pub fn volumetric_resolution(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Low => 32,
            Self::Medium => 48,
            Self::High => 64,
            Self::Ultra => 96,
        }
    }

    /// Número de muestras hacia el sol por voxel al acumular densidad.
    pub fn volumetric_steps(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Low => 8,
            Self::Medium => 12,
            Self::High => 16,
            Self::Ultra => 24,
        }
    }

    /// Mezcla con el historial temporal (0 = solo el frame actual). Un blend menor en los tiers
    /// altos compensa que el volumen tiene más detalle y delata antes el ghosting.
    pub fn volumetric_blend(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.25,
            Self::Medium => 0.20,
            Self::High => 0.15,
            Self::Ultra => 0.12,
        }
    }

    /// `Off` apaga el pase por completo; el resto comparte el interruptor del shadow map.
    pub fn volumetric_enabled(self) -> bool {
        self.volumetric_resolution() > 0
    }
}

pub const DEFAULT_SHADOW_TIER: ShadowTier = ShadowTier::Low;

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [ShadowTier; 5] = [
        ShadowTier::Off,
        ShadowTier::Low,
        ShadowTier::Medium,
        ShadowTier::High,
        ShadowTier::Ultra,
    ];

    #[test]
    fn wire_round_trips_every_tier() {
        for tier in ALL {
            assert_eq!(
                ShadowTier::from_wire(tier.wire()),
                Some(tier),
                "wire {}",
                tier.wire()
            );
        }
        assert_eq!(ShadowTier::from_wire("NOEXISTE"), None);
        // El editor también envía sinónimos en español.
        assert_eq!(ShadowTier::from_wire(" Apagado "), Some(ShadowTier::Off));
        assert_eq!(ShadowTier::from_wire("MEDIO"), Some(ShadowTier::Medium));
    }

    /// Un tier que no cambiara el recurso GPU fue justo el bug que había detrás de "las sombras no
    /// se aplican": la escalera de resoluciones se protege sola a partir de ahora.
    #[test]
    fn resolutions_never_shrink_as_the_tier_grows() {
        for pair in ALL.windows(2) {
            assert!(
                pair[0].shadow_map_size() <= pair[1].shadow_map_size(),
                "{} ({}) es más grande que {} ({})",
                pair[0].wire(),
                pair[0].shadow_map_size(),
                pair[1].wire(),
                pair[1].shadow_map_size(),
            );
        }
        assert_eq!(DEFAULT_SHADOW_TIER, ShadowTier::Low);
        assert_eq!(ShadowTier::Off.shadow_map_size(), 1);
        assert_eq!(ShadowTier::Low.shadow_map_size(), 1024);
        assert_eq!(ShadowTier::Medium.shadow_map_size(), 1024);
        assert_eq!(ShadowTier::High.shadow_map_size(), 2048);
        assert_eq!(ShadowTier::Ultra.shadow_map_size(), 2048);
    }

    #[test]
    fn cascades_grow_with_the_tier_and_never_exceed_four() {
        assert_eq!(ShadowTier::Off.cascade_count(), 0);
        assert_eq!(ShadowTier::Low.cascade_count(), 2);
        assert_eq!(ShadowTier::Medium.cascade_count(), 3);
        assert_eq!(ShadowTier::High.cascade_count(), 4);
        assert_eq!(ShadowTier::Ultra.cascade_count(), 4);
        for pair in ALL.windows(2) {
            assert!(pair[0].cascade_count() <= pair[1].cascade_count());
            assert!(pair[0].cascade_count() <= 4);
        }
    }

    /// 4 capas × 2048² texeles × 4 B = 64 MiB como máximo. El array entero se asigna siempre
    /// (incluso en Low), así que el presupuesto se mide por el peor tier, no por las capas usadas.
    #[test]
    fn shadow_array_stays_within_the_vram_budget() {
        let side = ShadowTier::Ultra.shadow_map_size();
        let bytes = 4_u64 * u64::from(side) * u64::from(side) * 4;
        assert_eq!(bytes, 64 * 1024 * 1024);
        for tier in ALL {
            let side = u64::from(tier.shadow_map_size());
            assert!(
                4 * side * side * 4 <= bytes,
                "{} excede el presupuesto",
                tier.wire()
            );
        }
    }

    #[test]
    fn ultra_is_the_only_tier_with_extended_shadow_distance() {
        for tier in [
            ShadowTier::Off,
            ShadowTier::Low,
            ShadowTier::Medium,
            ShadowTier::High,
        ] {
            assert_eq!(tier.shadow_distance_scale(), 1.0, "tier {}", tier.wire());
        }
        assert!((ShadowTier::Ultra.shadow_distance_scale() - 1.45).abs() < 1.0e-6);
        assert!(ShadowTier::Ultra.cascade_soft_radius() > ShadowTier::High.cascade_soft_radius());
        assert!(ShadowTier::Ultra.cascade_fade() >= ShadowTier::High.cascade_fade());
    }

    #[test]
    fn volumetric_quality_follows_the_same_tiers() {
        assert!(!ShadowTier::Off.volumetric_enabled());
        assert_eq!(ShadowTier::Off.volumetric_resolution(), 0);
        assert_eq!(ShadowTier::Off.volumetric_steps(), 0);
        let enabled: Vec<ShadowTier> = ALL
            .into_iter()
            .filter(|tier| tier.volumetric_enabled())
            .collect();
        for pair in enabled.windows(2) {
            assert!(
                pair[0].volumetric_resolution() <= pair[1].volumetric_resolution(),
                "resolución volumétrica decreciente"
            );
            assert!(
                pair[0].volumetric_steps() <= pair[1].volumetric_steps(),
                "pasos decrecientes"
            );
            assert!(pair[1].volumetric_blend() <= pair[0].volumetric_blend());
        }
        assert_eq!(ShadowTier::Low.volumetric_resolution(), 32);
        assert_eq!(ShadowTier::Ultra.volumetric_resolution(), 96);
    }
}
