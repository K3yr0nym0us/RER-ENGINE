//! Ajustes globales de reflejos (tier UI + debug views).
//!
//! El tier ya controla el render: `FrameRenderer` usa `reflections` para la cadena
//! SSR/denoise/composite y el trazado RT, y `reflection_debug_view` está cableado (presenta el
//! SSR/trazado en crudo). Lo que NO existe en Ash: presets de calidad por tier (pasos, distancia,
//! radio de denoise) y variación del tamaño de cubemap por tier; todo eso era de la era wgpu y
//! está listado como no portado en docs/Reflections_*.yaml.

use crate::config_3d::reflection_graphics::{
    DEFAULT_REFLECTION_TIER, ReflectionDebugView, ReflectionTier,
};
use crate::engine::State;
use crate::ipc::{EngineEvent, send_event};

impl State {
    /// Fuerza captura burst del cubemap si los reflejos están activos (p. ej. nuevas sondas).
    pub(crate) fn request_probe_capture_burst_if_reflections_active(&mut self) {
        if self.reflection_tier != ReflectionTier::Off && self.reflection_probes_enabled {
            self.probe_capture_burst_all = true;
        }
    }

    pub(crate) fn set_reflection_raytracing(&mut self, enabled: bool) {
        if self.reflection_raytracing_enabled == enabled {
            log::info!(
                "[reflexiones] RT sin cambios: {}",
                if enabled { "activado" } else { "desactivado" }
            );
            return;
        }
        self.reflection_raytracing_enabled = enabled;
        send_event(&EngineEvent::ReflectionRaytracingChanged { enabled });
        log::info!(
            "[reflexiones] RT {} (trazado por hardware en FrameRenderer)",
            if enabled { "activado" } else { "desactivado" }
        );
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
        if enabled && self.reflection_tier != ReflectionTier::Off {
            self.probe_capture_burst_all = true;
        } else if !enabled {
            // Sin probe_env wgpu: no hay write_probe_meta que llamar.
        }
        send_event(&EngineEvent::ReflectionProbesChanged { enabled });
        log::info!(
            "[reflexiones] Probes {} (captura burst + IBL en el shader forward)",
            if enabled { "activadas" } else { "desactivadas" }
        );
    }

    /// Quita malla/PBR legacy de probes guardadas como esferas reflectantes.
    pub(crate) fn sanitize_reflection_probe_entities(&mut self) {
        use crate::ecs::{MeshComponent, SurfacePbr, Transform};
        use crate::probe_registry::{self, DEFAULT_REFLECTION_PROBE_INFLUENCE_M};
        use glam::Vec3;
        for id in probe_registry::reflection_probe_entities(&self.save_registry) {
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
    ) -> Option<crate::ecs::EntityId> {
        use crate::probe_registry::{self, DEFAULT_REFLECTION_PROBE_INFLUENCE_M, MAX_PROBES};

        if probe_registry::reflection_probe_entities(&self.save_registry).len() >= MAX_PROBES {
            log::warn!("[reflexiones] spawn probe: límite de {MAX_PROBES} sondas alcanzado");
            crate::ipc::send_event(&crate::ipc::EngineEvent::Error {
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

    pub(crate) fn set_reflection_tier(&mut self, tier: ReflectionTier) {
        let prev = self.reflection_tier;
        if prev == tier {
            log::info!(
                "[reflexiones] Nivel de reflejos sin cambios: {}",
                tier.wire()
            );
            return;
        }
        self.reflection_tier = tier;
        self.probe_capture_burst_all =
            tier != ReflectionTier::Off && self.reflection_probes_enabled;
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
                "[reflexiones] diagnóstico: usa set_reflection_debug_view \"ssr_debug\" para ver la \
                 imagen SSR/trazada en crudo (\"final\" vuelve al composite)"
            );
        }
    }

    /// Cambia la vista de recepción de reflejos. `SsrDebug` presenta la imagen SSR/trazada en
    /// crudo; el renderer la elige en `resolve_present`, así que no hace falta recrear nada.
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
        if view != ReflectionDebugView::Final && self.reflection_tier == ReflectionTier::Off {
            log::warn!(
                "[reflexiones] la vista de debug no mostrará nada con el tier de reflejos en off"
            );
        }
    }
}

pub(crate) fn apply_reflection_settings_from_world_wire(
    state: &mut State,
    tier: Option<&str>,
    raytracing: Option<bool>,
    probes: Option<bool>,
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
        if resolved != ReflectionTier::Off && state.reflection_probes_enabled {
            state.probe_capture_burst_all = true;
        }
    }
    if let Some(rt) = raytracing {
        state.set_reflection_raytracing(rt);
    }
    if let Some(p) = probes {
        state.set_reflection_probes(p);
    }
}
