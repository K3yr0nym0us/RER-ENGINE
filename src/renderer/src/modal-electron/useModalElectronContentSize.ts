import { useLayoutEffect, useRef } from 'react'

/**
 * Mide el contenedor del contenido tras pintar y ajusta la altura de la ventana modal
 * vía IPC (setContentSize en main).
 */
export function useModalElectronContentSize(active: boolean) {
  const contentRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    if (!active || !contentRef.current) return

    const report = () => {
      const el = contentRef.current
      if (!el) return
      const height = Math.ceil(el.getBoundingClientRect().height)
      if (height > 0) {
        window.electronAPI.resizeModalElectron(height)
      }
    }

    report()
    const observer = new ResizeObserver(() => report())
    observer.observe(contentRef.current)
    return () => observer.disconnect()
  }, [active])

  return contentRef
}
