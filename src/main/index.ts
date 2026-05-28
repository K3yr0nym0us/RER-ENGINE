import { app, BrowserWindow, ipcMain, dialog, Menu, session, screen as electronScreen } from 'electron';
import { spawn, ChildProcess } from 'child_process';
import path from 'path';
import fs from 'fs';
import AdmZip from 'adm-zip';

import type { 
  EngineCommand, 
  EngineEvent, 
  GameStyle, 
  EngineStartPayload,
  OpenProjectResult, 
  ProjectSaveData,
  ProjectType,
} from '../shared-types/types';
import { entityPathMarker } from '../shared-types/types';

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

// Ventana secundaria del editor de scripts Lua
// (eliminada — el editor ahora vive en un modal de Bootstrap dentro del renderer)

// Últimos bounds efectivos del motor (para restaurarlo tras ocultarlo)
let lastEffectiveBounds: ViewportBounds | null = null

/** true tras recibir `ready` del proceso motor de la sesión actual. */
let engineReceivedReady = false

/** Evita reenviar `set_scene` 3D si el motor emite más de un `ready` (p. ej. tras `setup_empty_3d`). */
let engine3dStartupSceneSent = false

/** Binario base del motor en la sesión actual (`rer_engine_2d` / `rer_engine_3d`). */
let lastEngineBinary = 'rer_engine_2d'

function expectedGpuApiLabel(baseBinaryName: string): string {
  if (baseBinaryName === 'rer_engine_3d' && process.platform === 'win32') {
    return 'DirectX 12'
  }
  return 'Vulkan'
}

function gpuStartupErrorMessage(): string {
  const api = expectedGpuApiLabel(lastEngineBinary)
  if (lastEngineBinary === 'rer_engine_3d' && process.platform === 'win32') {
    return (
      `No se pudo iniciar el motor gráfico con ${api}. Instala o actualiza los controladores de video ` +
      'y asegúrate de tener DirectX 12 actualizado (Windows Update). ' +
      'Reinicia el editor después de instalar drivers.'
    )
  }
  return (
    `No se pudo iniciar el motor gráfico con ${api}. Instala o actualiza los controladores de video. ` +
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

// ---------------------------------------------------------------------------
// Ventana principal (UI React)
// ---------------------------------------------------------------------------
function createMainWindow(): void {
  Menu.setApplicationMenu(null)

  mainWindow = new BrowserWindow({
    width:  1280,
    height: 800,
    minWidth:  900,
    minHeight: 600,
    title: 'RER-ENGINE',
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

  // Linux: respaldo IPC al mover (el tracker X11 escucha ConfigureNotify).
  // Windows: solo el position-tracker nativo (WinEventHook); IPC aquí causa lag.
  mainWindow.on('move', () => {
    if (process.platform === 'linux') {
      mainWindow?.webContents.send('request-viewport-bounds')
    }
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

  const gpuLabel = expectedGpuApiLabel(baseBinaryName)
  console.log(`[engine] binario=${baseBinaryName} GPU esperada=${gpuLabel}`)

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
// IPC: renderer → motor y herramientas del editor
// ---------------------------------------------------------------------------
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

function toAssetPathKey(filePath: string): string {
  const normalized = path.normalize(filePath)
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized
}

function forEachEntity(data: ProjectSaveData, cb: (entity: ProjectSaveData['entities'][number]) => void): void {
  if ((data.scenes?.length ?? 0) > 0) {
    for (const scene of data.scenes ?? []) {
      for (const entity of scene.entities) cb(entity)
    }
    return
  }

  for (const entity of data.entities) cb(entity)
}

function countSavedEntities(data: ProjectSaveData): number {
  let count = 0
  forEachEntity(data, () => { count += 1 })
  return count
}

function formatEntityKindBreakdown(data: ProjectSaveData): string {
  const byKind = new Map<string, number>()
  forEachEntity(data, (entity) => {
    byKind.set(entity.kind, (byKind.get(entity.kind) ?? 0) + 1)
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
  const add = (p: string | null | undefined) => {
    if (!p || !path.isAbsolute(p) || !fs.existsSync(p)) return
    // En Windows evitamos duplicados del mismo archivo por separadores distintos (\ vs /).
    paths.add(toAssetPathKey(p))
  }

  add(data.backgroundPath)
  for (const scene of data.scenes ?? []) {
    add(scene.backgroundPath)
  }

  if (data.sprites) {
    for (const sprite of data.sprites) add(sprite.path)
  }
  for (const scene of data.scenes ?? []) {
    for (const sprite of scene.sprites ?? []) add(sprite.path)
  }

  if (data.sounds) {
    for (const sound of data.sounds) add(sound.path)
  }

  if (data.backgrounds) {
    for (const bg of data.backgrounds) add(bg.path)
  }

  add(data.playerTransform?.visual_model_path)
  if (data.models) {
    for (const model of data.models) add(model.path)
  }

  for (const scene of data.scenes ?? []) {
    add(scene.playerTransform?.visual_model_path)
    for (const model of scene.models ?? []) add(model.path)
  }

  forEachEntity(data, (entity) => {
    if (!entityPathMarker(entity.path)) add(entity.path)
    add(entity.visual_model_path)
    for (const anim of entity.animations ?? []) {
      add(anim.audio_path)
      for (const frame of anim.frames) {
        add(frame.path)
      }
    }
  })

  for (const bp of data.blueprints ?? []) {
    add(bp.path)
    add(bp.visualModelPath)
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
 * Copia todos los assets al directorio temporal (`assets/` y `sounds/`) y devuelve
 * un mapa de ruta-absoluta → ruta-relativa dentro del paquete .save.
 * Si dos archivos distintos tienen el mismo nombre, se les agrega un sufijo numérico.
 */
function copyAssetsToDir(
  assetPaths: Set<string>,
  assetsDir: string,
  soundsDir: string,
): Map<string, string> {
  fs.mkdirSync(assetsDir, { recursive: true })
  fs.mkdirSync(soundsDir, { recursive: true })
  const map = new Map<string, string>()
  const usedNames = new Map<string, number>()

  for (const src of assetPaths) {
    const baseName = path.basename(src)
    const targetDir = isAudioAsset(src) ? soundsDir : assetsDir
    const relPrefix = isAudioAsset(src) ? 'sounds' : 'assets'
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
    return count === 0 ? baseName : `${path.basename(baseName, '.lua')}_${count}.lua`
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

    const fileBase = `${sanitizeSegment(sourceScript.name, fallbackName)}.lua`
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

  const mapEntity = (entity: ProjectSaveData['entities'][number]) => {
    const entityFolder = `entity_${entity.id}`
    return {
      ...entity,
      scripts: entity.scripts?.map((script, idx) => saveScript(script, [entityFolder], `script_${idx + 1}`)),
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
      control_bindings: entity.control_bindings
        ? {
            keyboard_mouse: Object.fromEntries(
              Object.entries(entity.control_bindings.keyboard_mouse).map(([key, script], idx) => [
                key,
                saveScript(script, [entityFolder, 'controls', 'keyboard_mouse', sanitizeSegment(key, `key_${idx + 1}`)], `script_${idx + 1}`),
              ]),
            ),
            gamepad: Object.fromEntries(
              Object.entries(entity.control_bindings.gamepad).map(([key, script], idx) => [
                key,
                saveScript(script, [entityFolder, 'controls', 'gamepad', sanitizeSegment(key, `btn_${idx + 1}`)], `script_${idx + 1}`),
              ]),
            ),
          }
        : undefined,
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
      control_bindings: bp.control_bindings
        ? {
            keyboard_mouse: Object.fromEntries(
              Object.entries(bp.control_bindings.keyboard_mouse).map(([key, script], idx) => [
                key,
                saveScript(
                  script,
                  [blueprintFolder, 'controls', 'keyboard_mouse', sanitizeSegment(key, `key_${idx + 1}`)],
                  `script_${idx + 1}`,
                ),
              ]),
            ),
            gamepad: Object.fromEntries(
              Object.entries(bp.control_bindings.gamepad).map(([key, script], idx) => [
                key,
                saveScript(
                  script,
                  [blueprintFolder, 'controls', 'gamepad', sanitizeSegment(key, `btn_${idx + 1}`)],
                  `script_${idx + 1}`,
                ),
              ]),
            ),
          }
        : undefined,
    }
  }

  const hasScenes = (data.scenes?.length ?? 0) > 0

  return {
    count: total,
    data: {
      ...data,
      // Con multi-escena, las entidades viven solo en cada `scene`.
      entities: hasScenes ? [] : data.entities.map(mapEntity),
      scenes: data.scenes?.map((scene) => ({
        ...scene,
        entities: scene.entities.map(mapEntity),
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

  const mapEntity = (e: ProjectSaveData['entities'][number]) => ({
    ...e,
    path: remap(e.path) as string,
    visual_model_path: remap(e.visual_model_path) as string | undefined,
    animations: e.animations?.map((anim) => ({
      ...anim,
      audio_path: remap(anim.audio_path) as string | undefined,
      frames: anim.frames.map((f) => ({
        ...f,
        path: remap(f.path) as string,
      })),
    })),
  })

  const mapModels = (models: ProjectSaveData['models']) =>
    models?.map((m) => ({ name: m.name, path: remap(m.path) as string }))

  const mapPlayerTransform = (pt: ProjectSaveData['playerTransform']) =>
    pt
      ? { ...pt, visual_model_path: remap(pt.visual_model_path) as string | undefined }
      : pt

  return {
    ...data,
    backgroundPath: remap(data.backgroundPath) as string | null,
    playerTransform: mapPlayerTransform(data.playerTransform),
    models: mapModels(data.models),
    sprites: data.sprites?.map((s) => ({
      name: s.name,
      path: remap(s.path) as string,
    })),
    sounds: data.sounds?.map((s) => ({
      name: s.name,
      path: remap(s.path) as string,
    })),
    backgrounds: data.backgrounds?.map((b) => ({
      name: b.name,
      path: remap(b.path) as string,
    })),
    entities: hasScenes ? [] : data.entities.map(mapEntity),
    scenes: data.scenes?.map((scene) => ({
      ...scene,
      backgroundPath: remap(scene.backgroundPath) as string | null,
      playerTransform: mapPlayerTransform(scene.playerTransform),
      models: mapModels(scene.models),
      sprites: scene.sprites?.map((s) => ({
        name: s.name,
        path: remap(s.path) as string,
      })),
      entities: scene.entities.map(mapEntity),
    })),
    blueprints: data.blueprints?.map((bp) => ({
      ...bp,
      path: remap(bp.path) as string,
      visualModelPath: remap(bp.visualModelPath) as string | undefined,
      animations: bp.animations?.map((anim) => ({
        ...anim,
        audio_path: remap(anim.audio_path) as string | undefined,
        frames: anim.frames.map((f) => ({
          ...f,
          path: remap(f.path) as string,
        })),
      })),
    })),
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
    const scriptingDir = path.join(stagingDir, 'scripting')
    const assetPaths = collectAssetPaths(data)
    const pathMap = copyAssetsToDir(assetPaths, assetsDir, soundsDir)
    const remapped = remapPaths(data, pathMap)
    const scriptsPacked = serializeScriptsToFiles(remapped, scriptingDir)

    const manifestPath = path.join(stagingDir, 'manifest.json')
    fs.writeFileSync(manifestPath, JSON.stringify(scriptsPacked.data, null, 2), 'utf8')

    const zip = new AdmZip()
    zip.addLocalFolder(stagingDir)
    zip.writeZip(saveFilePath)

    const countLuaFiles = (dir: string): number => {
      if (!fs.existsSync(dir)) return 0
      let total = 0
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name)
        if (entry.isDirectory()) {
          total += countLuaFiles(full)
        } else if (entry.isFile() && path.extname(entry.name).toLowerCase() === '.lua') {
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

    const uniqueBackgrounds = new Set<string>()
    for (const bg of data.backgrounds ?? []) addRelIfPacked(uniqueBackgrounds, bg.path)

    const uniqueSprites = new Set<string>()
    for (const sprite of data.sprites ?? []) addRelIfPacked(uniqueSprites, sprite.path)
    for (const scene of data.scenes ?? []) {
      for (const sprite of scene.sprites ?? []) addRelIfPacked(uniqueSprites, sprite.path)
    }

    const classifiedAssets = new Set<string>([...uniqueSounds, ...uniqueBackgrounds, ...uniqueSprites])
    const otherAssetNames = Array.from(pathMap.values()).filter((rel) => !classifiedAssets.has(rel)).sort()
    const otherAssets = otherAssetNames.length
    const packedScriptFiles = countLuaFiles(scriptingDir)
    const otherSuffix = otherAssets > 0 ? ` [${otherAssetNames.join(', ')}]` : ''
    const entityCount = countSavedEntities(data)
    const entityKindSuffix = formatEntityKindBreakdown(data)

    if (data.type === '3D') {
      const uniqueModels = new Set<string>()
      for (const model of data.models ?? []) addRelIfPacked(uniqueModels, model.path)
      for (const scene of data.scenes ?? []) {
        for (const model of scene.models ?? []) addRelIfPacked(uniqueModels, model.path)
      }
      console.log(
        `[editor] Proyecto guardado en ${saveFilePath} `
        + `(entidades: ${entityCount}${entityKindSuffix}, modelos: ${uniqueModels.size}, fondos: ${uniqueBackgrounds.size}, sonidos: ${uniqueSounds.size}, scripts empaquetados: ${packedScriptFiles})`,
      )
    } else {
      console.log(
        `[editor] Proyecto guardado en ${saveFilePath} `
        + `(entidades: ${entityCount}${entityKindSuffix}, sprites: ${uniqueSprites.size}, fondos: ${uniqueBackgrounds.size}, sonidos: ${uniqueSounds.size}, scripts empaquetados: ${packedScriptFiles}, otros: ${otherAssets}${otherSuffix})`,
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
    if (path.isAbsolute(p)) return p
    // El JSON siempre guarda rutas con '/' — normalizamos al separador del OS actual
    const normalized = p.split('/').join(path.sep)
    return path.join(extractedDir, normalized)
  }

  const hasScenes = (data.scenes?.length ?? 0) > 0

  const mapEntity = (e: ProjectSaveData['entities'][number]) => ({
    ...e,
    path: resolve(e.path) as string,
    visual_model_path: resolve(e.visual_model_path) as string | undefined,
    scripts: e.scripts?.map((script) => ({
      ...script,
      source: resolveScriptSource(script.source, extractedDir),
    })),
    animations: e.animations?.map((anim) => ({
      ...anim,
      audio_path: resolve(anim.audio_path) as string | undefined,
      frames: anim.frames.map((f) => ({
        ...f,
        path: resolve(f.path) as string,
      })),
      scripts: anim.scripts?.map((script) => ({
        ...script,
        source: resolveScriptSource(script.source, extractedDir),
      })),
    })),
    control_bindings: e.control_bindings
      ? {
          keyboard_mouse: Object.fromEntries(
            Object.entries(e.control_bindings.keyboard_mouse).map(([key, script]) => [
              key,
              { ...script, source: resolveScriptSource(script.source, extractedDir) },
            ]),
          ),
          gamepad: Object.fromEntries(
            Object.entries(e.control_bindings.gamepad).map(([key, script]) => [
              key,
              { ...script, source: resolveScriptSource(script.source, extractedDir) },
            ]),
          ),
        }
      : undefined,
  })

  const mapModels = (models: ProjectSaveData['models']) =>
    models?.map((m) => ({ name: m.name, path: resolve(m.path) as string }))

  const mapPlayerTransform = (pt: ProjectSaveData['playerTransform']) =>
    pt
      ? { ...pt, visual_model_path: resolve(pt.visual_model_path) as string | undefined }
      : pt

  return {
    ...data,
    backgroundPath: resolve(data.backgroundPath) as string | null,
    playerTransform: mapPlayerTransform(data.playerTransform),
    models: mapModels(data.models),
    sprites: data.sprites?.map((s) => ({
      name: s.name,
      path: resolve(s.path) as string,
    })),
    sounds: data.sounds?.map((s) => ({
      name: s.name,
      path: resolve(s.path) as string,
    })),
    backgrounds: data.backgrounds?.map((b) => ({
      name: b.name,
      path: resolve(b.path) as string,
    })),
    entities: hasScenes ? [] : data.entities.map(mapEntity),
    scenes: data.scenes?.map((scene) => ({
      ...scene,
      backgroundPath: resolve(scene.backgroundPath) as string | null,
      playerTransform: mapPlayerTransform(scene.playerTransform),
      models: mapModels(scene.models),
      sprites: scene.sprites?.map((s) => ({
        name: s.name,
        path: resolve(s.path) as string,
      })),
      entities: scene.entities.map(mapEntity),
    })),
    blueprints: data.blueprints?.map((bp) => ({
      ...bp,
      path: resolve(bp.path) as string,
      visualModelPath: resolve(bp.visualModelPath) as string | undefined,
      scripts: bp.scripts?.map((script) => ({
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
        scripts: anim.scripts?.map((script) => ({
          ...script,
          source: resolveScriptSource(script.source, extractedDir),
        })),
      })),
      control_bindings: bp.control_bindings
        ? {
            keyboard_mouse: Object.fromEntries(
              Object.entries(bp.control_bindings.keyboard_mouse).map(([key, script]) => [
                key,
                { ...script, source: resolveScriptSource(script.source, extractedDir) },
              ]),
            ),
            gamepad: Object.fromEntries(
              Object.entries(bp.control_bindings.gamepad).map(([key, script]) => [
                key,
                { ...script, source: resolveScriptSource(script.source, extractedDir) },
              ]),
            ),
          }
        : undefined,
    })),
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
  return savePath
})

// Guardado silencioso (auto-save): sobrescribe el archivo .save indicado.
ipcMain.handle('save-project-silent', async (_event, filePath: string, data: ProjectSaveData): Promise<boolean> => {
  const targetPath = path.isAbsolute(filePath)
    ? ensureSaveExtension(filePath)
    : path.join(app.getPath('userData'), ensureSaveExtension(filePath))

  const ok = saveProjectToFile(targetPath, data)
  if (ok) currentProjectFilePath = targetPath
  return ok
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
  // No arrancamos el motor aquí: esperamos el primer 'viewport-bounds'

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow()
    }
  })
})

app.on('window-all-closed', () => {
  stopEngine()
  cleanupExtractedProjectDirs()
  clearAssetWatchers()
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
