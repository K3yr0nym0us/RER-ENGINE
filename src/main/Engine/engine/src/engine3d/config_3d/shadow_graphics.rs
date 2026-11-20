//! Nivel de calidad de sombras (resolución del shadow map y amplitud del filtro PCF).
//!
//! El motor usa un único shadow map (sin cascadas). El tier fija su resolución real;
//! `vulkan::FrameRenderer` reasigna la imagen cuando cambia y `scene_light_params` ajusta el
//! texel/radio del PCF, así que low/medium/high/ultra se ven distinto (no solo cuestan distinto).
//! Las cascadas (CSM) y la distancia de sombra extendida de Ultra siguen pendientes
//! (`docs/yaml/rendering/Shadows_3D.yaml` → Sombras / Preset de calidad).

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

    /// Resolución del shadow map (px por lado) según el tier. Off usa 1×1 (mínimo,
    /// no se renderiza ni se muestrea). Low reutiliza el tamaño base; el resto ×2 hasta 8192.
    pub fn shadow_map_size(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Low => crate::engine3d::engine::SHADOW_MAP_SIZE,
            Self::Medium => 2048,
            Self::High => 4096,
            Self::Ultra => 8192,
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
        assert_eq!(ShadowTier::Ultra.shadow_map_size(), 8192);
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
