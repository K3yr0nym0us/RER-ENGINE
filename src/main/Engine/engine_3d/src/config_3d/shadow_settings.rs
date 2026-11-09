//! Ajustes de calidad de sombras (tier UI): el array de cascadas vive en
//! `vulkan::FrameRenderer`; aquí se recuerda el tier, se propaga su resolución **por cascada** y se
//! notifica al editor.

use crate::config_3d::shadow_graphics::{DEFAULT_SHADOW_TIER, ShadowTier};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

impl State {
    pub(crate) fn set_shadow_tier(&mut self, tier: ShadowTier) {
        let prev = self.shadow_tier;
        if prev == tier {
            log::info!("[sombras] Nivel de sombras sin cambios: {}", tier.wire());
            return;
        }
        self.shadow_tier = tier;
        let new_size = tier.shadow_map_size();
        self.shadow_map_size = new_size;
        send_event(&EngineEvent::ShadowTierChanged {
            tier: tier.wire().to_string(),
        });
        if tier == ShadowTier::Off {
            log::info!(
                "[sombras] Nivel de sombras: {} -> {} (sin cascadas)",
                prev.wire(),
                tier.wire()
            );
        } else {
            // Se reporta el lado POR CASCADA: el array tiene `MAX_SHADOW_CASCADES` capas, así que
            // "1024 px" ya no es un mapa único sino 4 capas de 1024 (se dibujan las del tier).
            log::info!(
                "[sombras] Nivel de sombras: {} -> {} ({} cascadas de {} px, alcance x{:.2})",
                prev.wire(),
                tier.wire(),
                tier.cascades(),
                self.shadow_map_size,
                tier.shadow_distance_scale()
            );
        }
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
