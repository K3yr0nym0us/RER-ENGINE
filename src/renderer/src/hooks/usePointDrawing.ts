import { useState, useEffect, useCallback, useRef } from 'react'

/**
 * Hook reutilizable para herramientas de dibujo por puntos en el viewport del motor.
 *
 * Maneja el estado local de activación y progreso, y sincroniza con los eventos
 * que emite el motor (`drawing_progress`, `collider_created`, `tool_cancelled`)
 * a través del `toolProgress` proporcionado por `useEngine`.
 *
 * @param toolName    Nombre de la herramienta (e.g. "draw_collider")
 * @param totalPoints Número de puntos necesarios para completar la herramienta
 * @param send        Función para enviar comandos al motor
 * @param toolProgress Progreso actual (1..totalPoints-1) o null si no hay tarea activa
 */
export function usePointDrawing(
  toolName:    string,
  totalPoints: number,
  send:        (cmd: object) => void,
  toolProgress: number | null,
) {
  const [isActive, setIsActive] = useState(false)
  const [progress, setProgress] = useState(0)
  const sawEngineProgressRef = useRef(false)

  // Sincronizar con eventos del motor
  useEffect(() => {
    if (!isActive) return
    if (toolProgress === null) {
      // Completado o cancelado desde el motor, pero solo después de que
      // efectivamente haya comenzado el flujo (al menos 1 evento de progreso).
      if (sawEngineProgressRef.current) {
        setIsActive(false)
        setProgress(0)
        sawEngineProgressRef.current = false
      }
    } else {
      sawEngineProgressRef.current = true
      setProgress(toolProgress)
    }
  }, [toolProgress, isActive])

  const start = useCallback(() => {
    setIsActive(true)
    setProgress(0)
    sawEngineProgressRef.current = false
    send({ cmd: 'set_active_tool', tool: toolName })
  }, [send, toolName])

  const cancel = useCallback(() => {
    setIsActive(false)
    setProgress(0)
    sawEngineProgressRef.current = false
    send({ cmd: 'set_active_tool', tool: '' })
  }, [send])

  return { isActive, progress, totalPoints, start, cancel }
}
