import { useState, useEffect, useRef, useCallback } from 'react';

import { useContextEngine } from '@engine';
import { useLanguage } from '@context';
import type { GameStyle, ProjectType, ProjectSaveData, SavedScene } from '@shared-types';
import { getSceneProjectState } from '../pages/EngineView/sceneStateStore';

interface UseAutoSaveOptions {
  projectType?: ProjectType
  gameStyle?: GameStyle
  initialSave?: ProjectSaveData | null
  initialSavePath?: string | null
}

export interface UseAutoSaveReturn {
  autoSaveEnabled: boolean
  toggleAutoSave: () => void
  hasSavedOnce: boolean
  setHasSavedOnce: (v: boolean) => void
  handleSave: () => Promise<void>
}

export function useAutoSave({
  projectType = '2D' as ProjectType,
  gameStyle,
  initialSave = null,
  initialSavePath = null,
}: UseAutoSaveOptions = {}): UseAutoSaveReturn {
  const { worldConfig, backgroundPath, selectedEntity, entityTransformsRef, entityMetaRef, playerEntityIdRef, camera2dRef, loadedSpritesInfo, blueprints, sounds, backgrounds } = useContextEngine()
  const { locale } = useLanguage()
  const [hasSavedOnce, setHasSavedOnce] = useState(Boolean(initialSavePath))
  const [autoSaveEnabled, setAutoSaveEnabled] = useState(false)
  const lastSavePath = useRef<string | null>(initialSavePath)
  const autoSaveEnabledRef = useRef(false)
  const buildSaveDataRef = useRef<(() => ReturnType<typeof buildSaveData>) | null>(null)
  const autoSaveListenerRegisteredRef = useRef(false)

  useEffect(() => {
    if (initialSavePath) {
      setHasSavedOnce(true)
    } else if (initialSave) {
      setHasSavedOnce(true)
    }
  }, [initialSave, initialSavePath])

  useEffect(() => {
    if (initialSavePath) {
      lastSavePath.current = initialSavePath
    }
  }, [initialSavePath])

  const buildSaveData = useCallback(() => {
    if (!entityTransformsRef.current || !entityMetaRef.current) return null

    const transforms = entityTransformsRef.current
    const meta = entityMetaRef.current
    const DEFAULT_POS: [number,number,number] = [0, 0, 0]
    const DEFAULT_ROT: [number,number,number,number] = [0, 0, 0, 1]
    const DEFAULT_SCL: [number,number,number] = [1, 1, 1]
    const playerId = playerEntityIdRef.current

    const pointsFromTransform = (id: number): [[number, number], [number, number], [number, number], [number, number]] | undefined => {
      const t = transforms[id]
      if (!t) return undefined
      const cx = t.position[0]
      const cy = t.position[1]
      const hw = Math.abs(t.scale[0]) * 0.5
      const hh = Math.abs(t.scale[1]) * 0.5
      return [
        [cx - hw, cy - hh],
        [cx + hw, cy - hh],
        [cx + hw, cy + hh],
        [cx - hw, cy + hh],
      ]
    }

    const buildCurrentSceneEntities = () => Object.entries(meta)
      .filter(([idStr, m]) =>
        !(m.kind === 'character' && m.path === '[Player]' && Number(idStr) === playerId)
      )
      .map(([idStr, m]) => {
        const id = Number(idStr)
        const selectedName = selectedEntity?.id === id ? selectedEntity.name : undefined
        const livePoints = (m.kind === 'collider' || m.kind === 'execution_area')
          ? pointsFromTransform(id)
          : undefined
        // Las instancias de blueprint no guardan sus propias propiedades:
        // las heredan del blueprint original al cargar el proyecto.
        const isBlueprintInstance = !!m.blueprintId
        return {
          id,
          name: selectedName ?? m.name,
          kind: m.kind,
          path: m.path,
          position: transforms[id]?.position ?? DEFAULT_POS,
          rotation: transforms[id]?.rotation ?? DEFAULT_ROT,
          scale: transforms[id]?.scale ?? DEFAULT_SCL,
          physics_enabled: isBlueprintInstance ? undefined : m.physicsEnabled,
          physics_type: isBlueprintInstance ? undefined : m.physicsType,
          points: livePoints ?? m.points,
          animations: isBlueprintInstance ? undefined : m.animations,
          scripts: isBlueprintInstance ? undefined : m.scripts,
          control_bindings: isBlueprintInstance ? undefined : m.controlBindings,
          blueprint_id: m.blueprintId,
        }
      })

    const allEntities = buildCurrentSceneEntities()

    const playerTransform = playerId !== null
      ? {
          position: transforms[playerId]?.position ?? DEFAULT_POS,
          scale: transforms[playerId]?.scale ?? DEFAULT_SCL,
        }
      : null

    // Convertir loadedSpritesInfo Map a array para persistencia
    const spritesArray = Array.from(loadedSpritesInfo.entries()).map(([path, info]) => ({
      name: info.name,
      path,
    }))

    const currentSceneSnapshot = {
      id: 1,
      name: 'Escena 1',
      world: worldConfig,
      backgroundPath: backgroundPath ?? null,
      entities: allEntities,
      playerTransform,
      camera2d: camera2dRef.current,
      sprites: spritesArray,
    }

    const sceneState = getSceneProjectState()
    let scenes: SavedScene[] = [currentSceneSnapshot]
    let activeSceneId = 1

    if (sceneState && sceneState.scenes.length > 0) {
      activeSceneId = sceneState.activeSceneId
      scenes = sceneState.scenes
      const activeIndex = scenes.findIndex((scene) => scene.id === activeSceneId)
      if (activeIndex >= 0) {
        scenes[activeIndex] = {
          ...scenes[activeIndex],
          world: worldConfig,
          backgroundPath: backgroundPath ?? null,
          entities: allEntities,
          playerTransform,
          camera2d: camera2dRef.current,
          sprites: spritesArray,
        }
      }
    }

    const activeScene = scenes.find((scene) => scene.id === activeSceneId) ?? scenes[0]

    return {
      version: 1,
      type: projectType,
      gameStyle: initialSave?.gameStyle ?? gameStyle ?? (projectType === '2D' ? 'top-down' : 'first-person'),
      scenes,
      activeSceneId,
      world: activeScene?.world ?? worldConfig,
      backgroundPath: activeScene?.backgroundPath ?? (backgroundPath ?? null),
      entities: activeScene?.entities ?? allEntities,
      playerTransform: activeScene?.playerTransform ?? playerTransform,
      camera2d: activeScene?.camera2d ?? camera2dRef.current,
      savedAt: new Date().toISOString(),
      sprites: activeScene?.sprites ?? spritesArray,
      sounds,
      backgrounds,
      blueprints,
      language: locale,
    }
  }, [projectType, gameStyle, initialSave, worldConfig, backgroundPath, selectedEntity, playerEntityIdRef, entityTransformsRef, entityMetaRef, camera2dRef, loadedSpritesInfo, blueprints, sounds, backgrounds, locale])

  useEffect(() => {
    autoSaveEnabledRef.current = autoSaveEnabled
  }, [autoSaveEnabled])

  useEffect(() => {
    buildSaveDataRef.current = buildSaveData
  }, [buildSaveData])

  useEffect(() => {
    if (!hasSavedOnce && autoSaveEnabled) {
      setAutoSaveEnabled(false)
      window.engine.send({ cmd: 'set_autosave', enabled: false } as never)
    }
  }, [hasSavedOnce, autoSaveEnabled])

  useEffect(() => {
    if (autoSaveListenerRegisteredRef.current) return
    autoSaveListenerRegisteredRef.current = true

    window.electronAPI.onAutoSaveRequest(async (filePath: string) => {
      if (!autoSaveEnabledRef.current) return
      const snapshotBuilder = buildSaveDataRef.current
      if (!snapshotBuilder) return
      const data = snapshotBuilder()
      if (!data) return
      await window.electronAPI.saveProjectSilent(filePath, data)
    })
  }, [])

  useEffect(() => {
    return () => {
      window.engine.send({ cmd: 'set_autosave', enabled: false } as never)
    }
  }, [])

  const handleSave = useCallback(async () => {
    const data = buildSaveData()
    if (!data) return

    const savedPath = await window.electronAPI.saveProject(data)
    if (savedPath) {
      lastSavePath.current = savedPath
      setHasSavedOnce(true)
    }
  }, [buildSaveData])

  const setHasSavedOnceTrue = useCallback((v: boolean) => {
    setHasSavedOnce(v)
  }, [])

  const toggleAutoSave = useCallback(() => {
    if (!hasSavedOnce) return
    setAutoSaveEnabled((prev) => {
      const next = !prev
      window.engine.send({ cmd: 'set_autosave', enabled: next } as never)
      return next
    })
  }, [hasSavedOnce])

  return { 
    autoSaveEnabled, 
    toggleAutoSave, 
    hasSavedOnce, 
    setHasSavedOnce: setHasSavedOnceTrue,
    handleSave,
  }
}