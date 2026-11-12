# rer-worldgen — Plugin de generación procedural de mundos 3D

Crate independiente del motor RER-ENGINE que genera **terreno natural con
superficies suaves** (Marching Cubes), por chunks, con cuotas configurables por
feature y distribución automática de blueprints.

## Arquitectura

```
WorldGenConfig (JSON camelCase, validado)
   → FeaturePlan   (planner: sitios con cuotas máximas, cuevas, ríos, lagos)
   → Terrain       (heightfield con plan: montañas/colinas/valles/llanuras/
                    acantilados/lagos)  + CarveIndex (cuevas/ríos/lagos/roca)
   → DensityGrid   (grid con padding por chunk, Y-layer pruning, LOD)
   → mesh_chunk    (Marching Cubes + normales por gradiente + dedup)
   → chunk_splat   (biomas → pesos RGBA de material + albedo de preview)
   → scatter       (reglas por blueprint → colocaciones deterministas)
```

Contrato central: **todo es función pura de `(seed, config)`**. Sin estado
global, sin relojes, sin entropía externa. Los chunks vecinos comparten puntos
de grid exactos ⇒ las costuras son imposibles por construcción.

## Fases implementadas

- **F1/F2**: config + seeds por dominio + port propio de FastNoiseLite (MIT) +
  Marching Cubes (tablas de Bourke, orientación por gradiente, normales
  centrales) + chunks con Y-layer pruning.
- **F3/F4**: planificador con cuotas duras (`maxCount`), separación mínima,
  filtros cruzados; ríos descendentes con cascadas detectadas; lagos con nivel
  de agua; cuevas por carving (túneles-gusano + rooms, gates de profundidad,
  bocas naturales); formaciones rocosas aditivas (overhangs).
- **F5**: mapa de biomas (ruido de baja frecuencia restringido a los activados,
  override de nieve por altitud) + splat RGBA por chunk (R=hierba, G=roca,
  B=arena, A=nieve) + albedo de preview para el TextureArray del engine.
- **F7**: scatter de blueprints con reglas (biomas, altitud, pendiente,
  `maxInstances`, densidad, escala, rotación, distancia mínima, clústeres).
  El engine coloca por `spawn_cached_model_part_at` con `blueprint_id`
  (recursos GPU compartidos; el plugin nunca carga modelos).
- **F9**: LOD por anillos de distancia (voxel ×2/×4) + presupuestos de
  rendimiento verificados por test.

## Fachada FFI (`plugin_ffi`, ABI v2)

La DLL `rer_worldgen.dll` (target `cdylib`) se instala desde el editor
(descarga de GitHub Releases vía la sección de Plugins). El engine **no
depende del crate**: consume la ABI con `libloading` y espejo del contrato en
`engine3d/config_3d/worldgen/abi.rs`.

Exports: `wg_abi_version`, `wg_world_create`, `wg_world_free`,
`wg_chunk_status`, `wg_mesh_chunk(lod)`, `wg_chunk_textures`,
`wg_scatter`, y los `wg_free_*` correspondientes.

Reglas del contrato:
1. Cada export captura panics con `catch_unwind` (nunca cruzan la frontera).
2. **Allocator único**: la DLL allocó → la DLL libera (`wg_free_*`).
3. Errores: códigos `Status` + `wg_last_error` (UTF-8).
4. Cualquier cambio de firma incrementa `ABI_VERSION` (el engine rechaza DLL
   incompatibles).

## Tests

- Determinismo (ruido, campo, plan, scatter, hash dorado de mallas).
- Topología de Marching Cubes: cierre de malla (cada arista usada 2×), seams
  bit-idénticos entre chunks vecinos, orientación de normales.
- Cuotas y separación del planner; cuevas (bocas naturales, sin perforaciones
  espurias, salas excavadas); reglas de scatter nunca violadas.
- Conformancia de la DLL real contra el engine (`engine3d/config_3d/worldgen`).
- Presupuestos de rendimiento (release: chunk 32³ < 60 ms, `world_create` < 150 ms).

## Publicación

```
powershell -File scripts\publish-worldgen.ps1 -Version "1.0.0"
```

Construye la DLL, empaqueta `rer_worldgen.dll` en el ZIP
`rer-worldgen-<versión>-win-x64.zip` y calcula el SHA-256 para el catálogo del
editor (`src/main/plugins/pluginCatalog.ts`).
