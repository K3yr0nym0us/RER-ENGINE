import { useEffect, useRef, type RefObject } from 'react'

import { useContextEngine } from '@engine'
import { buildPlaneToolSetActiveCommand } from '../context/planeToolIpc'
import { useQuickBuild } from '../context/QuickBuildContext'
import { usePlaneTool, type PlaneToolName } from '../context/PlaneToolContext'

function is3dPlanePlacementEvent(event: { event: string; position?: unknown }): boolean {
  return (
    (event.event === 'collider_created' || event.event === 'execution_area_created') &&
    event.position != null
  )
}

/**
 * Herramientas 3D de muro invisible / trigger: toggle, ghost en el motor y colocación con click.
 * Q/E (rotación) las detecta solo el motor Rust vía winit + polling OS.
 */
export function usePlaneToolPlacement(viewportRef: RefObject<HTMLDivElement | null>) {
  const { activePlaneTool, setActivePlaneTool } = usePlaneTool()
  const { activeBluePrint, setActiveBluePrint } = useQuickBuild()
  const { engineReady, projectType, send } = useContextEngine()
  const activeRef = useRef(activePlaneTool)
  const hadPlaneToolRef = useRef(false)
  /** Tras colocar, el motor ya desactivó la herramienta; no enviar set_active_tool vacío (race IPC). */
  const skipNextDeactivateIpcRef = useRef(false)
  const sendRef = useRef(send)
  const is3D = projectType === '3D'

  useEffect(() => {
    sendRef.current = send
  }, [send])

  useEffect(() => {
    activeRef.current = activePlaneTool
  }, [activePlaneTool])

  useEffect(() => {
    if (activePlaneTool && activeBluePrint) {
      setActiveBluePrint(null)
    }
  }, [activePlaneTool, activeBluePrint, setActiveBluePrint])

  useEffect(() => {
    if (!engineReady || !is3D) return

    if (activePlaneTool) {
      hadPlaneToolRef.current = true
      sendRef.current(buildPlaneToolSetActiveCommand(activePlaneTool))
    } else if (hadPlaneToolRef.current) {
      hadPlaneToolRef.current = false
      if (!skipNextDeactivateIpcRef.current) {
        sendRef.current(buildPlaneToolSetActiveCommand(null))
      }
      skipNextDeactivateIpcRef.current = false
    }
  }, [activePlaneTool, engineReady, is3D])

  useEffect(() => {
    if (!engineReady || !is3D) return

    const onEvent = (event: { event: string; position?: unknown }) => {
      if (!is3dPlanePlacementEvent(event)) return

      const placedTool: PlaneToolName | null =
        event.event === 'collider_created' ? 'draw_collider' : 'draw_execution_area'

      if (activeRef.current === placedTool) {
        skipNextDeactivateIpcRef.current = true
        setActivePlaneTool(null)
      }
    }

    window.engine?.on(onEvent)
    return () => window.engine?.off(onEvent)
  }, [engineReady, is3D, setActivePlaneTool])

  useEffect(() => {
    if (!engineReady || !is3D) return
    const el = viewportRef.current
    if (!el) return

    const onPointerDown = (e: PointerEvent) => {
      if (!activeRef.current) return
      if (e.button !== 0) return

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
  }, [engineReady, is3D, viewportRef, send])

  useEffect(() => {
    if (!engineReady) return
    return () => setActivePlaneTool(null)
  }, [engineReady, setActivePlaneTool])
}
