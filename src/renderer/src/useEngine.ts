import { useState, useEffect, useRef } from 'react'
import type { EngineEvent, EntitySelected } from '../../shared/types'

export interface Entity {
  id: number
}

export interface SelectedEntity {
  id:       number
  name:     string
  position: [number, number, number]
  rotation: [number, number, number, number]
  scale:    [number, number, number]
}

export interface LogEntry {
  text:    string
  isError: boolean
}

export function useEngine(viewportRef: React.RefObject<HTMLDivElement | null>) {
  const [engineReady,    setEngineReady]    = useState(false)
  const [engineError,    setEngineError]    = useState<string | null>(null)
  const [log,            setLog]            = useState<LogEntry[]>([])
  const [entities,       setEntities]       = useState<Entity[]>([])
  const [selectedEntity, setSelectedEntity] = useState<SelectedEntity | null>(null)
  const readyTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const addLog = (text: string, isError = false) =>
    setLog((prev) => [...prev.slice(-199), { text, isError }])

  const reportBounds = () => {
    if (!viewportRef.current) return
    const rect = viewportRef.current.getBoundingClientRect()
    const dpr  = window.devicePixelRatio ?? 1
    window.electronAPI.sendViewportBounds({
      x:      rect.x      * dpr,
      y:      rect.y      * dpr,
      width:  rect.width  * dpr,
      height: rect.height * dpr,
    })
  }

  const send = (cmd: object) => window.engine.send(cmd as never)

  const loadModel = (path: string) => {
    setEntities([])
    send({ cmd: 'load_model', path })
  }

  const retryEngine = () => {
    setEngineError(null)
    setEngineReady(false)
    setEntities([])
    addLog('[retry] Reiniciando motor…')
    reportBounds()
  }

  // Reportar bounds del viewport al proceso principal
  useEffect(() => {
    reportBounds()
    const observer = new ResizeObserver(reportBounds)
    if (viewportRef.current) observer.observe(viewportRef.current)
    window.electronAPI.onRequestViewportBounds(reportBounds)
    return () => observer.disconnect()
  }, [])

  // Timeout de 5 s esperando el evento "ready"
  useEffect(() => {
    readyTimer.current = setTimeout(() => {
      setEngineError('El motor no respondió en 5 segundos. Puede que el binario no exista o haya fallado al iniciar.')
      addLog('[timeout] Motor no respondió en 5s', true)
    }, 5000)
    return () => {
      if (readyTimer.current) clearTimeout(readyTimer.current)
    }
  }, [])

  // Suscribirse a eventos del motor
  useEffect(() => {
    window.engine.on((event: EngineEvent) => {
      addLog(JSON.stringify(event), event.event === 'error')

      if (event.event === 'ready') {
        setEngineReady(true)
        setEngineError(null)
        if (readyTimer.current) clearTimeout(readyTimer.current)
      }
      if (event.event === 'model_loaded') {
        const id = (event as { id?: number }).id ?? -1
        setEntities((prev) =>
          prev.some((e) => e.id === id) ? prev : [...prev, { id }]
        )
      }
      if (event.event === 'entity_selected') {
        const e = event as unknown as EntitySelected
        setSelectedEntity({ id: e.id, name: e.name, position: e.position, rotation: e.rotation, scale: e.scale })
      }
      if (event.event === 'entity_deselected') {
        setSelectedEntity(null)
      }
      if (event.event === 'stopped') {
        setEngineReady(false)
        const code = (event as { code?: number }).code
        if (code !== 0 && code != null) {
          setEngineError(`El motor terminó inesperadamente (código ${code}).`)
        }
      }
      if (event.event === 'error') {
        setEngineError((event as { message?: string }).message ?? 'Error desconocido')
      }
    })
  }, [])

  return { engineReady, engineError, log, entities, selectedEntity, send, loadModel, reportBounds, retryEngine }
}
