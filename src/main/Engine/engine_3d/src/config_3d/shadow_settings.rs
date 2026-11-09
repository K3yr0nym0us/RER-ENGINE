//! Ajustes de calidad de sombras (tier UI): el shadow map vive ahora en
//! `vulkan::FrameRenderer`; aquí solo se recuerda el tier y se notifica.

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
        log::info!(
            "[sombras] Nivel de sombras: {} -> {} ({} px)",
            prev.wire(),
            tier.wire(),
            self.shadow_map_size
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
