# CHECKLIST — Motor 3D (`rer_engine_3d`)

Solo **tareas pendientes**. Lo implementado está en [docs/README.md](./docs/README.md), [docs/Entities_Model_3D.yaml](./docs/Entities_Model_3D.yaml), [docs/Project_Load_3D.yaml](./docs/Project_Load_3D.yaml) y docs relacionados.

Tareas globales: [CHECKLIST.md](./CHECKLIST.md). Contrato motor: [`engine_3d/ARCHITECTURE.md`](./src/main/Engine/engine_3d/ARCHITECTURE.md).

---

## Por implementar

### Funcionalidad

- [ ] **Root motion en animaciones 3D** — en Propiedades → Animaciones, marcar un clip embebido como *«Esta animación controla el movimiento»* (root motion): mientras se reproduce, el desplazamiento del hueso raíz del clip mueve la entidad en el mundo (además o en lugar del slide WASD de la cápsula). Útil para ataques con paso, empujones, trepar, cinemáticas de locomoción hechas en Blender. Hoy el jugador play character se mueve solo por shape cast + input; los clips son solo visuales. Persistencia en `.save`, motor autoritativo, opcional por animación.

- [x] **Reflejos con nivel (Off, Low, Medium, High, Ultra)** — Tier selector via IPC + Rhai/Nodos. Implementa SSR, temporal accumulation, RT HW (GGX importance sampling + Cook-Torrance BRDF/PDF + firefly clamp + spatiotemporal denoising + back-face culling + TLAS PreferUpdate), RT shadows automático en ≥High, y ShadowTier::Off. Docs en Reflections_3D.yaml v5, Shadows_3D.yaml v2.

### Infraestructura RT

- [ ] **TLAS PreferUpdate real (wgpu)** — Hoy usa `PreferBuild` + flag `ALLOW_UPDATE` porque wgpu no implementa `PreferUpdate` y logea cada frame. Cuando wgpu lo implemente, cambiar `update_mode` a `PreferUpdate` en `rt_accel.rs` y verificar que haga update incremental (solo cuando cambian transforms, no geometría).

### Gráficos / Preset de calidad

- [ ] **Cascades / CSM (Cascaded Shadow Maps)** — Hoy el motor usa un único shadow map (sin cascadas). Implementar CSM con número de cascadas por tier del preset de calidad: **Low 2, Medium 3, High 4, Ultra 4**. En **Ultra** además *Extended Shadow Distance* (distancia de sombras extendida). Sistema nuevo grande: split de cascadas por distancia, multiples matrices de luz, selección de cascada en el shader, atlas/array de shadow maps.

- [ ] **MSAA (anti-aliasing por multisampling)** — No existe hoy (todos los render targets son `sample_count: 1`). Añadir MSAA con nivel por tier del preset: **Low Off, Medium 2x, High 4x, Ultra 8x**. Cambio transversal: `sample_count` + texturas multisample + pase de *resolve* en todos los pipelines de la escena 3D (y compatibilidad con TAA/composite/reflejos).

