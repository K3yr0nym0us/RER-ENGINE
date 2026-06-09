import { BrowserWindow, screen as electronScreen } from 'electron'
import path from 'path'
import { pathToFileURL } from 'url'

export interface AiAssistantOverlayConfig {
  locale?: 'en' | 'es'
}

/** @deprecated Solo se conserva por compatibilidad con init; ya no restringe la posición. */
export interface ViewportPlacementRect {
  x: number
  y: number
  width: number
  height: number
}

export const FAB_SIZE = 126
const SPEECH_BUBBLE_MAX_WIDTH = 340
const OVERLAY_EDGE_PAD = 14
const INTRO_BUBBLE_HEIGHT = 108
const TAIL_GAP = 10
const COLLAPSED_INTRO_WIDTH = SPEECH_BUBBLE_MAX_WIDTH + OVERLAY_EDGE_PAD
const COLLAPSED_INTRO_HEIGHT = INTRO_BUBBLE_HEIGHT + TAIL_GAP + FAB_SIZE + OVERLAY_EDGE_PAD
const INPUT_BUBBLE_MAX_WIDTH = 450
const INPUT_FAB_GAP = 10
const ANSWER_BUBBLE_HEIGHT = 108
const COLLAPSED_INPUT_WIDTH = INPUT_BUBBLE_MAX_WIDTH + INPUT_FAB_GAP + FAB_SIZE + OVERLAY_EDGE_PAD
const COLLAPSED_INPUT_HEIGHT = FAB_SIZE + OVERLAY_EDGE_PAD
const COLLAPSED_ANSWER_WIDTH = INPUT_BUBBLE_MAX_WIDTH + OVERLAY_EDGE_PAD
const COLLAPSED_ANSWER_HEIGHT = FAB_SIZE + TAIL_GAP + ANSWER_BUBBLE_HEIGHT + OVERLAY_EDGE_PAD

export type AiAssistantLayout = 'intro' | 'thinking' | 'input' | 'answer'
const INITIAL_MARGIN = 16

let getMainWindow: () => BrowserWindow | null = () => null

let overlayWindow: BrowserWindow | null = null
let overlayVisible = false
let overlayLayout: AiAssistantLayout = 'intro'
let pendingConfig: AiAssistantOverlayConfig | null = null
/** Posición absoluta en pantalla (esquina superior izquierda). */
let savedPosition: { x: number; y: number } | null = null
let dragGrabOffset: { x: number; y: number } | null = null
let dragTickTimer: ReturnType<typeof setInterval> | null = null

const DRAG_TICK_MS = 1000 / 120

function stopDragTick(): void {
  if (dragTickTimer) {
    clearInterval(dragTickTimer)
    dragTickTimer = null
  }
}

function preloadPath(): string {
  return path.join(__dirname, '../preload/index.js')
}

function overlayUrl(): string {
  const base = process.env['ELECTRON_RENDERER_URL']
  if (base) {
    const url = new URL(base)
    url.hash = '#/ai-assistant-overlay'
    return url.toString()
  }
  const file = path.join(__dirname, '../renderer/index.html')
  return `${pathToFileURL(file).toString()}#/ai-assistant-overlay`
}

function raiseOverlayAboveEngine(win: BrowserWindow): void {
  if (process.platform === 'win32' || process.platform === 'linux') {
    win.setAlwaysOnTop(true, 'floating')
  }
  win.moveTop()
}

function clearOverlayAboveEngine(win: BrowserWindow): void {
  if (process.platform === 'win32' || process.platform === 'linux') {
    win.setAlwaysOnTop(false)
  }
}

function resolveOverlaySize(layout: AiAssistantLayout): { width: number; height: number } {
  switch (layout) {
    case 'thinking':
      return { width: COLLAPSED_INTRO_WIDTH, height: COLLAPSED_INTRO_HEIGHT }
    case 'answer':
      return { width: COLLAPSED_ANSWER_WIDTH, height: COLLAPSED_ANSWER_HEIGHT }
    case 'input':
      return { width: COLLAPSED_INPUT_WIDTH, height: COLLAPSED_INPUT_HEIGHT }
    default:
      return { width: COLLAPSED_INTRO_WIDTH, height: COLLAPSED_INTRO_HEIGHT }
  }
}

function defaultInitialPosition(width: number, height: number): { x: number; y: number } {
  const parent = getMainWindow()
  if (parent && !parent.isDestroyed()) {
    const cb = parent.getContentBounds()
    return {
      x: cb.x + cb.width - width - INITIAL_MARGIN,
      y: cb.y + cb.height - height - INITIAL_MARGIN,
    }
  }
  const display = electronScreen.getPrimaryDisplay().workArea
  return {
    x: display.x + display.width - width - INITIAL_MARGIN,
    y: display.y + display.height - height - INITIAL_MARGIN,
  }
}

function rememberPosition(win: BrowserWindow): void {
  const [x, y] = win.getPosition()
  savedPosition = { x, y }
}

function setOverlayBounds(
  win: BrowserWindow,
  layout: AiAssistantLayout,
  anchorBottomRight: boolean,
): void {
  const { width, height } = resolveOverlaySize(layout)
  const prev = win.getBounds()

  let x: number
  let y: number

  if (anchorBottomRight && prev.width > 0 && prev.height > 0) {
    x = prev.x + prev.width - width
    y = prev.y + prev.height - height
  } else if (savedPosition) {
    x = savedPosition.x
    y = savedPosition.y
  } else {
    const initial = defaultInitialPosition(width, height)
    x = initial.x
    y = initial.y
  }

  win.setBounds({ x, y, width, height })
  // Windows: resizable debe ser true para que app-region: drag funcione; fijamos tamaño con min/max.
  win.setMinimumSize(width, height)
  win.setMaximumSize(width, height)
  rememberPosition(win)
}

function sendConfigToOverlay(config: AiAssistantOverlayConfig | null): void {
  if (!overlayWindow || overlayWindow.isDestroyed()) return
  overlayWindow.webContents.send('ai-assistant:config', config)
}

async function ensureOverlayReady(win: BrowserWindow): Promise<void> {
  if (win.webContents.isLoading()) {
    await new Promise<void>((resolve) => {
      win.webContents.once('did-finish-load', () => resolve())
    })
  }
}

function getOrCreateOverlayWindow(): BrowserWindow {
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    return overlayWindow
  }

  overlayWindow = new BrowserWindow({
    modal: false,
    frame: false,
    transparent: true,
    backgroundColor: '#00000000',
    hasShadow: false,
    show: false,
    movable: true,
    // En Windows frameless, drag con app-region requiere resizable: true (Electron #30788).
    resizable: true,
    minimizable: false,
    maximizable: false,
    skipTaskbar: true,
    thickFrame: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: preloadPath(),
      sandbox: false,
      contextIsolation: true,
      nodeIntegration: false,
    },
  })

  overlayWindow.setBackgroundColor('#00000000')

  overlayWindow.on('moved', () => {
    if (!overlayWindow || overlayWindow.isDestroyed() || dragGrabOffset) return
    rememberPosition(overlayWindow)
  })

  overlayWindow.on('closed', () => {
    stopDragTick()
    overlayWindow = null
    overlayVisible = false
    overlayLayout = 'intro'
    pendingConfig = null
    savedPosition = null
    dragGrabOffset = null
  })

  void overlayWindow.loadURL(overlayUrl())
  return overlayWindow
}

export function initAiAssistantOverlay(
  getParent: () => BrowserWindow | null,
  _getViewportRect?: () => ViewportPlacementRect | null,
): void {
  getMainWindow = getParent
}

export function updateAiAssistantOverlayConfig(config: AiAssistantOverlayConfig): void {
  pendingConfig = { ...pendingConfig, ...config }
  if (!overlayVisible || !overlayWindow || overlayWindow.isDestroyed()) return
  sendConfigToOverlay(pendingConfig)
}

export async function showAiAssistantOverlay(config: AiAssistantOverlayConfig = {}): Promise<void> {
  pendingConfig = config
  const win = getOrCreateOverlayWindow()
  await ensureOverlayReady(win)

  if (overlayVisible) {
    sendConfigToOverlay(config)
    raiseOverlayAboveEngine(win)
    return
  }

  overlayLayout = 'intro'
  setOverlayBounds(win, 'intro', false)
  sendConfigToOverlay(config)
  if (!win.isVisible()) {
    win.show()
  }
  raiseOverlayAboveEngine(win)
  overlayVisible = true
}

export function hideAiAssistantOverlay(): void {
  overlayVisible = false
  overlayLayout = 'intro'
  pendingConfig = null
  if (!overlayWindow || overlayWindow.isDestroyed()) return
  clearOverlayAboveEngine(overlayWindow)
  sendConfigToOverlay(null)
  overlayWindow.hide()
}

/** Solo reordena z-index; no mueve la ventana. */
export function repositionAiAssistantOverlayIfOpen(): void {
  if (!overlayVisible || !overlayWindow || overlayWindow.isDestroyed()) return
  raiseOverlayAboveEngine(overlayWindow)
}

export function restackAiAssistantOverlayIfOpen(): void {
  repositionAiAssistantOverlayIfOpen()
}

export function setAiAssistantOverlayLayout(layout: AiAssistantLayout): void {
  if (!overlayWindow || overlayWindow.isDestroyed()) return
  overlayLayout = layout
  setOverlayBounds(overlayWindow, layout, true)
  raiseOverlayAboveEngine(overlayWindow)
}

/** @deprecated Use setAiAssistantOverlayLayout */
export function setAiAssistantOverlayExpanded(inputOpen: boolean): void {
  setAiAssistantOverlayLayout(inputOpen ? 'input' : 'intro')
}

function tickFabDrag(): void {
  if (!overlayWindow || overlayWindow.isDestroyed() || !dragGrabOffset) return
  const cursor = electronScreen.getCursorScreenPoint()
  const x = Math.round(cursor.x - dragGrabOffset.x)
  const y = Math.round(cursor.y - dragGrabOffset.y)
  const bounds = overlayWindow.getBounds()
  if (bounds.x === x && bounds.y === y) return
  overlayWindow.setBounds({ x, y, width: bounds.width, height: bounds.height })
}

/** Arrastre del personaje (zona no-drag); el padding usa app-region nativo. */
export function beginAiAssistantFabDrag(): void {
  if (!overlayWindow || overlayWindow.isDestroyed()) return
  stopDragTick()
  const cursor = electronScreen.getCursorScreenPoint()
  const [wx, wy] = overlayWindow.getPosition()
  dragGrabOffset = { x: cursor.x - wx, y: cursor.y - wy }
  tickFabDrag()
  dragTickTimer = setInterval(tickFabDrag, DRAG_TICK_MS)
}

export function endAiAssistantFabDrag(): void {
  stopDragTick()
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    rememberPosition(overlayWindow)
  }
  dragGrabOffset = null
}

export function resendAiAssistantConfig(webContentsId: number): void {
  if (!overlayVisible || !pendingConfig || !overlayWindow || overlayWindow.isDestroyed()) return
  if (overlayWindow.webContents.id !== webContentsId) return
  sendConfigToOverlay(pendingConfig)
}

export function destroyAiAssistantOverlay(): void {
  stopDragTick()
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    clearOverlayAboveEngine(overlayWindow)
    overlayWindow.destroy()
  }
  overlayWindow = null
  overlayVisible = false
  overlayLayout = 'intro'
  pendingConfig = null
  savedPosition = null
  dragGrabOffset = null
}

export function isAiAssistantOverlayVisible(): boolean {
  return overlayVisible
}
