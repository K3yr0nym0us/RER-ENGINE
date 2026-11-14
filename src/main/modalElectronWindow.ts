import { BrowserWindow, screen as electronScreen } from 'electron'
import path from 'path'
import { pathToFileURL } from 'url'

import { resolveAppWindowIcon } from './appWindowIcon'
import { registerDevToolsShortcut } from './devToolsShortcut'
import type { ModalElectronOpenRequest } from '../shared-types/types'
import {
  clampModalElectronContentHeight,
  centerRectInBounds,
  MODAL_BLOCKING_OVERLAY_COMPONENT_KEYS,
  MODAL_ELECTRON_MIN_CONTENT_HEIGHT,
  MODAL_VIEWPORT_CORNER_COMPONENT_KEYS,
  MODAL_VIEWPORT_CORNER_OFFSET,
  resolveModalElectronInitialContentHeight,
  resolveModalElectronWidth,
  type ModalAnchorRect,
} from './modalElectronSizes'

let lastModalContentWidth = 400
/** Si la ventana modal hija usa `modal: true` de Electron (bloquea la ventana padre). */
let modalWindowUsesNativeModal = false

let getMainWindow: () => BrowserWindow | null = () => null
let getViewportScreenOrigin: () => { x: number; y: number } | null = () => null
let modalWindow: BrowserWindow | null = null
let pendingRenderPayload: ModalElectronOpenRequest | null = null
/** Rect (DIP) de la ventana del motor sobre la que centrar la próxima modal. */
let modalAnchorRect: ModalAnchorRect | null = null
/** La modal visible está centrada sobre la ventana del motor (recentrar al cambiar alto). */
let modalCenteredOnEngine = false

/** Ancla la próxima modal al centro de la ventana nativa del motor. */
export function setModalElectronAnchorRect(rect: ModalAnchorRect | null): void {
  modalAnchorRect = rect
}

/** Modal visible (no oculta por closeModalElectronWindow). */
function isModalElectronShown(): boolean {
  return modalWindow != null && !modalWindow.isDestroyed() && modalWindow.isVisible()
}

/**
 * La ventana del motor (winit) es un HWND separado, hermano del shell Electron.
 * Sin esto, un clic en el viewport la eleva por encima de la modal aunque el motor tenga foco.
 */
function raiseModalAboveEngineViewport(win: BrowserWindow): void {
  win.setAlwaysOnTop(true, 'floating')
  win.moveTop()
}

function clearModalAboveEngineViewport(win: BrowserWindow): void {
  win.setAlwaysOnTop(false)
}

export function initModalElectron(
  getParent: () => BrowserWindow | null,
  getViewportOrigin?: () => { x: number; y: number } | null,
): void {
  getMainWindow = getParent
  getViewportScreenOrigin = getViewportOrigin ?? (() => null)
}

/**
 * Ancla la modal a la esquina superior izquierda del viewport del motor (coordenadas de pantalla).
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

  const display = electronScreen.getDisplayNearestPoint({
    x: Math.round(screenX),
    y: Math.round(screenY),
  })
  const { x: workX, y: workY, width: workW, height: workH } = display.workArea
  const x = Math.max(workX, Math.min(screenX, workX + workW - outerWidth))
  const y = Math.max(workY, Math.min(screenY, workY + workH - outerHeight))

  win.setBounds({
    x: Math.round(x),
    y: Math.round(y),
    width: outerWidth,
    height: outerHeight,
  })
}

/**
 * Centra la modal en el rect (DIP) de la ventana del motor, recortando al área de trabajo.
 */
function placeModalCenteredOnRect(win: BrowserWindow, rect: ModalAnchorRect): void {
  const frame = win.getBounds()
  const display = electronScreen.getDisplayNearestPoint({
    x: Math.round(rect.x + rect.width * 0.5),
    y: Math.round(rect.y + rect.height * 0.5),
  })
  const { x, y } = centerRectInBounds(rect, frame, display.workArea)

  win.setBounds({
    x,
    y,
    width: frame.width,
    height: frame.height,
  })
}

/** Cubre la ventana principal del editor (overlay bloqueante). */
function placeModalAsBlockingOverlay(win: BrowserWindow): void {
  const parent = getMainWindow()
  if (!parent || parent.isDestroyed()) {
    win.center()
    return
  }

  const bounds = parent.getBounds()
  win.setBounds({
    x: Math.round(bounds.x),
    y: Math.round(bounds.y),
    width: Math.round(bounds.width),
    height: Math.round(bounds.height),
  })
}

/** Recentra modales ancladas al viewport si la ventana principal se movió o redimensionó. */
export function repositionViewportCornerModalIfOpen(): void {
  if (!modalWindow || modalWindow.isDestroyed() || !modalWindow.isVisible()) return
  if (!pendingRenderPayload) return
  if (MODAL_BLOCKING_OVERLAY_COMPONENT_KEYS.has(pendingRenderPayload.componentKey)) {
    placeModalAsBlockingOverlay(modalWindow)
    raiseModalAboveEngineViewport(modalWindow)
    return
  }
  if (!MODAL_VIEWPORT_CORNER_COMPONENT_KEYS.has(pendingRenderPayload.componentKey)) return
  placeModalAtViewportCorner(modalWindow)
  raiseModalAboveEngineViewport(modalWindow)
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

function wantsNativeElectronModal(payload: ModalElectronOpenRequest): boolean {
  return (
    payload.blockingOverlay === true
    || MODAL_BLOCKING_OVERLAY_COMPONENT_KEYS.has(payload.componentKey)
  )
}

function getOrCreateModalWindow(wantsNativeModal: boolean): BrowserWindow {
  const parent = getMainWindow()
  if (modalWindow && !modalWindow.isDestroyed()) {
    if (modalWindowUsesNativeModal !== wantsNativeModal) {
      clearModalAboveEngineViewport(modalWindow)
      modalWindow.destroy()
      modalWindow = null
    }
  }
  if (modalWindow && !modalWindow.isDestroyed()) {
    return modalWindow
  }

  const windowIcon = resolveAppWindowIcon()
  modalWindowUsesNativeModal = wantsNativeModal

  // Con el editor Rust activo la ventana React está oculta; una modal hija de
  // una ventana oculta puede no mostrarse, así que en ese caso va sin parent.
  const usableParent =
    parent && !parent.isDestroyed() && parent.isVisible() ? parent : undefined

  const win = new BrowserWindow({
    parent: usableParent,
    modal: wantsNativeModal && usableParent != null,
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
      // La ventana modal puede estar oculta mientras se precarga: que el renderer
      // no se duerma y la primera apertura sea inmediata.
      backgroundThrottling: false,
    },
  })
  modalWindow = win
  registerDevToolsShortcut(win)

  win.on('close', (e) => {
    e.preventDefault()
    if (pendingRenderPayload?.blockingOverlay) return
    if (pendingRenderPayload?.componentKey === 'ProjectSaveBlockingModalBody') return
    closeModalElectronWindow()
  })

  win.on('closed', () => {
    modalWindow = null
    pendingRenderPayload = null
  })

  void win.loadURL(modalElectronUrl())
  return win
}

/**
 * Crea y carga la ventana modal oculta al arrancar el editor.
 *
 * La primera apertura deja de pagar la creación del BrowserWindow + carga del
 * bundle de React; luego `openModalElectronWindow` reutiliza esta instancia.
 */
export function preloadModalElectronWindow(): void {
  if (modalWindow && !modalWindow.isDestroyed()) return
  getOrCreateModalWindow(false)
  console.log('[modal] ventana precargada')
}

function sendRenderToModal(payload: ModalElectronOpenRequest | null): void {
  if (!modalWindow || modalWindow.isDestroyed()) return
  modalWindow.webContents.send('modal-electron:render', payload)
}

export function sendPatchToModal(data: {
  handlerId: string
  playerUiEditorState?: unknown
  entityPropertiesState?: unknown
  socketConfigModalState?: unknown
  models?: unknown
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
  if (modalAnchorRect) {
    return electronScreen.getDisplayNearestPoint({
      x: Math.round(modalAnchorRect.x + modalAnchorRect.width * 0.5),
      y: Math.round(modalAnchorRect.y + modalAnchorRect.height * 0.5),
    })
  }
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
  // Excepción: la modal de salida debe seguir centrada sobre la ventana del motor.
  if (modalCenteredOnEngine && modalAnchorRect) {
    placeModalCenteredOnRect(modalWindow, modalAnchorRect)
  }
  if (isModalElectronShown()) {
    raiseModalAboveEngineViewport(modalWindow)
  }
}

export async function openModalElectronWindow(payload: ModalElectronOpenRequest): Promise<void> {
  const display = getDisplayForModal()
  const isBlockingOverlay =
    payload.blockingOverlay === true
    || MODAL_BLOCKING_OVERLAY_COMPONENT_KEYS.has(payload.componentKey)

  pendingRenderPayload = payload
  const wantsNativeModal = wantsNativeElectronModal(payload)
  const win = getOrCreateModalWindow(wantsNativeModal)
  const windowIcon = resolveAppWindowIcon()
  if (windowIcon) win.setIcon(windowIcon)
  win.setTitle(payload.title)
  win.setResizable(false)
  win.setMinimumSize(280, 120)

  if (isBlockingOverlay) {
    await ensureModalReady(win)
    sendRenderToModal(payload)
    placeModalAsBlockingOverlay(win)
    if (!win.isVisible()) {
      win.show()
    }
    raiseModalAboveEngineViewport(win)
    win.focus()
    return
  }

  const { width } = resolveModalElectronWidth(payload.size, display.workAreaSize.width)
  lastModalContentWidth = width
  const initialHeight = resolveModalElectronInitialContentHeight(
    payload.componentKey,
    display.workAreaSize.height,
  )
  // Restaurar tamaño si una sesión previa dejó la ventana a pantalla completa.
  const parent = getMainWindow()
  if (parent && !parent.isDestroyed()) {
    const parentBounds = parent.getBounds()
    const winBounds = win.getBounds()
    if (
      winBounds.width >= parentBounds.width - 4
      && winBounds.height >= parentBounds.height - 4
    ) {
      win.setContentSize(width, initialHeight)
      win.center()
    }
  }
  win.setContentSize(width, initialHeight)
  const isViewportCornerModal = MODAL_VIEWPORT_CORNER_COMPONENT_KEYS.has(payload.componentKey)

  const resizable = payload.resizable === true
  win.setResizable(resizable)
  if (resizable) {
    win.setMinimumSize(720, 480)
  }

  await ensureModalReady(win)
  sendRenderToModal(payload)
  if (!win.isVisible()) {
    win.show()
  }
  if (modalAnchorRect) {
    // Modal de salida: centrada sobre la ventana nativa del editor, no sobre la pantalla.
    placeModalCenteredOnRect(win, modalAnchorRect)
    modalCenteredOnEngine = true
  } else if (isViewportCornerModal) {
    modalCenteredOnEngine = false
    // Tras show(): en Windows la modal hija puede recentrarse si se posicionó antes.
    placeModalAtViewportCorner(win)
  } else {
    modalCenteredOnEngine = false
    win.center()
  }
  raiseModalAboveEngineViewport(win)
  // Propiedades de entidad: no robar foco; el usuario sigue interactuando con el viewport.
  if (!isViewportCornerModal) {
    win.focus()
  }
}

export function closeModalElectronWindow(): void {
  const componentKey = pendingRenderPayload?.componentKey
  const wasBlockingOverlay =
    pendingRenderPayload?.blockingOverlay === true
    || (pendingRenderPayload?.componentKey != null
      && MODAL_BLOCKING_OVERLAY_COMPONENT_KEYS.has(pendingRenderPayload.componentKey))
  modalAnchorRect = null
  modalCenteredOnEngine = false
  const parent = getMainWindow()
  if (componentKey && parent && !parent.isDestroyed()) {
    parent.webContents.send('modal-electron:closed', { componentKey })
  }
  if (modalWindow && !modalWindow.isDestroyed()) {
    clearModalAboveEngineViewport(modalWindow)
    sendRenderToModal(null)
    if (wasBlockingOverlay) {
      modalWindow.setContentSize(348, MODAL_ELECTRON_MIN_CONTENT_HEIGHT)
      modalWindow.center()
    }
    modalWindow.hide()
  }
  pendingRenderPayload = null
}

export function resendPendingRenderToModal(webContentsId: number): void {
  if (!pendingRenderPayload || !modalWindow || modalWindow.isDestroyed()) return
  if (modalWindow.webContents.id !== webContentsId) return
  sendRenderToModal(pendingRenderPayload)
}

/**
 * Reenvía un evento del engine a la modal worldgen abierta.
 *
 * La ventana modal no recibe `engine:event` (solo va a la ventana principal);
 * la modal del generador de mundos lo necesita para la lista de modelos
 * (`models_list`). Solo se reenvía mientras esa modal esté visible para no
 * acumular eventos en el buffer del preload.
 */
export function forwardEngineEventToModal(event: unknown): void {
  if (pendingRenderPayload?.componentKey !== 'WorldGenModalBody') return
  if (!modalWindow || modalWindow.isDestroyed()) return
  modalWindow.webContents.send('engine:event', event)
}

export function destroyModalElectronWindow(): void {
  if (modalWindow && !modalWindow.isDestroyed()) {
    clearModalAboveEngineViewport(modalWindow)
    modalWindow.destroy()
  }
  modalWindow = null
  modalWindowUsesNativeModal = false
  pendingRenderPayload = null
  modalAnchorRect = null
  modalCenteredOnEngine = false
}
