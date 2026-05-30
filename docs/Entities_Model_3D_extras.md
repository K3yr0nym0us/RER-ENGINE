# Configuración 3D fuera de `Entities_Model_3D.yaml`

Referencia principal: [Entities_Model_3D.yaml](Entities_Model_3D.yaml).

Estos campos siguen en el manifest / runtime pero **no** forman parte del contrato de entidades/blueprints de 41 líneas. Decidir si se documentan en el YAML o se eliminan.

| Campo / concepto | Ubicación | Nota |
|------------------|-----------|------|
| `config_camera` | `SavedScene`, raíz manifest | Cámara FPS separada del `player` |
| `config_editor_camera` | `SavedScene` | Viewport orbital del editor |
| `world` | escena | Tamaño, grilla, gravedad, luz ambiente/intensidad/sombras |
| `backgroundPath` | escena | Fondo del mundo |
| `camera2d` | escena | Solo proyectos 2D |
| `sprites`, `models`, `sounds`, `backgrounds` | proyecto | Precarga de assets |
| `scenes`, `activeSceneId`, `version`, `gameStyle`, `language` | manifest | Metadatos de proyecto |
| Marcadores runtime (`[Player]`, `[Ground]`, `[EditorBox]`, …) | motor interno | Colisión distinta por tipo: ver nota abajo |

### Colisión por tipo de entidad (motor)

| Entidad | `colision: true` en motor |
|---------|---------------------------|
| **Player** | Cápsula de movimiento (shape cast); **sin** cuerpo Rapier de malla en la entidad |
| **Ground** | Halfspace global (`add_static_ground`); **sin** cuerpo por entidad |
| **Sun** | Sin collider físico |
| **`[EditorBox]` / cubos FP** | Caja estática según **escala** del transform (no AABB de malla GLB) |
| **Modelo `.glb`/`.fbx`** | AABB de la malla cargada (+ `physics_type` si aplica) |
| `EntitySaveMeta.path` / `visual_model_path` | Rust | Puente interno hasta consolidar todo en `model` |
| `SavedPlayerTransform` | refs UI FP | Vista agregada runtime (`player` + `config_camera`) |
| Animaciones 2D (`frames`, `pivot`, `selection_mode`) | tipos compartidos | Solo `engine_2d` |
| Colliders / execution_area dibujados | `engine_2d` | Eliminados del contrato 3D |
| Quick Build estado UI | front | No va al `.save` |
| `mesh_collision_extents` | runtime jugador | Cápsula tras reemplazar mesh; no en YAML |
