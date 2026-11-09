# Plan — Ventana única para el motor 3D en el editor Rust

## Objetivo

Eliminar la arquitectura de overlay y conseguir que editor egui y escena 3D convivan en:

- un solo proceso,
- una sola ventana winit,
- un solo `VkInstance` / `VkDevice` / `VkQueue`,
- una sola swapchain,
- un solo command buffer por frame,
- un solo event loop.

La UI y la escena deben dibujarse en el mismo frame:

1. egui calcula el rectángulo central.
2. El motor 3D renderiza dentro de ese rectángulo con `cmd_set_viewport` y `cmd_set_scissor`.
3. egui se renderiza encima en el mismo command buffer.

Las modales Electron no se tocan: siguen siendo ventanas independientes gestionadas por el host headless de Electron.

---

## Estado de partida

```text
Editor Rust
├── winit EventLoop
├── ventana egui
├── wgpu + egui-wgpu
└── overlay geometry por Win32

Motor 3D
├── winit EventLoop propio
├── ventana overlay propia
├── ash / Vulkan
├── VkInstance/VkDevice propios
├── swapchain propia
└── submit_and_present propio
```

Problemas:

- dos ventanas top-level,
- dos event loops,
- dos devices Vulkan/wgpu,
- dos swapchains,
- dos command buffers,
- tracker Win32 (`GWLP_HWNDPARENT`, `SetWindowPos`, `WinEventHook`),
- el viewport del motor se calcula con constantes, no con el layout real de egui.

---

## Decisiones técnicas

### Backend de UI

Usar **`egui-ash-renderer` 0.12.0**:

- usa `ash 0.38`,
- usa `egui 0.35` (la misma línea que el resto del editor),
- evita saltar a egui 0.36 en esta fase.

No usar `egui-ash` 0.4.0: depende de egui 0.25 y está desactualizado.

`egui-ash-renderer` 0.13 exige egui 0.36; se puede adoptar más adelante como
actualización independiente.

Esto implica:

- mantener `egui` y `egui-winit` en 0.35,
- quitar `egui-wgpu` y `wgpu` del editor,
- añadir `ash`, `ash-window` y `egui-ash-renderer`.

### Contexto Vulkan

El editor será el único dueño de:

- `VkInstance`,
- `VkSurfaceKHR`,
- `VkPhysicalDevice`,
- `VkDevice`,
- `VkQueue`,
- swapchain,
- semáforos y fences,
- command buffers.

El motor 3D recibirá ese contexto ya creado.

### Motor 3D como librería

Separar `engine_3d` en tres capas:

1. **Core**: ECS, física, escena, animación, scripts, input lógico.
2. **Renderer**: pases Vulkan, recursos GPU, `FrameRenderer`, sombras, SSR, RT, HUD 3D.
3. **Host**: ventana winit, swapchain, event loop y present.

La capa host solo se usará para el binario standalone de depuración. El editor usará core + renderer.

---

## Fase 1 — Preparar la separación del motor 3D

### 1.1 Crear API de renderer externo

Añadir una API equivalente a:

```rust
pub struct Engine3dRenderer {
    // recursos que hoy viven en FrameRenderer
}

impl Engine3dRenderer {
    pub fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        color_view: vk::ImageView,
        color_image: vk::Image,
        extent: vk::Extent2D,
        viewport: vk::Rect2D,
        frame: &FrameInput<'_>,
    ) -> Result<(), VulkanError>;
}
```

Requisitos:

- No crear swapchain.
- No adquirir imágenes.
- No hacer `submit` ni `present`.
- No abrir/cerrar el command buffer.
- Grabar en el command buffer y la imagen que le pase el editor.
- Aplicar viewport/scissor al rectángulo central.

### 1.2 Separar recursos compartidos

Mover a un contenedor reutilizable:

- `SharedGpu`
- allocator,
- pipelines,
- samplers,
- layouts,
- shadow map,
- G-buffer,
- buffers de instancias,
- BVH/TLAS.

Evitar que `State` cree su propio `VkDevice`.

### 1.3 Mantener standalone

`rer_engine_3d` sigue existiendo como host:

```text
rer_engine_3d (binario)
├── winit host
├── swapchain
└── llama a Engine3dRenderer::record
```

Así se puede seguir depurando sin el editor.

### Criterios

- El motor compila como librería.
- El binario standalone sigue funcionando.
- `cargo test` y `clippy` limpios.

---

## Fase 2 — Migrar el editor a ash + egui-ash-renderer

> Estado: el editor ya usa `ash` + `egui-ash-renderer` 0.12 y renderiza su UI
> sin `wgpu`. Falta compartir el `VkDevice`/swapchain con el motor en la fase 4.

### 2.1 Dependencias

- `egui = 0.35`
- `egui-winit = 0.35`
- `egui-ash-renderer = 0.12`
- `ash = 0.38`
- `ash-window = 0.13`

Eliminar:

- `wgpu`
- `egui-wgpu`
- `gpu.rs` basado en wgpu

### 2.2 Contexto Vulkan del editor

Crear en el editor:

- instancia,
- superficie winit,
- selección de physical device,
- device y queue,
- swapchain,
- imágenes/views,
- semáforos y fences,
- command pool y command buffers.

### 2.3 Renderer egui

Inicializar `egui-ash-renderer` con:

- `VkDevice`,
- `VkQueue`,
- color format,
- extent,
- pipeline cache.

Mantener `egui-winit` para:

- input,
- text IME,
- clipboard,
- cursor,
- foco.

### Criterios

- El editor arranca con ash.
- Se dibuja un frame egui sin motor.
- Resize y DPI funcionan.

---

## Fase 3 — Layout y viewport real

### 3.1 Rectángulo del CentralPanel

Durante el build de egui:

```rust
let viewport_rect_points = ui.available_rect_before_wrap();
```

Guardarlo en un `FrameLayout`:

```rust
pub struct FrameLayout {
    pub window_size_px: [u32; 2],
    pub viewport_rect_px: vk::Rect2D,
    pub pixels_per_point: f32,
}
```

Conversión:

```text
px = point * pixels_per_point
```

Redondear y recortar contra la ventana.

### 3.2 CentralPanel transparente

El `CentralPanel` no debe pintar fondo opaco sobre el viewport.

- Paneles laterales y barras pintan su fondo.
- El centro queda limpio para la escena.
- egui solo dibuja controles flotantes del centro si existen.

### Criterios

- `engine_viewport.rs` deja de calcular el rect con constantes.
- El rect sigue exactamente al layout egui en cualquier tamaño/DPI.

---

## Fase 4 — Frame unificado

### 4.1 Orden por frame

```text
1. winit entrega eventos
2. egui procesa input y construye UI
3. se obtiene viewport_rect_px
4. vkAcquireNextImageKHR
5. vkBeginCommandBuffer
6. barrera a COLOR_ATTACHMENT_OPTIMAL
7. clear de la swapchain completa
8. cmd_set_viewport(viewport_rect)
9. cmd_set_scissor(viewport_rect)
10. Engine3dRenderer::record(...)
11. cmd_set_viewport(ventana completa)
12. cmd_set_scissor(ventana completa)
13. egui_ash_renderer dibuja con loadOp = LOAD
14. vkEndCommandBuffer
15. un único vkQueueSubmit
16. vkQueuePresentKHR
```

### 4.2 Sincronización

- Un fence por frame.
- Semáforo de adquisición y de render.
- Recrear swapchain en resize/outdated/suboptimal.
- No duplicar command buffers entre motor y editor.

### 4.3 Limpieza

- Limpiar la swapchain completa antes del motor.
- El motor solo escribe dentro del viewport.
- egui pinta encima sin borrar la escena.

### Criterios

- La escena aparece dentro del área central.
- La UI se dibuja encima.
- No hay ventana overlay.
- No hay parpadeo ni desalineación al mover/redimensionar.

---

## Fase 5 — Input unificado

### 5.1 Event loop único

Eliminar el `EventLoop` del motor en modo editor.

`window_event` hace:

```rust
let response = egui_state.on_window_event(window, &event);

if response.consumed {
    return;
}

if pointer_inside_viewport {
    engine.handle_input(event, viewport_rect);
}
```

### 5.2 Coordenadas del motor

- Restar el origen del viewport.
- Convertir a píxeles físicos.
- Actualizar el tamaño lógico del motor cuando cambie el rect.

### 5.3 Foco

- Si egui quiere teclado, no enviar teclas al motor.
- Al hacer clic en el viewport, dar foco lógico al motor.
- Escape y atajos globales deben decidirse en un solo lugar.

### Criterios

- Sliders/acordeones no mueven la cámara.
- Clic en viewport sí controla la escena.
- No hay doble captura de teclado/ratón.

---

## Fase 6 — Modales Electron

No cambiar esta parte:

- El host headless sigue en Electron.
- `legacy_window_request` sigue igual.
- El editor Rust sigue pidiendo modales por stdout.
- Las modales siguen siendo ventanas Electron separadas.

Solo cambia el proceso Rust del editor/motor, no la lógica de proyecto ni las modales.

### Criterios

- Crear escena, eliminar escena, sockets, propiedades, RHAI y nodos siguen funcionando.
- No se rompe el guardado que pide `project_state` al host.

---

## Fase 7 — Motor 2D

Estado: el 2D ya usa Ash y `rer_engine_core::vulkan::SharedGpu`, y vive en
`engine/src/engine2d/` (feature `engine2d`, bin `rer_engine_2d`). Queda la parte de
ventana única:

1. Construir el `State` 2D sobre el `SharedGpu` del editor (sin crear swapchain propia).
2. Renderizar el scene target 2D al viewport del `CentralPanel` en el mismo frame que egui.
3. Traducir el input del rect y despachar los comandos 2D por el host in-process.

Mientras eso no esté, el editor sigue lanzando `rer_engine_2d` como proceso con ventana
overlay; el 3D ya va in-process.

---

## Fase 8 — Eliminación de la arquitectura vieja

Eliminar cuando la ventana única esté validada:

- `engine_viewport.rs`
- `setup_overlay_win32`
- `start_position_tracker`
- `GWLP_HWNDPARENT`
- `WinEventHook`
- `SetWindowPos` de seguimiento
- spawn del motor como proceso para el editor
- IPC de `set_bounds`
- ventana overlay del motor

Mantener el binario standalone para debug, si se desea.

---

## Riesgos

| Riesgo | Mitigación |
|---|---|
| `FrameRenderer` está acoplado a swapchain/FrameTarget | Extraer `record` a un `FrameTarget` externo |
| Portar editor de wgpu a ash es grande | Hacerlo en una rama/fase aislada y validar egui solo primero |
| egui 0.36 cambia APIs | Mantener 0.35 con `egui-ash-renderer` 0.12; subir versión en un cambio aparte |
| El motor crea su device en varios sitios | Centralizar en `SharedGpu` propiedad del editor |
| Input duplicado | Un solo event loop y `consumed` |
| Resize/DPI | Tests manuales en 100 %, 125 %, 150 % |
| Modales Electron | No tocar el host ni los `legacy_window_request` |

---

## Criterios de aceptación

1. Un solo proceso Rust para editor + motor.
2. Una sola ventana winit.
3. Un solo `VkDevice` y una sola swapchain.
4. Un command buffer y un submit por frame.
5. La escena 3D se dibuja solo dentro del rect del `CentralPanel`.
6. egui se dibuja encima en el mismo frame.
7. No existe ventana overlay ni tracker Win32.
8. `response.consumed` evita que la UI controle la cámara.
9. Las modales Electron siguen funcionando sin cambios.
10. `fmt`, `clippy`, tests y check del workspace limpios.

---

## Orden recomendado

1. Separar `engine_3d` en librería.
2. Adaptar `FrameRenderer::record` para command buffer/image/viewport externos.
3. Montar ash + `egui-ash-renderer` en el editor.
4. Conectar layout del `CentralPanel` al viewport.
5. Unificar command buffer y present.
6. Unificar input.
7. Validar modales Electron.
8. Retirar overlay/tracker.
9. Migrar 2D.
