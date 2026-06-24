# Sistema de reflejos (engine_3d)

## Capas del pipeline

| Capa | Módulo / shader | Qué pinta |
|------|-----------------|------------|
| **A — Forward IBL** | `forward_ibl.wgsl`, `shader.wgsl` | Cubemap o entorno procedural en metales |
| **B — SSR / RT** | `ssr.wgsl`, `rt_*.wgsl` | Detalle on-screen por píxel |
| **C — Composite** | `composite.wgsl` | `detail = max(reflection − scene, 0)` |
| **Captura** | `probes/capture.rs`, `fs_overlay` | Alimenta cubemaps (no es recepción) |

Cambios en B pueden no verse si A ya cubre el mismo aspecto y C solo suma el delta positivo.

## Política de probes (`policy.rs`)

- Entidad `[ReflectionProbe]` con `probe_index` en instancia (≥ 0): samplea **su propia ranura** (pendiente Fase 4; actualmente forward usa **nearest** como antes).
- Resto de superficies: **nearest** por `probe_meta.entries` (`refl_resolve_probe_layer` en `reflection_math.wgsl`).

Rust escribe `tex_layer_pad[2]` solo para entidades en `probe_index_map`.

## Orden del frame (`frame.rs`)

1. `prepare_probes` — slots, meta GPU, mapa id→ranura  
2. Main pass — geometría con `probe_index` en instancias  
3. Export albedo / G-buffer  
4. `encode_probe_captures` — 6 caras + mips  
5. `prepare_lit_scene` (`render.rs`)  
6. `ReflectionPass::run_screen` — SSR, temporal, RT  
7. `composite_into` + TAA escena (`render.rs`)

## Si cambias X, toca capa Y

| Cambio | Archivos típicos |
|--------|------------------|
| Ranuras / lista probes | `probes/registry.rs`, `config_base.rs` |
| Captura cubemap | `probes/capture.rs`, `probe_env.rs` |
| Look metálico forward | `forward_ibl.wgsl`, `shader.wgsl` |
| Resolución probe por tier | `settings.rs` → `ReflectionTier::cubemap_face_size` |
| SSR pasos / distancia | `settings.rs`, `ssr.wgsl` |
| RT calidad | `rt_reflections_v2.rs`, `settings.rs` |
| Mezcla final en pantalla | `composite.wgsl` |
| Política own-slot vs nearest | `reflection_math.wgsl` `refl_resolve_probe_layer` |

## Debug

- `ReflectionDebugView::ProbeLayers` (`probe_layers`): modo shader 28.

## Tests Rust

```bash
cargo test -p rer-engine-3d probes::registry
```
