import type { RefObject } from 'react'

import { useQuickBuildPlacement } from '@hooks'
import { usePlaneToolPlacement } from '@hooks'

/**
 * Activa el modo de construcción rápida registrando el hook IPC.
 * No renderiza nada en el DOM: el motor (Rust) dibuja el indicador visual
 * directamente sobre la ventana nativa que es siempre el elemento superior.
 */
export function QuickBuildOverlay({
  viewportRef,
}: {
  viewportRef: RefObject<HTMLDivElement | null>
}) {
  useQuickBuildPlacement(viewportRef)
  usePlaneToolPlacement(viewportRef)
  return null
}
