import { BrowserWindow, screen as electronScreen } from 'electron'
import path from 'path'
import { pathToFileURL } from 'url'

import { resolveAppWindowIcon } from './appWindowIcon'
import type { ModalElectronOpenRequest } from '../shared-types/types'
import {
  clampModalElectronContentHeight,
  resolveModalElectronInitialContentHeight,
  resolveModalElectronWidth,
} from './modalElectronSizes'

let lastModalContentWidth = 400

let getMainWindow: () => BrowserWindow | null = () => null
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

export function initModalElectron(getParent: () => BrowserWindow | null): void {
  getMainWindow = getParent
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
  win.center()

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
  raiseModalAboveEngineViewport(win)
  win.focus()
}

export function closeModalElectronWindow(): void {
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
