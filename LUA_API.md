# Scripting Lua — RER-ENGINE

Guía breve para escribir scripts en entidades. El motor **2D** y el **3D** comparten la misma forma de script; algunas funciones solo existen en uno de los dos.

**Render del motor:** Rust + wgpu — `rer_engine_2d` y `rer_engine_3d` usan **Vulkan** (Windows y Linux; sin variables de entorno). Ventana overlay junto al editor Electron. Los scripts Lua no configuran la GPU; solo llaman a la API `engine.*` de gameplay.

---

## Forma de un script

Devuelve una tabla con callbacks. `dt` es el delta en segundos.

```lua
local script = {}

function script.on_start(self, entity)
  engine.log("Hola desde entidad " .. entity.id)
end

function script.update(self, entity, dt)
  -- cada frame
end

function script.on_stop(self, entity)
end

return script
```

**Otros callbacks** (según tipo de script):

| Callback | Cuándo |
|----------|--------|
| `on_press(self, entity, key)` | Tecla/control asignado — **una vez por pulsación** (sin autorepeat; ver [Gravedad y controles 2D](#gravedad-y-controles-2d)) |
| `on_trigger_enter(self, trigger, actor)` | Un actor entra en un *execution area* (solo 2D) |

---

## Datos de la entidad (`entity` / `entities`)

En cada frame el motor inyecta `entity` (la tuya) y `entities[id]` (lectura del resto).

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

La integración real ocurre en `PhysicsWorld2D::step()` (shape-cast + suelo), no en el cuarto argumento de Lua.

**`on_press`**:

- Se ejecuta **una sola vez** por pulsación de tecla o botón (sin autorepeat). Usar para saltos y acciones discretas; el movimiento continuo va en `update` / `on_keep`.
- Si el cuerpo es **kinematic** y el script llama `move_entity` / `move_entity_facing` desde `on_press`, el motor convierte ese desplazamiento en un **slide corto** para que se note en una pulsación única (compatibilidad con scripts legacy).

**Ejemplo de control 2D:**

```lua
function script.on_press(self, entity, key)
  if key == "right" then
    engine.move_entity(entity.id, 7, 1, 0)
    engine.play_animation(entity.id, "Run")
  elseif key == "left" then
    engine.move_entity(entity.id, 7, -1, 0)
    engine.play_animation(entity.id, "Run")
  elseif key == "jump" then
    -- jump_speed_y = impulso; el 4º argumento no altera la gravedad del mundo
    engine.apply_kinematic_gravity(entity.id, 0, 12, 0)
    engine.play_animation(entity.id, "Jump")
  end
end
```

---

## Solo motor 3D

Controller de **play** en proyectos 3D (hoy usado en primera persona) y objetos con Rapier:

| Función | Qué hace |
|---------|----------|
| `engine.fp_press_key(key)` | Simula tecla pulsada en play (mismos nombres que el input: `"W"`, `"S"`, `"A"`, `"D"`, `"SHIFT"`, `"SPACE"`, etc.). |
| `engine.fp_jump()` | Salto del jugador en play. |
| `engine.fp_set_walk_speed(speed)` | Velocidad base al caminar. |
| `engine.fp_set_sprint_multiplier(mult)` | Multiplicador al sprintar. |
| `engine.fp_set_jump_speed(speed)` | Impulso de salto. |

Aliases equivalentes (misma implementación): `engine.play_character_press_key`, `play_character_jump`, `play_character_set_walk_speed`, `play_character_set_sprint_multiplier`, `play_character_set_jump_speed`.

En 3D las animaciones por frames 2D no son el foco; el personaje jugable en play usa cápsula cinemática (no el mismo pipeline que `move_entity` en XY de un sprite).

**Ejemplo mínimo FP:**

```lua
function script.on_start(self, entity)
  engine.fp_set_walk_speed(4.0)
  engine.fp_set_jump_speed(6.5)
end

function script.on_press(self, entity, key)
  engine.fp_press_key(key)
  if key == "SPACE" then engine.fp_jump() end
end
```

---

## Sandbox

**Bloqueado:** `io`, `os`, `package`, `require`, `dofile`, `loadfile`.

**Permitido:** `math`, `string`, `table`, `pairs`, `ipairs`, `print`, operaciones básicas de Lua.

---

## Más detalle

Implementación y límites por binario: `engine_2d/src/scripting.rs` y `engine_3d/src/scripting.rs`.
