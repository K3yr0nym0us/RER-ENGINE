# Scripting Rhai — RER-ENGINE

Guía breve para escribir scripts en entidades. El motor **2D** y el **3D** comparten la misma forma de script; algunas funciones solo existen en uno de los dos.

**Render del motor:** Rust + wgpu — `rer_engine_2d` y `rer_engine_3d` usan **Vulkan** (Windows y Linux; sin variables de entorno). Ventana overlay junto al editor Electron. Los scripts Rhai no configuran la GPU; solo llaman a la API `engine.*` de gameplay.

---

## Forma de un script

Define funciones de callback de nivel superior. `dt` es el delta en segundos.

El motor compila cada script con un preámbulo `let engine = #{ ... }` y **invoca** los callbacks explícitamente (`on_start!(entity);`, `update!(entity, dt);`, etc.) para que `engine` sea visible dentro del cuerpo de la función.

```rhai
fn on_start(entity) {
    engine.log("Hola desde entidad " + entity.id);
}

fn update(entity, dt) {
    // cada frame
}

fn on_stop(entity) {
}
```

**Otros callbacks** (según tipo de script):

| Callback | Cuándo |
|----------|--------|
| `on_press(entity, control_key)` | Tecla/control asignado — **una vez por pulsación** (sin autorepeat; ver [Gravedad y controles 2D](#gravedad-y-controles-2d)) |
| `on_keep(entity, control_key)` | Tecla/control mantenido — cada frame mientras se mantiene pulsado |
| `on_trigger_enter(trigger, actor)` | Un actor entra en un *execution area* (solo 2D) |

**Scripts de escena** (Level Blueprint / nodos visuales):

| Callback | Cuándo |
|----------|--------|
| `on_scene_start()` | Al iniciar play en la escena |
| `on_scene_tick(dt)` | Cada frame mientras la escena está en play |

---

## Datos de la entidad (`entity`)

En cada frame el motor inyecta un mapa con los datos de la entidad.

| Campo | Descripción |
|-------|-------------|
| `id` | ID numérico |
| `x`, `y` | Posición en el plano de juego (2D: mundo XY; 3D: uso principal en XZ del suelo) |
| `scale_x`, `scale_y` | Escala |
| `animations` | Lista de nombres de animación (2D; en 3D puede estar vacía) |

---

## API común (`engine.*`)

Disponible en **2D y 3D** salvo donde se indique lo contrario.

### Movimiento

| Función | Qué hace |
|---------|----------|
| `engine.move_to(id, x, y)` | Teletransporta a una posición absoluta (sin física). |
| `engine.translate(id, dx, dy)` | Mueve un delta sin física. |
| `engine.move_entity(id, speed, dir_x, dir_y)` | Movimiento con Rapier (respeta colisiones si hay cuerpo físico). |
| `engine.move_entity_facing(id, speed, amount_x, dir_y)` | Como `move_entity`, pero el eje horizontal sigue hacia dónde mira el personaje. |

### Apariencia y animación (principalmente 2D)

| Función | Qué hace |
|---------|----------|
| `engine.set_scale(id, sx, sy)` | Cambia escala. |
| `engine.play_animation(id, name)` | Reproduce animación por nombre; el motor espeja según la última dirección horizontal. |
| `engine.set_default_animation(id, name)` | Animación por defecto al parar. |
| `engine.stop_animation(id)` | Detiene y vuelve al frame 0. |

### Física

| Función | Qué hace |
|---------|----------|
| `engine.set_physics(id, enabled, body_type?)` | Activa/desactiva Rapier. `body_type`: `"dynamic"` (por defecto) o `"static"`. |

### Utilidad

| Función | Qué hace |
|---------|----------|
| `engine.log(message)` | Mensaje en la consola del editor. |

---

## Solo motor 2D

Herramientas del plano lateral y plataformas:

| Función | Qué hace |
|---------|----------|
| `engine.apply_kinematic_gravity(id, speed_x, jump_speed_y, gravity)` | Impulso de salto + velocidad horizontal en cuerpo **kinematic** (ver abajo). |
| `engine.apply_kinematic_impulse(id, dir_x, dir_y, impulse)` | Impulso puntual. |
| `engine.move_entity_slide(id, dx, dy, speed)` | Desplazamiento con shape-cast (sin teletransporte). |
| `engine.move_control(id, speed)` | Movimiento según la tecla del binding activo (`A`/`D`/`W`/`S`, `D-LEFT`, etc.). El motor resuelve la dirección; no compares la tecla en el script. |
| `engine.set_vsync(enabled)` | Activa o desactiva V-Sync. |

**Triggers:** coloca un script en un *execution area* y usa `on_trigger_enter(trigger, actor)` para reaccionar cuando otro personaje entra.

### Gravedad y controles (2D)

**Gravedad del mundo** (panel *Mundo* / IPC `set_gravity`):

- El valor es una **magnitud** (u/s²), siempre aplicada **hacia abajo** en el plano XY del motor.
- El runtime usa internamente el eje Y negativo de Rapier (`-abs(gravity)`), aunque el script o el editor envíen un número positivo.

**`apply_kinematic_gravity(id, speed_x, jump_speed_y, gravity)`** (solo en **play**, cuerpo `kinematic`):

| Parámetro | Efecto |
|-----------|--------|
| `speed_x` | Velocidad horizontal objetivo para el siguiente paso de física. |
| `jump_speed_y` | Impulso vertical sumado una vez (salto). |
| `gravity` | **Ignorado** en la implementación actual; la caída usa la gravedad del mundo configurada en el editor. |

La integración real ocurre en `PhysicsWorld2D::step()` (shape-cast + suelo), no en el cuarto argumento del script.

**`on_press`**:

- Se ejecuta **una sola vez** por pulsación de tecla o botón (sin autorepeat). Usar para saltos y acciones discretas; el movimiento continuo va en `update` / `on_keep`.
- El motor pasa `control_key` con el nombre del binding activo; no hace falta `if control_key == "D"`.

**Ejemplo de control 2D** (un script por tecla; la dirección la resuelve el motor):

```rhai
fn on_keep(entity, control_key) {
    engine.move_control(entity.id, 7.0);
    engine.play_animation(entity.id, "Run");
}

fn on_press(entity, control_key) {
    engine.move_entity_facing(entity.id, 12.0, 0.2, 1.0);
    engine.play_animation(entity.id, "JumpPj");
}
```

---

## Solo motor 3D

Controller de **play** en proyectos 3D (hoy usado en primera persona) y objetos con Rapier:

| Función | Qué hace |
|---------|----------|
| `engine.fp_press_key(key)` | Simula tecla pulsada en play (mismos nombres que el input: `"W"`, `"S"`, `"A"`, `"D"`, `"SHIFT"`, `"SPACE"`, etc.). **En scripts de control FP el motor aplica `control_key` automáticamente**; solo usa `fp_press_key` si necesitas otra tecla distinta al binding. |
| `engine.fp_jump()` | Salto del jugador en play. |
| `engine.fp_set_walk_speed(speed)` | Velocidad base al caminar. |
| `engine.fp_set_sprint_multiplier(mult)` | Multiplicador al sprintar. |
| `engine.fp_set_jump_speed(speed)` | Impulso de salto. |

En 3D las animaciones por frames 2D no son el foco; el personaje jugable en play usa cápsula cinemática (no el mismo pipeline que `move_entity` en XY de un sprite).

**Ejemplo mínimo FP (script de entidad):**

```rhai
fn on_start(entity) {
    engine.fp_set_walk_speed(4.0);
    engine.fp_set_jump_speed(6.5);
}
```

**Scripts de control FP** (acordeón Controles — un script por tecla; cuerpo suelto):

```rhai
// W / A / S / D — misma lógica en cada binding; el motor aplica la tecla del binding.
let WALK_SPEED = 4;
engine.fp_set_walk_speed(WALK_SPEED);
```

```rhai
// SHIFT
let SPRINT_MULTIPLIER = 3;
engine.fp_set_sprint_multiplier(SPRINT_MULTIPLIER);
```

```rhai
// SPACE
let JUMP_SPEED = 6;
engine.fp_set_jump_speed(JUMP_SPEED);
engine.fp_jump();
```

No uses `fp_press_key` ni compares `control_key` en scripts de control FP.

---

## Programación visual (editor de nodos)

Además de escribir Rhai a mano, el editor puede **compilar grafos de nodos** a los mismos callbacks. Detalle del modelo: [`docs/Programing_Model.yaml`](./docs/Programing_Model.yaml).

| Contexto | Dónde abrirlo | Callbacks generados |
|----------|---------------|---------------------|
| Escena | Sidebar **Escenas → Programación** | `on_scene_start()`, `on_scene_tick(dt)` |
| Entidad | Sidebar **Propiedades → Programar entidad** | `on_start(entity)`, `update(entity, dt)` |

Nodos de acción relevantes (compilación → API):

| Nodo (UI) | Rhai generado |
|-----------|---------------|
| Print | `engine.log("…")` |
| Play animation | `engine.play_animation(id, "nombre")` |
| Set scale | `engine.set_scale(entity.id, sx, sy)` |
| Teleport to | `engine.move_to(entity.id, x, y)` — posición **absoluta** |
| Translate | `engine.translate(entity.id, dx, dy)` — **delta** relativo |

En lógica de **entidad**, los nodos de movimiento y escala usan siempre `entity.id` del script en ejecución. En lógica de **escena**, *Play animation* permite elegir otra entidad por id.

---

## Sandbox

Rhai se ejecuta en modo sandbox del motor: sin acceso a sistema de archivos, red ni módulos externos. Solo la API `engine.*` registrada por el runtime y operaciones básicas del lenguaje.

---

## Más detalle

Implementación compartida: `engine_shared/src/scripting/` (`ScriptEngine`, `api.rs`, `script_cmd.rs`). Los crates `engine_2d` y `engine_3d` reexportan el módulo.
