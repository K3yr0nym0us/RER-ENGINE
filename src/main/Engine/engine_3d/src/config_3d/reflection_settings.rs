//! Ajustes globales de reflejos (tier UI + debug views).

use crate::config_3d::reflection_graphics::{
    ReflectionDebugView, ReflectionTier, DEFAULT_REFLECTION_TIER,
};
use crate::engine::State;
use crate::ipc::{send_event, EngineEvent};

impl State {
    /// Fuerza captura burst del cubemap si los reflejos están activos (p. ej. nuevas sondas).
    pub(crate) fn request_probe_capture_burst_if_reflections_active(&mut self) {
        if self.reflection_tier != ReflectionTier::Off {
            self.probe_capture_burst_all = true;
        }
    }

    pub(crate) fn set_reflection_tier(&mut self, tier: ReflectionTier) {
        let prev = self.reflection_tier;
        if prev == tier {
            log::info!("[reflexiones] Nivel de reflejos sin cambios: {}", tier.wire());
            return;
        }
        self.reflection_tier = tier;
        self.probe_capture_burst_all = tier != ReflectionTier::Off;
        self.reflections.invalidate_temporal();
        let tier_settings = crate::config_3d::reflection_graphics::ReflectionSettings::from_tier(tier);
        self.reflections.set_screen_fraction(&self.device, tier_settings.screen_fraction);
        // Cubemap de probes por tier (Low 128 … Ultra 1024): recrear solo si cambia el tamaño.
        let new_cubemap_size = tier.cubemap_face_size();
        if new_cubemap_size != self.probe_cubemap_size {
            self.rebuild_probe_env(new_cubemap_size);
        }
        self.script_engine
            .sync_reflection_tier_readback(tier.wire());
        send_event(&EngineEvent::ReflectionTierChanged {
            tier: tier.wire().to_string(),
        });
        log::info!(
            "[reflexiones] Nivel de reflejos: {} -> {}",
            prev.wire(),
            tier.wire()
        );
        if tier != ReflectionTier::Off {
            log::info!(
                "[reflexiones] diagnóstico: usa debug roughness/metallic/trace_strength/ssr_hits o probe_layers"
            );
        }
    }

    /// Recrea el cubemap de probes al tamaño de cara `face_size` (px). Necesario al cambiar el
    /// tier de reflejos (Low 128 … Ultra 1024) o tras recrear el shadow map (su vista cambió).
    /// Reconstruye los pipelines/bind groups de captura; las capturas se rehacen en los
    /// siguientes frames (round-robin), sin estado válido que preservar.
    pub(crate) fn rebuild_probe_env(&mut self, face_size: u32) {
        let texture_bgl = crate::texture::TextureArray::bind_group_layout(&self.device);
        let sample_bgl =
            crate::reflections::probe_env::ProbeEnvPass::sample_bind_group_layout(&self.device);
        let shadow_view = self
            ._shadow_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let shader = crate::shader_loader::load_scene_wgsl(
            &self.device,
            "main-shader-probe-resize",
            include_str!("../shader.wgsl"),
        );
        let skinned_shader = crate::shader_loader::load_scene_wgsl(
            &self.device,
            "skinned-shader-probe-resize",
            include_str!("../shader_skinned.wgsl"),
        );
        let new_probe = {
            let joint_bgl = self
                .joint_bind_group_layout
                .as_ref()
                .expect("joint bind group layout disponible tras init");
            crate::reflections::probe_env::ProbeEnvPass::new(
                &self.device,
                crate::taa::MRT_LIT_FORMAT,
                crate::engine::DEPTH_FORMAT,
                std::mem::size_of::<crate::engine::SceneUniforms>() as u64,
                &self.scene_bind_group_layout,
                &texture_bgl,
                joint_bgl,
                &sample_bgl,
                &shadow_view,
                &shader,
                &skinned_shader,
                face_size,
            )
        };
        self.probe_env = new_probe;
        self.probe_cubemap_size = face_size.max(8);
        self.probe_capture_cursor = 0;
        self.probe_capture_burst_all = self.reflection_tier != ReflectionTier::Off;
        self.last_probe_capture_ids = None;
    }

    pub(crate) fn set_reflection_debug_view(&mut self, view: ReflectionDebugView) {
        let prev = self.reflection_debug_view;
        if prev == view {
            log::info!("[reflexiones] Vista debug sin cambios: {}", view.wire());
            return;
        }
        self.reflection_debug_view = view;
        self.ssr_debug_mode = view.enables_ssr_stats();
        if view.enables_ssr_stats() {
            self.reflections.arm_ssr_debug_log();
        }
        self.reflections.invalidate_temporal();
        send_event(&EngineEvent::ReflectionDebugViewChanged {
            view: view.wire().to_string(),
        });
        log::info!(
            "[reflexiones] Vista debug: {} -> {}",
            prev.wire(),
            view.wire()
        );
    }

}

pub(crate) fn apply_reflection_settings_from_world_wire(
    state: &mut State,
    tier: Option<&str>,
    _raytracing: Option<bool>,
) {
    let resolved = tier
        .and_then(ReflectionTier::from_wire)
        .unwrap_or(DEFAULT_REFLECTION_TIER);
    if state.reflection_tier != resolved {
        state.set_reflection_tier(resolved);
    } else {
        state
            .script_engine
            .sync_reflection_tier_readback(resolved.wire());
        if resolved != ReflectionTier::Off {
            state.probe_capture_burst_all = true;
            state.reflections.invalidate_temporal();
        }
    }
}
