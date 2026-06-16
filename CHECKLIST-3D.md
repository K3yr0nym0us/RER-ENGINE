# CHECKLIST — Motor 3D (`rer_engine_3d`)

Solo **tareas pendientes**. Lo implementado está en [docs/README.md](./docs/README.md), [docs/Entities_Model_3D.yaml](./docs/Entities_Model_3D.yaml), [docs/Project_Load_3D.yaml](./docs/Project_Load_3D.yaml) y docs relacionados.

Tareas globales: [CHECKLIST.md](./CHECKLIST.md). Contrato motor: [`engine_3d/ARCHITECTURE.md`](./src/main/Engine/engine_3d/ARCHITECTURE.md).

---

## Por implementar

### Funcionalidad

- [ ] **Física por hueso (bone physics)** — herramienta 3D en Tools: visualizar esqueleto, asignar física por hueso, persistencia en `.save`.

- [ ] **Root motion en animaciones 3D** — en Propiedades → Animaciones, marcar un clip embebido como *«Esta animación controla el movimiento»* (root motion): mientras se reproduce, el desplazamiento del hueso raíz del clip mueve la entidad en el mundo (además o en lugar del slide WASD de la cápsula). Útil para ataques con paso, empujones, trepar, cinemáticas de locomoción hechas en Blender. Hoy el jugador play character se mueve solo por shape cast + input; los clips son solo visuales. Persistencia en `.save`, motor autoritativo, opcional por animación.

### Assets y exportación (Prioridad baja final del proyecto)

- [ ] **Build final con texturas por plataforma** — aplazado hasta pipeline Editor → Save → Build → Ejecutable. Editor: RTEX RGBA8 ([Rerasset_Format.yaml](./docs/Rerasset_Format.yaml)). Export: BC7/BC5/BC4 (Win/Linux), ASTC (Android futuro). Ver [CHECKLIST.md](./CHECKLIST.md) y [CHECKLIST-TEXTURAS-3D.md](./CHECKLIST-TEXTURAS-3D.md).
