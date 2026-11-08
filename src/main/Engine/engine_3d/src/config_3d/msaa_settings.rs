//! Ajustes MSAA del pase forward 3D.
//!
//! Nota: la creación de pipelines wgpu (MRT/G-buffer forward) se eliminó al cortar el frame a
//! Ash (Fase 2). Mientras tanto, `set_msaa_tier` solo registra el tier y emite el evento IPC; el
//! `sample_count` efectivo lo gestiona `vulkan::FrameRenderer` (Fase 4).

use crate::config_3d::msaa_graphics::{DEFAULT_MSAA_TIER, MsaaTier};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

impl State {
    pub(crate) fn set_msaa_tier(&mut self, tier: MsaaTier) {
        let prev = self.msaa_tier;
        if prev == tier {
            log::info!("[msaa] Nivel sin cambios: {}", tier.wire());
            return;
        }
        self.msaa_tier = tier;
        // El sample_count efectivo se aplica en FrameRenderer al recrear pipelines.
        let sample_count = crate::config_3d::msaa_graphics::clamp_sample_count(
            &self.vulkan,
            tier.desired_sample_count(),
        );
        self.apply_msaa_sample_count(sample_count);
        self.recreate_forward_msaa_pipelines();
        send_event(&EngineEvent::MsaaTierChanged {
            tier: tier.wire().to_string(),
            sample_count,
        });
        log::info!(
            "[msaa] Nivel: {} -> {} ({}x)",
            prev.wire(),
            tier.wire(),
            sample_count
        );
    }

    /// Registra el `sample_count` efectivo tras el clamp al soporte del device.
    pub(crate) fn apply_msaa_sample_count(&mut self, sample_count: u32) {
        self.msaa_sample_count = sample_count.max(1);
    }

    /// Los pipelines forward viven en `vulkan::FrameRenderer` (MSAA 1x por ahora).
    pub(crate) fn recreate_forward_msaa_pipelines(&mut self) {
        log::info!("[msaa] pipelines forward gestionados por FrameRenderer");
    }
}

pub(crate) fn apply_msaa_settings_from_world_wire(state: &mut State, tier: Option<&str>) {
    let resolved = tier
        .and_then(MsaaTier::from_wire)
        .unwrap_or(DEFAULT_MSAA_TIER);
    if state.msaa_tier != resolved {
        state.set_msaa_tier(resolved);
    }
}
