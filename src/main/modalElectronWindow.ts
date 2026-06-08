import { BrowserWindow, screen as electronScreen } from 'electron'
import path from 'path'
import { pathToFileURL } from 'url'

import { resolveAppWindowIcon } from './appWindowIcon'
import type { ModalElectronOpenRequest } from '../shared-types/types'
import {
  clampModalElectronContentHeight,
  MODAL_VIEWPORT_CORNER_COMPONENT_KEYS,
  MODAL_VIEWPORT_CORNER_OFFSET,
  resolveModalElectronInitialContentHeight,
  resolveModalElectronWidth,
} from './modalElectronSizes'

let lastModalContentWidth = 400

let getMainWindow: () => BrowserWindow | null = () => null
let getViewportScreenOrigin: () => { x: number; y: number } | null = () => null
let modalWindow: BrowserWindow | null = null
let pendingRenderPayload: ModalElectronOpenRequest | null = null

/** Modal visible (no oculta por closeModalElectronWindow). */
function isModalElectronShown(): boolean {
  return modalWindow != null && !modalWindow.isDestroyed() && modalWindow.isVisible()
}

/**
 * La ventana del motor (winit) es un HWND/X11 separado, hermano del shell Electron.
 * Sin esto, un clic en el viewport la eleva por encima de la modal aunque el motor tenga foco.
 */
function raiseModalAboveEngineViewport(win: BrowserWindow): void {
  if (process.platform === 'win32' || process.platform === 'linux') {
    win.setAlwaysOnTop(true, 'floating')
  }
  win.moveTop()
}

function clearModalAboveEngineViewport(win: BrowserWindow): void {
  if (process.platform === 'win32' || process.platform === 'linux') {
    win.setAlwaysOnTop(false)
  }
}

export function initModalElectron(
  getParent: () => BrowserWindow | null,
  getViewportOrigin?: () => { x: number; y: number } | null,
): void {
  getMainWindow = getParent
  getViewportScreenOrigin = getViewportOrigin ?? (() => null)
}

/**
 * Ancla la modal a la esquina superior izquierda del viewport del motor.
 * En Windows/Linux las modales hijas usan coordenadas relativas al padre, no de pantalla.
 */
function placeModalAtViewportCorner(win: BrowserWindow): void {
  const parent = getMainWindow()
  if (!parent || parent.isDestroyed()) {
    win.center()
    return
  }

  const offset = MODAL_VIEWPORT_CORNER_OFFSET
  const origin = getViewportScreenOrigin()
  const content = parent.getContentBounds()

  const screenX = (origin?.x ?? content.x) + offset
  const screenY = (origin?.y ?? content.y) + offset

  const frame = win.getBounds()
  const outerWidth = frame.width
  const outerHeight = frame.height

  let x = screenX
  let y = screenY

  if (process.platform === 'win32' || process.platform === 'linux') {
    const parentBounds = parent.getBounds()
    x = screenX - parentBounds.x
    y = screenY - parentBounds.y
    x = Math.max(0, Math.min(x, parentBounds.width - outerWidth))
    y = Math.max(0, Math.min(y, parentBounds.height - outerHeight))
  } else {
    const display = getDisplayForModal()
    const { x: workX, y: workY, width: workW, height: workH } = display.workArea
    x = Math.max(workX, Math.min(screenX, workX + workW - outerWidth))
    y = Math.max(workY, Math.min(screenY, workY + workH - outerHeight))
  }

  win.setBounds({
    x: Math.round(x),
    y: Math.round(y),
    width: outerWidth,
    height: outerHeight,
  })
}

/** Respaldo cuando el foco sale al viewport nativo del motor (blur de la ventana principal). */
export function restackModalElectronIfOpen(): void {
  if (modalWindow && !modalWindow.isDestroyed() && modalWindow.isVisible()) {
    raiseModalAboveEngineViewport(modalWindow)
  }
}

function preloadPath(): string {
  return path.join(__dirname, '../preload/index.js')
}

function modalElectronUrl(): string {
  const base = process.env['ELECTRON_RENDERER_URL']
  if (base) {
    const url = new URL(base)
    url.hash = '#/modal-electron'
    return url.toString()
  }
  const file = path.join(__dirname, '../renderer/index.html')
  return `${pathToFileURL(file).toString()}#/modal-electron`
}

function getOrCreateModalWindow(): BrowserWindow {
  const parent = getMainWindow()
  if (modalWindow && !modalWindow.isDestroyed()) {
    return modalWindow
  }

  const windowIcon = resolveAppWindowIcon()

  modalWindow = new BrowserWindow({
    parent: parent ?? undefined,
    modal: parent != null,
    show: false,
    autoHideMenuBar: true,
    resizable: false,
    minimizable: false,
    maximizable: false,
    backgroundColor: '#1a1a2e',
    ...(windowIcon ? { icon: windowIcon } : {}),
    webPreferences: {
      preload: preloadPath(),
      sandbox: false,
      contextIsolation: true,
      nodeIntegration: false,
    },
  })

  modalWindow.on('close', (e) => {
    e.preventDefault()
    closeModalElectronWindow()
  })

  modalWindow.on('closed', () => {
    modalWindow = null
    pendingRenderPayload = null
  })

  void modalWindow.loadURL(modalElectronUrl())
  return modalWindow
}

function sendRenderToModal(payload: ModalElectronOpenRequest | null): void {
  if (!modalWindow || modalWindow.isDestroyed()) return
  modalWindow.webContents.send('modal-electron:render', payload)
}

export function sendPatchToModal(data: {
  handlerId: string
  playerUiEditorState?: unknown
  entityPropertiesState?: unknown
}): void {
  if (!modalWindow || modalWindow.isDestroyed()) return
  modalWindow.webContents.send('modal-electron:patch', data)
}

async function ensureModalReady(win: BrowserWindow): Promise<void> {
  if (win.webContents.isLoading()) {
    await new Promise<void>((resolve) => {
      win.webContents.once('did-finish-load', () => resolve())
    })
  }
}

function getDisplayForModal(): Electron.Display {
  const parent = getMainWindow()
  return parent
    ? electronScreen.getDisplayMatching(parent.getBounds())
    : electronScreen.getPrimaryDisplay()
}

export function applyModalElectronContentHeight(contentHeight: number): void {
  if (!modalWindow || modalWindow.isDestroyed()) return
  const display = getDisplayForModal()
  const height = clampModalElectronContentHeight(
    contentHeight,
    display.workAreaSize.height,
  )
  // Solo cambia el tamaño; no recentrar (el usuario puede haber movido la ventana
  // o el contenido crece al expandir acordeones sin saltar de posición).
  modalWindow.setContentSize(lastModalContentWidth, height)
  if (isModalElectronShown()) {
    raiseModalAboveEngineViewport(modalWindow)
  }
}

export async function openModalElectronWindow(payload: ModalElectronOpenRequest): Promise<void> {
  const display = getDisplayForModal()
  const { width } = resolveModalElectronWidth(payload.size, display.workAreaSize.width)
  lastModalContentWidth = width

  pendingRenderPayload = payload
  const win = getOrCreateModalWindow()
  const windowIcon = resolveAppWindowIcon()
  if (windowIcon) win.setIcon(windowIcon)
  win.setTitle(payload.title)
  const initialHeight = resolveModalElectronInitialContentHeight(
    payload.componentKey,
    display.workAreaSize.height,
  )
  win.setContentSize(width, initialHeight)
  const isViewportCornerModal = MODAL_VIEWPORT_CORNER_COMPONENT_KEYS.has(payload.componentKey)

  const resizable = payload.resizable === true
  win.setResizable(resizable)
  if (resizable) {
    win.setMinimumSize(720, 480)
  } else {
    win.setMinimumSize(280, 120)
  }

  await ensureModalReady(win)
  sendRenderToModal(payload)
  if (!win.isVisible()) {
    win.show()
  }
  if (isViewportCornerModal) {
    // Tras show(): en Windows la modal hija puede recentrarse si se posicionó antes.
    placeModalAtViewportCorner(win)
  } else {
    win.center()
  }
  raiseModalAboveEngineViewport(win)
  win.focus()
}

export function closeModalElectronWindow(): void {
  const componentKey = pendingRenderPayload?.componentKey
  const parent = getMainWindow()
  if (componentKey && parent && !parent.isDestroyed()) {
    parent.webContents.send('modal-electron:closed', { componentKey })
  }
  if (modalWindow && !modalWindow.isDestroyed()) {
    clearModalAboveEngineViewport(modalWindow)
    sendRenderToModal(null)
    modalWindow.hide()
  }
  pendingRenderPayload = null
}

export function resendPendingRenderToModal(webContentsId: number): void {
  if (!pendingRenderPayload || !modalWindow || modalWindow.isDestroyed()) return
  if (modalWindow.webContents.id !== webContentsId) return
  sendRenderToModal(pendingRenderPayload)
}

export function destroyModalElectronWindow(): void {
  if (modalWindow && !modalWindow.isDestroyed()) {
    clearModalAboveEngineViewport(modalWindow)
    modalWindow.destroy()
  }
  modalWindow = null
  pendingRenderPayload = null
}
