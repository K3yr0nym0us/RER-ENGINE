//! Nivel de calidad de sombras (resolución del shadow map y amplitud del filtro PCF).
//!
//! El motor usa un único shadow map (sin cascadas). El tier fija su resolución real;
//! `vulkan::FrameRenderer` reasigna la imagen cuando cambia y `scene_light_params` ajusta el
//! texel/radio del PCF, así que low/medium/high/ultra se ven distinto (no solo cuestan distinto).
//! Las cascadas (CSM) y la distancia de sombra extendida de Ultra siguen pendientes
//! (`docs/Shadows_3D.yaml` → Sombras / Preset de calidad).

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
            Self::Low => crate::engine::SHADOW_MAP_SIZE,
            Self::Medium => 2048,
            Self::High => 4096,
            Self::Ultra => 8192,
        }
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
}
