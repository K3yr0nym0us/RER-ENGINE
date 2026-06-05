import { useLayoutEffect, useRef } from 'react'

import {
  isResizableModalComponent,
  isTallModalComponent,
  modalTallContentHeightPx,
} from './modalElectronLayout'

/**
 * Mide el contenedor del contenido tras pintar y ajusta la altura de la ventana modal
 * vía IPC (setContentSize en main).
 */
export function useModalElectronContentSize(
  active: boolean,
  componentKey?: string,
  resizable?: boolean,
) {
  const contentRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    if (!active || !contentRef.current) return
    if (resizable || isResizableModalComponent(componentKey)) return

    const minHeight = isTallModalComponent(componentKey)
      ? modalTallContentHeightPx()
      : 0

    const report = () => {
      const el = contentRef.current
      if (!el) return
      const measured = Math.ceil(el.getBoundingClientRect().height)
      const height = Math.max(minHeight, measured)
      if (height > 0) {
        window.electronAPI.resizeModalElectron(height)
      }
    }

    report()
    const observer = new ResizeObserver(() => report())
    observer.observe(contentRef.current)
    return () => observer.disconnect()
  }, [active, componentKey, resizable])

  return contentRef
}
