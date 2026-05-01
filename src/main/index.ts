import { app, BrowserWindow, ipcMain, dialog, Menu, session, screen as electronScreen } from 'electron';
import { spawn, ChildProcess } from 'child_process';
import path from 'path';
import fs from 'fs';
import AdmZip from 'adm-zip';

import type { EngineCommand, EngineEvent, OpenProjectResult, ProjectSaveData } from '../shared-types/types';

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

// Path del proyecto abierto/guardado actualmente.
let currentProjectFilePath: string | null = null

// Directorios temporales con contenido extraído de .save que deben vivir
// mientras el proyecto está abierto para que el motor lea rutas absolutas.
const extractedProjectDirs = new Set<string>()

// Ventana secundaria del editor de scripts Lua
// (eliminada — el editor ahora vive en un modal de Bootstrap dentro del renderer)

// Últimos bounds efectivos del motor (para restaurarlo tras ocultarlo)
let lastEffectiveBounds: ViewportBounds | null = null

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

  // Cuando la ventana principal se mueve en Linux, el motor necesita recalcular
  // su posición. En Windows, el hilo position-tracker nativo (Rust/Win32) se encarga
  // de mover la ventana del motor en tiempo real sin IPC, así que no hacemos nada aquí.
  mainWindow.on('move', () => {
    if (process.platform !== 'win32') {
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
    const relX = Math.max(0, Math.round(embed.rel_x ?? 0))
    const relY = Math.max(0, Math.round(embed.rel_y ?? 0))
    engineArgs = ['--embed', hwnd, String(x), String(y), String(width), String(height), String(relX), String(relY)]
    console.log(`[engine] modo embed Windows — hwnd=${hwnd} pos=(${x},${y}) size=${width}x${height} offset=(${relX},${relY})`)
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

// Oculta el motor (para que no tape modales del renderer)
ipcMain.on('hide-engine-viewport', () => {
  if (!engineStarted) return
  sendToEngine({ cmd: 'set_bounds', x: 0, y: 0, width: 1, height: 1 })
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
  sendToEngine({
    cmd:    'set_bounds',
    x:      Math.round(useBounds.x),
    y:      Math.round(useBounds.y),
    width:  Math.max(1, Math.round(useBounds.width)),
    height: Math.max(1, Math.round(useBounds.height)),
  })
})

// ---------------------------------------------------------------------------
// Helpers de guardado consolidado (.save)
// ---------------------------------------------------------------------------

function ensureSaveExtension(filePath: string): string {
  return path.extname(filePath).toLowerCase() === '.save' ? filePath : `${filePath}.save`
}

/**
 * Recorre un ProjectSaveData y devuelve todos los paths de archivo absolutos
 * que hay que copiar al paquete de assets del archivo .save.
 */
function collectAssetPaths(data: ProjectSaveData): Set<string> {
  const paths = new Set<string>()
  const add = (p: string | null | undefined) => {
    if (p && path.isAbsolute(p) && fs.existsSync(p)) paths.add(p)
  }

  add(data.backgroundPath)
  // Agregar sprites
  if (data.sprites) {
    for (const sprite of data.sprites) {
      add(sprite.path)
    }
  }
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
 * Copia todos los assets al directorio temporal `assets/` y devuelve
 * un mapa de ruta-absoluta → ruta-relativa dentro del paquete .save.
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
 * según el mapa generado por copyAssetsToDir para serializar dentro del .save.
 */
function remapPaths(data: ProjectSaveData, map: Map<string, string>): ProjectSaveData {
  const remap = (p: string | null | undefined): string | null | undefined =>
    p ? (map.get(p) ?? p) : p

  return {
    ...data,
    backgroundPath: remap(data.backgroundPath) as string | null,
    sprites: data.sprites?.map((s) => ({
      name: s.name,
      path: remap(s.path) as string,
    })),
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
 * Crea un archivo .save (ZIP) con `manifest.json` + `assets/`.
 */
function saveProjectToFile(saveFilePath: string, data: ProjectSaveData): boolean {
  const tempRoot = app.getPath('temp')
  const stagingDir = fs.mkdtempSync(path.join(tempRoot, 'rer-save-write-'))
  try {
    const assetsDir = path.join(stagingDir, 'assets')
    const assetPaths = collectAssetPaths(data)
    const pathMap    = copyAssetsToDir(assetPaths, assetsDir)
    const remapped   = remapPaths(data, pathMap)

    const manifestPath = path.join(stagingDir, 'manifest.json')
    fs.writeFileSync(manifestPath, JSON.stringify(remapped, null, 2), 'utf8')

    const zip = new AdmZip()
    zip.addLocalFolder(stagingDir)
    zip.writeZip(saveFilePath)

    console.log(`[editor] Proyecto guardado en ${saveFilePath} (${pathMap.size} assets empaquetados)`)
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
    if (path.isAbsolute(p)) return p
    // El JSON siempre guarda rutas con '/' — normalizamos al separador del OS actual
    const normalized = p.split('/').join(path.sep)
    return path.join(extractedDir, normalized)
  }

  return {
    ...data,
    backgroundPath: resolve(data.backgroundPath) as string | null,
    sprites: data.sprites?.map((s) => ({
      name: s.name,
      path: resolve(s.path) as string,
    })),
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

/**
 * Carga un archivo .save, lo extrae a un directorio temporal y devuelve
 * los datos del proyecto con paths absolutos para consumo del motor.
 */
function loadProjectFromSaveFile(saveFilePath: string): ProjectSaveData | null {
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

    const raw = fs.readFileSync(manifestPath, 'utf8')
    const parsed = JSON.parse(raw) as unknown
    if (!(parsed !== null && typeof parsed === 'object' && 'type' in parsed && 'gameStyle' in parsed)) {
      fs.rmSync(extractDir, { recursive: true, force: true })
      return null
    }

    extractedProjectDirs.add(extractDir)
    return resolveLoadedPaths(parsed as ProjectSaveData, extractDir)
  } catch (err) {
    console.error('[editor] Error al abrir archivo .save:', err)
    fs.rmSync(extractDir, { recursive: true, force: true })
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
  const project = loadProjectFromSaveFile(filePath)
  if (!project) return null
  currentProjectFilePath = filePath
  return { project, filePath }
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

// El renderer envía los bounds del viewport una vez montado (y en cada resize).
// Al primer mensaje arrancamos el motor con las coordenadas correctas.
let engineStarted = false

// Caché de los bounds relativos del viewport (posición dentro del contenido de Electron,
// pre-DPR). Se actualiza en cada 'viewport-bounds'.
let lastRelativeBounds: ViewportBounds | null = null

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
  }

  // En Windows el popup usa coordenadas de pantalla absolutas
  const effectiveBounds = process.platform === 'win32' ? viewportToScreenBounds(bounds) : bounds
  lastEffectiveBounds = effectiveBounds

  if (engineStarted) {
    // Motor corriendo: reposicionar y redimensionar
    // En Windows esto actualiza el punto de referencia del position-tracker
    // (tamaño + posición inicial). El tracker se encarga del movimiento en tiempo real.
    sendToEngine({
      cmd:    'set_bounds',
      x:      Math.round(effectiveBounds.x),
      y:      Math.round(effectiveBounds.y),
      width:  Math.max(1, Math.round(effectiveBounds.width)),
      height: Math.max(1, Math.round(effectiveBounds.height)),
      // Offsets físicos del renderer (sin conversión DPI): el tracker Rust
      // los usa directamente para el offset relativo al área de contenido.
      offset_x: process.platform === 'win32' ? Math.round(bounds.x) : undefined,
      offset_y: process.platform === 'win32' ? Math.round(bounds.y) : undefined,
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
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
