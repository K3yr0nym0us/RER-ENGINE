//! Contrato del sistema de reflejos: capas y política de ranuras de cubemap.
//!
//! ## Capas (orden del frame)
//! - **A — Forward IBL** (`shader.wgsl` / `forward_ibl.wgsl`): cubemap o procedural en metales.
//! - **B — SSR/RT** (`ssr.wgsl`, `rt_*.wgsl`): detalle on-screen por píxel.
//! - **C — Composite** (`composite.wgsl`): `detail = max(reflection - scene, 0)`.
//! - **Captura** (`probe_env` + `fs_overlay`): alimenta capa A; no es recepción.
//!
//! Cambios en B no se ven si A ya pintó un cubemap similar y C solo suma el delta.

/// Ranura de cubemap en el `texture_cube_array` (0..MAX_PROBES-1).
pub type ProbeSlot = usize;

#[allow(dead_code)]
/// Cómo el shader elige la capa de cubemap al samplear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeLayerPolicy {
    /// Entidad `[ReflectionProbe]`: usa `probe_index` de instancia (su propia ranura).
    OwnSlotForProbes,
    /// Cualquier superficie: probe más cercano por `probe_meta.entries`.
    NearestByWorldPos,
}

impl Default for ProbeLayerPolicy {
    fn default() -> Self {
        Self::OwnSlotForProbes
    }
}

#[allow(dead_code)]
/// Política activa de recepción (Fase 4 la implementa en WGSL vía `refl_resolve_probe_layer`).
pub const ACTIVE_PROBE_LAYER_POLICY: ProbeLayerPolicy = ProbeLayerPolicy::OwnSlotForProbes;
