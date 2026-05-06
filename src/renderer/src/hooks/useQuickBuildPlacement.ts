import { useEffect, useRef } from 'react'
import { useQuickBuild } from '../context/QuickBuildContext'
import { useContextEngine } from '@engine'
import type { PendingRestore } from '../context/useContextEngine/types'

/**
 * Gestiona el modo de construcción rápida usando el sistema IPC del motor.
 *
 * Cuando hay una blueprint activa:
 * - Activa la herramienta `quick_build_place` en el motor (Rust renderiza la entidad fantasma).
 * - Registra un listener que recibe coordenadas mundo al hacer click.
 * - Al recibir el click, encola un PendingRestore y carga la entidad con track_undo=true.
 */
export function useQuickBuildPlacement() {
  const { activeBluePrint } = useQuickBuild()
  const {
    engineReady,
    worldConfig,
    pendingRestoresRef,
    send,
    registerQuickBuildClickListener,
    unregisterQuickBuildClickListener,
  } = useContextEngine()

  // Ref estable para la blueprint activa dentro del listener (no re-registrar en cada render)
  const activeBluePrintRef = useRef(activeBluePrint)
  useEffect(() => {
    activeBluePrintRef.current = activeBluePrint
  }, [activeBluePrint])

  const gridCellSizeRef = useRef(worldConfig.gridCellSize)
  useEffect(() => {
    gridCellSizeRef.current = worldConfig.gridCellSize
  }, [worldConfig.gridCellSize])

  // Efecto: activar/desactivar herramienta en el motor
  useEffect(() => {
    if (!engineReady) return

    if (activeBluePrint) {
      const firstFrame = activeBluePrint.animations?.[0]?.frames?.[0]
      const previewPath = firstFrame?.path ?? activeBluePrint.path
      const hasCrop =
        firstFrame?.src_x != null &&
        firstFrame?.src_y != null &&
        firstFrame?.src_w != null &&
        firstFrame?.src_h != null

      send({
        cmd: 'set_active_tool',
        tool: 'quick_build_place',
        preview_path: previewPath,
        preview_kind: activeBluePrint.kind === 'scenario' ? 'scenario' : 'character',
        preview_scale: activeBluePrint.scale,
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
      send({ cmd: 'set_active_tool', tool: '' })
      unregisterQuickBuildClickListener()
    }

    // No hay cleanup que desregistre: el listener se mantiene activo
    // mientras activeBluePrint sea no-null, y se limpia solo cuando se desactiva.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeBluePrint, engineReady])

  // Efecto separado: registrar el listener de click UNA SOLA VEZ al montar
  // (usa activeBluePrintRef para acceder al valor actual sin re-registrar)
  useEffect(() => {
    if (!engineReady) return

    registerQuickBuildClickListener((worldX: number, worldY: number, fitToGrid: boolean) => {
      const bp = activeBluePrintRef.current
      if (!bp) return

      const placementScale = fitToGrid
        ? [gridCellSizeRef.current, gridCellSizeRef.current, bp.scale?.[2] ?? 1] as [number, number, number]
        : bp.scale

      console.log('[quick_build] click mundo:', worldX, worldY, '| bp.path:', bp.path, '| bp.kind:', bp.kind, '| scale:', placementScale)

      const pending: PendingRestore = {
        transform: {
          position: [worldX, worldY, 0],
          rotation: [0, 0, 0, 1],
          scale: placementScale,
        },
        name: bp.name,
        physicsEnabled: bp.physics_enabled ?? false,
        physicsType: bp.physics_type ?? 'static',
        animations: bp.animations as any[] | undefined,
        scripts: bp.scripts,
        controlBindings: bp.control_bindings,
      }

      const queue = pendingRestoresRef.current.get(bp.path) ?? []
      queue.push(pending)
      pendingRestoresRef.current.set(bp.path, queue)

      if (bp.kind === 'scenario') {
        send({ cmd: 'load_scenario', path: bp.path, track_undo: true })
      } else if (bp.kind === 'character') {
        send({ cmd: 'load_character', path: bp.path, track_undo: true })
      }
    })

    return () => {
      unregisterQuickBuildClickListener()
    }
    // Solo se registra al montar (engineReady); activeBluePrintRef es una ref estable
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engineReady])
}
