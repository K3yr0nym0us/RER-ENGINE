import { useState, useEffect, useRef, useCallback } from 'react'

import { useContextEngine } from '../context/useContextEngine'

const AUTO_SAVE_INTERVAL_MS = 5 * 60 * 1000 // 5 minutos

interface UseAutoSaveOptions {
  projectType?: string
  initialSave?: any | null
}

export interface UseAutoSaveReturn {
  autoSaveEnabled: boolean
  toggleAutoSave: () => void
  hasSavedOnce: boolean
  setHasSavedOnce: (v: boolean) => void
  handleSave: () => Promise<void>
}

export function useAutoSave({ projectType = '2D', initialSave = null }: UseAutoSaveOptions = {}): UseAutoSaveReturn {
  const { worldConfig, backgroundPath, entityTransformsRef, entityMetaRef, playerEntityIdRef, camera2dRef } = useContextEngine()
  const [hasSavedOnce, setHasSavedOnce] = useState(false)
  const [autoSaveEnabled, setAutoSaveEnabled] = useState(false)
  const intervalRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastSavePath = useRef<string | null>(null)

  useEffect(() => {
    if (initialSave) setHasSavedOnce(true)
  }, [initialSave])

  const buildSaveData = useCallback(() => {
    if (!entityTransformsRef.current || !entityMetaRef.current) return null

    const transforms = entityTransformsRef.current
    const meta = entityMetaRef.current
    const DEFAULT_POS: [number,number,number] = [0, 0, 0]
    const DEFAULT_ROT: [number,number,number,number] = [0, 0, 0, 1]
    const DEFAULT_SCL: [number,number,number] = [1, 1, 1]
    const playerId = playerEntityIdRef.current

    const allEntities = Object.entries(meta)
      .filter(([idStr, m]) =>
        !(m.kind === 'character' && m.path === '[Player]' && Number(idStr) === playerId)
      )
      .map(([idStr, m]) => {
        const id = Number(idStr)
        return {
          id,
          kind: m.kind,
          path: m.path,
          position: transforms[id]?.position ?? DEFAULT_POS,
          rotation: transforms[id]?.rotation ?? DEFAULT_ROT,
          scale: transforms[id]?.scale ?? DEFAULT_SCL,
          physics_enabled: m.physicsEnabled,
          physics_type: m.physicsType,
          points: m.points,
          animations: m.animations,
          scripts: m.scripts,
        }
      })

    const playerTransform = playerId !== null
      ? {
          position: transforms[playerId]?.position ?? DEFAULT_POS,
          scale: transforms[playerId]?.scale ?? DEFAULT_SCL,
        }
      : null

    return {
      version: 1,
      type: projectType,
      gameStyle: initialSave?.gameStyle ?? 'side-scroller',
      world: worldConfig,
      backgroundPath: backgroundPath ?? null,
      entities: allEntities,
      playerTransform,
      camera2d: camera2dRef.current,
      savedAt: new Date().toISOString(),
    }
  }, [projectType, initialSave, worldConfig, backgroundPath, playerEntityIdRef, entityTransformsRef, entityMetaRef, camera2dRef])

  useEffect(() => {
    if (!hasSavedOnce && autoSaveEnabled) {
      setAutoSaveEnabled(false)
    }
  }, [hasSavedOnce, autoSaveEnabled])

  useEffect(() => {
    if (autoSaveEnabled) {
      intervalRef.current = setTimeout(async () => {
        const data = buildSaveData()
        if (data) {
          await window.electronAPI.saveProjectSilent('autosave.json', data)
        }
      }, AUTO_SAVE_INTERVAL_MS)
    } else {
      if (intervalRef.current) {
        clearTimeout(intervalRef.current)
        intervalRef.current = null
      }
    }

    return () => {
      if (intervalRef.current) {
        clearTimeout(intervalRef.current)
        intervalRef.current = null
      }
    }
  }, [autoSaveEnabled, buildSaveData])

  const handleSave = useCallback(async () => {
    const data = buildSaveData()
    if (!data) return

    if (lastSavePath.current) {
      await window.electronAPI.saveProjectSilent(lastSavePath.current, data)
      setHasSavedOnce(true)
      return
    }

    const ok = await window.electronAPI.saveProject(data)
    if (ok) {
      setHasSavedOnce(true)
    }
  }, [buildSaveData])

  const setHasSavedOnceTrue = useCallback((v: boolean) => {
    setHasSavedOnce(v)
  }, [])

  const toggleAutoSave = useCallback(() => {
    if (!hasSavedOnce) return
    setAutoSaveEnabled((prev) => !prev)
  }, [hasSavedOnce])

  return { 
    autoSaveEnabled, 
    toggleAutoSave, 
    hasSavedOnce, 
    setHasSavedOnce: setHasSavedOnceTrue,
    handleSave,
  }
}