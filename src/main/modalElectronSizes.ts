import type { ModalElectronSize } from '../shared-types/types'

/** Rect (DIP) de la ventana del motor sobre la que se ancla la modal de salida. */
export interface ModalAnchorRect {
  x: number
  y: number
  width: number
  height: number
}

/** Convierte el rect físico que manda el editor Rust (px + escala) a DIP de Electron. */
export function engineWindowRectToDip(value: unknown): ModalAnchorRect | null {
  if (!value || typeof value !== 'object') return null
  const rect = value as Record<string, unknown>
  const values = ['x', 'y', 'width', 'height', 'scale'].map((key) => Number(rect[key]))
  if (values.some((number) => !Number.isFinite(number))) return null
  const [x, y, width, height, scale] = values
  if (scale <= 0 || width <= 0 || height <= 0) return null
  return { x: x / scale, y: y / scale, width: width / scale, height: height / scale }
}

/** Centro (DIP) de `frame` dentro de `rect`, recortado al área de trabajo. */
export function centerRectInBounds(
  rect: ModalAnchorRect,
  frame: { width: number; height: number },
  workArea: { x: number; y: number; width: number; height: number },
): { x: number; y: number } {
  const x = rect.x + (rect.width - frame.width) * 0.5
  const y = rect.y + (rect.height - frame.height) * 0.5
  return {
    x: Math.round(Math.max(workArea.x, Math.min(x, workArea.x + workArea.width - frame.width))),
    y: Math.round(Math.max(workArea.y, Math.min(y, workArea.y + workArea.height - frame.height))),
  }
}

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
