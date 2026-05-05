import { useRef, useEffect } from 'react'

export const BLUEPRINT_CANVAS_SIZE = 500
const GRID_SIZE = 32

interface UseBluePrintCanvasProps {
  spriteSrc: string
}

export function useBluePrintCanvas({ spriteSrc }: UseBluePrintCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const size = BLUEPRINT_CANVAS_SIZE

    const drawGrid = () => {
      ctx.strokeStyle = 'rgba(255,255,255,0.35)'
      ctx.lineWidth = 1
      for (let x = 0; x <= size; x += GRID_SIZE) {
        ctx.beginPath()
        ctx.moveTo(x, 0)
        ctx.lineTo(x, size)
        ctx.stroke()
      }
      for (let y = 0; y <= size; y += GRID_SIZE) {
        ctx.beginPath()
        ctx.moveTo(0, y)
        ctx.lineTo(size, y)
        ctx.stroke()
      }
    }

    ctx.clearRect(0, 0, size, size)

    if (!spriteSrc) {
      ctx.fillStyle = '#1a1a2e'
      ctx.fillRect(0, 0, size, size)
      drawGrid()
      return
    }

    const img = new window.Image()
    img.onload = () => {
      ctx.clearRect(0, 0, size, size)
      ctx.fillStyle = '#1a1a2e'
      ctx.fillRect(0, 0, size, size)

      const scale = Math.min(size / img.width, size / img.height)
      const drawWidth = img.width * scale
      const drawHeight = img.height * scale
      const offsetX = (size - drawWidth) / 2
      const offsetY = (size - drawHeight) / 2

      ctx.drawImage(img, offsetX, offsetY, drawWidth, drawHeight)
      drawGrid()
    }
    img.onerror = () => {
      ctx.clearRect(0, 0, size, size)
      ctx.fillStyle = '#1a1a2e'
      ctx.fillRect(0, 0, size, size)
      drawGrid()
    }
    img.src = spriteSrc
  }, [spriteSrc])

  return { canvasRef }
}
