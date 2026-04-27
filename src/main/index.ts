import { app, BrowserWindow, ipcMain, dialog, Menu, session, screen as electronScreen } from 'electron';
import { spawn, ChildProcess } from 'child_process';
import path from 'path';
import fs from 'fs';

import type { EngineCommand, EngineEvent, ProjectSaveData } from '../shared-types/types';

// Sin GPU hardware disponible: deshabilitar el proceso GPU de Chromium
// para evitar spam de viz_main_impl / command_buffer_proxy_impl
app.commandLine.appendSwitch('disable-gpu');
app.commandLine.appendSwitch('disable-software-rasterizer');

// En Linux forzar el backend X11 de Chromium/GTK para que el embedding
// XEMBED funcione correctamente. Las vars de entorno deben establecerse
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

// Buffer de eventos que llegaron antes de que el renderer estuviera listo
let rendererReady = false
const eventBuffer: EngineEvent[] = []

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

  // Cuando la ventana principal se mueve, pedir al renderer que reenvíe bounds.
  // El motor hijo necesita recalcular su posición relativa al nuevo origen.
  mainWindow.on('move', () => {
    mainWindow?.webContents.send('request-viewport-bounds')
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
}

function startEngine(embed?: ViewportBounds): void {
  const binaryName = process.platform === 'win32' ? 'rer-engine.exe' : 'rer-engine'
  const enginePath = app.isPackaged
    ? path.join(process.resourcesPath, 'engine', binaryName)
    : path.join(app.getAppPath(), 'src', 'main', 'Engine', 'target', 'debug', binaryName)

  // Argumentos de embedding en Linux
  let engineArgs: string[] = []
  if (process.platform === 'linux' && embed) {
    const xid = getMainWindowXID()
    if (xid !== 0) {
      const x      = Math.round(embed.x)
      const y      = Math.round(embed.y)
      const width  = Math.max(1, Math.round(embed.width))
      const height = Math.max(1, Math.round(embed.height))
      engineArgs = ['--embed', String(xid), String(x), String(y), String(width), String(height)]
      console.log(`[engine] modo embed — xid=${xid} pos=(${x},${y}) size=${width}x${height}`)
    }
  } else if (process.platform === 'win32' && embed) {
    // En Windows usamos SetParent vía winit para embedding nativo.
    // Pasamos el HWND real de la ventana Electron.
    const hwnd   = getMainWindowHWND()
    const x      = Math.round(embed.x)
    const y      = Math.round(embed.y)
    const width  = Math.max(1, Math.round(embed.width))
    const height = Math.max(1, Math.round(embed.height))
    engineArgs = ['--embed', hwnd, String(x), String(y), String(width), String(height)]
    console.log(`[engine] modo embed Windows — hwnd=${hwnd} pos=(${x},${y}) size=${width}x${height}`)
  }

  // LIBGL_ALWAYS_SOFTWARE=1 asegura que EGL use llvmpipe en vez de buscar DRI3.
  // EGL_LOG_LEVEL=fatal silencia el warning "DRI3 error" de libEGL.
  // Estas variables solo aplican en Linux; en Windows se omiten para no contaminar el entorno.
  const linuxEnv = process.platform === 'linux'
    ? {
        WAYLAND_DISPLAY: '',
        GDK_BACKEND:     'x11',
        LIBGL_ALWAYS_SOFTWARE: '1',
        EGL_LOG_LEVEL:   'fatal',
        // Asegurar que el motor herede el servidor de audio de WSLg
        ...(process.env.PULSE_SERVER ? { PULSE_SERVER: process.env.PULSE_SERVER } : {}),
      }
    : {}

  engineProcess = spawn(enginePath, engineArgs, {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: {
      ...process.env,
      ...linuxEnv,
    },
  })

  // stdout → eventos para el renderer
  engineProcess.stdout?.on('data', (data: Buffer) => {
    const lines = data.toString('utf8').split('\n').filter(Boolean)
    for (const line of lines) {
      try {
        const event = JSON.parse(line) as EngineEvent
        sendEventToRenderer(event)
      } catch {
        console.log('[engine stdout]', line)
      }
    }
  })

  // stderr → log de consola
  engineProcess.stderr?.on('data', (data: Buffer) => {
    console.error('[engine stderr]', data.toString('utf8').trimEnd())
  })

  engineProcess.on('close', (code) => {
    console.log(`[engine] proceso terminado con código ${code}`)
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
  }
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
// IPC: renderer → motor y herramientas del editor
// ---------------------------------------------------------------------------
ipcMain.on('engine:cmd', (_event, cmd: EngineCommand) => {
  sendToEngine(cmd)
})

// Diálogo para abrir modelos 3D
ipcMain.handle('open-model-dialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:       'Abrir modelo 3D',
    filters:     [{ name: 'Modelos 3D', extensions: ['glb', 'gltf'] }],
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

// ---------------------------------------------------------------------------
// Helpers de guardado con copia de assets
// ---------------------------------------------------------------------------

/**
 * Recorre un ProjectSaveData y devuelve todos los paths de archivo absolutos
 * que hay que copiar al directorio de assets del proyecto.
 */
function collectAssetPaths(data: ProjectSaveData): Set<string> {
  const paths = new Set<string>()
  const add = (p: string | null | undefined) => {
    if (p && path.isAbsolute(p) && fs.existsSync(p)) paths.add(p)
  }

  add(data.backgroundPath)
  for (const entity of data.entities) {
    add(entity.path)
    for (const anim of entity.animations ?? []) {
      add(anim.audio_path)
      for (const frame of anim.frames) {
        add(frame.path)
      }
    }
  }
  return paths
}

/**
 * Copia todos los assets al directorio `projectDir/assets/` y devuelve
 * un mapa de ruta-absoluta → ruta-relativa-desde-projectDir.
 * Si dos archivos distintos tienen el mismo nombre, se les agrega un sufijo numérico.
 */
function copyAssetsToDir(
  assetPaths: Set<string>,
  assetsDir: string,
): Map<string, string> {
  fs.mkdirSync(assetsDir, { recursive: true })
  const map = new Map<string, string>()
  const usedNames = new Map<string, number>()

  for (const src of assetPaths) {
    const baseName = path.basename(src)
    const count    = (usedNames.get(baseName) ?? 0)
    usedNames.set(baseName, count + 1)

    const destName = count === 0
      ? baseName
      : `${path.basename(baseName, path.extname(baseName))}_${count}${path.extname(baseName)}`

    const destAbs = path.join(assetsDir, destName)
    try {
      fs.copyFileSync(src, destAbs)
      // Siempre usar '/' en los paths del JSON para portabilidad entre OS
      map.set(src, `assets/${destName}`)
    } catch (err) {
      console.error(`[editor] No se pudo copiar asset ${src}:`, err)
    }
  }
  return map
}

/**
 * Clona el ProjectSaveData reemplazando todos los paths absolutos por relativos
 * según el mapa generado por copyAssetsToDir.
 */
function remapPaths(data: ProjectSaveData, map: Map<string, string>): ProjectSaveData {
  const remap = (p: string | null | undefined): string | null | undefined =>
    p ? (map.get(p) ?? p) : p

  return {
    ...data,
    backgroundPath: remap(data.backgroundPath) as string | null,
    entities: data.entities.map((e) => ({
      ...e,
      path: remap(e.path) as string,
      animations: e.animations?.map((anim) => ({
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
 * Función central de guardado: crea la carpeta del proyecto, copia los assets
 * y escribe project.json con rutas relativas.
 * Devuelve la ruta al project.json creado, o null si hubo error.
 */
function saveProjectToDir(projectDir: string, data: ProjectSaveData): string | null {
  try {
    fs.mkdirSync(projectDir, { recursive: true })
    const assetsDir = path.join(projectDir, 'assets')
    const assetPaths = collectAssetPaths(data)
    const pathMap    = copyAssetsToDir(assetPaths, assetsDir)
    const remapped   = remapPaths(data, pathMap)
    const jsonPath   = path.join(projectDir, 'project.json')
    fs.writeFileSync(jsonPath, JSON.stringify(remapped, null, 2), 'utf8')
    console.log(`[editor] Proyecto guardado en ${jsonPath} (${pathMap.size} assets copiados)`)
    return jsonPath
  } catch (err) {
    console.error('[editor] Error al guardar proyecto:', err)
    return null
  }
}

/**
 * Resuelve los paths relativos de un ProjectSaveData cargado desde disco,
 * convirtiendo rutas relativas a absolutas respecto a projectDir.
 */
function resolveLoadedPaths(data: ProjectSaveData, projectDir: string): ProjectSaveData {
  const resolve = (p: string | null | undefined): string | null | undefined => {
    if (!p) return p
    if (path.isAbsolute(p)) return p
    // El JSON siempre guarda rutas con '/' — normalizamos al separador del OS actual
    const normalized = p.split('/').join(path.sep)
    return path.join(projectDir, normalized)
  }

  return {
    ...data,
    backgroundPath: resolve(data.backgroundPath) as string | null,
    entities: data.entities.map((e) => ({
      ...e,
      path: resolve(e.path) as string,
      animations: e.animations?.map((anim) => ({
        ...anim,
        audio_path: resolve(anim.audio_path) as string | undefined,
        frames: anim.frames.map((f) => ({
          ...f,
          path: resolve(f.path) as string,
        })),
      })),
    })),
  }
}

// ---------------------------------------------------------------------------
// IPC: guardar / cargar proyecto
// ---------------------------------------------------------------------------

// Diálogo para abrir un proyecto existente (lee project.json dentro de una carpeta)
ipcMain.handle('open-project-dialog', async (): Promise<ProjectSaveData | null> => {
  if (!mainWindow) return null
  // Permitir abrir tanto la carpeta del proyecto como el project.json directamente
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Abrir proyecto',
    filters:    [{ name: 'Proyecto RER', extensions: ['json'] }],
    properties: ['openFile'],
  })
  if (result.canceled || !result.filePaths[0]) return null
  try {
    const jsonPath   = result.filePaths[0]
    const projectDir = path.dirname(jsonPath)
    const raw  = fs.readFileSync(jsonPath, 'utf8')
    const data = JSON.parse(raw) as unknown
    if (data !== null && typeof data === 'object' && 'type' in data && 'gameStyle' in data) {
      return resolveLoadedPaths(data as ProjectSaveData, projectDir)
    }
    return null
  } catch {
    return null
  }
})

// Diálogo para guardar el proyecto (el usuario elige/crea una CARPETA)
ipcMain.handle('save-project', async (_event, data: ProjectSaveData): Promise<boolean> => {
  if (!mainWindow) return false
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Guardar proyecto — elige o crea una carpeta',
    properties: ['openDirectory', 'createDirectory'],
  })
  if (result.canceled || !result.filePaths[0]) return false
  const ok = saveProjectToDir(result.filePaths[0], data)
  return ok !== null
})

// Guardado silencioso (auto-save): filePath es la carpeta del proyecto
ipcMain.handle('save-project-silent', async (_event, filePath: string, data: ProjectSaveData): Promise<boolean> => {
  // filePath puede ser la carpeta o el project.json; normalizamos a carpeta
  const projectDir = path.extname(filePath) === '.json' ? path.dirname(filePath) : filePath
  const ok = saveProjectToDir(projectDir, data)
  return ok !== null
})

// El renderer envía los bounds del viewport una vez montado (y en cada resize).
// Al primer mensaje arrancamos el motor con las coordenadas correctas.
let engineStarted = false

// En Windows el motor corre como owned popup (no WS_CHILD), por lo que
// necesita coordenadas de pantalla absolutas en vez de coordenadas relativas
// al área cliente de Electron. Convierte los bounds DPR-escalados del renderer
// (relativos al contenido de Electron) a coordenadas de pantalla físicas.
function viewportToScreenBounds(bounds: ViewportBounds): ViewportBounds {
  if (!mainWindow) return bounds
  const cb          = mainWindow.getContentBounds()
  const scaleFactor = electronScreen.getDisplayMatching(mainWindow.getBounds()).scaleFactor
  return {
    x:      Math.round(cb.x * scaleFactor + bounds.x),
    y:      Math.round(cb.y * scaleFactor + bounds.y),
    width:  bounds.width,
    height: bounds.height,
  }
}

ipcMain.on('viewport-bounds', (_event, bounds: ViewportBounds) => {
  // Si el proceso murió, permitir relanzar
  if (engineStarted && !engineProcess) {
    engineStarted = false
  }

  // En Windows el popup usa coordenadas de pantalla absolutas
  const effectiveBounds = process.platform === 'win32' ? viewportToScreenBounds(bounds) : bounds

  if (engineStarted) {
    // Motor corriendo: reposicionar y redimensionar
    sendToEngine({
      cmd: 'set_bounds',
      x:      Math.round(effectiveBounds.x),
      y:      Math.round(effectiveBounds.y),
      width:  Math.max(1, Math.round(effectiveBounds.width)),
      height: Math.max(1, Math.round(effectiveBounds.height)),
    })
    return
  }
  // Primera vez (o relanzar tras muerte): arrancar el motor
  engineStarted = true
  startEngine(effectiveBounds)
})

// ---------------------------------------------------------------------------
// Ciclo de vida de la app
// ---------------------------------------------------------------------------
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
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
