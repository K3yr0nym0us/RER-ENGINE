# RER-ENGINE — Guía de Scripting Lua

Referencia completa de la API expuesta al sistema de scripts Lua del motor.

---

## Estructura de un script

Los scripts deben retornar una tabla con las funciones de ciclo de vida:

```lua
local script = {}

function script.on_start(self, entity)
  -- Se llama una vez cuando el script se activa
end

function script.update(self, entity, dt)
  -- Se llama cada frame. dt = delta time en segundos
end

function script.on_stop(self, entity)
  -- Se llama al desactivar el script
end

return script
```

> **Modo compatibilidad:** el motor también acepta `function on_press(self, entity, key)` para scripts de control, y callbacks sueltos como `function on_trigger_enter(...)`.

---

## Tabla `entity`

El parámetro `entity` que reciben las funciones contiene el estado actual de la entidad:

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `entity.id` | `number` | ID único de la entidad |
| `entity.x` | `number` | Posición X en unidades de mundo |
| `entity.y` | `number` | Posición Y en unidades de mundo |
| `entity.scale_x` | `number` | Escala horizontal |
| `entity.scale_y` | `number` | Escala vertical |
| `entity.animations` | `table` | Lista de nombres de animaciones disponibles |

---

## API del motor (`engine.*`)

### Posición y movimiento

---

#### `engine.move_to(id, x, y)`

Mueve la entidad a una posición absoluta en el mundo.

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `id` | `number` | ID de la entidad |
| `x` | `number` | Posición X destino |
| `y` | `number` | Posición Y destino |

```lua
function script.update(self, entity, dt)
  engine.move_to(entity.id, 0, 0)  -- Teleportar al origen
end
```

---

#### `engine.translate(id, dx, dy)`

Desplaza la entidad por un delta relativo a su posición actual (sin física).

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `id` | `number` | ID de la entidad |
| `dx` | `number` | Delta en X (unidades de mundo) |
| `dy` | `number` | Delta en Y (unidades de mundo) |

```lua
function script.update(self, entity, dt)
  engine.translate(entity.id, 2 * dt, 0)  -- Mover 2 u/s hacia la derecha
end
```

---

#### `engine.move_entity(id, speed, dir_x, dir_y)`

Mueve la entidad usando el sistema de físicas Rapier. Aplica velocidad lineal para que las colisiones se resuelvan correctamente. Si la entidad no tiene cuerpo físico activo, aplica traslación directa como fallback.

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `id` | `number` | ID de la entidad |
| `speed` | `number` | Velocidad en unidades de mundo por segundo |
| `dir_x` | `number` | Componente X de la dirección (se normaliza internamente) |
| `dir_y` | `number` | Componente Y de la dirección (se normaliza internamente) |

```lua
-- Movimiento horizontal hacia la derecha
engine.move_entity(entity.id, 5.0, 1.0, 0.0)

-- Movimiento diagonal
engine.move_entity(entity.id, 7.0, 1.0, -1.0)
```

> **Uso recomendado** para personajes con físicas habilitadas. Respeta la gravedad y las colisiones definidas en el collider.

---

#### `engine.set_scale(id, sx, sy)`

Cambia la escala de la entidad.

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `id` | `number` | ID de la entidad |
| `sx` | `number` | Escala en X |
| `sy` | `number` | Escala en Y |

```lua
engine.set_scale(entity.id, 2.0, 2.0)  -- Duplicar tamaño
```

---

### Animaciones

---

#### `engine.play_animation(id, name)`

Reproduce una animación por nombre. Si la animación ya está activa, la llamada se ignora para evitar reinicios involuntarios. La animación se reproduce desde el frame 0.

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `id` | `number` | ID de la entidad |
| `name` | `string` | Nombre de la animación definido en el editor |

```lua
engine.play_animation(entity.id, "Run")
engine.play_animation(entity.id, "Idle")
```

El motor aplica el espejo horizontal automáticamente según la dirección en la
que quedó mirando la entidad (detectada por el último movimiento horizontal).
No es necesario llamar una función separada para flip.

---

> **Nota sobre orientación:** en el editor, cada animación tiene configurada su "Orientación" (Derecha / Izquierda). Ese dato representa la orientación base en que fue dibujada la animación y el motor decide automáticamente cuándo espejar en tiempo de ejecución.

---

#### `engine.stop_animation(id)`

Detiene la animación activa y muestra el frame 0.

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `id` | `number` | ID de la entidad |

```lua
engine.stop_animation(entity.id)
```

---

### Físicas

---

#### `engine.set_physics(id, enabled, body_type?)`

Habilita o deshabilita el cuerpo físico Rapier de la entidad.

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `id` | `number` | ID de la entidad |
| `enabled` | `boolean` | `true` para activar, `false` para desactivar |
| `body_type` | `string?` | `"dynamic"` (default) o `"static"`. Solo usado al activar. |

```lua
engine.set_physics(entity.id, true, "dynamic")  -- Activar como dinámico
engine.set_physics(entity.id, false)             -- Desactivar físicas
```

---

### Utilidades

---

#### `engine.log(message)`

Envía un mensaje al log del editor (visible en la consola del motor).

| Parámetro | Tipo | Descripción |
|-----------|------|-------------|
| `message` | `string` | Texto a registrar |

```lua
engine.log("Entidad " .. entity.id .. " inicializada")
```

---

## Tabla `entities`

Diccionario de todas las entidades activas, indexado por ID. Disponible en cualquier script:

```lua
local other = entities[42]
if other then
  engine.log("Posición de entidad 42: " .. other.x .. ", " .. other.y)
end
```

Cada entrada tiene los mismos campos que la tabla `entity` (ver arriba).

---

## Tipos de scripts

### Script de animación

Se adjunta a una animación específica desde el editor. Se activa al reproducir esa animación y se detiene al terminarla.

```lua
local script = {}

function script.on_start(self, entity)
  engine.log("Animación iniciada en entidad " .. entity.id)
end

function script.update(self, entity, dt)
  -- Código ejecutado cada frame mientras la animación esté activa
end

return script
```

### Script de control (input)

Se ejecuta cuando el jugador presiona la tecla/control asignada. Recibe la `entity` y la `key` presionada.

```lua
local script = {}

function script.on_press(self, entity, key)
  if key == "right" then
    engine.move_entity(entity.id, 5.0, 1.0, 0.0)
    engine.play_animation(entity.id, "Run")
  end
  if key == "left" then
    engine.move_entity(entity.id, 5.0, -1.0, 0.0)
    engine.play_animation(entity.id, "Run")
  end
  if key == "attack" then
    -- El flip de ataque depende de qué animación corría antes
    engine.play_animation(entity.id, "Attack")
  end
end

return script
```

### Script de trigger (área de ejecución)

Se ejecuta cuando un actor entra en un área de tipo `execution_area`.

```lua
local script = {}

function script.on_trigger_enter(self, trigger, actor)
  engine.log("Actor " .. actor.id .. " entró en trigger " .. trigger.id)
  engine.play_animation(actor.id, "Celebrate")
end

return script
```

---

## Patrones comunes

### Movimiento con físicas y animación direccional

```lua
local script = {}

function script.on_press(self, entity, key)
  if key == "right" then
    engine.move_entity(entity.id, 7.0, 1.0, 0.0)
    engine.play_animation(entity.id, "Run")
  end

  if key == "left" then
    engine.move_entity(entity.id, 7.0, -1.0, 0.0)
    engine.play_animation(entity.id, "Run")
  end

  if key == "jump" then
    engine.move_entity(entity.id, 10.0, 0.0, 1.0)
    engine.play_animation(entity.id, "Jump")
  end
end

return script
```

### Animación idle de retorno

```lua
local script = {}
local idle_timer = 0.0

function script.update(self, entity, dt)
  idle_timer = idle_timer + dt
  if idle_timer > 2.0 then
    engine.play_animation(entity.id, "Idle")
    idle_timer = 0.0
  end
end

return script
```

### Log de posición cada segundo

```lua
local script = {}
local elapsed = 0.0

function script.update(self, entity, dt)
  elapsed = elapsed + dt
  if elapsed >= 1.0 then
    engine.log("Pos: " .. entity.x .. ", " .. entity.y)
    elapsed = 0.0
  end
end

return script
```

---

## Sandbox de seguridad

Las siguientes librerías de Lua **están bloqueadas** por seguridad:

| Bloqueado | Razón |
|-----------|-------|
| `io` | Acceso al sistema de archivos |
| `os` | Ejecución de procesos del sistema |
| `package` / `require` | Carga de módulos externos |
| `dofile` / `loadfile` | Carga de archivos arbitrarios |

**Disponibles:** `math`, `string`, `table`, `pairs`, `ipairs`, `tostring`, `tonumber`, `print`.
