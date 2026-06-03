import type { ModalElectronSize } from '../shared-types/types'

/** Anchos de contenido alineados a Bootstrap 5 `.modal-dialog` (max-width). */
const CONTENT_WIDTH: Record<ModalElectronSize, number> = {
  sm: 300,
  md: 500,
  lg: 800,
  xl: 1140,
}

const HORIZONTAL_PADDING = 48

/** Altura mínima del área web antes de medir el DOM (evita ventana gigante al abrir). */
export const MODAL_ELECTRON_MIN_CONTENT_HEIGHT = 80

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
