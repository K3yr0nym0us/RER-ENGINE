import { app, BrowserWindow, ipcMain, dialog, Menu, session } from 'electron';
import { spawn, type ChildProcess } from 'child_process';
import path from 'path';

import type { OpenProjectResult, ProjectType } from '../shared-types/types';
import { resolveAppWindowIcon } from './appWindowIcon';
import { registerDevToolsShortcut } from './devToolsShortcut';
import { isEngineStartPayload } from './isEngineStartPayload';

/** Logs informativos del host Electron: solo con RER_VERBOSE_ELECTRON=1. */
const verboseElectronLogs = process.env.RER_VERBOSE_ELECTRON === '1';
function electronLog(...args: unknown[]): void {
  if (verboseElectronLogs) console.log(...args);
}

// Sin GPU hardware disponible: deshabilitar el proceso GPU de Chromium
// para evitar spam de viz_main_impl / command_buffer_proxy_impl
app.commandLine.appendSwitch('disable-gpu');
app.commandLine.appendSwitch('disable-software-rasterizer');

// ---------------------------------------------------------------------------
// Estado del launcher
// ---------------------------------------------------------------------------
let mainWindow: BrowserWindow | null = null
/** `null` al abrir un `.save`: Rust decide el motor leyendo el manifest. */
let currentProjectType: ProjectType | null = null
let currentProjectFilePath: string | null = null
let currentLocale: 'en' | 'es' = 'en'

/** El editor Rust se lanza como hijo: Electron sigue vivo para capturar su consola. */
let editorFrameProcess: ChildProcess | null = null
let editorFrameLaunched = false
let editorFrameStopping = false

/** Líneas habituales de Vulkan en Windows que no indican fallo del motor. */
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

// ---------------------------------------------------------------------------
// Ventana del launcher (única UI de Electron)
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

  registerDevToolsShortcut(mainWindow)

  if (process.env['ELECTRON_RENDERER_URL']) {
    const url = new URL(process.env['ELECTRON_RENDERER_URL'])
    url.searchParams.set('locale', currentLocale)
    mainWindow.loadURL(url.toString())
  } else {
    mainWindow.loadFile(
      path.join(__dirname, '../renderer/index.html'),
      { query: { locale: currentLocale } },
    )
  }

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

function hideMainWindowForEditor(): void {
  if (!mainWindow || mainWindow.isDestroyed()) return
  mainWindow.hide()
  mainWindow.setSkipTaskbar(true)
}

function failEditorAndQuit(message: string): void {
  if (mainWindow && !mainWindow.isDestroyed()) mainWindow.hide()
  dialog.showErrorBox('RER-ENGINE', message)
  app.quit()
}

// ---------------------------------------------------------------------------
// Proceso hijo: editor Rust/egui
// ---------------------------------------------------------------------------
function editorFrameBinaryName(): string {
  return process.platform === 'win32' ? 'rer_engine_editor.exe' : 'rer_engine_editor'
}

/**
 * Muestra solo lo que la consola egui NO muestra.
 *
 * Los eventos del motor (`{"event": ...}`) y los mensajes de control
 * (`{"type": ...}`) ya salen en la consola del editor, traducidos a texto
 * legible. Reenviarlos aquí inunda la terminal de `yarn dev` con
 * `debug_metrics`, hovers y snapshots del `.save`. Solo se conserva el
 * texto suelto (logs fuera del sink, errores de arranque, panics).
 */
function forwardEditorOutput(line: string): void {
  const trimmed = line.trim()
  if (!trimmed) return

  if (trimmed.startsWith('{')) {
    try {
      const msg = JSON.parse(trimmed) as Record<string, unknown>
      if (typeof msg.event === 'string' || typeof msg.type === 'string') return
    } catch {
      // Empieza por '{' pero no es JSON: se trata como texto.
    }
  }
  console.log('[editor]', trimmed)
}

function startEditorFrame(): void {
  if (editorFrameLaunched) return
  editorFrameLaunched = true

  const profile = (process.env.RER_ENGINE_PROFILE || 'debug').trim()
  const binaryName = editorFrameBinaryName()
  const editorPath = app.isPackaged
    ? path.join(process.resourcesPath, 'engine', binaryName)
    : path.join(app.getAppPath(), 'src', 'main', 'Engine', 'target', profile, binaryName)

  // Proyecto nuevo: el launcher elige el motor. Con `.save` solo va la ruta:
  // Rust extrae el ZIP, lee el manifest y decide 2D/3D.
  const args: string[] = []
  if (currentProjectType) {
    args.push('--project-type', currentProjectType)
  }
  if (currentProjectFilePath) args.push('--save-path', currentProjectFilePath)
  args.push('--locale', currentLocale)
  args.push('--electron-host')

  editorFrameProcess = spawn(editorPath, args, {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env },
  })

  electronLog(`[editor] ventana egui: ${editorPath}`)
  electronLog(`[editor] args: ${args.join(' ')}`)

  editorFrameProcess.stdout?.on('data', (data: Buffer) => {
    for (const line of data.toString('utf8').split('\n')) forwardEditorOutput(line)
  })
  editorFrameProcess.stderr?.on('data', (data: Buffer) => {
    for (const line of data.toString('utf8').split('\n')) {
      if (isBenignEngineStderrLine(line)) continue
      forwardEditorOutput(line)
    }
  })

  editorFrameProcess.on('close', (code) => {
    electronLog(`[editor] proceso terminado con código ${code}`)
    editorFrameProcess = null

    if (editorFrameStopping) {
      editorFrameStopping = false
      return
    }
    if (code === 0) {
      app.quit()
      return
    }
    failEditorAndQuit(`El editor Rust terminó inesperadamente (código ${code}).`)
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

  // El editor cierra su motor en el Drop; aquí solo garantizamos que termine.
  setTimeout(() => {
    if (!processToStop.killed && processToStop.exitCode === null) {
      processToStop.kill()
    }
  }, 800)
}

// ---------------------------------------------------------------------------
// IPC del launcher: solo idioma, diálogo de .save y arranque
// ---------------------------------------------------------------------------

/** El idioma elegido en el launcher se pasa al editor como `--locale`. */
ipcMain.on('set-locale', (_event, locale: unknown) => {
  currentLocale = locale === 'es' ? 'es' : 'en'
})

ipcMain.handle('open-project-dialog', async (): Promise<OpenProjectResult | null> => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    title:      'Abrir proyecto',
    filters:    [{ name: 'Proyecto RER Save', extensions: ['save'] }],
    properties: ['openFile'],
  })
  if (result.canceled || !result.filePaths[0]) return null
  return { filePath: result.filePaths[0] }
})

ipcMain.on('set-game-style', (_event, arg: unknown) => {
  if (!isEngineStartPayload(arg)) return

  if (arg.projectType === 'save') {
    if (typeof arg.save_path !== 'string' || arg.save_path.trim().length === 0) return
    currentProjectFilePath = arg.save_path.trim()
    currentProjectType = null
  } else {
    currentProjectType = arg.projectType
    currentProjectFilePath = null
  }

  hideMainWindowForEditor()
  setImmediate(() => startEditorFrame())
})

// ---------------------------------------------------------------------------
// Ciclo de vida de la app
// ---------------------------------------------------------------------------
app.whenReady().then(() => {
  if (!app.isPackaged) {
    process.env.ELECTRON_DISABLE_SECURITY_WARNINGS = 'true'
  }

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

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createMainWindow()
  })
})

app.on('before-quit', () => {
  stopEditorFrame()
})

app.on('window-all-closed', () => {
  app.quit()
})
