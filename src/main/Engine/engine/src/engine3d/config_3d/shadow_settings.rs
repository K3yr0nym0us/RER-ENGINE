//! Ajustes de calidad de sombras (tier UI): el shadow map vive ahora en
//! `vulkan::FrameRenderer`; aquí solo se recuerda el tier y se notifica.

use crate::engine3d::config_3d::shadow_graphics::{DEFAULT_SHADOW_TIER, ShadowTier};
use crate::engine3d::engine::State;
use crate::engine3d::ipc::{EngineEvent, send_event};

impl State {
    pub(crate) fn set_shadow_tier(&mut self, tier: ShadowTier) {
        let prev = self.shadow_tier;
        if prev == tier {
            log::info!(
                "[sombras] Nivel de sombras sin cambios: {} ({} cascadas, {} px/cascada, x{:.2})",
                tier.wire(),
                tier.cascade_count(),
                tier.shadow_map_size(),
                tier.shadow_distance_scale()
            );
            return;
        }
        self.shadow_tier = tier;
        let new_size = tier.shadow_map_size();
        self.shadow_map_size = new_size;
        send_event(&EngineEvent::ShadowTierChanged {
            tier: tier.wire().to_string(),
        });
        log::info!(
            "[sombras] Nivel de sombras: {} -> {} ({} cascadas, {} px/cascada, x{:.2})",
            prev.wire(),
            tier.wire(),
            tier.cascade_count(),
            self.shadow_map_size,
            tier.shadow_distance_scale()
        );
    }
}

// Superficie de configuración del subsistema. Igual que los ajustes volumétricos,
// el tier de sombras es el único interruptor expuesto en UI/IPC/.save por ahora.
#[allow(dead_code)]
impl State {
    /// Activa/desactiva el culling de casters dependiente de la cámara. Con `false`
    /// se recupera el comportamiento heredado: solo proyecta sombra lo que ve la cámara.
    pub(crate) fn set_shadow_caster_culling(&mut self, enabled: bool) {
        if self.shadow_caster_culling_enabled == enabled {
            return;
        }
        self.shadow_caster_culling_enabled = enabled;
        log::info!(
            "[sombras] Culling de casters por cámara: {}",
            if enabled { "activado" } else { "desactivado" }
        );
    }

    /// Fija un margen manual (metros) o vuelve al calculado desde bounds y dirección del sol.
    pub(crate) fn set_shadow_caster_margin(&mut self, margin_m: Option<f32>) {
        self.shadow_caster_margin_override_m = margin_m.map(|m| m.max(0.0));
        log::info!(
            "[sombras] Margen de casters: {}",
            match self.shadow_caster_margin_override_m {
                Some(m) => format!("{m:.1} m (manual)"),
                None => "automático".to_string(),
            }
        );
    }
}

pub(crate) fn apply_shadow_settings_from_world_wire(state: &mut State, tier: Option<&str>) {
    let resolved = tier
        .and_then(ShadowTier::from_wire)
        .unwrap_or(DEFAULT_SHADOW_TIER);
    if state.shadow_tier != resolved {
        state.set_shadow_tier(resolved);
    }
}
