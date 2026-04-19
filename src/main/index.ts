import { app, BrowserWindow, ipcMain, dialog, Menu } from 'electron'
import { spawn, ChildProcess } from 'child_process'
import path from 'path'
import type { EngineCommand, EngineEvent } from '../shared/types'

// Sin GPU hardware disponible: deshabilitar el proceso GPU de Chromium
// para evitar spam de viz_main_impl / command_buffer_proxy_impl
app.commandLine.appendSwitch('disable-gpu')
app.commandLine.appendSwitch('disable-software-rasterizer')

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
  }

  // LIBGL_ALWAYS_SOFTWARE=1 asegura que EGL use llvmpipe en vez de buscar DRI3.
  // EGL_LOG_LEVEL=fatal silencia el warning "DRI3 error" de libEGL.
  engineProcess = spawn(enginePath, engineArgs, {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: {
      ...process.env,
      WAYLAND_DISPLAY: '',
      GDK_BACKEND: 'x11',
      LIBGL_ALWAYS_SOFTWARE: '1',
      EGL_LOG_LEVEL: 'fatal',
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
    engineProcess.stdin.write(JSON.stringify(cmd) + '\n')
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

// El renderer envía los bounds del viewport una vez montado (y en cada resize).
// Al primer mensaje arrancamos el motor con las coordenadas correctas.
let engineStarted = false

ipcMain.on('viewport-bounds', (_event, bounds: ViewportBounds) => {
  // Si el proceso murió, permitir relanzar
  if (engineStarted && !engineProcess) {
    engineStarted = false
  }

  if (engineStarted) {
    // Motor corriendo: reposicionar y redimensionar
    sendToEngine({
      cmd: 'set_bounds',
      x:      Math.round(bounds.x),
      y:      Math.round(bounds.y),
      width:  Math.max(1, Math.round(bounds.width)),
      height: Math.max(1, Math.round(bounds.height)),
    })
    return
  }
  // Primera vez (o relanzar tras muerte): arrancar el motor
  engineStarted = true
  startEngine(bounds)
})

// ---------------------------------------------------------------------------
// Ciclo de vida de la app
// ---------------------------------------------------------------------------
app.whenReady().then(() => {
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
