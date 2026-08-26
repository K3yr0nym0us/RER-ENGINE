import { BrowserWindow, screen as electronScreen, type Rectangle } from 'electron'
import path from 'path'
import { pathToFileURL } from 'url'

export interface AiAssistantOverlayConfig {
  locale?: 'en' | 'es'
}

export const FAB_SIZE = 126
const OVERLAY_EDGE_PAD = 14
const STACK_PAD = 16
const STACK_GAP = 10
const INTRO_BUBBLE_HEIGHT = 120
const TAIL_GAP = 10
const COLLAPSED_INTRO_HEIGHT =
  STACK_PAD + INTRO_BUBBLE_HEIGHT + TAIL_GAP + FAB_SIZE + STACK_PAD
const INPUT_BUBBLE_MAX_WIDTH = 450
const INPUT_FAB_GAP = 10
const ANSWER_BUBBLE_HEIGHT = 140
const COLLAPSED_INPUT_WIDTH = INPUT_BUBBLE_MAX_WIDTH + INPUT_FAB_GAP + FAB_SIZE + OVERLAY_EDGE_PAD
const COLLAPSED_INPUT_HEIGHT = STACK_PAD + FAB_SIZE + STACK_PAD
const COLLAPSED_ANSWER_HEIGHT =
  STACK_PAD + FAB_SIZE + STACK_GAP + ANSWER_BUBBLE_HEIGHT + STACK_PAD

/** Tamaño único: evita saltos al cambiar fase (intro/input/answer). */
const OVERLAY_WIDTH = COLLAPSED_INPUT_WIDTH
const OVERLAY_HEIGHT =
  Math.max(COLLAPSED_INTRO_HEIGHT, COLLAPSED_INPUT_HEIGHT, COLLAPSED_ANSWER_HEIGHT) + 16

export type AiAssistantLayout = 'intro' | 'thinking' | 'input' | 'answer'
const INITIAL_MARGIN = 16

let getMainWindow: () => BrowserWindow | null = () => null

let overlayWindow: BrowserWindow | null = null
let overlayVisible = false
let pendingConfig: AiAssistantOverlayConfig | null = null
/** Offset desde la esquina del área de contenido de la ventana principal. */
let savedOffsetFromContent: { x: number; y: number } | null = null
let overlayHiddenByParentMinimize = false
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

function isMainWindowMinimized(): boolean {
  const parent = getMainWindow()
  if (!parent || parent.isDestroyed()) return false
  return parent.isMinimized()
}

function shouldOverlayBeOnScreen(): boolean {
  if (!overlayVisible || overlayHiddenByParentMinimize) return false
  if (!overlayWindow || overlayWindow.isDestroyed()) return false
  return !isMainWindowMinimized()
}

function raiseOverlayAboveEngine(win: BrowserWindow): void {
  if (!shouldOverlayBeOnScreen()) return
  win.setAlwaysOnTop(true, 'floating')
  win.moveTop()
}

function clearOverlayAboveEngine(win: BrowserWindow): void {
  win.setAlwaysOnTop(false)
}

function getMainContentBounds(): Rectangle | null {
  const parent = getMainWindow()
  if (!parent || parent.isDestroyed()) return null
  return parent.getContentBounds()
}

function defaultInitialOffset(width: number, height: number): { x: number; y: number } {
  const content = getMainContentBounds()
  if (content) {
    return {
      x: content.width - width - INITIAL_MARGIN,
      y: content.height - height - INITIAL_MARGIN,
    }
  }
  const display = electronScreen.getPrimaryDisplay().workArea
  return {
    x: display.width - width - INITIAL_MARGIN,
    y: display.height - height - INITIAL_MARGIN,
  }
}

function updateOffsetFromAbsolutePosition(absX: number, absY: number): void {
  const content = getMainContentBounds()
  if (!content) {
    savedOffsetFromContent = null
    return
  }
  savedOffsetFromContent = { x: absX - content.x, y: absY - content.y }
}

function rememberPosition(win: BrowserWindow): void {
  const [x, y] = win.getPosition()
  updateOffsetFromAbsolutePosition(x, y)
}

function syncOverlayPositionToMain(win: BrowserWindow, preferSaved = true): void {
  if (dragGrabOffset) return

  const width = OVERLAY_WIDTH
  const height = OVERLAY_HEIGHT

  if (!preferSaved || !savedOffsetFromContent) {
    savedOffsetFromContent = defaultInitialOffset(width, height)
  }

  const content = getMainContentBounds()
  if (!content || !savedOffsetFromContent) return

  let x = content.x + savedOffsetFromContent.x
  let y = content.y + savedOffsetFromContent.y

  const display = electronScreen.getDisplayNearestPoint({ x: Math.round(x), y: Math.round(y) })
  const { x: workX, y: workY, width: workW, height: workH } = display.workArea
  x = Math.max(workX, Math.min(x, workX + workW - width))
  y = Math.max(workY, Math.min(y, workY + workH - height))

  const bounds = win.getBounds()
  const nextX = Math.round(x)
  const nextY = Math.round(y)
  if (bounds.x !== nextX || bounds.y !== nextY || bounds.width !== width || bounds.height !== height) {
    win.setBounds({ x: nextX, y: nextY, width, height })
  }

  // Windows: resizable debe ser true para que app-region: drag funcione; fijamos tamaño con min/max.
  win.setMinimumSize(width, height)
  win.setMaximumSize(width, height)
}

function setOverlayBounds(win: BrowserWindow, preferSaved: boolean): void {
  syncOverlayPositionToMain(win, preferSaved)
  rememberPosition(win)
}

function hideOverlayForMainMinimize(): void {
  if (!overlayVisible || !overlayWindow || overlayWindow.isDestroyed()) return
  overlayHiddenByParentMinimize = true
  stopDragTick()
  dragGrabOffset = null
  clearOverlayAboveEngine(overlayWindow)
  overlayWindow.hide()
}

function syncOverlayWithMainWindowState(): void {
  if (!overlayVisible || !overlayWindow || overlayWindow.isDestroyed()) return

  const parent = getMainWindow()
  if (!parent || parent.isDestroyed()) return

  if (parent.isMinimized()) {
    hideOverlayForMainMinimize()
    return
  }

  const shouldShow =
    overlayHiddenByParentMinimize || (!overlayWindow.isVisible() && overlayVisible)
  overlayHiddenByParentMinimize = false

  syncOverlayPositionToMain(overlayWindow, true)

  if (shouldShow && !overlayWindow.isVisible()) {
    overlayWindow.show()
  }
  raiseOverlayAboveEngine(overlayWindow)
}

/** Llamar desde el evento `minimize` / `hide` de la ventana principal (antes que blur/restack). */
export function hideAiAssistantOverlayForMainMinimize(): void {
  hideOverlayForMainMinimize()
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

function attachOverlayToMainWindow(win: BrowserWindow): void {
  const parent = getMainWindow()
  if (parent && !parent.isDestroyed()) {
    win.setParentWindow(parent)
  }
}

function getOrCreateOverlayWindow(): BrowserWindow {
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    attachOverlayToMainWindow(overlayWindow)
    return overlayWindow
  }

  const parent = getMainWindow()

  overlayWindow = new BrowserWindow({
    parent: parent && !parent.isDestroyed() ? parent : undefined,
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
  attachOverlayToMainWindow(overlayWindow)

  overlayWindow.on('moved', () => {
    if (!overlayWindow || overlayWindow.isDestroyed() || dragGrabOffset) return
    rememberPosition(overlayWindow)
  })

  overlayWindow.on('closed', () => {
    stopDragTick()
    overlayWindow = null
    overlayVisible = false
    pendingConfig = null
    savedOffsetFromContent = null
    overlayHiddenByParentMinimize = false
    dragGrabOffset = null
  })

  void overlayWindow.loadURL(overlayUrl())
  return overlayWindow
}

export function initAiAssistantOverlay(getParent: () => BrowserWindow | null): void {
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

  setOverlayBounds(win, false)
  sendConfigToOverlay(config)
  overlayVisible = true

  if (isMainWindowMinimized()) {
    overlayHiddenByParentMinimize = true
    return
  }

  if (!win.isVisible()) {
    win.show()
  }
  raiseOverlayAboveEngine(win)
}

export function hideAiAssistantOverlay(): void {
  overlayVisible = false
  pendingConfig = null
  overlayHiddenByParentMinimize = false
  if (!overlayWindow || overlayWindow.isDestroyed()) return
  clearOverlayAboveEngine(overlayWindow)
  sendConfigToOverlay(null)
  overlayWindow.hide()
}

/** Sigue a la ventana principal (mover/redimensionar/minimizar/restaurar) y reordena z-index. */
export function repositionAiAssistantOverlayIfOpen(): void {
  syncOverlayWithMainWindowState()
}

export function restackAiAssistantOverlayIfOpen(): void {
  if (!shouldOverlayBeOnScreen()) return
  if (!overlayWindow || overlayWindow.isDestroyed()) return
  raiseOverlayAboveEngine(overlayWindow)
}

export function setAiAssistantOverlayLayout(_layout: AiAssistantLayout): void {
  if (!overlayWindow || overlayWindow.isDestroyed()) return
  raiseOverlayAboveEngine(overlayWindow)
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
  pendingConfig = null
  savedOffsetFromContent = null
  overlayHiddenByParentMinimize = false
  dragGrabOffset = null
}

export function isAiAssistantOverlayVisible(): boolean {
  return overlayVisible
}
