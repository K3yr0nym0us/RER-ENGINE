# RER-ENGINE

> **R**eact + **E**lectron + **R**ust ENGINE
> Un motor de videojuegos 2D/3D enfocado en una idea simple:
> **hacer que crear juegos sea algo natural, no mecánico.**

---

## 🧠 Filosofía del proyecto

RER-ENGINE nace de un problema claro:

> Los motores modernos son potentes, pero muchas veces se sienten **complejos, fragmentados y poco intuitivos**.

* La lógica de un personaje está repartida en múltiples sistemas
* Las acciones requieren navegar por capas de configuración
* El flujo de trabajo es técnico, no humano

### 🎯 Objetivo

Crear un motor donde:

* **Cada entidad contiene su propia lógica**
* **Las acciones son directas e intuitivas**
* **El editor refleja cómo piensas el juego, no cómo funciona internamente el engine**

👉 En otras palabras:
**menos “configurar sistemas”, más “crear comportamiento”.**

---

## ⚙️ Enfoque técnico

RER-ENGINE separa claramente:

* **Editor (Electron + React)** → interfaz, herramientas, flujo de usuario
* **Motor (Rust + wgpu)** → render, física, ejecución

Ambos se comunican mediante un protocolo IPC simple basado en JSON.

```
┌─────────────────────────────────────────────┐
│             Electron (BrowserWindow)        │
│  ┌──────────────────┐  ┌───────────────────┐│
│  │   React + TS     │  │  Viewport nativo  ││
│  │   (UI/Editor)    │  │  ← Rust / wgpu    ││
│  └──────────────────┘  └───────────────────┘│
└─────────────────────────────────────────────┘
          ↑ IPC — JSON lines stdin/stdout
```

### ¿Por qué esta arquitectura?

* Permite iterar el editor sin tocar el motor
* Mantiene el runtime desacoplado
* Facilita debugging y control total del pipeline

---

## 🧩 Principios de diseño

* **Human-first design**
  El editor debe reflejar cómo piensa el desarrollador, no cómo está implementado el motor.

* **Data-driven**
  Las entidades contienen datos + comportamiento claro (no lógica dispersa).

* **Modularidad real**
  El motor puede evolucionar sin romper el editor.

* **Simplicidad explícita**
  Preferir sistemas claros antes que soluciones mágicas u ocultas.

---

## 🧱 Tecnologías

### Motor (Rust)

* `wgpu` — render multiplataforma (Vulkan/Metal/GL)
* `winit` — ventana y eventos
* `glam` — matemáticas
* `rapier` — físicas 2D/3D
* `mlua` — scripting embebido
* `gltf` + `image` — assets

### Editor

* Electron
* React + TypeScript
* Bootstrap

---

## 🏗️ Estado actual

El motor ya cuenta con:

* Render 2D/3D funcional
* ECS básico
* Física integrada (Rapier)
* Sistema de scripting (Lua con lifecycle)
* Editor visual con manipulación de entidades
* Sistema de escenas múltiples
* Guardado empaquetado (`.save` con assets incluidos)
* Comunicación IPC estable

👉 Es una base sólida para evolucionar hacia un engine completo.

---

## ⚠️ Limitaciones actuales

El engine aún está en fase de maduración:

* Sin optimización de render (batching, culling)
* ECS sin queries avanzadas ni archetypes
* Sin particionado espacial
* Herramientas de debug limitadas
* Pipeline de assets aún básico
* IPC puede convertirse en cuello de botella en escenas grandes

---

## 🔧 Áreas de mejora prioritarias

* Optimización de render (batching, atlas, culling)
* Sistema de debug runtime (FPS, métricas)
* ECS más avanzado (queries multi-componente)
* Particionado espacial
* Prefabs / reutilización de entidades
* Hot reload (scripts, shaders, assets)

---

## 🧠 Scripting (ejemplo)

El comportamiento vive directamente en la entidad:

```lua
function on_start(self)
    engine.log("Entidad iniciada")
end

function update(self, dt)
    self:translate(2 * dt, 0)
end

function on_trigger_enter(self, other)
    engine.log("Colisión detectada")
end
```

👉 La lógica está donde pertenece: en el objeto.

---

## 💾 Formato de proyecto

Los proyectos se guardan como `.save`:

* ZIP portable
* `manifest.json`
* `assets/`, `sounds/`, `scripting/`

Permite mover proyectos entre sistemas sin romper rutas.

---

## 🚀 Visión a largo plazo

RER-ENGINE no busca competir con motores masivos.

Busca algo distinto:

> Un entorno donde crear videojuegos sea **intuitivo, directo y entendible**.

* Menos configuración
* Menos abstracciones innecesarias
* Más control real

---

## 🧪 Estado del proyecto

Proyecto experimental en desarrollo activo.
Probado en Linux (X11) y Windows 11.

---

## 📌 Regla de oro

Si puedes:

* Abrir el editor
* Ver el motor renderizando
* Crear entidades y darles comportamiento fácilmente

👉 entonces el engine está cumpliendo su propósito.
