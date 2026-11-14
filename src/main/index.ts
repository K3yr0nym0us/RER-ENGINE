import { app, BrowserWindow, ipcMain, dialog, Menu, session } from 'electron';
import type { ChildProcess } from 'child_process';
import { spawn } from 'child_process';
import path from 'path';
import fs from 'fs';
import AdmZip from 'adm-zip';

import type { 
  EngineCommand, 
  EngineEvent, 
  Entity3D,
  SavedEntity,
  GameStyle, 
  EngineStartPayload,
  OpenProjectResult, 
  ProjectSaveData,
  ProjectType,
  SavedControls,
  SavedScript,
} from '../shared-types/types';
import { entityPathMarker } from '../shared-types/types';
import {
  initModalElectron,
  openModalElectronWindow,
  closeModalElectronWindow,
  resendPendingRenderToModal,
  destroyModalElectronWindow,
  applyModalElectronContentHeight,
  sendPatchToModal,
  restackModalElectronIfOpen,
  repositionViewportCornerModalIfOpen,
  setModalElectronAnchorRect,
  preloadModalElectronWindow,
  forwardEngineEventToModal,
  publishEditorLocale,
} from './modalElectronWindow';
import { resolveAppWindowIcon } from './appWindowIcon';
import { registerDevToolsShortcut } from './devToolsShortcut';
import { engineWindowRectToDip } from './modalElectronSizes';
import { registerEngineCmdIpc } from './Engine/registerEngineCmdIpc';
import { sendEngineStartupScene } from './Engine/engineStartupScene';
import { autoStartEnabledPlugins, registerPluginIpc } from './plugins/registerPluginIpc';
import { stopOpenCodeServer } from './plugins/local-ai-assistant/openCodeRuntime'
import {
  destroyAiAssistantOverlay,
  initAiAssistantOverlay,
  hideAiAssistantOverlayForMainMinimize,
  repositionAiAssistantOverlayIfOpen,
  resendAiAssistantConfig,
  restackAiAssistantOverlayIfOpen,
  setAiAssistantOverlayLayout,
  showAiAssistantOverlay,
  hideAiAssistantOverlay,
  beginAiAssistantFabDrag,
  endAiAssistantFabDrag,
  updateAiAssistantOverlayConfig,
} from './aiAssistantWindow';
import type { AiAssistantOverlayConfig } from './aiAssistantWindow';
import type {
  ModalElectronDelegateRequest,
  ModalElectronOpenRequest,
  ModalElectronResultPayload,
} from '../shared-types/types';

// Sin GPU hardware disponible: deshabilitar el proceso GPU de Chromium
// para evitar spam de viz_main_impl / command_buffer_proxy_impl
app.commandLine.appendSwitch('disable-gpu');
app.commandLine.appendSwitch('disable-software-rasterizer');

// ---------------------------------------------------------------------------
// Variables de módulo
// ---------------------------------------------------------------------------
let mainWindow: BrowserWindow | null = null
let currentGameStyle: GameStyle | null = null
/** Cuando `gameStyle` es null, decide 2D vs 3D hasta arrancar el motor. */
let currentProjectType: ProjectType | null = null

// Buffer de eventos que llegaron antes de que el renderer estuviera listo
let rendererReady = false
const eventBuffer: EngineEvent[] = []

// Path del proyecto abierto/guardado actualmente.
let currentProjectFilePath: string | null = null
/** Carpeta extraída de un `.save` abierto; contiene `manifest.json` + assets empaquetados. */
let currentProjectExtractDir: string | null = null
/** Workspace de sesión para proyecto 3D nuevo (imported/models, source/models). No es un `.save`. */
let currentProjectSessionDir: string | null = null

// Directorios temporales con contenido extraído de .save que deben vivir
// mientras el proyecto está abierto para que el motor lea rutas absolutas.
const extractedProjectDirs = new Set<string>()

// Ventana secundaria del editor de scripts Rhai
// (eliminada — el editor ahora vive en un modal de Bootstrap dentro del renderer)

/** true tras recibir `ready` del proceso motor de la sesión actual. */
let engineReceivedReady = false

/** Evita reenviar `set_scene` 2D/3D si el motor emite más de un `ready`. */
let engine2dStartupSceneSent = false
let engine3dStartupSceneSent = false

/** Binario base del motor en la sesión actual (`rer_engine_2d` / `rer_engine_3d`). */
let lastEngineBinary = 'rer_engine_2d'

/** Proceso del editor Rust/egui (ventana independiente, convive con Electron). */
let editorFrameProcess: ChildProcess | null = null
/** Evita relanzar el editor si el usuario lo cerró durante la sesión. */
let editorFrameLaunched = false
/** Cierre controlado del editor: no notificar como crash. */
let editorFrameStopping = false
/** La ventana React está oculta y sigue viva como host de ventanas legacy. */
let editorHostHidden = false
/** Locale actual del renderer (se reenvía al editor si se lanza después). */
let currentLocale: 'en' | 'es' = 'en'

function gpuStartupErrorMessage(): string {
  return (
    'No se pudo iniciar el motor gráfico con Vulkan. Instala o actualiza los controladores de video. ' +
    'Reinicia el editor después de instalar drivers.'
  )
}

/** Líneas habituales de wgpu/Vulkan en Windows que no indican fallo del motor. */
function isBenignEngineStderrLine(line: string): boolean {
  const l = line.toLowerCase()
  return (
    l.includes('loader_get_json')
    || l.includes('eosoverlay')
    || l.includes('galaxyoverlayvklayer')
    || l.includes('unrecognized present mode')
    || l.includes('vk_layer_khronos_validation')
    || l.includes('windows_read_data_files_in_registry')
    || l.includes('does not conform to naming standard')
  )
}

/** Solo si el proceso murió sin haber enviado `ready` (no usar stderr: avisos del loader son ruido). */
function sendEventToRenderer(event: EngineEvent): void {
  if (rendererReady && mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('engine:event', event)
  } else {
    eventBuffer.push(event)
  }
  // La modal worldgen abierta necesita eventos del engine (models_list).
  forwardEngineEventToModal(event)
}

function focusMainEditorWindow(): void {
  if (!mainWindow || mainWindow.isDestroyed() || !mainWindow.isVisible()) return
  mainWindow.focus()
  mainWindow.webContents.focus()
}

// ---------------------------------------------------------------------------
// Ventana principal (UI React)
// ---------------------------------------------------------------------------
function createMainWindow(): void {
  Menu.setApplicationMenu(null)

  const windowIcon = resolveAppWindowIcon()

  mainWindow = new BrowserWindow({
    width:  1280,
    height: 800,
    minWidth:  900,
    minHeight: 600,
    title: 'RER-ENGINE',
    ...(windowIcon ? { icon: windowIcon } : {}),
    backgroundColor: '#0d0d1a',
    webPreferences: {
      preload:          path.join(__dirname, '../preload/index.js'),
      sandbox:          false,
      contextIsolation: true,
      nodeIntegration:  false,
      // La ventana queda oculta tras arrancar el editor Rust; su renderer sigue
      // vivo como host de las ventanas legacy (scripts/nodos).
      backgroundThrottling: false,
    },
  })

  // Abrir DevTools queda solo con el atajo manual (Ctrl+Shift+I / F12).
  registerDevToolsShortcut(mainWindow)

  // En desarrollo carga el servidor de Vite; en producción, el build.
  if (process.env['ELECTRON_RENDERER_URL']) {
    mainWindow.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    mainWindow.loadFile(
      path.join(__dirname, '../renderer/index.html'),
    )
  }

  mainWindow.on('closed', () => {
    rendererReady = false
    destroyAiAssistantOverlay()
    destroyModalElectronWindow()
    stopEngine()
    mainWindow = null
  })

  const syncViewportAndModalOnMainWindowChange = (): void => {
    repositionViewportCornerModalIfOpen()
    repositionAiAssistantOverlayIfOpen()
  }

  // Windows: el motor usa WinEventHook; la modal de propiedades se recoloca aquí.
  const hideOverlaysOnMainMinimize = (): void => {
    hideAiAssistantOverlayForMainMinimize()
    repositionViewportCornerModalIfOpen()
  }

  mainWindow.on('move', syncViewportAndModalOnMainWindowChange)
  mainWindow.on('resize', syncViewportAndModalOnMainWindowChange)
  mainWindow.on('minimize', hideOverlaysOnMainMinimize)
  mainWindow.on('hide', hideOverlaysOnMainMinimize)
  mainWindow.on('restore', syncViewportAndModalOnMainWindowChange)
  mainWindow.on('show', syncViewportAndModalOnMainWindowChange)

  // Clic en el viewport winit: la ventana principal pierde foco; mantener la modal encima del motor.
  mainWindow.on('blur', () => {
    restackModalElectronIfOpen()
    restackAiAssistantOverlayIfOpen()
  })

  // Una vez que el renderer cargó y sus listeners están activos,
  // vaciar el buffer de eventos que llegaron antes de tiempo.
  mainWindow.webContents.on('did-finish-load', () => {
    rendererReady = true
    for (const event of eventBuffer) {
      mainWindow?.webContents.send('engine:event', event)
    }
    eventBuffer.length = 0
  })
}

// ---------------------------------------------------------------------------
// Ventana React oculta como host de ventanas legacy
// ---------------------------------------------------------------------------
/**
 * Oculta la ventana React cuando el editor Rust ya está listo.
 *
 * No se destruye: su renderer sigue vivo para abrir el editor RHAI, el editor
 * de nodos y las modales legacy que todavía no están migradas.
 */
function hideMainWindowForEditor(): void {
  if (editorHostHidden) return
  editorHostHidden = true
  if (!mainWindow || mainWindow.isDestroyed()) return
  mainWindow.hide()
  mainWindow.setSkipTaskbar(true)
  console.log('[editor] ventana React oculta; Electron sigue como host de ventanas legacy')
}

/** Informa de un fallo del editor Rust y termina Electron (ya no hay UI editor). */
function failEditorAndQuit(message: string): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.hide()
  }
  dialog.showErrorBox('RER-ENGINE', message)
  app.quit()
}

// ---------------------------------------------------------------------------
// Proceso del motor Rust
// ---------------------------------------------------------------------------
function engineBinaryLabel(): string {
  if (currentProjectType === '3D') return 'rer_engine_3d'
  if (currentProjectType === '2D') return 'rer_engine_2d'
  return 'rer_engine_editor'
}

/** Procesa un evento del motor (el editor Rust los reenvía desde su stdout). */
function handleEngineEvent(event: EngineEvent): void {
  if (event.event === 'ready') {
    engineReceivedReady = true
    sendEngineStartupScene({
      getEngineBinary: () => lastEngineBinary,
      getProjectType: () => currentProjectType,
      getExtractDir: () => currentProjectExtractDir ?? '',
      getGameStyle: () => currentGameStyle,
      sendToEngine,
      is2dStartupSceneSent: () => engine2dStartupSceneSent,
      mark2dStartupSceneSent: () => { engine2dStartupSceneSent = true },
      is3dStartupSceneSent: () => engine3dStartupSceneSent,
      mark3dStartupSceneSent: () => { engine3dStartupSceneSent = true },
    })
  }
  // Tras ready: omitir errores GPU genéricos; dejar pasar errores de runtime (quick_build).
  if (event.event === 'error' && engineReceivedReady) {
    const msg = String((event as { message?: string }).message ?? '')
    if (!msg.includes('[quick_build]')) {
      return
    }
  }
  if (
    (event.event === 'collider_created' || event.event === 'execution_area_created')
    && (event as { position?: unknown }).position != null
  ) {
    focusMainEditorWindow()
  }
  sendEventToRenderer(event)
}

/** Mensajes propios del editor Rust (no son eventos del motor). */
function handleEditorMessage(message: Record<string, unknown>): boolean {
  switch (message.type) {
    case 'editor_locale': {
      const locale = message.locale === 'es' ? 'es' : 'en'
      currentLocale = locale
      mainWindow?.webContents.send('editor:locale', locale)
      publishEditorLocale(locale)
      updateAiAssistantOverlayConfig({ locale })
      return true
    }
    case 'editor_ready': {
      hideMainWindowForEditor();
      // Precarga la modal oculta después de que el editor nativo ya está en pantalla,
      // para no competir con el arranque del motor/GPU.
      setTimeout(() => preloadModalElectronWindow(), 600);
      return true
    }
    case 'legacy_window_request': {
      const kind = typeof message.kind === 'string' ? message.kind : ''
      if (!kind) return true
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send('legacy-window:open', {
          kind,
          payload: message.payload ?? {},
        })
      } else {
        console.warn('[legacy] no hay host React disponible para abrir', kind)
      }
      return true
    }
    case 'exit_request': {
      requestExitModal(message.rect)
      return true
    }
    case 'project_state_request': {
      const id = Number(message.id ?? 0)
      mainWindow?.webContents.send('project-state:request', id)
      return true
    }
    case 'project_saved': {
      const filePath = typeof message.path === 'string' ? message.path : ''
      mainWindow?.webContents.send('project-state:saved', filePath)
      return true
    }
    case 'project_save_failed': {
      const text = typeof message.message === 'string' ? message.message : ''
      mainWindow?.webContents.send('project-state:save-failed', text)
      return true
    }
    default:
      return false
  }
}

function handleEngineLine(line: string): void {
  const trimmed = line.trim()
  if (!trimmed) return

  try {
    const parsed = JSON.parse(trimmed) as Record<string, unknown>
    if (typeof parsed.type === 'string' && handleEditorMessage(parsed)) {
      return
    }
    const event = parsed as unknown as EngineEvent
    if (!('event' in event)) {
      console.log('[editor stdout]', trimmed)
      return
    }
    handleEngineEvent(event)
  } catch {
    console.log('[editor stdout]', trimmed)
  }
}

/**
 * Inicia una sesión de motor. El proceso del motor lo lanza y controla el
 * editor Rust; Electron solo sigue recibiendo eventos para la UI React antigua.
 */
function startEngineSession(): void {
  engineReceivedReady = false
  engine2dStartupSceneSent = false
  engine3dStartupSceneSent = false
  lastEngineBinary = engineBinaryLabel()
  startEditorFrame()
}

// ---------------------------------------------------------------------------
// Ventana del editor Rust/egui (independiente de Electron)
// ---------------------------------------------------------------------------
function editorFrameBinaryName(): string {
  return process.platform === 'win32' ? 'rer_engine_editor.exe' : 'rer_engine_editor'
}

/**
 * Lanza la ventana egui junto al editor actual.
 *
 * Es un proceso hijo de Electron para poder cerrarlo con la app, pero su
 * ventana es totalmente independiente: no se acopla, no se reposiciona y no
 * usa HWND padre.
 */
function startEditorFrame(): void {
  if (editorFrameLaunched) return
  editorFrameLaunched = true

  const profile = (process.env.RER_ENGINE_PROFILE || 'debug').trim()
  const binaryName = editorFrameBinaryName()
  const editorPath = app.isPackaged
    ? path.join(process.resourcesPath, 'engine', binaryName)
    : path.join(app.getAppPath(), 'src', 'main', 'Engine', 'target', profile, binaryName)

  const args = ['--project-type', currentProjectType === '2D' ? '2D' : '3D']
  if (currentGameStyle) args.push('--game-style', currentGameStyle)
  if (currentProjectFilePath) args.push('--save-path', currentProjectFilePath)
  const extractDir = currentProjectExtractDir ?? currentProjectSessionDir
  if (extractDir) args.push('--extract-dir', extractDir)
  args.push('--locale', currentLocale)
  args.push('--electron-host')

  editorFrameProcess = spawn(editorPath, args, {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env },
  })

  console.log(`[editor] ventana egui: ${editorPath}`)
  console.log(`[editor] args: ${args.join(' ')}`)

  // stdout: eventos del motor reenviados por el editor + mensajes del editor.
  editorFrameProcess.stdout?.on('data', (data: Buffer) => {
    const lines = data.toString('utf8').split('\n').filter(Boolean)
    for (const line of lines) {
      handleEngineLine(line)
    }
  })

  editorFrameProcess.stderr?.on('data', (data: Buffer) => {
    const lines = data.toString('utf8').split('\n').filter(Boolean)
    for (const line of lines) {
      if (isBenignEngineStderrLine(line)) continue
      console.log('[editor]', line)
    }
  })

  editorFrameProcess.on('close', (code) => {
    console.log(`[editor] proceso terminado con código ${code}`)
    editorFrameProcess = null

    if (editorFrameStopping) {
      editorFrameStopping = false
      return
    }

    if (code === 0) {
      sendEventToRenderer({ event: 'stopped', code } as EngineEvent)
      app.quit()
      return
    }

    // El editor Rust falló: no hay ventana de editor que devolver; informar y salir.
    const motorLabel = lastEngineBinary === 'rer_engine_3d' ? '3D' : '2D'
    const message = !engineReceivedReady
      ? gpuStartupErrorMessage()
      : `El motor ${motorLabel} se detuvo inesperadamente (código ${code}).`
    failEditorAndQuit(message)
  })

  editorFrameProcess.on('error', (err) => {
    console.error('[editor] no se pudo iniciar:', err.message)
    editorFrameProcess = null
    editorFrameLaunched = false
    failEditorAndQuit(`No se pudo iniciar el editor/motor: ${err.message}`)
  })
}

function stopEditorFrame(): void {
  editorFrameLaunched = false
  if (!editorFrameProcess) return

  editorFrameStopping = true
  const processToStop = editorFrameProcess
  editorFrameProcess = null

  // Cierre ordenado: el editor termina el motor en su Drop.
  if (!processToStop.killed && processToStop.stdin && !processToStop.stdin.destroyed) {
    processToStop.stdin.write('{"type":"exit"}\n')
  }
  setTimeout(() => {
    if (!processToStop.killed && processToStop.exitCode === null) {
      processToStop.kill()
    }
  }, 800)
}

/** Escribe un mensaje JSON al stdin del editor Rust. */
function sendToEditor(message: unknown): boolean {
  if (!editorFrameProcess?.stdin || editorFrameProcess.stdin.destroyed) {
    return false
  }
  const data = JSON.stringify(message) + '\n'
  editorFrameProcess.stdin.write(data, () => {})
  return true
}

function sendToEngine(cmd: EngineCommand): void {
  if (cmd.cmd === 'set_locale') {
    currentLocale = String((cmd as Record<string, unknown>)['locale'] ?? 'en').toLowerCase() === 'es' ? 'es' : 'en'
  }

  if (sendToEditor({ type: 'engine_command', command: cmd })) {
    return
  }
  if (cmd.cmd === 'set_locale') {
    updateAiAssistantOverlayConfig({ locale: currentLocale })
    console.log(`[i18n] set_locale con motor inactivo (overlay actualizado): ${currentLocale}`)
  }
}

function stopEngine(): void {
  // El editor es dueño del motor: al cerrarlo, su Drop termina el proceso.
  sendToEngine({ cmd: 'shutdown' })
  stopEditorFrame()
  engineReceivedReady = false
}

// ---------------------------------------------------------------------------
// Hot reload de assets: file watchers para PNG/sprites cargados externamente
// ---------------------------------------------------------------------------
const assetWatchers = new Map<string, fs.FSWatcher>()

function watchAsset(filePath: string): void {
  if (assetWatchers.has(filePath)) return
  try {
    const watcher = fs.watch(filePath, { persistent: false }, (eventType) => {
      if (eventType === 'change') {
        // Debounce: esperar 150 ms antes de recargar para evitar recargas dobles
        // mientras el programa externo termina de escribir el archivo.
        setTimeout(() => {
          sendToEngine({ cmd: 'reload_asset', path: filePath } as never)
        }, 150)
      }
    })
    watcher.on('error', () => {
      assetWatchers.delete(filePath)
    })
    assetWatchers.set(filePath, watcher)
  } catch {
    // Si el archivo no existe aún, ignorar
  }
}

function clearAssetWatchers(): void {
  for (const watcher of assetWatchers.values()) {
    try { watcher.close() } catch { /* ignorar */ }
  }
  assetWatchers.clear()
}

// ---------------------------------------------------------------------------
// IPC: renderer → motor y herramientas del editor
// ---------------------------------------------------------------------------
registerPluginIpc(() => mainWindow, sendToEngine)

ipcMain.handle('editor:get-locale', () => currentLocale)

registerEngineCmdIpc({
  getProjectType: () => currentProjectType,
  sendToEngine,
  setLocale: (locale) => {
    updateAiAssistantOverlayConfig({ locale })
  },
  watchPngAsset: watchAsset,
})

initAiAssistantOverlay(() => mainWindow)

ipcMain.handle(
  'ai-assistant:show',
  async (_event, config: AiAssistantOverlayConfig) => {
    await showAiAssistantOverlay(config ?? {})
  },
)

ipcMain.handle('ai-assistant:hide', () => {
  hideAiAssistantOverlay()
})

ipcMain.on('ai-assistant:set-layout', (_event, layout: string) => {
  const allowed = ['intro', 'thinking', 'input', 'answer', 'fab', 'panel'] as const
  if (allowed.includes(layout as (typeof allowed)[number])) {
    setAiAssistantOverlayLayout(layout as (typeof allowed)[number])
  }
})

ipcMain.on('ai-assistant:fab-drag-start', () => {
  beginAiAssistantFabDrag()
})

ipcMain.on('ai-assistant:fab-drag-end', () => {
  endAiAssistantFabDrag()
})

ipcMain.on('ai-assistant:ready', (event) => {
  resendAiAssistantConfig(event.sender.id)
})

// Diálogo para abrir imagen PNG como sprite (lo usan las modales)
ipcMain.handle('open-sprite-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Cargar sprite (PNG)',
    filters:    [{ name: 'Imágenes PNG', extensions: ['png'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para abrir imagen como fondo (lo usan las modales de UI)
ipcMain.handle('open-background-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:   'Cargar fondo del mundo',
    filters: [{ name: 'Imágenes', extensions: ['png', 'gif', 'jpg', 'jpeg', 'webp'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Convierte una imagen local a data URL para que el renderer no use file://
ipcMain.handle('get-image-data-url', async (_event, filePath: string): Promise<string | null> => {
  try {
    if (!filePath || !path.isAbsolute(filePath) || !fs.existsSync(filePath)) return null

    const ext = path.extname(filePath).toLowerCase()
    const mimeByExt: Record<string, string> = {
      '.png': 'image/png',
      '.jpg': 'image/jpeg',
      '.jpeg': 'image/jpeg',
      '.webp': 'image/webp',
      '.gif': 'image/gif',
    }
    const mime = mimeByExt[ext] ?? 'application/octet-stream'
    const bytes = fs.readFileSync(filePath)
    const base64 = bytes.toString('base64')
    return `data:${mime};base64,${base64}`
  } catch (error) {
    console.error('[ipc] get-image-data-url error:', error)
    return null
  }
})

// ---------------------------------------------------------------------------
// Modal Electron
// ---------------------------------------------------------------------------

ipcMain.handle('modal-electron:open', async (_event, payload: ModalElectronOpenRequest) => {
  await openModalElectronWindow(payload)
})

ipcMain.handle('modal-electron:close', () => {
  closeModalElectronWindow()
})

ipcMain.on('modal-electron:ready', (event) => {
  resendPendingRenderToModal(event.sender.id)
})

ipcMain.on('modal-electron:resize', (_event, contentHeight: number) => {
  if (typeof contentHeight === 'number' && Number.isFinite(contentHeight)) {
    applyModalElectronContentHeight(contentHeight)
  }
})

ipcMain.handle(
  'modal-electron:delegate',
  async (_event, request: ModalElectronDelegateRequest): Promise<{ blueprints?: unknown[] } | null> => {
    const win = mainWindow
    if (!win || win.isDestroyed()) return null
    return new Promise((resolve) => {
      const requestId = `${Date.now()}-${Math.random()}`
      const channel = `modal-electron:delegate-response-${requestId}`
      const timeout = setTimeout(() => {
        ipcMain.removeAllListeners(channel)
        resolve(null)
      }, 30_000)
      ipcMain.once(channel, (_replyEvent, result) => {
        clearTimeout(timeout)
        resolve(result ?? null)
      })
      win.webContents.send('modal-electron:delegate-request', { ...request, requestId })
    })
  },
)

ipcMain.on('modal-electron:parent-open', (_event, data: {
  parentHandlerId: string
  action: string
  payload?: Record<string, unknown>
}) => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('modal-electron:parent-open', data)
  }
})

ipcMain.on('modal-electron:patch', (_event, data: {
  handlerId: string
  playerUiEditorState?: unknown
  entityPropertiesState?: unknown
    socketConfigModalState?: unknown
    models?: unknown
}) => {
  sendPatchToModal(data)
})

ipcMain.handle(
  'modal-electron:player-ui-action',
  async (_event, req: { handlerId: string; action: unknown }) => {
    const win = mainWindow
    if (!win || win.isDestroyed()) return
    return new Promise<void>((resolve) => {
      const requestId = `${Date.now()}-${Math.random()}`
      const channel = `modal-electron:player-ui-action-done-${requestId}`
      const timeout = setTimeout(() => {
        ipcMain.removeAllListeners(channel)
        resolve()
      }, 30_000)
      ipcMain.once(channel, () => {
        clearTimeout(timeout)
        resolve()
      })
      win.webContents.send('modal-electron:player-ui-action-request', {
        ...req,
        requestId,
      })
    })
  },
)

ipcMain.handle(
  'modal-electron:entity-properties-action',
  async (_event, req: { handlerId: string; action: unknown }) => {
    const win = mainWindow
    if (!win || win.isDestroyed()) return
    return new Promise<void>((resolve) => {
      const requestId = `${Date.now()}-${Math.random()}`
      const channel = `modal-electron:entity-properties-action-done-${requestId}`
      const timeout = setTimeout(() => {
        ipcMain.removeAllListeners(channel)
        resolve()
      }, 30_000)
      ipcMain.once(channel, () => {
        clearTimeout(timeout)
        resolve()
      })
      win.webContents.send('modal-electron:entity-properties-action-request', {
        ...req,
        requestId,
      })
    })
  },
)

ipcMain.handle(
  'modal-electron:socket-config-modal-action',
  async (_event, req: { handlerId: string; action: unknown }) => {
    const win = mainWindow
    if (!win || win.isDestroyed()) return
    return new Promise<void>((resolve) => {
      const requestId = `${Date.now()}-${Math.random()}`
      const channel = `modal-electron:socket-config-modal-action-done-${requestId}`
      const timeout = setTimeout(() => {
        ipcMain.removeAllListeners(channel)
        resolve()
      }, 30_000)
      ipcMain.once(channel, () => {
        clearTimeout(timeout)
        resolve()
      })
      win.webContents.send('modal-electron:socket-config-modal-action-request', {
        ...req,
        requestId,
      })
    })
  },
)

ipcMain.handle(
  'modal-electron:player-ui-state',
  async (_event, req: { handlerId: string }) => {
    const win = mainWindow
    if (!win || win.isDestroyed()) return null
    return new Promise<unknown>((resolve) => {
      const requestId = `${Date.now()}-${Math.random()}`
      const channel = `modal-electron:player-ui-state-done-${requestId}`
      const timeout = setTimeout(() => {
        ipcMain.removeAllListeners(channel)
        resolve(null)
      }, 10_000)
      ipcMain.once(channel, (_replyEvent, result) => {
        clearTimeout(timeout)
        resolve(result ?? null)
      })
      win.webContents.send('modal-electron:player-ui-state-request', {
        ...req,
        requestId,
      })
    })
  },
)

ipcMain.on('modal-electron:result', (_event, data: ModalElectronResultPayload) => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('modal-electron:result', data)
  }
  closeModalElectronWindow()
})

// ---------------------------------------------------------------------------
// Helpers de lectura del .save
// ---------------------------------------------------------------------------

const SCRIPT_FILE_PREFIX = '@file:'
/** Entidad 2D (`path`) o 3D (`model`) en manifest. */
type SaveManifestEntity = Entity3D & Partial<Pick<SavedEntity, 'path' | 'kind' | 'control_bindings'>>

function getEntityAssetPath(entity: SaveManifestEntity): string | undefined {
  const pathField = entity.path?.trim()
  if (pathField) return pathField
  const modelField = entity.model?.trim()
  if (modelField) return modelField
  return undefined
}

function getBlueprintAssetPath(
  bp: NonNullable<ProjectSaveData['blueprints']>[number],
): string | undefined {
  const pathField = bp.path?.trim()
  if (pathField) return pathField
  const modelField = bp.model?.trim()
  if (modelField) return modelField
  return undefined
}

function mapEntityAssetPaths<T extends SaveManifestEntity>(
  entity: T,
  mapPath: (p: string | null | undefined) => string | null | undefined,
  skipAssetPath?: (p: string, entity: T) => boolean,
): T {
  const assetPath = getEntityAssetPath(entity)
  const next = { ...entity } as T & { path?: string; model?: string }
  if (assetPath && !skipAssetPath?.(assetPath, entity)) {
    const remapped = mapPath(assetPath) as string
    if (typeof next.path === 'string') next.path = remapped
    if (typeof next.model === 'string') next.model = remapped
  }
  return next as T
}

function mapEntityAnimations<T extends SaveManifestEntity>(
  entity: T,
  mapPath: (p: string | null | undefined) => string | null | undefined,
): T {
  if (!entity.animations?.length) return entity
  return {
    ...entity,
    animations: entity.animations.map((anim) => ({
      ...anim,
      audio_path: mapPath(anim.audio_path) as string | undefined,
      frames: anim.frames.map((f) => ({
        ...f,
        path: mapPath(f.path) as string,
      })),
    })),
  }
}

function forEachEntity(data: ProjectSaveData, cb: (entity: SaveManifestEntity) => void): void {
  const visit = (entity: Entity3D | null | undefined) => {
    if (entity) cb(entity)
  }

  if ((data.scenes?.length ?? 0) > 0) {
    for (const scene of data.scenes ?? []) {
      for (const entity of scene.entities) cb(entity)
      visit(scene.player)
    }
    return
  }

  for (const entity of data.entities) cb(entity)
  visit(data.player)
}

function countSavedEntities(data: ProjectSaveData): number {
  let count = 0
  forEachEntity(data, () => { count += 1 })
  return count
}

function countActiveSceneEntities(data: ProjectSaveData): number {
  const hasScenes = (data.scenes?.length ?? 0) > 0
  if (!hasScenes) return data.entities?.length ?? 0
  const active =
    data.scenes?.find((s) => s.id === data.activeSceneId) ?? data.scenes?.[0]
  return active?.entities?.length ?? 0
}

function countActiveSceneSprites(data: ProjectSaveData): number {
  const hasScenes = (data.scenes?.length ?? 0) > 0
  if (!hasScenes) return data.sprites?.length ?? 0
  const active =
    data.scenes?.find((s) => s.id === data.activeSceneId) ?? data.scenes?.[0]
  return active?.sprites?.length ?? 0
}

/** Recursos de la librería (accordion Recursos) declarados en manifest. */
function countManifestLibraryRefs(data: ProjectSaveData): {
  sounds: number
  fonts: number
  backgrounds: number
  hudImages: number
  sprites: number
  models: number
} {
  const importedModels =
    data.type === '3D'
      ? (data.resources?.models?.length ?? 0)
      : 0
  return {
    sounds: data.sounds?.length ?? 0,
    fonts: data.fonts?.length ?? 0,
    backgrounds: data.backgrounds?.length ?? 0,
    hudImages: data.hudImages?.length ?? 0,
    sprites: countActiveSceneSprites(data),
    models: importedModels,
  }
}

function formatLibraryResourcesInLog(counts: {
  sounds: number
  fonts: number
  backgrounds: number
  hudImages: number
}): string {
  return `fondos: ${counts.backgrounds}, sonidos: ${counts.sounds}, fuentes: ${counts.fonts}, imágenes HUD: ${counts.hudImages}`
}

function formatEntityKindBreakdown(data: ProjectSaveData): string {
  const byKind = new Map<string, number>()
  forEachEntity(data, (entity) => {
    const kind =
      entity.category
      ?? (entity as { kind?: string }).kind
      ?? 'unknown'
    byKind.set(kind, (byKind.get(kind) ?? 0) + 1)
  })
  if (byKind.size === 0) return ''
  const parts = [...byKind.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([kind, n]) => `${kind}: ${n}`)
  return ` [${parts.join(', ')}]`
}

const MODEL_SOURCE_EXTS = new Set(['.glb', '.gltf', '.fbx'])

/** En 3D, `entity.model` puede ser `model_id` (sin ruta); no resolver contra extract_dir. */
function isManifestModelIdRef(p: string, modelId?: string): boolean {
  if (entityPathMarker(p)) return true
  if (modelId && p === modelId) return true
  if (path.isAbsolute(p)) return false
  if (!p.includes('/') && !p.includes('\\')) {
    const ext = path.extname(p).toLowerCase()
    return ext === '' || (!MODEL_SOURCE_EXTS.has(ext) && ext !== '.rerasset')
  }
  return false
}

function ensureProjectSessionDir(): string {
  if (currentProjectSessionDir && fs.existsSync(currentProjectSessionDir)) {
    return currentProjectSessionDir
  }
  const tempRoot = app.getPath('temp')
  const sessionDir = fs.mkdtempSync(path.join(tempRoot, 'rer-project-session-'))
  fs.mkdirSync(path.join(sessionDir, 'imported', 'models'), { recursive: true })
  fs.mkdirSync(path.join(sessionDir, 'source', 'models'), { recursive: true })
  extractedProjectDirs.add(sessionDir)
  currentProjectSessionDir = sessionDir
  console.log('[editor] workspace de sesión 3D:', sessionDir)
  return sessionDir
}

function resolveScriptSource(source: string | undefined, extractedDir: string): string {
  if (!source) return ''
  if (!source.startsWith(SCRIPT_FILE_PREFIX)) {
    console.warn('[editor] Script con formato inválido en save (se esperaba @file:scripting/...):', source)
    return ''
  }

  const relPath = source.slice(SCRIPT_FILE_PREFIX.length)
  const normalized = relPath.split('/').join(path.sep)
  const absPath = path.join(extractedDir, normalized)
  if (!fs.existsSync(absPath)) {
    console.warn(`[editor] Script referenciado no encontrado en save: ${relPath}`)
    return ''
  }
  return fs.readFileSync(absPath, 'utf8')
}

function resolveLoadedPaths(data: ProjectSaveData, extractedDir: string): ProjectSaveData {
  const resolve = (p: string | null | undefined): string | null | undefined => {
    if (!p) return p
    const marker = entityPathMarker(p)
    if (marker) return marker
    if (path.isAbsolute(p)) {
      console.error('[editor] manifest path must be relative inside .save:', p)
    }
    const normalized = p.split('/').join(path.sep)
    return path.join(extractedDir, normalized)
  }

  const hasScenes = (data.scenes?.length ?? 0) > 0

  const resolveControls = (controls: SavedControls | undefined): SavedControls | undefined => {
    if (!controls) return undefined
    return {
      keyboard_mouse: Object.fromEntries(
        Object.entries(controls.keyboard_mouse).map(([key, script]) => [
          key,
          { ...script, source: resolveScriptSource(script.source, extractedDir) },
        ]),
      ),
      gamepad: Object.fromEntries(
        Object.entries(controls.gamepad).map(([key, script]) => [
          key,
          { ...script, source: resolveScriptSource(script.source, extractedDir) },
        ]),
      ),
    }
  }

  const skipModelIdRef = (p: string, ent: SaveManifestEntity) =>
    isManifestModelIdRef(p, ent.model_id)

  const mapSaveEntity = (e: SaveManifestEntity): SaveManifestEntity => {
    const controls =
      e.controls ?? e.control_bindings
    const withPaths = mapEntityAnimations(
      mapEntityAssetPaths(e, resolve, skipModelIdRef),
      resolve,
    )
    return {
      ...withPaths,
      scripts: withPaths.scripts?.map((script: SavedScript) => ({
        ...script,
        source: resolveScriptSource(script.source, extractedDir),
      })),
      animations: withPaths.animations?.map((anim) => ({
        ...anim,
        scripts: anim.scripts?.map((script: SavedScript) => ({
          ...script,
          source: resolveScriptSource(script.source, extractedDir),
        })),
      })),
      ...(controls ? { controls: resolveControls(controls) } : {}),
      ...(e.control_bindings ? { control_bindings: resolveControls(e.control_bindings) } : {}),
    }
  }

  return {
    ...data,
    world: hasScenes ? undefined : data.world,
    backgroundPath: hasScenes ? null : (resolve(data.backgroundPath) as string | null),
    sprites: hasScenes
      ? []
      : data.sprites?.map((s) => ({
          name: s.name,
          path: resolve(s.path) as string,
        })),
    sounds: data.sounds?.map((s) => ({
      name: s.name,
      path: resolve(s.path) as string,
    })),
    fonts: data.fonts?.map((f) => ({
      name: f.name,
      path: resolve(f.path) as string,
    })),
    backgrounds: data.backgrounds?.map((b) => ({
      name: b.name,
      path: resolve(b.path) as string,
    })),
    hudImages: data.hudImages?.map((img) => ({
      name: img.name,
      path: resolve(img.path) as string,
    })),
    playerUiTextBoxes: data.playerUiTextBoxes?.map((box) => ({
      ...box,
      font_path: resolve(box.font_path) as string,
    })),
    playerUiButtons: data.playerUiButtons?.map((btn) => ({
      ...btn,
      font_path: resolve(btn.font_path) as string,
      texture_path: resolve(btn.texture_path) as string | undefined,
    })),
    playerUiImages: data.playerUiImages?.map((img) => ({
      ...img,
      image_path: resolve(img.image_path) as string,
    })),
    playerUiObjects: data.playerUiObjects?.map((obj) => ({
      ...obj,
      texture_path: resolve(obj.texture_path) as string | undefined,
    })),
    entities: hasScenes ? [] : data.entities.map(mapSaveEntity),
    player: hasScenes ? null : data.player ? mapSaveEntity(data.player) : data.player,
    config_camera: hasScenes ? null : data.config_camera,
    config_editor_camera: hasScenes ? null : data.config_editor_camera,
    camera2d: hasScenes ? null : data.camera2d,
    scenes: data.scenes?.map((scene) => ({
      ...scene,
      backgroundPath: resolve(scene.backgroundPath) as string | null,
      models: [],
      sprites: scene.sprites?.map((s) => ({
        name: s.name,
        path: resolve(s.path) as string,
      })),
      entities: scene.entities.map(mapSaveEntity),
      player: scene.player ? mapSaveEntity(scene.player) : scene.player,
    })),
    blueprints: data.blueprints?.map((bp) => {
      const assetPath = getBlueprintAssetPath(bp)
      const remappedAsset = assetPath ? (resolve(assetPath) as string) : undefined
      return {
        ...bp,
        ...(bp.model !== undefined && remappedAsset ? { model: remappedAsset } : {}),
        ...(bp.path !== undefined && remappedAsset ? { path: remappedAsset } : {}),
        scripts: bp.scripts?.map((script: SavedScript) => ({
          ...script,
          source: resolveScriptSource(script.source, extractedDir),
        })),
        animations: bp.animations?.map((anim) => ({
          ...anim,
          audio_path: resolve(anim.audio_path) as string | undefined,
          frames: anim.frames.map((f) => ({
            ...f,
            path: resolve(f.path) as string,
          })),
          scripts: anim.scripts?.map((script: SavedScript) => ({
            ...script,
            source: resolveScriptSource(script.source, extractedDir),
          })),
        })),
      }
    }),
  }
}

/** Extrae un `.save` a un directorio temporal y registra la carpeta para su vida útil. */
function extractSaveArchive(saveFilePath: string): string | null {
  const tempRoot = app.getPath('temp')
  const extractDir = fs.mkdtempSync(path.join(tempRoot, 'rer-save-open-'))
  try {
    const zip = new AdmZip(saveFilePath)
    zip.extractAllTo(extractDir, true)

    const manifestPath = path.join(extractDir, 'manifest.json')
    if (!fs.existsSync(manifestPath)) {
      console.error('[editor] El archivo .save no contiene manifest.json')
      fs.rmSync(extractDir, { recursive: true, force: true })
      return null
    }

    extractedProjectDirs.add(extractDir)
    return extractDir
  } catch (err) {
    console.error('[save-load] error al extraer archivo .save:', err)
    fs.rmSync(extractDir, { recursive: true, force: true })
    return null
  }
}

function readManifestMeta(extractDir: string): { type: ProjectType; gameStyle: GameStyle } | null {
  try {
    const manifestPath = path.join(extractDir, 'manifest.json')
    const raw = fs.readFileSync(manifestPath, 'utf8')
    const parsed = JSON.parse(raw) as unknown
    if (!(parsed !== null && typeof parsed === 'object' && 'type' in parsed && 'gameStyle' in parsed)) {
      return null
    }
    const type = (parsed as { type: unknown }).type
    const gameStyle = (parsed as { gameStyle: unknown }).gameStyle
    if (type !== '2D' && type !== '3D') return null
    if (typeof gameStyle !== 'string' || !gameStyle.trim()) return null
    return { type, gameStyle: gameStyle as GameStyle }
  } catch (err) {
    console.error('[editor] Error al leer manifest.json:', err)
    return null
  }
}

/**
 * Lee manifest.json desde una carpeta ya extraída y devuelve el proyecto con paths absolutos.
 */
function loadProjectFromExtractDir(extractDir: string): ProjectSaveData | null {
  try {
    const manifestPath = path.join(extractDir, 'manifest.json')
    if (!fs.existsSync(manifestPath)) return null
    const raw = fs.readFileSync(manifestPath, 'utf8')
    const parsed = JSON.parse(raw) as unknown
    if (!(parsed !== null && typeof parsed === 'object' && 'type' in parsed && 'gameStyle' in parsed)) {
      return null
    }
    return resolveLoadedPaths(parsed as ProjectSaveData, extractDir)
  } catch (err) {
    console.error('[editor] Error al leer proyecto desde carpeta extraída:', err)
    return null
  }
}

// ---------------------------------------------------------------------------
// IPC: guardar / cargar proyecto
// ---------------------------------------------------------------------------

// Diálogo para abrir un proyecto existente (.save)
ipcMain.handle('open-project-dialog', async (): Promise<OpenProjectResult | null> => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Abrir proyecto',
    filters:    [{ name: 'Proyecto RER Save', extensions: ['save'] }],
    properties: ['openFile'],
  })
  if (result.canceled || !result.filePaths[0]) return null
  const filePath = result.filePaths[0]
  const extractDir = extractSaveArchive(filePath)
  if (!extractDir) return null
  const meta = readManifestMeta(extractDir)
  if (!meta) {
    fs.rmSync(extractDir, { recursive: true, force: true })
    extractedProjectDirs.delete(extractDir)
    return null
  }

  currentProjectFilePath = filePath
  currentProjectType = meta.type
  currentGameStyle = meta.gameStyle
  currentProjectExtractDir = extractDir
  currentProjectSessionDir = null
  clearAssetWatchers()

  const loaded = loadProjectFromExtractDir(extractDir)
  if (loaded) {
    const entityCount =
      loaded.type === '2D'
        ? countActiveSceneEntities(loaded)
        : countSavedEntities(loaded)
    const entityKindSuffix = formatEntityKindBreakdown(loaded)
    const lib = countManifestLibraryRefs(loaded)
    const libraryLog = formatLibraryResourcesInLog(lib)
    if (loaded.type === '3D') {
      console.log(
        `[editor] Proyecto cargado `
        + `(entidades: ${entityCount}${entityKindSuffix}, modelos: ${lib.models}, ${libraryLog})`,
      )
    } else {
      const sceneCount = loaded.scenes?.length ?? 1
      console.log(
        `[editor] Proyecto 2D cargado `
        + `(escena activa: ${loaded.activeSceneId ?? 1}, `
        + `${sceneCount} escena/s, entidades activas: ${entityCount}${entityKindSuffix}, `
        + `sprites escena: ${lib.sprites}, ${libraryLog})`,
      )
    }
  }

  return { filePath, extractDir, project: meta }
})

// Diálogo para elegir ruta del .save (sin empaquetar aún).
// El renderer responde al editor con el estado de proyecto para guardar en Rust.
ipcMain.on('project-state:response', (_event, id: number, state: unknown) => {
  sendToEditor({ type: 'project_state', id, state })
})

// El botón Guardar del renderer delega el empaquetado en el editor Rust.
ipcMain.on('project-state:save-request', () => {
  sendToEditor({ type: 'save_request' })
})

ipcMain.handle('read-project-manifest', (): ProjectSaveData | null => {
  if (!currentProjectExtractDir) return null
  return loadProjectFromExtractDir(currentProjectExtractDir)
})

function isEngineStartPayload(v: unknown): v is EngineStartPayload {
  if (typeof v !== 'object' || v === null) return false
  const o = v as Record<string, unknown>
  return (
    (o.projectType === '2D' || o.projectType === '3D')
    && ('mode' in o)
    && ('save_path' in o)
  )
}

// El renderer envía tipo de proyecto, modo y ruta del `.save` antes de arrancar el motor.
ipcMain.on('set-game-style', (_event, arg: unknown) => {
  if (!isEngineStartPayload(arg)) return
  currentProjectType = arg.projectType
  currentGameStyle = arg.mode === false ? null : arg.mode
  currentProjectFilePath =
    typeof arg.save_path === 'string' && arg.save_path.trim().length > 0
      ? arg.save_path.trim()
      : null
  const extractDir = arg.extract_dir
  if (typeof extractDir === 'string' && extractDir.trim().length > 0) {
    currentProjectExtractDir = extractDir.trim()
    currentProjectSessionDir = null
  } else if (arg.projectType === '3D' && !currentProjectFilePath) {
    currentProjectExtractDir = null
    ensureProjectSessionDir()
  } else {
    currentProjectExtractDir = null
    currentProjectSessionDir = null
  }
  hideMainWindowForEditor()
  setImmediate(() => startEngineSession())
})

let exitDialogOpen = false
let exitModalActionHandled = false

/** Abre la modal React de salida en Electron (por encima del editor nativo). */
function requestExitModal(engineRect?: unknown): void {
  if (exitDialogOpen) return
  exitDialogOpen = true
  exitModalActionHandled = false
  setModalElectronAnchorRect(engineWindowRectToDip(engineRect))
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('legacy-window:open', {
      kind: 'exit_confirm',
      payload: {},
    })
    return
  }
  setModalElectronAnchorRect(null)
  exitDialogOpen = false
  sendToEditor({ type: 'exit_cancelled' })
}

ipcMain.on('ui-screens:sync', (_event, screens: unknown) => {
  if (!screens || typeof screens !== 'object') return
  sendToEditor({ type: 'ui_screens_sync', screens })
})

ipcMain.on('quick-build:state', (_event, state: unknown) => {
  if (!state || typeof state !== 'object') return
  sendToEditor({ type: 'quick_build_state', state })
})

ipcMain.on('editor:exit-action', (_event, action: unknown) => {
  if (!exitDialogOpen || exitModalActionHandled) return
  const value: 'save' | 'discard' | 'cancel' =
    action === 'save' || action === 'discard' || action === 'cancel' ? action : 'cancel'
  exitModalActionHandled = true
  exitDialogOpen = false
  sendToEditor({
    type:
      value === 'save'
        ? 'exit_after_save'
        : value === 'discard'
          ? 'exit'
          : 'exit_cancelled',
  })
  closeModalElectronWindow()
})

// ---------------------------------------------------------------------------
// Ciclo de vida de la app
// ---------------------------------------------------------------------------
function cleanupExtractedProjectDirs(): void {
  for (const dir of extractedProjectDirs) {
    try {
      fs.rmSync(dir, { recursive: true, force: true })
    } catch (err) {
      console.error('[editor] No se pudo limpiar carpeta temporal:', err)
    }
  }
  extractedProjectDirs.clear()
}

app.whenReady().then(() => {
  // Vite HMR needs 'unsafe-eval' in development; Electron always warns about that.
  // Silence the console warning only when not packaged (packaged CSP has no unsafe-eval).
  if (!app.isPackaged) {
    process.env.ELECTRON_DISABLE_SECURITY_WARNINGS = 'true'
  }

  // Always set CSP. Packaged: strict. Dev: allow Vite HMR / React Fast Refresh.
  const CSP = app.isPackaged
    ? [
        "default-src 'self'",
        "script-src 'self'",
        "style-src 'self' 'unsafe-inline'",
        "img-src 'self' data: blob: file:",
        "media-src 'self' file: blob:",
        "connect-src 'self'",
        "font-src 'self' data:",
        "worker-src 'self' blob:",
        "child-src 'self' blob:",
      ].join('; ')
    : [
        "default-src 'self'",
        "script-src 'self' 'unsafe-eval' 'unsafe-inline'",
        "style-src 'self' 'unsafe-inline'",
        "img-src 'self' data: blob: file:",
        "media-src 'self' file: blob:",
        "connect-src 'self' ws: wss: http://localhost:* http://127.0.0.1:*",
        "font-src 'self' data:",
        "worker-src 'self' blob:",
        "child-src 'self' blob:",
      ].join('; ')

  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': [CSP],
      },
    })
  })

  createMainWindow()
  initModalElectron(() => mainWindow)
  void autoStartEnabledPlugins()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow()
    }
  })
})

let opencodeShutdownInProgress = false

app.on('before-quit', (event) => {
  if (opencodeShutdownInProgress) return
  event.preventDefault()
  opencodeShutdownInProgress = true
  stopEditorFrame()
  void stopOpenCodeServer().finally(() => {
    app.exit(0)
  })
})

app.on('window-all-closed', () => {
  destroyAiAssistantOverlay()
  destroyModalElectronWindow()
  stopEngine()
  cleanupExtractedProjectDirs()
  clearAssetWatchers()
  app.quit()
})
