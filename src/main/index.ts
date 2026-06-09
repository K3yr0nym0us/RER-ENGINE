import { app, BrowserWindow, ipcMain, dialog, Menu, session, screen as electronScreen } from 'electron';
import { spawn, ChildProcess } from 'child_process';
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
  AppResourceUsage,
} from '../shared-types/types';
import { entityPathMarker } from '../shared-types/types';
import {
  getGpuMetricsPlatform,
  isElectronGpuMetricsSupported,
  queryElectronAppGpuPercent,
} from './gpuProcessUsage';
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
} from './modalElectronWindow';
import { resolveAppWindowIcon } from './appWindowIcon';
import type {
  ModalElectronDelegateRequest,
  ModalElectronOpenRequest,
  ModalElectronResultPayload,
} from '../shared-types/types';

// Sin GPU hardware disponible: deshabilitar el proceso GPU de Chromium
// para evitar spam de viz_main_impl / command_buffer_proxy_impl
app.commandLine.appendSwitch('disable-gpu');
app.commandLine.appendSwitch('disable-software-rasterizer');

// En Linux forzar el backend X11 de Chromium/GTK para alinear el viewport
// overlay del motor (coordenadas de pantalla). Las vars deben establecerse
// antes de que las librerías nativas (libwayland, GTK, libGL) se inicialicen.
if (process.platform === 'linux') {
  app.commandLine.appendSwitch('ozone-platform-hint', 'x11');
  process.env['WAYLAND_DISPLAY']           = '';
  process.env['GDK_BACKEND']              = 'x11';
  process.env['__NV_PRIME_RENDER_OFFLOAD'] = '1';
  process.env['__GLX_VENDOR_LIBRARY_NAME'] = 'nvidia';
}

// ---------------------------------------------------------------------------
// Variables de módulo
// ---------------------------------------------------------------------------
let mainWindow: BrowserWindow | null = null
let engineProcess: ChildProcess | null = null
let currentLocale: 'en' | 'es' = 'en'
let currentGameStyle: GameStyle | null = null
/** Cuando `gameStyle` es null (p. ej. selector 3D), decide 2D vs 3D hasta elegir estilo. */
let currentProjectType: ProjectType | null = null

// Buffer de eventos que llegaron antes de que el renderer estuviera listo
let rendererReady = false
const eventBuffer: EngineEvent[] = []

// Path del proyecto abierto/guardado actualmente.
let currentProjectFilePath: string | null = null
/** Carpeta extraída del `.save` abierto (2D); el motor lee manifest + assets desde aquí. */
let currentProjectExtractDir: string | null = null

// Directorios temporales con contenido extraído de .save que deben vivir
// mientras el proyecto está abierto para que el motor lea rutas absolutas.
const extractedProjectDirs = new Set<string>()

// Ventana secundaria del editor de scripts Rhai
// (eliminada — el editor ahora vive en un modal de Bootstrap dentro del renderer)

// Últimos bounds efectivos del motor (para restaurarlo tras ocultarlo)
let lastEffectiveBounds: ViewportBounds | null = null

/** true tras recibir `ready` del proceso motor de la sesión actual. */
let engineReceivedReady = false

/** Evita reenviar `set_scene` 3D si el motor emite más de un `ready` (p. ej. tras `setup_empty_3d`). */
let engine3dStartupSceneSent = false

/** Binario base del motor en la sesión actual (`rer_engine_2d` / `rer_engine_3d`). */
let lastEngineBinary = 'rer_engine_2d'

const ENGINE_GPU_LABEL = 'Vulkan'

function gpuStartupErrorMessage(): string {
  return (
    'No se pudo iniciar el motor gráfico con Vulkan. Instala o actualiza los controladores de video. ' +
    'En WSL2: usa WSLg, instala mesa-vulkan-drivers (o drivers NVIDIA para WSL) y comprueba con vulkaninfo. ' +
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
function notifyEngineGpuStartupError(): void {
  if (engineReceivedReady) return
  sendEventToRenderer({ event: 'error', message: gpuStartupErrorMessage() } as EngineEvent)
}

function sendEventToRenderer(event: EngineEvent): void {
  if (rendererReady && mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('engine:event', event)
  } else {
    eventBuffer.push(event)
  }
}

function focusMainEditorWindow(): void {
  if (!mainWindow || mainWindow.isDestroyed()) return
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
    },
  })

  // Abrir DevTools automáticamente en desarrollo
  if (process.env.NODE_ENV === 'development' || !app.isPackaged) {
    mainWindow.webContents.openDevTools()
  }

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
    mainWindow = null
  })

  const syncViewportAndModalOnMainWindowChange = (): void => {
    if (process.platform === 'linux') {
      mainWindow?.webContents.send('request-viewport-bounds')
    }
    repositionViewportCornerModalIfOpen()
  }

  // Linux: respaldo IPC al mover (el tracker X11 escucha ConfigureNotify).
  // Windows: el motor usa WinEventHook; la modal de propiedades se recoloca aquí.
  mainWindow.on('move', syncViewportAndModalOnMainWindowChange)
  mainWindow.on('resize', syncViewportAndModalOnMainWindowChange)

  // Clic en el viewport winit: la ventana principal pierde foco; mantener la modal encima del motor.
  mainWindow.on('blur', () => {
    restackModalElectronIfOpen()
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
// Extraer XID nativo de la ventana principal (Linux X11)
// ---------------------------------------------------------------------------
function getMainWindowXID(): number {
  if (!mainWindow) return 0
  const handle = mainWindow.getNativeWindowHandle()
  // En Linux X11, el handle es el XID almacenado como uint32 little-endian
  return handle.readUInt32LE(0)
}

// ---------------------------------------------------------------------------
// Extraer HWND nativo de la ventana principal (Windows)
// ---------------------------------------------------------------------------
function getMainWindowHWND(): string {
  if (!mainWindow) return '0'
  const handle = mainWindow.getNativeWindowHandle()
  // En Windows 64-bit, HWND es un puntero de 8 bytes (little-endian)
  if (handle.length >= 8) {
    return handle.readBigUInt64LE(0).toString()
  }
  // Fallback 32-bit (improbable en la práctica)
  return handle.readUInt32LE(0).toString()
}

// ---------------------------------------------------------------------------
// Proceso del motor Rust
// ---------------------------------------------------------------------------
interface ViewportBounds {
  x:      number
  y:      number
  width:  number
  height: number
  // Offsets físicos del EngineView dentro del área de contenido de Electron.
  // Solo se usan en Windows: el position-tracker Rust los usa como offset
  // directo (evita conversión DPI de getContentBounds() que puede ser incorrecta).
  rel_x?: number
  rel_y?: number
}

function startEngine(embed?: ViewportBounds): void {
  engineReceivedReady = false
  engine3dStartupSceneSent = false

  // Seleccionar binario según el tipo de proyecto (2D / 3D)
  let baseBinaryName = 'rer_engine_shared'
  if (currentProjectType === '3D') {
    baseBinaryName = 'rer_engine_3d'
  } else if (currentProjectType === '2D') {
    baseBinaryName = 'rer_engine_2d'
  }

  lastEngineBinary = baseBinaryName

  const binaryName = process.platform === 'win32' ? `${baseBinaryName}.exe` : baseBinaryName
  const enginePath = app.isPackaged
    ? path.join(process.resourcesPath, 'engine', binaryName)
    : path.join(app.getAppPath(), 'src', 'main', 'Engine', 'target', (process.env.RER_ENGINE_PROFILE || 'debug').trim(), binaryName)

  // Modo overlay: ventana nativa separada alineada al hueco del editor.
  let engineArgs: string[] = []
  if (embed && (process.platform === 'linux' || process.platform === 'win32')) {
    const x      = Math.round(embed.x)
    const y      = Math.round(embed.y)
    const width  = Math.max(1, Math.round(embed.width))
    const height = Math.max(1, Math.round(embed.height))
    const relX   = Math.max(0, Math.round(embed.rel_x ?? 0))
    const relY   = Math.max(0, Math.round(embed.rel_y ?? 0))
    if (process.platform === 'linux') {
      const xid = getMainWindowXID()
      if (xid !== 0) {
        engineArgs = ['--overlay', String(xid), String(x), String(y), String(width), String(height), String(relX), String(relY)]
        console.log(`[engine] modo overlay Linux — xid=${xid} pos=(${x},${y}) size=${width}x${height} offset=(${relX},${relY})`)
      }
    } else {
      const hwnd = getMainWindowHWND()
      engineArgs = ['--overlay', hwnd, String(x), String(y), String(width), String(height), String(relX), String(relY)]
      console.log(`[engine] modo overlay Windows — hwnd=${hwnd} pos=(${x},${y}) size=${width}x${height} offset=(${relX},${relY})`)
    }
  }

  const linuxEnv = process.platform === 'linux'
    ? {
        WAYLAND_DISPLAY: '',
        GDK_BACKEND:     'x11',
        ...(process.env.PULSE_SERVER ? { PULSE_SERVER: process.env.PULSE_SERVER } : {}),
      }
    : {}

  console.log(`[engine] binario=${baseBinaryName} GPU esperada=${ENGINE_GPU_LABEL}`)

  const engineEnv: NodeJS.ProcessEnv = { ...process.env, ...linuxEnv }
  delete engineEnv.RER_GPU_BACKEND

  if (currentProjectFilePath) {
    engineEnv.RER_PROJECT_SAVE_PATH = currentProjectFilePath
  } else {
    delete engineEnv.RER_PROJECT_SAVE_PATH
  }

  // 3D: escena vacía al arrancar si vamos a cargar un `.save` (ruta o carpeta extraída).
  if (baseBinaryName === 'rer_engine_3d') {
    if (currentProjectExtractDir || currentProjectFilePath) {
      engineEnv.RER_3D_START_FROM_SAVE = '1'
    } else {
      delete engineEnv.RER_3D_START_FROM_SAVE
    }
  }

  if (currentProjectExtractDir) {
    engineEnv.RER_PROJECT_EXTRACT_DIR = currentProjectExtractDir
  } else {
    delete engineEnv.RER_PROJECT_EXTRACT_DIR
  }

  engineProcess = spawn(enginePath, engineArgs, {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: engineEnv,
  })

  // stdout → eventos para el renderer
  engineProcess.stdout?.on('data', (data: Buffer) => {
    const lines = data.toString('utf8').split('\n').filter(Boolean)
    for (const line of lines) {
      try {
        const event = JSON.parse(line) as EngineEvent
        if (event.event === 'ready') {
          engineReceivedReady = true
          sendEngine2dStartupScene()
          sendEngine3dStartupScene()
        }
        if (event.event === 'autosave_tick') {
          if (currentProjectFilePath && mainWindow && !mainWindow.isDestroyed()) {
            mainWindow.webContents.send('autosave:request', currentProjectFilePath)
          }
        }
        // Tras ready: omitir errores GPU genéricos; dejar pasar errores de runtime (quick_build).
        if (event.event === 'error' && engineReceivedReady) {
          const msg = String((event as { message?: string }).message ?? '')
          if (!msg.includes('[quick_build]')) {
            continue
          }
        }
        if (
          (event.event === 'collider_created' || event.event === 'execution_area_created')
          && (event as { position?: unknown }).position != null
        ) {
          focusMainEditorWindow()
        }
        sendEventToRenderer(event)
      } catch {
        console.log('[engine stdout]', line)
      }
    }
  })

  // stderr: solo diagnóstico; no dispara error en UI. Omitir ruido benigno del loader Vulkan.
  engineProcess.stderr?.on('data', (data: Buffer) => {
    const lines = data.toString('utf8').split('\n').filter(Boolean)
    for (const line of lines) {
      if (isBenignEngineStderrLine(line)) continue
      if (/\[rer_engine_(2d|3d)::/.test(line)) {
        console.log(line.trim())
      } else {
        console.error('[engine stderr]', line)
      }
    }
  })

  engineProcess.on('close', (code) => {
    console.log(`[engine] proceso terminado con código ${code}`)
    if (code !== 0 && code !== null && !engineReceivedReady) {
      notifyEngineGpuStartupError()
    }
    sendEventToRenderer({ event: 'stopped', code } as EngineEvent)
    engineProcess = null
  })

  engineProcess.on('error', (err) => {
    console.error('[engine] no se pudo iniciar:', err.message)
    sendEventToRenderer({
      event: 'error',
      message: `No se pudo iniciar el motor: ${err.message}`,
    } as EngineEvent)
  })
}

function sendToEngine(cmd: EngineCommand): void {
  if (engineProcess?.stdin && !engineProcess.stdin.destroyed) {
    const data = JSON.stringify(cmd) + '\n'
    engineProcess.stdin.write(data, () => {})
  } else if (cmd.cmd === 'set_locale') {
    const locale = String((cmd as Record<string, unknown>)['locale'] ?? 'en')
    console.log(`[i18n] set_locale recibido en main con motor inactivo (omitido): ${locale}`)
  }
}

/** Arranque 2D: escena + carpeta extraída del proyecto (vacío si proyecto nuevo). */
function sendEngine2dStartupScene(): void {
  if (lastEngineBinary !== 'rer_engine_2d') return
  const extractDir = currentProjectExtractDir ?? ''
  sendToEngine({
    cmd: 'set_scene',
    scene: '2D',
    save_path: extractDir,
  })
  console.log(`[engine] 2D set_scene enviado (extract_dir=${extractDir || '(nuevo)'})`)
}

/** Arranque 3D: escena + carpeta extraída del proyecto (vacío si proyecto nuevo). */
function sendEngine3dStartupScene(): void {
  if (lastEngineBinary !== 'rer_engine_3d') return
  const extractDir = currentProjectExtractDir ?? ''
  if (!extractDir) return
  if (engine3dStartupSceneSent) return
  engine3dStartupSceneSent = true
  const scene = currentGameStyle ?? 'first-person'
  sendToEngine({
    cmd: 'set_scene',
    scene,
    save_path: extractDir,
  })
  console.log(`[engine] 3D set_scene enviado (extract_dir=${extractDir})`)
}

function stopEngine(): void {
  if (engineProcess) {
    sendToEngine({ cmd: 'shutdown' })
    // Forzar kill tras 2 s si no cerró limpiamente
    setTimeout(() => {
      if (engineProcess && !engineProcess.killed) {
        engineProcess.kill()
      }
    }, 2000)
  }
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
// Uso de CPU/GPU de procesos Electron (caché; sin bloquear el hilo principal)
// ---------------------------------------------------------------------------
const ELECTRON_RESOURCE_SAMPLE_MS = 2000

let cachedElectronResourceUsage: AppResourceUsage = {
  electronCpuPercent: 0,
  electronGpuPercent: null,
  gpuMetricsPlatform: getGpuMetricsPlatform(),
  electronGpuMetricsSupported: isElectronGpuMetricsSupported(),
}

async function sampleElectronResourceUsage(): Promise<void> {
  let electronCpuPercent = 0

  for (const metric of app.getAppMetrics()) {
    electronCpuPercent += metric.cpu?.percentCPUUsage ?? 0
  }

  const electronGpuPercent = await queryElectronAppGpuPercent()

  cachedElectronResourceUsage = {
    electronCpuPercent,
    electronGpuPercent,
    gpuMetricsPlatform: getGpuMetricsPlatform(),
    electronGpuMetricsSupported: isElectronGpuMetricsSupported(),
  }
}

function startElectronResourceSampling(): void {
  void sampleElectronResourceUsage()
  setInterval(() => void sampleElectronResourceUsage(), ELECTRON_RESOURCE_SAMPLE_MS)
}

// ---------------------------------------------------------------------------
// IPC: renderer → motor y herramientas del editor
// ---------------------------------------------------------------------------
ipcMain.handle('get-app-resource-usage', (): AppResourceUsage => cachedElectronResourceUsage)

ipcMain.on('engine:cmd', (_event, cmd: EngineCommand) => {
  if (cmd.cmd === 'set_locale') {
    const next = String((cmd as Record<string, unknown>)['locale'] ?? 'en').toLowerCase() === 'es' ? 'es' : 'en'
    currentLocale = next
    console.log(`[i18n] IPC renderer -> main set_locale: ${currentLocale}`)
  }

  // Registrar file watcher para assets cargados por path
  const c = cmd as Record<string, unknown>
  if (
    typeof c['path'] === 'string' &&
    (c['cmd'] === 'load_character' || c['cmd'] === 'load_scenario' || c['cmd'] === 'load_background')
  ) {
    watchAsset(c['path'] as string)
  }
  sendToEngine(cmd)
})

// Diálogo para abrir modelos 3D
ipcMain.handle('open-model-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:       'Abrir modelo 3D',
    filters:     [{ name: 'Modelos 3D', extensions: ['glb', 'gltf', 'fbx'] }],
    properties:  ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para abrir archivo de audio (WAV, OGG, MP3)
ipcMain.handle('open-audio-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Cargar audio de animación',
    filters:    [{ name: 'Audio', extensions: ['wav', 'ogg', 'mp3'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para abrir archivo de fuente (TTF, OTF)
ipcMain.handle('open-font-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Cargar fuente',
    filters:    [{ name: 'Fuentes', extensions: ['ttf', 'otf'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para abrir imagen PNG como escenario 2D
ipcMain.handle('open-scenario-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Cargar escenario (PNG)',
    filters:    [{ name: 'Imágenes PNG', extensions: ['png'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para abrir imagen PNG como personaje 2D
ipcMain.handle('open-character-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Cargar personaje (PNG)',
    filters:    [{ name: 'Imágenes PNG', extensions: ['png'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para abrir imagen PNG como sprite (solo almacenamiento en motor)
ipcMain.handle('open-sprite-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Cargar sprite (PNG)',
    filters:    [{ name: 'Imágenes PNG', extensions: ['png'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para imágenes HUD (transparencia: PNG o WebP)
ipcMain.handle('open-hud-image-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Cargar imagen HUD',
    filters:    [{ name: 'Imágenes HUD (PNG, JPEG, WebP)', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
    properties: ['openFile'],
  })
  return result.canceled ? null : result.filePaths[0] ?? null
})

// Diálogo para abrir imagen PNG/GIF como fondo del mundo 2D
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

function sendEngineViewportBounds(bounds: ViewportBounds): void {
  const useScreenBounds = process.platform === 'win32' || process.platform === 'linux'
  sendToEngine({
    cmd:    'set_bounds',
    x:      Math.round(bounds.x),
    y:      Math.round(bounds.y),
    width:  Math.max(1, Math.round(bounds.width)),
    height: Math.max(1, Math.round(bounds.height)),
    offset_x: useScreenBounds && lastRelativeBounds ? Math.round(lastRelativeBounds.x) : undefined,
    offset_y: useScreenBounds && lastRelativeBounds ? Math.round(lastRelativeBounds.y) : undefined,
  })
}

function collapsedViewportBounds(bounds: ViewportBounds): ViewportBounds {
  return { ...bounds, width: 1, height: 1 }
}

// Oculta el motor (para que no tape modales del renderer)
ipcMain.on('hide-engine-viewport', () => {
  if (!engineStarted) return
  engineViewportHidden = true
  const bounds = lastEffectiveBounds ?? { x: 0, y: 0, width: 1, height: 1 }
  sendEngineViewportBounds(collapsedViewportBounds(bounds))
})

// ---------------------------------------------------------------------------
// Modal Electron (singleton, encima del motor sin hideEngineViewport)
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
    if (!mainWindow || mainWindow.isDestroyed()) return null
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
      mainWindow!.webContents.send('modal-electron:delegate-request', { ...request, requestId })
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
}) => {
  sendPatchToModal(data)
})

ipcMain.handle(
  'modal-electron:player-ui-action',
  async (_event, req: { handlerId: string; action: unknown }) => {
    if (!mainWindow || mainWindow.isDestroyed()) return
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
      mainWindow!.webContents.send('modal-electron:player-ui-action-request', {
        ...req,
        requestId,
      })
    })
  },
)

ipcMain.handle(
  'modal-electron:entity-properties-action',
  async (_event, req: { handlerId: string; action: unknown }) => {
    if (!mainWindow || mainWindow.isDestroyed()) return
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
      mainWindow!.webContents.send('modal-electron:entity-properties-action-request', {
        ...req,
        requestId,
      })
    })
  },
)

ipcMain.handle(
  'modal-electron:player-ui-state',
  async (_event, req: { handlerId: string }) => {
    if (!mainWindow || mainWindow.isDestroyed()) return null
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
      mainWindow!.webContents.send('modal-electron:player-ui-state-request', {
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

// Restaura el motor a los últimos bounds conocidos
ipcMain.on('restore-engine-viewport', (_event, bounds) => {
  if (!engineStarted) return
  let useBounds = null
  if (bounds && typeof bounds.x === 'number' && typeof bounds.y === 'number' && typeof bounds.width === 'number' && typeof bounds.height === 'number') {
    useBounds = bounds
  } else if (lastEffectiveBounds) {
    useBounds = lastEffectiveBounds
  }
  if (!useBounds) return
  engineViewportHidden = false
  sendEngineViewportBounds(useBounds)
})

// ---------------------------------------------------------------------------
// Helpers de guardado consolidado (.save)
// ---------------------------------------------------------------------------

function ensureSaveExtension(filePath: string): string {
  return path.extname(filePath).toLowerCase() === '.save' ? filePath : `${filePath}.save`
}

const SCRIPT_FILE_PREFIX = '@file:'
const AUDIO_EXTENSIONS = new Set(['.wav', '.ogg', '.mp3', '.flac', '.aac', '.m4a'])
const FONT_EXTENSIONS = new Set(['.ttf', '.otf'])
const HUD_IMAGE_EXTENSIONS = new Set(['.png', '.jpg', '.jpeg', '.webp'])

function sanitizeSegment(raw: string | null | undefined, fallback = 'item'): string {
  const text = (raw ?? '').trim()
  const cleaned = text
    .replace(/[<>:"/\\|?*\x00-\x1f]/g, '_')
    .replace(/\s+/g, '_')
    .replace(/^\.+/, '')
  return cleaned.length > 0 ? cleaned : fallback
}

function isAudioAsset(filePath: string): boolean {
  return AUDIO_EXTENSIONS.has(path.extname(filePath).toLowerCase())
}

function isFontAsset(filePath: string): boolean {
  return FONT_EXTENSIONS.has(path.extname(filePath).toLowerCase())
}

function isHudImageAsset(filePath: string): boolean {
  return HUD_IMAGE_EXTENSIONS.has(path.extname(filePath).toLowerCase())
}

function toAssetPathKey(filePath: string): string {
  const normalized = path.normalize(filePath)
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized
}

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
): T {
  const assetPath = getEntityAssetPath(entity)
  const remapped = assetPath ? (mapPath(assetPath) as string) : undefined
  const next = { ...entity } as T & { path?: string; model?: string }
  if (typeof next.path === 'string' && remapped) next.path = remapped
  if (typeof next.model === 'string' && remapped) next.model = remapped
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
  return {
    sounds: data.sounds?.length ?? 0,
    fonts: data.fonts?.length ?? 0,
    backgrounds: data.backgrounds?.length ?? 0,
    hudImages: data.hudImages?.length ?? 0,
    sprites: countActiveSceneSprites(data),
    models: data.models?.length ?? 0,
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

/**
 * Recorre un ProjectSaveData y devuelve todos los paths de archivo absolutos
 * que hay que copiar al paquete de assets del archivo .save.
 */
function collectAssetPaths(data: ProjectSaveData): Set<string> {
  const paths = new Set<string>()
  const hasScenes = (data.scenes?.length ?? 0) > 0
  const add = (p: string | null | undefined) => {
    if (!p || !path.isAbsolute(p) || !fs.existsSync(p)) return
    // En Windows evitamos duplicados del mismo archivo por separadores distintos (\ vs /).
    paths.add(toAssetPathKey(p))
  }

  if (!hasScenes) {
    add(data.backgroundPath)
    if (data.sprites) {
      for (const sprite of data.sprites) add(sprite.path)
    }
  }
  for (const scene of data.scenes ?? []) {
    add(scene.backgroundPath)
    for (const sprite of scene.sprites ?? []) add(sprite.path)
  }

  if (data.sounds) {
    for (const sound of data.sounds) add(sound.path)
  }

  if (data.fonts) {
    for (const font of data.fonts) add(font.path)
  }

  if (data.backgrounds) {
    for (const bg of data.backgrounds) add(bg.path)
  }

  if (data.hudImages) {
    for (const img of data.hudImages) add(img.path)
  }

  // playerUi* solo referencian fonts[] / hudImages[] — no empaquetar otra vez aquí.

  add(data.player?.model)
  if (data.models) {
    for (const model of data.models) add(model.path)
  }

  for (const scene of data.scenes ?? []) {
    add(scene.player?.model)
    for (const model of scene.models ?? []) add(model.path)
  }

  forEachEntity(data, (entity) => {
    const assetPath = getEntityAssetPath(entity)
    if (assetPath && !entityPathMarker(assetPath)) add(assetPath)
    for (const anim of entity.animations ?? []) {
      add(anim.audio_path)
      for (const frame of anim.frames) {
        add(frame.path)
      }
    }
  })

  for (const bp of data.blueprints ?? []) {
    add(getBlueprintAssetPath(bp))
    for (const anim of bp.animations ?? []) {
      add(anim.audio_path)
      for (const frame of anim.frames) {
        add(frame.path)
      }
    }
  }

  return paths
}

/**
 * Copia todos los assets al directorio temporal (`assets/`, `sounds/`, `fonts/`) y devuelve
 * un mapa de ruta-absoluta → ruta-relativa dentro del paquete .save.
 * Si dos archivos distintos tienen el mismo nombre, se les agrega un sufijo numérico.
 */
function copyAssetsToDir(
  assetPaths: Set<string>,
  assetsDir: string,
  soundsDir: string,
  fontsDir: string,
  hudImagesDir: string,
): Map<string, string> {
  fs.mkdirSync(assetsDir, { recursive: true })
  fs.mkdirSync(soundsDir, { recursive: true })
  fs.mkdirSync(fontsDir, { recursive: true })
  fs.mkdirSync(hudImagesDir, { recursive: true })
  const map = new Map<string, string>()
  const usedNames = new Map<string, number>()

  for (const src of assetPaths) {
    const baseName = path.basename(src)
    let targetDir = assetsDir
    let relPrefix = 'assets'
    if (isAudioAsset(src)) {
      targetDir = soundsDir
      relPrefix = 'sounds'
    } else if (isFontAsset(src)) {
      targetDir = fontsDir
      relPrefix = 'fonts'
    } else if (isHudImageAsset(src)) {
      targetDir = hudImagesDir
      relPrefix = 'hud-images'
    }
    const key = `${relPrefix}/${baseName}`
    const count = usedNames.get(key) ?? 0
    usedNames.set(key, count + 1)

    const destName = count === 0
      ? baseName
      : `${path.basename(baseName, path.extname(baseName))}_${count}${path.extname(baseName)}`

    const destAbs = path.join(targetDir, destName)
    try {
      fs.copyFileSync(src, destAbs)
      // Siempre usar '/' en los paths del JSON para portabilidad entre OS
      map.set(src, `${relPrefix}/${destName}`)
    } catch (err) {
      console.error(`[editor] No se pudo copiar asset ${src}:`, err)
    }
  }
  return map
}

function serializeScriptsToFiles(data: ProjectSaveData, scriptingDir: string): { data: ProjectSaveData; count: number } {
  fs.mkdirSync(scriptingDir, { recursive: true })
  const usedNames = new Map<string, number>()
  const sourceCache = new Map<string, string>()
  let total = 0

  const nextName = (folderKey: string, baseName: string) => {
    const key = `${folderKey}/${baseName}`
    const count = usedNames.get(key) ?? 0
    usedNames.set(key, count + 1)
    return count === 0 ? baseName : `${path.basename(baseName, '.rhai')}_${count}.rhai`
  }

  const saveScript = (sourceScript: { name: string; source: string }, folderParts: string[], fallbackName: string) => {
    const safeFolderParts = folderParts.map((part) => sanitizeSegment(part, 'group'))
    const folderRel = safeFolderParts.join('/')
    const cacheKey = `${folderRel}\n${sourceScript.name}\n${sourceScript.source ?? ''}`
    const cachedRef = sourceCache.get(cacheKey)
    if (cachedRef) {
      return {
        ...sourceScript,
        source: cachedRef,
      }
    }

    const fileBase = `${sanitizeSegment(sourceScript.name, fallbackName)}.rhai`
    const fileName = nextName(folderRel, fileBase)
    const relPath = folderRel.length > 0 ? `scripting/${folderRel}/${fileName}` : `scripting/${fileName}`
    const absDir = folderRel.length > 0 ? path.join(scriptingDir, ...safeFolderParts) : scriptingDir
    fs.mkdirSync(absDir, { recursive: true })
    fs.writeFileSync(path.join(absDir, fileName), sourceScript.source ?? '', 'utf8')
    total += 1
    const sourceRef = `${SCRIPT_FILE_PREFIX}${relPath}`
    sourceCache.set(cacheKey, sourceRef)
    return {
      ...sourceScript,
      source: sourceRef,
    }
  }

  const packControls = (
    controls: SavedControls | undefined,
    entityFolder: string,
  ): SavedControls | undefined => {
    if (!controls) return undefined
    return {
      keyboard_mouse: Object.fromEntries(
        Object.entries(controls.keyboard_mouse).map(([key, script], idx) => [
          key,
          saveScript(
            script,
            [entityFolder, 'controls', 'keyboard_mouse', sanitizeSegment(key, `key_${idx + 1}`)],
            `script_${idx + 1}`,
          ),
        ]),
      ),
      gamepad: Object.fromEntries(
        Object.entries(controls.gamepad).map(([key, script], idx) => [
          key,
          saveScript(
            script,
            [entityFolder, 'controls', 'gamepad', sanitizeSegment(key, `btn_${idx + 1}`)],
            `script_${idx + 1}`,
          ),
        ]),
      ),
    }
  }

  const mapEntity3d = (entity: Entity3D): Entity3D => {
    const entityFolder = `entity_${entity.id}`
    const controls =
      entity.controls ??
      (entity as Entity3D & { control_bindings?: SavedControls }).control_bindings
    return {
      ...entity,
      scripts: entity.scripts?.map((script, idx) =>
        saveScript(script, [entityFolder], `script_${idx + 1}`),
      ),
      animations: entity.animations?.map((anim, animIndex) => ({
        ...anim,
        scripts: anim.scripts?.map((script, scriptIndex) =>
          saveScript(
            script,
            [entityFolder, 'animations', sanitizeSegment(anim.name, `anim_${animIndex + 1}`)],
            `script_${scriptIndex + 1}`,
          ),
        ),
      })),
      controls: packControls(controls, entityFolder),
    }
  }

  const mapBlueprint = (bp: NonNullable<ProjectSaveData['blueprints']>[number]) => {
    const blueprintFolder = `blueprint_${sanitizeSegment(bp.id, 'bp')}`
    return {
      ...bp,
      scripts: bp.scripts?.map((script, idx) =>
        saveScript(script, [blueprintFolder], `script_${idx + 1}`),
      ),
      animations: bp.animations?.map((anim, animIndex) => ({
        ...anim,
        scripts: anim.scripts?.map((script, scriptIndex) =>
          saveScript(
            script,
            [blueprintFolder, 'animations', sanitizeSegment(anim.name, `anim_${animIndex + 1}`)],
            `script_${scriptIndex + 1}`,
          ),
        ),
      })),
    }
  }

  const hasScenes = (data.scenes?.length ?? 0) > 0

  return {
    count: total,
    data: {
      ...data,
      // Con multi-escena, las entidades viven solo en cada `scene`.
      entities: hasScenes ? [] : data.entities.map(mapEntity3d),
      player: data.player ? mapEntity3d(data.player) : data.player,
      scenes: data.scenes?.map((scene) => ({
        ...scene,
        entities: scene.entities.map(mapEntity3d),
        player: scene.player ? mapEntity3d(scene.player) : scene.player,
      })),
      blueprints: data.blueprints?.map(mapBlueprint),
    },
  }
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

/**
 * Clona el ProjectSaveData reemplazando todos los paths absolutos por relativos
 * según el mapa generado por copyAssetsToDir para serializar dentro del .save.
 */
function remapPaths(data: ProjectSaveData, map: Map<string, string>): ProjectSaveData {
  const remap = (p: string | null | undefined): string | null | undefined =>
    p ? (map.get(toAssetPathKey(p)) ?? p) : p

  const hasScenes = (data.scenes?.length ?? 0) > 0

  const mapEntity = (e: SaveManifestEntity): SaveManifestEntity =>
    mapEntityAnimations(mapEntityAssetPaths(e, remap), remap)

  const mapModels = (models: ProjectSaveData['models']) =>
    models?.map((m) => ({
      name: m.name,
      path: remap(m.path) as string,
      ...(m.category ? { category: m.category } : {}),
    }))

  return {
    ...data,
    world: hasScenes ? undefined : data.world,
    backgroundPath: hasScenes ? null : (remap(data.backgroundPath) as string | null),
    models: mapModels(data.models),
    sprites: hasScenes
      ? []
      : data.sprites?.map((s) => ({
          name: s.name,
          path: remap(s.path) as string,
        })),
    sounds: data.sounds?.map((s) => ({
      name: s.name,
      path: remap(s.path) as string,
    })),
    fonts: data.fonts?.map((f) => ({
      name: f.name,
      path: remap(f.path) as string,
    })),
    backgrounds: data.backgrounds?.map((b) => ({
      name: b.name,
      path: remap(b.path) as string,
    })),
    hudImages: data.hudImages?.map((img) => ({
      name: img.name,
      path: remap(img.path) as string,
    })),
    playerUiTextBoxes: data.playerUiTextBoxes?.map((box) => ({
      ...box,
      font_path: remap(box.font_path) as string,
    })),
    playerUiButtons: data.playerUiButtons?.map((btn) => ({
      ...btn,
      font_path: remap(btn.font_path) as string,
      texture_path: remap(btn.texture_path) as string | undefined,
    })),
    playerUiImages: data.playerUiImages?.map((img) => ({
      ...img,
      image_path: remap(img.image_path) as string,
    })),
    playerUiObjects: data.playerUiObjects?.map((obj) => ({
      ...obj,
      texture_path: remap(obj.texture_path) as string | undefined,
    })),
    entities: hasScenes ? [] : data.entities.map(mapEntity),
    player: hasScenes ? null : data.player ? mapEntity(data.player) : data.player,
    config_camera: hasScenes ? null : data.config_camera,
    config_editor_camera: hasScenes ? null : data.config_editor_camera,
    camera2d: hasScenes ? null : data.camera2d,
    scenes: data.scenes?.map((scene) => ({
      ...scene,
      backgroundPath: remap(scene.backgroundPath) as string | null,
      models: [],
      sprites: scene.sprites?.map((s) => ({
        name: s.name,
        path: remap(s.path) as string,
      })),
      entities: scene.entities.map(mapEntity),
      player: scene.player ? mapEntity(scene.player) : scene.player,
    })),
    blueprints: data.blueprints?.map((bp) => {
      const assetPath = getBlueprintAssetPath(bp)
      const remappedAsset = assetPath ? (remap(assetPath) as string) : undefined
      return {
        ...bp,
        ...(bp.model !== undefined && remappedAsset ? { model: remappedAsset } : {}),
        ...(bp.path !== undefined && remappedAsset ? { path: remappedAsset } : {}),
        animations: bp.animations?.map((anim) => ({
          ...anim,
          audio_path: remap(anim.audio_path) as string | undefined,
          frames: anim.frames.map((f) => ({
            ...f,
            path: remap(f.path) as string,
          })),
        })),
      }
    }),
  }
}

/**
 * Crea un archivo .save (ZIP) con `manifest.json` + `assets/`.
 */
function saveProjectToFile(saveFilePath: string, data: ProjectSaveData): boolean {
  const tempRoot = app.getPath('temp')
  const stagingDir = fs.mkdtempSync(path.join(tempRoot, 'rer-save-write-'))
  try {
    const assetsDir = path.join(stagingDir, 'assets')
    const soundsDir = path.join(stagingDir, 'sounds')
    const fontsDir = path.join(stagingDir, 'fonts')
    const hudImagesDir = path.join(stagingDir, 'hud-images')
    const scriptingDir = path.join(stagingDir, 'scripting')
    const assetPaths = collectAssetPaths(data)
    const pathMap = copyAssetsToDir(assetPaths, assetsDir, soundsDir, fontsDir, hudImagesDir)
    const remapped = remapPaths(data, pathMap)
    const scriptsPacked = serializeScriptsToFiles(remapped, scriptingDir)

    const manifestPath = path.join(stagingDir, 'manifest.json')
    fs.writeFileSync(manifestPath, JSON.stringify(scriptsPacked.data, null, 2), 'utf8')

    const zip = new AdmZip()
    zip.addLocalFolder(stagingDir)
    zip.writeZip(saveFilePath)

    const countRhaiFiles = (dir: string): number => {
      if (!fs.existsSync(dir)) return 0
      let total = 0
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name)
        if (entry.isDirectory()) {
          total += countRhaiFiles(full)
        } else if (entry.isFile() && path.extname(entry.name).toLowerCase() === '.rhai') {
          total += 1
        }
      }
      return total
    }

    const addRelIfPacked = (set: Set<string>, absPath: string | null | undefined) => {
      if (!absPath) return
      const rel = pathMap.get(toAssetPathKey(absPath))
      if (rel) set.add(rel)
    }

    // Desglose por recursos cargados en librería (Resources accordion), no por referencias.
    const uniqueSounds = new Set<string>()
    for (const sound of data.sounds ?? []) addRelIfPacked(uniqueSounds, sound.path)

    const uniqueFonts = new Set<string>()
    for (const font of data.fonts ?? []) addRelIfPacked(uniqueFonts, font.path)

    const uniqueBackgrounds = new Set<string>()
    for (const bg of data.backgrounds ?? []) addRelIfPacked(uniqueBackgrounds, bg.path)

    const uniqueHudImages = new Set<string>()
    for (const img of data.hudImages ?? []) addRelIfPacked(uniqueHudImages, img.path)

    const uniqueSprites = new Set<string>()
    for (const sprite of data.sprites ?? []) addRelIfPacked(uniqueSprites, sprite.path)
    for (const scene of data.scenes ?? []) {
      for (const sprite of scene.sprites ?? []) addRelIfPacked(uniqueSprites, sprite.path)
    }

    const classifiedAssets = new Set<string>([
      ...uniqueSounds,
      ...uniqueFonts,
      ...uniqueBackgrounds,
      ...uniqueHudImages,
      ...uniqueSprites,
    ])
    const otherAssetNames = Array.from(pathMap.values()).filter((rel) => !classifiedAssets.has(rel)).sort()
    const otherAssets = otherAssetNames.length
    const packedScriptFiles = countRhaiFiles(scriptingDir)
    const otherSuffix = otherAssets > 0 ? ` [${otherAssetNames.join(', ')}]` : ''
    const entityCount = countSavedEntities(data)
    const entityKindSuffix = formatEntityKindBreakdown(data)

    if (data.type === '3D') {
      const uniqueModels = new Set<string>()
      for (const model of data.models ?? []) addRelIfPacked(uniqueModels, model.path)
      for (const scene of data.scenes ?? []) {
        for (const model of scene.models ?? []) addRelIfPacked(uniqueModels, model.path)
      }
      const libraryLog = formatLibraryResourcesInLog({
        backgrounds: uniqueBackgrounds.size,
        sounds: uniqueSounds.size,
        fonts: uniqueFonts.size,
        hudImages: uniqueHudImages.size,
      })
      console.log(
        `[editor] Proyecto guardado `
        + `(entidades: ${entityCount}${entityKindSuffix}, modelos: ${uniqueModels.size}, ${libraryLog}, scripts empaquetados: ${packedScriptFiles})`,
      )
    } else {
      const libraryLog = formatLibraryResourcesInLog({
        backgrounds: uniqueBackgrounds.size,
        sounds: uniqueSounds.size,
        fonts: uniqueFonts.size,
        hudImages: uniqueHudImages.size,
      })
      console.log(
        `[editor] Proyecto guardado `
        + `(entidades: ${entityCount}${entityKindSuffix}, sprites: ${uniqueSprites.size}, ${libraryLog}, scripts empaquetados: ${packedScriptFiles}, otros: ${otherAssets}${otherSuffix})`,
      )
    }
    return true
  } catch (err) {
    console.error('[editor] Error al guardar proyecto:', err)
    return false
  } finally {
    fs.rmSync(stagingDir, { recursive: true, force: true })
  }
}

/**
 * Resuelve los paths relativos de un ProjectSaveData cargado desde un .save,
 * convirtiendo rutas relativas a absolutas respecto al directorio extraído.
 */
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

  const mapSaveEntity = (e: SaveManifestEntity): SaveManifestEntity => {
    const controls =
      e.controls ?? e.control_bindings
    const withPaths = mapEntityAnimations(mapEntityAssetPaths(e, resolve), resolve)
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

  const mapModels = (models: ProjectSaveData['models']) =>
    models?.map((m) => ({
      name: m.name,
      path: resolve(m.path) as string,
      ...(m.category ? { category: m.category } : {}),
    }))

  return {
    ...data,
    world: hasScenes ? undefined : data.world,
    backgroundPath: hasScenes ? null : (resolve(data.backgroundPath) as string | null),
    models: mapModels(data.models),
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
    console.error('[editor] Error al extraer archivo .save:', err)
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

// Diálogo para guardar el proyecto (archivo .save)
ipcMain.handle('save-project', async (_event, data: ProjectSaveData): Promise<string | null> => {
  if (!mainWindow) return null
  const result = await dialog.showSaveDialog(mainWindow, {
    title: 'Guardar proyecto',
    defaultPath: currentProjectFilePath ?? 'project.save',
    filters: [{ name: 'Proyecto RER Save', extensions: ['save'] }],
  })
  if (result.canceled || !result.filePath) return null

  const savePath = ensureSaveExtension(result.filePath)
  const ok = saveProjectToFile(savePath, data)
  if (!ok) return null

  currentProjectFilePath = savePath
  syncExtractDirFromSavePath(savePath)
  return savePath
})

// Guardado silencioso (auto-save): sobrescribe el archivo .save indicado.
ipcMain.handle('save-project-silent', async (_event, filePath: string, data: ProjectSaveData): Promise<boolean> => {
  const targetPath = path.isAbsolute(filePath)
    ? ensureSaveExtension(filePath)
    : path.join(app.getPath('userData'), ensureSaveExtension(filePath))

  const ok = saveProjectToFile(targetPath, data)
  if (ok) {
    currentProjectFilePath = targetPath
    syncExtractDirFromSavePath(targetPath)
  }
  return ok
})

ipcMain.handle('get-project-extract-dir', (): string | null => {
  return currentProjectExtractDir
})

ipcMain.handle('read-project-manifest', (): ProjectSaveData | null => {
  if (!currentProjectExtractDir) return null
  return loadProjectFromExtractDir(currentProjectExtractDir)
})

function syncExtractDirFromSavePath(savePath: string): void {
  const extractDir = extractSaveArchive(savePath)
  if (extractDir) {
    currentProjectExtractDir = extractDir
  }
}

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
  currentProjectExtractDir =
    typeof extractDir === 'string' && extractDir.trim().length > 0
      ? extractDir.trim()
      : null
})

// El renderer envía los bounds del viewport una vez montado (y en cada resize).
// Al primer mensaje arrancamos el motor con las coordenadas correctas.
let engineStarted = false
/** Motor arrancado en 1×1 hasta `restore-engine-viewport` (tras `ready` / fin de carga). */
let engineViewportHidden = true

// Caché de los bounds relativos del viewport (posición dentro del contenido de Electron,
// pre-DPR). Se actualiza en cada 'viewport-bounds'.
let lastRelativeBounds: ViewportBounds | null = null

// El motor overlay usa coordenadas de pantalla absolutas (popup separado).
// Convierte los bounds DPR-escalados del renderer (relativos al contenido de
// Electron) a coordenadas físicas de pantalla.
function viewportToScreenBounds(bounds: ViewportBounds): ViewportBounds {
  if (!mainWindow) return bounds
  const cb          = mainWindow.getContentBounds()
  const scaleFactor = electronScreen.getDisplayMatching(mainWindow.getBounds()).scaleFactor
  return {
    x:      Math.round(cb.x * scaleFactor + bounds.x),
    y:      Math.round(cb.y * scaleFactor + bounds.y),
    width:  bounds.width,
    height: bounds.height,
    // Pasar los offsets físicos del renderer tal cual (sin conversión DPI adicional).
    // El position-tracker Rust los usa como offset relativo al área de contenido.
    rel_x:  Math.round(bounds.x),
    rel_y:  Math.round(bounds.y),
  }
}

/** Origen en pantalla (DIP) de la esquina superior izquierda del viewport del motor. */
function getEngineViewportScreenOrigin(): { x: number; y: number } | null {
  if (!mainWindow || mainWindow.isDestroyed() || !lastRelativeBounds) return null

  const cb = mainWindow.getContentBounds()
  const scaleFactor = electronScreen.getDisplayMatching(mainWindow.getBounds()).scaleFactor
  return {
    x: cb.x + Math.round(lastRelativeBounds.x / scaleFactor),
    y: cb.y + Math.round(lastRelativeBounds.y / scaleFactor),
  }
}

ipcMain.on('viewport-bounds', (_event, bounds: ViewportBounds) => {
  lastRelativeBounds = bounds

  // Si el proceso murió, permitir relanzar
  if (engineStarted && !engineProcess) {
    engineStarted = false
    engineViewportHidden = true
  }

  const useScreenBounds = process.platform === 'win32' || process.platform === 'linux'
  const effectiveBounds = useScreenBounds ? viewportToScreenBounds(bounds) : bounds
  lastEffectiveBounds = effectiveBounds

  if (engineStarted) {
    if (engineViewportHidden) {
      return
    }
    sendEngineViewportBounds(effectiveBounds)
    return
  }
  // Primera vez (o relanzar tras muerte): arrancar oculto (1×1) hasta `restore-engine-viewport`
  engineStarted = true
  engineViewportHidden = true
  startEngine(collapsedViewportBounds(effectiveBounds))
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
  startElectronResourceSampling()

  // CSP estricto solo en producción (app.isPackaged).
  // En desarrollo, Vite inyecta scripts inline para HMR/React preamble
  // que serían bloqueados. El warning de Electron en dev desaparece
  // automáticamente al empaquetar la app.
  if (app.isPackaged) {
    const CSP = [
      "default-src 'self'",
      "script-src 'self'",
      "style-src 'self' 'unsafe-inline'",
      "img-src 'self' data: blob: file:",
      "media-src 'self' file: blob:",
      "connect-src 'self'",
      "font-src 'self' data:",
    ].join('; ')

    session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
      callback({
        responseHeaders: {
          ...details.responseHeaders,
          'Content-Security-Policy': [CSP],
        },
      })
    })
  }

  createMainWindow()
  initModalElectron(() => mainWindow, getEngineViewportScreenOrigin)
  // No arrancamos el motor aquí: esperamos el primer 'viewport-bounds'

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow()
    }
  })
})

app.on('window-all-closed', () => {
  destroyModalElectronWindow()
  stopEngine()
  cleanupExtractedProjectDirs()
  clearAssetWatchers()
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
