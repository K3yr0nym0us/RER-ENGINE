//! Ajustes de SSAA del `State` 3D. Espejo de `msaa_settings`: el tier llega por IPC, se recorta
//! al límite del adaptador y se publica de vuelta con `ssaa_tier_changed`. `FrameRenderer` lee la
//! escala efectiva desde `FrameInput` y redimensiona el G-buffer en el siguiente frame.

use super::ssaa_graphics::{self, SsaaTier, DEFAULT_SSAA_TIER};
use crate::engine3d::engine::State;
use crate::engine3d::ipc::{EngineEvent, send_event};

impl State {
    pub(crate) fn set_ssaa_tier(&mut self, tier: SsaaTier) {
        let prev = self.ssaa_tier;
        if prev == tier {
            log::info!("[ssaa] Nivel sin cambios: {}", tier.wire());
            return;
        }
        self.ssaa_tier = tier;
        let desired = tier.desired_scale();
        // El sample_count efectivo se aplica en FrameRenderer al redimensionar el G-buffer.
        let scale = ssaa_graphics::clamp_scale(&self.vulkan, desired, self.size.width, self.size.height);
        self.apply_ssaa_scale(scale);
        send_event(&EngineEvent::SsaaTierChanged {
            tier: tier.wire().to_string(),
            scale,
        });
        log::info!(
            "[ssaa] Nivel: {} -> {} ({:.2}×, {} muestras)",
            prev.wire(),
            tier.wire(),
            scale,
            tier.desired_samples()
        );
    }

    /// Registra la escala efectiva tras recortarla al límite de imagen del adaptador.
    pub(crate) fn apply_ssaa_scale(&mut self, scale: f32) {
        self.ssaa_scale = scale.max(1.0);
    }
}

pub(crate) fn apply_ssaa_settings_from_world_wire(state: &mut State, tier: Option<&str>) {
    let resolved = tier
        .and_then(SsaaTier::from_wire)
        .unwrap_or(DEFAULT_SSAA_TIER);
    if state.ssaa_tier != resolved {
        state.set_ssaa_tier(resolved);
    }
}
