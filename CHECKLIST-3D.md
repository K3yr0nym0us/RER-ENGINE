# CHECKLIST — Motor 3D (`rer_engine_3d`)

Solo **tareas pendientes**. Lo implementado está en [docs/README.md](./docs/README.md), [docs/Entities_Model_3D.yaml](./docs/Entities_Model_3D.yaml), [docs/Project_Load_3D.yaml](./docs/Project_Load_3D.yaml) y docs relacionados.

Tareas globales: [CHECKLIST.md](./CHECKLIST.md). Contrato motor: [`engine_3d/ARCHITECTURE.md`](./src/main/Engine/engine_3d/ARCHITECTURE.md).

---

## Por implementar

### Funcionalidad

- [ ] **Física por hueso (bone physics)** — herramienta 3D en Tools: visualizar esqueleto, asignar física por hueso, persistencia en `.save`.

- [ ] **Root motion en animaciones 3D** — en Propiedades → Animaciones, marcar un clip embebido como *«Esta animación controla el movimiento»* (root motion): mientras se reproduce, el desplazamiento del hueso raíz del clip mueve la entidad en el mundo (además o en lugar del slide WASD de la cápsula). Útil para ataques con paso, empujones, trepar, cinemáticas de locomoción hechas en Blender. Hoy el jugador play character se mueve solo por shape cast + input; los clips son solo visuales. Persistencia en `.save`, motor autoritativo, opcional por animación.

- [ ] **Reflejos con nivel (Desactivado, Bajo, Medio, Alto)** - En el accordion World poner en algun lado un selector de nivel de reflejos que sean compatibles con tecnologia RTX.
Hay que crear toda la logica detras de esto y debe poder tambien modificarse por Rhai o Nodos.

