//! Nivel de calidad de sombras (Cascaded Shadow Maps): cascadas, resolución por cascada y
//! distancia de sombra.
//!
//! El motor usa un array de `MAX_SHADOW_CASCADES` capas de profundidad; el tier decide cuántas capas
//! se dibujan y con qué lado de mapa. `vulkan::FrameRenderer` reasigna el array cuando cambia el
//! tamaño y `scene_shadow_params` fija el radio PCF en unidades de mundo, así que
//! low/medium/high/ultra se ven distinto y cuestan distinto. Ultra añade la distancia de sombra
//! extendida (card `CASCADES / CSM (3D)` en Trello).

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

    /// Cascadas dibujadas por tier: Low 2, Medium 3, High 4, Ultra 4 (card `CASCADES / CSM (3D)`).
    /// Off no dibuja ninguna; el shader lee el número en `shadow_params.w` y no selecciona vacías.
    pub fn cascades(self) -> usize {
        match self {
            Self::Off => 0,
            Self::Low => 2,
            Self::Medium => 3,
            Self::High | Self::Ultra => 4,
        }
    }

    /// Resolución del shadow map (px por lado) **por cascada**. Off usa 1×1 (mínimo, no se
    /// renderiza ni se muestrea).
    ///
    /// Con CSM el lado deja de subir a 8192: el array ya cuesta 4× un mapa único, y el detalle cerca
    /// de la cámara lo da la primera cascada (que encuadra unos pocos metros), no un mapa gigante
    /// que desperdicia texeles en el primer plano.
    pub fn shadow_map_size(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::Low | Self::Medium => crate::engine::SHADOW_MAP_SIZE,
            Self::High | Self::Ultra => 2048,
        }
    }

    /// Multiplicador sobre el radio de escena que fija el borde lejano de la última cascada. Ultra
    /// es la "extended shadow distance" de la card; se expresa como fracción del tamaño del mundo
    /// para que las sombras no se corten antes si la escena crece.
    pub fn shadow_distance_scale(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.55,
            Self::Medium => 0.75,
            Self::High => 1.0,
            Self::Ultra => 1.45,
        }
    }

    /// Radio PCF pedido en unidades de mundo. El shader lo traduce a texeles de la cascada que
    /// toque (`shadow_params.x`), porque cada cascada cubre un área distinta con el mismo mapa.
    pub fn shadow_soft_radius(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.045,
            Self::Medium => 0.055,
            Self::High => 0.065,
            Self::Ultra => 0.085,
        }
    }

    /// Fracción del borde de cada cascada (menos la última) donde la sombra se desvanece, para que
    /// el salto de densidad de texel entre cascadas no se lea como una línea dura.
    pub const fn cascade_fade(self) -> f32 {
        match self {
            Self::Low | Self::Medium => 0.10,
            Self::High | Self::Ultra => 0.12,
            Self::Off => 0.0,
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

    #[test]
    fn cascades_and_resolution_never_shrink_as_the_tier_grows() {
        // Un tier que no cambiara nada visible fue justo el bug que había detrás de "las sombras no
        // se aplican": la tabla se protege sola a partir de ahora.
        let previous = ALL
            .windows(2)
            .map(|pair| {
                let (a, b) = (pair[0], pair[1]);
                (
                    a.cascades() <= b.cascades(),
                    a.shadow_map_size() <= b.shadow_map_size(),
                    a.shadow_distance_scale() <= b.shadow_distance_scale(),
                    a.shadow_soft_radius() <= b.shadow_soft_radius(),
                )
            })
            .collect::<Vec<_>>();
        for (index, checks) in previous.iter().enumerate() {
            assert!(checks.0, "cascadas retroceden en {index}");
            assert!(checks.1, "resolución retrocede en {index}");
            assert!(checks.2, "alcance retrocede en {index}");
            assert!(checks.3, "radio PCF retrocede en {index}");
        }
    }

    #[test]
    fn tier_table_matches_the_csm_card() {
        // Card `CASCADES / CSM (3D)`: Low 2, Medium 3, High 4, Ultra 4 + Ultra extended distance.
        assert_eq!(ShadowTier::Off.cascades(), 0);
        assert_eq!(ShadowTier::Low.cascades(), 2);
        assert_eq!(ShadowTier::Medium.cascades(), 3);
        assert_eq!(ShadowTier::High.cascades(), 4);
        assert_eq!(ShadowTier::Ultra.cascades(), 4);
        assert!(
            ShadowTier::Ultra.shadow_distance_scale() > ShadowTier::High.shadow_distance_scale(),
            "Ultra tiene que alcanzar más lejos que High"
        );
        assert_eq!(
            ShadowTier::Ultra
                .cascades()
                .min(crate::config_3d::shadow_cascades::MAX_SHADOW_CASCADES),
            crate::config_3d::shadow_cascades::MAX_SHADOW_CASCADES
        );
    }

    #[test]
    fn off_is_the_only_tier_without_cascades_and_default_is_not_off() {
        assert_eq!(DEFAULT_SHADOW_TIER, ShadowTier::Low);
        for tier in ALL {
            assert_eq!(
                tier.cascades() == 0,
                tier == ShadowTier::Off,
                "cascadas 0 sólo en Off"
            );
        }
    }
}
