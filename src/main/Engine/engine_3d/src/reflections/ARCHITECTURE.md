# Sistema de reflejos (engine_3d)

Documentación detallada en `docs/`:

| Sistema | YAML |
|---------|------|
| SSR | [Reflections_SSR.yaml](../../../docs/Reflections_SSR.yaml) |
| Probes | [Reflections_Probes.yaml](../../../docs/Reflections_Probes.yaml) |
| RT | [Reflections_RT.yaml](../../../docs/Reflections_RT.yaml) |
| Índice | [Reflections_3D.yaml](../../../docs/Reflections_3D.yaml) |

## Filosofía (julio 2026)

- **Tier** = calidad de reflejos (resolución, distancia, denoise, material RT).
- **RT toggle** (`raytracing_enabled`) = fuente adicional; no sustituye SSR.
- **RT OFF** → SSR + temporal + composite según tier.
- **RT ON** → SSR + RT híbrido (`refl_compose_rt_ssr`); espejos `roughness ≤ 0.04` passthrough SSR (sin sombrear RT).

Preset por tier: `quality_preset.rs` → `ReflectionQualityPreset`.

## RT + espejos (junio 2026)

Superficies con `roughness ≤ REFL_MIRROR_ROUGHNESS_MAX` (0.04) no ejecutan sombreado RT: el pass escribe la textura SSR tal cual. Motivo: en geometría curva (esferas MatComp) el RT resolvía hits off-screen como probe/cielo, mientras SSR ya encontraba entidades en pantalla; alfombras planas con el mismo material no mostraban el fallo. Rugosos (`STEEL`, `PLASTIC`) siguen usando RT + mezcla con SSR (mín. 55% SSR cuando hay hit on-screen).

## Capas del pipeline

| Capa | Módulo / shader | Qué pinta |
|------|-----------------|------------|
| **A — Forward IBL** | `forward_ibl.wgsl`, `shader.wgsl` | Cubemap si SSR no traza el píxel |
| **B — SSR / RT** | `ssr.wgsl`, `rt_*.wgsl` | Detalle on-screen; miss SSR → cubemap probe |
| **C — Composite** | `composite.wgsl` | Suma SSR/RT sobre lit-composite |
| **Captura** | `probes_pipeline/capture.rs`, `fs_overlay` | Alimenta cubemaps (no es recepción) |

## Política de probes (`policy.rs`)

- Entidades `[ReflectionProbe]`: ranura cubemap propia + centro en `Transform`.
- Sin probes: sonda de respaldo `FALLBACK_SCENE_PROBE_ID` (~1.2 m sobre suelo).
- Resto de superficies: nearest por `probe_meta.entries`.

## Orden del frame (`frame.rs`)

1. `prepare_probes` — slots, meta GPU, mapa id→ranura  
2. Main pass — geometría con `probe_index` en instancias  
3. Export albedo / G-buffer  
4. `encode_probe_captures` — 6 caras + mips  
5. `prepare_lit_scene` (`render.rs`)  
6. `ReflectionPass::run_screen` — SSR, temporal, RT, denoise  
7. `composite_into` + TAA escena (`render.rs`)

## Si cambias X, toca capa Y

| Cambio | Archivos típicos |
|--------|------------------|
| Preset calidad tier | `quality_preset.rs`, `settings.rs` |
| Ranuras / lista probes | `probes_pipeline/registry.rs`, `config_base.rs` |
| Captura cubemap | `probes_pipeline/capture.rs`, `probe_env.rs` |
| Look metálico forward | `forward_ibl.wgsl`, `shader.wgsl` |
| Resolución probe por tier | `settings.rs` → `ReflectionTier::cubemap_face_size` |
| SSR pasos / distancia | `settings.rs`, `ssr_pipeline/` |
| RT calidad / denoise | `quality_preset.rs`, `rt_reflections_v2.rs` |
| Mezcla final en pantalla | `composite.wgsl`, `refl_compose_rt_ssr` |
| Espejos + RT (passthrough SSR) | `reflection_math.wgsl`, `rt_ray_query.wgsl`, `rt_bvh.wgsl` |
| Política own-slot vs nearest | `reflection_math.wgsl` `refl_resolve_probe_layer` |

## Debug

- Vistas SSR: `ReflectionDebugView` en `settings.rs` (prefijo `ssr_`).
- Log RT: `[RT] TLAS HW: … (SSR+RT híbrido, res N%)`.

## Tests Rust

```bash
cargo test -p rer-engine-3d probes::registry
```
