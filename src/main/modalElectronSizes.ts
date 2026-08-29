import type { ModalElectronSize } from '../shared-types/types'

/** Anchos de contenido alineados a Bootstrap 5 `.modal-dialog` (max-width). */
const CONTENT_WIDTH: Record<ModalElectronSize, number> = {
  sm: 300,
  md: 500,
  lg: 800,
  xl: 1140,
  xxl: 1680,
}

const HORIZONTAL_PADDING = 48

/** Altura mínima del área web antes de medir el DOM (evita ventana gigante al abrir). */
export const MODAL_ELECTRON_MIN_CONTENT_HEIGHT = 80

/** Modales de editor de código / nodos: fracción mínima de la pantalla. */
export const MODAL_TALL_CONTENT_HEIGHT_RATIO = 0.5

export const MODAL_TALL_COMPONENT_KEYS = new Set([
  'SceneScriptEditorModalBody',
  'ScriptEditorModalBody',
  'VisualScriptingModalBody',
])

/** Modales anclados a la esquina superior izquierda del viewport del motor (no centrados). */
export const MODAL_VIEWPORT_CORNER_COMPONENT_KEYS = new Set(['EntityPropertiesModalBody'])

/** Overlay bloqueante a tamaño de la ventana principal. */
export const MODAL_BLOCKING_OVERLAY_COMPONENT_KEYS = new Set(['ProjectSaveBlockingModalBody'])

/** Margen respecto a la esquina superior izquierda del viewport del motor. */
export const MODAL_VIEWPORT_CORNER_OFFSET = 48

export function resolveModalElectronInitialContentHeight(
  componentKey: string,
  screenHeight: number,
): number {
  if (!MODAL_TALL_COMPONENT_KEYS.has(componentKey)) {
    return MODAL_ELECTRON_MIN_CONTENT_HEIGHT
  }
  const target = Math.floor(screenHeight * MODAL_TALL_CONTENT_HEIGHT_RATIO)
  return clampModalElectronContentHeight(target, screenHeight)
}

export function resolveModalElectronWidth(
  size: ModalElectronSize | undefined,
  screenWidth: number,
): { width: number; contentWidth: number } {
  const key: ModalElectronSize = size ?? 'md'
  const contentWidth = CONTENT_WIDTH[key]
  const maxWidth = Math.floor(screenWidth * 0.92)
  const width = Math.min(contentWidth + HORIZONTAL_PADDING, maxWidth)
  return {
    width: Math.max(280, Math.round(width)),
    contentWidth,
  }
}

export function clampModalElectronContentHeight(
  contentHeight: number,
  screenHeight: number,
): number {
  const maxContent = Math.floor(screenHeight * 0.88)
  return Math.min(maxContent, Math.max(MODAL_ELECTRON_MIN_CONTENT_HEIGHT, Math.round(contentHeight)))
}
