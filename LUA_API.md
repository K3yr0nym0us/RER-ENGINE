# Scripting Lua — RER-ENGINE

Guía breve para escribir scripts en entidades. El motor **2D** y el **3D** comparten la misma forma de script; algunas funciones solo existen en uno de los dos.

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
| `on_press(self, entity, key)` | Tecla/control asignado (script de control) |
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
| `engine.apply_kinematic_gravity(id, speed_x, jump_speed_y, gravity)` | Salto/gravedad en cuerpo kinematic. |
| `engine.apply_kinematic_impulse(id, dir_x, dir_y, impulse)` | Impulso puntual. |
| `engine.move_entity_slide(id, dx, dy, speed)` | Desplazamiento con shape-cast (sin teletransporte). |
| `engine.set_vsync(enabled)` | Activa o desactiva V-Sync. |

**Triggers:** coloca un script en un *execution area* y usa `on_trigger_enter(trigger, actor)` para reaccionar cuando otro personaje entra.

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
    engine.apply_kinematic_gravity(entity.id, 0, 12, -30)
    engine.play_animation(entity.id, "Jump")
  end
end
```

---

## Solo motor 3D

Pensado para proyectos **first-person** y objetos 3D con Rapier:

| Función | Qué hace |
|---------|----------|
| `engine.fp_press_key(key)` | Simula tecla pulsada en play (mismos nombres que el input: `"W"`, `"S"`, `"A"`, `"D"`, `"SHIFT"`, `"SPACE"`, etc.). |
| `engine.fp_jump()` | Salto del jugador en play. |
| `engine.fp_set_walk_speed(speed)` | Velocidad base al caminar. |
| `engine.fp_set_sprint_multiplier(mult)` | Multiplicador al sprintar. |
| `engine.fp_set_jump_speed(speed)` | Impulso de salto. |

En 3D las animaciones por frames 2D no son el foco; el jugador FP usa cápsula cinemática (no el mismo pipeline que `move_entity` en XY de un sprite).

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
