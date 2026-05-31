import { useEffect, useRef, type RefObject } from 'react'
import { useQuickBuild } from '../context/QuickBuildContext'
import { useContextEngine } from '@engine'
import type { PendingRestore } from '../context/useContextEngine/types'
import type { BluePrintEntry } from '@shared-types'
import { beginModelReplaceLoading } from '../context/useContextEngine/hooks/sceneImportOverlay'
import {
  blueprintPlacementPhysics,
  blueprintUsesModel3D,
  buildBlueprintPlacementMeta,
  buildQuickBuildPendingRestore,
  isModel3DPath,
  queueQuickBuildPendingRestore,
  resolveBlueprintModelPath,
  resolveEngineModelPath,
} from '../utils/blueprintModelPath'

function resolvePreviewKind(bp: BluePrintEntry, is3D: boolean): string {
  if (is3D) return 'model'
  return bp.kind === 'scenario' ? 'scenario' : 'character'
}

/**
 * Construcción rápida: ghost = preview; 2D coloca vía quick_build_click + load_*;
 * 3D coloca en el motor con la misma vía que Entidades (spawn_cached_model_part_at).
 */
export function useQuickBuildPlacement(viewportRef: RefObject<HTMLDivElement | null>) {
  const { activeBluePrint } = useQuickBuild()
  const {
    engineReady,
    projectType,
    models,
    dispatch,
    modelReplaceInProgressRef,
    modelLoadOverlayKindRef,
    pendingRestoresRef,
    modelsRef,
    quickBuildActiveBlueprintIdRef,
    send,
    registerQuickBuildClickListener,
    unregisterQuickBuildClickListener,
  } = useContextEngine()

  const is3D = projectType === '3D'
  const is3DRef = useRef(is3D)
  const activeBluePrintRef = useRef(activeBluePrint)
  useEffect(() => {
    is3DRef.current = projectType === '3D'
  }, [projectType])
  useEffect(() => {
    activeBluePrintRef.current = activeBluePrint
    quickBuildActiveBlueprintIdRef.current = activeBluePrint?.id ?? null
  }, [activeBluePrint, quickBuildActiveBlueprintIdRef])

  useEffect(() => {
    if (!engineReady) return

    if (activeBluePrint) {
      const modelPath = resolveBlueprintModelPath(activeBluePrint)
      const enginePath = resolveEngineModelPath(modelPath, models)
      const firstFrame = activeBluePrint.animations?.[0]?.frames?.[0]
      const hasCrop =
        !is3D &&
        firstFrame?.src_x != null &&
        firstFrame?.src_y != null &&
        firstFrame?.src_w != null &&
        firstFrame?.src_h != null
      const placementMeta = buildBlueprintPlacementMeta(activeBluePrint, models)
      const { physicsEnabled, physicsType } = blueprintPlacementPhysics(activeBluePrint, models)
      const placementCategory = placementMeta.category

      if (is3D && blueprintUsesModel3D(activeBluePrint) && isModel3DPath(modelPath)) {
        const preloaded = models.some((m) => m.path === enginePath && m.loading !== true)
        if (!preloaded) {
          beginModelReplaceLoading(
            dispatch,
            modelReplaceInProgressRef,
            'entity',
            modelLoadOverlayKindRef,
          )
        }
      }

      if (is3D) {
        send({
          cmd: 'set_active_tool',
          tool: 'quick_build_place',
          preview_path: blueprintUsesModel3D(activeBluePrint) && isModel3DPath(modelPath)
            ? enginePath
            : modelPath,
          preview_kind: resolvePreviewKind(activeBluePrint, true),
          preview_scale: activeBluePrint.scale,
          preview_rotation: activeBluePrint.rotation ?? [0, 0, 0, 1],
          preview_name: activeBluePrint.name,
          preview_entity_category: placementCategory,
          preview_physics_enabled: physicsEnabled,
          preview_physics_type: physicsType,
          preview_blueprint: placementMeta,
          preview_blueprint_id: activeBluePrint.id,
          preview_src_rect: hasCrop
            ? [
                firstFrame!.src_x!,
                firstFrame!.src_y!,
                firstFrame!.src_w!,
                firstFrame!.src_h!,
              ]
            : undefined,
        })
      } else {
        const previewPath = firstFrame?.path ?? activeBluePrint.path
        send({
          cmd: 'set_active_tool',
          tool: 'quick_build_place',
          preview_path: previewPath,
          preview_kind: activeBluePrint.kind === 'scenario' ? 'scenario' : 'character',
          preview_scale: activeBluePrint.scale,
          preview_blueprint_id: activeBluePrint.id,
          preview_src_rect: hasCrop
            ? [
                firstFrame!.src_x!,
                firstFrame!.src_y!,
                firstFrame!.src_w!,
                firstFrame!.src_h!,
              ]
            : undefined,
        })
      }
    } else {
      send({ cmd: 'set_active_tool', tool: '' })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeBluePrint, engineReady, is3D])

  useEffect(() => {
    if (!engineReady) return

    registerQuickBuildClickListener((
      worldX: number,
      worldY: number,
      worldZ: number,
      fitToGrid: boolean,
      scaleFromEngine?: [number, number, number],
    ) => {
      const bp = activeBluePrintRef.current
      if (!bp) return

      const placementScale = scaleFromEngine ?? bp.scale

      // Modelos 3D: el motor coloca vía spawn_cached_model_part_at (misma vía que Entidades).
      if (is3DRef.current && blueprintUsesModel3D(bp)) {
        return
      }

      const pending: PendingRestore = {
        ...buildQuickBuildPendingRestore(bp),
        transform: {
          position: [worldX, worldY, worldZ],
          rotation: bp.rotation ?? [0, 0, 0, 1],
          scale: placementScale,
        },
      }

      const modelPath = resolveBlueprintModelPath(bp)
      const enginePath = resolveEngineModelPath(modelPath, modelsRef.current)
      queueQuickBuildPendingRestore(pendingRestoresRef.current, enginePath, pending)
      if (enginePath !== bp.path) {
        queueQuickBuildPendingRestore(pendingRestoresRef.current, bp.path, pending)
      }

      if (bp.kind === 'scenario') {
        send({ cmd: 'load_scenario', path: bp.path, track_undo: true })
      } else if (bp.kind === 'character') {
        send({ cmd: 'load_character', path: bp.path, track_undo: true })
      }
    })

    return () => {
      unregisterQuickBuildClickListener()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engineReady, is3D])

  useEffect(() => {
    if (!engineReady || !is3D) return
    const el = viewportRef.current
    if (!el) return

    const onPointerDown = (e: PointerEvent) => {
      const bp = activeBluePrintRef.current
      if (!bp) return
      if (e.button !== 0) return
      if (!blueprintUsesModel3D(bp)) return

      const modelPath = resolveBlueprintModelPath(bp)
      const enginePath = resolveEngineModelPath(modelPath, modelsRef.current)
      queueQuickBuildPendingRestore(
        pendingRestoresRef.current,
        enginePath,
        buildQuickBuildPendingRestore(bp),
      )

      const rect = el.getBoundingClientRect()
      const dpr = window.devicePixelRatio ?? 1
      send({
        cmd: 'place_quick_build_at_cursor',
        pixel_x: (e.clientX - rect.left) * dpr,
        pixel_y: (e.clientY - rect.top) * dpr,
      } as never)
    }

    el.addEventListener('pointerdown', onPointerDown, true)
    return () => el.removeEventListener('pointerdown', onPointerDown, true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engineReady, is3D, viewportRef])
}
