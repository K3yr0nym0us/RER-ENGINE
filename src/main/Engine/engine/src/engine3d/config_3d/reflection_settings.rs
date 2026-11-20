//! Ajustes globales de reflejos: modo (SSR / RT), tier de SSR y tier de RTX.
//!
//! SSR y RTX son mutuamente excluyentes. `reflection_raytracing_enabled` se mantiene como espejo
//! del modo para los caminos antiguos; la fuente de verdad es `reflection_mode`.
//! Las reflection probes conservan su comportamiento anterior (volumen/radio) y no se tocan.

use crate::engine3d::config_3d::reflection_graphics::{
    DEFAULT_REFLECTION_TIER, ReflectionDebugView, ReflectionMode, ReflectionTier, RtxPreset,
};
use crate::engine3d::engine::State;
use crate::engine3d::ipc::{EngineEvent, send_event};

impl State {
    /// Fuerza captura burst del cubemap si los reflejos están activos (p. ej. nuevas sondas).
    pub(crate) fn request_probe_capture_burst_if_reflections_active(&mut self) {
        if self.reflection_mode.is_active() && self.reflection_probes_enabled {
            self.probe_capture_burst_all = true;
        }
    }

    /// Parámetros reales del preset RTX seleccionado.
    pub(crate) fn rtx_preset(&self) -> RtxPreset {
        RtxPreset::for_tier(self.rt_tier)
    }

    /// Cambia el sistema de reflejos activo. SSR y RT nunca están activos a la vez.
    pub(crate) fn set_reflection_mode(&mut self, mode: ReflectionMode) {
        if self.reflection_mode == mode {
            return;
        }
        let prev = self.reflection_mode;
        self.reflection_mode = mode;
        self.reflection_raytracing_enabled = mode.uses_rt();
        if mode.is_active() && self.reflection_probes_enabled {
            self.probe_capture_burst_all = true;
        }
        send_event(&EngineEvent::ReflectionModeChanged {
            mode: mode.wire().to_string(),
        });
        log::info!(
            "[reflexiones] modo: {} -> {} (tier SSR {}, tier RTX {})",
            prev.wire(),
            mode.wire(),
            self.reflection_tier.wire(),
            self.rt_tier.wire()
        );
    }

    /// Compatibilidad con el comando/Rhai antiguo: activa RT o lo desactiva.
    pub(crate) fn set_reflection_raytracing(&mut self, enabled: bool) {
        if enabled {
            if self.reflection_mode != ReflectionMode::Rt {
                self.set_reflection_mode(ReflectionMode::Rt);
            }
        } else if self.reflection_mode == ReflectionMode::Rt {
            self.set_reflection_mode(ReflectionMode::Off);
        }
    }

    pub(crate) fn set_reflection_probes(&mut self, enabled: bool) {
        if self.reflection_probes_enabled == enabled {
            log::info!(
                "[reflexiones] Probes sin cambios: {}",
                if enabled { "activadas" } else { "desactivadas" }
            );
            return;
        }
        self.reflection_probes_enabled = enabled;
        if enabled && self.reflection_mode.is_active() {
            self.probe_capture_burst_all = true;
        }
        send_event(&EngineEvent::ReflectionProbesChanged { enabled });
        log::info!(
            "[reflexiones] Probes {} (comportamiento actual: captura burst + IBL por volumen)",
            if enabled { "activadas" } else { "desactivadas" }
        );
    }

    /// Quita malla/PBR legacy de probes guardadas como esferas reflectantes.
    pub(crate) fn sanitize_reflection_probe_entities(&mut self) {
        use crate::engine3d::ecs::{MeshComponent, SurfacePbr, Transform};
        use crate::engine3d::probe_registry::DEFAULT_REFLECTION_PROBE_INFLUENCE_M;
        use glam::Vec3;
        for id in crate::engine3d::probe_registry::reflection_probe_entities(&self.save_registry) {
            self.world.remove_component::<MeshComponent>(id);
            self.world.remove_component::<SurfacePbr>(id);
            if let Some(t) = self.world.get_mut::<Transform>(id) {
                let influence_m = t.scale.x.abs() * 0.5;
                if influence_m > 1.0 {
                    t.scale = Vec3::splat(DEFAULT_REFLECTION_PROBE_INFLUENCE_M * 2.0);
                }
            }
        }
    }

    /// Inserta una sonda `[ReflectionProbe]` en la escena (no altera el switch de uso).
    pub(crate) fn spawn_reflection_probe(
        &mut self,
        position: Option<[f32; 3]>,
    ) -> Option<crate::engine3d::ecs::EntityId> {
        use crate::engine3d::probe_registry::{DEFAULT_REFLECTION_PROBE_INFLUENCE_M, MAX_PROBES};

        if crate::engine3d::probe_registry::reflection_probe_entities(&self.save_registry).len()
            >= MAX_PROBES
        {
            log::warn!("[reflexiones] spawn probe: límite de {MAX_PROBES} sondas alcanzado");
            crate::engine3d::ipc::send_event(&crate::engine3d::ipc::EngineEvent::Error {
                message: format!("Límite de {MAX_PROBES} reflection probes en la escena."),
            });
            return None;
        }
        let pos = position.unwrap_or_else(|| self.default_reflection_probe_spawn_position());
        let id = self.spawn_reflection_probe_entity("", pos, DEFAULT_REFLECTION_PROBE_INFLUENCE_M);
        self.probe_capture_burst_all = true;
        self.request_probe_capture_burst_if_reflections_active();
        Some(id)
    }

    /// Establece el tier de SSR. Si el modo era SSR y el tier pasa a Off, el modo pasa a Off.
    pub(crate) fn set_reflection_tier(&mut self, tier: ReflectionTier) {
        let prev = self.reflection_tier;
        if prev != tier {
            self.reflection_tier = tier;
            if self.reflection_mode.is_active() && self.reflection_probes_enabled {
                self.probe_capture_burst_all = true;
            }
            self.script_engine
                .sync_reflection_tier_readback(tier.wire());
            send_event(&EngineEvent::ReflectionTierChanged {
                tier: tier.wire().to_string(),
            });
            log::info!("[reflexiones] tier SSR: {} -> {}", prev.wire(), tier.wire());
        }
        if tier.is_off() && self.reflection_mode == ReflectionMode::Ssr {
            self.set_reflection_mode(ReflectionMode::Off);
        }
    }

    /// Establece el tier de RTX (resolución, rayos, rebotes, denoise, roughness...).
    pub(crate) fn set_rt_tier(&mut self, tier: ReflectionTier) {
        let prev = self.rt_tier;
        if prev == tier {
            return;
        }
        self.rt_tier = tier;
        send_event(&EngineEvent::RtTierChanged {
            tier: tier.wire().to_string(),
        });
        log::info!(
            "[reflexiones] tier RTX: {} -> {} ({:?})",
            prev.wire(),
            tier.wire(),
            RtxPreset::for_tier(tier)
        );
        if tier.is_off() && self.reflection_mode == ReflectionMode::Rt {
            self.set_reflection_mode(ReflectionMode::Off);
        }
    }

    /// Cambia la vista de recepción de reflejos.
    pub(crate) fn set_reflection_debug_view(&mut self, view: ReflectionDebugView) {
        let prev = self.reflection_debug_view;
        if prev == view {
            log::info!("[reflexiones] Vista debug sin cambios: {}", view.wire());
            return;
        }
        self.reflection_debug_view = view;
        send_event(&EngineEvent::ReflectionDebugViewChanged {
            view: view.wire().to_string(),
        });
        log::info!(
            "[reflexiones] Vista debug: {} -> {}",
            prev.wire(),
            view.wire()
        );
        if view != ReflectionDebugView::Final && !self.reflection_mode.is_active() {
            log::warn!("[reflexiones] la vista de debug no mostrará nada con los reflejos en off");
        }
    }
}

/// Aplica la configuración de reflejos del snapshot del `.save`.
///
/// `mode`/`rt_tier` son `None` en saves antiguos: se infieren de `reflection_raytracing` y
/// `reflection_tier` para no perder el comportamiento previo.
pub(crate) fn apply_reflection_settings_from_world_wire(
    state: &mut State,
    tier: Option<&str>,
    raytracing: Option<bool>,
    probes: Option<bool>,
    mode: Option<&str>,
    rt_tier: Option<&str>,
) {
    let resolved = tier
        .and_then(ReflectionTier::from_wire)
        .unwrap_or(DEFAULT_REFLECTION_TIER);
    state.set_reflection_tier(resolved);

    if let Some(t) = rt_tier.and_then(ReflectionTier::from_wire) {
        state.set_rt_tier(t);
    } else if raytracing == Some(true) && state.rt_tier.is_off() {
        state.set_rt_tier(resolved);
    }

    let resolved_mode = mode.and_then(ReflectionMode::from_wire).unwrap_or_else(|| {
        if raytracing == Some(true) {
            ReflectionMode::Rt
        } else if !resolved.is_off() {
            ReflectionMode::Ssr
        } else {
            ReflectionMode::Off
        }
    });
    state.set_reflection_mode(resolved_mode);
    state
        .script_engine
        .sync_reflection_tier_readback(resolved.wire());

    if let Some(p) = probes {
        state.set_reflection_probes(p);
    }
}
