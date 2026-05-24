import type { RefObject } from 'react'

import { useQuickBuildPlacement } from '@hooks'

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
  return null
}

// eslint-disable-next-line @typescript-eslint/no-unused-vars
function _BluePrintCursor_UNUSED({ blueprint, x, y }: { blueprint: unknown; x: number; y: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  const firstFrame = blueprint.animations?.[0]?.frames?.[0]
  const framePath = firstFrame?.path ?? ''

  const { imageSrc } = useSpritePreviewImage(framePath)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !imageSrc) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const img = new window.Image()
    img.onload = () => {
      ctx.clearRect(0, 0, CURSOR_SIZE, CURSOR_SIZE)

      if (firstFrame?.src_w != null && firstFrame?.src_h != null) {
        ctx.drawImage(
          img,
          firstFrame.src_x ?? 0,
          firstFrame.src_y ?? 0,
          firstFrame.src_w,
          firstFrame.src_h,
          0,
          0,
          CURSOR_SIZE,
          CURSOR_SIZE,
        )
      } else {
        const scale = Math.min(CURSOR_SIZE / img.width, CURSOR_SIZE / img.height)
        const dw = img.width * scale
        const dh = img.height * scale
        ctx.drawImage(img, (CURSOR_SIZE - dw) / 2, (CURSOR_SIZE - dh) / 2, dw, dh)
      }
    }
    img.src = imageSrc
  }, [imageSrc, firstFrame])

  const half = CURSOR_SIZE / 2

  return (
    <canvas
      ref={canvasRef}
      width={CURSOR_SIZE}
      height={CURSOR_SIZE}
      style={{
        position: 'absolute',
        left: x - half,
        top: y - half,
        imageRendering: 'pixelated',
        opacity: 0.85,
        outline: '2px dashed rgba(255, 193, 7, 0.8)',
        outlineOffset: 2,
        borderRadius: 4,
      }}
    />
  )
}
