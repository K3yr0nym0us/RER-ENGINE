import { useState, useEffect, useRef } from 'react'

const AUTO_SAVE_INTERVAL_MS = 5 * 60 * 1000 // 5 minutos

export interface UseAutoSaveOptions {
  /** Función que ejecuta el guardado real */
  onSave: () => void
  /** Solo se puede activar si ya se guardó al menos una vez */
  hasSavedOnce: boolean
}

export interface UseAutoSaveReturn {
  autoSaveEnabled: boolean
  toggleAutoSave: () => void
}

export function useAutoSave({ onSave, hasSavedOnce }: UseAutoSaveOptions): UseAutoSaveReturn {
  const [autoSaveEnabled, setAutoSaveEnabled] = useState(false)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // Si el usuario nunca guardó y el auto-save estaba activo, desactivarlo
  useEffect(() => {
    if (!hasSavedOnce && autoSaveEnabled) {
      setAutoSaveEnabled(false)
    }
  }, [hasSavedOnce, autoSaveEnabled])

  // Manejar el intervalo de auto-guardado
  useEffect(() => {
    if (autoSaveEnabled) {
      intervalRef.current = setInterval(() => {
        onSave()
      }, AUTO_SAVE_INTERVAL_MS)
    } else {
      if (intervalRef.current) {
        clearInterval(intervalRef.current)
        intervalRef.current = null
      }
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current)
        intervalRef.current = null
      }
    }
  }, [autoSaveEnabled, onSave])

  const toggleAutoSave = () => {
    if (!hasSavedOnce) return
    setAutoSaveEnabled((prev) => !prev)
  }

  return { autoSaveEnabled, toggleAutoSave }
}
